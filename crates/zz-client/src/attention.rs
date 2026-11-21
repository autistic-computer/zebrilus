use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Debug,
};

use web_time::{Duration, Instant};
use zz_protocol::{MuxSnapshot, PaneId, PaneKindSnapshot, PaneStatusKind, PaneStatusState};

use crate::{
    AgentAttentionEdge, AgentAttentionStatus, ClientCore, agent_attention_status,
    navigation::{pane_label, session_label},
};

const MIN_BANNER_GAP: Duration = Duration::from_secs(2);
const MIN_CHIME_GAP: Duration = Duration::from_secs(2);
const MIN_BELL_GAP: Duration = Duration::from_millis(200);

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum AgentPaneStatus {
    #[default]
    Idle,
    Working,
    NeedsInput,
    Failed,
    Done,
}

impl AgentPaneStatus {
    #[must_use]
    pub const fn from_pane_status(state: Option<PaneStatusState>) -> Self {
        match state {
            Some(PaneStatusState::Working) => Self::Working,
            Some(PaneStatusState::Blocked) => Self::NeedsInput,
            Some(PaneStatusState::Error) => Self::Failed,
            Some(PaneStatusState::Done) => Self::Done,
            Some(PaneStatusState::Idle) | None => Self::Idle,
        }
    }
}

impl From<AgentAttentionStatus> for AgentPaneStatus {
    fn from(status: AgentAttentionStatus) -> Self {
        match status {
            AgentAttentionStatus::Idle => Self::Idle,
            AgentAttentionStatus::Working => Self::Working,
            AgentAttentionStatus::NeedsInput => Self::NeedsInput,
            AgentAttentionStatus::Failed => Self::Failed,
        }
    }
}

impl From<AgentAttentionEdge> for AgentPaneStatus {
    fn from(edge: AgentAttentionEdge) -> Self {
        match edge {
            AgentAttentionEdge::Request => Self::NeedsInput,
            AgentAttentionEdge::Failed => Self::Failed,
            AgentAttentionEdge::Done => Self::Done,
        }
    }
}

#[must_use]
pub fn pane_statuses(core: &ClientCore) -> BTreeMap<PaneId, AgentPaneStatus> {
    core.snapshot()
        .sessions
        .iter()
        .flat_map(|session| &session.windows)
        .flat_map(|window| window.panes.values())
        .filter_map(|pane| {
            let status = match pane.kind {
                PaneKindSnapshot::Terminal => AgentPaneStatus::from_pane_status(
                    pane.status.as_ref().map(|status| status.state),
                ),
                PaneKindSnapshot::Agent(_) => {
                    agent_attention_status(core.agent_state(pane.id)?).into()
                }
                _ => return None,
            };
            Some((pane.id, status))
        })
        .collect()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AgentBadge {
    NeedsInput,
    Failed,
    Working,
    Finished,
}

impl AgentBadge {
    const fn rank(self) -> u8 {
        match self {
            Self::NeedsInput => 0,
            Self::Failed => 1,
            Self::Working => 2,
            Self::Finished => 3,
        }
    }

    #[must_use]
    pub fn merge(self, other: Self) -> Self {
        if other.rank() < self.rank() {
            other
        } else {
            self
        }
    }

    pub fn merge_into(slot: &mut Option<Self>, badge: Self) {
        *slot = Some(slot.map_or(badge, |current| current.merge(badge)));
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Chime {
    Request,
    Failed,
    Done,
    Disconnected,
    Sent,
    Bell,
}

impl Chime {
    pub const ALL: [Self; 6] = [
        Self::Request,
        Self::Failed,
        Self::Done,
        Self::Disconnected,
        Self::Sent,
        Self::Bell,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Request => "request",
            Self::Failed => "failed",
            Self::Done => "done",
            Self::Disconnected => "disconnected",
            Self::Sent => "sent",
            Self::Bell => "bell",
        }
    }

    const fn rank(self) -> u8 {
        match self {
            Self::Request => 0,
            Self::Failed => 1,
            Self::Done | Self::Disconnected | Self::Sent | Self::Bell => 2,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AttentionEdge<K> {
    pub pane: K,
    pub status: AgentPaneStatus,
    pub chime: Chime,
}

#[must_use]
pub fn loudest<K>(edges: &[AttentionEdge<K>]) -> Option<Chime> {
    edges
        .iter()
        .map(|edge| edge.chime)
        .min_by_key(|chime| chime.rank())
}

const fn chime_for(previous: AgentPaneStatus, next: AgentPaneStatus) -> Option<Chime> {
    match (previous, next) {
        (AgentPaneStatus::NeedsInput, AgentPaneStatus::NeedsInput)
        | (AgentPaneStatus::Failed, AgentPaneStatus::Failed)
        | (AgentPaneStatus::Done, AgentPaneStatus::Done) => None,
        (_, AgentPaneStatus::NeedsInput) => Some(Chime::Request),
        (_, AgentPaneStatus::Failed) => Some(Chime::Failed),
        (AgentPaneStatus::Working, AgentPaneStatus::Idle) | (_, AgentPaneStatus::Done) => {
            Some(Chime::Done)
        }
        _ => None,
    }
}

#[derive(Debug)]
pub struct AgentAttentionTracker<K = PaneId> {
    previous: BTreeMap<K, AgentPaneStatus>,
    unseen: BTreeSet<K>,
}

impl<K> Default for AgentAttentionTracker<K> {
    fn default() -> Self {
        Self {
            previous: BTreeMap::new(),
            unseen: BTreeSet::new(),
        }
    }
}

impl<K: Copy + Ord> AgentAttentionTracker<K> {
    pub fn observe(
        &mut self,
        statuses: &BTreeMap<K, AgentPaneStatus>,
        watched: Option<K>,
    ) -> Vec<AttentionEdge<K>> {
        self.observe_reported(statuses, watched, |_| false, None)
    }

    pub fn observe_reported(
        &mut self,
        statuses: &BTreeMap<K, AgentPaneStatus>,
        watched: Option<K>,
        reported: impl Fn(K) -> bool,
        edge: Option<(K, AgentPaneStatus)>,
    ) -> Vec<AttentionEdge<K>> {
        self.previous.retain(|pane, _| statuses.contains_key(pane));
        self.unseen.retain(|pane| statuses.contains_key(pane));

        let diffed = statuses.iter().filter_map(|(&pane, &status)| {
            let previous = self.previous.insert(pane, status)?;
            (!reported(pane))
                .then(|| chime_for(previous, status))
                .flatten()
                .map(|chime| (pane, status, chime))
        });
        let mut changes = diffed.collect::<Vec<_>>();
        changes.extend(
            edge.filter(|(pane, _)| statuses.contains_key(pane))
                .and_then(|(pane, status)| {
                    Some((pane, status, chime_for(AgentPaneStatus::Idle, status)?))
                }),
        );

        let mut edges = Vec::new();
        for (pane, status, chime) in changes {
            if watched == Some(pane) {
                continue;
            }
            if chime == Chime::Done {
                self.unseen.insert(pane);
            }
            edges.push(AttentionEdge {
                pane,
                status,
                chime,
            });
        }

        if let Some(pane) = watched {
            self.unseen.remove(&pane);
        }
        edges
    }

    #[must_use]
    pub fn badge(&self, pane: K, status: AgentPaneStatus) -> Option<AgentBadge> {
        match status {
            AgentPaneStatus::NeedsInput => Some(AgentBadge::NeedsInput),
            AgentPaneStatus::Failed => Some(AgentBadge::Failed),
            AgentPaneStatus::Working => Some(AgentBadge::Working),
            AgentPaneStatus::Idle | AgentPaneStatus::Done => {
                self.unseen.contains(&pane).then_some(AgentBadge::Finished)
            }
        }
    }

    #[must_use]
    pub fn waiting(&self, statuses: &BTreeMap<K, AgentPaneStatus>) -> usize {
        statuses
            .iter()
            .filter(|&(&pane, &status)| {
                self.badge(pane, status)
                    .is_some_and(|badge| badge != AgentBadge::Working)
            })
            .count()
    }
}

#[derive(Debug)]
pub struct AttentionBanners<K> {
    posted: BTreeMap<K, Instant>,
    listening: bool,
}

impl<K> Default for AttentionBanners<K> {
    fn default() -> Self {
        Self {
            posted: BTreeMap::new(),
            listening: false,
        }
    }
}

impl<K: Copy + Ord + Debug> AttentionBanners<K> {
    pub fn admit(&mut self, pane: K, now: Instant) -> bool {
        if self
            .posted
            .get(&pane)
            .is_some_and(|last| now.saturating_duration_since(*last) < MIN_BANNER_GAP)
        {
            return false;
        }
        self.posted.insert(pane, now);
        true
    }

    pub fn retire(
        &mut self,
        statuses: &BTreeMap<K, AgentPaneStatus>,
        watched: Option<K>,
    ) -> Vec<K> {
        let retired = self
            .posted
            .keys()
            .copied()
            .filter(|pane| watched == Some(*pane) || !statuses.contains_key(pane))
            .collect::<Vec<_>>();
        for pane in &retired {
            self.posted.remove(pane);
        }
        retired
    }

    #[must_use]
    pub fn pane(&self, tag: &str) -> Option<K> {
        self.posted
            .keys()
            .copied()
            .find(|pane| tag_for(*pane) == tag)
    }

    pub fn listen(&mut self) -> bool {
        !std::mem::replace(&mut self.listening, true)
    }
}

#[must_use]
pub fn tag_for<K: Debug>(pane: K) -> String {
    format!("zz.attention.{pane:?}")
}

#[must_use]
pub fn banner(
    snapshot: &MuxSnapshot,
    pane: PaneId,
    status: AgentPaneStatus,
) -> Option<(String, String)> {
    let (session, window, pane) = snapshot.sessions.iter().find_map(|session| {
        session.windows.iter().find_map(|window| {
            window
                .panes
                .get(&pane)
                .map(|found| (session, window, found))
        })
    })?;
    let record = pane.status.as_ref();
    let who = record
        .map(|record| record.app.trim())
        .filter(|app| !app.is_empty())
        .map_or_else(|| pane_label(pane), str::to_owned);
    let what = match status {
        AgentPaneStatus::NeedsInput => match record.and_then(|record| record.kind) {
            Some(PaneStatusKind::Permission) => "wants permission",
            Some(PaneStatusKind::Question) => "has a question",
            Some(PaneStatusKind::Auth) => "needs you to sign in",
            None => "needs you",
        },
        AgentPaneStatus::Failed => "failed",
        AgentPaneStatus::Idle | AgentPaneStatus::Working | AgentPaneStatus::Done => "finished",
    };
    let place = match window.name.trim() {
        "" => format!("{} · window {}", session_label(session), window.index),
        name => format!("{} · {name}", session_label(session)),
    };
    let body = record
        .into_iter()
        .flat_map(|record| [record.message.trim(), record.title.trim()])
        .find(|text| !text.is_empty())
        .map_or_else(|| place.clone(), |text| format!("{text}\n{place}"));
    Some((format!("{who} {what}"), body))
}

pub fn chime_due(last: &mut Option<Instant>, now: Instant) -> bool {
    due(last, now, MIN_CHIME_GAP)
}

pub fn bell_due(last: &mut Option<Instant>, now: Instant) -> bool {
    due(last, now, MIN_BELL_GAP)
}

fn due(last: &mut Option<Instant>, now: Instant, gap: Duration) -> bool {
    if last.is_some_and(|last| now.saturating_duration_since(last) < gap) {
        return false;
    }
    *last = Some(now);
    true
}

#[must_use]
pub const fn chime_wav(chime: Chime) -> &'static [u8] {
    match chime {
        Chime::Request => include_bytes!("../assets/sounds/request.wav"),
        Chime::Failed => include_bytes!("../assets/sounds/failed.wav"),
        Chime::Done => include_bytes!("../assets/sounds/done.wav"),
        Chime::Disconnected => include_bytes!("../assets/sounds/disconnected.wav"),
        Chime::Sent => include_bytes!("../assets/sounds/sent.wav"),
        Chime::Bell => include_bytes!("../assets/sounds/bell.wav"),
    }
}

#[cfg(test)]
mod tests {
    use zz_mux::MuxState;
    use zz_protocol::PaneStatus;

    use super::*;

    fn u32_at(data: &[u8], offset: usize) -> u32 {
        u32::from_le_bytes(data[offset..offset + 4].try_into().expect("four bytes"))
    }

    fn u16_at(data: &[u8], offset: usize) -> u16 {
        u16::from_le_bytes(data[offset..offset + 2].try_into().expect("two bytes"))
    }

    fn statuses(entries: &[(u64, AgentPaneStatus)]) -> BTreeMap<PaneId, AgentPaneStatus> {
        entries
            .iter()
            .map(|&(pane, status)| (PaneId(pane), status))
            .collect()
    }

    fn blocked(panes: &[u8]) -> BTreeMap<u8, AgentPaneStatus> {
        panes
            .iter()
            .map(|&pane| (pane, AgentPaneStatus::NeedsInput))
            .collect()
    }

    fn status(app: &str, kind: Option<PaneStatusKind>, message: &str) -> PaneStatus {
        PaneStatus {
            state: PaneStatusState::Blocked,
            kind,
            progress: None,
            app: app.to_owned(),
            title: String::new(),
            message: message.to_owned(),
        }
    }

    #[test]
    fn chimes_closer_than_the_gap_are_dropped() {
        let mut last = None;
        let start = Instant::now();
        assert!(chime_due(&mut last, start));
        assert!(!chime_due(&mut last, start + MIN_CHIME_GAP / 2));
        assert!(chime_due(&mut last, start + MIN_CHIME_GAP));
        assert!(!chime_due(&mut last, start));

        let mut last = None;
        assert!(bell_due(&mut last, start));
        assert!(!bell_due(&mut last, start + MIN_BELL_GAP / 2));
        assert!(bell_due(&mut last, start + MIN_BELL_GAP));
    }

    #[test]
    fn bundled_chimes_are_stereo_48khz_pcm_wavs() {
        for chime in Chime::ALL {
            let data = chime_wav(chime);
            assert_eq!(&data[..4], b"RIFF");
            assert_eq!(u32_at(data, 4) as usize, data.len() - 8);
            assert_eq!(&data[8..12], b"WAVE");
            assert_eq!(&data[12..16], b"fmt ");
            assert_eq!(u32_at(data, 16), 16);
            assert_eq!(u16_at(data, 20), 1);
            assert_eq!(u16_at(data, 22), 2);
            assert_eq!(u32_at(data, 24), 48_000);
            assert_eq!(u32_at(data, 28), 192_000);
            assert_eq!(u16_at(data, 32), 4);
            assert_eq!(u16_at(data, 34), 16);
            assert_eq!(&data[36..40], b"data");
            assert_eq!(u32_at(data, 40) as usize, data.len() - 44);
            assert_eq!((data.len() - 44) % 4, 0);
        }
    }

    #[test]
    fn every_chime_is_distinct_and_audible() {
        for (index, chime) in Chime::ALL.into_iter().enumerate() {
            assert_eq!(chime as usize, index);
            let data = chime_wav(chime);
            assert!(data[44..].iter().any(|&byte| byte != 0), "{chime:?}");
            for other in &Chime::ALL[index + 1..] {
                assert_ne!(data, chime_wav(*other), "{chime:?} and {other:?}");
            }
        }
    }

    #[test]
    fn chime_edges_follow_the_question_and_completion_rules() {
        use AgentPaneStatus::{Done, Failed, Idle, NeedsInput, Working};
        assert_eq!(chime_for(Working, NeedsInput), Some(Chime::Request));
        assert_eq!(chime_for(Idle, NeedsInput), Some(Chime::Request));
        assert_eq!(chime_for(Failed, NeedsInput), Some(Chime::Request));
        assert_eq!(chime_for(Working, Failed), Some(Chime::Failed));
        assert_eq!(chime_for(Idle, Failed), Some(Chime::Failed));
        assert_eq!(chime_for(NeedsInput, Failed), Some(Chime::Failed));
        assert_eq!(chime_for(Working, Idle), Some(Chime::Done));
        assert_eq!(chime_for(Working, Done), Some(Chime::Done));
        assert_eq!(chime_for(Idle, Done), Some(Chime::Done));
        assert_eq!(chime_for(NeedsInput, Done), Some(Chime::Done));
        assert_eq!(chime_for(NeedsInput, Idle), None);
        assert_eq!(chime_for(Failed, Idle), None);
        assert_eq!(chime_for(Done, Idle), None);
        assert_eq!(chime_for(NeedsInput, NeedsInput), None);
        assert_eq!(chime_for(Failed, Failed), None);
        assert_eq!(chime_for(Done, Done), None);
        assert_eq!(chime_for(Idle, Working), None);
    }

    #[test]
    fn a_done_report_chimes_and_badges_without_a_working_edge() {
        let mut tracker = AgentAttentionTracker::default();
        tracker.observe(&statuses(&[(1, AgentPaneStatus::Idle)]), None);

        let chime = loudest(&tracker.observe(&statuses(&[(1, AgentPaneStatus::Done)]), None));
        assert_eq!(chime, Some(Chime::Done));
        assert_eq!(
            tracker.badge(PaneId(1), AgentPaneStatus::Done),
            Some(AgentBadge::Finished)
        );

        tracker.observe(&statuses(&[(1, AgentPaneStatus::Done)]), Some(PaneId(1)));
        assert_eq!(tracker.badge(PaneId(1), AgentPaneStatus::Done), None);
    }

    #[test]
    fn a_panes_first_observation_only_seeds_the_baseline() {
        let mut tracker = AgentAttentionTracker::default();
        let chime = loudest(&tracker.observe(&statuses(&[(1, AgentPaneStatus::NeedsInput)]), None));
        assert_eq!(chime, None);
        assert_eq!(
            tracker.badge(PaneId(1), AgentPaneStatus::NeedsInput),
            Some(AgentBadge::NeedsInput)
        );
    }

    #[test]
    fn an_unwatched_completion_chimes_and_stays_unseen_until_focus() {
        let mut tracker = AgentAttentionTracker::default();
        tracker.observe(&statuses(&[(1, AgentPaneStatus::Working)]), None);

        let chime = loudest(&tracker.observe(&statuses(&[(1, AgentPaneStatus::Idle)]), None));
        assert_eq!(chime, Some(Chime::Done));
        assert_eq!(
            tracker.badge(PaneId(1), AgentPaneStatus::Idle),
            Some(AgentBadge::Finished)
        );

        let chime =
            loudest(&tracker.observe(&statuses(&[(1, AgentPaneStatus::Idle)]), Some(PaneId(1))));
        assert_eq!(chime, None);
        assert_eq!(tracker.badge(PaneId(1), AgentPaneStatus::Idle), None);
    }

    #[test]
    fn reported_panes_take_their_edges_from_the_report_not_the_diff() {
        use AgentPaneStatus::{Done, Idle, NeedsInput, Working};
        let mut tracker = AgentAttentionTracker::default();
        let agent = |pane: PaneId| pane == PaneId(1);
        tracker.observe_reported(&statuses(&[(1, Idle), (2, Working)]), None, agent, None);

        let edges = tracker.observe_reported(
            &statuses(&[(1, Idle), (2, Idle)]),
            None,
            agent,
            Some((PaneId(1), Done)),
        );
        assert_eq!(
            edges,
            [
                AttentionEdge {
                    pane: PaneId(2),
                    status: Idle,
                    chime: Chime::Done,
                },
                AttentionEdge {
                    pane: PaneId(1),
                    status: Done,
                    chime: Chime::Done,
                },
            ]
        );
        assert_eq!(tracker.waiting(&statuses(&[(1, Idle), (2, Idle)])), 2);

        let edges =
            tracker.observe_reported(&statuses(&[(1, NeedsInput), (2, Idle)]), None, agent, None);
        assert_eq!(edges, []);
        let edges = tracker.observe_reported(
            &statuses(&[(1, Idle), (2, Idle)]),
            None,
            agent,
            Some((PaneId(1), NeedsInput)),
        );
        assert_eq!(loudest(&edges), Some(Chime::Request));
    }

    #[test]
    fn a_watched_pane_neither_chimes_nor_collects_a_badge() {
        let mut tracker = AgentAttentionTracker::default();
        tracker.observe(&statuses(&[(1, AgentPaneStatus::Working)]), Some(PaneId(1)));
        let chime =
            loudest(&tracker.observe(&statuses(&[(1, AgentPaneStatus::Idle)]), Some(PaneId(1))));
        assert_eq!(chime, None);
        assert_eq!(tracker.badge(PaneId(1), AgentPaneStatus::Idle), None);

        let mut tracker = AgentAttentionTracker::default();
        tracker.observe(&statuses(&[(1, AgentPaneStatus::Working)]), Some(PaneId(1)));
        let chime = loudest(&tracker.observe(
            &statuses(&[(1, AgentPaneStatus::NeedsInput)]),
            Some(PaneId(1)),
        ));
        assert_eq!(chime, None);
    }

    #[test]
    fn a_question_outranks_a_failure_and_a_failure_a_completion() {
        let mut tracker = AgentAttentionTracker::default();
        tracker.observe(
            &statuses(&[(1, AgentPaneStatus::Working), (2, AgentPaneStatus::Working)]),
            None,
        );
        let chime = loudest(&tracker.observe(
            &statuses(&[(1, AgentPaneStatus::Idle), (2, AgentPaneStatus::NeedsInput)]),
            None,
        ));
        assert_eq!(chime, Some(Chime::Request));

        let chime = loudest(&tracker.observe(
            &statuses(&[(1, AgentPaneStatus::Failed), (2, AgentPaneStatus::Done)]),
            None,
        ));
        assert_eq!(chime, Some(Chime::Failed));

        let chime = loudest(&tracker.observe(
            &statuses(&[
                (1, AgentPaneStatus::NeedsInput),
                (2, AgentPaneStatus::Failed),
            ]),
            None,
        ));
        assert_eq!(chime, Some(Chime::Request));
    }

    #[test]
    fn each_unwatched_edge_names_its_pane_and_status() {
        use AgentPaneStatus::{Done, Failed, Idle, Working};
        let mut tracker = AgentAttentionTracker::default();
        tracker.observe(&statuses(&[(1, Working), (2, Working), (3, Working)]), None);

        let edges = tracker.observe(
            &statuses(&[(1, Idle), (2, Failed), (3, Done)]),
            Some(PaneId(3)),
        );
        assert_eq!(
            edges,
            [
                AttentionEdge {
                    pane: PaneId(1),
                    status: Idle,
                    chime: Chime::Done,
                },
                AttentionEdge {
                    pane: PaneId(2),
                    status: Failed,
                    chime: Chime::Failed,
                },
            ]
        );
        assert_eq!(loudest(&edges), Some(Chime::Failed));
        assert_eq!(loudest::<PaneId>(&[]), None);
    }

    #[test]
    fn the_waiting_count_skips_working_and_seen_panes() {
        use AgentPaneStatus::{Failed, Idle, NeedsInput, Working};
        let mut tracker = AgentAttentionTracker::default();
        let start = statuses(&[
            (1, Working),
            (2, Working),
            (3, Working),
            (4, Idle),
            (5, Working),
        ]);
        tracker.observe(&start, None);
        let now = statuses(&[
            (1, Idle),
            (2, NeedsInput),
            (3, Failed),
            (4, Idle),
            (5, Working),
        ]);
        tracker.observe(&now, None);
        assert_eq!(tracker.waiting(&now), 3);

        tracker.observe(&now, Some(PaneId(1)));
        assert_eq!(tracker.waiting(&now), 2);
    }

    #[test]
    fn a_closed_pane_drops_its_baseline_and_its_badge() {
        let mut tracker = AgentAttentionTracker::default();
        tracker.observe(&statuses(&[(1, AgentPaneStatus::Working)]), None);
        tracker.observe(&statuses(&[(1, AgentPaneStatus::Idle)]), None);
        assert_eq!(
            tracker.badge(PaneId(1), AgentPaneStatus::Idle),
            Some(AgentBadge::Finished)
        );

        tracker.observe(&statuses(&[]), None);
        assert_eq!(tracker.badge(PaneId(1), AgentPaneStatus::Idle), None);
        let chime = loudest(&tracker.observe(&statuses(&[(1, AgentPaneStatus::Idle)]), None));
        assert_eq!(chime, None);
    }

    #[test]
    fn identical_pane_ids_on_different_hosts_do_not_share_transitions() {
        let mut tracker = AgentAttentionTracker::<(u8, PaneId)>::default();
        tracker.observe(
            &BTreeMap::from([((1, PaneId(7)), AgentPaneStatus::Working)]),
            None,
        );

        let chime = loudest(&tracker.observe(
            &BTreeMap::from([((2, PaneId(7)), AgentPaneStatus::Idle)]),
            None,
        ));

        assert_eq!(chime, None);
        assert_eq!(tracker.badge((2, PaneId(7)), AgentPaneStatus::Idle), None);
    }

    #[test]
    fn rollup_badges_keep_the_most_urgent_pane() {
        let mut slot = None;
        AgentBadge::merge_into(&mut slot, AgentBadge::Finished);
        AgentBadge::merge_into(&mut slot, AgentBadge::Working);
        assert_eq!(slot, Some(AgentBadge::Working));
        AgentBadge::merge_into(&mut slot, AgentBadge::NeedsInput);
        AgentBadge::merge_into(&mut slot, AgentBadge::Failed);
        assert_eq!(slot, Some(AgentBadge::NeedsInput));
    }

    #[test]
    fn a_pane_posts_again_only_after_the_gap() {
        let mut banners = AttentionBanners::default();
        let start = Instant::now();
        assert!(banners.admit(1_u8, start));
        assert!(!banners.admit(1, start + Duration::from_secs(1)));
        assert!(banners.admit(2, start + Duration::from_secs(1)));
        assert!(banners.admit(1, start + MIN_BANNER_GAP));
    }

    #[test]
    fn watched_and_closed_panes_retire_their_banner() {
        let mut banners = AttentionBanners::default();
        let now = Instant::now();
        for pane in [1_u8, 2, 3] {
            banners.admit(pane, now);
        }
        assert_eq!(banners.pane(&tag_for(2_u8)), Some(2));

        assert_eq!(banners.retire(&blocked(&[1, 2]), Some(2)), [2, 3]);
        assert_eq!(banners.pane(&tag_for(2_u8)), None);
        assert_eq!(banners.pane(&tag_for(1_u8)), Some(1));
        assert!(banners.admit(2, now));
    }

    #[test]
    fn the_response_handler_registers_once() {
        let mut banners = AttentionBanners::<u8>::default();
        assert!(banners.listen());
        assert!(!banners.listen());
    }

    #[test]
    fn a_banner_names_the_program_what_it_wants_and_where() {
        let mut state = MuxState::default();
        let (_, window, pane) = state.create_session("work").expect("session");
        state
            .rename_window(window, "deploy")
            .expect("rename window");
        state.set_pane_status(
            pane,
            Some(status(
                "terraform",
                Some(PaneStatusKind::Permission),
                "Apply 3 to add?",
            )),
        );
        let snapshot = state.snapshot();
        assert_eq!(
            banner(&snapshot, pane, AgentPaneStatus::NeedsInput),
            Some((
                "terraform wants permission".to_owned(),
                "Apply 3 to add?\nwork · deploy".to_owned()
            ))
        );

        state.set_pane_status(pane, Some(status("", None, "")));
        let snapshot = state.snapshot();
        let (title, body) = banner(&snapshot, pane, AgentPaneStatus::Failed).expect("banner");
        assert_eq!(
            title,
            format!(
                "{} failed",
                pane_label(&snapshot.sessions[0].windows[0].panes[&pane])
            )
        );
        assert_eq!(body, "work · deploy");

        assert_eq!(banner(&snapshot, PaneId(9999), AgentPaneStatus::Done), None);
    }
}
