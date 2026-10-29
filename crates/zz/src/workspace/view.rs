//! Main workspace view and pane-layout reconciliation.

use zz_ui::command::floating::{FloatingFrame as PopupFrame, floating_frame};

use std::{
    cell::Cell,
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
    sync::Arc,
    time::Duration,
};

use ala::{
    AnchoredPositionMode, Animation, AnimationExt as _, AnyElement, AnyView, AnyWindowHandle, App,
    Bounds, Context, Corners, CursorStyle, DragMoveEvent, Entity, EntityId, FocusHandle,
    IntoElement, KeyUpEvent, Keystroke, Modifiers, MouseButton, MouseDownEvent, MouseExitEvent,
    MouseMoveEvent, MouseUpEvent, Pixels, Point, Render, Size, StyleRefinement, Task, WeakEntity,
    Window, anchored, div, ease_out_quint, point, prelude::*, px,
};
#[cfg(test)]
use zz_client::pane_swap_command;
use zz_client::{
    ChromeAction, Disposition, DropZone, Effect, InputEvent, InputRouter, MenuPointerKind,
    NormalizedPaneRect, PaneRect, PrefixView, SurfaceKind, coerced_drop_zone, drop_preview_bounds,
    drop_zone_at,
    floating::{
        FloatCells, FloatGrip, FloatWindow, float_drag_command, float_drag_preview, float_pixels,
        float_title,
    },
    pane_drop_command, pane_rects, predicted_drop_layout,
};
use zz_mux::display_width;
use zz_protocol::{
    Axis, ClientMessageKind, CommandInvocation, DisplayPanesAction, FloatingPaneSnapshot,
    GuiResponse, InputMessage, LayoutNode, MenuState, ModalPaneSnapshot, MuxSnapshot,
    PROTOCOL_VERSION, PaneBorderLines, PaneBorderStatus, PaneId, PaneIndicator, PaneKindSnapshot,
    PopupBorderLines, SPLIT_RATIO_BASIS, SessionId, SplitId, WindowId, WindowSnapshot,
};
use zz_terminal::KeyAction as TerminalKeyAction;
use zz_ui::attachment::open_attachment_preview;
use zz_ui::dialog::DialogButtonProps;
use zz_ui::pip::{PipCard, PipDrag, PipLayout, PipPlaced, PipStage, PipSurface, PipThrow};
use zz_ui::{
    ActiveTheme as _, ElementExt as _, Emphasis, Tone, WindowExt as _, button::ButtonVariant,
    draws_window_controls, notification::Notification,
};
use zz_ui::{
    pane::{
        DropPreview, DropPreviewFrame, FloatingSurface, PaneArrivals, PaneChrome, PaneDrag,
        PaneDragOverlayState, PaneDragSource, PaneLens, PaneMotion, PaneOverlayCorner,
        PaneSplitAxis, PaneSplitHighlight, PaneSplitSide, PipGrab, TerminalPaneAction, moving_pane,
        pane_border_color, pane_drag_overlay, pane_drag_preview, pane_drop_preview,
        pane_header_frame, pane_indicator, pane_indicator_overlay, pane_overlay_stack,
        pane_split_hit_target, pane_split_slot, pane_split_surface, pane_surface, pane_sync_badge,
        pane_unzoom_control, pane_waiting_state, terminal_pane_header,
    },
    shell::{app_connection_state, app_workspace_surface},
};

use super::{
    new_session::NewSessionView,
    sidebar::{
        ChromeMode, SidebarModeChanged, SidebarReleaseFocus, SidebarRouteChanged, WorkspaceRoute,
        WorkspaceSidebar,
    },
    which_key::{self, WhichKeySheet},
};
use crate::{
    agent::AgentController,
    agent::{AgentView, ShowTerminal},
    browser::controller::{BrowserController, ControllerEvent},
    browser::view::BrowserView,
    chooser::buffer::ChooseBufferView,
    chooser::tree::ChooseTreeView,
    command::{
        confirm::ConfirmView,
        menu::MenuView,
        palette::{
            CommandPaletteEvent, CommandPaletteView, GuiEvent, PaletteMode, palette_backend,
            palette_backend_with,
        },
    },
    config::{self, frame_content_corner_radius},
    diagnostics,
    editor::EditorView,
    mux::{
        client::{
            AttachmentPreviewRequest, ClientNotification, ClientNotificationCleared,
            KeyTableChanged, MuxClient, PathPickerUpdate, RemoteInstallOffer, SshPromptRequest,
        },
        hosts::HostId,
        nav::{TreeTarget, kill_target_command},
        prefix::{binds_input, is_all_keys_input, is_sidebar_picker_input, terminal_key_input},
    },
    pane::display::DisplayPanesView,
    pane::layout::{SeparatorSide, pane_separator},
    pane::picker::{AgentChosen, ChooseFolder, PanePickerView},
    terminal::view::TerminalView,
    window::{corners::WindowCorners, drag::window_drag_handle},
};

const DROP_PREVIEW_MORPH: Duration = Duration::from_millis(180);
const DROP_PREVIEW_FADE: Duration = Duration::from_millis(140);

ala::actions!(zz, [ClosePane]);

const DIAGNOSTIC_TARGET: &str = "zz::diagnostics::app_render";

const PANE_KEY_CONTEXT: &str = "ZzPane";
const PANE_KEY_CONTEXT_ID: &str = "pane";

/// Prompt once during this launch when the initial local connection found a stale daemon.
/// Returns whether it consumed startup's dialog slot.
pub(crate) fn maybe_prompt_stale_daemon(
    mux: &Entity<MuxClient>,
    window: &mut Window,
    cx: &mut App,
) -> bool {
    prompt_stale_daemon(mux, false, window, cx)
}

fn prompt_stale_daemon(
    mux: &Entity<MuxClient>,
    explicit: bool,
    window: &mut Window,
    cx: &mut App,
) -> bool {
    let stale = mux.read(cx).stale_daemon();
    if stale.is_none() && !explicit {
        return false;
    }
    if !explicit && config::auto_restart_stale_daemon(cx) {
        mux.update(cx, |mux, cx| mux.restart_stale_daemon(true, cx));
        return true;
    }
    let daemon = stale.and_then(|stale| stale.daemon);

    let description = if explicit && stale.is_none() {
        "zz was updated, but the daemon is still running the previous build. Restarting it will \
         end all running sessions."
            .to_owned()
    } else {
        daemon.map_or_else(
            || {
                format!(
                    "The running daemon is from an older build (its protocol version is unknown; \
                 this zz speaks v{PROTOCOL_VERSION}). Restarting it will end all running sessions."
                )
            },
            |daemon| {
                format!(
                    "The running daemon is from an older build (daemon protocol v{daemon}; this zz \
                 speaks v{PROTOCOL_VERSION}). Restarting it will end all running sessions."
                )
            },
        )
    };
    let restart_mux = mux.clone();
    let dismiss_mux = mux.clone();
    window.open_alert_dialog(cx, move |alert, _, _| {
        let restart_mux = restart_mux.clone();
        let dismiss_mux = dismiss_mux.clone();
        alert
            .title("Restart the zz daemon?")
            .description(description.clone())
            .button_props(
                DialogButtonProps::default()
                    .ok_variant(ButtonVariant::Paint(Tone::Danger, Emphasis::Solid))
                    .ok_text("Restart daemon")
                    .cancel_text("Not now")
                    .show_cancel(true),
            )
            .on_ok(move |_, _, cx| {
                restart_mux.update(cx, |mux, cx| {
                    if explicit {
                        mux.restart_daemon_for_update(cx);
                        return;
                    }
                    mux.restart_stale_daemon(false, cx);
                });
                true
            })
            .on_cancel(move |_, _, cx| {
                if stale.is_some() {
                    dismiss_mux.update(
                        cx,
                        super::super::mux::client::MuxClient::dismiss_stale_daemon,
                    );
                }
                true
            })
    });
    true
}

fn pane_key_context(pane: PaneId) -> ala::KeyContext {
    let mut context = ala::KeyContext::new_with_defaults();
    context.add(PANE_KEY_CONTEXT);
    context.set(PANE_KEY_CONTEXT_ID, pane.0.to_string());
    context
}

const fn empty_workspace_available(
    snapshot_generation: u64,
    session_count: usize,
    has_error: bool,
) -> bool {
    snapshot_generation > 0 && session_count == 0 && !has_error
}

const fn empty_workspace_focus_owed(visible: bool, was_visible: bool, pending: bool) -> bool {
    visible && (pending || !was_visible)
}

fn attached_focused_window(
    snapshot: &MuxSnapshot,
    attached: Option<SessionId>,
) -> Option<WindowId> {
    snapshot
        .sessions
        .iter()
        .find(|session| Some(session.id) == attached)
        .map(|session| snapshot.focused_window_for(session))
}

const WINDOW_SHORTCUT_HINT_DELAY: Duration = Duration::from_millis(300);

fn window_shortcut_modifiers(cx: &App) -> Option<Modifiers> {
    let chord = crate::keymap::chord_for(cx, zz_client::UI_TABLE, ChromeAction::SelectWindow(0))?;
    Keystroke::parse(&chord).ok().map(|stroke| stroke.modifiers)
}

fn browser_metadata_command(
    pane: PaneId,
    event: &zz_browser::BrowserEvent,
) -> Option<CommandInvocation> {
    match event {
        zz_browser::BrowserEvent::TitleChanged { title, .. } => Some(CommandInvocation::new(
            "select-pane",
            vec![
                "-t".to_owned(),
                pane.to_string(),
                "-T".to_owned(),
                title.to_string(),
            ],
        )),
        _ => None,
    }
}

fn collect_pane_corners(
    node: &LayoutNode,
    corners: WindowCorners,
    by_pane: &mut BTreeMap<PaneId, WindowCorners>,
) {
    match node {
        LayoutNode::Pane(pane) => {
            by_pane.insert(*pane, corners);
        }
        LayoutNode::Empty => {}
        LayoutNode::Split {
            axis,
            first,
            second,
            ..
        } => {
            let (first_corners, second_corners) = corners.split(*axis);
            collect_pane_corners(first, first_corners, by_pane);
            collect_pane_corners(second, second_corners, by_pane);
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct SplitDrag {
    window: WindowId,
    split: SplitId,
    start_ratio: f32,
    axis: Axis,
}

#[derive(Clone, Copy, Debug)]
struct SplitDragState {
    drag: SplitDrag,
    ratio: f32,
    committed_snapshot_revision: Option<u64>,
}

struct SplitDragPreview;

const PIP_GRIP: f32 = 8.0;

#[derive(Clone, Copy, Debug)]
struct FloatDrag {
    pane: PaneId,
}

#[derive(Clone, Copy, Debug)]
struct FloatDragState {
    window: WindowId,
    pane: PaneId,
    grip: FloatGrip,
    lines: PaneBorderLines,
    area: FloatWindow,
    start: FloatCells,
    press: Point<Pixels>,
    cell: Size<Pixels>,
    delta: (i32, i32),
    started: bool,
    committed_snapshot_revision: Option<u64>,
    request: Option<u64>,
    pip: Option<PipDrag>,
    limit: Option<(u16, u16)>,
}

#[derive(Clone, Copy, Debug)]
struct PipLandingState {
    window: WindowId,
    pane: PaneId,
    target: FloatCells,
    request: Option<u64>,
}

impl FloatDragState {
    fn target(&self) -> FloatCells {
        let target = float_drag_preview(self.start, self.grip, self.delta);
        self.limit.map_or(target, |limit| {
            zz_client::floating::float_drag_within(target, self.grip, limit)
        })
    }

    fn preview(&self) -> Option<FloatCells> {
        self.started.then(|| self.target())
    }

    const fn throwing(&self) -> bool {
        self.pip.is_some()
    }

    fn thrown(&self) -> Option<PipDrag> {
        self.pip.filter(|_| self.started)
    }

    fn halfway(&self) -> Option<FloatCells> {
        (!self.grip.is_move() && self.grip.moves_origin()).then(|| FloatCells {
            xoff: self.start.xoff,
            yoff: self.start.yoff,
            ..self.target()
        })
    }
}

impl Render for SplitDragPreview {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size(px(1.0)).opacity(0.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PaneDragLayer {
    Idle,
    Armed,
    Dragging(PaneId),
}

#[derive(Clone, Debug, PartialEq)]
struct PaneDragState {
    source: PaneId,
    requires_prefix: bool,
    window: WindowId,
    layout: LayoutNode,
    slots: Vec<(PaneId, Bounds<Pixels>)>,
    target: Option<(PaneId, DropZone)>,
    preview: Option<DropPreview>,
    divider: Pixels,
}

impl PaneDragState {
    fn new(
        source: PaneId,
        window: WindowId,
        layout: &LayoutNode,
        canvas_size: Size<Pixels>,
        divider: Pixels,
    ) -> Option<Self> {
        if canvas_size.width <= px(0.0) || canvas_size.height <= px(0.0) {
            return None;
        }
        let slots = pane_rects(layout)
            .into_iter()
            .map(|(pane, rect)| (pane, pane_bounds(rect, canvas_size)))
            .collect::<Vec<_>>();
        slots.iter().any(|(pane, _)| *pane == source).then(|| Self {
            source,
            requires_prefix: true,
            window,
            layout: layout.clone(),
            slots,
            target: None,
            preview: None,
            divider,
        })
    }

    fn target_at(&self, position: Point<Pixels>) -> Option<(PaneId, DropZone)> {
        let (target, slot) = self
            .slots
            .iter()
            .find(|(pane, slot)| *pane != self.source && slot.contains(&position))?;
        let zone = drop_zone_at(
            PaneRect {
                x: slot.origin.x.into(),
                y: slot.origin.y.into(),
                width: slot.size.width.into(),
                height: slot.size.height.into(),
            },
            (position.x.into(), position.y.into()),
        );
        Some((
            *target,
            coerced_drop_zone(&self.layout, self.source, *target, zone),
        ))
    }

    fn set_target(
        &mut self,
        target: Option<(PaneId, DropZone)>,
        rendered: DropPreviewFrame,
    ) -> bool {
        let target = target.filter(|(target, _)| {
            *target != self.source && self.slots.iter().any(|(pane, _)| pane == target)
        });
        if self.target == target {
            return false;
        }
        self.target = target;
        let sequence = self.preview.map_or(0, |preview| preview.sequence + 1);
        self.preview = Some(match target {
            Some((pane, zone)) => {
                let to = self.slot(pane).map_or(rendered.bounds, |slot| {
                    let rect = drop_preview_bounds(
                        PaneRect {
                            x: slot.origin.x.into(),
                            y: slot.origin.y.into(),
                            width: slot.size.width.into(),
                            height: slot.size.height.into(),
                        },
                        zone,
                        self.divider.into(),
                    );
                    Bounds::new(
                        ala::point(px(rect.x), px(rect.y)),
                        ala::size(px(rect.width), px(rect.height)),
                    )
                });
                DropPreview {
                    from: if rendered.opacity > 0.0 {
                        rendered
                    } else {
                        DropPreviewFrame {
                            bounds: to,
                            opacity: 0.0,
                        }
                    },
                    to: DropPreviewFrame {
                        bounds: to,
                        opacity: 1.0,
                    },
                    sequence,
                    duration: DROP_PREVIEW_MORPH,
                }
            }
            None => DropPreview {
                from: rendered,
                to: DropPreviewFrame {
                    bounds: rendered.bounds,
                    opacity: 0.0,
                },
                sequence,
                duration: DROP_PREVIEW_FADE,
            },
        });
        true
    }

    fn slot(&self, pane: PaneId) -> Option<Bounds<Pixels>> {
        self.slots
            .iter()
            .find_map(|(candidate, rect)| (*candidate == pane).then_some(*rect))
    }

    fn matches_layout(&self, window: WindowId, layout: &LayoutNode) -> bool {
        self.window == window && self.layout == *layout
    }

    fn matches(&self, window: &WindowSnapshot, prefix_armed: bool) -> bool {
        (!self.requires_prefix || prefix_armed)
            && window.zoomed_pane.is_none()
            && self.slots.len() > 1
            && self.matches_layout(window.id, &window.layout)
            && self
                .slots
                .iter()
                .all(|(pane, _)| window.panes.contains_key(pane))
    }
}

#[derive(Clone, Debug, PartialEq)]
struct PaneLayoutOverride {
    window: WindowId,
    layout: LayoutNode,
    generation: u64,
}

impl PaneLayoutOverride {
    fn still_predicts(&self, active_window: Option<&WindowSnapshot>, generation: u64) -> bool {
        self.generation == generation
            && active_window
                .is_some_and(|window| window.id == self.window && window.zoomed_pane.is_none())
    }
}

struct PaneContent {
    view: AnyView,
    cached: bool,
    inactive_style: PaneInactiveStyle,
    covers_frame: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PaneInactiveStyle {
    Surface,
    Content,
}

impl PaneContent {
    fn element(&self) -> AnyElement {
        if self.cached {
            self.view
                .clone()
                .cached(StyleRefinement::default().size_full())
                .into_any_element()
        } else {
            self.view.clone().into_any_element()
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct TerminalHeaderState {
    active: bool,
    title: String,
    can_drag: bool,
    floats: bool,
    lens: Option<PaneLens>,
    radii: Corners<Pixels>,
    opacity: f32,
}

struct TerminalHeaderView {
    pane: PaneId,
    app: WeakEntity<AppView>,
    mux: Entity<MuxClient>,
    canvas: Rc<Cell<Bounds<Pixels>>>,
    state: Option<TerminalHeaderState>,
}

impl TerminalHeaderView {
    fn sync(&mut self, state: TerminalHeaderState, cx: &mut Context<Self>) {
        if self.state.as_ref() != Some(&state) {
            self.state = Some(state);
            cx.notify();
        }
    }
}

impl Render for TerminalHeaderView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(state) = self.state.clone() else {
            return div().w_full().into_any_element();
        };
        let pane = self.pane;
        let mux = self.mux.clone();
        let canvas = Rc::clone(&self.canvas);
        let drag_app = self.app.clone();
        let click_app = self.app.clone();
        let lens_app = self.app.clone();
        let title = state.title.clone();
        let header = terminal_pane_header(
            state.active,
            title.clone(),
            PaneDragSource::new(pane, title, state.can_drag, move |drag, _, cx| {
                if let Some(app) = drag_app.upgrade() {
                    app.update(cx, |view, cx| view.on_pane_drag_start(*drag, cx));
                }
            }),
            state.floats,
            state.lens,
            move |action, window, cx| {
                if matches!(action, TerminalPaneAction::Lens) {
                    if let Some(app) = lens_app.upgrade() {
                        app.update(cx, |view, cx| view.toggle_lens(pane, window, cx));
                    }
                    return;
                }
                let rows = pip_fill_rows(mux.read(cx), pane, canvas.get().size, window, cx);
                for command in action.commands(pane, rows) {
                    mux.read(cx).execute(command);
                }
            },
            cx,
        )
        .on_click(move |_, window, cx| {
            let Some(app) = click_app.upgrade() else {
                return;
            };
            app.update(cx, |view, cx| {
                view.mux.read(cx).execute(pane_select_command(pane));
                if let Some((_, focus)) = view.pane_focus_target(pane, cx) {
                    focus.focus(window, cx);
                }
            });
        });
        let header = if state.floats {
            header
        } else {
            window_drag_handle(("pane-header-window-drag", pane.0), header, window, cx)
        };
        pane_header_frame(header, state.radii, state.opacity).into_any_element()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct AppRevision {
    snapshot_generation: u64,
    attached_host: HostId,
    attached: Option<SessionId>,
    focused_window: Option<WindowId>,
    error: Option<Arc<str>>,
    command_output_pane: Option<PaneId>,
    menu: u64,
    confirm: u64,
    command_prompt: u64,
    choose_tree: u64,
    choose_buffer: u64,
    display_panes: u64,
    prefix_armed: bool,
    prefix_cancelled_request: Option<u64>,
    sidebar_focus: u64,
    bell: u64,
    pending_commands: u64,
}

impl AppRevision {
    fn for_mux(mux: &MuxClient) -> Self {
        let snapshot = mux.snapshot();
        let attached = mux.attached_session();
        Self {
            snapshot_generation: snapshot.generation,
            attached_host: mux.attached_host(),
            attached,
            focused_window: attached_focused_window(&snapshot, attached),
            error: mux.error(),
            command_output_pane: mux.command_output().map(|output| output.pane),
            menu: mux.menu_revision(),
            confirm: mux.confirm_revision(),
            command_prompt: mux.command_prompt_revision(),
            choose_tree: mux.choose_tree_revision(),
            choose_buffer: mux.choose_buffer_revision(),
            display_panes: mux.display_panes_revision(),
            prefix_armed: mux.prefix_armed(),
            prefix_cancelled_request: mux.prefix_cancelled_request(),
            sidebar_focus: mux.sidebar_focus_revision(),
            bell: mux.bell_revision(),
            pending_commands: mux.pending_commands_revision(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OverlayKind {
    CommandPalette,
    ChooseBuffer,
    ChooseTree,
    DisplayPanes,
    CommandOutput(PaneId),
    Menu,
    Confirm,
}

/// A folder as the palette's `@` mode types it: under `home` as `~/`, with
/// its spaces escaped and a `/` at its end, or the home folder when there is
/// none.
fn pane_in_session(snapshot: &MuxSnapshot, session: Option<SessionId>, pane: PaneId) -> bool {
    snapshot
        .sessions
        .iter()
        .filter(|candidate| Some(candidate.id) == session)
        .flat_map(|candidate| &candidate.windows)
        .any(|window| window.panes.contains_key(&pane))
}

#[derive(PartialEq)]
struct SynchronizeSignature {
    revision: AppRevision,
    sidebar_focused: bool,
    route: WorkspaceRoute,
}

// Each focus debt is owed by a different thing, so folding them into one enum
// would hide which one is outstanding.
#[allow(clippy::struct_excessive_bools)]
pub struct AppView {
    controller: Entity<BrowserController>,
    agent_controller: Entity<AgentController>,
    mux: Entity<MuxClient>,
    sidebar: Entity<WorkspaceSidebar>,
    new_session: Entity<NewSessionView>,
    pickers: BTreeMap<PaneId, Entity<PanePickerView>>,
    terminals: BTreeMap<PaneId, Entity<TerminalView>>,
    terminal_headers: BTreeMap<PaneId, Entity<TerminalHeaderView>>,
    browsers: BTreeMap<PaneId, Entity<BrowserView>>,
    inline_browser: Entity<crate::browser::inline::InlineBrowser>,
    agents: BTreeMap<PaneId, Entity<AgentView>>,
    lenses: BTreeSet<PaneId>,
    pending_lenses: BTreeSet<PaneId>,
    editors: BTreeMap<PaneId, Entity<EditorView>>,
    command_output: Option<(PaneId, Entity<TerminalView>)>,
    menu: Option<Entity<MenuView>>,
    confirm: Option<Entity<ConfirmView>>,
    choose_tree: Option<Entity<ChooseTreeView>>,
    choose_buffer: Option<Entity<ChooseBufferView>>,
    display_panes: Option<Entity<DisplayPanesView>>,
    command_palette: Option<Entity<CommandPaletteView>>,
    local_palette_prompt_revision: Option<u64>,
    local_palette_chooser_revision: Option<u64>,
    pending_palette_chooser_close: Option<u64>,
    gui_previewing: bool,
    pane_indicators: BTreeMap<PaneId, PaneIndicator>,
    focused_pane: Option<(PaneId, EntityId)>,
    focused_overlay: Option<OverlayKind>,
    sidebar_focus_owed: bool,
    pane_focus_owed: bool,
    sidebar_focus_revision: u64,
    bell_revision: u64,
    empty_workspace_visible: bool,
    empty_workspace_focus_pending: bool,
    window_handle: AnyWindowHandle,
    split_drag: Option<SplitDragState>,
    float_drag: Option<FloatDragState>,
    pip_throw: PipThrow,
    pip_landing: Option<PipLandingState>,
    pip_stage: PipStage,
    pane_motion: PaneMotion,
    terminal_resize_suppressed: Rc<Cell<bool>>,
    terminal_resizes_held: bool,
    snapshot_revision: u64,
    pane_drag: Option<PaneDragState>,
    pane_drop_preview: Rc<Cell<DropPreviewFrame>>,
    pane_layout_override: Option<PaneLayoutOverride>,
    pane_canvas_bounds: Rc<Cell<Bounds<Pixels>>>,
    pane_arrivals: PaneArrivals,
    input_router: InputRouter,
    dialog_prefix_cancel_sent: bool,
    dialog_prefix_cancel_pending: Option<u64>,
    synchronized_signature: Option<SynchronizeSignature>,
    which_key: Option<WhichKeySheet>,
    which_key_timer: Option<Task<()>>,
    which_key_generation: u64,
    window_shortcut_timer: Option<Task<()>>,
}

impl AppView {
    pub fn new(
        controller: Entity<BrowserController>,
        agent_controller: Entity<AgentController>,
        mux: Entity<MuxClient>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        agent_controller.update(cx, |controller, _| controller.attach_mux(mux.clone()));
        if window.is_window_active() {
            mux.update(cx, |mux, _| {
                mux.set_client_window_focused(true);
            });
        }
        let input_router = crate::keymap::input_router(cx);
        cx.observe_global::<crate::keymap::ChromeState>(|view, cx| {
            view.input_router = crate::keymap::input_router(cx);
        })
        .detach();
        let mut observed_revision = AppRevision::for_mux(mux.read(cx));
        let mut observed_snapshot = mux.read(cx).snapshot();
        let mut observed_palette_fleet: Vec<(
            HostId,
            String,
            crate::mux::hosts::HostState,
            Option<u64>,
        )> = Vec::new();
        let mut observed_palette_agents = Vec::new();
        cx.observe_in(&mux, window, move |view, mux, window, cx| {
            view.drain_gui_requests(cx);
            let snapshot = mux.read(cx).snapshot();
            let snapshot_arrived = !Arc::ptr_eq(&snapshot, &observed_snapshot);
            if snapshot_arrived {
                let panes = snapshot
                    .sessions
                    .iter()
                    .flat_map(|session| &session.windows)
                    .flat_map(|window| window.panes.keys())
                    .copied()
                    .collect::<BTreeSet<_>>();
                for pane in observed_snapshot
                    .sessions
                    .iter()
                    .flat_map(|session| &session.windows)
                    .flat_map(|window| window.panes.keys())
                    .filter(|pane| !panes.contains(pane))
                {
                    view.input_router.event(InputEvent::PaneRemoved(*pane));
                }
                observed_snapshot = snapshot;
                view.snapshot_revision = view.snapshot_revision.wrapping_add(1).max(1);
            }
            let revision = AppRevision::for_mux(mux.read(cx));
            let revision_changed = revision != observed_revision;
            if revision_changed {
                if observed_revision.attached.is_some() && revision.attached.is_none() {
                    view.input_router.event(InputEvent::Detached);
                }
                observed_revision = revision;
            }
            if let Some(palette) = &view.command_palette {
                let fleet_changed = !mux
                    .read(cx)
                    .fleet_hosts()
                    .map(|(id, name, state, snapshot)| {
                        (
                            id,
                            name,
                            state,
                            snapshot.map(|snapshot| snapshot.generation),
                        )
                    })
                    .eq(observed_palette_fleet
                        .iter()
                        .map(|(id, name, state, generation)| {
                            (*id, name.as_str(), state, *generation)
                        }));
                let current_snapshot = mux.read(cx).snapshot();
                let agent_statuses = || {
                    current_snapshot
                        .sessions
                        .iter()
                        .flat_map(|session| &session.windows)
                        .flat_map(|window| window.panes.values())
                        .filter(|pane| matches!(pane.kind, PaneKindSnapshot::Terminal))
                        .map(|pane| (pane.id, mux.read(cx).agent_attention_status(pane.id)))
                };
                let agents_changed = !agent_statuses().eq(observed_palette_agents.iter().copied());
                if fleet_changed || agents_changed || revision_changed || snapshot_arrived {
                    observed_palette_agents = agent_statuses().collect();
                    observed_palette_fleet = mux
                        .read(cx)
                        .fleet_hosts()
                        .map(|(id, name, state, snapshot)| {
                            (
                                id,
                                name.to_owned(),
                                state.clone(),
                                snapshot.map(|snapshot| snapshot.generation),
                            )
                        })
                        .collect();
                    palette.update(cx, |palette, cx| {
                        palette.refresh(window, cx);
                        cx.notify();
                    });
                }
            }
            if view
                .float_drag
                .and_then(|drag| drag.request)
                .is_some_and(|request| mux.read(cx).command_settled(request))
            {
                view.float_drag = None;
                cx.notify();
            }
            if revision_changed || snapshot_arrived {
                view.synchronize_panes(window, cx);
                cx.notify();
            }
            view.drain_agent_events(cx);
        })
        .detach();
        let window_handle = window.window_handle();
        let keystroke_listener = cx.listener(Self::intercept_keystroke);
        cx.intercept_keystrokes(keystroke_listener).detach();
        cx.observe_window_activation(window, |view, window, cx| {
            let window_active = window.is_window_active();
            view.mux.update(cx, |mux, _| {
                mux.set_client_window_focused(window_active);
            });
            view.input_router.event(InputEvent::WindowDeactivated);
            if !window_active {
                view.hide_window_shortcuts(cx);
            }
            if !window_active
                && view
                    .float_drag
                    .is_some_and(|drag| drag.committed_snapshot_revision.is_none())
            {
                view.float_drag = None;
                cx.notify();
            }
            if !window_active && view.pane_drag.take().is_some() {
                cx.stop_active_drag(window);
                cx.notify();
            }
        })
        .detach();
        cx.subscribe_in(
            &mux,
            window,
            |_, _, event: &ClientNotification, window, cx| {
                let notification = match event.kind {
                    ClientMessageKind::Info => Notification::info(event.text.clone()),
                    ClientMessageKind::Success => Notification::success(event.text.clone()),
                    ClientMessageKind::Warning => Notification::warning(event.text.clone()),
                    ClientMessageKind::Error => Notification::error(event.text.clone()),
                };
                let notification = match event.duration_ms {
                    Some(0) => notification.autohide(false),
                    Some(duration_ms) => {
                        notification.autohide_after(Duration::from_millis(u64::from(duration_ms)))
                    }
                    None => notification,
                };
                let notification = match event.message_id {
                    Some(message_id) => notification.key(timed_message_key(message_id)),
                    None => notification,
                };
                window.push_notification(notification, cx);
            },
        )
        .detach();
        cx.subscribe_in(&mux, window, |view, _, _: &KeyTableChanged, window, cx| {
            view.key_table_changed(window, cx);
        })
        .detach();
        cx.subscribe_in(
            &mux,
            window,
            |_, _, event: &ClientNotificationCleared, window, cx| {
                window.dismiss_notification(&timed_message_key(event.message_id), cx);
            },
        )
        .detach();
        cx.subscribe_in(
            &mux,
            window,
            |_, _, event: &AttachmentPreviewRequest, window, cx| {
                open_attachment_preview(Arc::clone(&event.image), window, cx);
            },
        )
        .detach();
        cx.subscribe_in(
            &mux,
            window,
            |_, mux, event: &SshPromptRequest, window, cx| {
                super::ssh_prompt::open(mux, event, window, cx);
            },
        )
        .detach();
        cx.subscribe_in(
            &mux,
            window,
            |_, mux, _: &RemoteInstallOffer, window, cx| {
                super::remote_install::open(mux, window, cx);
            },
        )
        .detach();
        cx.subscribe_in(
            &mux,
            window,
            |view, _, event: &PathPickerUpdate, window, cx| {
                view.on_path_picker_update(event, window, cx);
            },
        )
        .detach();
        cx.subscribe(
            &controller,
            |view, controller, event: &ControllerEvent, cx| {
                if let ControllerEvent::Browser { pane, tab, event } = event
                    && view.browsers.contains_key(pane)
                    && controller.read(cx).active_tab(*pane) == Some(*tab)
                    && let Some(command) = browser_metadata_command(*pane, event)
                {
                    view.mux.read(cx).execute(command);
                }
            },
        )
        .detach();
        let pages = mux.clone();
        cx.spawn(async move |_, cx| {
            let requests = zz_browser::artifact_requests();
            while let Ok(request) = requests.recv().await {
                pages.update(cx, |mux, _| mux.fetch_artifact(request));
            }
        })
        .detach();
        let inline_browser =
            cx.new(|cx| crate::browser::inline::InlineBrowser::new(controller.clone(), window, cx));
        cx.set_global(zz_ui::agent::AgentHtmlLive {
            live: Rc::new({
                let inline = inline_browser.clone();
                move |block, cx| {
                    inline
                        .read(cx)
                        .is_live(block)
                        .then(|| inline.clone().into_any_element())
                }
            }),
            activate: Rc::new({
                let inline = inline_browser.clone();
                move |live, window, cx| {
                    inline.update(cx, |inline, cx| {
                        inline.activate(
                            live.block,
                            &live.html,
                            live.size,
                            live.owner.map(PaneId),
                            window,
                            cx,
                        );
                    });
                }
            }),
        });
        let previews = controller.clone();
        cx.set_global(zz_ui::agent::AgentHtmlPreviewer(Rc::new(
            move |html, size, cx| {
                let width = f32::from(size.width).max(1.0).round() as u32;
                previews.update(cx, |controller, cx| {
                    controller.render_html_preview(&html, width, size.scale, cx)
                })
            },
        )));
        let html_mux = mux.clone();
        cx.set_global(zz_ui::agent::AgentHtmlOpener(Rc::new(
            move |html, _, cx| {
                html_mux.read(cx).execute(CommandInvocation::new(
                    "html",
                    ["-".to_owned(), "--".to_owned(), html],
                ));
            },
        )));
        let replies = mux.clone();
        let inline = inline_browser.clone();
        cx.spawn(async move |_, cx| {
            let sent = zz_browser::artifact_replies();
            while let Ok(reply) = sent.recv().await {
                if !zz_browser::is_local_page(&reply.token) {
                    replies.update(cx, |mux, _| mux.send_artifact_reply(&reply));
                    continue;
                }
                let Some(pane) = inline.update(cx, |inline, _| {
                    inline.take_reply(&reply.token, std::time::Instant::now())
                }) else {
                    continue;
                };
                replies.update(cx, |mux, _| {
                    mux.execute(CommandInvocation::new(
                        "agent-send",
                        [
                            "-t".to_owned(),
                            pane.to_string(),
                            "--".to_owned(),
                            reply.text,
                        ],
                    ));
                });
            }
        })
        .detach();
        let sidebar = cx.new(|cx| WorkspaceSidebar::new(mux.clone(), cx));
        if cx.try_global::<crate::tray::DesktopTray>().is_some() {
            cx.global_mut::<crate::tray::DesktopTray>().sidebar = Some(sidebar.downgrade());
        }
        cx.subscribe_in(
            &sidebar,
            window,
            |view, _, _: &SidebarReleaseFocus, window, cx| {
                view.sidebar_focus_owed = false;
                view.focus_active_pane(window, cx);
            },
        )
        .detach();
        cx.subscribe(&sidebar, |_, _, _: &SidebarModeChanged, cx| cx.notify())
            .detach();
        cx.subscribe(&sidebar, |view, _, _: &SidebarRouteChanged, cx| {
            view.input_router.event(InputEvent::WindowDeactivated);
            cx.notify();
        })
        .detach();
        let sidebar_focus = sidebar.read(cx).focus_handle();
        cx.on_focus(&sidebar_focus, window, |_, _, _| {
            log::info!(target: "zz::diagnostics::input", "sidebar_focus_gained");
        })
        .detach();
        cx.on_blur(&sidebar_focus, window, |_, _, _| {
            log::info!(target: "zz::diagnostics::input", "sidebar_focus_lost");
        })
        .detach();
        let new_session = cx.new(|cx| NewSessionView::new(mux.clone(), cx));
        Self {
            controller,
            agent_controller,
            mux,
            sidebar,
            new_session,
            pickers: BTreeMap::new(),
            terminals: BTreeMap::new(),
            terminal_headers: BTreeMap::new(),
            browsers: BTreeMap::new(),
            inline_browser,
            agents: BTreeMap::new(),
            lenses: BTreeSet::new(),
            pending_lenses: BTreeSet::new(),
            editors: BTreeMap::new(),
            command_output: None,
            menu: None,
            confirm: None,
            choose_tree: None,
            choose_buffer: None,
            display_panes: None,
            command_palette: None,
            local_palette_prompt_revision: None,
            local_palette_chooser_revision: None,
            pending_palette_chooser_close: None,
            gui_previewing: false,
            pane_indicators: BTreeMap::new(),
            focused_pane: None,
            focused_overlay: None,
            sidebar_focus_owed: false,
            pane_focus_owed: false,
            sidebar_focus_revision: 0,
            bell_revision: 0,
            empty_workspace_visible: false,
            empty_workspace_focus_pending: false,
            window_handle,
            split_drag: None,
            float_drag: None,
            pip_throw: PipThrow::default(),
            pip_landing: None,
            pip_stage: PipStage::default(),
            pane_motion: PaneMotion::default(),
            terminal_resize_suppressed: Rc::new(Cell::new(false)),
            terminal_resizes_held: false,
            snapshot_revision: 0,
            pane_drag: None,
            pane_drop_preview: Rc::new(Cell::new(DropPreviewFrame::default())),
            pane_layout_override: None,
            pane_canvas_bounds: Rc::new(Cell::new(Bounds::default())),
            pane_arrivals: PaneArrivals::default(),
            input_router,
            dialog_prefix_cancel_sent: false,
            dialog_prefix_cancel_pending: None,
            synchronized_signature: None,
            which_key: None,
            which_key_timer: None,
            which_key_generation: 0,
            window_shortcut_timer: None,
        }
    }

    pub(crate) fn on_window_shortcut_modifiers(
        &mut self,
        modifiers: Modifiers,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let held = window_shortcut_modifiers(cx) == Some(modifiers);
        if !held {
            self.hide_window_shortcuts(cx);
            return;
        }
        if self.window_shortcut_timer.is_some() {
            return;
        }
        self.window_shortcut_timer = Some(cx.spawn_in(window, async move |view, cx| {
            cx.background_executor()
                .timer(WINDOW_SHORTCUT_HINT_DELAY)
                .await;
            view.update(cx, |view, cx| {
                view.sidebar
                    .update(cx, |sidebar, cx| sidebar.show_window_shortcuts(true, cx));
            })
            .ok();
        }));
    }

    fn hide_window_shortcuts(&mut self, cx: &mut Context<Self>) {
        self.window_shortcut_timer = None;
        self.sidebar
            .update(cx, |sidebar, cx| sidebar.show_window_shortcuts(false, cx));
    }

    fn intercept_keystroke(
        &mut self,
        event: &ala::KeystrokeEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if window.window_handle() != self.window_handle
            || matches!(
                event.keystroke.key.as_str(),
                "shift" | "control" | "alt" | "platform" | "function"
            )
        {
            return;
        }
        if !(event.is_held && self.mux.read(cx).prefix_armed()) {
            self.hide_which_key(cx);
        }
        if !self.sidebar.read(cx).window_shortcuts_shown() {
            self.window_shortcut_timer = None;
        }
        if self.reconcile_dialog_prefix(window, cx) {
            return;
        }
        let keystroke = &event.keystroke;
        let overlay_open = self.overlay_open(cx);
        if !overlay_open
            && keystroke.key == "escape"
            && self
                .pane_drag
                .as_ref()
                .is_some_and(|drag| !drag.requires_prefix)
        {
            self.pane_drag = None;
            cx.stop_active_drag(window);
            cx.notify();
            cx.stop_propagation();
            return;
        }
        if !overlay_open
            && self.dialog_prefix_cancel_pending.is_some()
            && !keystroke.modifiers.platform
            && !keystroke.modifiers.function
        {
            cx.stop_propagation();
            return;
        }
        let input = terminal_key_input(
            keystroke,
            if event.is_held {
                TerminalKeyAction::Repeat
            } else {
                TerminalKeyAction::Press
            },
        );
        let (prefix, active) = {
            let mux = self.mux.read(cx);
            let active = mux.active_pane().and_then(|id| {
                let snapshot = mux.snapshot();
                snapshot
                    .sessions
                    .iter()
                    .filter(|session| Some(session.id) == mux.attached_session())
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
            (
                PrefixView {
                    armed: mux.prefix_armed(),
                    claimed: !overlay_open
                        && !keystroke.modifiers.platform
                        && !keystroke.modifiers.function
                        && mux.claims_prefix_input(&input),
                },
                active,
            )
        };
        let armed = prefix.armed;
        if prefix.claimed && armed && keystroke.key == "escape" && self.pane_drag.take().is_some() {
            cx.stop_active_drag(window);
            cx.notify();
        }
        if active != self.input_router.active_pane() {
            if let Some((pane, kind)) = active {
                self.input_router
                    .event(InputEvent::SetActivePane(pane, kind));
            } else {
                self.input_router.event(InputEvent::Detached);
            }
        }
        self.input_router.set_capture(if overlay_open {
            None
        } else {
            self.mux.read(cx).modal_capture()
        });
        let disposition = self.input_router.key(&input, prefix);
        let effects = self.input_router.drain_effects();
        if prefix.claimed && disposition == Disposition::Native && active.is_none() {
            log::warn!(
                target: "zz::diagnostics::input",
                "prefix_key_dropped keystroke={keystroke} armed={armed} reason=no_active_pane"
            );
        }
        if prefix.claimed
            && event.is_held
            && effects.is_empty()
            && let Some((pane, _)) = active
        {
            log::debug!(
                target: "zz::diagnostics::input",
                "prefix_key_autorepeat_swallowed keystroke={keystroke} armed={armed} pane={pane}"
            );
        }
        for effect in effects {
            match effect {
                Effect::Chrome(action) => self.run_chrome_action(action, window, cx),
                Effect::ForwardKey { pane, input } => {
                    if armed
                        && which_key::delay(config::resolved_config(cx).which_key_delay.value)
                            .is_some()
                        && is_all_keys_input(self.mux.read(cx).prefix_bindings(), &input)
                    {
                        self.input_router.suppress_release(
                            input
                                .unshifted_codepoint
                                .map_or(input.key, zz_terminal::KeyCode::Character),
                        );
                        self.which_key = WhichKeySheet::build(self.mux.read(cx), "prefix", true);
                        cx.notify();
                    } else if armed
                        && cx
                            .try_global::<config::AppConfig>()
                            .is_some_and(|config| config.picker_focus_sidebar.value)
                        && is_sidebar_picker_input(self.mux.read(cx).prefix_bindings(), &input)
                    {
                        self.input_router.suppress_release(
                            input
                                .unshifted_codepoint
                                .map_or(input.key, zz_terminal::KeyCode::Character),
                        );
                        self.mux.update(cx, |mux, _| {
                            if mux.send_prefix_cancel().is_some() {
                                mux.execute(CommandInvocation::new(
                                    "focus-sidebar",
                                    [] as [&str; 0],
                                ));
                            }
                        });
                    } else if armed
                        && let Some(action) = self.prefix_chrome_action(keystroke, &input, cx)
                    {
                        self.input_router.suppress_release(
                            input
                                .unshifted_codepoint
                                .map_or(input.key, zz_terminal::KeyCode::Character),
                        );
                        self.mux.update(cx, |mux, _| mux.send_prefix_cancel());
                        self.run_chrome_action(action, window, cx);
                    } else {
                        log::info!(
                            target: "zz::diagnostics::input",
                            "prefix_key_forwarded keystroke={keystroke} armed={armed} pane={pane}"
                        );
                        self.mux.read(cx).send_input(InputMessage::Key {
                            pane,
                            input,
                            text_follows: false,
                        });
                    }
                }
                Effect::RequestFocus(_) => {}
            }
        }
        if disposition == Disposition::Consumed {
            cx.stop_propagation();
        }
    }

    fn prefix_chrome_action(
        &self,
        keystroke: &Keystroke,
        input: &zz_terminal::KeyInput,
        cx: &App,
    ) -> Option<ChromeAction> {
        let mux = self.mux.read(cx);
        if mux.key_table() != Some(("prefix", false)) || binds_input(mux.prefix_bindings(), input) {
            return None;
        }
        crate::keymap::resolve(cx, zz_client::PREFIX_TABLE, keystroke)
    }

    fn run_chrome_action(
        &mut self,
        action: ChromeAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let action: Box<dyn ala::Action> = match action {
            ChromeAction::OpenCommandPalette => {
                self.open_command_palette(None, window, cx);
                return;
            }
            ChromeAction::SelectWindow(position) => {
                if self
                    .sidebar
                    .update(cx, |sidebar, cx| sidebar.open_window_shortcut(position, cx))
                {
                    self.focus_active_pane(window, cx);
                }
                return;
            }
            ChromeAction::NewSession => Box::new(crate::menus::NewSession),
            ChromeAction::NewWindow => Box::new(crate::menus::NewWindow),
            ChromeAction::SplitRight => Box::new(crate::menus::SplitRight),
            ChromeAction::SplitDown => Box::new(crate::menus::SplitDown),
            ChromeAction::Detach => Box::new(crate::menus::Detach),
            ChromeAction::ToggleSidebar => Box::new(crate::menus::ToggleSidebar),
            ChromeAction::OpenSettings => Box::new(config::settings::OpenSettings),
            _ => return,
        };
        window.dispatch_action(action, cx);
    }

    fn overlay_open(&self, cx: &App) -> bool {
        self.menu.is_some()
            || self.confirm.is_some()
            || self.command_palette.as_ref().is_some_and(|palette| {
                palette.read(cx).is_local() || palette.read(cx).is_window_chooser()
            })
            || self.sidebar.read(cx).route() == WorkspaceRoute::Settings
    }

    fn which_key_blocked(&self, window: &Window, cx: &App) -> bool {
        self.overlay_open(cx)
            || self.visible_overlay(cx).is_some()
            || self.pane_drag.is_some()
            || cx.has_active_drag()
            || window
                .root::<zz_ui::Root>()
                .flatten()
                .is_some_and(|root| root.read(cx).has_active_dialog())
    }

    fn hide_which_key(&mut self, cx: &mut Context<Self>) {
        self.which_key_timer = None;
        self.which_key_generation = self.which_key_generation.wrapping_add(1);
        if self.which_key.take().is_some() {
            cx.notify();
        }
    }

    fn key_table_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.hide_which_key(cx);
        let Some((table, false)) = self.mux.read(cx).key_table() else {
            return;
        };
        let table = table.to_owned();
        let Some(delay) = which_key::delay(config::resolved_config(cx).which_key_delay.value)
        else {
            return;
        };
        let generation = self.which_key_generation;
        self.which_key_timer = Some(cx.spawn_in(window, async move |view, cx| {
            cx.background_executor().timer(delay).await;
            view.update_in(cx, |view, window, cx| {
                view.show_which_key(generation, &table, window, cx);
            })
            .ok();
        }));
    }

    fn show_which_key(
        &mut self,
        generation: u64,
        table: &str,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        if generation != self.which_key_generation {
            return;
        }
        self.which_key_timer = None;
        if self.mux.read(cx).key_table() != Some((table, false))
            || self.which_key_blocked(window, cx)
        {
            return;
        }
        self.which_key = WhichKeySheet::build(self.mux.read(cx), table, false);
        cx.notify();
    }

    fn reconcile_dialog_prefix(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let active = window
            .root::<zz_ui::Root>()
            .flatten()
            .is_some_and(|root| root.read(cx).has_active_dialog());
        let acknowledged = self.mux.read(cx).prefix_cancelled_request();
        if self
            .dialog_prefix_cancel_pending
            .is_some_and(|pending| acknowledged.is_some_and(|acknowledged| acknowledged >= pending))
        {
            self.dialog_prefix_cancel_pending = None;
        }
        if !active {
            self.dialog_prefix_cancel_sent = false;
        } else if !self.dialog_prefix_cancel_sent
            && let Some(request_id) = self.mux.update(cx, |mux, _| mux.send_prefix_cancel())
        {
            self.dialog_prefix_cancel_pending = Some(request_id);
            self.dialog_prefix_cancel_sent = true;
        }
        active
    }

    /// Forward a claimed key's release to the daemon and stop it reaching the
    /// widget that never saw the press.
    pub fn on_claim_key_up(&mut self, event: &KeyUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        let input = terminal_key_input(&event.keystroke, TerminalKeyAction::Release);
        let disposition = self.input_router.key(&input, PrefixView::default());
        for effect in self.input_router.drain_effects() {
            if let Effect::ForwardKey { pane, input } = effect {
                self.mux.read(cx).send_input(InputMessage::Key {
                    pane,
                    input,
                    text_follows: false,
                });
            }
        }
        if disposition == Disposition::Consumed {
            cx.stop_propagation();
        }
    }

    fn active_pane(&self, cx: &App) -> Option<PaneId> {
        self.mux.read(cx).active_pane()
    }

    fn open_command_palette(
        &mut self,
        mode: Option<PaletteMode>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.show_command_palette(
            move |backend, window, cx| CommandPaletteView::new_unified(backend, mode, window, cx),
            window,
            cx,
        );
    }

    /// Open the palette `build` makes, closing whatever other palette or
    /// prompt is up.
    fn show_command_palette(
        &mut self,
        build: impl FnOnce(
            Rc<dyn zz_ui::command::PaletteBackend>,
            &mut Window,
            &mut Context<CommandPaletteView>,
        ) -> CommandPaletteView,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.menu.is_some() || self.confirm.is_some() {
            return;
        }
        let mux = self.mux.clone();
        mux.update(cx, |mux, _| mux.send_prefix_cancel());
        self.local_palette_prompt_revision = Some(mux.read(cx).command_prompt_revision());
        self.local_palette_chooser_revision = Some(mux.read(cx).choose_tree_revision());
        if mux.read(cx).command_prompt().is_some() {
            mux.read(cx).send_input(InputMessage::CommandPrompt {
                action: zz_protocol::CommandPromptAction::Close,
            });
        }
        if mux.read(cx).choose_tree().is_some() {
            mux.read(cx).send_input(InputMessage::ChooseTree {
                action: zz_protocol::ChooseTreeAction::Close,
            });
        }
        let offers = crate::command::gui::offers(&self.sidebar, cx);
        let backend = palette_backend_with(mux, offers);
        let palette = cx.new(|cx| build(backend, window, cx));
        self.observe_command_palette(&palette, window, cx);
        palette.read(cx).focus(cx).focus(window, cx);
        self.command_palette = Some(palette);
        self.focused_overlay = Some(OverlayKind::CommandPalette);
        self.synchronized_signature = None;
        cx.notify();
    }

    fn on_gui_event(&mut self, event: &GuiEvent, cx: &mut Context<Self>) {
        match event {
            GuiEvent::Preview { action, value } => {
                self.gui_previewing = true;
                crate::command::gui::preview(action, value, cx);
            }
            GuiEvent::Revert => {
                if std::mem::take(&mut self.gui_previewing) {
                    crate::command::gui::revert(cx);
                }
            }
            GuiEvent::Apply { action, value } => {
                self.gui_previewing = false;
                crate::command::gui::apply(action, value, &self.sidebar, cx);
            }
        }
    }

    fn observe_command_palette(
        &self,
        palette: &Entity<CommandPaletteView>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.subscribe_in(palette, window, |view, palette, event, window, cx| {
            if let CommandPaletteEvent::Gui(event) = event {
                view.on_gui_event(event, cx);
                return;
            }
            if view.command_palette.as_ref() != Some(palette) || palette.read(cx).is_finished() {
                return;
            }
            match event {
                CommandPaletteEvent::ReturnToDefault => {
                    view.pending_palette_chooser_close =
                        Some(view.mux.read(cx).choose_tree_closed_revision());
                    view.local_palette_prompt_revision =
                        Some(view.mux.read(cx).command_prompt_revision());
                    view.local_palette_chooser_revision = None;
                    palette.update(cx, |palette, cx| palette.return_to_default(window, cx));
                    view.synchronized_signature = None;
                    cx.notify();
                }
                CommandPaletteEvent::Gui(_) => {}
            }
        })
        .detach();
        cx.observe_release_in(palette, window, |view, _, _, cx| {
            if std::mem::take(&mut view.gui_previewing) {
                crate::command::gui::revert(cx);
            }
        })
        .detach();
        cx.observe_in(palette, window, move |view, palette, window, cx| {
            if view.command_palette.as_ref() != Some(&palette)
                || !palette.read(cx).is_local()
                || !palette.read(cx).is_finished()
            {
                return;
            }
            view.command_palette = None;
            view.local_palette_prompt_revision = None;
            view.local_palette_chooser_revision = None;
            view.focused_overlay = None;
            view.synchronized_signature = None;
            if view.sidebar.read(cx).route() == WorkspaceRoute::Settings {
                if let Some(settings) = view.sidebar.read(cx).settings_view() {
                    settings.read(cx).focus().focus(window, cx);
                }
            } else {
                view.focus_active_pane(window, cx);
            }
            cx.notify();
        })
        .detach();
    }

    fn on_path_picker_update(
        &mut self,
        event: &PathPickerUpdate,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let PathPickerUpdate::Open { pane, start_dir } = event {
            self.open_path_palette(*pane, start_dir.as_deref(), window, cx);
        }
    }

    /// `prefix F`: the palette in `&` mode on `pane`'s folder, or the `-c`
    /// folder, putting what you pick at that pane's cursor.
    fn open_path_palette(
        &mut self,
        pane: PaneId,
        start_dir: Option<&str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.command_palette.is_some() || self.sidebar.read(cx).route() != WorkspaceRoute::App {
            return;
        }
        let (attached, snapshot, local) = {
            let mux = self.mux.read(cx);
            (
                mux.attached_session(),
                mux.snapshot(),
                mux.attached_host() == HostId::LOCAL,
            )
        };
        if !pane_in_session(&snapshot, attached, pane) {
            return;
        }
        let home = local.then(std::env::home_dir).flatten();
        let body = start_dir
            .filter(|dir| dir.starts_with(['/', '~']))
            .map(|dir| {
                zz_ui::files::start_dir(Some(dir), home.as_ref().and_then(|home| home.to_str()))
            })
            .unwrap_or_default();
        self.show_command_palette(
            move |backend, window, cx| {
                CommandPaletteView::new_target(backend, &body, Some(pane), window, cx)
            },
            window,
            cx,
        );
    }

    fn open_folder_palette(
        &mut self,
        picker: &Entity<PanePickerView>,
        start: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.command_palette.is_some() || self.sidebar.read(cx).route() != WorkspaceRoute::App {
            return;
        }
        let host = self.mux.read(cx).attached_host();
        let home = (host == HostId::LOCAL).then(std::env::home_dir).flatten();
        let body =
            zz_ui::files::start_dir(Some(start), home.as_ref().and_then(|home| home.to_str()));
        let picker = picker.downgrade();
        let pick: zz_ui::command::FolderPick = Rc::new(move |picked, folder, window, cx| {
            if HostId::from(picked) != host {
                return;
            }
            let _ = picker.update(cx, |picker, cx| {
                picker.pick_folder(&folder, cx);
                picker.focus().focus(window, cx);
            });
        });
        self.show_command_palette(
            move |backend, window, cx| {
                CommandPaletteView::new_folder_pick(backend, &body, pick, window, cx)
            },
            window,
            cx,
        );
    }

    // Reached from the app shell's ClosePane action.
    pub(crate) fn close_active_pane(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(pane) = self.active_pane(cx) else {
            return false;
        };
        self.mux
            .read(cx)
            .execute(kill_target_command(TreeTarget::Pane(pane)));
        true
    }

    pub(crate) fn prompt_daemon_update(mux: &Entity<MuxClient>, window: &mut Window, cx: &mut App) {
        prompt_stale_daemon(mux, true, window, cx);
    }

    pub fn sidebar(&self) -> Entity<WorkspaceSidebar> {
        self.sidebar.clone()
    }

    pub(crate) fn mux(&self) -> Entity<MuxClient> {
        self.mux.clone()
    }

    fn pane_focus_target(&self, pane: PaneId, cx: &App) -> Option<(EntityId, FocusHandle)> {
        if let Some(picker) = self.pickers.get(&pane) {
            Some((picker.entity_id(), picker.read(cx).focus().clone()))
        } else if let Some(agent) = self.agents.get(&pane) {
            Some((agent.entity_id(), agent.read(cx).focus(cx)))
        } else if let Some(terminal) = self.terminals.get(&pane) {
            Some((terminal.entity_id(), terminal.read(cx).focus()))
        } else if let Some(browser) = self.browsers.get(&pane) {
            Some((browser.entity_id(), browser.read(cx).pane_focus_handle(cx)))
        } else {
            self.editors
                .get(&pane)
                .map(|editor| (editor.entity_id(), editor.read(cx).focus(cx)))
        }
    }

    fn inline_browser_holds(&self, pane: PaneId, window: &Window, cx: &App) -> bool {
        self.inline_browser.read(cx).holds_focus(window)
            && window.context_stack().iter().any(|context| {
                context
                    .get(PANE_KEY_CONTEXT_ID)
                    .map(ala::SharedString::as_ref)
                    == Some(pane.0.to_string().as_str())
            })
    }

    fn audit_pane_focus(&self, phase: &str, window: &Window, cx: &App) {
        let Some((pane, entity)) = self.focused_pane else {
            return;
        };
        let target = self.pane_focus_target(pane, cx);
        let observed = window.focused(cx);
        let contexts = window.context_stack();
        let holds_keyboard = (target.as_ref().is_some_and(|(current, focus)| {
            *current == entity && observed.as_ref() == Some(focus)
        }) && contexts.iter().any(|context| {
            context
                .get(PANE_KEY_CONTEXT_ID)
                .map(ala::SharedString::as_ref)
                == Some(pane.0.to_string().as_str())
        })) || self.inline_browser_holds(pane, window, cx);
        if holds_keyboard {
            log::debug!(
                target: "zz::diagnostics::input",
                "pane_focus_live phase={phase} pane={pane} entity={entity:?}"
            );
            return;
        }
        log::warn!(
            target: "zz::diagnostics::input",
            "pane_focus_lost phase={phase} pane={pane} entity={entity:?} view={:?} wanted={:?} window_focus={observed:?} contexts={contexts:?}",
            target.as_ref().map(|(entity, _)| *entity),
            target.as_ref().map(|(_, focus)| focus),
        );
    }

    fn focus_active_pane(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.sidebar.read(cx).route() == WorkspaceRoute::Settings {
            return;
        }
        if self.empty_workspace_visible {
            let focus = self.new_session.read(cx).focus().clone();
            focus.focus(window, cx);
            self.focused_pane = None;
            return;
        }
        let active_pane = {
            let mux = self.mux.read(cx);
            let snapshot = mux.snapshot();
            snapshot
                .sessions
                .iter()
                .find(|session| Some(session.id) == mux.attached_session())
                .and_then(|session| {
                    let focused_window = snapshot.focused_window_for(session);
                    session
                        .windows
                        .iter()
                        .find(|mux_window| mux_window.id == focused_window)
                })
                .map(|mux_window| mux_window.active_pane)
        };
        let target = active_pane.and_then(|pane| {
            self.pane_focus_target(pane, cx)
                .map(|(entity, focus)| (pane, entity, focus))
        });
        log::info!(
            target: "zz::diagnostics::input",
            "focus_active_pane pane={active_pane:?} focused={}",
            target.is_some()
        );
        self.focused_pane = target.map(|(pane, entity, focus)| {
            focus.focus(window, cx);
            (pane, entity)
        });
    }

    fn visible_overlay(&self, cx: &App) -> Option<(OverlayKind, FocusHandle)> {
        if let Some(menu) = &self.menu {
            Some((OverlayKind::Menu, menu.read(cx).focus().clone()))
        } else if let Some(confirm) = &self.confirm {
            Some((OverlayKind::Confirm, confirm.read(cx).focus().clone()))
        } else if let Some(palette) = &self.command_palette {
            Some((OverlayKind::CommandPalette, palette.read(cx).focus(cx)))
        } else if let Some(chooser) = &self.choose_buffer {
            Some((OverlayKind::ChooseBuffer, chooser.read(cx).focus().clone()))
        } else if let Some(chooser) = &self.choose_tree {
            Some((OverlayKind::ChooseTree, chooser.read(cx).focus().clone()))
        } else if let Some(panes) = &self.display_panes {
            Some((OverlayKind::DisplayPanes, panes.read(cx).focus().clone()))
        } else {
            self.command_output
                .as_ref()
                .map(|(pane, output)| (OverlayKind::CommandOutput(*pane), output.read(cx).focus()))
        }
    }

    fn synchronize_panes(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let revision = AppRevision::for_mux(self.mux.read(cx));
        let (sidebar_focused, route) = {
            let sidebar = self.sidebar.read(cx);
            (sidebar.is_focused(window), sidebar.route())
        };
        if let Some(last) = &self.synchronized_signature
            && last.revision == revision
            && last.sidebar_focused == sidebar_focused
            && last.route == route
        {
            return;
        }
        let previous_route = self.synchronized_signature.as_ref().map(|last| last.route);
        let entering_settings =
            route == WorkspaceRoute::Settings && previous_route != Some(WorkspaceRoute::Settings);
        if route == WorkspaceRoute::App && previous_route == Some(WorkspaceRoute::Settings) {
            self.pane_focus_owed = true;
        }
        if self
            .synchronized_signature
            .as_ref()
            .is_some_and(|last| last.revision.attached_host != revision.attached_host)
        {
            self.pane_focus_owed = true;
        }
        let started = diagnostics::timer(DIAGNOSTIC_TARGET);
        let mux = self.mux.read(cx);
        let attached = mux.attached_session();
        let snapshot = mux.snapshot();
        let command_output = mux.command_output();
        let menu = mux.menu().cloned();
        let confirm = mux.confirm().cloned();
        let command_prompt = mux.command_prompt().cloned();
        let command_prompt_revision = mux.command_prompt_revision();
        let choose_tree = mux.choose_tree().cloned();
        let choose_tree_revision = mux.choose_tree_revision();
        if self
            .pending_palette_chooser_close
            .is_some_and(|closed_revision| closed_revision != mux.choose_tree_closed_revision())
        {
            self.pending_palette_chooser_close = None;
        }
        let choose_buffer = mux.choose_buffer().cloned();
        let choose_buffer_revision = mux.choose_buffer_revision();
        let display_panes = mux.display_panes().cloned();
        let display_panes_revision = mux.display_panes_revision();
        let sidebar_focus_revision = mux.sidebar_focus_revision();
        let sidebar_focus_requested = sidebar_focus_revision != self.sidebar_focus_revision;
        let bell_revision = mux.bell_revision();
        let empty_workspace_visible = !mux.has_hosts()
            || empty_workspace_available(
                snapshot.generation,
                snapshot.sessions.len(),
                mux.error().is_some(),
            );
        self.empty_workspace_focus_pending = empty_workspace_focus_owed(
            empty_workspace_visible,
            self.empty_workspace_visible,
            self.empty_workspace_focus_pending,
        );
        self.empty_workspace_visible = empty_workspace_visible;
        if bell_revision != self.bell_revision {
            self.bell_revision = bell_revision;
            window.request_attention();
            // Only macOS has a one-call system beep.
            #[cfg(target_os = "macos")]
            objc2_app_kit::NSBeep();
        }
        let mut wanted_pickers = BTreeSet::new();
        let mut wanted_terminals = BTreeSet::new();
        let mut wanted_browsers = BTreeSet::new();
        let mut wanted_agents = BTreeSet::new();
        let mut wanted_editors = BTreeSet::new();
        self.promote_pending_lenses(&snapshot, attached);
        let lensed = self.lensed_panes(&snapshot, attached);
        let open = lensed.keys().copied().collect::<BTreeSet<_>>();
        #[cfg(feature = "agent-pane")]
        for pane in self.lenses.difference(&open) {
            self.mux.update(cx, |mux, _| mux.close_agent_lens(*pane));
        }
        self.lenses = open;
        self.agent_controller.update(cx, |controller, cx| {
            controller.retain_panes(&self.lenses, cx);
        });
        let active_window = snapshot
            .sessions
            .iter()
            .find(|session| Some(session.id) == attached)
            .map(|session| snapshot.focused_window_for(session));

        for session in snapshot
            .sessions
            .iter()
            .filter(|session| Some(session.id) == attached)
        {
            for mux_window in &session.windows {
                for (pane, snapshot) in &mux_window.panes {
                    match &snapshot.kind {
                        PaneKindSnapshot::Picker => {
                            wanted_pickers.insert(*pane);
                            if !self.pickers.contains_key(pane) {
                                let mux = self.mux.clone();
                                let pane = *pane;
                                let view = cx.new(|cx| PanePickerView::new(pane, mux, cx));
                                cx.subscribe_in(
                                    &view,
                                    window,
                                    |this, _, chosen: &AgentChosen, _, cx| {
                                        this.on_agent_chosen(chosen, cx);
                                    },
                                )
                                .detach();
                                cx.subscribe_in(
                                    &view,
                                    window,
                                    |this, picker, choose: &ChooseFolder, window, cx| {
                                        this.open_folder_palette(picker, &choose.start, window, cx);
                                    },
                                )
                                .detach();
                                self.pickers.insert(pane, view);
                            }
                        }
                        PaneKindSnapshot::Terminal => {
                            wanted_terminals.insert(*pane);
                            if !self.terminals.contains_key(pane) {
                                let mux = self.mux.clone();
                                let terminal_resize_suppressed =
                                    Rc::clone(&self.terminal_resize_suppressed);
                                let pane = *pane;
                                let view = cx.new(|cx| {
                                    TerminalView::new(
                                        pane,
                                        mux,
                                        terminal_resize_suppressed,
                                        window,
                                        cx,
                                    )
                                });
                                self.terminals.insert(pane, view);
                            }
                            if let Some(provider) = lensed.get(pane).copied() {
                                wanted_agents.insert(*pane);
                                self.ensure_lens_view(*pane, provider, window, cx);
                            }
                            if !self.terminal_headers.contains_key(pane) {
                                let pane = *pane;
                                let app = cx.entity().downgrade();
                                let mux = self.mux.clone();
                                let canvas = Rc::clone(&self.pane_canvas_bounds);
                                let header = cx.new(|_| TerminalHeaderView {
                                    pane,
                                    app,
                                    mux,
                                    canvas,
                                    state: None,
                                });
                                self.terminal_headers.insert(pane, header);
                            }
                        }
                        PaneKindSnapshot::Browser(browser) => {
                            wanted_browsers.insert(*pane);
                            if !self.browsers.contains_key(pane) {
                                let controller = self.controller.clone();
                                let mux = self.mux.clone();
                                let pane = *pane;
                                let descriptor = browser.clone();
                                let resize_held = Rc::clone(&self.terminal_resize_suppressed);
                                let pane_canvas = Rc::clone(&self.pane_canvas_bounds);
                                let view = cx.new(|cx| {
                                    let mut view = BrowserView::new(
                                        pane,
                                        &descriptor,
                                        controller,
                                        mux,
                                        window,
                                        cx,
                                    );
                                    view.share_resize_hold(resize_held);
                                    view.share_pane_canvas(pane_canvas);
                                    view
                                });
                                cx.subscribe(&view, |view, _, drag: &PaneDrag, cx| {
                                    view.on_pane_drag_start(*drag, cx);
                                })
                                .detach();
                                self.browsers.insert(pane, view);
                            } else if let Some(view) = self.browsers.get(pane) {
                                view.update(cx, |view, cx| {
                                    view.synchronize_profile(&browser.profile, cx);
                                    view.synchronize_tabs(
                                        &browser.tabs,
                                        browser.active_tab,
                                        window,
                                        cx,
                                    );
                                });
                            }
                        }
                        PaneKindSnapshot::Editor(editor) => {
                            wanted_editors.insert(*pane);
                            if let Some(view) = self.editors.get(pane) {
                                view.update(cx, |view, cx| {
                                    view.synchronize_descriptor(editor, window, cx);
                                });
                            } else {
                                let mux = self.mux.clone();
                                let pane = *pane;
                                let descriptor = editor.clone();
                                let view = cx
                                    .new(|cx| EditorView::new(pane, &descriptor, mux, window, cx));
                                self.editors.insert(pane, view);
                            }
                        }
                    }
                }
            }
        }

        match command_output {
            Some(output) => {
                let current_matches = self
                    .command_output
                    .as_ref()
                    .is_some_and(|(pane, _)| *pane == output.pane);
                if !current_matches {
                    let mux = self.mux.clone();
                    let pane = output.pane;
                    let view = cx.new(|cx| TerminalView::new_command_output(pane, mux, window, cx));
                    self.command_output = Some((pane, view));
                }
            }
            None => {
                if self.command_output.take().is_some() {
                    self.focused_pane = None;
                }
            }
        }

        match menu {
            Some(state) => {
                if let Some(menu) = &self.menu {
                    menu.update(cx, |menu, cx| menu.synchronize(state, cx));
                } else {
                    let mux = self.mux.clone();
                    self.menu = Some(cx.new(|cx| MenuView::new(mux, state, cx)));
                }
            }
            None => {
                if self.menu.take().is_some() {
                    self.focused_pane = None;
                }
            }
        }

        match confirm {
            Some(state) => {
                if let Some(confirm) = &self.confirm {
                    confirm.update(cx, |confirm, cx| confirm.synchronize(state, cx));
                } else {
                    let mux = self.mux.clone();
                    self.confirm = Some(cx.new(|cx| ConfirmView::new(mux, state, cx)));
                }
            }
            None => {
                if self.confirm.take().is_some() {
                    self.focused_pane = None;
                }
            }
        }

        if let Some((pane, output)) = &self.command_output {
            let commands = self
                .mux
                .update(cx, |mux, _| mux.take_terminal_commands(*pane));
            for command in commands {
                output.update(cx, |output, cx| {
                    output.apply_ui_command(command, window, cx);
                });
            }
        }

        match command_prompt.as_ref() {
            Some(_)
                if self.command_palette.as_ref().is_some_and(|palette| {
                    palette.read(cx).is_local()
                        && self.local_palette_prompt_revision == Some(command_prompt_revision)
                }) =>
            {
                if let Some(palette) = &self.command_palette {
                    palette.update(cx, |palette, cx| palette.refresh(window, cx));
                }
            }
            Some(state) => {
                if self.command_palette.as_ref().is_none_or(|palette| {
                    palette.read(cx).is_local() || palette.read(cx).is_window_chooser()
                }) {
                    self.local_palette_prompt_revision = None;
                    let mux = self.mux.clone();
                    let snapshot = Arc::clone(&snapshot);
                    self.command_palette = Some(cx.new(|cx| {
                        CommandPaletteView::new(
                            palette_backend(mux),
                            state,
                            command_prompt_revision,
                            snapshot,
                            window,
                            cx,
                        )
                    }));
                }
                if let Some(palette) = &self.command_palette {
                    palette.update(cx, |palette, cx| {
                        palette.synchronize(state, command_prompt_revision, &snapshot, window, cx);
                    });
                }
            }
            None if choose_tree
                .as_ref()
                .is_some_and(|state| state.kind == zz_protocol::ChooseTreeKind::Windows)
                && self.pending_palette_chooser_close.is_none()
                && !self.command_palette.as_ref().is_some_and(|palette| {
                    palette.read(cx).is_local()
                        && self.local_palette_chooser_revision == Some(choose_tree_revision)
                }) =>
            {
                let state = choose_tree.as_ref().expect("window chooser state");
                if self
                    .command_palette
                    .as_ref()
                    .is_none_or(|palette| !palette.read(cx).is_window_chooser())
                {
                    self.local_palette_prompt_revision = None;
                    self.local_palette_chooser_revision = None;
                    let mux = self.mux.clone();
                    let palette = cx.new(|cx| {
                        CommandPaletteView::new_window_chooser(
                            palette_backend(mux),
                            state,
                            choose_tree_revision,
                            window,
                            cx,
                        )
                    });
                    self.observe_command_palette(&palette, window, cx);
                    self.command_palette = Some(palette);
                }
                if let Some(palette) = &self.command_palette {
                    palette.update(cx, |palette, cx| {
                        palette.synchronize_window_chooser(state, choose_tree_revision, window, cx);
                    });
                }
            }
            None => {
                if let Some(palette) = self
                    .command_palette
                    .as_ref()
                    .filter(|palette| palette.read(cx).is_local())
                {
                    palette.update(cx, |palette, cx| palette.refresh(window, cx));
                } else if self.command_palette.take().is_some() {
                    self.focused_pane = None;
                }
            }
        }

        match choose_tree
            .as_ref()
            .filter(|state| state.kind != zz_protocol::ChooseTreeKind::Windows)
        {
            Some(state) => {
                if self.choose_tree.is_none() {
                    let mux = self.mux.clone();
                    self.choose_tree = Some(cx.new(|cx| ChooseTreeView::new(mux, cx)));
                }
                if let Some(chooser) = &self.choose_tree {
                    chooser.update(cx, |chooser, cx| {
                        chooser.synchronize(state, choose_tree_revision, cx);
                    });
                }
            }
            None => {
                if self.choose_tree.take().is_some() {
                    self.focused_pane = None;
                }
            }
        }

        match choose_buffer.as_ref() {
            Some(state) => {
                if self.choose_buffer.is_none() {
                    let mux = self.mux.clone();
                    self.choose_buffer = Some(cx.new(|cx| ChooseBufferView::new(mux, cx)));
                }
                if let Some(chooser) = &self.choose_buffer {
                    chooser.update(cx, |chooser, cx| {
                        chooser.synchronize(state, choose_buffer_revision, cx);
                    });
                }
            }
            None => {
                if self.choose_buffer.take().is_some() {
                    self.focused_pane = None;
                }
            }
        }

        if let Some(state) = display_panes.as_ref() {
            self.pane_indicators = state
                .indicators
                .iter()
                .map(|indicator| (indicator.pane, indicator.clone()))
                .collect();
            if self.display_panes.is_none() {
                let mux = self.mux.clone();
                self.display_panes = Some(cx.new(|cx| DisplayPanesView::new(mux, cx)));
            }
            if let Some(overlay) = &self.display_panes {
                overlay.update(cx, |overlay, cx| {
                    overlay.synchronize(display_panes_revision, cx);
                });
            }
        } else {
            self.pane_indicators.clear();
            if self.display_panes.take().is_some() {
                self.focused_pane = None;
            }
        }

        for pane in &wanted_terminals {
            let commands = self
                .mux
                .update(cx, |mux, _| mux.take_terminal_commands(*pane));
            if let Some(terminal) = self.terminals.get(pane) {
                for command in commands {
                    terminal.update(cx, |terminal, cx| {
                        terminal.apply_ui_command(command, window, cx);
                    });
                }
            }
        }

        for pane in &wanted_browsers {
            let commands = self
                .mux
                .update(cx, |mux, _| mux.take_browser_commands(*pane));
            if let Some(browser) = self.browsers.get(pane) {
                for command in commands {
                    browser.update(cx, |browser, cx| browser.apply_command(command, cx));
                }
            }
        }

        self.pickers.retain(|pane, _| wanted_pickers.contains(pane));
        self.terminals
            .retain(|pane, _| wanted_terminals.contains(pane));
        self.terminal_headers
            .retain(|pane, _| wanted_terminals.contains(pane));
        self.agents.retain(|pane, _| wanted_agents.contains(pane));
        self.editors.retain(|pane, _| wanted_editors.contains(pane));
        let removed_browsers = self
            .browsers
            .keys()
            .filter(|pane| !wanted_browsers.contains(pane))
            .copied()
            .collect::<Vec<_>>();
        for pane in removed_browsers {
            self.controller
                .update(cx, |controller, _| controller.close_pane(pane));
            self.browsers.remove(&pane);
        }

        for session in snapshot
            .sessions
            .iter()
            .filter(|session| Some(session.id) == attached)
        {
            for mux_window in &session.windows {
                let active_window_visible = Some(mux_window.id) == active_window;
                for pane in mux_window.panes.keys() {
                    let covered_by_output = self
                        .command_output
                        .as_ref()
                        .is_some_and(|(output_pane, _)| output_pane == pane);
                    let covered_by_choose_tree = self.choose_tree.is_some();
                    let covered_by_choose_buffer = self.choose_buffer.is_some();
                    let covered_by_settings = route == WorkspaceRoute::Settings;
                    let float = mux_window.floating.iter().find(|float| float.pane == *pane);
                    let visible_in_layout = match float {
                        Some(float) => float.visible || mux_window.zoomed_pane == Some(*pane),
                        None => mux_window.zoomed_pane.is_none_or(|zoomed| zoomed == *pane),
                    };
                    let visible = active_window_visible
                        && visible_in_layout
                        && !covered_by_output
                        && !covered_by_choose_tree
                        && !covered_by_choose_buffer
                        && !covered_by_settings;
                    if let Some(terminal) = self.terminals.get(pane) {
                        terminal.update(cx, |terminal, cx| {
                            terminal.set_visible(visible, cx);
                            terminal.set_floating(
                                float.is_some() && mux_window.zoomed_pane != Some(*pane),
                                cx,
                            );
                        });
                    }
                    if let Some(browser) = self.browsers.get(pane) {
                        browser.update(cx, |browser, cx| {
                            browser.set_visible(visible, window, cx);
                        });
                    }
                    if let Some(agent) = self.agents.get(pane) {
                        agent.update(cx, |agent, cx| agent.set_visible(visible, cx));
                    }
                }
            }
        }

        let active_pane = snapshot
            .sessions
            .iter()
            .find(|session| Some(session.id) == attached)
            .and_then(|session| {
                let focused_window = snapshot.focused_window_for(session);
                session
                    .windows
                    .iter()
                    .find(|mux_window| mux_window.id == focused_window)
            })
            .map(|mux_window| mux_window.active_pane);
        let persistent_navigator_focused = {
            let sidebar = self.sidebar.read(cx);
            sidebar.is_focused(window)
                && (sidebar.mode() == ChromeMode::Sidebar || sidebar.slideover_open())
        };
        let picker_takes_over = active_pane.is_some_and(|active| {
            self.pickers.contains_key(&active)
                && self.focused_pane.map(|(pane, _)| pane) != Some(active)
        });
        self.audit_pane_focus("pass", window, cx);
        let floating_input = self.menu.is_some()
            || self.confirm.is_some()
            || self
                .command_palette
                .as_ref()
                .is_some_and(|palette| palette.read(cx).is_local());
        let overlay = (route == WorkspaceRoute::App || floating_input)
            .then(|| self.visible_overlay(cx))
            .flatten();
        let previous_overlay = self.focused_overlay;
        let overlay_needs_focus = overlay
            .as_ref()
            .is_some_and(|(kind, _)| self.focused_overlay != Some(*kind));
        self.focused_overlay = overlay.as_ref().map(|(kind, _)| *kind);
        if floating_input {
            if let Some((_, focus)) = overlay
                && overlay_needs_focus
            {
                self.sidebar_focus_owed = persistent_navigator_focused;
                focus.focus(window, cx);
            }
            self.pane_focus_owed = false;
            self.focused_pane = None;
        } else if route == WorkspaceRoute::Settings {
            if (entering_settings
                || matches!(
                    previous_overlay,
                    Some(OverlayKind::Menu | OverlayKind::Confirm)
                ))
                && let Some(settings) = self.sidebar.read(cx).settings_view()
            {
                let entity = settings.entity_id();
                let focus = settings.read(cx).focus();
                focus.focus(window, cx);
                cx.defer_in(window, move |_, window, cx| {
                    focus.focus(window, cx);
                    ala::App::notify(cx, entity);
                });
            }
            self.sidebar_focus_owed = false;
            self.pane_focus_owed = false;
            self.focused_pane = None;
        } else if let Some((_, focus)) = overlay {
            if overlay_needs_focus {
                self.sidebar_focus_owed = persistent_navigator_focused;
                focus.focus(window, cx);
            }
            self.pane_focus_owed = false;
            self.focused_pane = None;
        } else if sidebar_focus_requested {
            log::info!(
                target: "zz::diagnostics::input",
                "sidebar_focus_applied revision={sidebar_focus_revision} was_focused={persistent_navigator_focused}"
            );
            self.sidebar
                .update(cx, |sidebar, cx| sidebar.focus(window, cx));
            self.sidebar_focus_revision = sidebar_focus_revision;
            self.sidebar_focus_owed = false;
            self.pane_focus_owed = false;
            self.focused_pane = None;
        } else if self.empty_workspace_focus_pending {
            let focus = self.new_session.read(cx).focus().clone();
            focus.focus(window, cx);
            self.empty_workspace_focus_pending = false;
            self.sidebar_focus_owed = false;
            self.pane_focus_owed = false;
            self.focused_pane = None;
        } else if self.sidebar_focus_owed {
            self.sidebar
                .update(cx, |sidebar, cx| sidebar.refocus(window, cx));
            self.sidebar_focus_owed = false;
            self.pane_focus_owed = false;
            self.focused_pane = None;
        } else if persistent_navigator_focused && !picker_takes_over && !self.pane_focus_owed {
            self.focused_pane = None;
        } else if let Some((pane, entity, focus)) = active_pane.and_then(|pane| {
            self.pane_focus_target(pane, cx)
                .map(|(entity, focus)| (pane, entity, focus))
        }) {
            let stranded = window.focused(cx).is_none();
            if (self.focused_pane != Some((pane, entity)) || stranded)
                && !self.inline_browser_holds(pane, window, cx)
            {
                log::info!(
                    target: "zz::diagnostics::input",
                    "pane_focus_applied pane={pane} previous={:?} stranded={stranded}",
                    self.focused_pane.map(|(pane, _)| pane)
                );
                focus.focus(window, cx);
                cx.defer_in(window, move |view, window, cx| {
                    ala::App::notify(cx, entity);
                    view.audit_pane_focus("applied", window, cx);
                });
            }
            self.pane_focus_owed = false;
            self.focused_pane = Some((pane, entity));
        } else {
            self.focused_pane = None;
        }
        log::trace!(
            target: "zz::diagnostics::app_render",
            "synchronize_panes attached={attached:?} mux_generation={} sessions={} wanted_pickers={} wanted_terminals={} wanted_browsers={} wanted_agents={} wanted_editors={} picker_entities={} terminal_entities={} browser_entities={} agent_entities={} editor_entities={} command_output={} command_palette={} choose_tree={} choose_buffer={} display_panes={} active_window={active_window:?} active_pane={active_pane:?} focused_pane={:?} elapsed_us={}",
            snapshot.generation,
            snapshot.sessions.len(),
            wanted_pickers.len(),
            wanted_terminals.len(),
            wanted_browsers.len(),
            wanted_agents.len(),
            wanted_editors.len(),
            self.pickers.len(),
            self.terminals.len(),
            self.browsers.len(),
            self.agents.len(),
            self.editors.len(),
            self.command_output.is_some(),
            self.command_palette.is_some(),
            self.choose_tree.is_some(),
            self.choose_buffer.is_some(),
            self.display_panes.is_some(),
            self.focused_pane,
            diagnostics::elapsed_us(started),
        );
        self.synchronized_signature = Some(SynchronizeSignature {
            revision,
            sidebar_focused: self.sidebar.read(cx).is_focused(window),
            route,
        });
    }

    fn drain_gui_requests(&mut self, cx: &mut Context<Self>) {
        if !self.mux.read(cx).has_gui_requests() {
            return;
        }
        let screenshots = self.mux.update(cx, |mux, _| mux.take_screenshot_requests());
        for (pane, request_id, path) in screenshots {
            if let Some(browser) = self.browsers.get(&pane) {
                browser.update(cx, |browser, cx| browser.screenshot(request_id, path, cx));
            } else {
                self.mux.read(cx).respond_to_request(GuiResponse::Error {
                    request_id,
                    message: format!("{pane} has no live browser view in the attached session"),
                });
            }
        }
    }

    fn on_agent_chosen(&mut self, chosen: &AgentChosen, cx: &mut Context<Self>) {
        self.pending_lenses.insert(chosen.pane);
        let host = self.mux.read(cx).attached_host();
        self.sidebar.update(cx, |sidebar, _| {
            sidebar.follow_moved_window(host, chosen.window, chosen.from);
        });
    }

    fn promote_pending_lenses(&mut self, snapshot: &MuxSnapshot, attached: Option<SessionId>) {
        let panes = snapshot
            .sessions
            .iter()
            .flat_map(|session| {
                session.windows.iter().flat_map(move |window| {
                    window.panes.values().map(move |pane| (session.id, pane))
                })
            })
            .filter(|(_, pane)| self.pending_lenses.contains(&pane.id))
            .map(|(session, pane)| {
                let ready = Some(session) == attached
                    && matches!(pane.kind, PaneKindSnapshot::Terminal)
                    && zz_client::window_summary::pane_agent(pane).is_some();
                (pane.id, ready)
            })
            .collect::<Vec<_>>();
        self.pending_lenses
            .retain(|pane| panes.iter().any(|(id, ready)| id == pane && !ready));
        self.lenses.extend(
            panes
                .into_iter()
                .filter(|(_, ready)| *ready)
                .map(|(id, _)| id),
        );
    }

    fn lensed_panes(
        &self,
        snapshot: &MuxSnapshot,
        attached: Option<SessionId>,
    ) -> BTreeMap<PaneId, zz_protocol::AgentProvider> {
        snapshot
            .sessions
            .iter()
            .filter(|session| Some(session.id) == attached)
            .flat_map(|session| &session.windows)
            .flat_map(|window| &window.panes)
            .filter(|(pane, _)| self.lenses.contains(pane))
            .filter_map(|(pane, snapshot)| {
                matches!(snapshot.kind, PaneKindSnapshot::Terminal)
                    .then(|| zz_client::window_summary::pane_agent(snapshot))
                    .flatten()
                    .map(|provider| (*pane, provider))
            })
            .collect()
    }

    fn ensure_lens_view(
        &mut self,
        pane: PaneId,
        provider: zz_protocol::AgentProvider,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        #[cfg(feature = "agent-pane")]
        self.mux.update(cx, |mux, _| mux.open_agent_lens(pane));
        if self.agents.contains_key(&pane) {
            return;
        }
        let controller = self.agent_controller.clone();
        let mux = self.mux.clone();
        let view = cx.new(|cx| AgentView::new(pane, provider, controller, mux, window, cx));
        cx.subscribe_in(
            &view,
            window,
            move |this, _, _: &ShowTerminal, window, cx| {
                if this.lenses.contains(&pane) {
                    this.toggle_lens(pane, window, cx);
                }
            },
        )
        .detach();
        self.agents.insert(pane, view);
    }

    fn toggle_lens(&mut self, pane: PaneId, window: &mut Window, cx: &mut Context<Self>) {
        if !self.lenses.remove(&pane) {
            self.lenses.insert(pane);
        }
        #[cfg(feature = "agent-pane")]
        if !self.lenses.contains(&pane) {
            self.mux.update(cx, |mux, _| mux.close_agent_lens(pane));
        }
        self.synchronized_signature = None;
        self.synchronize_panes(window, cx);
        if let Some((_, focus)) = self.pane_focus_target(pane, cx) {
            focus.focus(window, cx);
        }
        cx.notify();
    }

    fn pane_lens(&self, snapshot: &zz_protocol::PaneSnapshot) -> Option<PaneLens> {
        matches!(snapshot.kind, PaneKindSnapshot::Terminal)
            .then(|| zz_client::window_summary::pane_agent(snapshot))
            .flatten()
            .map(|provider| PaneLens {
                provider,
                open: self.lenses.contains(&snapshot.id),
            })
    }

    /// Hand the agent controller everything the mux client buffered for it:
    /// the seq-filtered stream and the published pane states.
    #[cfg(feature = "agent-pane")]
    fn drain_agent_events(&mut self, cx: &mut Context<Self>) {
        if !self.mux.read(cx).has_agent_events() {
            return;
        }
        let registered = self.agent_controller.read(cx).registered_panes();
        let events = self
            .mux
            .update(cx, |mux, _| mux.take_agent_events_for(&registered));
        self.agent_controller.update(cx, |controller, cx| {
            for (pane, items) in events.items {
                controller.apply_stream_items(pane, items, cx);
            }
            for (pane, state) in events.states {
                controller.apply_pane_state(pane, &state, cx);
            }
        });
    }

    #[cfg(not(feature = "agent-pane"))]
    fn drain_agent_events(&mut self, _cx: &mut Context<Self>) {}

    fn reconcile_split_drag(&mut self, active_window: Option<&WindowSnapshot>, cx: &App) {
        let Some(mut drag) = self.split_drag else {
            return;
        };
        if drag.committed_snapshot_revision.is_none() && !cx.has_active_drag() {
            let ratio_basis_points = split_ratio_basis(drag.ratio);
            if ratio_basis_points == split_ratio_basis(drag.drag.start_ratio) {
                self.set_split_drag(None);
                return;
            }
            drag.ratio = f32::from(ratio_basis_points) / f32::from(SPLIT_RATIO_BASIS);
            drag.committed_snapshot_revision = Some(self.snapshot_revision);
            self.set_split_drag(Some(drag));
            self.mux
                .read(cx)
                .send_input(zz_protocol::InputMessage::ResizeSplit {
                    window: drag.drag.window,
                    split: drag.drag.split,
                    ratio_basis_points,
                });
        }
        if drag
            .committed_snapshot_revision
            .is_some_and(|revision| revision != self.snapshot_revision)
        {
            self.set_split_drag(None);
            return;
        }
        let Some(window) = active_window
            .filter(|window| window.id == drag.drag.window && window.zoomed_pane.is_none())
        else {
            self.set_split_drag(None);
            return;
        };
        if !window.layout.contains_split(drag.drag.split) {
            self.set_split_drag(None);
        }
    }

    fn begin_float_drag(
        &mut self,
        window: WindowId,
        border_status: PaneBorderStatus,
        float: FloatingPaneSnapshot,
        grip: FloatGrip,
        cell: Size<Pixels>,
        press: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        let mux = self.mux.read(cx);
        mux.execute(pane_select_command(float.pane));
        if self
            .float_drag
            .is_some_and(|drag| drag.committed_snapshot_revision.is_some())
        {
            return;
        }
        let snapshot = mux.snapshot();
        let window_snapshot = snapshot
            .sessions
            .iter()
            .flat_map(|session| &session.windows)
            .find(|snapshot| snapshot.id == window);
        let (columns, rows) = window_snapshot.map_or((0, 0), |snapshot| (snapshot.sx, snapshot.sy));
        let modal = window_snapshot
            .and_then(|snapshot| snapshot.modal)
            .is_some_and(|modal| modal.pane == float.pane);
        let landing = self
            .pip_landing
            .take_if(|landing| landing.window == window && landing.pane == float.pane);
        let start = landing.map_or_else(|| FloatCells::from(&float), |landing| landing.target);
        let layout = PipLayout {
            window: (columns, rows),
            cell,
            canvas: self.pane_canvas_bounds.get().size,
            lines: float.border_lines,
            header: cx.theme().pane_header_height(),
        };
        let shown = self
            .pane_motion
            .drawn(float.pane)
            .map_or_else(Point::default, |drawn| {
                drawn.origin - self.pane_canvas_bounds.get().origin - layout.frame(start).origin
            });
        let pip = (!modal && grip.is_move()).then(|| PipDrag::new(start, layout, press, shown));
        if pip.is_some() {
            self.pip_throw = PipThrow::default();
            self.pip_throw.track(cx.background_executor().now(), press);
        }
        self.float_drag = Some(FloatDragState {
            window,
            pane: float.pane,
            grip,
            lines: float.border_lines,
            area: FloatWindow {
                border_status,
                rows,
            },
            start,
            press,
            cell,
            delta: (0, 0),
            started: false,
            committed_snapshot_revision: None,
            request: None,
            pip,
            limit: (!modal).then(|| layout.max_cells()),
        });
    }

    fn on_pip_grab_move(
        &mut self,
        event: &DragMoveEvent<PipGrab>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let pane = event.drag(cx).pane;
        let window_handle = window;
        if self.float_drag.is_none_or(|drag| drag.pane != pane) {
            let snapshot = self.mux.read(cx).snapshot();
            let Some((window, float)) = snapshot
                .sessions
                .iter()
                .flat_map(|session| &session.windows)
                .find_map(|window| {
                    let float = window.floating.iter().find(|float| float.pane == pane)?;
                    Some((window, *float))
                })
            else {
                return;
            };
            let (id, border_status) = (window.id, window.pane_border_status);
            let cell = self.float_cell_size(window_handle, cx);
            self.begin_float_drag(
                id,
                border_status,
                float,
                FloatGrip::MOVE,
                cell,
                event.event.position,
                cx,
            );
        }
        let Some(drag) = self
            .float_drag
            .as_mut()
            .filter(|drag| drag.pane == pane && drag.committed_snapshot_revision.is_none())
        else {
            return;
        };
        let Some(pip) = drag.pip.as_mut() else {
            return;
        };
        drag.started = true;
        pip.moved(event.event.position);
        self.pip_throw
            .track(cx.background_executor().now(), event.event.position);
        cx.notify();
        cx.stop_propagation();
    }

    fn on_float_drag_move(
        &mut self,
        event: &DragMoveEvent<FloatDrag>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let pane = event.drag(cx).pane;
        let Some(drag) = self
            .float_drag
            .as_mut()
            .filter(|drag| drag.pane == pane && drag.committed_snapshot_revision.is_none())
        else {
            return;
        };
        if let Some(pip) = drag.pip.as_mut() {
            drag.started = true;
            pip.moved(event.event.position);
            self.pip_throw
                .track(cx.background_executor().now(), event.event.position);
            cx.notify();
            cx.stop_propagation();
            return;
        }
        let offset = event.event.position - drag.press;
        let delta = (
            (offset.x / drag.cell.width).round() as i32,
            (offset.y / drag.cell.height).round() as i32,
        );
        drag.started = true;
        if drag.delta != delta {
            drag.delta = delta;
            cx.notify();
        }
        cx.stop_propagation();
    }

    fn on_float_mouse_up(&mut self, event: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        if event.button != MouseButton::Left
            || self
                .float_drag
                .is_none_or(|drag| drag.committed_snapshot_revision.is_some())
        {
            return;
        }
        self.commit_float_drag(cx);
        cx.notify();
    }

    fn commit_float_drag(&mut self, cx: &App) {
        let Some(mut drag) = self.float_drag else {
            return;
        };
        if drag.throwing() {
            self.float_drag = None;
            if let Some(pip) = drag.thrown() {
                self.throw_pip(drag.window, drag.pane, pip, cx);
            }
            return;
        }
        let target = drag.target();
        if !drag.started || target == drag.start {
            self.float_drag = None;
            return;
        }
        drag.committed_snapshot_revision = Some(self.snapshot_revision);
        drag.request = self.mux.read(cx).execute_tracked(float_drag_command(
            drag.pane, drag.grip, drag.start, target, drag.lines, drag.area,
        ));
        self.float_drag = drag.request.is_some().then_some(drag);
    }

    fn throw_pip(&mut self, window: WindowId, pane: PaneId, pip: PipDrag, cx: &App) {
        let velocity = self.pip_throw.velocity(cx.background_executor().now());
        let landing = pip.release(velocity);
        if landing.stash.is_some() {
            self.pip_stage.stash(pane);
        }
        self.pane_motion
            .fling(pane, velocity, landing.stash.is_none());
        let request = self
            .mux
            .read(cx)
            .execute_tracked(landing.place.command(pane));
        self.pip_landing = Some(PipLandingState {
            window,
            pane,
            target: landing.target,
            request,
        });
    }

    fn settle_pip_landing(&mut self, cx: &App) {
        if self.pip_landing.is_some_and(|landing| {
            landing
                .request
                .is_none_or(|request| self.mux.read(cx).command_settled(request))
        }) {
            self.pip_landing = None;
        }
    }

    fn reconcile_float_drag(&mut self, active_window: Option<&WindowSnapshot>, cx: &App) {
        let Some(drag) = self.float_drag else {
            return;
        };
        if drag.committed_snapshot_revision.is_none() && drag.started && !cx.has_active_drag() {
            self.commit_float_drag(cx);
        }
        let Some(drag) = self.float_drag else {
            return;
        };
        let current = active_window
            .filter(|window| window.id == drag.window)
            .and_then(|window| {
                window
                    .floating
                    .iter()
                    .find(|float| float.pane == drag.pane && float.visible)
            })
            .map(FloatCells::from);
        let Some(current) = current else {
            self.float_drag = None;
            return;
        };
        if drag
            .committed_snapshot_revision
            .is_some_and(|revision| revision != self.snapshot_revision)
            && drag.halfway() != Some(current)
        {
            self.float_drag = None;
        }
    }

    fn set_split_drag(&mut self, split_drag: Option<SplitDragState>) {
        self.split_drag = split_drag;
        self.terminal_resize_suppressed
            .set(self.split_drag.is_some() || self.terminal_resizes_held);
    }

    /// Keeps terminals at their grid size while the chrome around the panes
    /// slides, so the daemon sees one resize when it settles instead of one a
    /// frame. Releasing repaints every terminal so each reports its new size.
    pub(crate) fn hold_terminal_resizes(&mut self, hold: bool, cx: &mut Context<Self>) {
        self.terminal_resizes_held = hold;
        self.terminal_resize_suppressed
            .set(self.split_drag.is_some() || hold);
        if !hold {
            for terminal in self.terminals.values() {
                terminal.update(cx, |_, cx| cx.notify());
            }
        }
    }

    fn reconcile_pane_layout_override(
        &mut self,
        active_window: Option<&WindowSnapshot>,
        generation: u64,
    ) {
        if !self
            .pane_layout_override
            .as_ref()
            .is_some_and(|pending| pending.still_predicts(active_window, generation))
        {
            self.pane_layout_override = None;
        }
    }

    fn reconcile_pane_drag(
        &mut self,
        active_window: Option<&WindowSnapshot>,
        prefix_armed: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(drag) = self.pane_drag.as_ref() else {
            return;
        };
        if !cx.has_active_drag() {
            let drag = take_pane_drag(&mut self.pane_drag).unwrap();
            self.finish_pane_drag(drag, cx);
            return;
        }
        let valid_window = active_window.is_some_and(|window| drag.matches(window, prefix_armed));
        if !valid_window {
            self.pane_drag = None;
            cx.stop_active_drag(window);
            cx.notify();
        }
    }

    fn pane_drag_state(&self, source: PaneId, cx: &App) -> Option<PaneDragState> {
        let mux = self.mux.read(cx);
        let attached = mux.attached_session()?;
        let snapshot = mux.snapshot();
        let session = snapshot
            .sessions
            .iter()
            .find(|session| session.id == attached)?;
        let focused_window = snapshot.focused_window_for(session);
        let window = session
            .windows
            .iter()
            .find(|window| window.id == focused_window)?;
        PaneDragState::new(
            source,
            window.id,
            &window.layout,
            self.pane_canvas_bounds.get().size,
            pane_split_slot(config::pane_margin(cx)),
        )
    }

    fn on_pane_drag_start(&mut self, payload: PaneDrag, cx: &mut Context<Self>) {
        let pane = payload.pane;
        if self
            .pane_drag
            .as_ref()
            .is_some_and(|drag| drag.source == pane)
        {
            return;
        }
        if let Some(mut drag) = self.pane_drag_state(pane, cx) {
            drag.requires_prefix = payload.requires_prefix;
            self.pane_drop_preview.set(DropPreviewFrame::default());
            self.pane_drag = Some(drag);
            cx.notify();
        }
    }

    fn on_pane_drag_move(
        &mut self,
        event: &DragMoveEvent<PaneDrag>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let source = event.drag(cx).pane;
        let position = event.event.position - event.bounds.origin;
        let rendered = self.pane_drop_preview.get();
        let Some(drag) = self.pane_drag.as_mut().filter(|drag| drag.source == source) else {
            return;
        };
        let target = drag.target_at(position);
        if drag.set_target(target, rendered) {
            cx.notify();
        }
    }

    fn on_pane_drop(&mut self, drag: PaneDrag, window: &mut Window, cx: &mut Context<Self>) {
        let Some(mut state) = self.pane_drag.take_if(|state| state.source == drag.pane) else {
            return;
        };
        state.target =
            state.target_at(window.mouse_position() - self.pane_canvas_bounds.get().origin);
        if let Some((target, zone)) = state.target
            && let Some(command) = pane_drop_command(state.source, target, zone)
        {
            self.pane_layout_override =
                predicted_drop_layout(&state.layout, state.source, target, zone).map(|layout| {
                    PaneLayoutOverride {
                        window: state.window,
                        layout,
                        generation: self.mux.read(cx).snapshot().generation,
                    }
                });
            self.mux.read(cx).execute(command);
        }
        self.finish_pane_drag(drag, cx);
    }

    fn finish_pane_drag(&self, drag: PaneDrag, cx: &mut Context<Self>) {
        if drag.requires_prefix && self.mux.read(cx).prefix_armed() {
            self.dismiss_armed_prefix(drag.pane, cx);
        }
        cx.notify();
    }

    fn on_pane_mouse_up(
        &mut self,
        event: &MouseUpEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.button != MouseButton::Left || self.pane_drag.is_none() {
            return;
        }
        cx.defer_in(window, |view, _window, cx| {
            let Some(source) = take_pane_drag(&mut view.pane_drag) else {
                return;
            };
            view.finish_pane_drag(source, cx);
        });
    }

    fn on_pane_mouse_exit(&mut self, _: &MouseExitEvent, _: &mut Window, cx: &mut Context<Self>) {
        let rendered = self.pane_drop_preview.get();
        if self
            .pane_drag
            .as_mut()
            .is_some_and(|drag| drag.set_target(None, rendered))
        {
            cx.notify();
        }
    }

    /// `server_client_check_mouse`: a pointer move over a pane that is not the
    /// active one selects it while `focus-follows-mouse` is on. Unlike a click
    /// it leaves an armed prefix alone, which is all the pin's
    /// `window_set_active_pane` does.
    fn on_pane_pointer_focus(&mut self, pane: PaneId, cx: &mut Context<Self>) {
        self.mux.read(cx).execute(pane_select_command(pane));
    }

    fn on_pane_click(&mut self, pane: PaneId, cx: &mut Context<Self>) {
        self.mux.read(cx).execute(pane_select_command(pane));
        self.dismiss_armed_prefix(pane, cx);
        cx.notify();
    }

    fn dismiss_armed_prefix(&self, pane: PaneId, cx: &App) {
        let escape = Keystroke {
            modifiers: ala::Modifiers::default(),
            key: "escape".to_owned(),
            key_char: None,
        };
        for action in [TerminalKeyAction::Press, TerminalKeyAction::Release] {
            self.mux.read(cx).send_input(InputMessage::Key {
                pane,
                input: terminal_key_input(&escape, action),
                text_follows: false,
            });
        }
    }

    fn on_split_drag_move(
        &mut self,
        event: &DragMoveEvent<SplitDrag>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let drag = *event.drag(cx);
        if self.split_drag.is_some_and(|state| {
            state.drag.split == drag.split && state.committed_snapshot_revision.is_some()
        }) {
            return;
        }
        let ratio = split_ratio_from_pointer(drag.axis, event.event.position, event.bounds);
        if self.split_drag.is_none_or(|state| {
            state.drag.split != drag.split || state.ratio.to_bits() != ratio.to_bits()
        }) {
            self.set_split_drag(Some(SplitDragState {
                drag,
                ratio,
                committed_snapshot_revision: None,
            }));
            cx.notify();
        }
        cx.stop_propagation();
    }

    /// A pointer report that lands outside the menu's own surface. `menu.c`
    /// gives `menu_key_cb` every mouse report while the menu owns the client's
    /// overlay, so the reports the menu's occluding surface never sees have to
    /// reach it too: that is where a release closes a menu that is not
    /// stay-open, and where any press closes one that is.
    fn route_menu_pointer(
        &mut self,
        kind: MenuPointerKind,
        buttons: u8,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        let Some(menu) = self.menu.clone() else {
            return;
        };
        menu.update(cx, |menu, cx| {
            menu.pointer(kind, buttons, position, cx);
        });
        cx.stop_propagation();
    }

    fn on_menu_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.route_menu_pointer(
            MenuPointerKind::Press,
            crate::command::menu::press_buttons(event.button),
            event.position,
            cx,
        );
    }

    fn on_menu_mouse_up(&mut self, event: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.route_menu_pointer(
            MenuPointerKind::Release,
            crate::command::menu::RELEASE_BUTTONS,
            event.position,
            cx,
        );
    }

    fn on_split_mouse_up(&mut self, event: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        if event.button != MouseButton::Left {
            return;
        }
        let Some(mut state) = self.split_drag else {
            return;
        };
        let ratio_basis_points = split_ratio_basis(state.ratio);
        if ratio_basis_points == split_ratio_basis(state.drag.start_ratio) {
            self.set_split_drag(None);
        } else {
            state.ratio = f32::from(ratio_basis_points) / f32::from(SPLIT_RATIO_BASIS);
            state.committed_snapshot_revision = Some(self.snapshot_revision);
            self.set_split_drag(Some(state));
            self.mux
                .read(cx)
                .send_input(zz_protocol::InputMessage::ResizeSplit {
                    window: state.drag.window,
                    split: state.drag.split,
                    ratio_basis_points,
                });
        }
        cx.notify();
        cx.stop_propagation();
    }

    fn pane_drag_handle(
        pane: PaneId,
        title: String,
        state: PaneDragOverlayState,
        radii: Corners<Pixels>,
        cx: &mut Context<Self>,
    ) -> ala::Stateful<ala::Div> {
        let app_view = cx.entity();
        pane_drag_overlay(("pane-drag-overlay", pane.0), state, radii, cx)
            .on_click(cx.listener(move |view, _, _, cx| {
                view.on_pane_click(pane, cx);
            }))
            .on_drag(
                PaneDrag {
                    pane,
                    requires_prefix: true,
                },
                move |drag, grab, _, cx| {
                    app_view.update(cx, |view, cx| {
                        view.on_pane_drag_start(*drag, cx);
                    });
                    pane_drag_preview(drag.pane, title.clone(), grab, cx)
                },
            )
            .on_drop(cx.listener(move |view, drag: &PaneDrag, window, cx| {
                view.on_pane_drop(*drag, window, cx);
            }))
    }

    fn pane_drop_preview_element(&self, drag: &PaneDragState, cx: &App) -> Option<AnyElement> {
        let preview = drag.preview?;
        let radius = config::pane_content_radii(cx, WindowCorners::NONE).top_left;
        let border = cx.theme().pane_edge_width().max(px(1.0));
        let rendered = self.pane_drop_preview.clone();
        Some(
            pane_drop_preview(radius, border, cx)
                .with_animation(
                    ("pane-drop-preview", preview.sequence),
                    Animation::new(preview.duration).with_easing(ease_out_quint()),
                    move |element, delta| {
                        let frame = preview.at(delta);
                        rendered.set(frame);
                        element
                            .left(frame.bounds.origin.x)
                            .top(frame.bounds.origin.y)
                            .w(frame.bounds.size.width)
                            .h(frame.bounds.size.height)
                            .opacity(frame.opacity)
                    },
                )
                .into_any_element(),
        )
    }

    fn pane_content(
        &self,
        pane: PaneId,
        inactive: bool,
        inactive_opacity: f32,
        cx: &mut Context<Self>,
    ) -> Option<PaneContent> {
        if let Some((_, output)) = self
            .command_output
            .as_ref()
            .filter(|(output_pane, _)| *output_pane == pane)
        {
            output.update(cx, |output, cx| {
                output.set_text_dimmed(inactive, inactive_opacity, cx);
            });
            Some(PaneContent {
                view: AnyView::from(output.clone()),
                cached: true,
                inactive_style: PaneInactiveStyle::Content,
                covers_frame: false,
            })
        } else if let Some(picker) = self.pickers.get(&pane) {
            Some(PaneContent {
                view: AnyView::from(picker.clone()),
                cached: true,
                inactive_style: PaneInactiveStyle::Surface,
                covers_frame: false,
            })
        } else if let Some(agent) = self.agents.get(&pane) {
            Some(PaneContent {
                view: AnyView::from(agent.clone()),
                cached: true,
                inactive_style: PaneInactiveStyle::Surface,
                covers_frame: false,
            })
        } else if let Some(terminal) = self.terminals.get(&pane) {
            terminal.update(cx, |terminal, cx| {
                terminal.set_text_dimmed(inactive, inactive_opacity, cx);
            });
            Some(PaneContent {
                view: AnyView::from(terminal.clone()),
                cached: true,
                inactive_style: PaneInactiveStyle::Content,
                covers_frame: false,
            })
        } else if let Some(browser) = self.browsers.get(&pane) {
            browser.update(cx, |browser, cx| {
                browser.set_chrome_dimmed(inactive, inactive_opacity, cx);
            });
            Some(PaneContent {
                view: AnyView::from(browser.clone()),
                cached: false,
                inactive_style: PaneInactiveStyle::Content,
                covers_frame: true,
            })
        } else {
            self.editors.get(&pane).map(|editor| PaneContent {
                view: AnyView::from(editor.clone()),
                cached: false,
                inactive_style: PaneInactiveStyle::Surface,
                covers_frame: false,
            })
        }
    }

    fn render_layout(
        &self,
        node: &LayoutNode,
        window: &WindowSnapshot,
        corners: WindowCorners,
        drag_layer: PaneDragLayer,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match node {
            LayoutNode::Pane(pane) => {
                let radii = config::pane_content_radii(cx, corners);
                let active = *pane == window.active_pane;
                let inactive = !active;
                let inactive_opacity = config::pane_inactive_opacity(cx);
                let pane_snapshot = window.panes.get(pane);
                let synchronized = pane_snapshot.is_some_and(|pane| pane.synchronized_input);
                let dead_label = dead_label(pane_snapshot);
                let terminal_overlay = self
                    .command_output
                    .as_ref()
                    .filter(|(output_pane, _)| output_pane == pane)
                    .map(|(_, output)| output)
                    .or_else(|| {
                        self.terminals
                            .get(pane)
                            .filter(|_| !self.pickers.contains_key(pane))
                    });
                if let Some(terminal) = terminal_overlay {
                    terminal.update(cx, |terminal, cx| {
                        terminal.set_pane_status(
                            dead_label.clone(),
                            synchronized,
                            window.zoomed_pane == Some(*pane),
                            cx,
                        );
                    });
                }
                let pane_content = self.pane_content(*pane, inactive, inactive_opacity, cx);
                let waiting = pane_content.is_none();
                let content = pane_content.as_ref().map_or_else(
                    || {
                        crate::window::corners::round_div_radii(div().size_full(), radii)
                            .into_any_element()
                    },
                    PaneContent::element,
                );
                let terminal_header =
                    pane_snapshot.filter(|pane| matches!(pane.kind, PaneKindSnapshot::Terminal));
                let header_view = terminal_header
                    .and_then(|snapshot| Some((snapshot, self.terminal_headers.get(pane)?)));
                let content = if let Some((snapshot, header_view)) = header_view {
                    let state = TerminalHeaderState {
                        active,
                        title: zz_client::navigation::pane_label(snapshot),
                        can_drag: self.mux.read(cx).is_connected()
                            && window.zoomed_pane.is_none()
                            && window.panes.len() > 1,
                        floats: window.floating.iter().any(|float| float.pane == *pane),
                        lens: self.pane_lens(snapshot),
                        radii,
                        opacity: if inactive { inactive_opacity } else { 1.0 },
                    };
                    header_view.update(cx, |header, cx| header.sync(state, cx));
                    div()
                        .flex()
                        .flex_col()
                        .size_full()
                        .child(
                            AnyView::from(header_view.clone()).cached(
                                StyleRefinement::default()
                                    .h(cx.theme().pane_header_height())
                                    .w_full()
                                    .flex_none(),
                            ),
                        )
                        .child(div().flex_1().min_h_0().min_w_0().child(content))
                        .into_any_element()
                } else {
                    content
                };
                let mut status_tags: Vec<AnyElement> = Vec::new();
                if terminal_overlay.is_none() {
                    if let Some(label) = dead_label {
                        status_tags.push(
                            zz_ui::pane::pane_status_badge(zz_ui::IconName::CircleX, label, cx)
                                .into_any_element(),
                        );
                    }
                    if waiting {
                        status_tags.push(
                            pane_waiting_state(format!("Waiting for {pane}"), cx)
                                .into_any_element(),
                        );
                    }
                    if synchronized {
                        status_tags.push(pane_sync_badge(cx).into_any_element());
                    }
                    if window.zoomed_pane == Some(*pane) {
                        status_tags.push(self.zoom_control(*pane).into_any_element());
                    }
                }
                let mut overlays: Vec<AnyElement> = Vec::with_capacity(3);
                if !status_tags.is_empty() {
                    overlays.push(
                        pane_overlay_stack(PaneOverlayCorner::TopRight, status_tags)
                            .when(terminal_header.is_some(), |stack| {
                                stack.top(cx.theme().pane_header_height() + px(8.0))
                            })
                            .into_any_element(),
                    );
                }
                if let Some(indicator) = self
                    .pane_indicators
                    .get(pane)
                    .and_then(|indicator| self.pane_indicator(indicator, cx))
                {
                    overlays.push(indicator.into_any_element());
                }
                let drag_state = match drag_layer {
                    PaneDragLayer::Idle => None,
                    PaneDragLayer::Dragging(source) if source == *pane => {
                        Some(PaneDragOverlayState::Source)
                    }
                    PaneDragLayer::Armed => Some(PaneDragOverlayState::Armed),
                    PaneDragLayer::Dragging(_) => Some(PaneDragOverlayState::Target),
                };
                if let Some(drag_state) = drag_state {
                    let title = window
                        .panes
                        .get(pane)
                        .map_or_else(String::new, |pane| pane.title.clone());
                    overlays.push(
                        Self::pane_drag_handle(*pane, title, drag_state, radii, cx)
                            .into_any_element(),
                    );
                }
                let surface_dimmed = inactive
                    && pane_content
                        .as_ref()
                        .is_none_or(|content| content.inactive_style == PaneInactiveStyle::Surface);
                let covers_frame = pane_content
                    .as_ref()
                    .is_some_and(|content| content.covers_frame);
                let gaps = config::pane_gaps(cx);
                let follows_pointer = inactive && self.mux.read(cx).focus_follows_mouse();
                let hovered_pane = *pane;
                let shown = if cx.reduce_motion() {
                    1.0
                } else {
                    self.pane_arrivals
                        .shown(*pane, cx.background_executor().now())
                };
                let surface = pane_surface(
                    ("mux-pane", pane.0),
                    content,
                    overlays,
                    PaneChrome::new(radii, gaps)
                        .active(active)
                        .dimmed(surface_dimmed, inactive_opacity)
                        .rim_over_content(covers_frame)
                        .frame(gaps.then(|| {
                            cx.theme()
                                .pane_frame(crate::theme::chrome_background(cx), active)
                        })),
                    cx,
                )
                .key_context(pane_key_context(*pane))
                .when(shown < 1.0, |surface| surface.opacity(shown))
                .when(follows_pointer, |surface| {
                    surface.on_mouse_move(cx.listener(
                        move |view, event: &MouseMoveEvent, _, cx| {
                            if event.pressed_button.is_none() {
                                view.on_pane_pointer_focus(hovered_pane, cx);
                            }
                        },
                    ))
                });
                moving_pane(*pane, &self.pane_motion, surface)
                    .hold(
                        self.terminals
                            .get(pane)
                            .map(|terminal| terminal.read(cx).moving()),
                    )
                    .into_any_element()
            }
            LayoutNode::Empty => div().size_full().into_any_element(),
            LayoutNode::Split {
                id,
                axis,
                ratio,
                first,
                second,
            } => {
                let snapshot_ratio = *ratio;
                let ratio = self
                    .split_drag
                    .filter(|drag| drag.drag.window == window.id && drag.drag.split == *id)
                    .map_or(snapshot_ratio, |drag| drag.ratio);
                let (first_corners, second_corners) = corners.split(*axis);
                let first_element =
                    self.render_layout(first, window, first_corners, drag_layer, cx);
                let second_element =
                    self.render_layout(second, window, second_corners, drag_layer, cx);
                let resizing = self
                    .split_drag
                    .is_some_and(|drag| drag.drag.window == window.id && drag.drag.split == *id);
                let ratio_override = self
                    .split_drag
                    .filter(|drag| drag.drag.window == window.id)
                    .map(|drag| (drag.drag.split, drag.ratio));
                let highlight =
                    pane_separator(node, window.active_pane, ratio_override).map(|separator| {
                        let span = separator.span();
                        PaneSplitHighlight::new(
                            span.start(),
                            span.length(),
                            match separator.side() {
                                SeparatorSide::First => PaneSplitSide::First,
                                SeparatorSide::Second => PaneSplitSide::Second,
                            },
                            cx.theme().accent,
                        )
                    });
                let split_axis = match axis {
                    Axis::Horizontal => PaneSplitAxis::Horizontal,
                    Axis::Vertical => PaneSplitAxis::Vertical,
                };
                let drag = SplitDrag {
                    window: window.id,
                    split: *id,
                    start_ratio: snapshot_ratio,
                    axis: *axis,
                };
                let hit_target: AnyElement = if matches!(drag_layer, PaneDragLayer::Dragging(_)) {
                    div().absolute().into_any_element()
                } else {
                    pane_split_hit_target(
                        ("split-divider", id.0),
                        split_axis,
                        ratio,
                        config::pane_margin(cx),
                    )
                    .on_drag(drag, |_: &SplitDrag, _, _, cx| cx.new(|_| SplitDragPreview))
                    .into_any_element()
                };
                let split = *id;
                pane_split_surface(
                    ("mux-split", id.0),
                    split_axis,
                    ratio,
                    resizing,
                    config::pane_gaps(cx),
                    config::pane_margin(cx),
                    None,
                    highlight,
                    first_element,
                    second_element,
                    hit_target,
                    cx,
                )
                .on_drag_move::<SplitDrag>(cx.listener(
                    move |view, event: &DragMoveEvent<SplitDrag>, window, cx| {
                        if event.drag(cx).split == split {
                            view.on_split_drag_move(event, window, cx);
                        }
                    },
                ))
                .into_any_element()
            }
        }
    }

    fn synchronize_pane_corners(
        &mut self,
        active_window: Option<&WindowSnapshot>,
        layout: Option<&LayoutNode>,
        corners: WindowCorners,
        cx: &mut Context<Self>,
    ) {
        let mut by_pane = BTreeMap::new();
        if let Some(active_window) = active_window {
            if let Some(zoomed) = active_window.zoomed_pane {
                by_pane.insert(zoomed, corners);
            } else if let Some(layout) = layout {
                collect_pane_corners(layout, corners, &mut by_pane);
            }
        }

        for (pane, picker) in &self.pickers {
            let corners = by_pane.get(pane).copied().unwrap_or(WindowCorners::NONE);
            picker.update(cx, |picker, cx| {
                picker.set_window_corners(corners, cx);
            });
        }
        for (pane, terminal) in &self.terminals {
            let corners = by_pane.get(pane).copied().unwrap_or(WindowCorners::NONE);
            terminal.update(cx, |terminal, cx| {
                terminal.set_window_corners(corners, cx);
            });
        }
        for (pane, browser) in &self.browsers {
            let corners = by_pane.get(pane).copied().unwrap_or(WindowCorners::NONE);
            browser.update(cx, |browser, cx| {
                browser.set_window_corners(corners, cx);
            });
        }
        for (pane, agent) in &self.agents {
            let corners = by_pane.get(pane).copied().unwrap_or(WindowCorners::NONE);
            agent.update(cx, |agent, cx| {
                agent.set_window_corners(corners, cx);
            });
        }
        for (pane, editor) in &self.editors {
            let corners = by_pane.get(pane).copied().unwrap_or(WindowCorners::NONE);
            editor.update(cx, |editor, cx| {
                editor.set_window_corners(corners, cx);
            });
        }
        if let Some((pane, output)) = &self.command_output {
            let corners = by_pane.get(pane).copied().unwrap_or(WindowCorners::NONE);
            output.update(cx, |output, cx| {
                output.set_window_corners(corners, cx);
            });
        }
    }

    fn pane_indicator(&self, indicator: &PaneIndicator, cx: &Context<Self>) -> Option<ala::Div> {
        let state = self.mux.read(cx).display_panes()?;
        let mux = self.mux.clone();
        let pane = indicator.pane;
        let chip = pane_indicator(("pane-indicator", pane.0), state, indicator, cx)
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_up(MouseButton::Left, move |_, _, cx| {
                mux.read(cx)
                    .send_input(zz_protocol::InputMessage::DisplayPanes {
                        action: DisplayPanesAction::Select(pane),
                    });
                cx.stop_propagation();
            });
        Some(pane_indicator_overlay(chip))
    }

    fn float_cell_size(&self, window: &Window, cx: &App) -> Size<Pixels> {
        float_cell_size(self.mux.read(cx), window)
    }

    fn floating_layer(
        &self,
        window_snapshot: &WindowSnapshot,
        cell: Size<Pixels>,
        canvas: Size<Pixels>,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let floats = window_snapshot
            .floating
            .iter()
            .filter(|float| float.visible)
            .collect::<Vec<_>>();
        let cards = floats
            .iter()
            .filter(|float| {
                window_snapshot
                    .modal
                    .is_none_or(|modal| modal.pane != float.pane)
            })
            .filter_map(|float| {
                let (frame, held) = self.float_frame(window_snapshot, float, cell, canvas, cx)?;
                Some(PipCard {
                    pane: float.pane,
                    frame,
                    held,
                })
            })
            .collect::<Vec<_>>();
        let placed = self
            .pip_stage
            .arrange(&cards, canvas, cx.theme().pane_header_height())
            .into_iter()
            .map(|placed| (placed.pane, placed))
            .collect::<BTreeMap<_, _>>();
        let mut layer = Vec::new();
        for float in floats.into_iter().rev() {
            if let Some(modal) = window_snapshot
                .modal
                .filter(|modal| modal.pane == float.pane)
            {
                layer.push(self.modal_scrim(modal, cx));
            }
            layer.extend(self.floating_pane(
                window_snapshot,
                float,
                cell,
                canvas,
                placed.get(&float.pane).copied(),
                cx,
            ));
        }
        (!layer.is_empty()).then(|| {
            div()
                .absolute()
                .inset_0()
                .overflow_hidden()
                .children(layer)
                .into_any_element()
        })
    }

    fn modal_scrim(&self, modal: ModalPaneSnapshot, cx: &App) -> AnyElement {
        let mux = self.mux.clone();
        div()
            .id(("floating-modal-scrim", modal.pane.0))
            .debug_selector(|| "floating-modal-scrim".to_owned())
            .absolute()
            .inset_0()
            .bg(cx.theme().scrim)
            .occlude()
            .on_any_mouse_down(move |_, _, cx| {
                if modal.close_on_click {
                    mux.read(cx).execute(CommandInvocation::new(
                        "kill-pane",
                        ["-t", &modal.pane.to_string()],
                    ));
                }
                cx.stop_propagation();
            })
            .into_any_element()
    }

    fn float_frame(
        &self,
        window_snapshot: &WindowSnapshot,
        float: &FloatingPaneSnapshot,
        cell: Size<Pixels>,
        canvas: Size<Pixels>,
        cx: &App,
    ) -> Option<(Bounds<Pixels>, bool)> {
        let pane = float.pane;
        let modal = window_snapshot
            .modal
            .is_some_and(|modal| modal.pane == pane);
        let drag = self
            .float_drag
            .filter(|drag| drag.window == window_snapshot.id && drag.pane == pane);
        let throw = drag.and_then(|drag| drag.thrown());
        let landing = self
            .pip_landing
            .filter(|landing| landing.window == window_snapshot.id && landing.pane == pane);
        let cells = match (throw, landing) {
            (Some(pip), _) => pip.start,
            (None, Some(landing)) => landing.target,
            (None, None) => drag
                .filter(|drag| !drag.throwing())
                .and_then(|drag| drag.preview())
                .unwrap_or_else(|| FloatCells::from(float)),
        };
        let bordered = float.border_lines != PaneBorderLines::None;
        if canvas.width > Pixels::ZERO
            && canvas.height > Pixels::ZERO
            && float_pixels(
                cells,
                bordered,
                (f32::from(cell.width), f32::from(cell.height)),
                (f32::from(canvas.width), f32::from(canvas.height)),
            )
            .is_none()
        {
            return None;
        }
        let pad = i32::from(bordered);
        let mut frame = if modal {
            Bounds::new(
                point(
                    cell.width * (cells.xoff - pad) as f32,
                    cell.height * (cells.yoff - pad) as f32,
                ),
                ala::size(
                    cell.width * (i32::from(cells.sx) + 2 * pad) as f32,
                    cell.height * (i32::from(cells.sy) + 2 * pad) as f32,
                ),
            )
        } else {
            PipLayout {
                window: (window_snapshot.sx, window_snapshot.sy),
                cell,
                canvas,
                lines: float.border_lines,
                header: cx.theme().pane_header_height(),
            }
            .frame(cells)
        };
        if let Some(pip) = throw {
            frame.origin = pip.drawn_origin();
        }
        Some((frame, throw.is_some()))
    }

    fn floating_pane(
        &self,
        window_snapshot: &WindowSnapshot,
        float: &FloatingPaneSnapshot,
        cell: Size<Pixels>,
        canvas: Size<Pixels>,
        placed: Option<PipPlaced>,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let pane = float.pane;
        let modal = window_snapshot
            .modal
            .is_some_and(|modal| modal.pane == pane);
        let (mut frame, held) = self.float_frame(window_snapshot, float, cell, canvas, cx)?;
        if let Some(placed) = placed {
            frame = placed.frame;
        }
        let stashed = placed.and_then(|placed| placed.stashed);
        let bordered = float.border_lines != PaneBorderLines::None;
        let active = pane == window_snapshot.active_pane;
        let snapshot = window_snapshot.panes.get(&pane);
        if let Some(terminal) = self.terminals.get(&pane) {
            let dead = dead_label(snapshot);
            let synchronized = snapshot.is_some_and(|pane| pane.synchronized_input);
            terminal.update(cx, |terminal, cx| {
                terminal.set_pane_status(dead, synchronized, false, cx);
            });
        }
        let inactive_opacity = config::pane_inactive_opacity(cx);
        let content = self
            .pane_content(pane, !active, inactive_opacity, cx)
            .map_or_else(
                || div().size_full().into_any_element(),
                |content| content.element(),
            );
        let title = snapshot
            .filter(|_| float.border_status.is_on())
            .map(|pane| float_title(&pane.border_status_text))
            .unwrap_or_default();
        let inset = if bordered { cell } else { Size::default() };
        let grips = if bordered && stashed.is_none() {
            self.float_grips(
                window_snapshot.id,
                window_snapshot.pane_border_status,
                *float,
                cell,
                frame.size,
                !modal,
                cx,
            )
        } else {
            Vec::new()
        };
        let surface = if modal {
            FloatingSurface::new(("floating-pane", pane.0), content, cx)
                .title(title.left)
                .title_right(title.right)
                .content_inset(inset.width, inset.height)
                .edge_color(pane_border_color(active, cx))
                .bordered(bordered)
                .into_any_element()
        } else {
            let own_header =
                snapshot.is_some_and(|pane| matches!(pane.kind, PaneKindSnapshot::Browser(_)));
            let fill = self.terminals.get(&pane).map_or_else(
                || cx.theme().pane_color(cx.theme().background),
                |terminal| terminal.read(cx).fill(cx),
            );
            let surface = PipSurface::new(("floating-pane", pane.0), content)
                .bordered(bordered)
                .fill(fill)
                .stashed(
                    stashed.is_some(),
                    cx.listener(move |view, _: &ala::ClickEvent, _, cx| {
                        view.pip_stage.restore(pane);
                        view.mux.read(cx).execute(pane_select_command(pane));
                        cx.notify();
                    }),
                );
            if own_header {
                surface.into_any_element()
            } else {
                let label = if title.left.is_empty() {
                    snapshot
                        .map(zz_client::navigation::pane_label)
                        .unwrap_or_default()
                } else {
                    title.left
                };
                let opacity = if active { 1.0 } else { inactive_opacity };
                surface
                    .content_inset(inset.width, inset.height)
                    .header(self.pip_header(pane, snapshot, label, active, opacity, cx))
                    .into_any_element()
            }
        };
        let card = div()
            .relative()
            .size_full()
            .child(surface)
            .children(grips)
            .children(
                self.pane_indicators
                    .get(&pane)
                    .and_then(|indicator| self.pane_indicator(indicator, cx)),
            );
        Some(
            div()
                .absolute()
                .left(frame.origin.x)
                .top(frame.origin.y)
                .w(frame.size.width)
                .h(frame.size.height)
                .debug_selector(move || format!("floating-pane-{}", pane.0))
                .key_context(pane_key_context(pane))
                .map(|slot| {
                    if modal {
                        slot.child(card)
                    } else {
                        slot.child(moving_pane(pane, &self.pane_motion, card).held(held))
                    }
                })
                .into_any_element(),
        )
    }

    fn pip_header(
        &self,
        pane: PaneId,
        snapshot: Option<&zz_protocol::PaneSnapshot>,
        label: String,
        active: bool,
        opacity: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let connected = self.mux.read(cx).is_connected();
        if let Some(header_view) = self.terminal_headers.get(&pane) {
            let state = TerminalHeaderState {
                active,
                title: label,
                can_drag: connected,
                floats: true,
                lens: snapshot.and_then(|snapshot| self.pane_lens(snapshot)),
                radii: Corners::default(),
                opacity,
            };
            header_view.update(cx, |header, cx| header.sync(state, cx));
            return AnyView::from(header_view.clone())
                .cached(
                    StyleRefinement::default()
                        .h(cx.theme().pane_header_height())
                        .w_full(),
                )
                .into_any_element();
        }
        let mux = self.mux.clone();
        let icon = snapshot.map_or(zz_ui::IconName::SquareTerminal, |pane| {
            zz_ui::pane::pane_kind_icon(&pane.kind)
        });
        div()
            .opacity(opacity)
            .child(zz_ui::pane::pane_header(
                icon,
                active,
                label.clone(),
                PaneDragSource::new(pane, label, connected, |_, _, _| {}),
                true,
                None,
                move |action, _, cx| {
                    for command in action.commands(pane, None) {
                        mux.read(cx).execute(command);
                    }
                },
                cx,
            ))
            .into_any_element()
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "a grip needs the window, its status row, the float and its frame"
    )]
    fn float_grips(
        &self,
        window: WindowId,
        border_status: PaneBorderStatus,
        float: FloatingPaneSnapshot,
        cell: Size<Pixels>,
        frame: Size<Pixels>,
        pip: bool,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let (edge, near) = if pip {
            (ala::size(px(PIP_GRIP), px(PIP_GRIP)), px(-PIP_GRIP / 2.0))
        } else {
            (cell, Pixels::ZERO)
        };
        let far_x = (frame.width - edge.width - near).max(Pixels::ZERO);
        let far_y = (frame.height - edge.height - near).max(Pixels::ZERO);
        let inner_width = (frame.width - edge.width * 2.0 - near * 2.0).max(Pixels::ZERO);
        let top = near + edge.height;
        let edges = |left, top, right, bottom| FloatGrip {
            left,
            top,
            right,
            bottom,
        };
        [
            (
                "move",
                FloatGrip::MOVE,
                Bounds::new(
                    point(edge.width, Pixels::ZERO),
                    ala::size(inner_width, edge.height),
                ),
                None,
            ),
            (
                "left",
                edges(true, false, false, false),
                Bounds::new(
                    point(near, top),
                    ala::size(edge.width, (far_y - top).max(Pixels::ZERO)),
                ),
                Some(CursorStyle::ResizeLeftRight),
            ),
            (
                "right",
                edges(false, false, true, false),
                Bounds::new(
                    point(far_x, top),
                    ala::size(edge.width, (far_y - top).max(Pixels::ZERO)),
                ),
                Some(CursorStyle::ResizeLeftRight),
            ),
            (
                "top",
                edges(false, true, false, false),
                Bounds::new(
                    point(near + edge.width, near),
                    ala::size(inner_width, edge.height),
                ),
                Some(CursorStyle::ResizeUpDown),
            ),
            (
                "bottom",
                edges(false, false, false, true),
                Bounds::new(
                    point(near + edge.width, far_y),
                    ala::size(inner_width, edge.height),
                ),
                Some(CursorStyle::ResizeUpDown),
            ),
            (
                "top-left",
                edges(true, true, false, false),
                Bounds::new(point(near, near), edge),
                Some(CursorStyle::ResizeUpLeftDownRight),
            ),
            (
                "top-right",
                edges(false, true, true, false),
                Bounds::new(point(far_x, near), edge),
                Some(CursorStyle::ResizeUpRightDownLeft),
            ),
            (
                "bottom-left",
                edges(true, false, false, true),
                Bounds::new(point(near, far_y), edge),
                Some(CursorStyle::ResizeUpRightDownLeft),
            ),
            (
                "bottom-right",
                edges(false, false, true, true),
                Bounds::new(point(far_x, far_y), edge),
                Some(CursorStyle::ResizeUpLeftDownRight),
            ),
        ]
        .into_iter()
        .filter(|(name, ..)| *name != if pip { "move" } else { "top" })
        .map(|(name, grip, bounds, cursor)| {
            div()
                .id(ala::SharedString::from(format!(
                    "floating-grip-{}-{name}",
                    float.pane.0
                )))
                .debug_selector(move || format!("floating-grip-{}-{name}", float.pane.0))
                .absolute()
                .occlude()
                .left(bounds.origin.x)
                .top(bounds.origin.y)
                .w(bounds.size.width)
                .h(bounds.size.height)
                .when_some(cursor, ala::Styled::cursor)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |view, event: &MouseDownEvent, _, cx| {
                        view.begin_float_drag(
                            window,
                            border_status,
                            float,
                            grip,
                            cell,
                            event.position,
                            cx,
                        );
                    }),
                )
                .on_drag(FloatDrag { pane: float.pane }, |_, _, _, cx| {
                    cx.new(|_| SplitDragPreview)
                })
                .into_any_element()
        })
        .collect()
    }

    fn menu_overlay(&self, origin: Point<Pixels>, window: &Window, cx: &App) -> Option<AnyElement> {
        let menu = self.menu.as_ref()?;
        let frame = menu_frame(
            menu.read(cx).state(),
            origin,
            self.pane_canvas_bounds.get().size,
            window.scale_factor(),
        );
        Some(
            anchored()
                .position_mode(AnchoredPositionMode::Local)
                .position(frame.bounds.origin)
                .snap_to_window_with_margin(px(8.0))
                .child(menu.clone())
                .into_any_element(),
        )
    }

    fn confirm_overlay(&self, origin: Point<Pixels>, cx: &App) -> Option<AnyElement> {
        let confirm = self.confirm.as_ref()?;
        let canvas = self.pane_canvas_bounds.get().size;
        let prompt = &confirm.read(cx).state().prompt;
        let width = px((display_width(prompt).saturating_mul(8).saturating_add(32)) as f32)
            .max(px(180.0))
            .min((canvas.width - px(24.0)).max(px(180.0)));
        let height = px(48.0);
        let left = origin.x + ((canvas.width - width) / 2.0).max(Pixels::ZERO);
        let top = origin.y + ((canvas.height - height) / 2.0).max(Pixels::ZERO);
        Some(
            div()
                .absolute()
                .left(left)
                .top(top)
                .w(width)
                .h(height)
                .debug_selector(|| "confirm-before".to_owned())
                .child(FloatingSurface::new(
                    "confirm-before-surface",
                    confirm.clone(),
                    cx,
                ))
                .into_any_element(),
        )
    }

    fn zoom_control(&self, pane: PaneId) -> impl IntoElement {
        let mux = self.mux.clone();
        div()
            .id(("unzoom-pane", pane.0))
            .cursor(CursorStyle::PointingHand)
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_up(MouseButton::Left, move |_, _, cx| {
                mux.read(cx).execute(CommandInvocation::new(
                    "resize-pane",
                    ["-Z", "-t", &pane.to_string()],
                ));
                cx.stop_propagation();
            })
            .child(pane_unzoom_control())
    }
}

impl Render for AppView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let started = diagnostics::timer(DIAGNOSTIC_TARGET);
        self.reconcile_dialog_prefix(window, cx);
        self.synchronize_panes(window, cx);

        let (snapshot, attached, error, prefix_armed, has_hosts) = {
            let mux = self.mux.read(cx);
            (
                mux.snapshot(),
                mux.attached_session(),
                mux.error(),
                mux.prefix_armed(),
                mux.has_hosts(),
            )
        };
        let prompt_visible = self.command_palette.is_some();
        let diagnostic_error = error.clone();
        let active_session = snapshot
            .sessions
            .iter()
            .find(|session| Some(session.id) == attached);
        let active_window = active_session.and_then(|session| {
            let focused_window = snapshot.focused_window_for(session);
            session
                .windows
                .iter()
                .find(|window| window.id == focused_window)
        });
        let (route, chrome, settings_view) = {
            let sidebar = self.sidebar.read(cx);
            let route = sidebar.route();
            let chrome = if route == WorkspaceRoute::Settings {
                ChromeMode::Sidebar
            } else {
                sidebar.mode()
            };
            let settings = (route == WorkspaceRoute::Settings)
                .then(|| sidebar.settings_view())
                .flatten();
            (route, chrome, settings)
        };
        self.reconcile_split_drag(active_window, cx);
        self.reconcile_float_drag(active_window, cx);
        self.reconcile_pane_drag(active_window, prefix_armed, window, cx);
        self.reconcile_pane_layout_override(active_window, snapshot.generation);
        let chrome_above_panes =
            matches!(chrome, ChromeMode::Titlebar) || draws_window_controls(window);
        let layout_corners = match chrome {
            ChromeMode::Sidebar if route == WorkspaceRoute::App => {
                let corners = WindowCorners::for_window(window).right().top();
                if chrome_above_panes {
                    WindowCorners::NONE
                } else {
                    corners
                }
            }
            ChromeMode::Sidebar => {
                let corners = WindowCorners::for_window(window).right();
                if chrome_above_panes {
                    corners.bottom()
                } else {
                    corners
                }
            }
            ChromeMode::Titlebar => WindowCorners::for_window(window).bottom(),
        };
        let predicted_layout = self
            .pane_layout_override
            .as_ref()
            .map(|pending| pending.layout.clone());
        let pane_layout =
            active_window.map(|window| predicted_layout.as_ref().unwrap_or(&window.layout));
        self.synchronize_pane_corners(active_window, pane_layout, layout_corners, cx);
        let pane_drag_armed = prefix_armed
            && active_window
                .is_some_and(|window| window.zoomed_pane.is_none() && window.panes.len() > 1);
        let drag_layer = match self
            .pane_drag
            .as_ref()
            .filter(|_| cx.has_active_drag())
            .map(|drag| drag.source)
        {
            Some(source) => PaneDragLayer::Dragging(source),
            None if pane_drag_armed => PaneDragLayer::Armed,
            None => PaneDragLayer::Idle,
        };
        let drop_preview = self
            .pane_drag
            .as_ref()
            .filter(|_| cx.has_active_drag())
            .and_then(|drag| self.pane_drop_preview_element(drag, cx));
        self.settle_pip_landing(cx);
        if let Some(active_window) = active_window {
            self.pip_stage.observe(active_window);
            self.pane_motion.begin(
                active_window.id,
                window.viewport_size(),
                self.split_drag.is_some(),
            );
        }
        let floating_layer = active_window
            .filter(|active_window| {
                route != WorkspaceRoute::Settings
                    && active_window.floating.iter().any(|float| float.visible)
            })
            .and_then(|active_window| {
                let cell = self.float_cell_size(window, cx);
                let canvas = self.pane_canvas_bounds.get().size;
                self.floating_layer(active_window, cell, canvas, cx)
            });

        if let Some(active_window) = active_window {
            self.pane_arrivals.observe(
                active_window.id,
                active_window.panes.keys().copied(),
                cx.background_executor().now(),
            );
            if self.pane_arrivals.fading() && !cx.reduce_motion() {
                window.request_animation_frame();
            }
        }
        let content = if let Some(settings) = settings_view {
            settings.into_any_element()
        } else if let Some(active_window) = active_window {
            if let Some(pane) = active_window.zoomed_pane {
                self.render_layout(
                    &LayoutNode::Pane(pane),
                    active_window,
                    layout_corners,
                    PaneDragLayer::Idle,
                    cx,
                )
            } else {
                self.render_layout(
                    pane_layout.unwrap_or(&active_window.layout),
                    active_window,
                    layout_corners,
                    drag_layer,
                    cx,
                )
            }
        } else if !has_hosts
            || empty_workspace_available(
                snapshot.generation,
                snapshot.sessions.len(),
                error.is_some(),
            )
        {
            self.new_session.clone().into_any_element()
        } else {
            app_connection_state(
                error.map_or_else(
                    || "connecting to zz daemon…".to_owned(),
                    |error| error.to_string(),
                ),
                cx,
            )
            .into_any_element()
        };
        log::trace!(
            target: "zz::diagnostics::app_render",
            "render window_bounds={:?} scale_factor={} route={route:?} mux_generation={} attached={attached:?} active_session={:?} active_window={:?} pickers={} terminals={} browsers={} agents={} editors={} command_output={} command_prompt={} error={diagnostic_error:?} elapsed_before_tree_us={}",
            window.bounds(),
            window.scale_factor(),
            snapshot.generation,
            active_session.map(|session| session.id),
            active_window.map(|window| window.id),
            self.pickers.len(),
            self.terminals.len(),
            self.browsers.len(),
            self.agents.len(),
            self.editors.len(),
            self.command_output.is_some(),
            prompt_visible,
            diagnostics::elapsed_us(started),
        );

        let pane_margin = if route == WorkspaceRoute::Settings || active_window.is_none() {
            px(0.)
        } else {
            config::pane_margin(cx)
        };
        let canvas_top = match (chrome_above_panes, route) {
            (true, WorkspaceRoute::App) => config::pane_margin(cx),
            (true, WorkspaceRoute::Settings) => px(0.),
            (false, _) => pane_margin,
        };
        let canvas_origin = ala::point(pane_margin, canvas_top);
        let mut overlays = if route == WorkspaceRoute::Settings {
            Vec::new()
        } else {
            [
                self.display_panes
                    .clone()
                    .map(IntoElement::into_any_element),
                self.choose_tree.clone().map(IntoElement::into_any_element),
                self.choose_buffer
                    .clone()
                    .map(IntoElement::into_any_element),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
        };
        if let Some(palette) = &self.command_palette {
            overlays.push(
                AnyView::from(palette.clone())
                    .cached(StyleRefinement::default().absolute().inset_0())
                    .into_any_element(),
            );
        }
        if let Some(menu) = self.menu_overlay(canvas_origin, window, cx) {
            overlays.push(menu);
        }
        if let Some(confirm) = self.confirm_overlay(canvas_origin, cx) {
            overlays.push(confirm);
        }
        if (self.which_key.is_some() || self.which_key_timer.is_some())
            && self.which_key_blocked(window, cx)
        {
            self.which_key_timer = None;
            self.which_key = None;
        }
        if let Some(sheet) = &self.which_key {
            let canvas = self.pane_canvas_bounds.get().size;
            let canvas = if canvas.width > px(0.) && canvas.height > px(0.) {
                canvas
            } else {
                window.viewport_size()
            };
            overlays.push(sheet.element(canvas_top, pane_margin, canvas));
        }
        let measured_canvas_bounds = self.pane_canvas_bounds.clone();
        let content = div().relative().size_full().child(
            div()
                .absolute()
                .left(pane_margin)
                .top(canvas_top)
                .right(pane_margin)
                .bottom(pane_margin)
                .flex()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .min_h_0()
                        .relative()
                        .on_prepaint(move |bounds, _, _| {
                            measured_canvas_bounds.set(bounds);
                        })
                        .on_drag_move::<PaneDrag>(cx.listener(Self::on_pane_drag_move))
                        .on_drag_move::<FloatDrag>(cx.listener(Self::on_float_drag_move))
                        .on_drag_move::<PipGrab>(cx.listener(Self::on_pip_grab_move))
                        .on_drop(cx.listener(|view, drag: &PaneDrag, window, cx| {
                            view.on_pane_drop(*drag, window, cx);
                        }))
                        .child(content)
                        .children(floating_layer)
                        .children(drop_preview),
                ),
        );
        layout_corners.round_div(
            app_workspace_surface("app-root", content, overlays, cx)
                .debug_selector(|| "app-root".into())
                .on_action(
                    cx.listener(|view, _: &crate::menus::OpenCommandPalette, window, cx| {
                        view.open_command_palette(None, window, cx);
                    }),
                )
                .on_action(
                    cx.listener(|view, _: &crate::menus::ChooseWindow, window, cx| {
                        view.open_command_palette(Some(PaletteMode::Target), window, cx);
                    }),
                )
                .capture_any_mouse_down(cx.listener(Self::on_menu_mouse_down))
                .capture_any_mouse_up(cx.listener(Self::on_menu_mouse_up))
                .capture_any_mouse_up(cx.listener(Self::on_split_mouse_up))
                .capture_any_mouse_up(cx.listener(Self::on_float_mouse_up))
                .capture_any_mouse_up(cx.listener(Self::on_pane_mouse_up))
                .on_mouse_exit(cx.listener(Self::on_pane_mouse_exit)),
            frame_content_corner_radius(cx),
        )
    }
}

fn menu_frame(
    state: &MenuState,
    origin: Point<Pixels>,
    canvas: Size<Pixels>,
    scale: f32,
) -> PopupFrame {
    floating_frame(
        state.left,
        state.top,
        state.width,
        state.height,
        state.client_columns,
        state.client_rows,
        state.cell_width_px,
        state.cell_height_px,
        state.border_lines != PopupBorderLines::None,
        origin,
        canvas,
        scale,
    )
}

fn dead_label(snapshot: Option<&zz_protocol::PaneSnapshot>) -> Option<String> {
    snapshot.filter(|pane| pane.dead).map(|pane| {
        pane.dead_status
            .map_or_else(|| "Dead".to_owned(), |status| format!("Dead · {status}"))
    })
}

fn float_cell_size(mux: &MuxClient, window: &Window) -> Size<Pixels> {
    let appearance = mux.appearance();
    zz_ui::terminal::terminal_cell_size(
        &crate::terminal::view::terminal_font(&appearance),
        crate::terminal::view::terminal_font_size(&appearance),
        &appearance,
        window,
    )
}

/// How tall, borders included, Float makes `pane`: the tallest float that fits
/// its window's `canvas`, the panes' area in pixels.
pub(crate) fn pip_fill_rows(
    mux: &MuxClient,
    pane: PaneId,
    canvas: Size<Pixels>,
    window: &Window,
    cx: &App,
) -> Option<u16> {
    let snapshot = mux.snapshot();
    let pane_window = snapshot
        .sessions
        .iter()
        .flat_map(|session| &session.windows)
        .find(|candidate| candidate.panes.contains_key(&pane))?;
    Some(
        PipLayout {
            window: (pane_window.sx, pane_window.sy),
            cell: float_cell_size(mux, window),
            canvas,
            lines: pane_window.pane_border_lines,
            header: cx.theme().pane_header_height(),
        }
        .fill_rows(),
    )
}

fn pane_select_command(pane: PaneId) -> CommandInvocation {
    CommandInvocation::new("select-pane", ["-t", &pane.to_string()])
}

/// Toast tag for a daemon-timed message, so an explicit clear retires exactly
/// the toast that message raised.
fn timed_message_key(message_id: u64) -> String {
    format!("timed-message-{message_id}")
}

fn take_pane_drag(state: &mut Option<PaneDragState>) -> Option<PaneDrag> {
    let state = state.take()?;
    Some(PaneDrag {
        pane: state.source,
        requires_prefix: state.requires_prefix,
    })
}

fn pane_bounds(rect: NormalizedPaneRect, canvas_size: Size<Pixels>) -> Bounds<Pixels> {
    let width = f32::from(canvas_size.width);
    let height = f32::from(canvas_size.height);
    Bounds::new(
        ala::point(px(rect.x * width), px(rect.y * height)),
        ala::size(px(rect.width * width), px(rect.height * height)),
    )
}

fn split_ratio_from_pointer(axis: Axis, pointer: Point<Pixels>, bounds: Bounds<Pixels>) -> f32 {
    let (offset, extent) = match axis {
        Axis::Horizontal => (
            f32::from(pointer.x - bounds.origin.x),
            f32::from(bounds.size.width),
        ),
        Axis::Vertical => (
            f32::from(pointer.y - bounds.origin.y),
            f32::from(bounds.size.height),
        ),
    };
    if extent <= 0.0 {
        return 0.5;
    }
    (offset / extent).clamp(0.0, 1.0)
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the clamped ratio is converted to a bounded protocol fixed-point value"
)]
fn split_ratio_basis(ratio: f32) -> u16 {
    (ratio.clamp(0.0, 1.0) * f32::from(SPLIT_RATIO_BASIS)).round() as u16
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};

    use super::*;
    use crate::terminal::view::GridSize;
    use ala::{Modifiers, TestAppContext, point};
    use zz_client::CoreEvent;
    use zz_protocol::CommandPromptKind;

    ala::actions!(window_shortcut_tests, [WindowShortcutFallback]);

    #[derive(Debug, PartialEq)]
    enum PaneReleaseStep {
        Drop(CommandInvocation),
        Teardown(PaneId),
    }

    struct PaneReleaseOrderPreview;

    impl Render for PaneReleaseOrderPreview {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().w(px(100.0)).h(px(100.0))
        }
    }

    struct PaneReleaseOrderView {
        source: PaneId,
        steps: Rc<RefCell<Vec<PaneReleaseStep>>>,
    }

    impl Render for PaneReleaseOrderView {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let source = self.source;
            let target = PaneId(7);
            let drop_steps = self.steps.clone();
            div()
                .flex()
                .w(px(200.0))
                .h(px(100.0))
                .child(
                    div()
                        .id("pane-release-order-source")
                        .w(px(100.0))
                        .h(px(100.0))
                        .on_drag(
                            PaneDrag {
                                pane: source,
                                requires_prefix: true,
                            },
                            |_, _, _, cx| cx.new(|_| PaneReleaseOrderPreview),
                        ),
                )
                .child(
                    div()
                        .id("pane-release-order-target")
                        .w(px(100.0))
                        .h(px(100.0))
                        .on_drop::<PaneDrag>(move |drag, _, _| {
                            drop_steps.borrow_mut().push(PaneReleaseStep::Drop(
                                pane_drop_command(drag.pane, target, DropZone::Center)
                                    .expect("different pane ids produce a swap"),
                            ));
                        }),
                )
                .capture_any_mouse_up(cx.listener(|_view, event: &MouseUpEvent, window, cx| {
                    if event.button == MouseButton::Left {
                        cx.defer_in(window, |view, _, _| {
                            view.steps
                                .borrow_mut()
                                .push(PaneReleaseStep::Teardown(view.source));
                        });
                    }
                }))
        }
    }

    #[test]
    fn empty_workspace_requires_a_real_snapshot_without_an_error() {
        assert!(!empty_workspace_available(0, 0, false));
        assert!(!empty_workspace_available(4, 1, false));
        assert!(!empty_workspace_available(4, 0, true));
        assert!(empty_workspace_available(4, 0, false));
    }

    #[test]
    fn the_empty_workspace_focus_debt_outlives_a_pass_that_never_pays_it() {
        let became_visible = empty_workspace_focus_owed(true, false, false);
        assert!(became_visible);
        assert!(empty_workspace_focus_owed(true, true, became_visible));
        assert!(!empty_workspace_focus_owed(true, true, false));
        assert!(!empty_workspace_focus_owed(false, true, true));
    }

    #[test]
    fn the_revision_carries_the_window_the_focus_passes_resolve() {
        let pane = PaneId(1);
        let window = |id: WindowId| WindowSnapshot {
            id,
            index: u32::try_from(id.0).expect("fixture index"),
            name: id.to_string(),
            automatic_rename: true,
            active_pane: pane,
            zoomed_pane: None,
            layout: LayoutNode::Pane(pane),
            panes: BTreeMap::new(),
            layout_dump: String::new(),
            visible_layout_dump: String::new(),
            status_label: String::new(),
            activity: false,
            silence: false,
            pane_border_status: zz_protocol::PaneBorderStatus::Off,
            pane_border_lines: zz_protocol::PaneBorderLines::Single,
            pane_border_indicators: zz_protocol::PaneBorderIndicators::Colour,
            pane_order: Vec::new(),
            pane_z_order: Vec::new(),
            floating: Vec::new(),
            modal: None,
            sx: 0,
            sy: 0,
            pane_base_index: 0,
        };
        let attached = SessionId(1);
        let session = zz_protocol::SessionSnapshot {
            id: attached,
            name: "zz".to_owned(),
            active_window: WindowId(1),
            windows: vec![window(WindowId(1)), window(WindowId(2))],
            viewers: Vec::new(),
        };
        let snapshot = MuxSnapshot {
            generation: 4,
            sessions: vec![session],
            focused_window: Some(WindowId(2)),
        };
        assert_eq!(
            attached_focused_window(&snapshot, Some(attached)),
            Some(WindowId(2))
        );
        assert_eq!(attached_focused_window(&snapshot, None), None);
        assert_eq!(
            attached_focused_window(
                &MuxSnapshot {
                    focused_window: None,
                    ..snapshot.clone()
                },
                Some(attached)
            ),
            Some(WindowId(1))
        );

        let revision = AppRevision {
            snapshot_generation: snapshot.generation,
            attached_host: HostId::LOCAL,
            attached: Some(attached),
            focused_window: attached_focused_window(&snapshot, Some(attached)),
            error: None,
            command_output_pane: None,
            menu: 0,
            confirm: 0,
            command_prompt: 0,
            choose_tree: 0,
            choose_buffer: 0,
            display_panes: 0,
            prefix_armed: false,
            prefix_cancelled_request: None,
            sidebar_focus: 0,
            bell: 0,
            pending_commands: 0,
        };
        assert_ne!(
            revision,
            AppRevision {
                focused_window: Some(WindowId(1)),
                ..revision.clone()
            }
        );
    }

    #[test]
    fn browser_metadata_uses_tmux_pane_titles_only() {
        let pane = PaneId(7);
        let session = zz_browser::SessionId(11);
        assert_eq!(
            browser_metadata_command(
                pane,
                &zz_browser::BrowserEvent::AddressChanged {
                    session,
                    url: Arc::from("https://example.com/path"),
                },
            ),
            None
        );
        assert_eq!(
            browser_metadata_command(
                pane,
                &zz_browser::BrowserEvent::TitleChanged {
                    session,
                    title: Arc::from("Example Domain"),
                },
            ),
            Some(CommandInvocation::new(
                "select-pane",
                ["-t", "%7", "-T", "Example Domain"],
            ))
        );
        assert_eq!(
            browser_metadata_command(
                pane,
                &zz_browser::BrowserEvent::FrameReady {
                    session,
                    generation: 1,
                },
            ),
            None
        );
    }

    fn three_pane_layout() -> LayoutNode {
        LayoutNode::Split {
            id: SplitId(1),
            axis: Axis::Horizontal,
            ratio: 0.4,
            first: Box::new(LayoutNode::Pane(PaneId(3))),
            second: Box::new(LayoutNode::Split {
                id: SplitId(2),
                axis: Axis::Vertical,
                ratio: 0.5,
                first: Box::new(LayoutNode::Pane(PaneId(7))),
                second: Box::new(LayoutNode::Pane(PaneId(9))),
            }),
        }
    }

    fn three_pane_drag(source: PaneId) -> PaneDragState {
        PaneDragState::new(
            source,
            WindowId(5),
            &three_pane_layout(),
            ala::size(px(1_000.0), px(800.0)),
            px(8.0),
        )
        .expect("source belongs to the frozen layout")
    }

    #[test]
    fn pane_drag_state_freezes_slots_and_rejects_foreign_layouts() {
        let layout = three_pane_layout();
        let canvas_size = ala::size(px(1_000.0), px(800.0));
        let frozen_slots = pane_rects(&layout)
            .into_iter()
            .map(|(pane, rect)| (pane, pane_bounds(rect, canvas_size)))
            .collect::<Vec<_>>();
        let drag = three_pane_drag(PaneId(3));

        assert_eq!(drag.slots, frozen_slots);
        assert_eq!(
            drag.slot(PaneId(3)),
            Some(Bounds::new(
                point(px(0.0), px(0.0)),
                ala::size(px(400.0), px(800.0)),
            ))
        );
        assert_eq!(
            drag.slot(PaneId(7)),
            Some(Bounds::new(
                point(px(400.0), px(0.0)),
                ala::size(px(600.0), px(400.0)),
            ))
        );
        assert!(drag.matches_layout(WindowId(5), &layout));
        assert!(!drag.matches_layout(WindowId(6), &layout));
        let changed_layout = LayoutNode::Split {
            id: SplitId(1),
            axis: Axis::Horizontal,
            ratio: 0.6,
            first: Box::new(LayoutNode::Pane(PaneId(3))),
            second: Box::new(LayoutNode::Split {
                id: SplitId(2),
                axis: Axis::Vertical,
                ratio: 0.5,
                first: Box::new(LayoutNode::Pane(PaneId(7))),
                second: Box::new(LayoutNode::Pane(PaneId(9))),
            }),
        };
        assert!(!drag.matches_layout(WindowId(5), &changed_layout));
    }

    #[test]
    fn pointer_targets_skip_the_source_and_carry_the_hovered_zone() {
        let drag = three_pane_drag(PaneId(3));

        assert_eq!(drag.target_at(point(px(200.0), px(400.0))), None);
        assert_eq!(drag.target_at(point(px(2_000.0), px(400.0))), None);
        assert_eq!(
            drag.target_at(point(px(950.0), px(200.0))),
            Some((PaneId(7), DropZone::Right))
        );
        assert_eq!(
            drag.target_at(point(px(700.0), px(40.0))),
            Some((PaneId(7), DropZone::Top))
        );
        assert_eq!(
            drag.target_at(point(px(450.0), px(600.0))),
            Some((PaneId(9), DropZone::Left))
        );
        let stacked = three_pane_drag(PaneId(7));
        assert_eq!(
            stacked.target_at(point(px(700.0), px(440.0))),
            Some((PaneId(9), DropZone::Center))
        );
    }

    #[test]
    fn drop_previews_halve_the_target_slot_and_morph_from_what_is_on_screen() {
        let mut drag = three_pane_drag(PaneId(3));
        let blank = DropPreviewFrame::default();

        assert!(!drag.set_target(Some((PaneId(3), DropZone::Center)), blank));
        assert!(!drag.set_target(Some((PaneId(11), DropZone::Center)), blank));
        assert!(drag.set_target(Some((PaneId(7), DropZone::Right)), blank));

        let right_half = Bounds::new(point(px(712.0), px(0.0)), ala::size(px(288.0), px(400.0)));
        let preview = drag.preview.expect("a target shows a preview");
        assert_eq!(preview.to.bounds, right_half);
        assert_eq!(preview.to.opacity.to_bits(), 1.0_f32.to_bits());
        assert_eq!(preview.from.bounds, right_half);
        assert_eq!(preview.from.opacity.to_bits(), 0.0_f32.to_bits());
        assert_eq!(preview.duration, DROP_PREVIEW_MORPH);
        assert!(!drag.set_target(Some((PaneId(7), DropZone::Right)), blank));

        let mid_morph = DropPreviewFrame {
            bounds: Bounds::new(point(px(640.0), px(20.0)), ala::size(px(320.0), px(380.0))),
            opacity: 0.6,
        };
        assert!(drag.set_target(Some((PaneId(9), DropZone::Center)), mid_morph));
        let preview = drag.preview.expect("a target shows a preview");
        assert_eq!(preview.from, mid_morph);
        assert_eq!(
            preview.to.bounds,
            Bounds::new(point(px(408.0), px(408.0)), ala::size(px(592.0), px(392.0)))
        );
        assert_eq!(preview.sequence, 1);

        let showing = DropPreviewFrame {
            bounds: preview.to.bounds,
            opacity: 1.0,
        };
        assert!(drag.set_target(None, showing));
        let preview = drag.preview.expect("the fade-out is still a preview");
        assert_eq!(preview.from, showing);
        assert_eq!(preview.to.bounds, showing.bounds);
        assert_eq!(preview.to.opacity.to_bits(), 0.0_f32.to_bits());
        assert_eq!(preview.duration, DROP_PREVIEW_FADE);
        assert_eq!(preview.at(0.5).opacity.to_bits(), 0.5_f32.to_bits());
    }

    fn one_pane_snapshot(generation: u64) -> MuxSnapshot {
        let terminal = PaneId(0);
        let window = WindowSnapshot {
            id: WindowId(0),
            index: 0,
            name: "zz".to_owned(),
            automatic_rename: true,
            active_pane: terminal,
            zoomed_pane: None,
            layout: LayoutNode::Pane(terminal),
            panes: [(
                terminal,
                zz_protocol::PaneSnapshot {
                    id: terminal,
                    title: terminal.to_string(),
                    kind: PaneKindSnapshot::Terminal,
                    synchronized_input: false,
                    bell: false,
                    dead: false,
                    dead_status: None,
                    border_colour: None,
                    active_border_colour: None,
                    border_status_text: String::new(),
                    mode: None,
                    status: None,
                    summary: zz_protocol::PaneSummary::default(),
                },
            )]
            .into_iter()
            .collect(),
            layout_dump: String::new(),
            visible_layout_dump: String::new(),
            status_label: String::new(),
            activity: false,
            silence: false,
            pane_border_status: zz_protocol::PaneBorderStatus::Off,
            pane_border_lines: zz_protocol::PaneBorderLines::Single,
            pane_border_indicators: zz_protocol::PaneBorderIndicators::Colour,
            pane_order: Vec::new(),
            pane_z_order: Vec::new(),
            floating: Vec::new(),
            modal: None,
            sx: 0,
            sy: 0,
            pane_base_index: 0,
        };
        MuxSnapshot {
            generation,
            focused_window: Some(WindowId(0)),
            sessions: vec![zz_protocol::SessionSnapshot {
                id: SessionId(0),
                name: "zz".to_owned(),
                active_window: WindowId(0),
                windows: vec![window],
                viewers: Vec::new(),
            }],
        }
    }

    fn menu_state_for_test() -> MenuState {
        MenuState {
            left: 29,
            top: 6,
            width: 20,
            height: 4,
            client_columns: 80,
            client_rows: 24,
            cell_width_px: 8,
            cell_height_px: 18,
            title: "menu".to_owned(),
            style: "bg=themedarkgrey,fg=themewhite".to_owned(),
            selected_style: "bg=themeyellow,fg=themeblack".to_owned(),
            border_style: "bg=themedarkgrey,fg=themelightgrey".to_owned(),
            border_lines: PopupBorderLines::Single,
            items: vec![Some(zz_protocol::MenuItem {
                name: "Quit item".to_owned(),
                key: Some("q".to_owned()),
                annotation: Some("q".to_owned()),
                enabled: true,
            })],
            selected: Some(0),
            stay_open: false,
            mouse_keys: false,
        }
    }

    /// Every overlay payload the daemon can publish, and the workspace consumer
    /// each one must find.
    ///
    /// The exhaustive match is the point: a `CoreEvent` variant added later
    /// cannot compile until someone says whether it raises an overlay, and a
    /// kind that says it does has to name an element the matrix can find on the
    /// drawn window. Cycle 11's silently destructive desktop chooser was a
    /// payload that reached a no-op arm; nothing under the campaign ran the
    /// ala client, so nothing caught it.
    fn overlay_selector(event: &CoreEvent) -> Option<&'static str> {
        match event {
            CoreEvent::CommandPromptChanged => Some("command-palette-overlay"),
            CoreEvent::ChooseTreeChanged => Some("choose-tree-overlay"),
            CoreEvent::ChooseBufferChanged => Some("choose-buffer-overlay"),
            CoreEvent::DisplayPanesChanged => Some("display-panes-input"),
            CoreEvent::MenuChanged => Some("display-menu"),
            CoreEvent::ConfirmChanged => Some("confirm-before"),
            CoreEvent::HelloReceived
            | CoreEvent::Attached { .. }
            | CoreEvent::SnapshotChanged
            | CoreEvent::ViewportChanged { .. }
            | CoreEvent::AppearanceChanged
            | CoreEvent::MuxOptionsChanged
            | CoreEvent::KeyTablesChanged
            | CoreEvent::StatusChanged
            | CoreEvent::PrefixArmed { .. }
            | CoreEvent::PrefixCancelled { .. }
            | CoreEvent::KeyTableChanged
            | CoreEvent::CommandOutputChanged
            | CoreEvent::PaneRemoved { .. }
            | CoreEvent::Bell { .. }
            | CoreEvent::FocusSidebar
            | CoreEvent::OpenPathPicker { .. }
            | CoreEvent::Detached { .. }
            | CoreEvent::ServerStopping
            | CoreEvent::CommandResponse(_)
            | CoreEvent::ClientMessage { .. }
            | CoreEvent::ClientMessageCleared { .. }
            | CoreEvent::Clipboard { .. }
            | CoreEvent::OpenUri { .. }
            | CoreEvent::BrowserCommand { .. }
            | CoreEvent::TerminalUiCommand { .. }
            | CoreEvent::HistoryChunk { .. }
            | CoreEvent::KittyImageBegin { .. }
            | CoreEvent::KittyImageChunk { .. }
            | CoreEvent::KittyImagesRemoved { .. }
            | CoreEvent::AgentUpdates { .. }
            | CoreEvent::AgentStateChanged { .. }
            | CoreEvent::AgentLagged { .. }
            | CoreEvent::AgentSessions { .. }
            | CoreEvent::ProvidersChanged
            | CoreEvent::Message(_) => None,
        }
    }

    fn command_prompt_state_for_test() -> zz_protocol::CommandPromptState {
        zz_protocol::CommandPromptState {
            prompt: ":".to_owned(),
            input: String::new(),
            cursor: 0,
            kind: CommandPromptKind::Command,
            history: Vec::new(),
            prompt_type: zz_protocol::CommandPromptType::Command,
            mode: zz_protocol::CommandPromptMode::Text,
            no_freeze: false,
            pane: None,
            command_mode: false,
            prompt_cursor: zz_protocol::PromptCursor::default(),
        }
    }

    fn choose_tree_state_for_test() -> zz_protocol::ChooseTreeState {
        let item = |label: &str, target| zz_protocol::ChooseTreeItem {
            label: label.to_owned(),
            detail: String::new(),
            target,
            depth: 0,
            flags: 0,
            pane_kind: None,
            key: String::new(),
            text: String::new(),
        };
        zz_protocol::ChooseTreeState {
            items: vec![
                item(
                    "session",
                    zz_protocol::ChooseTreeTarget::Session(SessionId(0)),
                ),
                item("window", zz_protocol::ChooseTreeTarget::Window(WindowId(0))),
            ],
            search: None,
            selected: 0,
            kind: zz_protocol::ChooseTreeKind::Panes,
            filter_no_matches: false,
            prompt: String::new(),
            help: false,
        }
    }

    fn choose_buffer_state_for_test() -> zz_protocol::ChooseBufferState {
        let item = |name: &str| zz_protocol::ChooseBufferItem {
            name: name.to_owned(),
            preview: "preview".to_owned(),
            size_bytes: 7,
            created_unix_seconds: 0,
            key: String::new(),
            text: String::new(),
            tagged: false,
        };
        zz_protocol::ChooseBufferState {
            items: vec![item("buffer0"), item("buffer1")],
            search: None,
            selected: 0,
            filter_no_matches: false,
            help: false,
            prompt: String::new(),
        }
    }

    fn display_panes_state_for_test() -> zz_protocol::DisplayPanesState {
        zz_protocol::DisplayPanesState {
            window: WindowId(0),
            duration_ms: 1000,
            indicators: vec![zz_protocol::PaneIndicator {
                pane: PaneId(0),
                index: 0,
                select_key: b'0',
                flags: zz_protocol::PaneIndicator::ACTIVE,
                label: "0".to_owned(),
            }],
            colour: None,
            active_colour: None,
        }
    }

    #[cfg(unix)]
    struct InputTestServer {
        stream: std::os::unix::net::UnixStream,
        thread: Option<std::thread::JoinHandle<()>>,
        _directory: tempfile::TempDir,
    }

    #[cfg(unix)]
    impl Drop for InputTestServer {
        fn drop(&mut self) {
            let _ = self.stream.shutdown(std::net::Shutdown::Both);
            self.thread.take().unwrap().join().unwrap();
        }
    }

    #[cfg(unix)]
    fn input_test_client() -> Option<(zz_daemon_client::InteractiveClient, InputTestServer)> {
        use zz_protocol::{ProtocolMessage, read_protocol_message, write_protocol_message};

        let directory = tempfile::Builder::new()
            .prefix("zz-input-")
            .tempdir_in("/tmp")
            .unwrap();
        let socket = directory.path().join("s");
        let listener = match std::os::unix::net::UnixListener::bind(&socket) {
            Ok(listener) => listener,
            Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
                eprintln!(
                    "SKIPPED: input routing fixture requires a Unix protocol socket: {error}"
                );
                return None;
            }
            Err(error) => panic!("bind input routing fixture: {error}"),
        };
        let (sender, receiver) = std::sync::mpsc::channel();
        let thread = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            assert!(matches!(
                read_protocol_message(&mut stream).unwrap(),
                ProtocolMessage::ClientHello(_) | ProtocolMessage::Hello(_)
            ));
            sender.send(stream.try_clone().unwrap()).unwrap();
            let mut mux_options = zz_protocol::MuxOptions::default();
            mux_options.set(
                zz_protocol::MuxOptionKey::Prefix,
                "C-a",
                zz_protocol::MuxOptionSource::RuntimeCommand,
            );
            write_protocol_message(
                &mut stream,
                &ProtocolMessage::ServerHello(Box::new(zz_protocol::ServerHello {
                    protocol_version: PROTOCOL_VERSION,
                    server_id: 1,
                    client_id: zz_protocol::ClientId(1),
                    client_instance_id: zz_protocol::ClientInstanceId(1),
                    capabilities: vec![
                        zz_protocol::PANE_FRAME_CAPABILITY.to_owned(),
                        zz_protocol::CONTROL_CAPABILITY.to_owned(),
                    ],
                    appearance: zz_terminal::TerminalAppearance::default(),
                    appearance_provenance: zz_terminal::AppearanceProvenance::default(),
                    mux_options,
                    status: zz_protocol::StatusLine::default(),
                    key_tables: zz_protocol::KeyTables::default().snapshot(),
                    clock: 0,
                })),
            )
            .unwrap();
            while read_protocol_message(&mut stream).is_ok() {}
        });
        let client = zz_daemon_client::InteractiveClient::connect(&socket).unwrap();
        let stream = receiver.recv().unwrap();
        Some((
            client,
            InputTestServer {
                stream,
                thread: Some(thread),
                _directory: directory,
            },
        ))
    }

    #[cfg(unix)]
    fn input_test_workspace(
        cx: &mut TestAppContext,
        client: zz_daemon_client::InteractiveClient,
    ) -> (Entity<AppView>, &mut ala::VisualTestContext) {
        cx.update(zz_ui::init);
        let workspace_slot = Rc::new(RefCell::new(None));
        let captured_workspace = Rc::clone(&workspace_slot);
        let (_, cx) = cx.add_window_view(move |window, cx| {
            let controller = cx.new(|cx| {
                BrowserController::new(Err(zz_browser::BrowserError::AlreadyShutdown), cx)
            });
            let agent_controller = cx.new(|_| AgentController::new());
            let mux = cx
                .new(|cx| MuxClient::new(Ok(client), zz_daemon_client::default_socket_path(), cx));
            let workspace = cx.new(|cx| {
                AppView::new(
                    controller.clone(),
                    agent_controller.clone(),
                    mux,
                    window,
                    cx,
                )
            });
            captured_workspace.replace(Some(workspace.clone()));
            let shell = cx.new(|cx| {
                crate::app_shell::AppShell::new(workspace, controller, agent_controller, cx)
            });
            crate::build_root(shell, window, cx)
        });
        let workspace = workspace_slot.borrow().clone().unwrap();
        let mux = workspace.read_with(cx, |workspace, _| workspace.mux.clone());
        mux.update(cx, |mux, cx| {
            mux.attach_snapshot_for_test(SessionId(0), one_pane_snapshot(1), cx);
        });
        cx.run_until_parked();
        workspace.update_in(cx, |workspace, window, cx| {
            workspace.focus_active_pane(window, cx);
        });
        (workspace, cx)
    }

    #[cfg(unix)]
    #[ala::test]
    fn panes_keep_the_gap_above_them_inside_the_workspace_under_the_titlebar(
        cx: &mut TestAppContext,
    ) {
        cx.executor().allow_parking();
        let Some((client, _server)) = input_test_client() else {
            return;
        };
        let (workspace, cx) = input_test_workspace(cx, client);
        let gap = px(8.);
        cx.update(|_, cx| {
            let mut config = config::AppConfig::default();
            config.pane_gaps.value = true;
            config.pane_margin.value = 8.;
            cx.set_global(config);
            crate::theme::refresh_current_theme(cx);
            cx.set_reduce_motion(true);
        });
        let sidebar = workspace.read_with(cx, |workspace, _| workspace.sidebar.clone());
        for titlebar in [false, true] {
            if titlebar {
                sidebar.update(cx, WorkspaceSidebar::toggle_mode);
            }
            cx.run_until_parked();
            cx.update(|window, cx| {
                _ = window.draw(cx);
            });
            let root = cx.debug_bounds("app-root").expect("the workspace is drawn");
            let canvas = workspace.read_with(cx, |workspace, _| workspace.pane_canvas_bounds.get());
            assert_eq!(canvas.top() - root.top(), gap, "titlebar {titlebar}");
            assert_eq!(canvas.left() - root.left(), gap, "titlebar {titlebar}");
            if titlebar {
                let bar = cx.update(|_, cx| cx.theme().title_bar_height());
                assert_eq!(canvas.top(), bar, "the panes start where the titlebar ends");
            }
        }
    }

    #[cfg(unix)]
    #[ala::test]
    fn ui_chord_bound_to_the_prefix_opens_palette_without_arming(cx: &mut TestAppContext) {
        cx.executor().allow_parking();
        let Some((client, _server)) = input_test_client() else {
            return;
        };
        let (workspace, cx) = input_test_workspace(cx, client);
        let mux = workspace.read_with(cx, |workspace, _| workspace.mux.clone());
        let input = mux.update(cx, |mux, _| mux.record_input_for_test());
        cx.update(|_, cx| {
            crate::keymap::install(
                &[crate::keymap::ChromeOverride::Bind {
                    table: zz_client::UI_TABLE,
                    key: "C-a".to_owned(),
                    action: ChromeAction::OpenCommandPalette.name().to_owned(),
                }],
                config::DEFAULT_BROWSER_ELEMENT_SELECTOR_HOTKEY,
                cx,
            );
        });
        let stroke = Keystroke::parse("ctrl-a").unwrap();
        assert!(mux.read_with(cx, |mux, _| {
            mux.claims_prefix_input(&terminal_key_input(&stroke, TerminalKeyAction::Press))
        }));
        cx.simulate_keystrokes("ctrl-a");
        cx.simulate_event(KeyUpEvent { keystroke: stroke });
        cx.run_until_parked();
        assert!(workspace.read_with(cx, |workspace, _| workspace.command_palette.is_some()));
        assert!(!mux.read_with(cx, |mux, _| mux.prefix_armed()));
        assert!(
            !input
                .borrow()
                .iter()
                .any(|message| matches!(message, InputMessage::Key { .. }))
        );
    }

    #[cfg(unix)]
    #[ala::test]
    fn claimed_press_and_release_reach_mux_without_terminal_key_down(cx: &mut TestAppContext) {
        cx.executor().allow_parking();
        let Some((client, _server)) = input_test_client() else {
            return;
        };
        let (workspace, cx) = input_test_workspace(cx, client);
        let (mux, terminal) = workspace.read_with(cx, |workspace, _| {
            (
                workspace.mux.clone(),
                workspace.terminals[&PaneId(0)].clone(),
            )
        });
        assert!(cx.update(|window, cx| terminal.read(cx).focus().is_focused(window)));
        let input = mux.update(cx, |mux, _| mux.record_input_for_test());
        let stroke = Keystroke::parse("ctrl-a").unwrap();
        cx.simulate_event(ala::KeyDownEvent {
            keystroke: stroke.clone(),
            is_held: false,
            prefer_character_input: false,
        });
        cx.simulate_event(KeyUpEvent {
            keystroke: stroke.clone(),
        });
        assert_eq!(
            input.borrow().as_slice(),
            &[
                InputMessage::Key {
                    pane: PaneId(0),
                    input: terminal_key_input(&stroke, TerminalKeyAction::Press),
                    text_follows: false
                },
                InputMessage::Key {
                    pane: PaneId(0),
                    input: terminal_key_input(&stroke, TerminalKeyAction::Release),
                    text_follows: false
                },
            ]
        );
        assert_eq!(
            workspace.read_with(cx, |workspace, _| workspace.input_router.owner()),
            zz_client::InputOwner::None
        );
    }

    #[cfg(unix)]
    #[ala::test]
    fn standalone_modifier_tap_leaves_the_armed_prefix_waiting(cx: &mut TestAppContext) {
        cx.executor().allow_parking();
        let Some((client, _server)) = input_test_client() else {
            return;
        };
        let (workspace, cx) = input_test_workspace(cx, client);
        let mux = workspace.read_with(cx, |workspace, _| workspace.mux.clone());
        mux.update(cx, |mux, cx| mux.set_prefix_armed_for_test(true, cx));
        let input = mux.update(cx, |mux, _| mux.record_input_for_test());
        cx.simulate_modifiers_change(Modifiers::shift());
        cx.simulate_modifiers_change(Modifiers::none());
        cx.run_until_parked();
        assert!(
            !input
                .borrow()
                .iter()
                .any(|message| matches!(message, InputMessage::Key { .. }))
        );
        assert!(mux.read_with(cx, |mux, _| mux.prefix_armed()));
    }

    #[cfg(unix)]
    fn publish_key_table(
        mux: &Entity<MuxClient>,
        table: Option<&str>,
        repeat: bool,
        cx: &mut ala::VisualTestContext,
    ) {
        mux.update(cx, |mux, cx| {
            mux.handle_message_for_test(
                zz_protocol::ProtocolMessage::Event(zz_protocol::Event {
                    sequence: 0,
                    payload: zz_protocol::EventPayload::KeyTableActive {
                        table: table.map(str::to_owned),
                        repeat,
                    },
                }),
                cx,
            );
        });
        cx.run_until_parked();
    }

    #[cfg(unix)]
    fn which_key_visible(workspace: &Entity<AppView>, cx: &mut ala::VisualTestContext) -> bool {
        workspace.read_with(cx, |workspace, _| workspace.which_key.is_some())
    }

    #[cfg(unix)]
    fn wait_for_which_key(cx: &mut ala::VisualTestContext, milliseconds: u64) {
        cx.executor()
            .advance_clock(Duration::from_millis(milliseconds));
        cx.run_until_parked();
    }

    #[cfg(unix)]
    #[ala::test]
    fn which_key_shows_after_the_prefix_pause_and_hides_on_the_next_key(cx: &mut TestAppContext) {
        cx.executor().allow_parking();
        let Some((client, _server)) = input_test_client() else {
            return;
        };
        let (workspace, cx) = input_test_workspace(cx, client);
        let mux = workspace.read_with(cx, |workspace, _| workspace.mux.clone());
        let input = mux.update(cx, |mux, _| mux.record_input_for_test());
        let stroke = Keystroke::parse("ctrl-a").unwrap();
        cx.simulate_event(ala::KeyDownEvent {
            keystroke: stroke.clone(),
            is_held: false,
            prefer_character_input: false,
        });
        publish_key_table(&mux, Some("prefix"), false, cx);
        cx.simulate_event(KeyUpEvent {
            keystroke: stroke.clone(),
        });
        cx.run_until_parked();
        assert_eq!(
            input.borrow().as_slice(),
            &[
                InputMessage::Key {
                    pane: PaneId(0),
                    input: terminal_key_input(&stroke, TerminalKeyAction::Press),
                    text_follows: false
                },
                InputMessage::Key {
                    pane: PaneId(0),
                    input: terminal_key_input(&stroke, TerminalKeyAction::Release),
                    text_follows: false
                },
            ]
        );
        wait_for_which_key(cx, 399);
        assert!(!which_key_visible(&workspace, cx));
        wait_for_which_key(cx, 2);
        assert!(which_key_visible(&workspace, cx));
        workspace.read_with(cx, |workspace, _| {
            let sheet = workspace.which_key.as_ref().unwrap();
            assert_eq!(sheet.header().table.as_ref(), "prefix");
            assert_eq!(sheet.header().prefix_raw.as_ref(), "C-a");
            assert!(sheet.header().prefix.is_some());
            let split = sheet
                .rows()
                .iter()
                .find(|row| row.id.as_ref() == "%")
                .unwrap();
            assert_eq!(split.group.as_deref(), Some("Panes"));
            assert!(!split.caps[0].keys.is_empty());
            assert!(!split.caps[0].yours);
            assert!(sheet.rows().iter().all(|row| row.id.as_ref() != "M-1"));
            assert_eq!(
                sheet.header().more.as_ref().map(|more| more.raw.as_ref()),
                Some("?")
            );
        });
        assert!(workspace.read_with(cx, |workspace, _| workspace.focused_overlay.is_none()));
        let sheet = cx.debug_bounds("which-key").unwrap();
        let viewport = cx.update(|window, _| window.viewport_size());
        assert!(sheet.bottom() <= viewport.height);
        assert!(sheet.bottom() > viewport.height - px(40.0));
        assert!(sheet.top() >= px(0.));
        assert!(sheet.left() >= px(16.0));
        let canvas = workspace.read_with(cx, |workspace, _| workspace.pane_canvas_bounds.get());
        assert!(sheet.top() >= canvas.top(), "{sheet:?} {canvas:?}");
        let terminal =
            workspace.read_with(cx, |workspace, _| workspace.terminals[&PaneId(0)].clone());
        assert!(cx.update(|window, cx| terminal.read(cx).focus().is_focused(window)));
        cx.simulate_keystrokes("c");
        assert!(!which_key_visible(&workspace, cx));
        assert!(
            input
                .borrow()
                .iter()
                .any(|message| matches!(message, InputMessage::Key { input, .. } if input.text.as_deref() == Some("c")))
        );
        assert!(
            !input
                .borrow()
                .iter()
                .any(|message| matches!(message, InputMessage::CancelPrefix { .. }))
        );
    }

    #[cfg(unix)]
    #[ala::test]
    fn a_held_prefix_key_keeps_the_which_key_timer(cx: &mut TestAppContext) {
        cx.executor().allow_parking();
        let Some((client, _server)) = input_test_client() else {
            return;
        };
        let (workspace, cx) = input_test_workspace(cx, client);
        let mux = workspace.read_with(cx, |workspace, _| workspace.mux.clone());
        mux.update(cx, |mux, cx| mux.set_prefix_armed_for_test(true, cx));
        publish_key_table(&mux, Some("prefix"), false, cx);
        wait_for_which_key(cx, 200);
        cx.simulate_event(ala::KeyDownEvent {
            keystroke: Keystroke::parse("ctrl-a").unwrap(),
            is_held: true,
            prefer_character_input: false,
        });
        cx.run_until_parked();
        wait_for_which_key(cx, 201);
        assert!(which_key_visible(&workspace, cx));
    }

    #[cfg(unix)]
    #[ala::test]
    fn which_key_hides_for_a_repeat_window_and_a_cleared_table(cx: &mut TestAppContext) {
        cx.executor().allow_parking();
        let Some((client, _server)) = input_test_client() else {
            return;
        };
        let (workspace, cx) = input_test_workspace(cx, client);
        let mux = workspace.read_with(cx, |workspace, _| workspace.mux.clone());
        publish_key_table(&mux, Some("prefix"), false, cx);
        wait_for_which_key(cx, 401);
        assert!(which_key_visible(&workspace, cx));
        publish_key_table(&mux, Some("prefix"), true, cx);
        assert!(!which_key_visible(&workspace, cx));
        wait_for_which_key(cx, 2_000);
        assert!(!which_key_visible(&workspace, cx));
        publish_key_table(&mux, Some("prefix"), false, cx);
        wait_for_which_key(cx, 200);
        publish_key_table(&mux, None, false, cx);
        wait_for_which_key(cx, 2_000);
        assert!(!which_key_visible(&workspace, cx));
        assert!(workspace.read_with(cx, |workspace, _| workspace.which_key_timer.is_none()));
        publish_key_table(&mux, Some("prefix"), false, cx);
        wait_for_which_key(cx, 401);
        assert!(which_key_visible(&workspace, cx));
        workspace.update_in(cx, |workspace, window, cx| {
            workspace.open_command_palette(None, window, cx);
        });
        cx.run_until_parked();
        assert!(!which_key_visible(&workspace, cx));
        assert!(workspace.read_with(cx, |workspace, _| {
            workspace.focused_overlay == Some(OverlayKind::CommandPalette)
        }));
        publish_key_table(&mux, Some("prefix"), false, cx);
        wait_for_which_key(cx, 401);
        assert!(!which_key_visible(&workspace, cx));
    }

    #[cfg(unix)]
    #[ala::test]
    fn which_key_lists_a_custom_table_flat_and_hides_on_its_keys(cx: &mut TestAppContext) {
        cx.executor().allow_parking();
        let Some((client, _server)) = input_test_client() else {
            return;
        };
        let (workspace, cx) = input_test_workspace(cx, client);
        let mux = workspace.read_with(cx, |workspace, _| workspace.mux.clone());
        let mut tables = zz_protocol::KeyTables::default();
        for (key, direction) in [("h", "-L"), ("l", "-R")] {
            tables.bind(
                "resize",
                key,
                zz_protocol::Binding {
                    commands: vec![CommandInvocation::new("resize-pane", [direction])],
                    repeat: true,
                    note: None,
                },
            );
        }
        mux.update(cx, |mux, cx| {
            mux.handle_message_for_test(
                zz_protocol::ProtocolMessage::Event(zz_protocol::Event {
                    sequence: 0,
                    payload: zz_protocol::EventPayload::KeyTablesChanged {
                        tables: tables.snapshot(),
                    },
                }),
                cx,
            );
        });
        publish_key_table(&mux, Some("resize"), false, cx);
        let input = mux.update(cx, |mux, _| mux.record_input_for_test());
        wait_for_which_key(cx, 401);
        workspace.read_with(cx, |workspace, _| {
            let sheet = workspace.which_key.as_ref().unwrap();
            assert_eq!(sheet.header().table.as_ref(), "resize");
            assert!(sheet.header().prefix.is_none());
            assert_eq!(
                sheet
                    .rows()
                    .iter()
                    .map(|row| row.caps[0].raw.as_ref())
                    .collect::<Vec<_>>(),
                ["h l"]
            );
            assert!(sheet.header().more.is_none());
            assert!(
                sheet
                    .rows()
                    .iter()
                    .all(|row| row.group.is_none() && row.repeat)
            );
        });
        cx.simulate_keystrokes("h");
        assert!(!which_key_visible(&workspace, cx));
        assert!(
            input
                .borrow()
                .iter()
                .any(|message| matches!(message, InputMessage::Key { input, .. } if input.text.as_deref() == Some("h")))
        );
        publish_key_table(&mux, Some("resize"), false, cx);
        wait_for_which_key(cx, 401);
        assert!(which_key_visible(&workspace, cx));
    }

    #[cfg(unix)]
    #[ala::test]
    fn question_mark_opens_every_key_and_leaves_the_prefix_armed(cx: &mut TestAppContext) {
        cx.executor().allow_parking();
        let Some((client, _server)) = input_test_client() else {
            return;
        };
        let (workspace, cx) = input_test_workspace(cx, client);
        let mux = workspace.read_with(cx, |workspace, _| workspace.mux.clone());
        mux.update(cx, |mux, cx| mux.set_prefix_armed_for_test(true, cx));
        publish_key_table(&mux, Some("prefix"), false, cx);
        let input = mux.update(cx, |mux, _| mux.record_input_for_test());
        cx.simulate_keystrokes("?");
        workspace.read_with(cx, |workspace, _| {
            let sheet = workspace.which_key.as_ref().unwrap();
            assert!(sheet.header().more.is_none());
            assert!(sheet.rows().iter().any(|row| row.id.as_ref() == "M-1"));
        });
        assert!(input.borrow().is_empty(), "{:?}", input.borrow());
        cx.simulate_keystrokes("c");
        assert!(!which_key_visible(&workspace, cx));
        assert!(
            input
                .borrow()
                .iter()
                .any(|message| matches!(message, InputMessage::Key { input, .. } if input.text.as_deref() == Some("c")))
        );
    }

    #[cfg(unix)]
    #[ala::test]
    fn which_key_delay_zero_never_shows(cx: &mut TestAppContext) {
        cx.executor().allow_parking();
        let Some((client, _server)) = input_test_client() else {
            return;
        };
        let (workspace, cx) = input_test_workspace(cx, client);
        cx.update(|_, cx| {
            let mut config = config::resolved_config(cx);
            config.which_key_delay = zz_config::ConfigValue::from_default(0.0);
            cx.set_global(config);
        });
        let mux = workspace.read_with(cx, |workspace, _| workspace.mux.clone());
        publish_key_table(&mux, Some("prefix"), false, cx);
        assert!(workspace.read_with(cx, |workspace, _| workspace.which_key_timer.is_none()));
        wait_for_which_key(cx, 5_000);
        assert!(!which_key_visible(&workspace, cx));
    }

    #[ala::test]
    fn local_palette_survives_mux_updates_and_reopens_with_both_shortcuts(cx: &mut TestAppContext) {
        cx.update(|cx| {
            zz_ui::init(cx);
            crate::keymap::install(&[], config::DEFAULT_BROWSER_ELEMENT_SELECTOR_HOTKEY, cx);
        });
        let (workspace, cx) = cx.add_window_view(move |window, cx| {
            let controller = cx.new(|cx| {
                BrowserController::new(Err(zz_browser::BrowserError::AlreadyShutdown), cx)
            });
            let agent_controller = cx.new(|_| AgentController::new());
            let mux = cx.new(|cx| {
                MuxClient::new(
                    Err(zz_daemon_client::DaemonError::Thread(
                        "palette lifecycle".to_owned(),
                    )),
                    zz_daemon_client::default_socket_path(),
                    cx,
                )
            });
            AppView::new(controller, agent_controller, mux, window, cx)
        });
        let mux = workspace.read_with(cx, |workspace, _| workspace.mux.clone());
        mux.update(cx, |mux, cx| {
            mux.attach_snapshot_for_test(SessionId(0), one_pane_snapshot(1), cx);
        });
        cx.run_until_parked();
        let shortcuts = if cfg!(target_os = "macos") {
            ["cmd-k", "cmd-p"]
        } else {
            ["ctrl-shift-k", "ctrl-shift-p"]
        };
        for shortcut in shortcuts {
            cx.simulate_keystrokes(shortcut);
            cx.run_until_parked();
            let palette = workspace.read_with(cx, |workspace, cx| {
                let palette = workspace.command_palette.clone().expect("palette opened");
                assert!(palette.read(cx).is_local());
                assert!(workspace.mux.read(cx).command_prompt().is_none());
                palette
            });
            mux.update(cx, |mux, cx| {
                mux.attach_snapshot_for_test(SessionId(0), one_pane_snapshot(2), cx);
            });
            cx.run_until_parked();
            workspace.read_with(cx, |workspace, _| {
                assert_eq!(workspace.command_palette.as_ref(), Some(&palette));
            });
            cx.simulate_keystrokes(shortcut);
            cx.run_until_parked();
            workspace.read_with(cx, |workspace, _| {
                assert_ne!(workspace.command_palette.as_ref(), Some(&palette));
            });
            cx.simulate_keystrokes("escape");
            cx.run_until_parked();
            workspace.read_with(cx, |workspace, _| {
                assert!(workspace.command_palette.is_none());
            });
        }
        let sidebar = workspace.read_with(cx, |workspace, _| workspace.sidebar.clone());
        sidebar.update_in(cx, WorkspaceSidebar::open_settings);
        cx.run_until_parked();
        let settings_focus = sidebar.read_with(cx, |sidebar, cx| {
            sidebar.settings_view().unwrap().read(cx).focus()
        });
        for shortcut in shortcuts {
            cx.simulate_keystrokes(shortcut);
            cx.simulate_keystrokes(shortcut);
            cx.simulate_keystrokes("escape");
            cx.run_until_parked();
            assert!(cx.update(|window, cx| settings_focus.contains_focused(window, cx)));
        }
    }

    #[ala::test]
    fn window_shortcuts_intercept_before_pane_input_and_honor_unbinds(cx: &mut TestAppContext) {
        let fallback_count = Rc::new(Cell::new(0));
        let (modifier, key) = if cfg!(target_os = "macos") {
            ("cmd", "D-1")
        } else {
            ("ctrl", "C-1")
        };
        cx.update(|cx| {
            zz_ui::init(cx);
            crate::keymap::install(&[], config::DEFAULT_BROWSER_ELEMENT_SELECTOR_HOTKEY, cx);
            cx.bind_keys((1..=9).map(|position| {
                ala::KeyBinding::new(
                    &format!("{modifier}-{position}"),
                    WindowShortcutFallback,
                    None,
                )
            }));
            let fallback_count = fallback_count.clone();
            cx.on_action(move |_: &WindowShortcutFallback, _| {
                fallback_count.set(fallback_count.get() + 1);
            });
        });
        let (workspace, cx) = cx.add_window_view(|window, cx| {
            let controller = cx.new(|cx| {
                BrowserController::new(Err(zz_browser::BrowserError::AlreadyShutdown), cx)
            });
            let agent_controller = cx.new(|_| AgentController::new());
            let mux = cx.new(|cx| {
                MuxClient::new(
                    Err(zz_daemon_client::DaemonError::Thread(
                        "window shortcuts".to_owned(),
                    )),
                    zz_daemon_client::default_socket_path(),
                    cx,
                )
            });
            AppView::new(controller, agent_controller, mux, window, cx)
        });
        let mux = workspace.read_with(cx, |workspace, _| workspace.mux.clone());
        mux.update(cx, |mux, cx| {
            mux.attach_snapshot_for_test(SessionId(0), one_pane_snapshot(1), cx);
        });
        cx.run_until_parked();
        for position in 1..=9 {
            cx.simulate_keystrokes(&format!("{modifier}-{position}"));
        }
        assert_eq!(fallback_count.get(), 0);
        cx.update(|_, cx| {
            crate::keymap::install(
                &[crate::keymap::ChromeOverride::Unbind {
                    table: zz_client::UI_TABLE,
                    key: key.to_owned(),
                }],
                config::DEFAULT_BROWSER_ELEMENT_SELECTOR_HOTKEY,
                cx,
            );
        });
        cx.simulate_keystrokes(&format!("{modifier}-1"));
        assert_eq!(fallback_count.get(), 1);
    }

    #[ala::test]
    fn daemon_window_chooser_uses_palette_and_preserves_selection_actions(cx: &mut TestAppContext) {
        cx.update(|cx| {
            zz_ui::init(cx);
            crate::keymap::install(&[], config::DEFAULT_BROWSER_ELEMENT_SELECTOR_HOTKEY, cx);
        });
        let (workspace, cx) = cx.add_window_view(move |window, cx| {
            let controller = cx.new(|cx| {
                BrowserController::new(Err(zz_browser::BrowserError::AlreadyShutdown), cx)
            });
            let agent_controller = cx.new(|_| AgentController::new());
            let mux = cx.new(|cx| {
                MuxClient::new(
                    Err(zz_daemon_client::DaemonError::Thread(
                        "window palette".to_owned(),
                    )),
                    zz_daemon_client::default_socket_path(),
                    cx,
                )
            });
            AppView::new(controller, agent_controller, mux, window, cx)
        });
        let mux = workspace.read_with(cx, |workspace, _| workspace.mux.clone());
        let input = mux.update(cx, |mux, cx| {
            mux.attach_snapshot_for_test(SessionId(0), one_pane_snapshot(1), cx);
            mux.record_input_for_test()
        });
        let sequence = Cell::new(0);
        let publish = |state, cx: &mut ala::VisualTestContext| {
            sequence.set(sequence.get() + 1);
            mux.update(cx, |mux, cx| {
                mux.handle_message_for_test(
                    zz_protocol::ProtocolMessage::Event(zz_protocol::Event {
                        sequence: sequence.get(),
                        payload: zz_protocol::EventPayload::ChooseTree { state },
                    }),
                    cx,
                );
            });
            cx.run_until_parked();
            cx.update(|window, cx| {
                _ = window.draw(cx);
            });
        };
        let mut state = choose_tree_state_for_test();
        state.kind = zz_protocol::ChooseTreeKind::Windows;
        state.items[0].flags =
            zz_protocol::ChooseTreeItem::HAS_CHILDREN | zz_protocol::ChooseTreeItem::EXPANDED;
        state.items[1].label = "1:build".to_owned();
        state.items[1].depth = 1;
        let mut logs = state.items[1].clone();
        logs.label = "2:logs".to_owned();
        logs.target = zz_protocol::ChooseTreeTarget::Window(WindowId(1));
        state.items.push(logs);
        state.selected = 2;
        publish(Some(state.clone()), cx);
        assert!(!input.borrow().iter().any(|message| matches!(
            message,
            InputMessage::ChooseTree {
                action: zz_protocol::ChooseTreeAction::ExpandAll
            }
        )));
        assert!(cx.debug_bounds("choose-tree-overlay").is_none());
        assert!(cx.debug_bounds("command-palette-overlay").is_some());
        let palette = workspace.read_with(cx, |workspace, cx| {
            assert!(workspace.choose_tree.is_none());
            let palette = workspace.command_palette.clone().expect("window palette");
            assert!(palette.read(cx).is_window_chooser());
            palette
        });
        assert!(cx.update(|window, cx| palette.read(cx).focus(cx).is_focused(window)));
        input.borrow_mut().clear();
        cx.simulate_input("logs");
        cx.run_until_parked();
        assert!(input.borrow().is_empty(), "search stays in the palette");
        state.items.insert(0, state.items[0].clone());
        publish(Some(state.clone()), cx);
        assert_eq!(
            workspace.read_with(cx, |workspace, _| workspace.command_palette.clone()),
            Some(palette)
        );
        cx.simulate_keystrokes("enter");
        assert!(input.borrow().iter().any(|message| matches!(
            message,
            InputMessage::ChooseTree {
                action: zz_protocol::ChooseTreeAction::ActivateIndex(3)
            }
        )));
        publish(None, cx);
        assert!(workspace.read_with(cx, |workspace, _| workspace.command_palette.is_none()));
        publish(Some(state.clone()), cx);
        input.borrow_mut().clear();
        cx.simulate_keystrokes("escape");
        assert!(input.borrow().iter().any(|message| matches!(
            message,
            InputMessage::ChooseTree {
                action: zz_protocol::ChooseTreeAction::Close
            }
        )));
        publish(None, cx);
        publish(Some(state.clone()), cx);
        cx.simulate_keystrokes(if cfg!(target_os = "macos") {
            "cmd-k"
        } else {
            "ctrl-shift-k"
        });
        publish(None, cx);
        assert!(workspace.read_with(cx, |workspace, cx| {
            workspace
                .command_palette
                .as_ref()
                .is_some_and(|palette| palette.read(cx).is_local())
        }));
        cx.simulate_keystrokes("escape");
        cx.run_until_parked();
        for dismiss_immediately in [false, true] {
            publish(Some(state.clone()), cx);
            let original = workspace.read_with(cx, |workspace, _| {
                workspace.command_palette.clone().unwrap()
            });
            input.borrow_mut().clear();
            cx.simulate_input("x");
            cx.run_until_parked();
            cx.simulate_keystrokes("backspace");
            cx.run_until_parked();
            assert!(original.read_with(cx, |palette, _| palette.is_window_chooser()));
            cx.simulate_keystrokes("backspace");
            cx.run_until_parked();
            assert!(original.read_with(cx, |palette, _| palette.is_local()
                && !palette.is_window_chooser()));
            assert_eq!(
                workspace.read_with(cx, |workspace, _| workspace.command_palette.clone()),
                Some(original.clone())
            );
            assert!(cx.update(|window, cx| original.read(cx).focus(cx).is_focused(window)));
            assert_eq!(
                input
                    .borrow()
                    .iter()
                    .filter(|message| matches!(
                        message,
                        InputMessage::ChooseTree {
                            action: zz_protocol::ChooseTreeAction::Close
                        }
                    ))
                    .count(),
                1
            );
            if dismiss_immediately {
                cx.simulate_keystrokes("escape");
                cx.run_until_parked();
            }
            publish(Some(state.clone()), cx);
            assert_eq!(
                workspace.read_with(cx, |workspace, _| workspace.command_palette.clone()),
                (!dismiss_immediately).then_some(original.clone())
            );
            if dismiss_immediately {
                mux.update(cx, |mux, cx| {
                    for state in [None, Some(state.clone())] {
                        sequence.set(sequence.get() + 1);
                        mux.handle_message_for_test(
                            zz_protocol::ProtocolMessage::Event(zz_protocol::Event {
                                sequence: sequence.get(),
                                payload: zz_protocol::EventPayload::ChooseTree { state },
                            }),
                            cx,
                        );
                    }
                });
                cx.run_until_parked();
            } else {
                publish(None, cx);
                assert_eq!(
                    workspace.read_with(cx, |workspace, _| workspace.command_palette.clone()),
                    Some(original.clone())
                );
                publish(Some(state.clone()), cx);
            }
            workspace.read_with(cx, |workspace, cx| {
                let current = workspace.command_palette.as_ref().unwrap();
                assert_ne!(current, &original);
                assert!(current.read(cx).is_window_chooser());
            });
            cx.simulate_keystrokes("escape");
            publish(None, cx);
        }
    }

    #[ala::test]
    fn every_daemon_overlay_payload_reaches_a_desktop_consumer(cx: &mut TestAppContext) {
        cx.update(zz_ui::init);
        let mux_slot = Rc::new(RefCell::new(None));
        let input_slot = Rc::new(RefCell::new(None));
        let captured_mux = Rc::clone(&mux_slot);
        let captured_input = Rc::clone(&input_slot);
        let (workspace, cx) = cx.add_window_view(move |window, cx| {
            let controller = cx.new(|cx| {
                crate::browser::controller::BrowserController::new(
                    Err(zz_browser::BrowserError::AlreadyShutdown),
                    cx,
                )
            });
            let agent_controller = cx.new(|_| AgentController::new());
            let mux = cx.new(|cx| {
                MuxClient::new(
                    Err(zz_daemon_client::DaemonError::Thread(
                        "overlay matrix".to_owned(),
                    )),
                    zz_daemon_client::default_socket_path(),
                    cx,
                )
            });
            let input = mux.update(cx, |mux, _| mux.record_input_for_test());
            captured_mux.replace(Some(mux.clone()));
            captured_input.replace(Some(input));
            AppView::new(controller, agent_controller, mux, window, cx)
        });
        let mux: Entity<MuxClient> = mux_slot.borrow().clone().expect("captured mux");
        let input = input_slot.borrow().clone().expect("captured input");
        let sequence = Cell::new(0_u64);

        let publish = |payload: zz_protocol::EventPayload, cx: &mut ala::VisualTestContext| {
            sequence.set(sequence.get() + 1);
            mux.update(cx, |mux, cx| {
                mux.handle_message_for_test(
                    zz_protocol::ProtocolMessage::Event(zz_protocol::Event {
                        sequence: sequence.get(),
                        payload,
                    }),
                    cx,
                );
            });
            cx.run_until_parked();
            cx.update(|window, cx| {
                _ = window.draw(cx);
            });
        };

        mux.update(cx, |mux, cx| {
            mux.attach_snapshot_for_test(SessionId(0), one_pane_snapshot(1), cx);
        });
        cx.run_until_parked();

        // Kind, the payload that raises it, the payload that retires it, the
        // key the overlay answers, and the shape that key has to reach the
        // daemon as. Every element the matrix looks for is the selector
        // `overlay_selector` maps the kind's CoreEvent to.
        struct OverlayCase {
            event: CoreEvent,
            raise: zz_protocol::EventPayload,
            retire: zz_protocol::EventPayload,
            keystroke: &'static str,
            reaches_daemon: fn(&InputMessage) -> bool,
        }

        let cases = vec![
            OverlayCase {
                event: CoreEvent::CommandPromptChanged,
                raise: zz_protocol::EventPayload::CommandPrompt {
                    state: Some(command_prompt_state_for_test()),
                },
                retire: zz_protocol::EventPayload::CommandPrompt { state: None },
                keystroke: "escape",
                reaches_daemon: |message| matches!(message, InputMessage::CommandPrompt { .. }),
            },
            OverlayCase {
                event: CoreEvent::ChooseTreeChanged,
                raise: zz_protocol::EventPayload::ChooseTree {
                    state: Some(choose_tree_state_for_test()),
                },
                retire: zz_protocol::EventPayload::ChooseTree { state: None },
                keystroke: "down",
                reaches_daemon: |message| matches!(message, InputMessage::ChooseTree { .. }),
            },
            OverlayCase {
                event: CoreEvent::ChooseBufferChanged,
                raise: zz_protocol::EventPayload::ChooseBuffer {
                    state: Some(choose_buffer_state_for_test()),
                },
                retire: zz_protocol::EventPayload::ChooseBuffer { state: None },
                keystroke: "down",
                reaches_daemon: |message| matches!(message, InputMessage::ChooseBuffer { .. }),
            },
            OverlayCase {
                event: CoreEvent::DisplayPanesChanged,
                raise: zz_protocol::EventPayload::DisplayPanes {
                    state: Some(display_panes_state_for_test()),
                },
                retire: zz_protocol::EventPayload::DisplayPanes { state: None },
                keystroke: "escape",
                reaches_daemon: |message| matches!(message, InputMessage::DisplayPanes { .. }),
            },
            OverlayCase {
                event: CoreEvent::MenuChanged,
                raise: zz_protocol::EventPayload::Menu {
                    state: Some(menu_state_for_test()),
                },
                retire: zz_protocol::EventPayload::Menu { state: None },
                keystroke: "q",
                reaches_daemon: |message| matches!(message, InputMessage::Menu { .. }),
            },
            OverlayCase {
                event: CoreEvent::ConfirmChanged,
                raise: zz_protocol::EventPayload::Confirm {
                    state: Some(zz_protocol::ConfirmState {
                        prompt: "Confirm? (y/n) ".to_owned(),
                        confirm_key: b'y',
                        default_yes: false,
                        prompt_cursor: zz_protocol::PromptCursor::default(),
                    }),
                },
                retire: zz_protocol::EventPayload::Confirm { state: None },
                keystroke: "y",
                reaches_daemon: |message| matches!(message, InputMessage::Confirm { .. }),
            },
        ];

        for case in cases {
            let selector = overlay_selector(&case.event)
                .unwrap_or_else(|| panic!("{:?} claims no overlay element", case.event));
            publish(case.raise, cx);
            assert!(
                cx.debug_bounds(selector).is_some(),
                "{selector} published its state and drew nothing"
            );
            let focused = workspace.read_with(cx, |workspace, cx| {
                workspace
                    .visible_overlay(cx)
                    .map(|(_, focus)| focus)
                    .expect("a published overlay is the visible overlay")
            });
            assert!(
                cx.update(|window, cx| focused.contains_focused(window, cx)),
                "{selector} drew without taking the keyboard"
            );
            input.borrow_mut().clear();
            cx.simulate_keystrokes(case.keystroke);
            assert!(
                input.borrow().iter().any(case.reaches_daemon),
                "{selector} swallowed {} instead of answering the daemon: {:?}",
                case.keystroke,
                input.borrow()
            );
            publish(case.retire, cx);
            assert!(
                cx.debug_bounds(selector).is_none(),
                "{selector} outlived the state that raised it"
            );
        }

        // The two update payloads carry no state of their own: they advance a
        // chooser the daemon already published, so the consumer they need is
        // the running chooser's own synchronize, not a new element.
        publish(
            zz_protocol::EventPayload::ChooseTree {
                state: Some(choose_tree_state_for_test()),
            },
            cx,
        );
        let tree_revision = mux.read_with(cx, |mux, _| mux.choose_tree_revision());
        publish(
            zz_protocol::EventPayload::ChooseTreeUpdate {
                search: Some(zz_protocol::ChooseTreeSearchState {
                    query: "win".to_owned(),
                    reverse: false,
                }),
                selected: 1,
            },
            cx,
        );
        assert!(cx.debug_bounds("choose-tree-overlay").is_some());
        mux.read_with(cx, |mux, _| {
            let state = mux.choose_tree().expect("the update kept the chooser open");
            assert_eq!(state.selected, 1, "ChooseTreeUpdate moved no selection");
            assert_eq!(
                state.search.as_ref().map(|search| search.query.as_str()),
                Some("win"),
                "ChooseTreeUpdate landed no search"
            );
            assert_ne!(
                mux.choose_tree_revision(),
                tree_revision,
                "ChooseTreeUpdate bumped no revision, so no consumer redraws"
            );
        });
        publish(zz_protocol::EventPayload::ChooseTree { state: None }, cx);

        publish(
            zz_protocol::EventPayload::ChooseBuffer {
                state: Some(choose_buffer_state_for_test()),
            },
            cx,
        );
        let buffer_revision = mux.read_with(cx, |mux, _| mux.choose_buffer_revision());
        publish(
            zz_protocol::EventPayload::ChooseBufferUpdate {
                search: Some(zz_protocol::ChooseBufferSearchState {
                    query: "buf".to_owned(),
                    reverse: false,
                }),
                selected: 1,
            },
            cx,
        );
        assert!(cx.debug_bounds("choose-buffer-overlay").is_some());
        mux.read_with(cx, |mux, _| {
            let state = mux
                .choose_buffer()
                .expect("the update kept the chooser open");
            assert_eq!(state.selected, 1, "ChooseBufferUpdate moved no selection");
            assert_eq!(
                state.search.as_ref().map(|search| search.query.as_str()),
                Some("buf"),
                "ChooseBufferUpdate landed no search"
            );
            assert_ne!(
                mux.choose_buffer_revision(),
                buffer_revision,
                "ChooseBufferUpdate bumped no revision, so no consumer redraws"
            );
        });
        publish(zz_protocol::EventPayload::ChooseBuffer { state: None }, cx);
        assert!(workspace.read_with(cx, |workspace, cx| workspace.visible_overlay(cx).is_none()));
    }

    /// Derived from pinned tmux d77c9dc6, menu.c `menu_key_cb`:
    ///
    /// ```c
    /// if (md->flags & MENU_NOMOUSE) {
    ///         if (MOUSE_BUTTONS(m->b) != MOUSE_BUTTON_1)
    ///                 return (1);
    ///         return (0);
    /// }
    /// ```
    ///
    /// A `display-menu` raised without `-M` and without an invoking mouse
    /// event is `MENU_NOMOUSE` — `menu_prepare` also leaves `MODE_MOUSE_ALL`
    /// and `MODE_MOUSE_BUTTON` off for it. Button 1 is swallowed and the menu
    /// stays; any other button returns 1, which closes the menu with nothing
    /// chosen. `MenuState::mouse_keys` carries that flag to every client, and
    /// the desktop was the client that dropped it: `resolve_menu_mouse` in the
    /// client core reads it, but only crates/zz-tui/src/input.rs called that,
    /// so a right-click over the desktop menu did nothing and a left-click
    /// chose a row the pin would never have chosen.
    #[ala::test]
    fn a_nomouse_menu_leaves_on_any_button_but_the_first(cx: &mut TestAppContext) {
        cx.update(zz_ui::init);
        let mux_slot = Rc::new(RefCell::new(None));
        let input_slot = Rc::new(RefCell::new(None));
        let captured_mux = Rc::clone(&mux_slot);
        let captured_input = Rc::clone(&input_slot);
        let (_, cx) = cx.add_window_view(move |window, cx| {
            let controller = cx.new(|cx| {
                crate::browser::controller::BrowserController::new(
                    Err(zz_browser::BrowserError::AlreadyShutdown),
                    cx,
                )
            });
            let agent_controller = cx.new(|_| AgentController::new());
            let mux = cx.new(|cx| {
                MuxClient::new(
                    Err(zz_daemon_client::DaemonError::Thread(
                        "menu mouse".to_owned(),
                    )),
                    zz_daemon_client::default_socket_path(),
                    cx,
                )
            });
            let input = mux.update(cx, |mux, _| mux.record_input_for_test());
            captured_mux.replace(Some(mux.clone()));
            captured_input.replace(Some(input));
            AppView::new(controller, agent_controller, mux, window, cx)
        });
        let mux: Entity<MuxClient> = mux_slot.borrow().clone().expect("captured mux");
        let input = input_slot.borrow().clone().expect("captured input");
        let sequence = Cell::new(0_u64);
        let raise = |mouse_keys: bool, cx: &mut ala::VisualTestContext| {
            let mut state = menu_state_for_test();
            state.mouse_keys = mouse_keys;
            sequence.set(sequence.get() + 1);
            mux.update(cx, |mux, cx| {
                mux.handle_message_for_test(
                    zz_protocol::ProtocolMessage::Event(zz_protocol::Event {
                        sequence: sequence.get(),
                        payload: zz_protocol::EventPayload::Menu { state: Some(state) },
                    }),
                    cx,
                );
            });
            cx.run_until_parked();
            cx.update(|window, cx| {
                _ = window.draw(cx);
            });
        };

        mux.update(cx, |mux, cx| {
            mux.attach_snapshot_for_test(SessionId(0), one_pane_snapshot(1), cx);
        });
        cx.run_until_parked();

        raise(false, cx);
        let row = cx
            .debug_bounds("display-menu-row-0")
            .expect("the menu drew its first row");
        let inside = row.center();

        input.borrow_mut().clear();
        cx.simulate_mouse_down(inside, MouseButton::Right, Modifiers::default());
        assert!(
            input.borrow().iter().any(|message| matches!(
                message,
                InputMessage::Menu {
                    action: zz_protocol::MenuAction::Cancel
                }
            )),
            "a NOMOUSE menu must leave on button 3: {:?}",
            input.borrow()
        );

        input.borrow_mut().clear();
        cx.simulate_mouse_down(inside, MouseButton::Left, Modifiers::default());
        assert!(
            input.borrow().is_empty(),
            "a NOMOUSE menu swallows the button 1 press: {:?}",
            input.borrow()
        );
        cx.simulate_mouse_up(inside, MouseButton::Left, Modifiers::default());
        assert!(
            input.borrow().iter().any(|message| matches!(
                message,
                InputMessage::Menu {
                    action: zz_protocol::MenuAction::Cancel
                }
            )),
            "a NOMOUSE menu leaves on the button 1 release, which reports 3: {:?}",
            input.borrow()
        );
        assert!(
            !input.borrow().iter().any(|message| matches!(
                message,
                InputMessage::Menu {
                    action: zz_protocol::MenuAction::Choose(_)
                }
            )),
            "a NOMOUSE menu chooses nothing under button 1: {:?}",
            input.borrow()
        );

        raise(true, cx);
        input.borrow_mut().clear();
        cx.simulate_mouse_down(inside, MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_up(inside, MouseButton::Left, Modifiers::default());
        assert!(
            input.borrow().iter().any(|message| matches!(
                message,
                InputMessage::Menu {
                    action: zz_protocol::MenuAction::Choose(0)
                }
            )),
            "a menu that took the mouse still chooses the row under button 1: {:?}",
            input.borrow()
        );
    }

    #[ala::test]
    fn a_bare_motion_over_a_nomouse_menu_sends_nothing(cx: &mut TestAppContext) {
        let (input, cx) = menu_mouse_fixture(cx, false, false);
        let row = cx
            .debug_bounds("display-menu-row-1")
            .expect("the menu drew its second row");

        cx.simulate_mouse_move(row.center(), None, Modifiers::default());
        assert!(
            input.borrow().is_empty(),
            "the pin never hears a bare motion over a NOMOUSE menu: {:?}",
            input.borrow()
        );

        cx.simulate_keystrokes("escape");
        assert!(
            menu_cancelled(&input),
            "the menu is still up for its keys: {:?}",
            input.borrow()
        );
    }

    /// A workspace with one pane and a `display-menu` up, plus the input queue
    /// the daemon would have received. `mouse_keys` is the pin's `MENU_NOMOUSE`
    /// flag inverted: true is a menu that took the mouse.
    fn menu_mouse_fixture(
        cx: &mut TestAppContext,
        mouse_keys: bool,
        stay_open: bool,
    ) -> (Rc<RefCell<Vec<InputMessage>>>, &mut ala::VisualTestContext) {
        cx.update(zz_ui::init);
        let mux_slot = Rc::new(RefCell::new(None));
        let input_slot = Rc::new(RefCell::new(None));
        let captured_mux = Rc::clone(&mux_slot);
        let captured_input = Rc::clone(&input_slot);
        let (_, cx) = cx.add_window_view(move |window, cx| {
            let controller = cx.new(|cx| {
                crate::browser::controller::BrowserController::new(
                    Err(zz_browser::BrowserError::AlreadyShutdown),
                    cx,
                )
            });
            let agent_controller = cx.new(|_| AgentController::new());
            let mux = cx.new(|cx| {
                MuxClient::new(
                    Err(zz_daemon_client::DaemonError::Thread(
                        "menu mouse".to_owned(),
                    )),
                    zz_daemon_client::default_socket_path(),
                    cx,
                )
            });
            let input = mux.update(cx, |mux, _| mux.record_input_for_test());
            captured_mux.replace(Some(mux.clone()));
            captured_input.replace(Some(input));
            AppView::new(controller, agent_controller, mux, window, cx)
        });
        let mux: Entity<MuxClient> = mux_slot.borrow().clone().expect("captured mux");
        let input = input_slot.borrow().clone().expect("captured input");
        mux.update(cx, |mux, cx| {
            mux.attach_snapshot_for_test(SessionId(0), one_pane_snapshot(1), cx);
        });
        cx.run_until_parked();

        let mut state = menu_state_for_test();
        state.mouse_keys = mouse_keys;
        state.stay_open = stay_open;
        state.height = 5;
        state.items = (0..3)
            .map(|index| {
                Some(zz_protocol::MenuItem {
                    name: format!("Item {index}"),
                    key: None,
                    annotation: None,
                    enabled: true,
                })
            })
            .collect();
        state.selected = None;
        mux.update(cx, |mux, cx| {
            mux.handle_message_for_test(
                zz_protocol::ProtocolMessage::Event(zz_protocol::Event {
                    sequence: 1,
                    payload: zz_protocol::EventPayload::Menu { state: Some(state) },
                }),
                cx,
            );
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
        input.borrow_mut().clear();
        (input, cx)
    }

    fn menu_chose(input: &Rc<RefCell<Vec<InputMessage>>>, index: u32) -> bool {
        input.borrow().iter().any(|message| {
            matches!(
                message,
                InputMessage::Menu {
                    action: zz_protocol::MenuAction::Choose(chosen)
                } if *chosen == index
            )
        })
    }

    fn menu_cancelled(input: &Rc<RefCell<Vec<InputMessage>>>) -> bool {
        input.borrow().iter().any(|message| {
            matches!(
                message,
                InputMessage::Menu {
                    action: zz_protocol::MenuAction::Cancel
                }
            )
        })
    }

    /// `menu_key_cb`: a report inside the box that neither chooses nor closes
    /// sets `md->choice` to `m->y - (md->py + 1)`, so a bare motion moves the
    /// highlight and nothing else. Enter then chooses the row the highlight
    /// sits on, which is how this reads the highlight without asking the view
    /// for its private state.
    #[ala::test]
    fn a_mouse_menu_moves_its_highlight_under_a_motion(cx: &mut TestAppContext) {
        let (input, cx) = menu_mouse_fixture(cx, true, false);
        let row = cx
            .debug_bounds("display-menu-row-2")
            .expect("the menu drew its third row");

        cx.simulate_mouse_move(row.center(), None, Modifiers::default());
        assert!(
            !menu_cancelled(&input) && input.borrow().is_empty(),
            "a motion neither chooses nor closes: {:?}",
            input.borrow()
        );

        cx.simulate_keystrokes("enter");
        assert!(
            menu_chose(&input, 2),
            "Enter chooses the row the motion highlighted: {:?}",
            input.borrow()
        );
    }

    /// `menu_key_cb`: for a menu that is not stay-open, `!move && MOUSE_RELEASE`
    /// inside the box goes to `chosen`, and `chosen` reads `md->choice`, which
    /// the release never sets. So the release chooses the row the highlight
    /// already sits on, not the row it lands on, and a press on its own only
    /// moves the highlight there.
    #[ala::test]
    fn a_release_inside_a_mouse_menu_chooses_the_highlighted_row(cx: &mut TestAppContext) {
        let (input, cx) = menu_mouse_fixture(cx, true, false);
        let highlighted = cx
            .debug_bounds("display-menu-row-2")
            .expect("the menu drew its third row");
        let landed_on = cx
            .debug_bounds("display-menu-row-0")
            .expect("the menu drew its first row");

        cx.simulate_mouse_down(landed_on.center(), MouseButton::Left, Modifiers::default());
        assert!(
            input.borrow().is_empty(),
            "the press only moves the highlight: {:?}",
            input.borrow()
        );

        cx.simulate_mouse_move(highlighted.center(), None, Modifiers::default());
        cx.simulate_mouse_up(landed_on.center(), MouseButton::Left, Modifiers::default());
        assert!(
            menu_chose(&input, 2),
            "the release chooses the highlighted row: {:?}",
            input.borrow()
        );
        assert!(
            !menu_chose(&input, 0),
            "and not the row it landed on: {:?}",
            input.borrow()
        );
    }

    /// `menu_key_cb`: outside the box, a menu that is not stay-open closes on
    /// `!move && MOUSE_RELEASE` and on nothing else, so the press that opened
    /// the way to it leaves the menu up.
    #[ala::test]
    fn a_release_outside_a_mouse_menu_closes_it_and_a_press_does_not(cx: &mut TestAppContext) {
        let (input, cx) = menu_mouse_fixture(cx, true, false);
        let menu = cx
            .debug_bounds("display-menu")
            .expect("the menu drew its box");
        let outside = ala::point(menu.origin.x - px(24.0), menu.center().y);

        cx.simulate_mouse_down(outside, MouseButton::Left, Modifiers::default());
        assert!(
            !menu_cancelled(&input),
            "a press outside leaves the menu up: {:?}",
            input.borrow()
        );
        cx.simulate_mouse_up(outside, MouseButton::Left, Modifiers::default());
        assert!(
            menu_cancelled(&input),
            "a release outside closes the menu: {:?}",
            input.borrow()
        );
        assert!(
            !menu_chose(&input, 0) && !menu_chose(&input, 1) && !menu_chose(&input, 2),
            "closing outside the box chooses nothing: {:?}",
            input.borrow()
        );
    }

    /// `menu_key_cb`: with `MENU_STAYOPEN` the outside arm inverts. It closes on
    /// any report that is neither a release, a wheel nor a drag, so the press
    /// that leaves a plain menu up closes this one, and the wheel that closes
    /// nothing still closes nothing.
    #[ala::test]
    fn a_stay_open_menu_closes_outside_on_a_press_and_not_on_a_wheel(cx: &mut TestAppContext) {
        let (input, cx) = menu_mouse_fixture(cx, true, true);
        let menu = cx
            .debug_bounds("display-menu")
            .expect("the menu drew its box");
        let outside = ala::point(menu.origin.x - px(24.0), menu.center().y);

        cx.simulate_event(ala::ScrollWheelEvent {
            position: outside,
            delta: ala::ScrollDelta::Lines(ala::point(0.0, -1.0)),
            modifiers: Modifiers::default(),
            touch_phase: ala::TouchPhase::Moved,
        });
        assert!(
            !menu_cancelled(&input),
            "a wheel outside a stay-open menu leaves it up: {:?}",
            input.borrow()
        );

        cx.simulate_mouse_down(outside, MouseButton::Left, Modifiers::default());
        assert!(
            menu_cancelled(&input),
            "a press outside closes a stay-open menu: {:?}",
            input.borrow()
        );
    }

    #[ala::test]
    fn a_capture_modal_float_draws_over_a_scrim_and_takes_every_key(cx: &mut TestAppContext) {
        cx.update(zz_ui::init);
        let mux_slot = Rc::new(RefCell::new(None));
        let input_slot = Rc::new(RefCell::new(None));
        let captured_mux = Rc::clone(&mux_slot);
        let captured_input = Rc::clone(&input_slot);
        let (_workspace, cx) = cx.add_window_view(move |window, cx| {
            let controller = cx.new(|cx| {
                crate::browser::controller::BrowserController::new(
                    Err(zz_browser::BrowserError::AlreadyShutdown),
                    cx,
                )
            });
            let agent_controller = cx.new(|_| AgentController::new());
            let mux = cx.new(|cx| {
                MuxClient::new(
                    Err(zz_daemon_client::DaemonError::Thread(
                        "test client".to_owned(),
                    )),
                    zz_daemon_client::default_socket_path(),
                    cx,
                )
            });
            let input = mux.update(cx, |mux, _| mux.record_input_for_test());
            captured_mux.replace(Some(mux.clone()));
            captured_input.replace(Some(input));
            AppView::new(controller, agent_controller, mux, window, cx)
        });
        let mux: Entity<MuxClient> = mux_slot.borrow().clone().expect("captured mux");
        let input = input_slot.borrow().clone().expect("captured input");
        let tiled = PaneId(0);
        let modal = PaneId(1);
        let mut snapshot = one_pane_snapshot(1);
        let window = &mut snapshot.sessions[0].windows[0];
        let mut float_pane = window.panes[&tiled].clone();
        float_pane.id = modal;
        float_pane.title = modal.to_string();
        float_pane.border_status_text = "#[fg=red]modal".to_owned();
        window.panes.insert(modal, float_pane);
        window.floating = vec![FloatingPaneSnapshot {
            pane: modal,
            xoff: 4,
            yoff: 2,
            sx: 20,
            sy: 6,
            visible: true,
            border_lines: PaneBorderLines::Single,
            border_status: zz_protocol::PaneBorderStatus::Top,
        }];
        window.modal = Some(ModalPaneSnapshot {
            pane: modal,
            capture_keys: true,
            close_on_click: true,
            close_on_cancel: true,
        });
        mux.update(cx, |mux, cx| {
            mux.attach_snapshot_for_test(SessionId(0), snapshot, cx);
        });
        cx.run_until_parked();
        for _ in 0..2 {
            cx.update(|window, cx| {
                _ = window.draw(cx);
            });
        }

        let float = cx
            .debug_bounds("floating-pane-1")
            .expect("a visible float is drawn");
        let scrim = cx
            .debug_bounds("floating-modal-scrim")
            .expect("a modal draws its scrim");
        assert!(scrim.contains(&float.origin));
        assert!(cx.debug_bounds("floating-grip-1-move").is_some());
        assert!(cx.debug_bounds("floating-pane-0").is_none());

        input.borrow_mut().clear();
        cx.simulate_keystrokes("ctrl-a");
        let sent = input.borrow().clone();
        assert!(
            sent.iter().any(|message| matches!(
                message,
                InputMessage::Key { pane, input, .. }
                    if *pane == modal
                        && input.key == zz_terminal::KeyCode::Character('a')
                        && input.modifiers.control()
            )),
            "the capture modal did not take the key: {sent:?}"
        );
        assert!(
            !sent
                .iter()
                .any(|message| matches!(message, InputMessage::Key { pane, .. } if *pane == tiled)),
            "the tiled pane under the modal got a key: {sent:?}"
        );
    }

    #[ala::test]
    fn menu_and_confirm_surfaces_take_focus_and_bypass_the_prefix_claim(cx: &mut TestAppContext) {
        cx.update(zz_ui::init);
        let mux_slot = Rc::new(RefCell::new(None));
        let input_slot = Rc::new(RefCell::new(None));
        let captured_mux = Rc::clone(&mux_slot);
        let captured_input = Rc::clone(&input_slot);
        let (workspace, cx) = cx.add_window_view(move |window, cx| {
            let controller = cx.new(|cx| {
                crate::browser::controller::BrowserController::new(
                    Err(zz_browser::BrowserError::AlreadyShutdown),
                    cx,
                )
            });
            let agent_controller = cx.new(|_| AgentController::new());
            let mux = cx.new(|cx| {
                MuxClient::new(
                    Err(zz_daemon_client::DaemonError::Thread(
                        "test client".to_owned(),
                    )),
                    zz_daemon_client::default_socket_path(),
                    cx,
                )
            });
            let input = mux.update(cx, |mux, _| mux.record_input_for_test());
            captured_mux.replace(Some(mux.clone()));
            captured_input.replace(Some(input));
            AppView::new(controller, agent_controller, mux, window, cx)
        });
        let mux = mux_slot.borrow().clone().expect("captured mux");
        let input = input_slot.borrow().clone().expect("captured input");
        mux.update(cx, |mux, cx| {
            mux.attach_snapshot_for_test(SessionId(0), one_pane_snapshot(1), cx);
            mux.handle_message_for_test(
                zz_protocol::ProtocolMessage::Event(zz_protocol::Event {
                    sequence: 1,
                    payload: zz_protocol::EventPayload::Menu {
                        state: Some(menu_state_for_test()),
                    },
                }),
                cx,
            );
            mux.set_prefix_armed_for_test(true, cx);
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });

        let menu = workspace.read_with(cx, |workspace, _| {
            workspace.menu.clone().expect("menu entity")
        });
        assert!(cx.debug_bounds("display-menu").is_some());
        assert!(cx.update(|window, cx| menu.read(cx).focus().contains_focused(window, cx)));
        input.borrow_mut().clear();
        cx.simulate_keystrokes("q");
        assert!(input.borrow().iter().any(|message| matches!(
            message,
            InputMessage::Menu {
                action: zz_protocol::MenuAction::Choose(0)
            }
        )));
        assert!(
            !input
                .borrow()
                .iter()
                .any(|message| matches!(message, InputMessage::Key { .. }))
        );
        input.borrow_mut().clear();
        cx.simulate_keystrokes("ctrl-a");
        assert!(input.borrow().is_empty());
        assert!(cx.update(|window, cx| menu.read(cx).focus().contains_focused(window, cx)));

        mux.update(cx, |mux, cx| {
            mux.handle_message_for_test(
                zz_protocol::ProtocolMessage::Event(zz_protocol::Event {
                    sequence: 2,
                    payload: zz_protocol::EventPayload::Menu { state: None },
                }),
                cx,
            );
            mux.handle_message_for_test(
                zz_protocol::ProtocolMessage::Event(zz_protocol::Event {
                    sequence: 3,
                    payload: zz_protocol::EventPayload::Confirm {
                        state: Some(zz_protocol::ConfirmState {
                            prompt: "Confirm? (y/n) ".to_owned(),
                            confirm_key: b'y',
                            default_yes: false,
                            prompt_cursor: zz_protocol::PromptCursor::default(),
                        }),
                    },
                }),
                cx,
            );
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
        let confirm = workspace.read_with(cx, |workspace, _| {
            workspace.confirm.clone().expect("confirm entity")
        });
        assert!(cx.debug_bounds("confirm-before").is_some());
        assert!(cx.update(|window, cx| confirm.read(cx).focus().contains_focused(window, cx)));
        input.borrow_mut().clear();
        cx.simulate_keystrokes("y");
        assert!(input.borrow().iter().any(|message| matches!(
            message,
            InputMessage::Confirm {
                action: zz_protocol::ConfirmAction::Reply(true)
            }
        )));
        assert!(
            !input
                .borrow()
                .iter()
                .any(|message| matches!(message, InputMessage::Key { .. }))
        );
    }

    fn two_pane_snapshot(active: PaneId) -> MuxSnapshot {
        two_pane_snapshot_of(active, PaneKindSnapshot::Picker)
    }

    fn two_pane_snapshot_of(active: PaneId, second: PaneKindSnapshot) -> MuxSnapshot {
        let terminal = PaneId(0);
        let picker = PaneId(2);
        let generation_bias = u64::from(second != PaneKindSnapshot::Picker) * 100;
        let pane_snapshot = |id: PaneId, kind: PaneKindSnapshot| zz_protocol::PaneSnapshot {
            id,
            title: id.to_string(),
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
            summary: zz_protocol::PaneSummary::default(),
        };
        let window = WindowSnapshot {
            id: WindowId(0),
            index: 0,
            name: "zz".to_owned(),
            automatic_rename: true,
            active_pane: active,
            zoomed_pane: None,
            layout: LayoutNode::Split {
                id: SplitId(0),
                axis: Axis::Horizontal,
                ratio: 0.5,
                first: Box::new(LayoutNode::Pane(terminal)),
                second: Box::new(LayoutNode::Pane(picker)),
            },
            panes: [
                (
                    terminal,
                    pane_snapshot(terminal, PaneKindSnapshot::Terminal),
                ),
                (picker, pane_snapshot(picker, second)),
            ]
            .into_iter()
            .collect(),
            layout_dump: String::new(),
            visible_layout_dump: String::new(),
            status_label: String::new(),
            activity: false,
            silence: false,
            pane_border_status: zz_protocol::PaneBorderStatus::Off,
            pane_border_lines: zz_protocol::PaneBorderLines::Single,
            pane_border_indicators: zz_protocol::PaneBorderIndicators::Colour,
            pane_order: Vec::new(),
            pane_z_order: Vec::new(),
            floating: Vec::new(),
            modal: None,
            sx: 0,
            sy: 0,
            pane_base_index: 0,
        };
        MuxSnapshot {
            generation: 10 + active.0 + generation_bias,
            focused_window: Some(WindowId(0)),
            sessions: vec![zz_protocol::SessionSnapshot {
                id: SessionId(0),
                name: "zz".to_owned(),
                active_window: WindowId(0),
                windows: vec![window],
                viewers: Vec::new(),
            }],
        }
    }

    #[ala::test]
    fn window_activation_is_the_only_workspace_client_focus_source(cx: &mut TestAppContext) {
        cx.update(zz_ui::init);
        let mux_slot = Rc::new(RefCell::new(None));
        let input_slot = Rc::new(RefCell::new(None));
        let initial_active_slot = Rc::new(Cell::new(None));
        let captured_mux = Rc::clone(&mux_slot);
        let captured_input = Rc::clone(&input_slot);
        let captured_initial_active = Rc::clone(&initial_active_slot);
        let (_, cx) = cx.add_window_view(move |window, cx| {
            let controller = cx.new(|cx| {
                crate::browser::controller::BrowserController::new(
                    Err(zz_browser::BrowserError::AlreadyShutdown),
                    cx,
                )
            });
            let agent_controller = cx.new(|_| AgentController::new());
            let mux = cx.new(|cx| {
                MuxClient::new(
                    Err(zz_daemon_client::DaemonError::Thread(
                        "test client".to_owned(),
                    )),
                    zz_daemon_client::default_socket_path(),
                    cx,
                )
            });
            let input = mux.update(cx, |mux, _| mux.record_input_for_test());
            captured_mux.replace(Some(mux.clone()));
            captured_input.replace(Some(input));
            captured_initial_active.set(Some(window.is_window_active()));
            AppView::new(controller, agent_controller, mux, window, cx)
        });
        let mux = mux_slot.borrow().clone().expect("captured mux");
        let input = input_slot.borrow().clone().expect("captured input");
        let initial_active = initial_active_slot.get().expect("initial activation state");
        assert!(!initial_active);
        mux.update(cx, |mux, cx| {
            mux.handle_message_for_test(
                zz_protocol::ProtocolMessage::Attached {
                    session: SessionId(0),
                    snapshot: two_pane_snapshot_of(PaneId(0), PaneKindSnapshot::Terminal),
                    read_only: false,
                    client_flags: String::new(),
                },
                cx,
            );
        });
        cx.run_until_parked();
        assert!(
            input
                .borrow()
                .iter()
                .filter_map(|message| match message {
                    InputMessage::ClientFocus { focused } => Some(*focused),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .is_empty()
        );

        mux.update(cx, |mux, cx| {
            mux.handle_message_for_test(
                zz_protocol::ProtocolMessage::Event(zz_protocol::Event {
                    sequence: 1,
                    payload: zz_protocol::EventPayload::Snapshot(two_pane_snapshot_of(
                        PaneId(2),
                        PaneKindSnapshot::Terminal,
                    )),
                }),
                cx,
            );
        });
        cx.run_until_parked();
        assert!(
            input
                .borrow()
                .iter()
                .all(|message| !matches!(message, InputMessage::ClientFocus { .. }))
        );

        input.borrow_mut().clear();
        cx.update(|window, _| window.activate_window());
        cx.run_until_parked();
        assert_eq!(
            input
                .borrow()
                .iter()
                .filter_map(|message| match message {
                    InputMessage::ClientFocus { focused } => Some(*focused),
                    _ => None,
                })
                .collect::<Vec<_>>(),
            [true]
        );

        input.borrow_mut().clear();
        cx.deactivate_window();
        assert_eq!(
            input
                .borrow()
                .iter()
                .filter_map(|message| match message {
                    InputMessage::ClientFocus { focused } => Some(*focused),
                    _ => None,
                })
                .collect::<Vec<_>>(),
            [false]
        );
    }

    #[ala::test]
    fn committed_split_drag_lives_until_same_generation_snapshot_arrives(cx: &mut TestAppContext) {
        cx.update(zz_ui::init);
        let mux_slot = Rc::new(RefCell::new(None));
        let captured_mux = Rc::clone(&mux_slot);
        let (workspace, cx) = cx.add_window_view(move |window, cx| {
            let controller = cx.new(|cx| {
                crate::browser::controller::BrowserController::new(
                    Err(zz_browser::BrowserError::AlreadyShutdown),
                    cx,
                )
            });
            let agent_controller = cx.new(|_| AgentController::new());
            let mux = cx.new(|cx| {
                MuxClient::new(
                    Err(zz_daemon_client::DaemonError::Thread(
                        "test client".to_owned(),
                    )),
                    zz_daemon_client::default_socket_path(),
                    cx,
                )
            });
            captured_mux.replace(Some(mux.clone()));
            AppView::new(controller, agent_controller, mux, window, cx)
        });
        let mux: Entity<MuxClient> = mux_slot.borrow().clone().expect("captured mux");
        let snapshot = two_pane_snapshot(PaneId(2));
        let generation = snapshot.generation;
        mux.update(cx, |mux, cx| {
            mux.attach_snapshot_for_test(SessionId(0), snapshot, cx);
        });
        cx.run_until_parked();

        workspace.update(cx, |workspace, cx| {
            workspace.set_split_drag(Some(SplitDragState {
                drag: SplitDrag {
                    window: WindowId(0),
                    split: SplitId(0),
                    start_ratio: 0.5,
                    axis: Axis::Horizontal,
                },
                ratio: 0.6,
                committed_snapshot_revision: Some(workspace.snapshot_revision),
            }));
            cx.notify();
        });
        cx.run_until_parked();
        assert!(workspace.read_with(cx, |workspace, _| workspace.split_drag.is_some()));

        let acknowledged = two_pane_snapshot(PaneId(2));
        assert_eq!(acknowledged.generation, generation);
        mux.update(cx, |mux, cx| {
            mux.attach_snapshot_for_test(SessionId(0), acknowledged, cx);
        });
        cx.run_until_parked();
        assert!(workspace.read_with(cx, |workspace, _| workspace.split_drag.is_none()));
    }

    fn settled_float_drag_drops_its_preview(
        cx: &mut TestAppContext,
        reply: impl Fn(u64) -> zz_protocol::CommandResponse,
    ) {
        cx.update(zz_ui::init);
        let mux_slot = Rc::new(RefCell::new(None));
        let captured_mux = Rc::clone(&mux_slot);
        let (workspace, cx) = cx.add_window_view(move |window, cx| {
            let controller = cx.new(|cx| {
                crate::browser::controller::BrowserController::new(
                    Err(zz_browser::BrowserError::AlreadyShutdown),
                    cx,
                )
            });
            let agent_controller = cx.new(|_| AgentController::new());
            let mux = cx.new(|cx| {
                MuxClient::new(
                    Err(zz_daemon_client::DaemonError::Thread(
                        "test client".to_owned(),
                    )),
                    zz_daemon_client::default_socket_path(),
                    cx,
                )
            });
            captured_mux.replace(Some(mux.clone()));
            AppView::new(controller, agent_controller, mux, window, cx)
        });
        let mux: Entity<MuxClient> = mux_slot.borrow().clone().expect("captured mux");
        let tiled = PaneId(0);
        let floating = PaneId(1);
        let mut snapshot = one_pane_snapshot(1);
        let window = &mut snapshot.sessions[0].windows[0];
        let mut float_pane = window.panes[&tiled].clone();
        float_pane.id = floating;
        window.panes.insert(floating, float_pane);
        let float = FloatingPaneSnapshot {
            pane: floating,
            xoff: 4,
            yoff: 2,
            sx: 20,
            sy: 6,
            visible: true,
            border_lines: PaneBorderLines::Single,
            border_status: PaneBorderStatus::Off,
        };
        window.floating = vec![float];
        mux.update(cx, |mux, cx| {
            mux.attach_snapshot_for_test(SessionId(0), snapshot, cx);
        });
        cx.run_until_parked();
        let drawn = |cx: &mut ala::VisualTestContext| {
            cx.update(|window, cx| {
                _ = window.draw(cx);
            });
            cx.debug_bounds("floating-pane-1")
                .expect("a visible float is drawn")
                .origin
        };
        let resting = drawn(cx);

        workspace.update(cx, |workspace, cx| {
            workspace.float_drag = Some(FloatDragState {
                window: WindowId(0),
                pane: floating,
                grip: FloatGrip::MOVE,
                lines: PaneBorderLines::Single,
                area: FloatWindow {
                    border_status: PaneBorderStatus::Off,
                    rows: 24,
                },
                start: FloatCells::from(&float),
                press: Point::default(),
                cell: ala::size(px(8.0), px(16.0)),
                delta: (3, 2),
                started: true,
                committed_snapshot_revision: Some(workspace.snapshot_revision),
                request: Some(7),
                pip: None,
                limit: None,
            });
            cx.notify();
        });
        mux.read_with(cx, |mux, _| mux.watch_command_for_test(7));
        cx.run_until_parked();
        assert_ne!(drawn(cx), resting, "a committed drag previews the move");
        let revision = workspace.read_with(cx, |workspace, _| workspace.snapshot_revision);

        let settle = |request_id, cx: &mut ala::VisualTestContext| {
            mux.update(cx, |mux, cx| {
                mux.handle_message_for_test(
                    zz_protocol::ProtocolMessage::CommandResponse(reply(request_id)),
                    cx,
                );
            });
            cx.run_until_parked();
        };
        settle(6, cx);
        assert!(
            workspace.read_with(cx, |workspace, _| workspace.float_drag.is_some()),
            "another request's reply leaves the drag alone"
        );

        settle(7, cx);
        workspace.read_with(cx, |workspace, _| {
            assert!(workspace.float_drag.is_none());
            assert_eq!(workspace.snapshot_revision, revision);
        });
        assert_eq!(drawn(cx), resting, "the settled drag left its preview up");

        workspace.update(cx, |workspace, cx| {
            workspace.begin_float_drag(
                WindowId(0),
                PaneBorderStatus::Off,
                float,
                FloatGrip::MOVE,
                ala::size(px(8.0), px(16.0)),
                Point::default(),
                cx,
            );
            assert!(
                workspace
                    .float_drag
                    .is_some_and(|drag| drag.committed_snapshot_revision.is_none()),
                "a settled drag no longer blocks the next one"
            );
        });
    }

    #[ala::test]
    fn a_refused_float_drag_drops_its_preview_without_a_snapshot(cx: &mut TestAppContext) {
        settled_float_drag_drops_its_preview(cx, |request_id| {
            zz_protocol::CommandResponse::Error {
                request_id,
                error: zz_protocol::ServerError::InvalidCommand("client is read-only".to_owned()),
                output: zz_protocol::RawText::default(),
            }
        });
    }

    #[ala::test]
    fn an_accepted_float_drag_that_changed_nothing_drops_its_preview(cx: &mut TestAppContext) {
        settled_float_drag_drops_its_preview(cx, |request_id| {
            zz_protocol::CommandResponse::Success {
                request_id,
                output: zz_protocol::RawText::default(),
                exit_code: 0,
                stderr: String::new(),
                stdout_claim: zz_protocol::StdoutClaim::None,
            }
        });
    }

    #[ala::test]
    fn a_float_drag_measures_rows_from_the_daemon_window(cx: &mut TestAppContext) {
        cx.update(zz_ui::init);
        let mux_slot = Rc::new(RefCell::new(None));
        let captured_mux = Rc::clone(&mux_slot);
        let (workspace, cx) = cx.add_window_view(move |window, cx| {
            let controller = cx.new(|cx| {
                crate::browser::controller::BrowserController::new(
                    Err(zz_browser::BrowserError::AlreadyShutdown),
                    cx,
                )
            });
            let agent_controller = cx.new(|_| AgentController::new());
            let mux = cx.new(|cx| {
                MuxClient::new(
                    Err(zz_daemon_client::DaemonError::Thread(
                        "test client".to_owned(),
                    )),
                    zz_daemon_client::default_socket_path(),
                    cx,
                )
            });
            captured_mux.replace(Some(mux.clone()));
            AppView::new(controller, agent_controller, mux, window, cx)
        });
        let mux: Entity<MuxClient> = mux_slot.borrow().clone().expect("captured mux");
        let tiled = PaneId(0);
        let floating = PaneId(1);
        let mut snapshot = one_pane_snapshot(1);
        let window = &mut snapshot.sessions[0].windows[0];
        let mut float_pane = window.panes.remove(&tiled).expect("the one pane to float");
        float_pane.id = floating;
        window.panes.insert(floating, float_pane);
        window.layout = LayoutNode::Empty;
        window.active_pane = floating;
        window.pane_order = vec![floating];
        window.pane_z_order = vec![floating];
        window.sx = 100;
        window.sy = 37;
        let float = FloatingPaneSnapshot {
            pane: floating,
            xoff: 4,
            yoff: 30,
            sx: 20,
            sy: 6,
            visible: true,
            border_lines: PaneBorderLines::Single,
            border_status: PaneBorderStatus::Off,
        };
        window.floating = vec![float];
        window.pane_border_status = PaneBorderStatus::Bottom;
        window.layout_dump =
            r#"{"V":2,"L":{"t":"p","w":22,"h":8,"x":3,"y":29,"a":true,"i":0,"z":0,"I":"%1"}}"#
                .to_owned();
        window.visible_layout_dump.clone_from(&window.layout_dump);
        mux.update(cx, |mux, cx| {
            mux.attach_snapshot_for_test(SessionId(0), snapshot, cx);
        });
        cx.run_until_parked();

        workspace.update(cx, |workspace, cx| {
            workspace.begin_float_drag(
                WindowId(0),
                PaneBorderStatus::Bottom,
                float,
                FloatGrip::MOVE,
                ala::size(px(8.0), px(16.0)),
                Point::default(),
                cx,
            );
            assert_eq!(
                workspace.float_drag.map(|drag| drag.area.rows),
                Some(37),
                "a window with no tiled pane judges the bottom status row on the daemon's height"
            );
        });
    }

    #[ala::test]
    fn a_dragged_pip_follows_the_pointer_and_lands_in_the_third_it_was_dropped_in(
        cx: &mut TestAppContext,
    ) {
        let (workspace, mux, cx) = float_workspace_for_test(cx);
        let mut snapshot = tile_and_float_snapshot(1, false);
        let window = &mut snapshot.sessions[0].windows[0];
        window.sx = 80;
        window.sy = 24;
        let float = window.floating[0];
        cx.update(|_, cx| {
            let mut config = config::AppConfig::default();
            config.pane_gaps.value = true;
            config.pane_margin.value = 32.0;
            cx.set_global(config);
        });
        mux.update(cx, |mux, cx| {
            mux.connect_for_test();
            mux.attach_snapshot_for_test(SessionId(0), snapshot, cx);
        });
        cx.run_until_parked();
        let drawn = |cx: &mut ala::VisualTestContext| {
            cx.update(|window, cx| {
                _ = window.draw(cx);
            });
            cx.debug_bounds("floating-pane-1")
                .expect("a visible float is drawn")
                .origin
        };
        for _ in 0..120 {
            cx.executor().advance_clock(Duration::from_millis(16));
            drawn(cx);
            if !workspace.read_with(cx, |workspace, _| workspace.pane_motion.moving()) {
                break;
            }
        }
        let resting = drawn(cx);
        assert!(
            workspace.read_with(cx, |workspace, _| workspace.pane_canvas_bounds.get().origin)
                != Point::default(),
            "the canvas sits off the window origin, as it does beside a sidebar"
        );
        let header = cx
            .debug_bounds("pip-header")
            .expect("a PiP draws the pane header in its top border");
        let title = point(header.left() + px(30.0), header.center().y);
        let grabbed = title + point(px(4.0), px(0.0));
        cx.simulate_mouse_down(title, MouseButton::Left, Modifiers::default());
        for nudge in [title + point(px(3.0), px(0.0)), grabbed] {
            cx.simulate_mouse_move(nudge, MouseButton::Left, Modifiers::default());
        }
        for step in 1..=5 {
            cx.simulate_mouse_move(
                grabbed + point(px(300.0 * step as f32), px(180.0 * step as f32)),
                MouseButton::Left,
                Modifiers::default(),
            );
        }
        assert_eq!(drawn(cx), resting + point(px(1500.0), px(900.0)));
        cx.simulate_mouse_up(
            grabbed + point(px(1500.0), px(900.0)),
            MouseButton::Left,
            Modifiers::default(),
        );
        cx.run_until_parked();
        workspace.read_with(cx, |workspace, _| {
            assert!(workspace.float_drag.is_none());
            let landing = workspace
                .pip_landing
                .expect("a thrown PiP lands on its place");
            assert_eq!(
                landing.target,
                zz_client::floating::FloatPlace::BottomRight.cells(
                    FloatCells::from(&float),
                    (80, 24),
                    PaneBorderLines::Single
                )
            );
        });
    }

    #[ala::test]
    fn focusing_a_tile_slides_the_pip_past_the_edge_and_a_click_brings_it_back(
        cx: &mut TestAppContext,
    ) {
        let (workspace, mux, cx) = float_workspace_for_test(cx);
        let mut snapshot = tile_and_float_snapshot(1, false);
        let window = &mut snapshot.sessions[0].windows[0];
        window.sx = 80;
        window.sy = 24;
        window.floating[0].xoff = 58;
        mux.update(cx, |mux, cx| {
            mux.connect_for_test();
            mux.attach_snapshot_for_test(SessionId(0), snapshot.clone(), cx);
        });
        cx.run_until_parked();
        let settle = |cx: &mut ala::VisualTestContext| {
            for _ in 0..120 {
                cx.executor().advance_clock(Duration::from_millis(16));
                cx.update(|window, cx| {
                    _ = window.draw(cx);
                });
                if !workspace.read_with(cx, |workspace, _| workspace.pane_motion.moving()) {
                    break;
                }
            }
            cx.debug_bounds("floating-pane-1")
                .expect("a visible float is drawn")
        };
        let resting = settle(cx);
        let canvas = workspace.read_with(cx, |workspace, _| workspace.pane_canvas_bounds.get());
        assert!(resting.right() <= canvas.right());
        snapshot.generation = 2;
        snapshot.sessions[0].windows[0].active_pane = PaneId(0);
        mux.update(cx, |mux, cx| {
            mux.attach_snapshot_for_test(SessionId(0), snapshot.clone(), cx);
        });
        cx.run_until_parked();
        let stashed = settle(cx);
        assert_eq!(stashed.left(), canvas.right() - px(60.0));
        cx.simulate_click(
            point(canvas.right() - px(30.0), stashed.center().y),
            Modifiers::default(),
        );
        cx.run_until_parked();
        assert_eq!(
            settle(cx),
            resting,
            "a click on the peek brings the PiP back"
        );
        let sent = mux.read_with(cx, |mux, _| mux.executed_for_test());
        assert!(
            sent.iter()
                .any(|command| command.name == "select-pane" && command.args.contains(&"%1".into())),
            "{sent:?}"
        );
    }

    #[ala::test]
    fn a_pip_is_thrown_by_the_title_in_its_pane_header(cx: &mut TestAppContext) {
        let (workspace, mux, cx) = float_workspace_for_test(cx);
        let mut snapshot = tile_and_float_snapshot(1, false);
        let window = &mut snapshot.sessions[0].windows[0];
        window.sx = 80;
        window.sy = 24;
        mux.update(cx, |mux, cx| {
            mux.connect_for_test();
            mux.attach_snapshot_for_test(SessionId(0), snapshot, cx);
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
        let header = cx
            .debug_bounds("pip-header")
            .expect("a PiP draws the pane header in its top border");
        let title = point(header.left() + px(30.0), header.center().y);
        cx.simulate_mouse_down(title, MouseButton::Left, Modifiers::default());
        for step in 1..=6 {
            cx.simulate_mouse_move(
                title + point(px(100.0 * step as f32), px(60.0 * step as f32)),
                MouseButton::Left,
                Modifiers::default(),
            );
        }
        cx.simulate_mouse_up(
            title + point(px(600.0), px(360.0)),
            MouseButton::Left,
            Modifiers::default(),
        );
        cx.run_until_parked();
        workspace.read_with(cx, |workspace, _| {
            assert!(workspace.float_drag.is_none());
            assert!(
                workspace.pip_landing.is_some(),
                "letting go of the title throws the PiP"
            );
        });
        let sent = mux.read_with(cx, |mux, _| mux.executed_for_test());
        assert!(
            sent.iter()
                .any(|command| command.name == "move-pane" && command.args.contains(&"-P".into())),
            "{sent:?}"
        );
    }

    fn float_workspace_for_test(
        cx: &mut TestAppContext,
    ) -> (
        Entity<AppView>,
        Entity<MuxClient>,
        &mut ala::VisualTestContext,
    ) {
        cx.update(zz_ui::init);
        let mux_slot = Rc::new(RefCell::new(None));
        let captured_mux = Rc::clone(&mux_slot);
        let (workspace, cx) = cx.add_window_view(move |window, cx| {
            let controller = cx.new(|cx| {
                crate::browser::controller::BrowserController::new(
                    Err(zz_browser::BrowserError::AlreadyShutdown),
                    cx,
                )
            });
            let agent_controller = cx.new(|_| AgentController::new());
            let mux = cx.new(|cx| {
                MuxClient::new(
                    Err(zz_daemon_client::DaemonError::Thread(
                        "test client".to_owned(),
                    )),
                    zz_daemon_client::default_socket_path(),
                    cx,
                )
            });
            captured_mux.replace(Some(mux.clone()));
            AppView::new(controller, agent_controller, mux, window, cx)
        });
        let mux: Entity<MuxClient> = mux_slot.borrow().clone().expect("captured mux");
        (workspace, mux, cx)
    }

    fn tile_and_float_snapshot(generation: u64, zoomed: bool) -> MuxSnapshot {
        let tiled = PaneId(0);
        let floating = PaneId(1);
        let mut snapshot = one_pane_snapshot(generation);
        let window = &mut snapshot.sessions[0].windows[0];
        let mut float_pane = window.panes[&tiled].clone();
        float_pane.id = floating;
        window.panes.insert(floating, float_pane);
        window.floating = vec![FloatingPaneSnapshot {
            pane: floating,
            xoff: 4,
            yoff: 2,
            sx: 20,
            sy: 6,
            visible: !zoomed,
            border_lines: PaneBorderLines::Single,
            border_status: PaneBorderStatus::Off,
        }];
        window.active_pane = floating;
        window.zoomed_pane = zoomed.then_some(floating);
        snapshot
    }

    #[ala::test]
    fn float_makes_a_pip_as_tall_as_the_pane_canvas(cx: &mut TestAppContext) {
        let (workspace, mux, cx) = float_workspace_for_test(cx);
        cx.simulate_resize(ala::size(px(1065.0), px(686.0)));
        let settle = |cx: &mut ala::VisualTestContext| {
            for _ in 0..60 {
                cx.executor().advance_clock(Duration::from_millis(16));
                cx.update(|window, cx| {
                    _ = window.draw(cx);
                });
            }
        };
        mux.update(cx, |mux, cx| {
            mux.connect_for_test();
            mux.attach_snapshot_for_test(SessionId(0), one_pane_snapshot(1), cx);
        });
        cx.run_until_parked();
        settle(cx);
        let canvas = workspace.read_with(cx, |workspace, _| workspace.pane_canvas_bounds.get());
        let cell = cx.update(|window, cx| workspace.read(cx).float_cell_size(window, cx));
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a test window is a few hundred cells"
        )]
        let window_cells = (
            (canvas.size.width / cell.width).floor() as u16,
            (canvas.size.height / cell.height).floor() as u16,
        );
        let mut snapshot = one_pane_snapshot(2);
        (
            snapshot.sessions[0].windows[0].sx,
            snapshot.sessions[0].windows[0].sy,
        ) = window_cells;
        mux.update(cx, |mux, cx| {
            mux.attach_snapshot_for_test(SessionId(0), snapshot, cx);
        });
        cx.run_until_parked();
        settle(cx);
        let float = cx
            .debug_bounds("pane-action-Float")
            .expect("the tiled header has Float");
        cx.simulate_click(float.center(), Modifiers::default());
        cx.run_until_parked();
        let sent = mux.read_with(cx, |mux, _| mux.executed_for_test());
        let rows: u16 = sent
            .iter()
            .find(|command| command.name == "break-pane")
            .and_then(|command| {
                let at = command.args.iter().position(|arg| arg == "-y")?;
                command.args.get(at + 1)?.parse().ok()
            })
            .unwrap_or_else(|| panic!("Float sent no break-pane -y: {sent:?}"));

        let mut floated = tile_and_float_snapshot(3, false);
        let window = &mut floated.sessions[0].windows[0];
        (window.sx, window.sy) = window_cells;
        let cells = zz_client::floating::FloatPlace::BottomRight.cells(
            FloatCells {
                xoff: 0,
                yoff: 0,
                sx: window_cells.0 / 3,
                sy: rows - 2,
            },
            window_cells,
            PaneBorderLines::Single,
        );
        let float = &mut window.floating[0];
        (float.xoff, float.yoff, float.sx, float.sy) = (cells.xoff, cells.yoff, cells.sx, cells.sy);
        mux.update(cx, |mux, cx| {
            mux.attach_snapshot_for_test(SessionId(0), floated, cx);
        });
        cx.run_until_parked();
        settle(cx);
        let pip = cx
            .debug_bounds("floating-pane-1")
            .expect("the PiP is drawn");
        let gap = pip.top() - canvas.top();
        assert!(
            gap < cell.height + px(12.0),
            "the PiP {pip:?} stops {gap:?} under the canvas {canvas:?} top"
        );
        assert!(pip.bottom() <= canvas.bottom(), "{pip:?} {canvas:?}");
    }

    #[ala::test]
    fn a_pip_resizes_from_its_top_edge_and_top_corners(cx: &mut TestAppContext) {
        let (_workspace, mux, cx) = float_workspace_for_test(cx);
        mux.update(cx, |mux, cx| {
            mux.attach_snapshot_for_test(SessionId(0), tile_and_float_snapshot(1, false), cx);
        });
        cx.run_until_parked();
        for _ in 0..60 {
            cx.executor().advance_clock(Duration::from_millis(16));
            cx.update(|window, cx| {
                _ = window.draw(cx);
            });
        }
        let card = cx
            .debug_bounds("floating-pane-1")
            .expect("the PiP is drawn");
        for (grip, bounds) in [
            ("top", cx.debug_bounds("floating-grip-1-top")),
            ("top-left", cx.debug_bounds("floating-grip-1-top-left")),
            ("top-right", cx.debug_bounds("floating-grip-1-top-right")),
        ] {
            let bounds = bounds.unwrap_or_else(|| panic!("the PiP has no {grip} grip"));
            assert!(
                bounds.top() < card.top() && card.top() < bounds.bottom(),
                "the {grip} grip {bounds:?} does not straddle the PiP's top edge {card:?}"
            );
        }
        assert!(cx.debug_bounds("floating-grip-1-move").is_none());
    }

    #[ala::test]
    fn a_zoomed_float_reports_its_grid_like_a_tile(cx: &mut TestAppContext) {
        let (workspace, mux, cx) = float_workspace_for_test(cx);
        let floating = PaneId(1);
        let report = |cx: &mut ala::VisualTestContext| {
            let terminal =
                workspace.read_with(cx, |workspace, _| workspace.terminals[&floating].clone());
            let sent = mux.update(cx, |mux, _| mux.record_input_for_test());
            terminal.update(cx, |terminal, cx| {
                terminal.update_geometry(
                    GridSize {
                        columns: 120,
                        rows: 24,
                        cell_width_px: 8,
                        cell_height_px: 16,
                    },
                    Bounds::new(point(px(0.0), px(0.0)), ala::size(px(960.0), px(384.0))),
                    Bounds::new(point(px(0.0), px(0.0)), ala::size(px(960.0), px(384.0))),
                    px(8.0),
                    px(16.0),
                    px(0.0),
                    None,
                    None,
                    cx,
                );
            });
            sent.borrow()
                .iter()
                .filter_map(|message| match message {
                    InputMessage::ResizeTerminalV2 {
                        pane,
                        columns,
                        rows,
                        ..
                    } => Some((*pane, *columns, *rows)),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };

        mux.update(cx, |mux, cx| {
            mux.attach_snapshot_for_test(SessionId(0), tile_and_float_snapshot(1, false), cx);
        });
        cx.run_until_parked();
        assert_eq!(report(cx), Vec::new(), "a float's PTY follows its cells");

        mux.update(cx, |mux, cx| {
            mux.attach_snapshot_for_test(SessionId(0), tile_and_float_snapshot(2, true), cx);
        });
        cx.run_until_parked();
        for _ in 0..120 {
            cx.executor().advance_clock(Duration::from_millis(16));
            cx.update(|window, cx| {
                _ = window.draw(cx);
            });
            if !workspace.read_with(cx, |workspace, _| workspace.pane_motion.moving()) {
                break;
            }
        }
        assert_eq!(
            report(cx),
            vec![(floating, 120, 24)],
            "a zoomed float fills the window, so its size drives the window extent"
        );
    }

    #[ala::test]
    fn display_panes_labels_a_float_in_its_own_frame(cx: &mut TestAppContext) {
        let (_workspace, mux, cx) = float_workspace_for_test(cx);
        let mut snapshot = tile_and_float_snapshot(1, false);
        snapshot.sessions[0].windows[0].active_pane = PaneId(0);
        mux.update(cx, |mux, cx| {
            mux.attach_snapshot_for_test(SessionId(0), snapshot, cx);
        });
        cx.run_until_parked();
        let indicator = |pane: u64, index: u32| zz_protocol::PaneIndicator {
            pane: PaneId(pane),
            index,
            select_key: b'0' + u8::try_from(index).unwrap(),
            flags: if pane == 0 {
                zz_protocol::PaneIndicator::ACTIVE
            } else {
                0
            },
            label: index.to_string(),
        };
        mux.update(cx, |mux, cx| {
            mux.handle_message_for_test(
                zz_protocol::ProtocolMessage::Event(zz_protocol::Event {
                    sequence: 1,
                    payload: zz_protocol::EventPayload::DisplayPanes {
                        state: Some(zz_protocol::DisplayPanesState {
                            indicators: vec![indicator(0, 0), indicator(1, 1)],
                            ..display_panes_state_for_test()
                        }),
                    },
                }),
                cx,
            );
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
        assert!(cx.debug_bounds("pane-indicator-0").is_some());
        let frame = cx
            .debug_bounds("floating-pane-1")
            .expect("a visible float is drawn");
        let label = cx
            .debug_bounds("pane-indicator-1")
            .expect("the float carries its display-panes label");
        assert!(
            frame.contains(&label.origin) && frame.contains(&label.bottom_right()),
            "the label {label:?} sits inside the float {frame:?}"
        );
    }

    /// `rendering.geometry-residue`'s probe. The desktop client measures its
    /// pane in pixels and `terminal_grid_size` floors, so a box that is not a
    /// whole number of cells reaches the PTY one column and one row short of
    /// the box the engine laid out. The daemon's `set_pane_geometry` swallows a
    /// difference of one cell rather than re-solving the window extent from it,
    /// so `#{pane_width}` keeps answering the laid-out number while the PTY has
    /// the floored one.
    #[ala::test]
    fn attached_pane_measurement_floors_below_the_laid_out_box(cx: &mut TestAppContext) {
        cx.update(zz_ui::init);
        let mux_slot = Rc::new(RefCell::new(None));
        let captured_mux = Rc::clone(&mux_slot);
        let (workspace, cx) = cx.add_window_view(move |window, cx| {
            let controller = cx.new(|cx| {
                crate::browser::controller::BrowserController::new(
                    Err(zz_browser::BrowserError::AlreadyShutdown),
                    cx,
                )
            });
            let agent_controller = cx.new(|_| AgentController::new());
            let mux = cx.new(|cx| {
                MuxClient::new(
                    Err(zz_daemon_client::DaemonError::Thread(
                        "test client".to_owned(),
                    )),
                    zz_daemon_client::default_socket_path(),
                    cx,
                )
            });
            captured_mux.replace(Some(mux.clone()));
            AppView::new(controller, agent_controller, mux, window, cx)
        });
        let mux: Entity<MuxClient> = mux_slot.borrow().clone().expect("captured mux");
        mux.update(cx, |mux, cx| {
            mux.attach_snapshot_for_test(SessionId(0), two_pane_snapshot(PaneId(2)), cx);
        });
        cx.run_until_parked();

        let cell_width = px(8.0);
        let line_height = px(16.0);
        let laid_out = zz_ui::terminal::terminal_grid_size(
            ala::size(px(960.0), px(384.0)),
            cell_width,
            line_height,
            1.0,
        );
        assert_eq!((laid_out.columns, laid_out.rows), (120, 24));

        let drawn = zz_ui::terminal::terminal_grid_size(
            ala::size(px(959.5), px(383.5)),
            cell_width,
            line_height,
            1.0,
        );
        assert_eq!((drawn.columns, drawn.rows), (119, 23));

        let terminal =
            workspace.read_with(cx, |workspace, _| workspace.terminals[&PaneId(0)].clone());
        let sent = mux.update(cx, |mux, _| mux.record_input_for_test());
        terminal.update(cx, |terminal, cx| {
            terminal.update_geometry(
                drawn,
                Bounds::new(point(px(0.0), px(0.0)), ala::size(px(959.5), px(383.5))),
                Bounds::new(point(px(0.0), px(0.0)), ala::size(px(959.5), px(383.5))),
                cell_width,
                line_height,
                px(0.0),
                None,
                None,
                cx,
            );
        });

        let sent = sent.borrow();
        assert_eq!(sent.len(), 1);
        assert!(matches!(
            &sent[0],
            InputMessage::ResizeTerminalV2 {
                pane,
                columns,
                rows,
                layout_generation: 0,
                ..
            } if *pane == PaneId(0) && *columns == 119 && *rows == 23
        ));
    }

    #[ala::test]
    fn split_drag_defers_terminal_resize_until_override_release(cx: &mut TestAppContext) {
        cx.update(zz_ui::init);
        let mux_slot = Rc::new(RefCell::new(None));
        let captured_mux = Rc::clone(&mux_slot);
        let (workspace, cx) = cx.add_window_view(move |window, cx| {
            let controller = cx.new(|cx| {
                crate::browser::controller::BrowserController::new(
                    Err(zz_browser::BrowserError::AlreadyShutdown),
                    cx,
                )
            });
            let agent_controller = cx.new(|_| AgentController::new());
            let mux = cx.new(|cx| {
                MuxClient::new(
                    Err(zz_daemon_client::DaemonError::Thread(
                        "test client".to_owned(),
                    )),
                    zz_daemon_client::default_socket_path(),
                    cx,
                )
            });
            captured_mux.replace(Some(mux.clone()));
            AppView::new(controller, agent_controller, mux, window, cx)
        });
        let mux: Entity<MuxClient> = mux_slot.borrow().clone().expect("captured mux");
        mux.update(cx, |mux, cx| {
            mux.attach_snapshot_for_test(SessionId(0), two_pane_snapshot(PaneId(2)), cx);
        });
        cx.run_until_parked();

        let terminal =
            workspace.read_with(cx, |workspace, _| workspace.terminals[&PaneId(0)].clone());
        let sent = mux.update(cx, |mux, _| mux.record_input_for_test());
        let update_geometry = |grid_size, cx: &mut ala::VisualTestContext| {
            terminal.update(cx, |terminal, cx| {
                terminal.update_geometry(
                    grid_size,
                    Bounds::new(point(px(0.0), px(0.0)), ala::size(px(960.0), px(384.0))),
                    Bounds::new(point(px(0.0), px(0.0)), ala::size(px(960.0), px(384.0))),
                    px(8.0),
                    px(16.0),
                    px(0.0),
                    None,
                    None,
                    cx,
                );
            });
        };
        let before = GridSize {
            columns: 80,
            rows: 24,
            cell_width_px: 8,
            cell_height_px: 16,
        };
        let during = GridSize {
            columns: 120,
            ..before
        };
        update_geometry(before, cx);
        sent.borrow_mut().clear();

        workspace.update(cx, |workspace, _| {
            workspace.set_split_drag(Some(SplitDragState {
                drag: SplitDrag {
                    window: WindowId(0),
                    split: SplitId(0),
                    start_ratio: 0.5,
                    axis: Axis::Horizontal,
                },
                ratio: 0.6,
                committed_snapshot_revision: Some(workspace.snapshot_revision),
            }));
        });
        update_geometry(during, cx);
        assert!(sent.borrow().is_empty());

        workspace.update(cx, |workspace, cx| {
            workspace.snapshot_revision = workspace.snapshot_revision.wrapping_add(1);
            workspace.reconcile_split_drag(None, cx);
        });
        update_geometry(during, cx);
        {
            let sent = sent.borrow();
            assert_eq!(sent.len(), 1);
            assert!(matches!(
                &sent[0],
                InputMessage::ResizeTerminalV2 {
                    pane,
                    columns,
                    rows,
                    cell_width_px,
                    cell_height_px,
                    layout_generation: 0,
                } if *pane == PaneId(0)
                    && *columns == 120
                    && *rows == 24
                    && *cell_width_px == 8
                    && *cell_height_px == 16
            ));
        }
        update_geometry(during, cx);
        assert_eq!(sent.borrow().len(), 1);
        sent.borrow_mut().clear();

        workspace.update(cx, |workspace, _| {
            workspace.set_split_drag(Some(SplitDragState {
                drag: SplitDrag {
                    window: WindowId(0),
                    split: SplitId(0),
                    start_ratio: 0.5,
                    axis: Axis::Horizontal,
                },
                ratio: 0.4,
                committed_snapshot_revision: Some(workspace.snapshot_revision),
            }));
        });
        update_geometry(before, cx);
        workspace.update(cx, |workspace, cx| {
            workspace.snapshot_revision = workspace.snapshot_revision.wrapping_add(1);
            workspace.reconcile_split_drag(None, cx);
        });
        update_geometry(during, cx);
        assert!(sent.borrow().is_empty());
    }

    #[ala::test]
    fn dialog_prefix_cancel_barrier_handles_acknowledgements_and_connection_reset(
        cx: &mut TestAppContext,
    ) {
        cx.update(zz_ui::init);
        let observed = Rc::new(RefCell::new(Vec::new()));
        let captured_observed = Rc::clone(&observed);
        cx.update(move |cx| {
            cx.observe_keystrokes(move |event, _, _| {
                captured_observed.borrow_mut().push((
                    event.keystroke.modifiers.platform,
                    event.keystroke.modifiers.function,
                    event.keystroke.key.clone(),
                ));
            })
            .detach();
        });
        let workspace_slot = Rc::new(RefCell::new(None));
        let mux_slot = Rc::new(RefCell::new(None));
        let input_slot = Rc::new(RefCell::new(None));
        let captured_workspace = Rc::clone(&workspace_slot);
        let captured_mux = Rc::clone(&mux_slot);
        let captured_input = Rc::clone(&input_slot);
        let (_, cx) = cx.add_window_view(move |window, cx| {
            let controller = cx.new(|cx| {
                crate::browser::controller::BrowserController::new(
                    Err(zz_browser::BrowserError::AlreadyShutdown),
                    cx,
                )
            });
            let agent_controller = cx.new(|_| AgentController::new());
            let mux = cx.new(|cx| {
                MuxClient::new(
                    Err(zz_daemon_client::DaemonError::Thread(
                        "test client".to_owned(),
                    )),
                    zz_daemon_client::default_socket_path(),
                    cx,
                )
            });
            let input = mux.update(cx, |mux, _| mux.record_input_for_test());
            let workspace =
                cx.new(|cx| AppView::new(controller, agent_controller, mux.clone(), window, cx));
            captured_workspace.replace(Some(workspace.clone()));
            captured_mux.replace(Some(mux));
            captured_input.replace(Some(input));
            zz_ui::Root::new(workspace, window, cx)
        });
        let workspace = workspace_slot.borrow().clone().expect("captured workspace");
        let mux = mux_slot.borrow().clone().expect("captured mux");
        let input = input_slot.borrow().clone().expect("captured input");
        input.borrow_mut().clear();

        cx.update(|window, cx| window.open_dialog(cx, |dialog, _, _| dialog));
        cx.run_until_parked();
        assert_eq!(
            input.borrow().as_slice(),
            &[InputMessage::CancelPrefix { request_id: 1 }]
        );
        assert_eq!(
            workspace.read_with(cx, |workspace, _| workspace.dialog_prefix_cancel_pending),
            Some(1)
        );

        cx.update(zz_ui::WindowExt::close_dialog);
        cx.simulate_keystrokes("c cmd-dialogtest fn-dialogtest");
        assert_eq!(
            input.borrow().as_slice(),
            &[InputMessage::CancelPrefix { request_id: 1 }]
        );
        assert_eq!(
            observed.borrow().as_slice(),
            &[
                (true, false, "dialogtest".to_owned()),
                (false, true, "dialogtest".to_owned()),
            ]
        );

        mux.update(cx, |mux, cx| {
            mux.acknowledge_prefix_cancel_for_test(1, cx);
        });
        cx.run_until_parked();
        assert_eq!(
            workspace.read_with(cx, |workspace, _| workspace.dialog_prefix_cancel_pending),
            None
        );
        cx.simulate_keystrokes("c");
        assert_eq!(
            observed.borrow().as_slice(),
            &[
                (true, false, "dialogtest".to_owned()),
                (false, true, "dialogtest".to_owned()),
                (false, false, "c".to_owned()),
            ]
        );

        cx.update(|window, cx| window.open_dialog(cx, |dialog, _, _| dialog));
        cx.run_until_parked();
        assert_eq!(
            input.borrow().as_slice(),
            &[
                InputMessage::CancelPrefix { request_id: 1 },
                InputMessage::CancelPrefix { request_id: 2 },
            ]
        );
        cx.update(zz_ui::WindowExt::close_dialog);
        mux.update(cx, |mux, cx| {
            mux.acknowledge_prefix_cancel_for_test(1, cx);
        });
        cx.run_until_parked();
        assert_eq!(
            workspace.read_with(cx, |workspace, _| workspace.dialog_prefix_cancel_pending),
            Some(2)
        );
        mux.update(cx, |mux, cx| {
            mux.acknowledge_prefix_cancel_for_test(2, cx);
        });
        cx.run_until_parked();
        assert_eq!(
            workspace.read_with(cx, |workspace, _| workspace.dialog_prefix_cancel_pending),
            None
        );

        cx.update(|window, cx| window.open_dialog(cx, |dialog, _, _| dialog));
        cx.run_until_parked();
        assert_eq!(
            input.borrow().as_slice(),
            &[
                InputMessage::CancelPrefix { request_id: 1 },
                InputMessage::CancelPrefix { request_id: 2 },
                InputMessage::CancelPrefix { request_id: 3 },
            ]
        );
        cx.update(zz_ui::WindowExt::close_dialog);
        mux.update(cx, |mux, cx| {
            mux.reset_session_state_for_test(cx);
            mux.acknowledge_prefix_cancel_for_test(2, cx);
        });
        cx.run_until_parked();
        assert_eq!(
            mux.read_with(cx, |mux, _| mux.prefix_cancelled_request()),
            Some(3)
        );
        assert_eq!(
            workspace.read_with(cx, |workspace, _| workspace.dialog_prefix_cancel_pending),
            None
        );
        cx.simulate_keystrokes("r");
        assert_eq!(
            observed.borrow().as_slice(),
            &[
                (true, false, "dialogtest".to_owned()),
                (false, true, "dialogtest".to_owned()),
                (false, false, "c".to_owned()),
                (false, false, "r".to_owned()),
            ]
        );
    }

    #[ala::test]
    fn directional_focus_crosses_pane_kinds(cx: &mut TestAppContext) {
        cx.update(zz_ui::init);
        let mux_slot = Rc::new(RefCell::new(None));
        let captured_mux = Rc::clone(&mux_slot);
        let (workspace, cx) = cx.add_window_view(move |window, cx| {
            let controller = cx.new(|cx| {
                crate::browser::controller::BrowserController::new(
                    Err(zz_browser::BrowserError::AlreadyShutdown),
                    cx,
                )
            });
            let agent_controller = cx.new(|_| AgentController::new());
            let mux = cx.new(|cx| {
                MuxClient::new(
                    Err(zz_daemon_client::DaemonError::Thread(
                        "test client".to_owned(),
                    )),
                    zz_daemon_client::default_socket_path(),
                    cx,
                )
            });
            captured_mux.replace(Some(mux.clone()));
            AppView::new(controller, agent_controller, mux, window, cx)
        });
        let mux: Entity<MuxClient> = mux_slot.borrow().clone().expect("captured mux");

        let publish = |snapshot: MuxSnapshot, cx: &mut ala::VisualTestContext| {
            mux.update(cx, |mux, cx| {
                mux.attach_snapshot_for_test(SessionId(0), snapshot, cx);
            });
            cx.run_until_parked();
        };
        let activate = |pane: PaneId, cx: &mut ala::VisualTestContext| {
            publish(two_pane_snapshot(pane), cx);
        };

        publish(one_pane_snapshot(1), cx);
        publish(two_pane_snapshot(PaneId(2)), cx);
        let picker_after_split = workspace.read_with(cx, |workspace, cx| {
            workspace
                .pickers
                .get(&PaneId(2))
                .expect("picker entity")
                .read(cx)
                .focus()
                .clone()
        });
        assert!(
            cx.update(|window, cx| picker_after_split.contains_focused(window, cx)),
            "the picker raised by a split never took focus"
        );
        let terminal_has_the_keyboard = |cx: &mut ala::VisualTestContext| {
            cx.update(|window, _| {
                window
                    .context_stack()
                    .iter()
                    .any(|context| context.contains("Terminal"))
            })
        };

        activate(PaneId(0), cx);
        let terminal_focus = workspace.read_with(cx, |workspace, cx| {
            workspace
                .terminals
                .get(&PaneId(0))
                .expect("terminal entity")
                .read(cx)
                .focus()
        });
        assert!(
            cx.update(|window, cx| terminal_focus.contains_focused(window, cx))
                && terminal_has_the_keyboard(cx),
            "the terminal never took focus"
        );

        activate(PaneId(2), cx);
        assert!(
            cx.update(|window, cx| picker_after_split.contains_focused(window, cx)),
            "terminal -> picker left the window unfocused"
        );

        activate(PaneId(0), cx);
        assert!(
            cx.update(|window, cx| terminal_focus.contains_focused(window, cx))
                && terminal_has_the_keyboard(cx),
            "picker -> terminal left the window unfocused"
        );

        activate(PaneId(2), cx);
        mux.update(cx, |mux, cx| mux.set_prefix_armed_for_test(true, cx));
        cx.run_until_parked();
        cx.simulate_keystrokes("ctrl-a");
        cx.simulate_keystrokes("h");
        mux.update(cx, |mux, cx| mux.set_prefix_armed_for_test(false, cx));
        activate(PaneId(0), cx);
        assert!(
            terminal_has_the_keyboard(cx),
            "the prefix chord left the terminal without the keyboard"
        );

        for round in 0..3 {
            activate(PaneId(2), cx);
            assert!(
                cx.update(|window, cx| picker_after_split.contains_focused(window, cx)),
                "round {round}: terminal -> picker left the window unfocused"
            );
            activate(PaneId(0), cx);
            assert!(
                cx.update(|window, cx| terminal_focus.contains_focused(window, cx))
                    && terminal_has_the_keyboard(cx),
                "round {round}: picker -> terminal left the window unfocused"
            );
        }

        activate(PaneId(0), cx);
        cx.update(ala::Window::blur);
        let mut same_pane_again = two_pane_snapshot(PaneId(0));
        same_pane_again.generation = 99;
        publish(same_pane_again, cx);
        assert!(
            terminal_has_the_keyboard(cx),
            "a stranded window focus was never handed back to the active pane"
        );

        let typed = mux.update(cx, |mux, _| mux.record_input_for_test());
        activate(PaneId(2), cx);
        activate(PaneId(0), cx);
        cx.simulate_input("x");
        assert!(
            typed.borrow().iter().any(|input| matches!(
                input,
                InputMessage::Text { pane, text } if *pane == PaneId(0) && text == "x"
            )),
            "the terminal took no typed text after picker -> terminal: {:?}",
            typed.borrow()
        );
        typed.borrow_mut().clear();

        activate(PaneId(2), cx);
        publish(
            two_pane_snapshot_of(PaneId(2), PaneKindSnapshot::Terminal),
            cx,
        );
        let materialized = workspace.read_with(cx, |workspace, cx| {
            workspace
                .terminals
                .get(&PaneId(2))
                .expect("the answered picker becomes a terminal")
                .read(cx)
                .focus()
        });
        assert!(
            cx.update(|window, cx| materialized.contains_focused(window, cx))
                && terminal_has_the_keyboard(cx),
            "the pane the picker turned into never took the keyboard"
        );
    }

    #[ala::test]
    fn a_cross_host_attach_hands_the_keyboard_to_the_machine_it_lands_on(cx: &mut TestAppContext) {
        cx.update(zz_ui::init);
        let mux_slot = Rc::new(RefCell::new(None));
        let captured_mux = Rc::clone(&mux_slot);
        let (workspace, cx) = cx.add_window_view(move |window, cx| {
            crate::config::set_fleet_hosts_for_test(
                vec![crate::config::HostEntry {
                    name: "remote".to_owned(),
                    endpoint: zz_daemon_client::Endpoint::parse("unix:///tmp/zz-cross-host.sock")
                        .expect("test endpoint"),
                }],
                cx,
            );
            let controller = cx.new(|cx| {
                crate::browser::controller::BrowserController::new(
                    Err(zz_browser::BrowserError::AlreadyShutdown),
                    cx,
                )
            });
            let agent_controller = cx.new(|_| AgentController::new());
            let mux = cx.new(|cx| {
                MuxClient::new(
                    Err(zz_daemon_client::DaemonError::Thread(
                        "test client".to_owned(),
                    )),
                    zz_daemon_client::default_socket_path(),
                    cx,
                )
            });
            captured_mux.replace(Some(mux.clone()));
            AppView::new(controller, agent_controller, mux, window, cx)
        });
        let mux: Entity<MuxClient> = mux_slot.borrow().clone().expect("captured mux");
        let publish = |snapshot: MuxSnapshot, cx: &mut ala::VisualTestContext| {
            mux.update(cx, |mux, cx| {
                mux.attach_snapshot_for_test(SessionId(0), snapshot, cx);
            });
            cx.run_until_parked();
        };
        let terminal_has_the_keyboard = |cx: &mut ala::VisualTestContext| {
            cx.update(|window, _| {
                window
                    .context_stack()
                    .iter()
                    .any(|context| context.contains("Terminal"))
            })
        };

        publish(one_pane_snapshot(1), cx);
        workspace.update_in(cx, |workspace, window, cx| {
            workspace
                .sidebar
                .update(cx, |sidebar, cx| sidebar.focus(window, cx));
        });
        cx.run_until_parked();
        assert!(
            !terminal_has_the_keyboard(cx),
            "the tree never took the keyboard, so the switch under test proves nothing"
        );

        mux.update(cx, |mux, cx| mux.attach_host_for_test("remote", cx));
        cx.run_until_parked();
        assert!(
            workspace.read_with(cx, |workspace, _| workspace.pane_focus_owed),
            "a machine with nothing on screen yet dropped the handover instead of holding it"
        );

        publish(one_pane_snapshot(1), cx);
        assert!(
            terminal_has_the_keyboard(cx),
            "the pane the new machine landed on never took the keyboard"
        );
    }

    #[ala::test]
    fn pane_drop_dispatches_before_deferred_teardown(cx: &mut TestAppContext) {
        let steps = Rc::new(RefCell::new(Vec::new()));
        let captured_steps = steps.clone();
        let (_, cx) = cx.add_window_view(move |_, _| PaneReleaseOrderView {
            source: PaneId(3),
            steps: captured_steps,
        });

        cx.simulate_mouse_move(
            point(px(10.0), px(10.0)),
            None::<MouseButton>,
            Modifiers::default(),
        );
        cx.simulate_mouse_down(
            point(px(10.0), px(10.0)),
            MouseButton::Left,
            Modifiers::default(),
        );
        cx.simulate_mouse_move(
            point(px(20.0), px(10.0)),
            MouseButton::Left,
            Modifiers::default(),
        );
        cx.simulate_mouse_move(
            point(px(150.0), px(10.0)),
            MouseButton::Left,
            Modifiers::default(),
        );
        cx.simulate_mouse_up(
            point(px(150.0), px(10.0)),
            MouseButton::Left,
            Modifiers::default(),
        );
        cx.run_until_parked();

        assert_eq!(
            *steps.borrow(),
            vec![
                PaneReleaseStep::Drop(pane_swap_command(PaneId(3), PaneId(7))),
                PaneReleaseStep::Teardown(PaneId(3)),
            ]
        );
    }

    #[ala::test]
    fn pane_handle_drop_and_escape_leave_keyboard_input_alone(cx: &mut TestAppContext) {
        cx.update(zz_ui::init);
        let (workspace, cx) = cx.add_window_view(|window, cx| {
            let controller = cx.new(|cx| {
                BrowserController::new(Err(zz_browser::BrowserError::AlreadyShutdown), cx)
            });
            let agent_controller = cx.new(|_| AgentController::new());
            let mux = cx.new(|cx| {
                MuxClient::new(
                    Err(zz_daemon_client::DaemonError::Thread(
                        "test client".to_owned(),
                    )),
                    zz_daemon_client::default_socket_path(),
                    cx,
                )
            });
            AppView::new(controller, agent_controller, mux, window, cx)
        });
        let mux = workspace.read_with(cx, |workspace, _| workspace.mux.clone());
        let snapshot = two_pane_snapshot(PaneId(0));
        mux.update(cx, |mux, cx| {
            mux.attach_snapshot_for_test(SessionId(0), snapshot.clone(), cx);
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        let input = mux.update(cx, |mux, _| mux.record_input_for_test());
        let bounds = workspace.read_with(cx, |workspace, _| workspace.pane_canvas_bounds.get());
        let target = bounds.origin + point(bounds.size.width * 0.75, bounds.size.height * 0.5);
        cx.simulate_mouse_move(target, None::<MouseButton>, Modifiers::default());
        let payload = PaneDrag {
            pane: PaneId(0),
            requires_prefix: false,
        };
        workspace.update_in(cx, |workspace, window, cx| {
            workspace.on_pane_drag_start(payload, cx);
            let state = workspace.pane_drag.as_mut().unwrap();
            let active_window = &snapshot.sessions[0].windows[0];
            assert!(state.matches(active_window, false));
            state.requires_prefix = true;
            assert!(!state.matches(active_window, false));
            assert!(state.matches(active_window, true));
            state.requires_prefix = false;
            let mut zoomed = active_window.clone();
            zoomed.zoomed_pane = Some(payload.pane);
            assert!(!state.matches(&zoomed, false));
            let mut floated = active_window.clone();
            let float = PaneId(9);
            let mut float_pane = floated.panes[&PaneId(0)].clone();
            float_pane.id = float;
            floated.panes.insert(float, float_pane);
            floated.floating = vec![FloatingPaneSnapshot {
                pane: float,
                xoff: 4,
                yoff: 2,
                sx: 20,
                sy: 6,
                visible: true,
                border_lines: PaneBorderLines::Single,
                border_status: PaneBorderStatus::Off,
            }];
            floated.active_pane = float;
            assert!(state.matches(&floated, false));
            assert!(state.target.is_none());
            workspace.on_pane_drop(payload, window, cx);
            assert!(workspace.pane_drag.is_none());
            assert_eq!(
                workspace.pane_layout_override.as_ref().unwrap().layout,
                predicted_drop_layout(
                    &active_window.layout,
                    PaneId(0),
                    PaneId(2),
                    DropZone::Center
                )
                .unwrap()
            );

            workspace.on_pane_drag_start(payload, cx);
            workspace.intercept_keystroke(
                &ala::KeystrokeEvent {
                    keystroke: Keystroke {
                        key: "escape".to_owned(),
                        key_char: None,
                        modifiers: Modifiers::default(),
                    },
                    action: None,
                    context_stack: Vec::new(),
                    is_held: false,
                },
                window,
                cx,
            );
            assert!(workspace.pane_drag.is_none());
            assert!(!cx.has_active_drag());
        });
        assert!(
            !input
                .borrow()
                .iter()
                .any(|message| matches!(message, InputMessage::Key { .. }))
        );
        assert!(!mux.read_with(cx, |mux, _| mux.prefix_armed()));
    }

    #[test]
    fn taking_a_completed_pane_drag_always_clears_drag_state() {
        let mut drag = Some(three_pane_drag(PaneId(3)));
        drag.as_mut()
            .expect("source belongs to the frozen layout")
            .set_target(
                Some((PaneId(7), DropZone::Top)),
                DropPreviewFrame::default(),
            );

        assert_eq!(
            take_pane_drag(&mut drag),
            Some(PaneDrag {
                pane: PaneId(3),
                requires_prefix: true
            })
        );
        assert!(drag.is_none());
        assert_eq!(take_pane_drag(&mut drag), None);
    }

    #[test]
    fn pane_selection_targets_the_requested_pane() {
        assert_eq!(
            pane_select_command(PaneId(7)),
            CommandInvocation::new("select-pane", ["-t", "%7"])
        );
    }

    #[test]
    fn an_optimistic_layout_lives_exactly_until_the_next_snapshot() {
        let pending = PaneLayoutOverride {
            window: WindowId(5),
            layout: three_pane_layout(),
            generation: 12,
        };
        let window = WindowSnapshot {
            id: WindowId(5),
            index: 0,
            name: "work".to_owned(),
            automatic_rename: true,
            active_pane: PaneId(3),
            zoomed_pane: None,
            layout: three_pane_layout(),
            panes: BTreeMap::new(),
            layout_dump: String::new(),
            visible_layout_dump: String::new(),
            status_label: String::new(),
            activity: false,
            silence: false,
            pane_border_status: zz_protocol::PaneBorderStatus::Off,
            pane_border_lines: zz_protocol::PaneBorderLines::Single,
            pane_border_indicators: zz_protocol::PaneBorderIndicators::Colour,
            pane_order: Vec::new(),
            pane_z_order: Vec::new(),
            floating: Vec::new(),
            modal: None,
            sx: 0,
            sy: 0,
            pane_base_index: 0,
        };

        assert!(pending.still_predicts(Some(&window), 12));
        assert!(!pending.still_predicts(Some(&window), 13));
        assert!(!pending.still_predicts(None, 12));
        assert!(!pending.still_predicts(
            Some(&WindowSnapshot {
                id: WindowId(6),
                ..window.clone()
            }),
            12
        ));
        assert!(!pending.still_predicts(
            Some(&WindowSnapshot {
                zoomed_pane: Some(PaneId(3)),
                ..window
            }),
            12
        ));
    }

    #[test]
    fn split_drag_ratio_tracks_each_axis_across_the_full_layout() {
        let bounds = Bounds::new(point(px(100.0), px(50.0)), ala::size(px(800.0), px(400.0)));
        assert_eq!(
            split_ratio_basis(split_ratio_from_pointer(
                Axis::Horizontal,
                point(px(500.0), px(80.0)),
                bounds,
            )),
            5_000
        );
        assert_eq!(
            split_ratio_basis(split_ratio_from_pointer(
                Axis::Vertical,
                point(px(120.0), px(350.0)),
                bounds,
            )),
            7_500
        );
        assert_eq!(
            split_ratio_basis(split_ratio_from_pointer(
                Axis::Horizontal,
                point(px(10.0), px(80.0)),
                bounds,
            )),
            0
        );
        assert_eq!(
            split_ratio_basis(split_ratio_from_pointer(
                Axis::Vertical,
                point(px(120.0), px(900.0)),
                bounds,
            )),
            10_000
        );
        assert_eq!(split_ratio_basis(0.456_74), 4_567);
    }

    type PathPickerFixture<'a> = (
        Entity<AppView>,
        Entity<MuxClient>,
        Rc<RefCell<Vec<InputMessage>>>,
        Rc<RefCell<Vec<zz_protocol::ProtocolMessage>>>,
        &'a mut ala::VisualTestContext,
    );

    fn path_picker_workspace(cx: &mut TestAppContext) -> PathPickerFixture<'_> {
        cx.update(zz_ui::init);
        let mux_slot = Rc::new(RefCell::new(None));
        let captured_mux = Rc::clone(&mux_slot);
        let (workspace, cx) = cx.add_window_view(move |window, cx| {
            let controller = cx.new(|cx| {
                BrowserController::new(Err(zz_browser::BrowserError::AlreadyShutdown), cx)
            });
            let agent_controller = cx.new(|_| AgentController::new());
            let mux = cx.new(|cx| {
                MuxClient::new(
                    Err(zz_daemon_client::DaemonError::Thread(
                        "path picker".to_owned(),
                    )),
                    zz_daemon_client::default_socket_path(),
                    cx,
                )
            });
            captured_mux.replace(Some(mux.clone()));
            AppView::new(controller, agent_controller, mux, window, cx)
        });
        let mux: Entity<MuxClient> = mux_slot.borrow().clone().expect("captured mux");
        let (input, lists) = mux.update(cx, |mux, cx| {
            mux.attach_snapshot_for_test(SessionId(0), one_pane_snapshot(1), cx);
            (
                mux.record_input_for_test(),
                mux.record_path_lists_for_test(),
            )
        });
        cx.run_until_parked();
        workspace.update_in(cx, |workspace, window, cx| {
            workspace.focus_active_pane(window, cx);
        });
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
        (workspace, mux, input, lists, cx)
    }

    fn publish_to_mux(
        mux: &Entity<MuxClient>,
        message: zz_protocol::ProtocolMessage,
        cx: &mut ala::VisualTestContext,
    ) {
        mux.update(cx, |mux, cx| mux.handle_message_for_test(message, cx));
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
    }

    fn open_path_picker_event(sequence: u64) -> zz_protocol::ProtocolMessage {
        open_path_picker_event_for(sequence, PaneId(0))
    }

    fn open_path_picker_event_for(sequence: u64, pane: PaneId) -> zz_protocol::ProtocolMessage {
        zz_protocol::ProtocolMessage::Event(zz_protocol::Event {
            sequence,
            payload: zz_protocol::EventPayload::OpenPathPicker {
                pane,
                start_dir: None,
            },
        })
    }

    fn path_list_root_for_test() -> zz_protocol::PathListRoot {
        zz_protocol::PathListRoot {
            root: "/home/me/src".to_owned(),
            display_root: "~/src/".to_owned(),
            cwd: Some("/home/me/src".to_owned()),
            insert: zz_protocol::InsertStyle::Shell(zz_protocol::ShellKind::Posix),
        }
    }

    fn palette_input(
        workspace: &Entity<AppView>,
        cx: &mut ala::VisualTestContext,
    ) -> Option<String> {
        cx.update(|window, cx| {
            let palette = workspace.read(cx).command_palette.clone()?;
            let palette = palette.read(cx);
            palette
                .focus(cx)
                .is_focused(window)
                .then(|| palette.query().to_owned())
        })
    }

    fn terminal_is_focused(workspace: &Entity<AppView>, cx: &mut ala::VisualTestContext) -> bool {
        cx.update(|window, cx| {
            workspace.read(cx).terminals[&PaneId(0)]
                .read(cx)
                .focus()
                .is_focused(window)
        })
    }

    fn answer_listing(
        mux: &Entity<MuxClient>,
        entries: Vec<zz_protocol::PathEntry>,
        cx: &mut ala::VisualTestContext,
    ) {
        publish_to_mux(
            mux,
            zz_protocol::ProtocolMessage::PathListBegin {
                request_id: 1,
                result: Ok(path_list_root_for_test()),
            },
            cx,
        );
        publish_to_mux(
            mux,
            zz_protocol::ProtocolMessage::PathListChunk {
                request_id: 1,
                entries,
                done: true,
                truncated: false,
            },
            cx,
        );
    }

    fn notes() -> Vec<zz_protocol::PathEntry> {
        let mut entries = (0..30)
            .map(|index| zz_protocol::PathEntry {
                rel: format!("file{index:02}.rs"),
                kind: zz_protocol::PathKind::File,
                symlink: false,
            })
            .collect::<Vec<_>>();
        entries.push(zz_protocol::PathEntry {
            rel: "my notes.txt".to_owned(),
            kind: zz_protocol::PathKind::File,
            symlink: false,
        });
        entries
    }

    #[ala::test]
    fn prefix_f_opens_the_palette_on_the_panes_folder_and_pastes_the_pick(cx: &mut TestAppContext) {
        let (workspace, mux, input, lists, cx) = path_picker_workspace(cx);
        assert!(terminal_is_focused(&workspace, cx));
        input.borrow_mut().clear();
        publish_to_mux(&mux, open_path_picker_event(1), cx);
        assert_eq!(palette_input(&workspace, cx).as_deref(), Some("~/"));
        assert!(
            input
                .borrow()
                .iter()
                .any(|message| matches!(message, InputMessage::CancelPrefix { .. }))
        );
        assert_eq!(
            lists.borrow().as_slice(),
            &[zz_protocol::ProtocolMessage::PathListRequest {
                request_id: 1,
                pane: PaneId(0),
                dir: Some("~/".to_owned()),
            }]
        );
        answer_listing(&mux, notes(), cx);
        input.borrow_mut().clear();
        cx.simulate_input("notes");
        cx.run_until_parked();
        assert!(input.borrow().is_empty());
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        assert_eq!(
            input.borrow().as_slice(),
            &[InputMessage::TerminalView {
                pane: PaneId(0),
                action: zz_terminal::TerminalViewAction::Paste("'my notes.txt' ".to_owned()),
            }]
        );
        assert!(workspace.read_with(cx, |workspace, _| workspace.command_palette.is_none()));
        assert!(terminal_is_focused(&workspace, cx));
    }

    #[ala::test]
    fn prefix_f_closes_on_escape_and_cancels_its_listing(cx: &mut TestAppContext) {
        let (workspace, mux, _, lists, cx) = path_picker_workspace(cx);
        publish_to_mux(&mux, open_path_picker_event(1), cx);
        assert!(workspace.read_with(cx, |workspace, _| workspace.command_palette.is_some()));
        cx.simulate_keystrokes("escape");
        cx.run_until_parked();
        assert!(workspace.read_with(cx, |workspace, _| workspace.command_palette.is_none()));
        assert!(terminal_is_focused(&workspace, cx));
        assert!(lists.borrow().iter().any(|message| matches!(
            message,
            zz_protocol::ProtocolMessage::PathListCancel { request_id: 1 }
        )));
    }

    #[ala::test]
    fn prefix_f_pastes_into_the_pane_it_came_from(cx: &mut TestAppContext) {
        let (workspace, mux, input, _, cx) = path_picker_workspace(cx);
        mux.update(cx, |mux, cx| {
            mux.attach_snapshot_for_test(
                SessionId(0),
                two_pane_snapshot_of(PaneId(0), PaneKindSnapshot::Terminal),
                cx,
            );
        });
        cx.run_until_parked();
        publish_to_mux(&mux, open_path_picker_event_for(1, PaneId(2)), cx);
        answer_listing(&mux, notes(), cx);
        input.borrow_mut().clear();
        cx.simulate_input("notes");
        cx.run_until_parked();
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        assert_eq!(
            input.borrow().as_slice(),
            &[InputMessage::TerminalView {
                pane: PaneId(2),
                action: zz_terminal::TerminalViewAction::Paste("'my notes.txt' ".to_owned()),
            }]
        );
        assert!(workspace.read_with(cx, |workspace, _| workspace.command_palette.is_none()));
    }

    #[test]
    fn a_picked_path_follows_the_listing_root() {
        let root = path_list_root_for_test();
        let entry = |rel: &str, kind| zz_protocol::PathEntry {
            rel: rel.to_owned(),
            kind,
            symlink: false,
        };
        let insert = crate::command::palette::path_insert_text;
        assert_eq!(
            insert(&root, &entry("src/main.rs", zz_protocol::PathKind::File)).as_deref(),
            Some("src/main.rs ")
        );
        assert_eq!(
            insert(&root, &entry("docs", zz_protocol::PathKind::Dir)).as_deref(),
            Some("docs/ ")
        );
        let claude = zz_protocol::PathListRoot {
            insert: zz_protocol::InsertStyle::Claude,
            ..root
        };
        assert_eq!(
            insert(&claude, &entry("src/main.rs", zz_protocol::PathKind::File)).as_deref(),
            Some("@src/main.rs ")
        );
    }
}
