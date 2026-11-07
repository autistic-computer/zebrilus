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
