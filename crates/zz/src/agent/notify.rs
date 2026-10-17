use std::{
    collections::BTreeMap,
    fmt::Debug,
    time::{Duration, Instant},
};

use zz_client::navigation::{pane_label, session_label};
use zz_protocol::{MuxSnapshot, PaneId, PaneStatusKind};

use crate::agent::sound::AgentPaneStatus;

const MIN_BANNER_GAP: Duration = Duration::from_secs(2);

#[derive(Debug)]
pub(crate) struct AttentionBanners<K> {
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
    pub(crate) fn admit(&mut self, pane: K, now: Instant) -> bool {
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

    pub(crate) fn retire(
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

    pub(crate) fn pane(&self, tag: &str) -> Option<K> {
        self.posted
            .keys()
            .copied()
            .find(|pane| tag_for(*pane) == tag)
    }

    pub(crate) fn listen(&mut self) -> bool {
        !std::mem::replace(&mut self.listening, true)
    }
}

pub(crate) fn tag_for<K: Debug>(pane: K) -> String {
    format!("zz.attention.{pane:?}")
}

pub(crate) fn banner(
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

#[cfg(test)]
mod tests {
    use zz_mux::MuxState;
    use zz_protocol::{PaneStatus, PaneStatusState};

    use super::*;

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

    fn statuses(panes: &[u8]) -> BTreeMap<u8, AgentPaneStatus> {
        panes
            .iter()
            .map(|&pane| (pane, AgentPaneStatus::NeedsInput))
            .collect()
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

        assert_eq!(banners.retire(&statuses(&[1, 2]), Some(2)), [2, 3]);
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
