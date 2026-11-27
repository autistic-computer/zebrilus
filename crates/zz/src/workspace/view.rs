mod agents;
mod drag;
mod floating;
mod focus;
mod keys;
mod layout;
mod overlays;
mod palettes;
mod pane_content;
mod synchronize;

use zz_ui::command::FolderPicked;
use zz_ui::command::floating::{FloatingFrame as PopupFrame, floating_frame};
use zz_ui::shell::window_drag_handle;

use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
    sync::Arc,
    time::Duration,
};

use ala::{
    AnchoredPositionMode, Animation, AnimationExt as _, AnyElement, AnyView, AnyWindowHandle, App,
    Bounds, Context, Corners, DragMoveEvent, Entity, EntityId, FocusHandle, IntoElement,
    KeyUpEvent, Keystroke, Modifiers, MouseButton, MouseDownEvent, MouseExitEvent, MouseMoveEvent,
    MouseUpEvent, Pixels, Point, Render, Size, StyleRefinement, Task, WeakEntity, Window, anchored,
    canvas, div, ease_out_quint, point, prelude::*, px,
};
use penna::blocks::dialog::DialogButtonProps;
use penna::{
    ActiveTheme as _, ElementExt as _, Emphasis, Tone, WindowExt as _,
    blocks::notification::Notification,
    components::button::{Button, ButtonVariant, ButtonVariants as _},
    draws_window_controls,
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
    new_chat::agent_picker_place,
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
use zz_ui::pip::{PipCard, PipDrag, PipLayout, PipPlaced, PipStage, PipSurface, PipThrow};
use zz_ui::{
    pane::{
        DropPreview, DropPreviewFrame, FloatingSurface, HeaderPlace, PaneArrivals, PaneChrome,
        PaneDrag, PaneDragOverlayState, PaneDragSource, PaneFlip, PaneMotion, PaneOverlayCorner,
        PaneSplitAxis, PaneSplitHighlight, PaneSplitSide, PipGrab, TerminalPaneAction,
        content_pane, moving_pane, pane_border_color, pane_drag_overlay, pane_drag_preview,
        pane_drop_preview, pane_header_frame, pane_indicator, pane_indicator_overlay,
        pane_overlay_stack, pane_split_hit_target, pane_split_slot, pane_split_surface,
        pane_surface, pane_sync_badge, pane_waiting_state,
    },
    pane_look::{PaneLook, pane_color},
    shell::{app_connection_state, app_workspace_surface},
};

use super::{
    new_session::NewSessionView,
    sidebar::{
        ChromeMode, SidebarModeChanged, SidebarReleaseFocus, SidebarRouteChanged, WorkspaceRoute,
        WorkspaceSidebar,
    },
    which_key,
};
use crate::{
    agent::AgentController,
    agent::AgentView,
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
            HostUpdateQuestion, KeyTableChanged, MuxClient, PathPickerUpdate, SshPromptRequest,
            StaleDaemonInfo,
        },
        hosts::HostId,
        nav::{TreeTarget, kill_target_command},
        prefix::terminal_key_input,
    },
    pane::display::DisplayPanesView,
    pane::layout::{SeparatorSide, pane_separator},
    pane::picker::{
        AgentChosen, ChatPicked, ChooseFolder, ChooseMachine, LeaveLanding, PanePickerView,
    },
    terminal::view::TerminalView,
    window::corners::WindowCorners,
};

const DROP_PREVIEW_MORPH: Duration = Duration::from_millis(180);
const DROP_PREVIEW_FADE: Duration = Duration::from_millis(140);

ala::actions!(zz, [ClosePane]);

const DIAGNOSTIC_TARGET: &str = "zz::diagnostics::app_render";

const PANE_KEY_CONTEXT: &str = "ZzPane";
const PANE_KEY_CONTEXT_ID: &str = "pane";

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
    let description = match stale {
        Some(StaleDaemonInfo::Protocol { daemon: None }) => format!(
            "The running daemon is from an older build (its protocol version is unknown; this zz \
             speaks v{PROTOCOL_VERSION}). Restarting it will end all running sessions."
        ),
        Some(StaleDaemonInfo::Protocol {
            daemon: Some(daemon),
        }) => format!(
            "The running daemon is from an older build (daemon protocol v{daemon}; this zz speaks \
             v{PROTOCOL_VERSION}). Restarting it will end all running sessions."
        ),
        Some(StaleDaemonInfo::Revision { daemon, minimum }) => format!(
            "The running daemon is revision {daemon}; this zz needs revision {minimum} or newer. \
             Restarting it will end all running sessions."
        ),
        None => {
            let mux = mux.read(cx);
            mux.daemon_behind(HostId::LOCAL).map_or_else(
                || {
                    "zz was updated, but the daemon is still running the previous build. \
                     Restarting it will end all running sessions."
                        .to_owned()
                },
                |behind| behind.restart_text(mux.host_session_count(HostId::LOCAL)),
            )
        }
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

#[cfg(feature = "agent-pane")]
fn parkable_agents(mux: &MuxClient) -> BTreeSet<(HostId, PaneId)> {
    mux.fleet_hosts()
        .filter(|(_, _, state, _)| matches!(state, crate::mux::hosts::HostState::Connected))
        .filter_map(|(host, _, _, snapshot)| Some((host, snapshot?)))
        .flat_map(|(host, snapshot)| {
            snapshot
                .sessions
                .iter()
                .flat_map(|session| &session.windows)
                .flat_map(|window| &window.panes)
                .filter(|(_, pane)| matches!(pane.kind, PaneKindSnapshot::Agent(_)))
                .map(move |(id, _)| (host, *id))
        })
        .collect()
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
    event: &ala_cef::BrowserEvent,
) -> Option<CommandInvocation> {
    match event {
        ala_cef::BrowserEvent::TitleChanged { title, .. } => Some(CommandInvocation::new(
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

const WINDOW_CONTROLS_CLEARANCE: Pixels = px(8.0);

fn window_controls_inset(holds_controls: bool, header_height: Pixels, cx: &App) -> Pixels {
    if holds_controls {
        let extra_inset =
            penna::window_controls_inset_in(header_height, cx) - penna::WINDOW_CONTROLS_INSET;
        penna::window_controls_span(cx) + extra_inset + WINDOW_CONTROLS_CLEARANCE
    } else {
        px(0.0)
    }
}

pub(crate) fn top_right_pane(node: &LayoutNode) -> Option<PaneId> {
    match node {
        LayoutNode::Pane(pane) => Some(*pane),
        LayoutNode::Empty => None,
        LayoutNode::Split {
            axis: Axis::Horizontal,
            second,
            ..
        } => top_right_pane(second),
        LayoutNode::Split {
            axis: Axis::Vertical,
            first,
            ..
        } => top_right_pane(first),
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
    limit: Option<PipLayout>,
}

#[derive(Clone, Copy, Debug)]
struct PipLandingState {
    window: WindowId,
    pane: PaneId,
    target: FloatCells,
    request: Option<u64>,
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

#[derive(Clone, Debug, PartialEq)]
struct PaneLayoutOverride {
    window: WindowId,
    layout: LayoutNode,
    generation: u64,
}

#[derive(Clone, Debug)]
struct PipFace {
    label: String,
    icon: penna::IconName,
    inset: Size<Pixels>,
}

struct Departing {
    view: AnyView,
    face: PipFace,
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

#[derive(Clone, Debug, PartialEq)]
struct TerminalHeaderState {
    icon: penna::IconName,
    program: Option<&'static zz_client::program::Program>,
    active: bool,
    title: String,
    can_drag: bool,
    place: zz_ui::pane::PaneState,
    folder: Option<String>,
    flip: Option<PaneFlip>,
    keys: zz_ui::pane::PaneActionKeys,
    buttons: Vec<zz_ui::pane::PaneKindButton>,
    menu: Vec<zz_ui::pane::PaneTitleRow>,
    trailing_inset: Pixels,
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
        let flip_app = self.app.clone();
        let title = state.title.clone();
        let header = zz_ui::pane::pane_header_with_menu(
            zz_ui::pane::pane_heading_icon(state.icon.clone(), state.program),
            state.active,
            title.clone(),
            state.folder.clone().map(Into::into),
            PaneDragSource::new(pane, title, state.can_drag, move |drag, _, cx| {
                if let Some(app) = drag_app.upgrade() {
                    app.update(cx, |view, cx| view.on_pane_drag_start(*drag, cx));
                }
            }),
            state.place,
            state.flip,
            &state.buttons,
            state.menu.clone(),
            &state.keys,
            state.trailing_inset,
            move |action, window, cx| {
                if matches!(action, TerminalPaneAction::Flip) {
                    if let Some(app) = flip_app.upgrade() {
                        app.update(cx, |view, cx| view.flip_pane(pane, window, cx));
                    }
                    return;
                }
                if let TerminalPaneAction::Kind(key) = action {
                    if let Some(app) = flip_app.upgrade() {
                        app.update(cx, |view, cx| view.pane_kind_action(pane, key, window, cx));
                    }
                    return;
                }
                if let TerminalPaneAction::Title(index) = action {
                    if let Some(app) = flip_app.upgrade() {
                        app.update(cx, |view, cx| view.pane_title_pick(pane, index, cx));
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
        let header = if state.place.floats {
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
    chat_mode: bool,
    chat_landing: bool,
    chat_target: Option<HostId>,
}

#[allow(clippy::struct_excessive_bools)]
pub struct AppView {
    controller: Entity<BrowserController>,
    agent_controller: Entity<AgentController>,
    mux: Entity<MuxClient>,
    sidebar: Entity<WorkspaceSidebar>,
    new_session: Entity<NewSessionView>,
    empty_picker: Entity<PanePickerView>,
    pickers: BTreeMap<PaneId, Entity<PanePickerView>>,
    terminals: BTreeMap<PaneId, Entity<TerminalView>>,
    terminal_headers: BTreeMap<PaneId, Entity<TerminalHeaderView>>,
    browsers: BTreeMap<PaneId, Entity<BrowserView>>,
    inline_browser: Entity<crate::browser::inline::InlineBrowser>,
    agents: BTreeMap<PaneId, Entity<AgentView>>,
    #[cfg(feature = "agent-pane")]
    agents_host: HostId,
    #[cfg(feature = "agent-pane")]
    parked_agents: BTreeMap<(HostId, PaneId), Entity<AgentView>>,
    flips: zz_client::agent_flip::AgentFlips,
    editors: BTreeMap<PaneId, Entity<EditorView>>,
    #[cfg(feature = "agent-pane")]
    views: BTreeMap<PaneId, Entity<crate::view_pane::ViewPaneView>>,
    simulators: BTreeMap<PaneId, Entity<crate::simulator::SimulatorView>>,
    simulator_devices: BTreeMap<PaneId, zz_protocol::SimulatorDescriptor>,
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
    pane_focus_held: bool,
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
    tile_reports: Rc<RefCell<BTreeMap<PaneId, (u16, u16, u64)>>>,
    snapshot_revision: u64,
    pane_drag: Option<PaneDragState>,
    pane_drop_preview: Rc<Cell<DropPreviewFrame>>,
    pane_layout_override: Option<PaneLayoutOverride>,
    window_controls_pane: Option<PaneId>,
    pane_canvas_bounds: Rc<Cell<Bounds<Pixels>>>,
    pane_arrivals: PaneArrivals,
    pip_faces: RefCell<BTreeMap<PaneId, PipFace>>,
    departing: BTreeMap<PaneId, Departing>,
    input_router: InputRouter,
    dialog_prefix_cancel_sent: bool,
    dialog_prefix_cancel_pending: Option<u64>,
    synchronized_signature: Option<SynchronizeSignature>,
    which_key: Option<zz_ui::which_key::WhichKeySheet>,
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
            view.drain_flip_answers(cx);
            for pane in view.simulators.keys().copied().collect::<Vec<_>>() {
                view.ensure_simulator(pane, window, cx);
            }
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
                let notification = match (event.message_id, &event.key) {
                    (Some(message_id), _) => notification.key(timed_message_key(message_id)),
                    (None, Some(key)) => notification.key(key.clone()),
                    (None, None) => notification,
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
            |_, mux, question: &HostUpdateQuestion, window, cx| {
                super::host_update::ask(mux, question, window, cx);
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
            let requests = ala_cef::artifact_requests();
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
                            live.clear,
                            live.click,
                            window,
                            cx,
                        );
                    });
                }
            }),
            overlay: false,
        });
        let previews = controller.clone();
        cx.set_global(zz_ui::agent::AgentHtmlPreviewer(Rc::new(
            move |html, size, cx| {
                let width = f32::from(size.width).max(1.0).round() as u32;
                let height = size
                    .height
                    .map(|height| f32::from(height).max(1.0).round() as u32);
                previews.update(cx, |controller, cx| {
                    controller.render_html_preview(&html, width, height, size.scale, cx)
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
        let senders = cx.weak_entity();
        cx.set_global(zz_ui::agent::ui::AgentUiSender(Rc::new(
            move |pane, text, _, cx| {
                senders
                    .update(cx, |view, cx| {
                        view.agents.get(&PaneId(pane)).is_some_and(|agent| {
                            agent.update(cx, |agent, cx| agent.send_text(&text, cx))
                        })
                    })
                    .unwrap_or(false)
            },
        )));
        let replies = mux.clone();
        let inline = inline_browser.clone();
        let drafts = agent_controller.clone();
        cx.spawn(async move |_, cx| {
            let sent = ala_cef::artifact_replies();
            while let Ok(reply) = sent.recv().await {
                if !ala_cef::is_local_page(&reply.token) {
                    replies.update(cx, |mux, _| mux.send_artifact_reply(&reply));
                    continue;
                }
                let Some(pane) = inline.update(cx, |inline, _| {
                    inline.take_reply(&reply.token, std::time::Instant::now())
                }) else {
                    continue;
                };
                drafts.update(cx, |agents, cx| agents.draft(pane, &reply.text, cx));
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
        let empty_picker = cx.new(|cx| PanePickerView::empty(mux.clone(), cx));
        cx.subscribe_in(
            &empty_picker,
            window,
            |this, picker, choose: &ChooseFolder, window, cx| {
                this.open_folder_palette(picker, choose, window, cx);
            },
        )
        .detach();
        cx.subscribe_in(
            &empty_picker,
            window,
            |this, _, choose: &ChooseMachine, _, cx| this.on_choose_machine(choose, cx),
        )
        .detach();
        cx.subscribe(&empty_picker, |this, _, picked: &ChatPicked, cx| {
            this.sidebar
                .update(cx, |sidebar, cx| sidebar.chat_picked(cx));
            this.mux.update(cx, |mux, cx| {
                mux.land(picked.host, picked.landing.clone(), cx);
            });
        })
        .detach();
        cx.subscribe(&empty_picker, |this, _, _: &LeaveLanding, cx| {
            this.sidebar.update(cx, WorkspaceSidebar::close_landing);
        })
        .detach();
        Self {
            controller,
            agent_controller,
            mux,
            sidebar,
            new_session,
            empty_picker,
            pickers: BTreeMap::new(),
            terminals: BTreeMap::new(),
            terminal_headers: BTreeMap::new(),
            browsers: BTreeMap::new(),
            inline_browser,
            agents: BTreeMap::new(),
            #[cfg(feature = "agent-pane")]
            agents_host: HostId::LOCAL,
            #[cfg(feature = "agent-pane")]
            parked_agents: BTreeMap::new(),
            flips: zz_client::agent_flip::AgentFlips::default(),
            editors: BTreeMap::new(),
            #[cfg(feature = "agent-pane")]
            views: BTreeMap::new(),
            simulators: BTreeMap::new(),
            simulator_devices: BTreeMap::new(),
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
            pane_focus_held: false,
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
            tile_reports: Rc::default(),
            snapshot_revision: 0,
            pane_drag: None,
            pane_drop_preview: Rc::new(Cell::new(DropPreviewFrame::default())),
            pane_layout_override: None,
            window_controls_pane: None,
            pane_canvas_bounds: Rc::new(Cell::new(Bounds::default())),
            pane_arrivals: PaneArrivals::default(),
            pip_faces: RefCell::default(),
            departing: BTreeMap::new(),
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

    fn active_pane(&self, cx: &App) -> Option<PaneId> {
        self.mux.read(cx).active_pane()
    }

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
        let landing = self.sidebar.read(cx).chat_landing();
        let active_window = active_session.filter(|_| !landing).and_then(|session| {
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
        let chrome_above_panes = matches!(chrome, ChromeMode::Titlebar);
        let layout_corners = match chrome {
            ChromeMode::Sidebar if route == WorkspaceRoute::App => {
                WindowCorners::for_window(window).right().top()
            }
            ChromeMode::Sidebar => WindowCorners::for_window(window).right(),
            ChromeMode::Titlebar => WindowCorners::for_window(window).bottom(),
        };
        let predicted_layout = self
            .pane_layout_override
            .as_ref()
            .map(|pending| pending.layout.clone());
        let pane_layout =
            active_window.map(|window| predicted_layout.as_ref().unwrap_or(&window.layout));
        self.synchronize_pane_corners(active_window, pane_layout, layout_corners, cx);
        let window_controls_pane = (route == WorkspaceRoute::App
            && chrome == ChromeMode::Sidebar
            && draws_window_controls(window))
        .then(|| {
            active_window.and_then(|active| {
                active
                    .zoomed_pane
                    .or_else(|| pane_layout.and_then(top_right_pane))
            })
        })
        .flatten();
        self.synchronize_window_controls_pane(window_controls_pane, cx);
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
            let mux = self.mux.read(cx);
            let host = mux.attached_host();
            self.pip_stage
                .observe(active_window, |pane| mux.agent_parent(host, pane).is_some());
            self.pane_motion.begin(
                active_window.id,
                window.viewport_size(),
                self.split_drag.is_some(),
            );
        }
        let motion = self.pane_motion.clone();
        self.departing.retain(|pane, _| motion.departing(*pane));
        let floating_layer = active_window
            .filter(|active_window| {
                route != WorkspaceRoute::Settings
                    && (active_window.floating.iter().any(|float| float.visible)
                        || !self.departing.is_empty())
            })
            .and_then(|active_window| {
                let cell = self.float_cell_size(window, cx);
                let canvas = self.pane_canvas_bounds.get().size;
                self.floating_layer(active_window, cell, canvas, cx)
            });
        if floating_layer.is_none() {
            zz_ui::pip::show_pips(cx.entity_id(), &[], cx);
        }

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
        let plated = |id: &'static str, content: AnyElement, cx: &App| {
            if config::pane_gaps(cx) {
                content_pane(
                    id,
                    content,
                    config::pane_margin(cx),
                    config::pane_content_radii(cx, layout_corners),
                    crate::theme::chrome_background(cx),
                    cx,
                )
                .into_any_element()
            } else {
                content
            }
        };
        let content = if let Some(settings) = settings_view {
            plated("settings-pane", settings.into_any_element(), cx)
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
        } else if landing
            || !has_hosts
            || empty_workspace_available(
                snapshot.generation,
                snapshot.sessions.len(),
                error.is_some(),
            )
        {
            let landing = if has_hosts && self.sidebar.read(cx).chat_mode() {
                self.empty_picker.clone().into_any_element()
            } else {
                self.new_session.clone().into_any_element()
            };
            plated("landing-pane", landing, cx)
        } else {
            let update = self
                .mux
                .read(cx)
                .attached_client_too_old()
                .then(|| crate::update::available_release(cx))
                .flatten();
            let state = app_connection_state(
                error.map_or_else(
                    || "connecting to zz daemon…".to_owned(),
                    |error| error.to_string(),
                ),
                cx,
            )
            .when_some(update, |state, version| {
                state.child(
                    Button::new("connection-update-install")
                        .primary()
                        .label(format!("Update to {version}"))
                        .on_click(|_, window, cx| crate::update::install(window, cx)),
                )
            })
            .into_any_element();
            plated("connection-pane", state, cx)
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

fn timed_message_key(message_id: u64) -> String {
    format!("timed-message-{message_id}")
}

fn pane_bounds(rect: NormalizedPaneRect, canvas_size: Size<Pixels>) -> Bounds<Pixels> {
    let width = f32::from(canvas_size.width);
    let height = f32::from(canvas_size.height);
    Bounds::new(
        ala::point(px(rect.x * width), px(rect.y * height)),
        ala::size(px(rect.width * width), px(rect.height * height)),
    )
}

#[cfg(test)]
mod drag_tests;
#[cfg(test)]
mod floating_tests;
#[cfg(test)]
mod focus_tests;
#[cfg(test)]
mod keys_tests;
#[cfg(test)]
mod layout_tests;
#[cfg(test)]
mod overlays_tests;
#[cfg(test)]
mod palettes_tests;
#[cfg(test)]
mod pane_content_tests;
#[cfg(test)]
mod test_support;

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    ala::actions!(window_shortcut_tests, [WindowShortcutFallback]);

    #[derive(Debug, PartialEq)]
    pub(super) enum PaneReleaseStep {
        Drop(CommandInvocation),
        Teardown(PaneId),
    }

    struct PaneReleaseOrderPreview;

    impl Render for PaneReleaseOrderPreview {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().w(px(100.0)).h(px(100.0))
        }
    }

    pub(super) struct PaneReleaseOrderView {
        pub(super) source: PaneId,
        pub(super) steps: Rc<RefCell<Vec<PaneReleaseStep>>>,
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
}
