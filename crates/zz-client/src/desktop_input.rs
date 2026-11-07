use zz_protocol::PaneId;
use zz_terminal::Modifiers;

use crate::{ChromeAction, ChromeKeymap, InputEvent, InputRouter, PrefixView, SurfaceKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DesktopDrag {
    None,
    Free,
    Prefix,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DesktopGuard {
    Native,
    CancelDrag,
    WaitPrefixCancel,
    Route,
}

#[derive(Clone, Copy, Debug)]
pub struct DesktopGuardContext<'a> {
    pub key: &'a str,
    pub overlay: bool,
    pub cancel_pending: bool,
    pub drag: DesktopDrag,
    pub modifiers: Modifiers,
    pub function: bool,
}

#[must_use]
pub fn desktop_intercepts(window_matches: bool, key: &str) -> bool {
    window_matches && !matches!(key, "shift" | "control" | "alt" | "platform" | "function")
}

#[must_use]
pub fn desktop_guard(context: DesktopGuardContext<'_>) -> DesktopGuard {
    if !context.overlay && context.key == "escape" && context.drag == DesktopDrag::Free {
        DesktopGuard::CancelDrag
    } else if !context.overlay
        && context.cancel_pending
        && !context.modifiers.platform()
        && !context.function
    {
        DesktopGuard::WaitPrefixCancel
    } else {
        DesktopGuard::Route
    }
}

#[derive(Clone, Copy, Debug)]
pub struct DesktopKeyContext {
    pub before: Option<(PaneId, SurfaceKind)>,
    pub active: Option<(PaneId, SurfaceKind)>,
    pub capture: Option<PaneId>,
    pub prefix: PrefixView,
    pub overlay: bool,
    pub modifiers: Modifiers,
    pub function: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DesktopKeyProjection {
    pub event: Option<InputEvent>,
    pub capture: Option<PaneId>,
    pub prefix: PrefixView,
}

#[must_use]
pub fn desktop_key_projection(context: DesktopKeyContext) -> DesktopKeyProjection {
    DesktopKeyProjection {
        event: (context.before != context.active).then(|| {
            context.active.map_or(InputEvent::Detached, |(pane, kind)| {
                InputEvent::SetActivePane(pane, kind)
            })
        }),
        capture: if context.overlay {
            None
        } else {
            context.capture
        },
        prefix: PrefixView {
            armed: context.prefix.armed,
            claimed: context.prefix.claimed
                && !context.overlay
                && !context.modifiers.platform()
                && !context.function,
        },
    }
}

#[must_use]
pub fn desktop_router(keymap: ChromeKeymap) -> InputRouter {
    let mut router = InputRouter::new(keymap);
    for action in [
        ChromeAction::UiZoomIn,
        ChromeAction::UiZoomOut,
        ChromeAction::UiZoomReset,
    ] {
        router.unbind_action(action);
    }
    router
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DesktopChromeDispatch {
    None,
    NewSession,
    NewWindow,
    SelectWindow(u8),
    SplitRight,
    SplitDown,
    Detach,
    ToggleSidebar,
    OpenSettings,
    OpenCommandPalette,
}

#[must_use]
pub const fn desktop_chrome_dispatch(action: ChromeAction) -> DesktopChromeDispatch {
    match action {
        ChromeAction::NewSession => DesktopChromeDispatch::NewSession,
        ChromeAction::NewWindow => DesktopChromeDispatch::NewWindow,
        ChromeAction::SelectWindow(index) => DesktopChromeDispatch::SelectWindow(index),
        ChromeAction::SplitRight => DesktopChromeDispatch::SplitRight,
        ChromeAction::SplitDown => DesktopChromeDispatch::SplitDown,
        ChromeAction::Detach => DesktopChromeDispatch::Detach,
        ChromeAction::ToggleSidebar => DesktopChromeDispatch::ToggleSidebar,
        ChromeAction::OpenSettings => DesktopChromeDispatch::OpenSettings,
        ChromeAction::OpenCommandPalette => DesktopChromeDispatch::OpenCommandPalette,
        _ => DesktopChromeDispatch::None,
    }
}

#[must_use]
pub fn next_prefix_cancel_request(previous: u64) -> u64 {
    previous.saturating_add(1).max(1)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DesktopDialogPrefix {
    pub sent: bool,
    pub pending: Option<u64>,
}

#[must_use]
pub fn desktop_dialog_prefix(
    mut state: DesktopDialogPrefix,
    active: bool,
    acknowledged: Option<u64>,
) -> (DesktopDialogPrefix, bool) {
    if state
        .pending
        .is_some_and(|pending| acknowledged.is_some_and(|acknowledged| acknowledged >= pending))
    {
        state.pending = None;
    }
    if !active {
        state.sent = false;
    }
    let request = active && !state.sent;
    (state, request)
}
