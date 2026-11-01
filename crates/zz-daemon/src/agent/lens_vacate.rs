use super::{AgentStreamPayload, Claude, Codex, CodexTranslator, Lens, Link, Source};

impl Claude {
    fn unload(&mut self) {
        if let Some(link) = self.link.as_mut() {
            link.forget();
        }
        self.tail = None;
        self.last = None;
        self.open.clear();
        self.subagents = None;
        self.agents.clear();
        self.dialog_closed = false;
    }
}

impl Codex {
    pub(super) fn left(&self, pane_pid: u32) -> bool {
        !self.pid.is_some_and(crate::pid_alive)
            && crate::agent::codex_queue::codex_pid(pane_pid).is_none()
    }

    fn unload(&mut self) {
        self.leave();
        self.tail = None;
        self.owner = None;
        self.translator = CodexTranslator::default();
        self.busy = false;
        self.ended = None;
        self.children.clear();
        self.held.clear();
        self.asked.clear();
        self.dropped.clear();
    }
}

impl Source {
    pub(super) fn vacant(&self) -> bool {
        match self {
            Self::Claude(claude) => claude.vacant,
            Self::Codex(codex) => codex.pid != codex.owner,
        }
    }

    pub(super) fn own(&mut self) {
        if let Self::Codex(codex) = self {
            codex.owner = codex.pid;
        }
    }

    pub(super) fn awaiting(&self) -> bool {
        match self {
            Self::Claude(_) => false,
            Self::Codex(codex) => codex.tail.is_none() && codex.pid.is_some() && codex.crowded(),
        }
    }

    fn moved(&self, session: &str) -> bool {
        match self {
            Self::Claude(claude) => claude
                .record
                .as_ref()
                .is_none_or(|record| record.session_id != session),
            Self::Codex(codex) => codex.pid.is_some_and(|pid| !crate::pid_alive(pid)),
        }
    }

    fn unload(&mut self) -> Option<String> {
        match self {
            Self::Claude(claude) => {
                let abandoned = claude.link.as_mut().and_then(Link::abandon);
                claude.unload();
                abandoned
            }
            Self::Codex(codex) => {
                codex.unload();
                None
            }
        }
    }
}

impl Lens {
    pub(super) fn moved(&self) -> bool {
        self.session
            .as_deref()
            .is_some_and(|session| self.source.moved(session))
    }

    pub(super) fn vacate(&mut self) -> Vec<AgentStreamPayload> {
        if !std::mem::take(&mut self.vacant) || self.session.is_none() {
            return Vec::new();
        }
        self.failed = self.source.unload();
        let mut payloads = self.settle(false);
        self.failed = None;
        self.session = None;
        self.permission = None;
        self.dialog = None;
        self.busy = false;
        payloads.push(AgentStreamPayload::SessionReset { restoring: false });
        payloads.push(AgentStreamPayload::SessionReady {
            session_id: self.placeholder(),
            modes: None,
            config_options: Some(self.config_options()),
        });
        payloads.push(self.commands());
        payloads
    }
}
