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

impl FloatDragState {
    fn target(&self) -> FloatCells {
        let target = float_drag_preview(self.start, self.grip, self.delta);
        self.limit
            .map_or(target, |layout| layout.within(target, self.grip))
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
                        && zz_client::prefix_keys::which_key_delay(
                            config::resolved_config(cx).which_key_delay.value,
                        )
                        .is_some()
                        && zz_client::prefix_keys::is_all_keys_input(
                            self.mux.read(cx).prefix_bindings(),
                            &input,
                        )
                    {
                        self.input_router.suppress_release(
                            input
                                .unshifted_codepoint
                                .map_or(input.key, zz_terminal::KeyCode::Character),
                        );
                        self.which_key = which_key::sheet(self.mux.read(cx), "prefix", true);
                        cx.notify();
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
        if mux.key_table() != Some(("prefix", false))
            || zz_client::prefix_keys::binds_input(mux.prefix_bindings(), input)
        {
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
                .root::<penna::Root>()
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
        let Some(delay) = zz_client::prefix_keys::which_key_delay(
            config::resolved_config(cx).which_key_delay.value,
        ) else {
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
        self.which_key = which_key::sheet(self.mux.read(cx), table, false);
        cx.notify();
    }

    fn reconcile_dialog_prefix(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let active = window
            .root::<penna::Root>()
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
        choose: &ChooseFolder,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.command_palette.is_some() || self.sidebar.read(cx).route() != WorkspaceRoute::App {
            return;
        }
        let host = choose.host;
        let home = (host == HostId::LOCAL).then(std::env::home_dir).flatten();
        let body = zz_ui::files::start_dir(
            Some(&choose.start),
            home.as_ref().and_then(|home| home.to_str()),
        );
        let picker = picker.downgrade();
        let pick: zz_ui::command::FolderPick = Rc::new(move |picked, folder, window, cx| {
            if HostId::from(picked) != host {
                return;
            }
            let _ = picker.update(cx, |picker, cx| match folder {
                FolderPicked::Followed(folder) => picker.follow_folder(&folder, cx),
                FolderPicked::Chosen(folder) => {
                    picker.pick_folder(&folder, cx);
                    picker.focus().focus(window, cx);
                }
            });
        });
        self.show_command_palette(
            move |backend, window, cx| {
                CommandPaletteView::new_folder_pick_on(
                    backend,
                    host.into(),
                    &body,
                    pick,
                    window,
                    cx,
                )
            },
            window,
            cx,
        );
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

    fn pane_focus_target(&self, pane: PaneId, cx: &App) -> Option<(EntityId, FocusHandle)> {
        if let Some(picker) = self.pickers.get(&pane) {
            Some((picker.entity_id(), picker.read(cx).focus().clone()))
        } else if let Some(agent) = self.agents.get(&pane) {
            Some((agent.entity_id(), agent.read(cx).focus(cx)))
        } else if let Some(terminal) = self.terminals.get(&pane) {
            Some((terminal.entity_id(), terminal.read(cx).focus()))
        } else if let Some(browser) = self.browsers.get(&pane) {
            Some((browser.entity_id(), browser.read(cx).pane_focus_handle(cx)))
        } else if let Some(editor) = self.editors.get(&pane) {
            Some((editor.entity_id(), editor.read(cx).focus(cx)))
        } else if let Some(simulator) = self.simulators.get(&pane) {
            Some((simulator.entity_id(), simulator.read(cx).focus()))
        } else {
            self.view_focus_target(pane, cx)
        }
    }

    #[cfg(feature = "agent-pane")]
    fn view_focus_target(&self, pane: PaneId, cx: &App) -> Option<(EntityId, FocusHandle)> {
        self.views
            .get(&pane)
            .map(|view| (view.entity_id(), view.read(cx).focus()))
    }

    #[cfg(not(feature = "agent-pane"))]
    fn view_focus_target(&self, _: PaneId, _: &App) -> Option<(EntityId, FocusHandle)> {
        None
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

    pub(crate) const fn hold_pane_focus(&mut self) {
        self.pane_focus_held = true;
    }

    pub(crate) fn release_pane_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.pane_focus_held = false;
        self.synchronized_signature = None;
        self.synchronize_panes(window, cx);
        cx.notify();
    }

    fn focus_active_pane(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.pane_focus_held || self.sidebar.read(cx).route() == WorkspaceRoute::Settings {
            return;
        }
        if self.empty_workspace_visible {
            let focus = self.empty_focus(cx);
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
        let (sidebar_focused, route, chat_mode, chat_landing, chat_target) = {
            let sidebar = self.sidebar.read(cx);
            (
                sidebar.is_focused(window),
                sidebar.route(),
                sidebar.chat_mode(),
                sidebar.chat_landing(),
                sidebar.chat_target(),
            )
        };
        if let Some(last) = &self.synchronized_signature
            && last.revision == revision
            && last.sidebar_focused == sidebar_focused
            && last.route == route
            && last.chat_mode == chat_mode
            && last.chat_landing == chat_landing
            && last.chat_target == chat_target
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
        let empty_workspace_visible = chat_landing
            || !mux.has_hosts()
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
        if self.empty_workspace_visible && !empty_workspace_visible {
            self.pane_focus_owed = true;
        }
        self.empty_workspace_visible = empty_workspace_visible;
        if bell_revision != self.bell_revision {
            self.bell_revision = bell_revision;
            window.request_attention();
            crate::agent::sound::play(zz_client::attention::Chime::Bell, cx);
        }
        let mut wanted_pickers = BTreeSet::new();
        let mut wanted_terminals = BTreeSet::new();
        let mut wanted_browsers = BTreeSet::new();
        let mut wanted_agents = BTreeSet::new();
        let mut wanted_editors = BTreeSet::new();
        let mut wanted_views = BTreeSet::new();
        let mut wanted_simulators = BTreeSet::new();
        let mut existing = BTreeSet::new();
        self.flips.observe(&snapshot);
        let active_window = snapshot
            .sessions
            .iter()
            .find(|session| Some(session.id) == attached)
            .map(|session| snapshot.focused_window_for(session));

        #[cfg(feature = "agent-pane")]
        self.follow_agent_host(cx);
        let whole = &snapshot;
        for session in snapshot
            .sessions
            .iter()
            .filter(|session| Some(session.id) == attached)
        {
            for mux_window in &session.windows {
                for (pane, snapshot) in &mux_window.panes {
                    existing.insert(*pane);
                    match &snapshot.kind {
                        PaneKindSnapshot::Picker => {
                            wanted_pickers.insert(*pane);
                            self.ensure_picker(*pane, window, cx);
                        }
                        PaneKindSnapshot::Terminal => {
                            if agent_picker_place(whole, *pane, chat_mode).is_some() {
                                wanted_pickers.insert(*pane);
                                self.ensure_picker(*pane, window, cx);
                            }
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
                            self.ensure_pane_header(*pane, cx);
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
                        PaneKindSnapshot::Agent(agent) => {
                            wanted_agents.insert(*pane);
                            self.ensure_agent_view(*pane, agent, window, cx);
                            self.ensure_pane_header(*pane, cx);
                        }
                        PaneKindSnapshot::Editor(editor) => {
                            wanted_editors.insert(*pane);
                            if let Some(view) = self.editors.get(pane) {
                                view.update(cx, |view, cx| {
                                    view.synchronize_descriptor(
                                        editor,
                                        &snapshot.title,
                                        window,
                                        cx,
                                    );
                                });
                            } else {
                                let mux = self.mux.clone();
                                let pane = *pane;
                                let descriptor = editor.clone();
                                let title = snapshot.title.clone();
                                let view = cx.new(|cx| {
                                    EditorView::new(pane, &descriptor, &title, mux, window, cx)
                                });
                                self.editors.insert(pane, view);
                            }
                        }
                        PaneKindSnapshot::View(descriptor) => {
                            wanted_views.insert(*pane);
                            let vendor = descriptor
                                .origin
                                .and_then(|origin| mux_window.panes.get(&origin))
                                .and_then(|origin| match &origin.kind {
                                    PaneKindSnapshot::Agent(agent) => {
                                        zz_client::window_summary::provider_program(agent.provider)
                                    }
                                    _ => None,
                                });
                            self.ensure_view(*pane, descriptor, vendor, window, cx);
                            self.ensure_pane_header(*pane, cx);
                        }
                        PaneKindSnapshot::Other { kind } => {
                            if kind == zz_protocol::SIMULATOR_PANE_KIND {
                                wanted_simulators.insert(*pane);
                                self.ensure_simulator(*pane, window, cx);
                                self.ensure_pane_header(*pane, cx);
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
        self.empty_picker
            .update(cx, |picker, cx| picker.set_target(chat_target, cx));
        self.terminals
            .retain(|pane, _| wanted_terminals.contains(pane));
        self.terminal_headers.retain(|pane, _| {
            wanted_terminals.contains(pane)
                || wanted_agents.contains(pane)
                || wanted_views.contains(pane)
                || wanted_simulators.contains(pane)
        });
        self.simulators
            .retain(|pane, _| wanted_simulators.contains(pane));
        self.simulator_devices
            .retain(|pane, _| wanted_simulators.contains(pane));
        self.drain_agent_events(cx);
        #[cfg(feature = "agent-pane")]
        self.park_agents(&wanted_agents, &existing, cx);
        self.agents.retain(|pane, _| wanted_agents.contains(pane));
        self.agent_controller.update(cx, |controller, cx| {
            controller.retain_panes(&wanted_agents, &existing, cx);
        });
        self.editors.retain(|pane, _| wanted_editors.contains(pane));
        #[cfg(feature = "agent-pane")]
        {
            let gone = self
                .views
                .keys()
                .filter(|pane| !wanted_views.contains(pane))
                .copied()
                .collect::<Vec<_>>();
            for pane in gone {
                let face = self.pip_faces.borrow_mut().remove(&pane);
                if self.pane_motion.depart(pane, cx)
                    && let (Some(view), Some(face)) = (self.views.get(&pane), face)
                {
                    self.departing.insert(
                        pane,
                        Departing {
                            view: AnyView::from(view.clone()),
                            face,
                        },
                    );
                }
            }
            self.views.retain(|pane, _| wanted_views.contains(pane));
        }
        #[cfg(not(feature = "agent-pane"))]
        let _ = wanted_views;
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
                    if let Some(simulator) = self.simulators.get(pane) {
                        simulator.update(cx, |simulator, cx| {
                            simulator.set_visible(visible, cx);
                            simulator.set_floating(
                                float.is_some() && mux_window.zoomed_pane != Some(*pane),
                                cx,
                            );
                        });
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
        if self.pane_focus_held {
            self.focused_pane = None;
        } else if floating_input {
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
            let focus = self.empty_focus(cx);
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
            chat_mode,
            chat_landing,
            chat_target,
        });
    }

    fn drain_gui_requests(&mut self, cx: &mut Context<Self>) {
        if !self.mux.read(cx).has_gui_requests() {
            return;
        }
        let commands = self.mux.update(cx, |mux, _| mux.take_simulator_commands());
        for (pane, command) in commands {
            if let Some(simulator) = self.simulators.get(&pane) {
                simulator.update(cx, |simulator, cx| simulator.command(command, cx));
            } else {
                self.mux.read(cx).respond_to_request(GuiResponse::Error {
                    request_id: command.request_id(),
                    message: format!("{pane} has no simulator view in the attached session"),
                });
            }
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

    fn ensure_picker(&mut self, pane: PaneId, window: &mut Window, cx: &mut Context<Self>) {
        if self.pickers.contains_key(&pane) {
            return;
        }
        let mux = self.mux.clone();
        let view = cx.new(|cx| PanePickerView::new(pane, mux, cx));
        cx.subscribe_in(&view, window, |this, _, chosen: &AgentChosen, _, cx| {
            this.on_agent_chosen(chosen, cx);
        })
        .detach();
        cx.subscribe_in(
            &view,
            window,
            |this, picker, choose: &ChooseFolder, window, cx| {
                this.open_folder_palette(picker, choose, window, cx);
            },
        )
        .detach();
        self.pickers.insert(pane, view);
    }

    fn on_choose_machine(&mut self, choose: &ChooseMachine, cx: &mut Context<Self>) {
        self.sidebar
            .update(cx, |sidebar, cx| sidebar.new_chat(choose.host, cx));
    }

    fn empty_focus(&self, cx: &App) -> FocusHandle {
        if self.sidebar.read(cx).chat_mode() && self.mux.read(cx).has_hosts() {
            self.empty_picker.read(cx).focus().clone()
        } else {
            self.new_session.read(cx).focus().clone()
        }
    }

    fn on_agent_chosen(&mut self, chosen: &AgentChosen, cx: &mut Context<Self>) {
        let host = self.mux.read(cx).attached_host();
        self.sidebar.update(cx, |sidebar, _| {
            sidebar.follow_moved_window(host, chosen.window, chosen.from);
        });
    }

    fn ensure_pane_header(&mut self, pane: PaneId, cx: &mut Context<Self>) {
        if self.terminal_headers.contains_key(&pane) {
            return;
        }
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

    fn ensure_agent_view(
        &mut self,
        pane: PaneId,
        agent: &zz_protocol::AgentDescriptor,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        #[cfg(feature = "agent-pane")]
        {
            if !self.agents.contains_key(&pane)
                && let Some(view) = self.parked_agents.remove(&(self.agents_host, pane))
            {
                let host = self.agents_host;
                self.agent_controller
                    .update(cx, |controller, _| controller.unpark_pane(host, pane));
                self.agents.insert(pane, view);
            }
            let from_seq = self.agent_controller.read(cx).last_applied(pane);
            self.mux
                .update(cx, |mux, _| mux.watch_agent(pane, from_seq));
        }
        if self.agents.contains_key(&pane) {
            return;
        }
        let controller = self.agent_controller.clone();
        let mux = self.mux.clone();
        let provider = agent.provider;
        let pane_canvas = Rc::clone(&self.pane_canvas_bounds);
        let view = cx.new(|cx| {
            let mut view = AgentView::new(pane, provider, controller, mux, window, cx);
            view.share_pane_canvas(pane_canvas);
            view
        });
        #[cfg(feature = "agent-pane")]
        cx.subscribe_in(
            &view,
            window,
            move |this, _, picked: &crate::agent::AgentAccountPicked, window, cx| {
                this.switch_agent_account(pane, picked.account.as_deref(), picked.mode, window, cx);
            },
        )
        .detach();
        self.agents.insert(pane, view);
    }

    #[cfg(feature = "agent-pane")]
    fn follow_agent_host(&mut self, cx: &mut Context<Self>) {
        let host = self.mux.read(cx).attached_host();
        if host == self.agents_host {
            return;
        }
        let from = std::mem::replace(&mut self.agents_host, host);
        let parkable = parkable_agents(self.mux.read(cx));
        for (pane, view) in std::mem::take(&mut self.agents) {
            self.park_agent(from, pane, view, &parkable, cx);
        }
        self.agent_controller.update(cx, |controller, cx| {
            controller.retain_panes(&BTreeSet::new(), &BTreeSet::new(), cx);
        });
    }

    #[cfg(feature = "agent-pane")]
    fn park_agents(
        &mut self,
        wanted: &BTreeSet<PaneId>,
        existing: &BTreeSet<PaneId>,
        cx: &mut Context<Self>,
    ) {
        let leaving = self
            .agents
            .keys()
            .filter(|pane| !wanted.contains(pane))
            .copied()
            .collect::<Vec<_>>();
        if leaving.is_empty() && self.parked_agents.is_empty() {
            return;
        }
        let parkable = parkable_agents(self.mux.read(cx));
        let host = self.agents_host;
        for pane in leaving {
            let Some(view) = self.agents.remove(&pane) else {
                continue;
            };
            if existing.contains(&pane) && !parkable.contains(&(host, pane)) {
                let (text, images) = view.read(cx).draft(cx);
                self.agent_controller.update(cx, |controller, _| {
                    controller.keep_draft(pane, &text, images);
                });
            }
            self.park_agent(host, pane, view, &parkable, cx);
        }
        self.parked_agents.retain(|key, _| parkable.contains(key));
        self.agent_controller
            .update(cx, |controller, _| controller.retain_parked(&parkable));
    }

    #[cfg(feature = "agent-pane")]
    fn park_agent(
        &mut self,
        host: HostId,
        pane: PaneId,
        view: Entity<AgentView>,
        parkable: &BTreeSet<(HostId, PaneId)>,
        cx: &mut Context<Self>,
    ) {
        self.mux.update(cx, |mux, _| mux.unwatch_agent(pane));
        if !parkable.contains(&(host, pane)) {
            return;
        }
        view.update(cx, |view, cx| view.set_visible(false, cx));
        self.agent_controller
            .update(cx, |controller, _| controller.park_pane(host, pane));
        self.parked_agents.insert((host, pane), view);
    }

    #[cfg(feature = "agent-pane")]
    fn switch_agent_account(
        &mut self,
        pane: PaneId,
        account: Option<&str>,
        mode: zz_client::agent_flip::FlipMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mux = self.mux.clone();
        if self
            .flips
            .switch_account_when(pane, account, mode, |command| {
                mux.read(cx).execute_answered(command)
            })
        {
            self.refresh_flip(pane, window, cx);
        }
    }

    fn flip_pane(&mut self, pane: PaneId, window: &mut Window, cx: &mut Context<Self>) {
        let snapshot = self.mux.read(cx).snapshot();
        let Some(target) = snapshot
            .sessions
            .iter()
            .flat_map(|session| &session.windows)
            .find_map(|window| window.panes.get(&pane))
            .and_then(zz_client::agent_flip::flip_target)
        else {
            return;
        };
        let mux = self.mux.clone();
        if self.flips.press(pane, target.to, |command| {
            mux.read(cx).execute_answered(command)
        }) {
            self.refresh_flip(pane, window, cx);
        }
    }

    fn choose_flip(
        &mut self,
        pane: PaneId,
        choice: zz_ui::pane::FlipChoice,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(mode) = choice.mode() else {
            self.flips.dismiss(pane);
            self.refresh_flip(pane, window, cx);
            return;
        };
        let mux = self.mux.clone();
        if self
            .flips
            .choose(pane, mode, |command| mux.read(cx).execute_answered(command))
        {
            self.refresh_flip(pane, window, cx);
        }
    }

    fn refresh_flip(&mut self, pane: PaneId, window: &mut Window, cx: &mut Context<Self>) {
        self.synchronized_signature = None;
        self.synchronize_panes(window, cx);
        if let Some((_, focus)) = self.pane_focus_target(pane, cx) {
            focus.focus(window, cx);
        }
        cx.notify();
    }

    fn drain_flip_answers(&mut self, cx: &mut Context<Self>) {
        if !self.mux.read(cx).has_command_answers() {
            return;
        }
        let answers = self.mux.update(cx, |mux, _| mux.take_command_answers());
        for (request, result) in answers {
            if !self.flips.owns(request) {
                continue;
            }
            if let Some(error) = self
                .flips
                .settle(request, result.as_ref().copied().map_err(String::as_str))
            {
                self.mux.update(cx, |_, cx| {
                    MuxClient::emit_notification(ClientMessageKind::Error, error, cx);
                });
            }
        }
        cx.notify();
    }

    fn flip_overlay(&self, pane: PaneId, cx: &mut Context<Self>) -> Option<AnyElement> {
        let flip = self.flips.state(pane);
        if *flip == zz_client::agent_flip::Flip::Idle {
            return None;
        }
        let to = self.flips_target(pane, cx)?;
        let app = cx.entity().downgrade();
        zz_ui::pane::pane_flip_overlay(
            pane,
            to,
            flip,
            move |choice, window, cx| {
                if let Some(app) = app.upgrade() {
                    app.update(cx, |view, cx| view.choose_flip(pane, choice, window, cx));
                }
            },
            cx,
        )
    }

    fn flips_target(&self, pane: PaneId, cx: &App) -> Option<zz_ui::pane::FlipTo> {
        self.mux
            .read(cx)
            .snapshot()
            .sessions
            .iter()
            .flat_map(|session| &session.windows)
            .find_map(|window| window.panes.get(&pane))
            .and_then(zz_client::agent_flip::flip_target)
            .map(|target| target.to)
    }

    #[cfg(feature = "agent-pane")]
    fn drain_agent_events(&mut self, cx: &mut Context<Self>) {
        if !self.mux.read(cx).has_agent_events() {
            return;
        }
        let registered = self.agent_controller.read(cx).registered_panes();
        let before = self.agent_controller.read(cx).moves();
        let events = self
            .mux
            .update(cx, |mux, _| mux.take_agent_events_for(&registered));
        let moved = self.agent_controller.update(cx, |controller, cx| {
            for (pane, items) in events.items {
                controller.apply_stream_items(pane, items, cx);
            }
            for (pane, state) in events.states {
                controller.apply_pane_state(pane, &state, cx);
            }
            for (pane, reply) in events.sessions {
                controller.apply_sessions(pane, &reply, cx);
            }
            controller.moves() != before
        });
        if moved {
            cx.notify();
        }
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
            limit: (!modal).then_some(layout),
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
            let mux = self.mux.read(cx);
            if let Some(parent) = mux.agent_parent(mux.attached_host(), pane) {
                mux.execute(pane_select_command(parent));
            }
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
        let border = PaneLook::global(cx).edge_width(cx.theme()).max(px(1.0));
        let rendered = self.pane_drop_preview.clone();
        Some(
            pane_drop_preview("pane-drop-preview", radius, border, cx)
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
        } else if let Some(editor) = self.editors.get(&pane) {
            Some(PaneContent {
                view: AnyView::from(editor.clone()),
                cached: false,
                inactive_style: PaneInactiveStyle::Surface,
                covers_frame: false,
            })
        } else if let Some(simulator) = self.simulators.get(&pane) {
            Some(PaneContent {
                view: AnyView::from(simulator.clone()),
                cached: false,
                inactive_style: PaneInactiveStyle::Surface,
                covers_frame: false,
            })
        } else {
            self.view_content(pane)
        }
    }

    fn pane_kind_buttons(&self, pane: PaneId, cx: &App) -> Vec<zz_ui::pane::PaneKindButton> {
        if let Some(agent) = self.agents.get(&pane) {
            return agent.read(cx).header_buttons(cx);
        }
        #[cfg(feature = "agent-pane")]
        if let Some(view) = self.views.get(&pane) {
            return view.read(cx).header_buttons();
        }
        if let Some(simulator) = self.simulators.get(&pane) {
            return simulator.read(cx).header_buttons();
        }
        Vec::new()
    }

    fn pane_title_menu(&self, pane: PaneId, cx: &App) -> Vec<zz_ui::pane::PaneTitleRow> {
        self.simulators
            .get(&pane)
            .map(|simulator| simulator.read(cx).title_menu())
            .unwrap_or_default()
    }

    fn pane_title_pick(&mut self, pane: PaneId, index: usize, cx: &mut Context<Self>) {
        if let Some(simulator) = self.simulators.get(&pane) {
            simulator.update(cx, |simulator, cx| simulator.title_pick(index, cx));
            cx.notify();
        }
    }

    fn pane_kind_action(
        &mut self,
        pane: PaneId,
        key: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(agent) = self.agents.get(&pane) {
            agent.update(cx, |agent, cx| agent.header_action(key, window, cx));
            cx.notify();
            return;
        }
        #[cfg(feature = "agent-pane")]
        if let Some(view) = self.views.get(&pane) {
            view.update(cx, |view, cx| view.header_action(key, cx));
            cx.notify();
            return;
        }
        if let Some(simulator) = self.simulators.get(&pane) {
            simulator.update(cx, |simulator, cx| simulator.header_action(key, cx));
            cx.notify();
        }
    }

    fn ensure_simulator(&mut self, pane: PaneId, window: &Window, cx: &mut Context<Self>) {
        let descriptor = self.mux.read(cx).simulator(pane).unwrap_or_default();
        let floating = self
            .mux
            .read(cx)
            .snapshot()
            .sessions
            .iter()
            .flat_map(|session| &session.windows)
            .any(|window| window.floating.iter().any(|float| float.pane == pane));
        let Some(simulator) = self.simulators.get(&pane) else {
            let mux = self.mux.clone();
            let simulator = cx.new(|cx| {
                let mut simulator =
                    crate::simulator::SimulatorView::new(pane, &descriptor, mux, cx);
                simulator.set_floating(floating, cx);
                simulator
            });
            self.simulators.insert(pane, simulator);
            self.simulator_devices.insert(pane, descriptor);
            return;
        };
        if self.simulator_devices.get(&pane) == Some(&descriptor) {
            return;
        }
        if simulator.update(cx, |simulator, cx| {
            simulator.synchronize_descriptor(&descriptor, cx)
        }) {
            self.pane_motion.relink(pane, window, cx);
        }
        self.simulator_devices.insert(pane, descriptor);
    }

    fn pane_flip(&self, snapshot: &zz_protocol::PaneSnapshot) -> Option<PaneFlip> {
        PaneFlip::of(snapshot, self.flips.state(snapshot.id))
    }

    #[cfg(feature = "agent-pane")]
    fn view_content(&self, pane: PaneId) -> Option<PaneContent> {
        self.views.get(&pane).map(|view| PaneContent {
            view: AnyView::from(view.clone()),
            cached: false,
            inactive_style: PaneInactiveStyle::Surface,
            covers_frame: false,
        })
    }

    #[cfg(not(feature = "agent-pane"))]
    fn view_content(&self, _: PaneId) -> Option<PaneContent> {
        None
    }

    #[cfg(feature = "agent-pane")]
    fn ensure_view(
        &mut self,
        pane: PaneId,
        descriptor: &zz_protocol::ViewDescriptor,
        vendor: Option<&'static zz_client::program::Program>,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(view) = self.views.get(&pane) {
            if view.update(cx, |view, cx| {
                view.synchronize_descriptor(descriptor, vendor, cx)
            }) {
                self.pane_motion.relink(pane, window, cx);
            }
            return;
        }
        let mux = self.mux.clone();
        let agents = self.agent_controller.clone();
        let view = cx.new(|cx| {
            crate::view_pane::ViewPaneView::new(pane, descriptor, vendor, mux, agents, cx)
        });
        self.views.insert(pane, view);
    }

    #[cfg(not(feature = "agent-pane"))]
    fn ensure_view(
        &mut self,
        _: PaneId,
        _: &zz_protocol::ViewDescriptor,
        _: Option<&'static zz_client::program::Program>,
        _: &Window,
        _: &mut Context<Self>,
    ) {
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
                let behind_pip = window
                    .floating
                    .iter()
                    .any(|float| float.visible && float.pane == window.active_pane);
                let dimmed = inactive && !behind_pip;
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
                let floats = window.floating.iter().any(|float| float.pane == *pane);
                let header_place = pane_snapshot
                    .filter(|_| {
                        !self.pickers.contains_key(pane) && self.terminal_headers.contains_key(pane)
                    })
                    .map_or(HeaderPlace::Stacked, |held| {
                        HeaderPlace::of(&held.kind, floats)
                    });
                let header_band = header_place.band(
                    zz_ui::pane::tiled_pane_surface(
                        crate::theme::chrome_background(cx),
                        crate::theme::shell_frame(cx),
                        config::pane_gaps(cx),
                        active,
                        cx,
                    ),
                    cx,
                );
                if let Some(terminal) = terminal_overlay {
                    terminal.update(cx, |terminal, cx| {
                        terminal.set_pane_status(dead_label.clone(), synchronized, cx);
                        terminal.set_header_band(header_band, cx);
                    });
                }
                if let Some(agent) = self.agents.get(pane) {
                    agent.update(cx, |agent, cx| agent.set_header_band(header_band, cx));
                }
                let pane_content = self.pane_content(*pane, dimmed, inactive_opacity, cx);
                let newer_kind = pane_snapshot.and_then(|held| match &held.kind {
                    PaneKindSnapshot::Other { kind } => Some(kind.as_str()),
                    _ => None,
                });
                let waiting = pane_content.is_none() && newer_kind.is_none();
                let content = pane_content.as_ref().map_or_else(
                    || {
                        crate::window::corners::round_div_radii(
                            div().size_full().flex().children(newer_kind.map(|kind| {
                                penna::blocks::feedback::empty_state(
                                    None,
                                    zz_client::navigation::newer_pane_text(kind),
                                    cx,
                                )
                            })),
                            radii,
                        )
                        .into_any_element()
                    },
                    PaneContent::element,
                );
                let terminal_header = pane_snapshot.filter(|held| {
                    matches!(
                        held.kind,
                        PaneKindSnapshot::Terminal
                            | PaneKindSnapshot::Agent(_)
                            | PaneKindSnapshot::View(_)
                    ) && !self.pickers.contains_key(pane)
                        || self.simulators.contains_key(pane)
                });
                let header_view = terminal_header
                    .and_then(|snapshot| Some((snapshot, self.terminal_headers.get(pane)?)));
                let content = if let Some((snapshot, header_view)) = header_view {
                    let heading = zz_client::window_summary::pane_heading(snapshot);
                    let simulator = self.simulators.get(pane).map(|view| view.read(cx).title());
                    let state = TerminalHeaderState {
                        icon: if simulator.is_some() {
                            crate::simulator::PHONE
                        } else {
                            zz_ui::pane::pane_kind_icon(&snapshot.kind)
                        },
                        program: heading.program,
                        active,
                        title: simulator.map_or(heading.title, Into::into),
                        can_drag: self.mux.read(cx).is_connected()
                            && window.zoomed_pane.is_none()
                            && window.panes.len() > 1,
                        place: zz_ui::pane::PaneState {
                            floats,
                            lone: window.panes.len() == 1,
                            zoomed: window.zoomed_pane == Some(*pane),
                        },
                        folder: self.pane_folder(snapshot, cx),
                        flip: self.pane_flip(snapshot),
                        keys: self.pane_action_keys(cx),
                        buttons: self.pane_kind_buttons(*pane, cx),
                        menu: self.pane_title_menu(*pane, cx),
                        trailing_inset: window_controls_inset(
                            self.window_controls_pane == Some(*pane),
                            cx.theme().pane_header_height(),
                            cx,
                        ),
                        radii,
                        opacity: if dimmed { inactive_opacity } else { 1.0 },
                    };
                    header_view.update(cx, |header, cx| header.sync(state, cx));
                    zz_ui::pane::headed_pane(
                        header_place,
                        AnyView::from(header_view.clone())
                            .cached(
                                StyleRefinement::default()
                                    .h(cx.theme().pane_header_height())
                                    .w_full()
                                    .flex_none(),
                            )
                            .into_any_element(),
                        content,
                        self.tile_size_report(*pane, pane_snapshot),
                        cx,
                    )
                    .into_any_element()
                } else if let Some(report) = self.tile_size_report(*pane, pane_snapshot) {
                    div()
                        .relative()
                        .size_full()
                        .child(content)
                        .child(report)
                        .into_any_element()
                } else {
                    content
                };
                let mut status_tags: Vec<AnyElement> = Vec::new();
                status_tags.extend(self.flip_overlay(*pane, cx));
                if terminal_overlay.is_none() {
                    if let Some(label) = dead_label {
                        status_tags.push(
                            zz_ui::pane::pane_status_badge(penna::IconName::CircleX, label, cx)
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
                let surface_dimmed = dimmed
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
                            PaneLook::global(cx).frame(
                                cx.theme(),
                                crate::theme::chrome_background(cx),
                                active,
                            )
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

    fn synchronize_window_controls_pane(&mut self, pane: Option<PaneId>, cx: &mut Context<Self>) {
        self.window_controls_pane = pane;
        for (id, browser) in &self.browsers {
            let inset = window_controls_inset(
                Some(*id) == pane,
                zz_ui::browser::BrowserHeader::TAB_ROW_HEIGHT,
                cx,
            );
            browser.update(cx, |browser, cx| {
                browser.set_header_trailing_inset(inset, cx);
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

    fn tile_size_report(
        &self,
        pane: PaneId,
        snapshot: Option<&zz_protocol::PaneSnapshot>,
    ) -> Option<AnyElement> {
        snapshot.filter(|snapshot| {
            matches!(
                snapshot.kind,
                PaneKindSnapshot::Agent(_)
                    | PaneKindSnapshot::View(_)
                    | PaneKindSnapshot::Browser(_)
                    | PaneKindSnapshot::Editor(_)
            ) && !self.terminals.contains_key(&pane)
                || self.simulators.contains_key(&pane)
        })?;
        let mux = self.mux.clone();
        let sent = Rc::clone(&self.tile_reports);
        let held = Rc::clone(&self.terminal_resize_suppressed);
        let moving = self.pane_motion.moving();
        Some(
            canvas(
                move |bounds, window, cx| {
                    if held.get() || moving {
                        return;
                    }
                    let mux = mux.read(cx);
                    let cell = float_cell_size(mux, window);
                    let grid = zz_ui::terminal::terminal_grid_size(
                        bounds.size,
                        cell.width,
                        cell.height,
                        window.scale_factor(),
                    );
                    let generation = mux.layout_generation();
                    let report = (grid.columns.max(1), grid.rows.max(1), generation);
                    if sent.borrow().get(&pane) == Some(&report) {
                        return;
                    }
                    sent.borrow_mut().insert(pane, report);
                    mux.send_input(InputMessage::ResizeTerminalV2 {
                        pane,
                        columns: report.0,
                        rows: report.1,
                        cell_width_px: grid.cell_width_px,
                        cell_height_px: grid.cell_height_px,
                        layout_generation: generation,
                    });
                },
                |_, (), _, _| {},
            )
            .absolute()
            .inset_0()
            .into_any_element(),
        )
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
            .filter(|float| float.visible && !self.pip_stage.hidden(float.pane))
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
            .arrange(&cards, canvas, cx.theme().pane_header_height());
        zz_ui::pip::show_pips(cx.entity_id(), &placed, cx);
        let placed = placed
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
        for (pane, rest) in self.pane_motion.departures() {
            let Some(departing) = self.departing.get(&pane) else {
                continue;
            };
            let card = zz_ui::pip::departing_pip(
                pane,
                penna::Icon::new(departing.face.icon.clone()),
                departing.face.label.clone().into(),
                departing.view.clone(),
                departing.face.inset,
                cx,
            );
            layer.push(
                div()
                    .absolute()
                    .left(rest.origin.x)
                    .top(rest.origin.y)
                    .w(rest.size.width)
                    .h(rest.size.height)
                    .child(moving_pane(pane, &self.pane_motion, card))
                    .into_any_element(),
            );
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
                terminal.set_pane_status(dead, synchronized, cx);
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
                || pane_color(cx.theme(), cx.theme().background),
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
                        .map(|pane| zz_client::window_summary::pane_heading(pane).title)
                        .unwrap_or_default()
                } else {
                    title.left
                };
                let opacity = if active { 1.0 } else { inactive_opacity };
                if self.pane_motion.launched(pane) {
                    self.pip_faces.borrow_mut().insert(
                        pane,
                        PipFace {
                            label: label.clone(),
                            icon: snapshot.map_or(penna::IconName::SquareTerminal, |pane| {
                                zz_ui::pane::pane_kind_icon(&pane.kind)
                            }),
                            inset,
                        },
                    );
                }
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
                        slot.child(
                            moving_pane(pane, &self.pane_motion, card)
                                .held(held)
                                .launches(),
                        )
                    }
                })
                .into_any_element(),
        )
    }

    fn pane_action_keys(&self, cx: &App) -> zz_ui::pane::PaneActionKeys {
        let mux = self.mux.read(cx);
        crate::mux::prefix::pane_action_keys(
            mux.canonical_prefix().as_deref(),
            mux.prefix_bindings(),
        )
    }

    fn pane_folder(&self, pane: &zz_protocol::PaneSnapshot, cx: &App) -> Option<String> {
        match self.agent_controller.read(cx).pane_cwd(pane.id) {
            Some(cwd) if matches!(pane.kind, PaneKindSnapshot::Agent(_)) => {
                Some(zz_client::providers::home_relative(&cwd.to_string_lossy()))
            }
            _ => zz_client::navigation::pane_folder(pane),
        }
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
        let program =
            snapshot.and_then(|pane| zz_client::window_summary::pane_heading(pane).program);
        if let Some(header_view) = self.terminal_headers.get(&pane) {
            let simulator = self.simulators.get(&pane).map(|view| view.read(cx).title());
            let state = TerminalHeaderState {
                icon: if simulator.is_some() {
                    crate::simulator::PHONE
                } else {
                    snapshot.map_or(penna::IconName::SquareTerminal, |pane| {
                        zz_ui::pane::pane_kind_icon(&pane.kind)
                    })
                },
                program,
                active,
                title: simulator.map_or(label, Into::into),
                can_drag: connected,
                place: zz_ui::pane::PaneState {
                    floats: true,
                    ..zz_ui::pane::PaneState::default()
                },
                folder: snapshot.and_then(|snapshot| self.pane_folder(snapshot, cx)),
                flip: snapshot.and_then(|snapshot| self.pane_flip(snapshot)),
                keys: self.pane_action_keys(cx),
                buttons: self.pane_kind_buttons(pane, cx),
                menu: self.pane_title_menu(pane, cx),
                trailing_inset: px(0.0),
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
        let icon = snapshot.map_or(penna::IconName::SquareTerminal, |pane| {
            zz_ui::pane::pane_kind_icon(&pane.kind)
        });
        div()
            .opacity(opacity)
            .child(zz_ui::pane::pane_header(
                zz_ui::pane::pane_heading_icon(icon, program),
                active,
                label.clone(),
                snapshot
                    .and_then(|snapshot| self.pane_folder(snapshot, cx))
                    .map(Into::into),
                PaneDragSource::new(pane, label, connected, |_, _, _| {}),
                zz_ui::pane::PaneState {
                    floats: true,
                    ..zz_ui::pane::PaneState::default()
                },
                None,
                &[],
                &self.pane_action_keys(cx),
                px(0.0),
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
        zz_ui::pip::float_grip_areas(frame, edge, point(near, near), pip)
            .into_iter()
            .map(
                |zz_ui::pip::FloatGripArea {
                     name,
                     grip,
                     bounds,
                     cursor,
                 }| {
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
                },
            )
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
