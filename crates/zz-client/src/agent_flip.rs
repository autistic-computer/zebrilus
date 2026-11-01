use std::collections::BTreeMap;

use zz_protocol::{
    AgentProvider, CommandInvocation, MuxSnapshot, PaneId, PaneKindSnapshot, PaneSnapshot,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlipTo {
    Chat,
    Cli,
}

impl FlipTo {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Chat => "Switch to chat",
            Self::Cli => "Switch to the CLI",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FlipTarget {
    pub provider: AgentProvider,
    pub to: FlipTo,
}

#[must_use]
pub fn flip_target(pane: &PaneSnapshot) -> Option<FlipTarget> {
    let to = match pane.kind {
        PaneKindSnapshot::Terminal => FlipTo::Chat,
        PaneKindSnapshot::Agent(_) => FlipTo::Cli,
        _ => return None,
    };
    let provider = crate::window_summary::pane_agent(pane)?;
    Some(FlipTarget { provider, to })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlipMode {
    Ask,
    Now,
    WhenIdle,
    Cancel,
}

impl FlipMode {
    const fn flag(self) -> Option<&'static str> {
        match self {
            Self::Ask => None,
            Self::Now => Some("--now"),
            Self::WhenIdle => Some("--when-idle"),
            Self::Cancel => Some("--cancel"),
        }
    }
}

#[must_use]
pub fn flip_command(pane: PaneId, mode: FlipMode) -> CommandInvocation {
    let mut args = vec!["-t".to_owned(), pane.to_string()];
    args.extend(mode.flag().map(str::to_owned));
    CommandInvocation::new("flip-agent", args)
}

#[must_use]
pub fn busy_reason(error: &str) -> Option<&str> {
    let start = error.find("busy:")?;
    let before = &error[..start];
    if !before.is_empty() && !before.ends_with(": ") {
        return None;
    }
    let reason = error[start + "busy:".len()..].trim();
    Some(if reason.is_empty() {
        "something is running"
    } else {
        reason
    })
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Flip {
    #[default]
    Idle,
    Sending(FlipMode),
    Busy(String),
    Armed,
}

impl Flip {
    #[must_use]
    pub const fn pending(&self) -> bool {
        matches!(self, Self::Sending(_))
    }

    #[must_use]
    pub const fn armed(&self) -> bool {
        matches!(self, Self::Armed | Self::Sending(FlipMode::Cancel))
    }

    #[must_use]
    pub fn busy(&self) -> Option<&str> {
        match self {
            Self::Busy(reason) => Some(reason),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
struct PaneFlip {
    to: FlipTo,
    flip: Flip,
}

#[derive(Clone, Debug, Default)]
pub struct AgentFlips {
    panes: BTreeMap<PaneId, PaneFlip>,
    armed: BTreeMap<PaneId, FlipTo>,
    requests: BTreeMap<u64, (PaneId, FlipMode)>,
}

static IDLE: Flip = Flip::Idle;
static ARMED: Flip = Flip::Armed;

impl AgentFlips {
    #[must_use]
    pub fn state(&self, pane: PaneId) -> &Flip {
        match self.panes.get(&pane) {
            Some(entry) => &entry.flip,
            None if self.armed.contains_key(&pane) => &ARMED,
            None => &IDLE,
        }
    }

    fn side(&self, pane: PaneId) -> Option<FlipTo> {
        self.panes
            .get(&pane)
            .map(|entry| entry.to)
            .or_else(|| self.armed.get(&pane).copied())
    }

    pub fn press(
        &mut self,
        pane: PaneId,
        to: FlipTo,
        send: impl FnOnce(CommandInvocation) -> Option<u64>,
    ) -> bool {
        match self.state(pane) {
            Flip::Idle => self.send(pane, to, FlipMode::Ask, send),
            Flip::Busy(_) => {
                self.dismiss(pane);
                true
            }
            Flip::Armed => self.send(pane, to, FlipMode::Cancel, send),
            Flip::Sending(_) => false,
        }
    }

    pub fn choose(
        &mut self,
        pane: PaneId,
        mode: FlipMode,
        send: impl FnOnce(CommandInvocation) -> Option<u64>,
    ) -> bool {
        let Some(to) = self.side(pane) else {
            return false;
        };
        let allowed = matches!(
            (self.state(pane), mode),
            (Flip::Busy(_), FlipMode::Now | FlipMode::WhenIdle) | (Flip::Armed, FlipMode::Cancel)
        );
        allowed && self.send(pane, to, mode, send)
    }

    pub fn dismiss(&mut self, pane: PaneId) {
        if self
            .panes
            .get(&pane)
            .is_some_and(|entry| matches!(entry.flip, Flip::Busy(_)))
        {
            self.panes.remove(&pane);
        }
    }

    fn send(
        &mut self,
        pane: PaneId,
        to: FlipTo,
        mode: FlipMode,
        send: impl FnOnce(CommandInvocation) -> Option<u64>,
    ) -> bool {
        let Some(request) = send(flip_command(pane, mode)) else {
            return false;
        };
        self.requests.insert(request, (pane, mode));
        self.panes.insert(
            pane,
            PaneFlip {
                to,
                flip: Flip::Sending(mode),
            },
        );
        true
    }

    #[must_use]
    pub fn owns(&self, request: u64) -> bool {
        self.requests.contains_key(&request)
    }

    pub fn settle(&mut self, request: u64, result: Result<(), &str>) -> Option<String> {
        let (pane, mode) = self.requests.remove(&request)?;
        let entry = self.panes.get_mut(&pane)?;
        if entry.flip != Flip::Sending(mode) {
            return result.err().map(str::to_owned);
        }
        let (next, error) = match (mode, result) {
            (FlipMode::WhenIdle, Ok(())) => (Flip::Armed, None),
            (FlipMode::Ask, Err(error)) if busy_reason(error).is_some() => (
                Flip::Busy(busy_reason(error).unwrap_or_default().to_owned()),
                None,
            ),
            (FlipMode::Cancel, Err(error)) => (Flip::Armed, Some(error.to_owned())),
            (_, Ok(())) => (Flip::Idle, None),
            (_, Err(error)) => (Flip::Idle, Some(error.to_owned())),
        };
        if next == Flip::Idle {
            self.panes.remove(&pane);
        } else {
            entry.flip = next;
        }
        error
    }

    pub fn observe(&mut self, snapshot: &MuxSnapshot) {
        self.observe_panes(
            snapshot
                .sessions
                .iter()
                .flat_map(|session| &session.windows)
                .flat_map(|window| &window.panes)
                .filter_map(|(id, pane)| {
                    Some((*id, flip_target(pane)?.to, pane.summary.flip_armed))
                }),
        );
    }

    pub fn observe_panes(&mut self, panes: impl IntoIterator<Item = (PaneId, FlipTo, bool)>) {
        let mut sides = BTreeMap::new();
        self.armed.clear();
        for (pane, to, armed) in panes {
            sides.insert(pane, to);
            if armed {
                self.armed.insert(pane, to);
            }
        }
        self.panes
            .retain(|pane, entry| sides.get(pane) == Some(&entry.to) && entry.flip != Flip::Armed);
        let panes = &self.panes;
        self.requests
            .retain(|_, (pane, _)| panes.contains_key(pane));
    }
}

pub const BUSY_TITLE: &str = "The agent is busy";
pub const WHEN_IDLE_LABEL: &str = "Switch when idle";
pub const NOW_LABEL: &str = "Switch now";
pub const CANCEL_LABEL: &str = "Cancel";

#[must_use]
pub fn busy_detail(reason: &str) -> String {
    format!("Switching now stops {reason}.")
}

#[must_use]
pub const fn armed_label(to: FlipTo) -> &'static str {
    match to {
        FlipTo::Chat => "Switches to chat when idle",
        FlipTo::Cli => "Switches to the CLI when idle",
    }
}

#[must_use]
pub const fn flip_tooltip(to: FlipTo, armed: bool) -> &'static str {
    if armed {
        "Cancel the switch"
    } else {
        to.label()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use zz_protocol::{AgentDescriptor, PaneSummary};

    const PANE: PaneId = PaneId(7);

    fn sent() -> (RefCell<Vec<CommandInvocation>>, RefCell<u64>) {
        (RefCell::new(Vec::new()), RefCell::new(0))
    }

    fn sender(
        log: &(RefCell<Vec<CommandInvocation>>, RefCell<u64>),
    ) -> impl FnOnce(CommandInvocation) -> Option<u64> + '_ {
        move |command| {
            log.0.borrow_mut().push(command);
            let mut id = log.1.borrow_mut();
            *id += 1;
            Some(*id)
        }
    }

    fn args(command: &CommandInvocation) -> Vec<String> {
        command
            .args
            .iter()
            .map(|arg| arg.as_str().to_owned())
            .collect()
    }

    #[test]
    fn a_busy_reply_offers_the_choice_and_when_idle_arms_the_flip() {
        let log = sent();
        let mut flips = AgentFlips::default();
        assert!(flips.press(PANE, FlipTo::Chat, sender(&log)));
        assert_eq!(flips.state(PANE), &Flip::Sending(FlipMode::Ask));
        assert_eq!(args(&log.0.borrow()[0]), ["-t", "%7"]);
        assert_eq!(log.0.borrow()[0].name, "flip-agent");
        assert!(!flips.press(PANE, FlipTo::Chat, sender(&log)));
        assert_eq!(
            flips.settle(
                1,
                Err("invalid command: busy: a turn and 2 background tasks")
            ),
            None
        );
        assert_eq!(
            flips.state(PANE),
            &Flip::Busy("a turn and 2 background tasks".to_owned())
        );
        assert!(flips.choose(PANE, FlipMode::WhenIdle, sender(&log)));
        assert_eq!(args(&log.0.borrow()[1]), ["-t", "%7", "--when-idle"]);
        assert_eq!(flips.settle(2, Ok(())), None);
        assert_eq!(flips.state(PANE), &Flip::Armed);
        assert!(flips.state(PANE).armed());
    }

    #[test]
    fn an_armed_flip_cancels_from_the_button_or_the_chip() {
        let log = sent();
        let mut flips = AgentFlips::default();
        flips.press(PANE, FlipTo::Cli, sender(&log));
        flips.settle(1, Err("busy: a turn"));
        flips.choose(PANE, FlipMode::WhenIdle, sender(&log));
        flips.settle(2, Ok(()));
        assert!(!flips.choose(PANE, FlipMode::Now, sender(&log)));
        assert!(flips.choose(PANE, FlipMode::Cancel, sender(&log)));
        assert_eq!(args(&log.0.borrow()[2]), ["-t", "%7", "--cancel"]);
        assert!(flips.state(PANE).armed());
        assert_eq!(
            flips.settle(3, Err("no flip is armed")),
            Some("no flip is armed".to_owned())
        );
        assert_eq!(flips.state(PANE), &Flip::Armed);
        assert!(flips.press(PANE, FlipTo::Cli, sender(&log)));
        assert_eq!(args(&log.0.borrow()[3]), ["-t", "%7", "--cancel"]);
        assert_eq!(flips.settle(4, Ok(())), None);
        assert_eq!(flips.state(PANE), &Flip::Idle);
    }

    #[test]
    fn switching_now_or_dismissing_ends_the_choice() {
        let log = sent();
        let mut flips = AgentFlips::default();
        flips.press(PANE, FlipTo::Chat, sender(&log));
        flips.settle(1, Err("busy: a turn"));
        flips.dismiss(PANE);
        assert_eq!(flips.state(PANE), &Flip::Idle);
        flips.press(PANE, FlipTo::Chat, sender(&log));
        flips.settle(2, Err("busy: a turn"));
        assert!(flips.press(PANE, FlipTo::Chat, sender(&log)));
        assert_eq!(flips.state(PANE), &Flip::Idle);
        flips.press(PANE, FlipTo::Chat, sender(&log));
        flips.settle(3, Err("busy: a turn"));
        assert!(flips.choose(PANE, FlipMode::Now, sender(&log)));
        assert_eq!(args(&log.0.borrow()[3]), ["-t", "%7", "--now"]);
        assert_eq!(flips.settle(4, Ok(())), None);
        assert_eq!(flips.state(PANE), &Flip::Idle);
    }

    #[test]
    fn other_errors_pass_through_and_unsent_commands_change_nothing() {
        let log = sent();
        let mut flips = AgentFlips::default();
        flips.press(PANE, FlipTo::Chat, sender(&log));
        assert!(flips.owns(1));
        assert_eq!(
            flips.settle(
                1,
                Err("invalid command: flip-agent: pane %7 cannot flip yet")
            ),
            Some("invalid command: flip-agent: pane %7 cannot flip yet".to_owned())
        );
        assert_eq!(flips.state(PANE), &Flip::Idle);
        assert!(!flips.owns(1));
        assert_eq!(flips.settle(9, Ok(())), None);
        assert!(!flips.press(PANE, FlipTo::Chat, |_| None));
        assert_eq!(flips.state(PANE), &Flip::Idle);
    }

    fn pane(kind: PaneKindSnapshot, program: &str) -> PaneSnapshot {
        PaneSnapshot {
            id: PANE,
            title: String::new(),
            kind,
            synchronized_input: false,
            bell: false,
            dead: false,
            dead_status: None,
            border_colour: None,
            active_border_colour: None,
            border_status_text: String::new(),
            mode: None,
            status: None,
            summary: PaneSummary {
                process: vec![program.to_owned()],
                ..PaneSummary::default()
            },
        }
    }

    fn agent() -> PaneKindSnapshot {
        PaneKindSnapshot::Agent(AgentDescriptor {
            provider: AgentProvider::ClaudeCode,
            ..AgentDescriptor::default()
        })
    }

    #[test]
    fn a_pane_that_flipped_or_closed_forgets_its_flip() {
        let log = sent();
        let mut flips = AgentFlips::default();
        flips.press(PANE, FlipTo::Chat, sender(&log));
        flips.settle(1, Err("busy: a turn"));
        flips.choose(PANE, FlipMode::WhenIdle, sender(&log));
        flips.observe_panes([(PANE, FlipTo::Chat, true)]);
        assert!(flips.owns(2));
        flips.settle(2, Ok(()));
        flips.observe_panes([(PANE, FlipTo::Chat, true)]);
        assert_eq!(flips.state(PANE), &Flip::Armed);
        flips.observe_panes([(PANE, FlipTo::Cli, false)]);
        assert_eq!(flips.state(PANE), &Flip::Idle);
        flips.press(PANE, FlipTo::Cli, sender(&log));
        flips.observe_panes([]);
        assert_eq!(flips.state(PANE), &Flip::Idle);
        assert!(!flips.owns(3));
    }

    #[test]
    fn a_reply_that_lands_after_the_pane_flipped_does_not_arm() {
        let log = sent();
        let mut flips = AgentFlips::default();
        flips.press(PANE, FlipTo::Chat, sender(&log));
        flips.settle(1, Err("busy: a turn"));
        flips.choose(PANE, FlipMode::WhenIdle, sender(&log));
        flips.observe_panes([(PANE, FlipTo::Cli, false)]);
        assert_eq!(flips.settle(2, Ok(())), None);
        assert_eq!(flips.state(PANE), &Flip::Idle);
    }

    #[test]
    fn the_daemons_armed_flag_arms_every_client_and_clears_it() {
        let log = sent();
        let mut flips = AgentFlips::default();
        flips.observe_panes([(PANE, FlipTo::Cli, true)]);
        assert_eq!(flips.state(PANE), &Flip::Armed);
        assert!(flips.choose(PANE, FlipMode::Cancel, sender(&log)));
        assert_eq!(args(&log.0.borrow()[0]), ["-t", "%7", "--cancel"]);
        assert_eq!(flips.settle(1, Ok(())), None);
        flips.observe_panes([(PANE, FlipTo::Cli, false)]);
        assert_eq!(flips.state(PANE), &Flip::Idle);

        flips.observe_panes([(PANE, FlipTo::Cli, true)]);
        assert!(flips.press(PANE, FlipTo::Cli, sender(&log)));
        assert_eq!(args(&log.0.borrow()[1]), ["-t", "%7", "--cancel"]);
        flips.settle(2, Ok(()));
        flips.observe_panes([(PANE, FlipTo::Cli, false)]);
        assert_eq!(flips.state(PANE), &Flip::Idle);

        flips.press(PANE, FlipTo::Cli, sender(&log));
        flips.settle(3, Err("busy: a turn"));
        flips.choose(PANE, FlipMode::WhenIdle, sender(&log));
        flips.settle(4, Ok(()));
        assert_eq!(flips.state(PANE), &Flip::Armed);
        flips.observe_panes([(PANE, FlipTo::Cli, false)]);
        assert_eq!(
            flips.state(PANE),
            &Flip::Idle,
            "the flip ran or was cancelled elsewhere"
        );
    }

    #[test]
    fn flip_targets_follow_the_pane_kind_and_its_program() {
        assert_eq!(
            flip_target(&pane(PaneKindSnapshot::Terminal, "/usr/local/bin/claude")),
            Some(FlipTarget {
                provider: AgentProvider::ClaudeCode,
                to: FlipTo::Chat
            })
        );
        assert_eq!(
            flip_target(&pane(agent(), "")).map(|target| target.to),
            Some(FlipTo::Cli)
        );
        assert_eq!(flip_target(&pane(PaneKindSnapshot::Terminal, "zsh")), None);
        assert_eq!(flip_target(&pane(PaneKindSnapshot::Picker, "claude")), None);
    }

    #[test]
    fn busy_reasons_come_from_the_daemons_text() {
        assert_eq!(busy_reason("busy: a turn"), Some("a turn"));
        assert_eq!(
            busy_reason("invalid command: busy: 2 background tasks"),
            Some("2 background tasks")
        );
        assert_eq!(busy_reason("busy:"), Some("something is running"));
        assert_eq!(busy_reason("not busy: really"), None);
        assert_eq!(busy_reason("pane %3 cannot flip yet"), None);
    }
}
