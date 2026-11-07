use zz_protocol::{MuxSnapshot, PaneId, PaneKindSnapshot, SessionId};
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

#[must_use]
pub fn desktop_prefix_acknowledged(previous: Option<u64>, request_id: u64) -> Option<u64> {
    previous.max(Some(request_id))
}

#[must_use]
pub fn desktop_prefix_reset(previous: Option<u64>, next_request: u64) -> Option<u64> {
    if next_request == 0 {
        previous
    } else {
        desktop_prefix_acknowledged(previous, next_request)
    }
}

#[must_use]
pub fn desktop_active_pane(snapshot: &MuxSnapshot, attached: Option<SessionId>) -> Option<PaneId> {
    let attached = attached?;
    let session = snapshot
        .sessions
        .iter()
        .find(|session| session.id == attached)?;
    let focused = snapshot.focused_window_for(session);
    session
        .windows
        .iter()
        .find(|window| window.id == focused)
        .map(|window| window.active_pane)
}

#[must_use]
pub fn desktop_modal_capture(
    snapshot: &MuxSnapshot,
    attached: Option<SessionId>,
) -> Option<PaneId> {
    let attached = attached?;
    let session = snapshot
        .sessions
        .iter()
        .find(|session| session.id == attached)?;
    let focused = snapshot.focused_window_for(session);
    session
        .windows
        .iter()
        .find(|window| window.id == focused)?
        .modal
        .filter(|modal| modal.capture_keys)
        .map(|modal| modal.pane)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DesktopSnapshotTarget {
    pub active: Option<(PaneId, SurfaceKind)>,
    pub capture: Option<PaneId>,
}

#[must_use]
pub fn desktop_snapshot_target(
    snapshot: &MuxSnapshot,
    attached: Option<SessionId>,
) -> DesktopSnapshotTarget {
    let active = desktop_active_pane(snapshot, attached).and_then(|id| {
        snapshot
            .sessions
            .iter()
            .filter(|session| Some(session.id) == attached)
            .flat_map(|session| &session.windows)
            .find_map(|window| window.panes.get(&id))
            .map(|pane| {
                (
                    id,
                    match pane.kind {
                        PaneKindSnapshot::Terminal => SurfaceKind::Terminal,
                        PaneKindSnapshot::Browser(_) => SurfaceKind::Browser,
                        _ => SurfaceKind::Other,
                    },
                )
            })
    });
    DesktopSnapshotTarget {
        active,
        capture: desktop_modal_capture(snapshot, attached),
    }
}
