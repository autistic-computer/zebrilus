pub trait Machine {
    type State;
    type Signal;
    type Ctx;
    type Effect;
    type Time: Ord;
    type Reject;

    fn transition(
        state: &Self::State,
        signal: &Self::Signal,
        ctx: &Self::Ctx,
    ) -> Decision<Self::State, Self::Effect, Self::Reject>;

    fn wake_at(state: &Self::State) -> Option<Self::Time>;

    fn step(
        state: &mut Self::State,
        signal: &Self::Signal,
        ctx: &Self::Ctx,
    ) -> Step<Self::Effect, Self::Time, Self::Reject> {
        let decision = Self::transition(state, signal, ctx);
        let moved = decision.next.is_some();
        if let Some(next) = decision.next {
            *state = next;
        }
        Step {
            moved,
            effects: decision.effects,
            rejected: decision.rejected,
            wake_at: Self::wake_at(state),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Decision<S, E, R = ()> {
    pub next: Option<S>,
    pub effects: Vec<E>,
    pub rejected: Option<R>,
}

impl<S, E, R> Decision<S, E, R> {
    #[must_use]
    pub fn stay() -> Self {
        Self {
            next: None,
            effects: Vec::new(),
            rejected: None,
        }
    }

    #[must_use]
    pub fn to(next: S) -> Self {
        Self {
            next: Some(next),
            effects: Vec::new(),
            rejected: None,
        }
    }

    #[must_use]
    pub fn reject(reason: R) -> Self {
        Self {
            next: None,
            effects: Vec::new(),
            rejected: Some(reason),
        }
    }

    #[must_use]
    pub fn with_effect(mut self, effect: E) -> Self {
        self.effects.push(effect);
        self
    }

    #[must_use]
    pub fn with_effects(mut self, effects: impl IntoIterator<Item = E>) -> Self {
        self.effects.extend(effects);
        self
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Step<E, T, R = ()> {
    pub moved: bool,
    pub effects: Vec<E>,
    pub rejected: Option<R>,
    pub wake_at: Option<T>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Offer {
        Waiting { deadline: u64 },
        Active,
        Failed,
    }

    enum Signal {
        Accept,
        Decline,
        Tick,
    }

    #[derive(Debug, PartialEq, Eq)]
    enum Effect {
        Notify,
        Terminate,
    }

    struct OfferMachine;

    impl Machine for OfferMachine {
        type State = Offer;
        type Signal = Signal;
        type Ctx = u64;
        type Effect = Effect;
        type Time = u64;
        type Reject = &'static str;

        fn transition(
            state: &Offer,
            signal: &Signal,
            now: &u64,
        ) -> Decision<Offer, Effect, &'static str> {
            match (state, signal) {
                (Offer::Waiting { .. }, Signal::Accept) => {
                    Decision::to(Offer::Active).with_effect(Effect::Notify)
                }
                (Offer::Waiting { deadline }, Signal::Tick) if now >= deadline => {
                    Decision::to(Offer::Failed).with_effect(Effect::Terminate)
                }
                (Offer::Active, Signal::Decline) => Decision::reject("already active"),
                _ => Decision::stay(),
            }
        }

        fn wake_at(state: &Offer) -> Option<u64> {
            match state {
                Offer::Waiting { deadline } => Some(*deadline),
                Offer::Active | Offer::Failed => None,
            }
        }
    }

    #[test]
    fn a_transition_commits_and_returns_its_effects() {
        let mut state = Offer::Waiting { deadline: 10 };
        let step = OfferMachine::step(&mut state, &Signal::Accept, &0);
        assert_eq!(state, Offer::Active);
        assert!(step.moved);
        assert_eq!(step.effects, [Effect::Notify]);
        assert_eq!(step.wake_at, None);
    }

    #[test]
    fn an_unrelated_signal_keeps_the_pending_deadline() {
        let mut state = Offer::Waiting { deadline: 10 };
        let step = OfferMachine::step(&mut state, &Signal::Decline, &3);
        assert_eq!(state, Offer::Waiting { deadline: 10 });
        assert!(!step.moved);
        assert_eq!(step.wake_at, Some(10));
    }

    #[test]
    fn a_due_tick_fires_the_deadline_and_clears_it() {
        let mut state = Offer::Waiting { deadline: 10 };
        assert_eq!(
            OfferMachine::step(&mut state, &Signal::Tick, &3).wake_at,
            Some(10)
        );
        let step = OfferMachine::step(&mut state, &Signal::Tick, &10);
        assert_eq!(state, Offer::Failed);
        assert_eq!(step.effects, [Effect::Terminate]);
        assert_eq!(step.wake_at, None);
    }

    #[test]
    fn a_rejected_signal_leaves_the_state_alone() {
        let mut state = Offer::Active;
        let step = OfferMachine::step(&mut state, &Signal::Decline, &0);
        assert_eq!(state, Offer::Active);
        assert!(!step.moved);
        assert_eq!(step.rejected, Some("already active"));
    }
}
