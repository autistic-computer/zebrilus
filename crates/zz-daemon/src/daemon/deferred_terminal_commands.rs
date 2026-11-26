use super::*;

pub(super) enum DeferredTerminalCommand {
    SetWordSeparators {
        terminal: Arc<TerminalSession>,
        separators: WordSeparators,
    },
    SetEngineKnobs {
        terminal: Arc<TerminalSession>,
        knobs: EngineKnobs,
    },
    ArmCopySource {
        terminal: Arc<TerminalSession>,
        source: Arc<TerminalSession>,
    },
    SetAllowPassthrough {
        terminal: Arc<TerminalSession>,
        enabled: bool,
    },
    SetWrapSearch {
        terminal: Arc<TerminalSession>,
        enabled: bool,
    },
    SetAppearance {
        terminal: Arc<TerminalSession>,
        appearance: Arc<TerminalAppearance>,
    },
    AttachView {
        terminal: Arc<TerminalSession>,
        view: TerminalViewId,
    },
    Resize {
        terminal: Arc<TerminalSession>,
        geometry: TerminalGeometry,
    },
    ViewAction {
        terminal: Arc<TerminalSession>,
        view: TerminalViewId,
        action: zz_terminal::TerminalViewAction,
    },
    SendTokens {
        terminals: Vec<Arc<TerminalSession>>,
        keys: Vec<zz_protocol::KeyToken>,
        repeat: u32,
    },
    ResetScreen {
        terminals: Vec<Arc<TerminalSession>>,
    },
}

impl DeferredTerminalCommand {
    pub(super) fn copy_mode_terminal(&self) -> Option<&Arc<TerminalSession>> {
        match self {
            Self::ViewAction {
                terminal, action, ..
            } if terminal_view_action_enters_copy_mode(action)
                || matches!(
                    action,
                    zz_terminal::TerminalViewAction::CopyMode(_)
                        | zz_terminal::TerminalViewAction::CopyModeCounted { .. }
                ) =>
            {
                Some(terminal)
            }
            _ => None,
        }
    }

    #[cfg(test)]
    pub(super) const fn wrap_search(&self) -> Option<bool> {
        match self {
            Self::SetWrapSearch { enabled, .. } => Some(*enabled),
            _ => None,
        }
    }

    pub(super) fn run(self) {
        match self {
            Self::SetWordSeparators {
                terminal,
                separators,
            } => terminal.set_word_separators(separators),
            Self::SetAllowPassthrough { terminal, enabled } => {
                terminal.set_allow_passthrough(enabled);
            }
            Self::SetWrapSearch { terminal, enabled } => terminal.set_wrap_search(enabled),
            Self::SetEngineKnobs { terminal, knobs } => terminal.set_engine_knobs(knobs),
            Self::ArmCopySource { .. } => {
                unreachable!("copy sources resume through the terminal inbox")
            }
            Self::SetAppearance {
                terminal,
                appearance,
            } => terminal.set_appearance(appearance),
            Self::AttachView { terminal, view } => terminal.attach_view(view),
            Self::ResetScreen { terminals } => {
                for terminal in terminals {
                    terminal.reset_screen();
                }
            }
            Self::Resize { terminal, geometry } => terminal.resize(
                geometry.columns,
                geometry.rows,
                geometry.cell_width_px,
                geometry.cell_height_px,
            ),
            Self::ViewAction {
                terminal,
                view,
                action,
            } => terminal.view_action(view, action),
            Self::SendTokens {
                terminals,
                keys,
                repeat,
            } => repeat_until_rejected(repeat, || send_tokens(&terminals, &keys)),
        }
    }
}

pub(super) fn repeat_until_rejected(mut repeat: u32, mut send: impl FnMut() -> bool) {
    while repeat > 0 && send() {
        repeat -= 1;
    }
}
