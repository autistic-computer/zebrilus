#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WhichKeyTiming {
    pub generation: u64,
    pub timer: bool,
    pub visible: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WhichKeyChange {
    pub state: WhichKeyTiming,
    pub notify: bool,
    pub build: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TimerDecision {
    Stale,
    Drop,
    Show,
}

#[must_use]
pub const fn hidden_generation(generation: u64) -> u64 {
    generation.wrapping_add(1)
}

#[must_use]
pub const fn hide_for_input(held: bool, armed: bool) -> bool {
    !(held && armed)
}

#[must_use]
pub const fn arm_table(table: Option<(&str, bool)>) -> Option<&str> {
    match table {
        Some((name, false)) => Some(name),
        _ => None,
    }
}

#[must_use]
pub const fn timer_decision(
    generation: u64,
    fired_generation: u64,
    table_matches: bool,
    blocked: bool,
) -> TimerDecision {
    if generation != fired_generation {
        TimerDecision::Stale
    } else if !table_matches || blocked {
        TimerDecision::Drop
    } else {
        TimerDecision::Show
    }
}

impl WhichKeyTiming {
    #[must_use]
    pub const fn hide(self) -> WhichKeyChange {
        WhichKeyChange {
            state: Self {
                generation: hidden_generation(self.generation),
                timer: false,
                visible: false,
            },
            notify: self.visible,
            build: false,
        }
    }

    #[must_use]
    pub const fn input(self, eligible: bool, held: bool, armed: bool) -> WhichKeyChange {
        if eligible && hide_for_input(held, armed) {
            self.hide()
        } else {
            WhichKeyChange {
                state: self,
                notify: false,
                build: false,
            }
        }
    }

    #[must_use]
    pub const fn table_changed(self, eligible_table: bool, delay_enabled: bool) -> WhichKeyChange {
        let mut next = self.hide();
        next.state.timer = eligible_table && delay_enabled;
        next
    }

    #[must_use]
    pub const fn fire(
        self,
        fired_generation: u64,
        table_matches: bool,
        blocked: bool,
        has_rows: bool,
    ) -> WhichKeyChange {
        let decision = timer_decision(self.generation, fired_generation, table_matches, blocked);
        if matches!(decision, TimerDecision::Stale) {
            return WhichKeyChange {
                state: self,
                notify: false,
                build: false,
            };
        }
        let mut state = self;
        state.timer = false;
        let show = matches!(decision, TimerDecision::Show);
        if show {
            state.visible = has_rows;
        }
        WhichKeyChange {
            state,
            notify: show,
            build: show,
        }
    }

    #[must_use]
    pub const fn render(self, blocked: bool) -> WhichKeyChange {
        let mut state = self;
        if blocked {
            state.timer = false;
            state.visible = false;
        }
        WhichKeyChange {
            state,
            notify: false,
            build: false,
        }
    }

    #[must_use]
    pub const fn show_all(self, has_rows: bool) -> WhichKeyChange {
        let mut state = self;
        state.visible = has_rows;
        WhichKeyChange {
            state,
            notify: true,
            build: true,
        }
    }
}
