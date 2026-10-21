use std::collections::VecDeque;
use std::time::{Duration, Instant};

use zz_machine::{Decision, Machine};

pub(crate) const CANCEL_GRACE: Duration = Duration::from_secs(10);
const REMEMBERED_COMMANDS: usize = 32;

pub(crate) type CommandId = String;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Fact {
    SessionReady,
    SessionClosed,
    Exited {
        message: String,
    },
    Prompt {
        turn: u64,
        command: Option<CommandId>,
    },
    Accepted {
        turn: u64,
        command: CommandId,
    },
    Steer {
        command: CommandId,
    },
    Cancel,
    Started {
        command: CommandId,
    },
    Reported {
        command: Option<CommandId>,
        stop_reason: Option<String>,
        error: Option<String>,
    },
    Ended {
        command: CommandId,
        end: End,
    },
    Busy,
    Idle,
    #[cfg_attr(not(test), expect(dead_code))]
    PermissionOpened {
        request: u64,
    },
    #[cfg_attr(not(test), expect(dead_code))]
    PermissionClosed {
        request: u64,
    },
    Tick,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum End {
    Completed,
    Cancelled,
    Failed(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    Completed { stop_reason: String },
    Cancelled,
    Failed { message: String },
    Dropped,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Effect {
    Interrupt,
    Settle { turn: Option<u64>, outcome: Outcome },
    Dismiss { requests: Vec<u64> },
    Drain,
    Reclaim,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Reject {
    Stale(CommandId),
    Unknown(CommandId),
    NotRunning,
    Busy,
    Offline,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Phase {
    Offline,
    Idle,
    Running(Turn),
    Cancelling { turn: Turn, deadline: Instant },
    Exited,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Turn {
    pub(crate) owner: Option<u64>,
    commands: Vec<Command>,
    unbound: bool,
    began: bool,
    stop_reason: Option<String>,
    error: Option<String>,
    permissions: Vec<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Command {
    id: CommandId,
    started: bool,
    ended: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TurnState {
    pub(crate) phase: Phase,
    pub(crate) permissions: Vec<u64>,
    finished: VecDeque<CommandId>,
}

pub(crate) struct Ctx {
    pub(crate) now: Instant,
}

pub(crate) struct TurnMachine;

type TurnDecision = Decision<TurnState, Effect, Reject>;

impl Machine for TurnMachine {
    type State = TurnState;
    type Signal = Fact;
    type Ctx = Ctx;
    type Effect = Effect;
    type Time = Instant;
    type Reject = Reject;

    fn transition(state: &TurnState, fact: &Fact, ctx: &Ctx) -> TurnDecision {
        match fact {
            Fact::SessionReady => match state.phase {
                Phase::Offline | Phase::Exited => {
                    Decision::to(state.with_phase(Phase::Idle)).with_effect(Effect::Drain)
                }
                _ => Decision::stay(),
            },
            Fact::SessionClosed => close(state, Phase::Offline, Outcome::Dropped, None),
            Fact::Exited { message } => close(
                state,
                Phase::Exited,
                Outcome::Failed {
                    message: message.clone(),
                },
                Some(Effect::Reclaim),
            ),
            Fact::Prompt { turn, command } => match &state.phase {
                Phase::Idle => Decision::to(
                    state.with_phase(Phase::Running(Turn::prompted(*turn, command.as_ref()))),
                ),
                Phase::Running(current) if current.owner.is_none() => {
                    let mut current = current.clone();
                    current.owner = Some(*turn);
                    if let Some(command) = command {
                        current.commands.push(Command::new(command, false));
                    }
                    Decision::to(state.with_turn(current))
                }
                Phase::Running(_) | Phase::Cancelling { .. } => Decision::reject(Reject::Busy),
                Phase::Offline | Phase::Exited => Decision::reject(Reject::Offline),
            },
            Fact::Accepted { turn, command } => {
                let Some(current) = state.turn().filter(|current| current.owner == Some(*turn))
                else {
                    return Decision::reject(Reject::Unknown(command.clone()));
                };
                if current.has(command) {
                    return Decision::stay();
                }
                let mut current = current.clone();
                current.bind(command, false);
                interrupt_late(state, Decision::to(state.with_turn(current)))
            }
            Fact::Steer { command } => match &state.phase {
                Phase::Running(turn) if turn.has(command) => Decision::stay(),
                Phase::Running(turn) => {
                    let mut turn = turn.clone();
                    turn.commands.push(Command::new(command, false));
                    Decision::to(state.with_turn(turn))
                }
                Phase::Idle => Decision::reject(Reject::NotRunning),
                Phase::Cancelling { .. } => Decision::reject(Reject::Busy),
                Phase::Offline | Phase::Exited => Decision::reject(Reject::Offline),
            },
            Fact::Cancel => match &state.phase {
                Phase::Running(turn) => {
                    let mut next = state.with_phase(Phase::Cancelling {
                        turn: turn.clone(),
                        deadline: ctx.now + CANCEL_GRACE,
                    });
                    let dismissed = next.dismiss_from(turn);
                    Decision::to(next)
                        .with_effect(Effect::Interrupt)
                        .with_effects(dismissed)
                }
                Phase::Cancelling { .. } => Decision::stay(),
                Phase::Offline | Phase::Idle | Phase::Exited => {
                    Decision::reject(Reject::NotRunning)
                }
            },
            Fact::Started { command } => {
                if state.finished(command) {
                    return Decision::reject(Reject::Stale(command.clone()));
                }
                match &state.phase {
                    Phase::Idle => {
                        Decision::to(state.with_phase(Phase::Running(Turn::agent(Some(command)))))
                    }
                    Phase::Running(turn) | Phase::Cancelling { turn, .. } => {
                        let known = turn.has(command);
                        let mut turn = turn.clone();
                        turn.start(command);
                        let decision = Decision::to(state.with_turn(turn));
                        if known {
                            decision
                        } else {
                            interrupt_late(state, decision)
                        }
                    }
                    Phase::Offline | Phase::Exited => Decision::reject(Reject::Offline),
                }
            }
            Fact::Reported {
                command,
                stop_reason,
                error,
            } => {
                if let Some(command) = command
                    && state.finished(command)
                {
                    return Decision::reject(Reject::Stale(command.clone()));
                }
                let Some(turn) = state.turn() else {
                    return Decision::reject(Reject::NotRunning);
                };
                if let Some(command) = command
                    && !turn.has(command)
                {
                    return Decision::reject(Reject::Unknown(command.clone()));
                }
                let mut turn = turn.clone();
                if stop_reason.is_some() {
                    turn.stop_reason.clone_from(stop_reason);
                }
                if error.is_some() {
                    turn.error.clone_from(error);
                }
                Decision::to(state.with_turn(turn))
            }
            Fact::Ended { command, end } => {
                if state.finished(command) {
                    return Decision::reject(Reject::Stale(command.clone()));
                }
                let Some(turn) = state
                    .turn()
                    .filter(|turn| turn.has(command) || turn.unbound)
                else {
                    return Decision::reject(Reject::Unknown(command.clone()));
                };
                let mut turn = turn.clone();
                if !turn.has(command) {
                    turn.bind(command, true);
                }
                turn.end(command, end);
                if turn.unbound || turn.commands.iter().any(|command| !command.ended) {
                    return Decision::to(state.with_turn(turn));
                }
                let outcome = turn.outcome(state.cancelling(), *end == End::Completed);
                settle(state, &turn, outcome)
            }
            Fact::Busy => match &state.phase {
                Phase::Idle => Decision::to(state.with_phase(Phase::Running(Turn::agent(None)))),
                Phase::Running(turn) if !turn.began => {
                    let mut turn = turn.clone();
                    turn.began = true;
                    Decision::to(state.with_turn(turn))
                }
                _ => Decision::stay(),
            },
            Fact::Idle => match &state.phase {
                Phase::Running(turn) if turn.began && !turn.open() => {
                    settle(state, turn, turn.outcome(false, true))
                }
                Phase::Cancelling { turn, .. } => settle(state, turn, Outcome::Cancelled),
                _ => Decision::stay(),
            },
            Fact::PermissionOpened { request } => {
                if state.permissions.contains(request) {
                    return Decision::stay();
                }
                let mut next = match state.turn() {
                    Some(turn) => {
                        let mut turn = turn.clone();
                        turn.permissions.push(*request);
                        state.with_turn(turn)
                    }
                    None => state.clone(),
                };
                next.permissions.push(*request);
                Decision::to(next)
            }
            Fact::PermissionClosed { request } => {
                if !state.permissions.contains(request) {
                    return Decision::stay();
                }
                let mut next = state.clone();
                next.permissions.retain(|pending| pending != request);
                Decision::to(next)
            }
            Fact::Tick => match &state.phase {
                Phase::Cancelling { turn, deadline } if ctx.now >= *deadline => {
                    settle(state, turn, Outcome::Cancelled)
                }
                _ => Decision::stay(),
            },
        }
    }

    fn wake_at(state: &TurnState) -> Option<Instant> {
        match state.phase {
            Phase::Cancelling { deadline, .. } => Some(deadline),
            _ => None,
        }
    }
}

fn interrupt_late(state: &TurnState, decision: TurnDecision) -> TurnDecision {
    if state.cancelling() {
        decision.with_effect(Effect::Interrupt)
    } else {
        decision
    }
}

fn settle(state: &TurnState, turn: &Turn, outcome: Outcome) -> TurnDecision {
    let follow = if turn.owner.is_none() || matches!(outcome, Outcome::Completed { .. }) {
        Effect::Drain
    } else {
        Effect::Reclaim
    };
    let mut next = state.with_phase(Phase::Idle);
    next.remember(turn);
    let dismissed = next.dismiss_from(turn);
    Decision::to(next)
        .with_effect(Effect::Settle {
            turn: turn.owner,
            outcome,
        })
        .with_effects(dismissed)
        .with_effect(follow)
}

fn close(
    state: &TurnState,
    phase: Phase,
    outcome: Outcome,
    follow: Option<Effect>,
) -> TurnDecision {
    let mut next = state.with_phase(phase);
    let mut effects = Vec::new();
    if let Some(turn) = state.turn() {
        next.remember(turn);
        let outcome = if state.cancelling() {
            Outcome::Cancelled
        } else {
            outcome
        };
        effects.push(Effect::Settle {
            turn: turn.owner,
            outcome,
        });
    }
    effects.extend(next.dismiss());
    effects.extend(follow);
    Decision::to(next).with_effects(effects)
}

impl Default for TurnState {
    fn default() -> Self {
        Self {
            phase: Phase::Offline,
            permissions: Vec::new(),
            finished: VecDeque::new(),
        }
    }
}

impl TurnState {
    #[cfg_attr(not(test), expect(dead_code))]
    pub(crate) fn accepts_prompt(&self) -> bool {
        self.phase == Phase::Idle
    }

    #[cfg_attr(not(test), expect(dead_code))]
    pub(crate) fn waiting(&self) -> bool {
        self.turn().is_some() && !self.permissions.is_empty()
    }

    pub(crate) fn turn(&self) -> Option<&Turn> {
        match &self.phase {
            Phase::Running(turn) | Phase::Cancelling { turn, .. } => Some(turn),
            Phase::Offline | Phase::Idle | Phase::Exited => None,
        }
    }

    fn cancelling(&self) -> bool {
        matches!(self.phase, Phase::Cancelling { .. })
    }

    fn finished(&self, command: &str) -> bool {
        self.finished.iter().any(|finished| finished == command)
    }

    fn with_phase(&self, phase: Phase) -> Self {
        Self {
            phase,
            permissions: self.permissions.clone(),
            finished: self.finished.clone(),
        }
    }

    fn with_turn(&self, turn: Turn) -> Self {
        match &self.phase {
            Phase::Running(_) => self.with_phase(Phase::Running(turn)),
            Phase::Cancelling { deadline, .. } => self.with_phase(Phase::Cancelling {
                turn,
                deadline: *deadline,
            }),
            Phase::Offline | Phase::Idle | Phase::Exited => self.clone(),
        }
    }

    fn remember(&mut self, turn: &Turn) {
        for command in &turn.commands {
            if !self.finished(&command.id) {
                self.finished.push_back(command.id.clone());
            }
        }
        while self.finished.len() > REMEMBERED_COMMANDS {
            self.finished.pop_front();
        }
    }

    fn dismiss(&mut self) -> Option<Effect> {
        let requests = std::mem::take(&mut self.permissions);
        (!requests.is_empty()).then_some(Effect::Dismiss { requests })
    }

    fn dismiss_from(&mut self, turn: &Turn) -> Option<Effect> {
        let requests = self
            .permissions
            .iter()
            .copied()
            .filter(|request| turn.permissions.contains(request))
            .collect::<Vec<_>>();
        self.permissions
            .retain(|request| !turn.permissions.contains(request));
        (!requests.is_empty()).then_some(Effect::Dismiss { requests })
    }
}

impl Turn {
    pub(crate) fn active_command(&self) -> Option<&str> {
        self.commands
            .iter()
            .rev()
            .find(|command| !command.ended)
            .map(|command| command.id.as_str())
    }

    fn prompted(owner: u64, command: Option<&CommandId>) -> Self {
        Self {
            owner: Some(owner),
            unbound: command.is_none(),
            commands: command
                .iter()
                .map(|command| Command::new(command, false))
                .collect(),
            began: false,
            stop_reason: None,
            error: None,
            permissions: Vec::new(),
        }
    }

    fn agent(command: Option<&CommandId>) -> Self {
        Self {
            owner: None,
            unbound: false,
            commands: command
                .into_iter()
                .map(|command| Command::new(command, true))
                .collect(),
            began: true,
            stop_reason: None,
            error: None,
            permissions: Vec::new(),
        }
    }

    fn open(&self) -> bool {
        self.commands
            .iter()
            .any(|command| command.started && !command.ended)
    }

    fn has(&self, command: &str) -> bool {
        self.commands.iter().any(|known| known.id == command)
    }

    fn bind(&mut self, command: &str, started: bool) {
        self.commands.push(Command::new(command, started));
        self.unbound = false;
        self.began |= started;
    }

    fn start(&mut self, command: &str) {
        match self.commands.iter_mut().find(|known| known.id == command) {
            Some(known) => known.started = true,
            None => self.commands.push(Command::new(command, true)),
        }
        self.unbound = false;
        self.began = true;
    }

    fn end(&mut self, command: &str, end: &End) {
        if let Some(known) = self.commands.iter_mut().find(|known| known.id == command) {
            known.ended = true;
        }
        if let End::Failed(message) = end {
            self.error.get_or_insert_with(|| message.clone());
        }
    }

    fn outcome(&self, cancelling: bool, completed: bool) -> Outcome {
        if cancelling {
            return Outcome::Cancelled;
        }
        if let Some(message) = &self.error {
            return Outcome::Failed {
                message: message.clone(),
            };
        }
        if completed {
            Outcome::Completed {
                stop_reason: self
                    .stop_reason
                    .clone()
                    .unwrap_or_else(|| "end_turn".to_owned()),
            }
        } else {
            Outcome::Cancelled
        }
    }
}

impl Command {
    fn new(id: &str, started: bool) -> Self {
        Self {
            id: id.to_owned(),
            started,
            ended: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use zz_machine::Step;

    use super::*;

    struct Run {
        state: TurnState,
        start: Instant,
    }

    impl Run {
        fn ready() -> Self {
            let mut run = Self {
                state: TurnState::default(),
                start: Instant::now(),
            };
            run.at(0, &Fact::SessionReady);
            run
        }

        fn at(&mut self, seconds: u64, fact: &Fact) -> Step<Effect, Instant, Reject> {
            let ctx = Ctx {
                now: self.start + Duration::from_secs(seconds),
            };
            TurnMachine::step(&mut self.state, fact, &ctx)
        }

        fn feed(&mut self, fact: &Fact) -> Step<Effect, Instant, Reject> {
            self.at(0, fact)
        }

        fn running(&self) -> bool {
            self.state.turn().is_some()
        }
    }

    fn prompt(turn: u64, command: &str) -> Fact {
        Fact::Prompt {
            turn,
            command: Some(command.to_owned()),
        }
    }

    fn started(command: &str) -> Fact {
        Fact::Started {
            command: command.to_owned(),
        }
    }

    fn ended(command: &str, end: End) -> Fact {
        Fact::Ended {
            command: command.to_owned(),
            end,
        }
    }

    fn steer(command: &str) -> Fact {
        Fact::Steer {
            command: command.to_owned(),
        }
    }

    fn error(command: &str, message: &str) -> Fact {
        Fact::Reported {
            command: Some(command.to_owned()),
            stop_reason: None,
            error: Some(message.to_owned()),
        }
    }

    fn settled(turn: Option<u64>, outcome: Outcome, follow: Effect) -> Vec<Effect> {
        vec![Effect::Settle { turn, outcome }, follow]
    }

    fn completed() -> Outcome {
        Outcome::Completed {
            stop_reason: "end_turn".to_owned(),
        }
    }

    #[test]
    fn a_prompt_settles_when_its_command_completes() {
        let mut run = Run::ready();
        assert!(run.state.accepts_prompt());
        run.feed(&prompt(1, "u"));
        run.feed(&started("u"));
        run.feed(&Fact::Reported {
            command: Some("u".to_owned()),
            stop_reason: Some("max_tokens".to_owned()),
            error: None,
        });
        let step = run.feed(&ended("u", End::Completed));
        assert_eq!(
            step.effects,
            settled(
                Some(1),
                Outcome::Completed {
                    stop_reason: "max_tokens".to_owned()
                },
                Effect::Drain
            )
        );
        assert!(run.state.accepts_prompt());
        assert_eq!(run.feed(&Fact::Idle).effects, []);
    }

    #[test]
    fn a_steer_hands_the_turn_to_the_new_command() {
        let mut run = Run::ready();
        run.feed(&prompt(1, "u"));
        run.feed(&started("u"));
        run.feed(&steer("s"));
        let superseded = run.feed(&ended("u", End::Cancelled));
        assert_eq!(superseded.effects, []);
        assert!(run.running());
        run.feed(&started("s"));
        let step = run.feed(&ended("s", End::Completed));
        assert_eq!(step.effects, settled(Some(1), completed(), Effect::Drain));
    }

    #[test]
    fn a_hard_failure_reported_as_cancelled_fails_the_turn() {
        let mut run = Run::ready();
        run.feed(&prompt(1, "u"));
        run.feed(&started("u"));
        run.feed(&error("u", "prompt is too long"));
        let step = run.feed(&ended("u", End::Cancelled));
        assert_eq!(
            step.effects,
            settled(
                Some(1),
                Outcome::Failed {
                    message: "prompt is too long".to_owned()
                },
                Effect::Reclaim
            )
        );
    }

    #[test]
    fn a_failed_end_keeps_the_error_the_agent_reported() {
        let mut run = Run::ready();
        run.feed(&prompt(1, "u"));
        run.feed(&started("u"));
        run.feed(&error("u", "overloaded"));
        let step = run.feed(&ended("u", End::Failed("command failed".to_owned())));
        assert_eq!(
            step.effects,
            settled(
                Some(1),
                Outcome::Failed {
                    message: "overloaded".to_owned()
                },
                Effect::Reclaim
            )
        );
    }

    #[test]
    fn a_late_error_from_a_cancelled_turn_cannot_fail_the_next_one() {
        let mut run = Run::ready();
        run.feed(&prompt(1, "a"));
        run.feed(&started("a"));
        let cancel = run.at(1, &Fact::Cancel);
        assert_eq!(cancel.effects, [Effect::Interrupt]);
        assert_eq!(
            cancel.wake_at,
            Some(run.start + Duration::from_secs(1) + CANCEL_GRACE)
        );
        assert_eq!(run.at(5, &Fact::Tick).effects, []);
        let forced = run.at(11, &Fact::Tick);
        assert_eq!(
            forced.effects,
            settled(Some(1), Outcome::Cancelled, Effect::Reclaim)
        );
        assert_eq!(forced.wake_at, None);
        run.feed(&prompt(2, "b"));
        run.feed(&started("b"));
        let late = run.feed(&error("a", "old turn error"));
        assert_eq!(late.rejected, Some(Reject::Stale("a".to_owned())));
        assert_eq!(
            run.feed(&ended("a", End::Cancelled)).rejected,
            Some(Reject::Stale("a".to_owned()))
        );
        let step = run.feed(&ended("b", End::Completed));
        assert_eq!(step.effects, settled(Some(2), completed(), Effect::Drain));
    }

    #[test]
    fn a_cancel_before_the_command_is_known_interrupts_it_once_it_is() {
        let mut run = Run::ready();
        run.feed(&Fact::Prompt {
            turn: 1,
            command: None,
        });
        assert_eq!(run.feed(&Fact::Cancel).effects, [Effect::Interrupt]);
        assert_eq!(run.state.turn().and_then(Turn::active_command), None);
        let started = run.feed(&started("t1"));
        assert_eq!(started.effects, [Effect::Interrupt]);
        assert_eq!(run.state.turn().and_then(Turn::active_command), Some("t1"));
        let step = run.feed(&ended("t1", End::Cancelled));
        assert_eq!(
            step.effects,
            settled(Some(1), Outcome::Cancelled, Effect::Reclaim)
        );
    }

    #[test]
    fn a_user_cancel_wins_over_the_error_the_interrupt_produces() {
        let mut run = Run::ready();
        run.feed(&prompt(1, "u"));
        run.feed(&started("u"));
        run.feed(&Fact::Cancel);
        run.feed(&error("u", "interrupted"));
        let step = run.feed(&ended("u", End::Cancelled));
        assert_eq!(
            step.effects,
            settled(Some(1), Outcome::Cancelled, Effect::Reclaim)
        );
        assert_eq!(step.wake_at, None);
    }

    #[test]
    fn an_idle_from_before_the_prompt_started_does_not_end_it() {
        let mut run = Run::ready();
        run.feed(&prompt(1, "u"));
        assert_eq!(run.feed(&Fact::Idle).effects, []);
        assert!(run.running());
        run.feed(&Fact::Busy);
        let step = run.feed(&Fact::Idle);
        assert_eq!(step.effects, settled(Some(1), completed(), Effect::Drain));
    }

    #[test]
    fn an_idle_before_a_started_command_ends_waits_for_its_end() {
        let mut run = Run::ready();
        run.feed(&Fact::Prompt {
            turn: 1,
            command: None,
        });
        run.feed(&Fact::Busy);
        run.feed(&started("t1"));
        assert_eq!(run.feed(&Fact::Idle).effects, []);
        let step = run.feed(&ended("t1", End::Failed("usage limit".to_owned())));
        assert_eq!(
            step.effects,
            settled(
                Some(1),
                Outcome::Failed {
                    message: "usage limit".to_owned()
                },
                Effect::Reclaim
            )
        );
    }

    #[test]
    fn a_turn_the_agent_starts_runs_until_it_goes_idle() {
        let mut run = Run::ready();
        run.feed(&Fact::Busy);
        assert!(run.running());
        assert!(!run.state.accepts_prompt());
        let step = run.feed(&Fact::Idle);
        assert_eq!(step.effects, settled(None, completed(), Effect::Drain));
    }

    #[test]
    fn a_command_nobody_here_sent_opens_an_agent_turn() {
        let mut run = Run::ready();
        run.feed(&started("wake"));
        let step = run.feed(&ended("wake", End::Completed));
        assert_eq!(step.effects, settled(None, completed(), Effect::Drain));
    }

    #[test]
    fn an_unbound_prompt_adopts_the_first_command_that_starts() {
        let mut run = Run::ready();
        run.feed(&Fact::Prompt {
            turn: 1,
            command: None,
        });
        run.feed(&started("t1"));
        assert!(
            !run.feed(&Fact::Accepted {
                turn: 1,
                command: "t1".to_owned()
            })
            .moved
        );
        let step = run.feed(&ended("t1", End::Completed));
        assert_eq!(step.effects, settled(Some(1), completed(), Effect::Drain));
    }

    #[test]
    fn an_accepted_prompt_waits_for_its_own_command() {
        let mut run = Run::ready();
        run.feed(&Fact::Prompt {
            turn: 1,
            command: None,
        });
        run.feed(&Fact::Accepted {
            turn: 1,
            command: "t1".to_owned(),
        });
        assert_eq!(
            run.feed(&ended("t0", End::Completed)).rejected,
            Some(Reject::Unknown("t0".to_owned()))
        );
        let step = run.feed(&ended("t1", End::Failed("rate limited".to_owned())));
        assert_eq!(
            step.effects,
            settled(
                Some(1),
                Outcome::Failed {
                    message: "rate limited".to_owned()
                },
                Effect::Reclaim
            )
        );
    }

    #[test]
    fn permissions_make_the_turn_wait_and_go_with_it() {
        let mut run = Run::ready();
        run.feed(&prompt(1, "u"));
        run.feed(&started("u"));
        run.feed(&Fact::PermissionOpened { request: 7 });
        run.feed(&Fact::PermissionOpened { request: 8 });
        assert!(run.state.waiting());
        run.feed(&Fact::PermissionClosed { request: 7 });
        assert!(run.state.waiting());
        let step = run.feed(&ended("u", End::Completed));
        assert_eq!(
            step.effects,
            [
                Effect::Settle {
                    turn: Some(1),
                    outcome: completed()
                },
                Effect::Dismiss { requests: vec![8] },
                Effect::Drain
            ]
        );
        assert!(!run.state.waiting());
    }

    #[test]
    fn a_permission_asked_while_idle_outlives_the_next_turn() {
        let mut run = Run::ready();
        run.feed(&Fact::PermissionOpened { request: 9 });
        run.feed(&prompt(1, "u"));
        run.feed(&started("u"));
        run.feed(&Fact::PermissionOpened { request: 10 });
        let step = run.feed(&ended("u", End::Completed));
        assert_eq!(
            step.effects,
            [
                Effect::Settle {
                    turn: Some(1),
                    outcome: completed()
                },
                Effect::Dismiss { requests: vec![10] },
                Effect::Drain
            ]
        );
        assert_eq!(run.state.permissions, [9]);
    }

    #[test]
    fn a_prompt_during_an_agent_turn_takes_it_over() {
        let mut run = Run::ready();
        run.feed(&Fact::Busy);
        let step = run.feed(&prompt(1, "u"));
        assert!(step.moved);
        assert_eq!(step.rejected, None);
        run.feed(&started("u"));
        let step = run.feed(&ended("u", End::Completed));
        assert_eq!(step.effects, settled(Some(1), completed(), Effect::Drain));
    }

    #[test]
    fn a_cancel_dismisses_open_permissions() {
        let mut run = Run::ready();
        run.feed(&prompt(1, "u"));
        run.feed(&Fact::PermissionOpened { request: 3 });
        let step = run.feed(&Fact::Cancel);
        assert_eq!(
            step.effects,
            [Effect::Interrupt, Effect::Dismiss { requests: vec![3] }]
        );
    }

    #[test]
    fn an_exit_fails_the_turn_and_hands_the_queue_back() {
        let mut run = Run::ready();
        run.feed(&prompt(1, "u"));
        run.feed(&started("u"));
        run.feed(&Fact::PermissionOpened { request: 4 });
        let step = run.feed(&Fact::Exited {
            message: "agent exited".to_owned(),
        });
        assert_eq!(
            step.effects,
            [
                Effect::Settle {
                    turn: Some(1),
                    outcome: Outcome::Failed {
                        message: "agent exited".to_owned()
                    }
                },
                Effect::Dismiss { requests: vec![4] },
                Effect::Reclaim
            ]
        );
        assert_eq!(run.state.phase, Phase::Exited);
        assert_eq!(run.feed(&prompt(2, "v")).rejected, Some(Reject::Offline));
        assert_eq!(run.feed(&Fact::SessionReady).effects, [Effect::Drain]);
        assert!(run.state.accepts_prompt());
    }

    #[test]
    fn a_closed_session_drops_the_turn_without_reclaiming_the_queue() {
        let mut run = Run::ready();
        run.feed(&prompt(1, "u"));
        let step = run.feed(&Fact::SessionClosed);
        assert_eq!(
            step.effects,
            [Effect::Settle {
                turn: Some(1),
                outcome: Outcome::Dropped
            }]
        );
        assert_eq!(run.state.phase, Phase::Offline);
    }

    #[test]
    fn steering_or_cancelling_with_nothing_running_is_rejected() {
        let mut run = Run::ready();
        assert_eq!(run.feed(&steer("s")).rejected, Some(Reject::NotRunning));
        assert_eq!(run.feed(&Fact::Cancel).rejected, Some(Reject::NotRunning));
        assert_eq!(
            run.feed(&Fact::Reported {
                command: None,
                stop_reason: None,
                error: Some("x".to_owned())
            })
            .rejected,
            Some(Reject::NotRunning)
        );
    }

    #[test]
    fn finished_commands_are_forgotten_after_the_window() {
        let mut run = Run::ready();
        for turn in 0..=REMEMBERED_COMMANDS as u64 {
            let command = format!("c{turn}");
            run.feed(&prompt(turn + 1, &command));
            run.feed(&ended(&command, End::Completed));
        }
        assert_eq!(run.state.finished.len(), REMEMBERED_COMMANDS);
        assert!(!run.state.finished("c0"));
        assert!(run.state.finished("c1"));
    }
}
