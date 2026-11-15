use std::time::{Duration, Instant};

use zz_machine::{Decision, Machine};
use zz_protocol::{
    AgentAccount, AgentLoginBrowser, AgentLoginFailure, AgentLoginPane, AgentLoginStep,
};

pub(crate) const START_TIMEOUT: Duration = Duration::from_mins(1);
pub(crate) const SIGN_IN_TIMEOUT: Duration = Duration::from_mins(15);
const SIGNED_OUT_AFTER_LOGIN: &str =
    "The CLI finished the sign-in, but the account still reads as signed out.";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Prompt {
    Browser { url: String },
    DeviceCode { url: String, code: String },
    PasteCode { url: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PaneCondition {
    Absent,
    Idle,
    Busy,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Fact {
    Prompted(Prompt),
    CodeSubmitted,
    SignedIn,
    Verified {
        account: AgentAccount,
        pane: PaneCondition,
    },
    Failed(AgentLoginFailure),
    Cancel,
    Restart(AgentLoginBrowser),
    Tick,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Effect {
    Launch,
    StopCli,
    SendCode,
    Probe,
    SwitchPane,
    RemoveHome,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Reject {
    NotAwaitingCode,
    Finished,
    Verifying,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Starting,
    Waiting,
    Exchanging,
    Probing,
    Finished,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LoginState {
    pub(crate) browser: AgentLoginBrowser,
    pub(crate) step: AgentLoginStep,
    fresh: bool,
    phase: Phase,
    deadline: Option<Instant>,
}

impl LoginState {
    pub(crate) fn new(browser: AgentLoginBrowser, fresh: bool, now: Instant) -> Self {
        Self {
            browser,
            step: AgentLoginStep::Starting,
            fresh,
            phase: Phase::Starting,
            deadline: Some(now + START_TIMEOUT),
        }
    }

    pub(crate) const fn finished(&self) -> bool {
        matches!(self.phase, Phase::Finished)
    }

    pub(crate) const fn awaits_code(&self) -> bool {
        matches!(self.step, AgentLoginStep::PasteCode { .. })
    }

    fn with(&self, phase: Phase, step: AgentLoginStep, deadline: Option<Instant>) -> Self {
        Self {
            browser: self.browser,
            step,
            fresh: self.fresh,
            phase,
            deadline,
        }
    }

    fn end(&self, step: AgentLoginStep) -> LoginDecision {
        Decision::to(self.with(Phase::Finished, step, None))
            .with_effect(Effect::StopCli)
            .with_effects(self.fresh.then_some(Effect::RemoveHome))
    }
}

pub(crate) struct Ctx {
    pub(crate) now: Instant,
}

pub(crate) struct LoginMachine;

type LoginDecision = Decision<LoginState, Effect, Reject>;

impl Machine for LoginMachine {
    type State = LoginState;
    type Signal = Fact;
    type Ctx = Ctx;
    type Effect = Effect;
    type Time = Instant;
    type Reject = Reject;

    fn transition(state: &LoginState, fact: &Fact, ctx: &Ctx) -> LoginDecision {
        if state.finished() {
            return Decision::reject(Reject::Finished);
        }
        match fact {
            Fact::Prompted(prompt) if state.phase == Phase::Starting => {
                let step = match prompt.clone() {
                    Prompt::Browser { url } => AgentLoginStep::OpenUrl { url },
                    Prompt::DeviceCode { url, code } => AgentLoginStep::DeviceCode { url, code },
                    Prompt::PasteCode { url } => AgentLoginStep::PasteCode { url },
                };
                Decision::to(state.with(Phase::Waiting, step, Some(ctx.now + SIGN_IN_TIMEOUT)))
            }
            Fact::CodeSubmitted if state.awaits_code() => Decision::to(state.with(
                Phase::Exchanging,
                AgentLoginStep::Verifying,
                Some(ctx.now + START_TIMEOUT),
            ))
            .with_effect(Effect::SendCode),
            Fact::CodeSubmitted => Decision::reject(Reject::NotAwaitingCode),
            Fact::SignedIn if state.phase == Phase::Probing => Decision::stay(),
            Fact::SignedIn => {
                Decision::to(state.with(Phase::Probing, AgentLoginStep::Verifying, None))
                    .with_effects([Effect::StopCli, Effect::Probe])
            }
            Fact::Verified { account, pane } if state.phase == Phase::Probing => {
                if !account.signed_in {
                    return state.end(AgentLoginStep::Failed(AgentLoginFailure::Cli(
                        SIGNED_OUT_AFTER_LOGIN.to_owned(),
                    )));
                }
                let (pane, switch) = match pane {
                    PaneCondition::Absent => (None, None),
                    PaneCondition::Idle => {
                        (Some(AgentLoginPane::Switched), Some(Effect::SwitchPane))
                    }
                    PaneCondition::Busy => (Some(AgentLoginPane::Busy), None),
                };
                let done = AgentLoginStep::Done {
                    email: account.email.clone(),
                    organization: account.organization.clone(),
                    plan: account.plan.clone(),
                    pane,
                };
                Decision::to(state.with(Phase::Finished, done, None)).with_effects(switch)
            }
            Fact::Prompted(_) | Fact::Verified { .. } => Decision::stay(),
            Fact::Failed(failure) => state.end(AgentLoginStep::Failed(failure.clone())),
            Fact::Cancel => state.end(AgentLoginStep::Cancelled),
            Fact::Restart(_) if state.phase == Phase::Probing => {
                Decision::reject(Reject::Verifying)
            }
            Fact::Restart(browser) if *browser == state.browser => Decision::stay(),
            Fact::Restart(browser) => Decision::to(LoginState {
                browser: *browser,
                ..state.with(
                    Phase::Starting,
                    AgentLoginStep::Starting,
                    Some(ctx.now + START_TIMEOUT),
                )
            })
            .with_effects([Effect::StopCli, Effect::Launch]),
            Fact::Tick => match state.deadline {
                Some(deadline) if ctx.now >= deadline => {
                    state.end(AgentLoginStep::Failed(AgentLoginFailure::TimedOut))
                }
                _ => Decision::stay(),
            },
        }
    }

    fn wake_at(state: &LoginState) -> Option<Instant> {
        state.deadline
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zz_protocol::AgentProvider;

    const CLAUDE_MANUAL: &str = "https://claude.com/cai/oauth/authorize?code=true&redirect_uri=https%3A%2F%2Fplatform.claude.com%2Foauth%2Fcode%2Fcallback";
    const CLAUDE_AUTOMATIC: &str = "https://claude.com/cai/oauth/authorize?code=true&redirect_uri=http%3A%2F%2Flocalhost%3A60291%2Fcallback";
    const CODEX_DEVICE: &str = "https://auth.openai.com/codex/device";
    const CODEX_BROWSER: &str = "https://auth.openai.com/oauth/authorize?redirect_uri=http%3A%2F%2F127.0.0.1%3A1455%2Fauth%2Fcallback";

    struct Run {
        state: LoginState,
        now: Instant,
        steps: Vec<AgentLoginStep>,
        effects: Vec<Effect>,
    }

    impl Run {
        fn new(browser: AgentLoginBrowser, fresh: bool) -> Self {
            let now = Instant::now();
            Self {
                state: LoginState::new(browser, fresh, now),
                now,
                steps: vec![AgentLoginStep::Starting],
                effects: Vec::new(),
            }
        }

        fn feed(&mut self, fact: &Fact) -> Option<Reject> {
            let step = LoginMachine::step(&mut self.state, fact, &Ctx { now: self.now });
            if step.moved && self.steps.last() != Some(&self.state.step) {
                self.steps.push(self.state.step.clone());
            }
            self.effects.extend(step.effects);
            step.rejected
        }

        fn wait(&mut self, by: Duration) {
            self.now += by;
        }
    }

    fn account(email: &str, plan: &str) -> AgentAccount {
        AgentAccount {
            provider: AgentProvider::ClaudeCode,
            id: Some("a2".to_owned()),
            signed_in: true,
            email: Some(email.to_owned()),
            organization: None,
            plan: Some(plan.to_owned()),
        }
    }

    fn done(email: &str, plan: &str, pane: Option<AgentLoginPane>) -> AgentLoginStep {
        AgentLoginStep::Done {
            email: Some(email.to_owned()),
            organization: None,
            plan: Some(plan.to_owned()),
            pane,
        }
    }

    #[test]
    fn a_claude_sign_in_elsewhere_waits_for_the_pasted_code_then_switches_the_idle_pane() {
        let mut run = Run::new(AgentLoginBrowser::Elsewhere, true);
        assert_eq!(
            run.feed(&Fact::CodeSubmitted),
            Some(Reject::NotAwaitingCode)
        );
        run.feed(&Fact::Prompted(Prompt::PasteCode {
            url: CLAUDE_MANUAL.to_owned(),
        }));
        assert_eq!(run.feed(&Fact::CodeSubmitted), None);
        assert_eq!(
            run.feed(&Fact::CodeSubmitted),
            Some(Reject::NotAwaitingCode)
        );
        run.feed(&Fact::SignedIn);
        run.feed(&Fact::Verified {
            account: account("me@example.com", "max"),
            pane: PaneCondition::Idle,
        });
        assert_eq!(
            run.steps,
            [
                AgentLoginStep::Starting,
                AgentLoginStep::PasteCode {
                    url: CLAUDE_MANUAL.to_owned()
                },
                AgentLoginStep::Verifying,
                done("me@example.com", "max", Some(AgentLoginPane::Switched)),
            ]
        );
        assert_eq!(
            run.effects,
            [
                Effect::SendCode,
                Effect::StopCli,
                Effect::Probe,
                Effect::SwitchPane
            ]
        );
        assert_eq!(LoginMachine::wake_at(&run.state), None);
        assert_eq!(run.feed(&Fact::Cancel), Some(Reject::Finished));
    }

    #[test]
    fn a_claude_sign_in_in_the_local_browser_leaves_a_busy_pane_alone() {
        let mut run = Run::new(AgentLoginBrowser::Local, false);
        run.feed(&Fact::Prompted(Prompt::Browser {
            url: CLAUDE_AUTOMATIC.to_owned(),
        }));
        assert_eq!(
            run.feed(&Fact::CodeSubmitted),
            Some(Reject::NotAwaitingCode)
        );
        run.feed(&Fact::SignedIn);
        run.feed(&Fact::Verified {
            account: account("me@example.com", "pro"),
            pane: PaneCondition::Busy,
        });
        assert_eq!(
            run.steps,
            [
                AgentLoginStep::Starting,
                AgentLoginStep::OpenUrl {
                    url: CLAUDE_AUTOMATIC.to_owned()
                },
                AgentLoginStep::Verifying,
                done("me@example.com", "pro", Some(AgentLoginPane::Busy)),
            ]
        );
        assert_eq!(run.effects, [Effect::StopCli, Effect::Probe]);
    }

    #[test]
    fn a_codex_device_code_sign_in_restarts_in_the_local_browser_and_times_out() {
        let mut run = Run::new(AgentLoginBrowser::Elsewhere, true);
        run.feed(&Fact::Prompted(Prompt::DeviceCode {
            url: CODEX_DEVICE.to_owned(),
            code: "KGAI-S83O5".to_owned(),
        }));
        run.feed(&Fact::Restart(AgentLoginBrowser::Elsewhere));
        run.wait(Duration::from_mins(3));
        run.feed(&Fact::Restart(AgentLoginBrowser::Local));
        assert_eq!(run.state.browser, AgentLoginBrowser::Local);
        run.feed(&Fact::Prompted(Prompt::Browser {
            url: CODEX_BROWSER.to_owned(),
        }));
        run.wait(Duration::from_mins(14) + Duration::from_secs(59));
        run.feed(&Fact::Tick);
        run.wait(Duration::from_secs(1));
        run.feed(&Fact::Tick);
        assert_eq!(
            run.steps,
            [
                AgentLoginStep::Starting,
                AgentLoginStep::DeviceCode {
                    url: CODEX_DEVICE.to_owned(),
                    code: "KGAI-S83O5".to_owned()
                },
                AgentLoginStep::Starting,
                AgentLoginStep::OpenUrl {
                    url: CODEX_BROWSER.to_owned()
                },
                AgentLoginStep::Failed(AgentLoginFailure::TimedOut),
            ]
        );
        assert_eq!(
            run.effects,
            [
                Effect::StopCli,
                Effect::Launch,
                Effect::StopCli,
                Effect::RemoveHome
            ]
        );
    }

    #[test]
    fn a_codex_sign_in_fails_or_is_cancelled_and_only_a_fresh_account_is_removed() {
        let mut run = Run::new(AgentLoginBrowser::Elsewhere, true);
        run.feed(&Fact::Failed(AgentLoginFailure::DeviceCodeDisabled));
        assert_eq!(
            run.steps.last(),
            Some(&AgentLoginStep::Failed(
                AgentLoginFailure::DeviceCodeDisabled
            ))
        );
        assert_eq!(run.effects, [Effect::StopCli, Effect::RemoveHome]);

        let mut run = Run::new(AgentLoginBrowser::Local, false);
        run.feed(&Fact::Prompted(Prompt::Browser {
            url: CODEX_BROWSER.to_owned(),
        }));
        run.feed(&Fact::Cancel);
        assert_eq!(run.steps.last(), Some(&AgentLoginStep::Cancelled));
        assert_eq!(run.effects, [Effect::StopCli]);

        let mut run = Run::new(AgentLoginBrowser::Local, true);
        run.feed(&Fact::SignedIn);
        assert_eq!(
            run.feed(&Fact::Restart(AgentLoginBrowser::Elsewhere)),
            Some(Reject::Verifying)
        );
        run.feed(&Fact::Verified {
            account: AgentAccount {
                signed_in: false,
                ..account("me@example.com", "pro")
            },
            pane: PaneCondition::Absent,
        });
        assert_eq!(
            run.steps.last(),
            Some(&AgentLoginStep::Failed(AgentLoginFailure::Cli(
                SIGNED_OUT_AFTER_LOGIN.to_owned()
            )))
        );
        assert_eq!(
            run.effects,
            [
                Effect::StopCli,
                Effect::Probe,
                Effect::StopCli,
                Effect::RemoveHome
            ]
        );
    }
}
