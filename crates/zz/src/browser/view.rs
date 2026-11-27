mod chrome_data;
mod context_menu;
mod dialogs;
mod events;
mod find;
mod omnibox;
mod tabs;

use std::{
    cell::Cell,
    collections::HashMap,
    fs::File,
    io::Read as _,
    ops::Range,
    path::{Path, PathBuf},
    rc::Rc,
    sync::Arc,
    time::Duration,
};
use zz_ui::pane_look::pane_color;
use zz_ui::shell::window_drag_handle;

#[cfg(target_os = "macos")]
use ala::PromptLevel;
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
use ala::wgpu;
use ala::{
    Anchor, AnyElement, AnyView, App, Bounds, ClipboardEntry, ClipboardItem, ClipboardString,
    Context, Corners, CursorStyle, DismissEvent, Entity, EntityInputHandler, FocusHandle,
    Focusable, Image, ImageFormat, IntoElement, KeyBinding, KeyDownEvent, KeyUpEvent, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, NoAction, PathPromptOptions, Pixels, Point,
    Render, RenderImage, ScrollWheelEvent, SharedString, StyleRefinement, Subscription,
    UTF16Selection, WeakEntity, Window, anchored, deferred, div, point, prelude::*, px,
};
#[cfg(target_os = "windows")]
use ala_cef::WinGpuTexture;
use ala_cef::{
    BrowserCursor, BrowserEvent, BrowserGpuContext, BrowserKey, ContextMenuRequest,
    CookieImportBatch, DownloadProgress, DownloadRequest, EditCommand, ElementPickerAppearance,
    FileDialogKind, FileDialogRequest, ImeCharacterBounds, JsDialogKind, JsDialogRequest, KeyInput,
    MAX_COOKIE_IMPORT_BYTES, Modifiers, PdfPrintResult, PointerButton, PointerEvent, PointerPhase,
    RuntimePhase, SessionId, Viewport, WheelEvent, chrome_import, ime_range_bounds,
    parse_cookie_import,
};
#[cfg(target_os = "macos")]
use core_video::pixel_buffer::CVPixelBuffer;
use penna::{
    ActiveTheme as _, Colorize as _, Elevation, IconName, WindowExt as _,
    components::button::{Button, ButtonVariants as _},
    components::input::{InputEvent, InputState},
    components::menu::{DropdownMenu as _, PopupMenu, PopupMenuItem},
    to_hex,
};
use zz_client::{
    BROWSER_TABLE, ChromeAction,
    url_input::{diagnostic_url, normalize_url, resolve_address},
};
use zz_protocol::{
    BrowserCommand, BrowserDescriptor, ClientMessageKind, CommandInvocation, GuiResponse,
    InputMessage, KeyToken, MAX_BROWSER_KEY_REPEAT, PaneId, normalize_browser_profile_name,
};
use zz_terminal::{KeyAction as TerminalKeyAction, KeyInput as TerminalKeyInput};
use zz_ui::browser::{
    BrowserActionMenuState, BrowserEmptyHint, BrowserErrorPanel, BrowserFindAction,
    BrowserFindState, BrowserHeader, BrowserMenuActions, BrowserMenuEngine, BrowserMenuProfile,
    BrowserPickStatus, BrowserProfileDiscoveryState, BrowserSiteMenuState, BrowserStatusChip,
    BrowserTabInfo, BrowserTabStrip, BrowserToolbar, browser_action_menu, browser_address,
    browser_find_bar, browser_omnibox_panel, browser_omnibox_row, browser_recent_row,
    browser_site_controls_button, browser_site_menu, browser_start_surface, browser_toolbar_button,
};
use zz_ui::dialogs::{
    browser_clear_site_data_alert, browser_js_alert_dialog, browser_js_confirm_dialog,
    browser_js_prompt_dialog,
};
use zz_ui::pane::{PaneActions, PaneDrag, PaneDragSource, PaneOverlayCorner, pane_overlay_stack};

#[cfg(target_os = "macos")]
use crate::browser::macos_surface::MacBrowserSurfaceCache;
#[cfg(target_os = "macos")]
use crate::browser::underlay::PaneUnderlay;

use super::recent_pages::{self, RecentPage};
use crate::{
    browser::controller::{
        BrowserController, BrowserPaneFrameContent, BrowserSessionRequest, ControllerEvent,
        EgressSpec, TabId,
    },
    browser::element::BrowserElement,
    browser::screenshot::Screenshot,
    config::pane_content_radii,
    diagnostics,
    keymap::ChromeChord,
    mux::{client::MuxClient, prefix::terminal_key_input},
    window::corners::{WindowCorners, round_div_radii},
    workspace::ClosePane,
};
use zz_client::recent_pages::{Omnibox, dedupe_recents};

const DEFAULT_URL: &str = "about:blank";
const TAB_ID: TabId = TabId(0);
const EMPTY_STATE_RECENT_LIMIT: usize = 8;
const OMNIBOX_SUGGESTION_LIMIT: usize = 8;
const BROWSER_KEY_CONTEXT: &str = "Browser";
const RESIZE_COVER_TIMEOUT: Duration = Duration::from_millis(250);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ResizeCover {
    None,
    Held,
    Awaiting(u64),
}
const OMNIBOX_INPUT_KEY_CONTEXT: &str = "BrowserOmnibox > PennaInput";
#[cfg(target_os = "macos")]
const FULL_DISK_ACCESS_SETTINGS_URL: &str =
    "x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_AllFiles";

ala::actions!(
    browser,
    [
        ZoomIn,
        ZoomOut,
        ResetZoom,
        ToggleDevTools,
        NewTab,
        NextTab,
        PreviousTab,
        SelectLastTab,
        GoBack,
        GoForward,
        Reload,
        HardReload,
        FocusAddress,
        OmniboxNext,
        OmniboxPrevious,
        OmniboxDelete
    ]
);

#[allow(clippy::unsafe_derive_deserialize)]
#[derive(Clone, Debug, ala::Action, PartialEq, Eq, serde::Deserialize)]
#[action(namespace = browser, no_json)]
struct BrowserEdit {
    command: EditCommand,
}

#[allow(clippy::unsafe_derive_deserialize)]
#[derive(Clone, Debug, ala::Action, PartialEq, Eq, serde::Deserialize)]
#[action(namespace = browser, no_json)]
struct SelectTab {
    index: usize,
}

#[allow(clippy::unsafe_derive_deserialize)]
#[derive(Clone, Debug, ala::Action, PartialEq, Eq, serde::Deserialize)]
#[action(namespace = browser, no_json)]
struct ConfiguredElementSelector {
    chord: String,
}

pub fn init(cx: &mut App) {
    cx.bind_keys(raw_key_bindings());
    crate::keymap::bind(cx, BROWSER_TABLE, browser_chrome_bindings);
}

#[cfg(target_os = "macos")]
fn prompt_for_chrome_data_access(window: &mut Window, cx: &mut App) {
    let response = window.prompt(
        PromptLevel::Warning,
        "Allow access to Chrome data",
        Some(
            "macOS blocked zz from reading Chrome's protected data. Open Full Disk Access, enable zz, then quit and reopen zz before retrying the import.",
        ),
        &["Open Full Disk Access", "Cancel"],
        cx,
    );
    cx.spawn(async move |cx| {
        if response.await == Ok(0) {
            cx.update(|cx| cx.open_url(FULL_DISK_ACCESS_SETTINGS_URL));
        }
    })
    .detach();
}

fn raw_key_bindings() -> [KeyBinding; 5] {
    [
        KeyBinding::new("tab", NoAction, Some(BROWSER_KEY_CONTEXT)),
        KeyBinding::new("shift-tab", NoAction, Some(BROWSER_KEY_CONTEXT)),
        KeyBinding::new("down", OmniboxNext, Some(OMNIBOX_INPUT_KEY_CONTEXT)),
        KeyBinding::new("up", OmniboxPrevious, Some(OMNIBOX_INPUT_KEY_CONTEXT)),
        KeyBinding::new(
            "shift-delete",
            OmniboxDelete,
            Some(OMNIBOX_INPUT_KEY_CONTEXT),
        ),
    ]
}

fn browser_chrome_bindings(chords: &[ChromeChord]) -> Vec<KeyBinding> {
    let context = Some(BROWSER_KEY_CONTEXT);
    chords
        .iter()
        .filter_map(|chord| {
            let edit = |command| BrowserEdit { command };
            Some(match chord.action() {
                ChromeAction::BrowserZoomIn => chord.binding(ZoomIn, context),
                ChromeAction::BrowserZoomOut => chord.binding(ZoomOut, context),
                ChromeAction::BrowserZoomReset => chord.binding(ResetZoom, context),
                ChromeAction::BrowserDevTools => chord.binding(ToggleDevTools, context),
                ChromeAction::BrowserNewTab => chord.binding(NewTab, context),
                ChromeAction::BrowserNextTab => chord.binding(NextTab, context),
                ChromeAction::BrowserPreviousTab => chord.binding(PreviousTab, context),
                ChromeAction::BrowserSelectTab(index) => chord.binding(
                    SelectTab {
                        index: usize::from(index),
                    },
                    context,
                ),
                ChromeAction::BrowserSelectLastTab => chord.binding(SelectLastTab, context),
                ChromeAction::BrowserBack => chord.binding(GoBack, context),
                ChromeAction::BrowserForward => chord.binding(GoForward, context),
                ChromeAction::BrowserReload => chord.binding(Reload, context),
                ChromeAction::BrowserHardReload => chord.binding(HardReload, context),
                ChromeAction::BrowserFind => chord.binding(crate::menus::Find, context),
                ChromeAction::BrowserFocusAddress => chord.binding(FocusAddress, context),
                ChromeAction::BrowserElementSelector => chord.binding(
                    ConfiguredElementSelector {
                        chord: chord.key().to_owned(),
                    },
                    context,
                ),
                ChromeAction::BrowserUndo => chord.binding(crate::menus::Undo, context),
                ChromeAction::BrowserRedo => chord.binding(crate::menus::Redo, context),
                ChromeAction::BrowserCut => chord.binding(crate::menus::Cut, context),
                ChromeAction::BrowserCopy => chord.binding(crate::menus::Copy, context),
                ChromeAction::BrowserPaste => chord.binding(crate::menus::Paste, context),
                ChromeAction::BrowserPasteAndMatchStyle => {
                    chord.binding(edit(EditCommand::PasteAndMatchStyle), context)
                }
                ChromeAction::BrowserSelectAll => chord.binding(crate::menus::SelectAll, context),
                ChromeAction::ClosePane => chord.binding(ClosePane, context),
                _ => return None,
            })
        })
        .collect()
}

fn is_blank_url(url: &str) -> bool {
    url == "about:blank"
}

fn tab_host_label(url: &str) -> SharedString {
    if url.is_empty() || is_blank_url(url) {
        return "New Tab".into();
    }
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let host = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    let host = host.trim_start_matches("www.");
    if host.is_empty() {
        "New Tab".into()
    } else {
        host.to_owned().into()
    }
}

#[derive(Clone)]
struct PendingHistoryUse {
    profile: String,
    input: String,
    selected: bool,
    started: bool,
}

#[allow(
    clippy::struct_excessive_bools,
    reason = "independent loading, history-direction, and session-lifecycle states"
)]
struct BrowserTab {
    id: TabId,
    url: String,
    title: String,
    loading: bool,
    loading_progress: Option<f32>,
    can_go_back: bool,
    can_go_forward: bool,
    page_zoom_factor: f64,
    page_zoom_percent: u16,
    error: Option<Arc<str>>,
    started: bool,
    popup: bool,
}

impl BrowserTab {
    fn new(id: TabId, url: String) -> Self {
        Self {
            id,
            url,
            title: String::new(),
            loading: false,
            loading_progress: None,
            can_go_back: false,
            can_go_forward: false,
            page_zoom_factor: 1.0,
            page_zoom_percent: 100,
            error: None,
            started: true,
            popup: false,
        }
    }

    fn restored(id: TabId, url: String) -> Self {
        Self {
            started: false,
            ..Self::new(id, url)
        }
    }
}

const DIAGNOSTIC_TARGET: &str = "zz::diagnostics::browser_render";

#[derive(Clone, Copy, PartialEq, Eq)]
enum ChromeProfileDiscovery {
    NotStarted,
    Loading,
    Loaded,
    Failed,
}

#[derive(Clone, PartialEq)]
struct ChromeState {
    active: bool,
    can_drag: bool,
    floats: bool,
    place: zz_ui::pane::PaneState,
    drag_title: String,
    opacity: f32,
    trailing_inset: Pixels,
    shows_empty_state: bool,
    loading_progress: Option<f32>,
    can_go_back: bool,
    can_go_forward: bool,
    element_pick_active: bool,
    current_url: String,
    profile: String,
    chrome_profiles: Vec<chrome_import::profiles::DetectedChromeProfile>,
    chrome_profile_discovery: ChromeProfileDiscovery,
    page_zoom_percent: u16,
    tabs: Vec<BrowserTabInfo>,
    active_tab_index: usize,
}

struct BrowserChromeView {
    browser: WeakEntity<BrowserView>,
    address: Entity<InputState>,
    state: ChromeState,
}

type HistoryIcons = Option<(String, Vec<(String, Option<Arc<[u8]>>)>)>;

impl BrowserChromeView {
    fn new(
        browser: WeakEntity<BrowserView>,
        address: Entity<InputState>,
        state: ChromeState,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&address, |_, _, cx| cx.notify()).detach();
        Self {
            browser,
            address,
            state,
        }
    }
}

#[allow(
    clippy::struct_excessive_bools,
    reason = "independent toolbar, visibility, loading, and recovery states"
)]
pub(crate) struct BrowserView {
    pane: PaneId,
    mux: Entity<MuxClient>,
    controller: Entity<BrowserController>,
    address: Entity<InputState>,
    chrome: Entity<BrowserChromeView>,
    address_editing: bool,
    omnibox: Omnibox,
    focus_handle: FocusHandle,
    chrome_opacity: f32,
    header_trailing_inset: Pixels,
    window_corners: WindowCorners,
    content_bounds: Option<Bounds<Pixels>>,
    page_buttons_down: u8,
    viewport: Viewport,
    resize_held: Rc<Cell<bool>>,
    pane_canvas: Rc<Cell<Bounds<Pixels>>>,
    resize_cover: ResizeCover,
    resize_covers: u64,
    frame_size: (u32, u32),
    visible: bool,
    window_visible: bool,
    image: Option<Arc<RenderImage>>,
    retired_images: Vec<Arc<RenderImage>>,
    #[cfg(target_os = "macos")]
    mac_surface: Option<CVPixelBuffer>,
    #[cfg(target_os = "macos")]
    mac_surface_cache: MacBrowserSurfaceCache,
    #[cfg(target_os = "macos")]
    underlay: Option<PaneUnderlay>,
    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    gpu_texture: Option<wgpu::Texture>,
    #[cfg(target_os = "windows")]
    win_gpu_texture: Option<WinGpuTexture>,
    gpu_context: Option<BrowserGpuContext>,
    frame_session: Option<SessionId>,
    image_generation: u64,
    history_icons: HistoryIcons,
    current_url: String,
    mux_tabs: Vec<String>,
    mux_active: usize,
    applying_mux: bool,
    profile: String,
    page_zoom_factor: f64,
    page_zoom_percent: u16,
    title: String,
    loading: bool,
    loads: u64,
    loading_progress: Option<f32>,
    status_message: Option<SharedString>,
    find_input: Entity<InputState>,
    find_open: bool,
    find_case_sensitive: bool,
    find_matches: Option<(i32, i32)>,
    active_downloads: HashMap<u32, String>,
    can_go_back: bool,
    can_go_forward: bool,
    tabs: Vec<BrowserTab>,
    active_tab: TabId,
    next_tab_id: u64,
    pending_history_uses: HashMap<TabId, PendingHistoryUse>,
    error: Option<Arc<str>>,
    recoverable: bool,
    chrome_profiles: Vec<chrome_import::profiles::DetectedChromeProfile>,
    chrome_profile_discovery: ChromeProfileDiscovery,
    element_pick_active: bool,
    context_menu: Option<PageContextMenu>,
    cursor: CursorStyle,
    marked_text: Option<String>,
    prompt_composition: bool,
    ime_bounds: Arc<[ImeCharacterBounds]>,
    _subscriptions: Vec<Subscription>,
}

#[derive(Clone, Copy)]
enum ZoomOp {
    In,
    Out,
    Reset,
}

#[allow(
    clippy::disallowed_methods,
    reason = "the picker overlay is page CSS, not chrome"
)]
fn element_picker_appearance(theme: &penna::Theme, page_zoom: f64) -> ElementPickerAppearance {
    ElementPickerAppearance {
        highlight_outline: to_hex(theme.foreground),
        highlight_fill: to_hex(theme.foreground.fill()),
        highlight_contrast: to_hex(theme.background.opaque()),
        preview_background: to_hex(theme.surface(Elevation::Panel).fill.opaque()),
        preview_foreground: to_hex(theme.foreground),
        preview_border: to_hex(theme.border()),
        shadow: theme
            .casts_shadow(penna::PanelLook::Popover)
            .then(|| to_hex(theme.shadow_color)),
        radius: f32::from(theme.radius),
        font_family: theme.mono_font_family.to_string(),
        page_zoom,
    }
}

fn browser_session_request(
    pane: PaneId,
    url: String,
    profile: String,
    viewport: Viewport,
    page_zoom_factor: f64,
    gpu_context: Option<BrowserGpuContext>,
    mux: &Entity<MuxClient>,
    cx: &App,
) -> BrowserSessionRequest {
    let egress = browser_egress_spec(mux, pane, &profile, cx);
    let request_profile = egress
        .as_ref()
        .map_or_else(|| profile, |egress| egress.composite_profile.clone());
    BrowserSessionRequest::new(
        url,
        request_profile,
        viewport,
        page_zoom_factor,
        gpu_context,
    )
    .with_egress(egress)
}

fn browser_egress_spec(
    mux: &Entity<MuxClient>,
    pane: PaneId,
    profile: &str,
    cx: &App,
) -> Option<EgressSpec> {
    if !crate::config::browser_egress_enabled(cx) {
        return None;
    }
    browser_egress_route_spec(mux, pane, profile, cx)
}

fn browser_egress_route_spec(
    mux: &Entity<MuxClient>,
    pane: PaneId,
    profile: &str,
    cx: &App,
) -> Option<EgressSpec> {
    if cfg!(target_os = "windows") {
        return None;
    }
    let (egress_host, socks_port) = mux.read(cx).attached_ssh_egress()?;
    let composite_profile =
        match ala_cef::BrowserProfilePaths::egress_profile_name(profile, &egress_host) {
            Ok(profile) => profile,
            Err(error) => {
                log::warn!(
                    target: "zz::browser::egress",
                    "could not derive browser egress profile for pane {pane}: {error}"
                );
                return None;
            }
        };
    Some(EgressSpec {
        composite_profile,
        egress_host,
        socks_port,
    })
}

struct PageContextMenu {
    menu: Entity<PopupMenu>,
    position: Point<Pixels>,
    _subscription: Subscription,
}

impl BrowserView {
    pub(crate) fn new(
        pane: PaneId,
        descriptor: &BrowserDescriptor,
        controller: Entity<BrowserController>,
        mux: Entity<MuxClient>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mux_tabs = descriptor.tabs.clone();
        let mux_active = descriptor.active_tab;
        let initial_url = normalize_url(descriptor.url()).unwrap_or_else(|error| {
            log::warn!("browser pane {pane} initial url rejected ({error}); opening blank");
            DEFAULT_URL.to_owned()
        });
        let profile = normalize_browser_profile_name(&descriptor.profile)
            .unwrap_or_else(|_| ala_cef::DEFAULT_BROWSER_PROFILE.to_owned());
        let active_index = descriptor
            .active_tab
            .min(descriptor.tabs.len().saturating_sub(1));
        let mut next_tab_id = TAB_ID.0;
        let mut tabs = Vec::with_capacity(descriptor.tabs.len().max(1));
        for (index, url) in descriptor.tabs.iter().enumerate() {
            let id = TabId(next_tab_id);
            next_tab_id += 1;
            if index == active_index {
                tabs.push(BrowserTab::new(id, initial_url.clone()));
            } else {
                let url = normalize_url(url).unwrap_or_else(|_| DEFAULT_URL.to_owned());
                tabs.push(BrowserTab::restored(id, url));
            }
        }
        if tabs.is_empty() {
            tabs.push(BrowserTab::new(TAB_ID, initial_url.clone()));
            next_tab_id = TAB_ID.0 + 1;
        }
        let active_tab = tabs[active_index.min(tabs.len() - 1)].id;
        let focus_handle = cx.focus_handle();
        let address_value = if is_blank_url(&initial_url) {
            String::new()
        } else {
            initial_url.clone()
        };
        let address = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Search or enter a URL")
                .default_value(address_value)
                .context_menu(true)
        });
        let find_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Find in page")
                .context_menu(true)
        });
        let chrome = cx.new({
            let browser = cx.entity().downgrade();
            let address = address.clone();
            let state = ChromeState {
                active: false,
                can_drag: false,
                floats: false,
                place: zz_ui::pane::PaneState::default(),
                drag_title: initial_url.clone(),
                opacity: 1.0,
                trailing_inset: px(0.0),
                shows_empty_state: is_blank_url(&initial_url),
                loading_progress: None,
                can_go_back: false,
                can_go_forward: false,
                element_pick_active: false,
                current_url: initial_url.clone(),
                profile: profile.clone(),
                chrome_profiles: Vec::new(),
                chrome_profile_discovery: ChromeProfileDiscovery::NotStarted,
                page_zoom_percent: 100,
                tabs: Vec::new(),
                active_tab_index: 0,
            };
            move |cx| BrowserChromeView::new(browser, address, state, cx)
        });

        let address_subscription = cx.subscribe_in(
            &address,
            window,
            |view, address, event: &InputEvent, window, cx| match event {
                InputEvent::PressEnter { .. } => {
                    view.accept_omnibox(window, cx);
                }
                InputEvent::Focus => {
                    view.select_pane(cx);
                    view.omnibox.begin(&address.read(cx).value());
                    view.set_address_editing(true, cx);
                    let address = address.clone();
                    cx.defer_in(window, move |_, _, cx| {
                        let end = address.read(cx).value().len();
                        address.update(cx, |address, cx| {
                            address.set_selected_range(0..end, cx);
                        });
                    });
                }
                InputEvent::Blur => view.set_address_editing(false, cx),
                InputEvent::Change => {
                    let value = address.read(cx).value().to_string();
                    if let Some(completion) = view.update_omnibox(&value, cx) {
                        let query = view.omnibox.query.clone();
                        let address = address.clone();
                        cx.defer_in(window, move |view, window, cx| {
                            if !view.address_editing
                                || view.omnibox.query != query
                                || view.omnibox.selected != Some(0)
                            {
                                return;
                            }
                            address.update(cx, |address, cx| {
                                address.set_value(completion.clone(), window, cx);
                                address.set_selected_range(query.len()..completion.len(), cx);
                            });
                        });
                    }
                }
                InputEvent::PasteImages(_) | InputEvent::PastePaths(_) => {}
            },
        );
        let find_subscription = cx.subscribe_in(
            &find_input,
            window,
            |view, _, event: &InputEvent, _, cx| match event {
                InputEvent::Focus => view.select_pane(cx),
                InputEvent::Change => view.update_find(cx),
                InputEvent::PressEnter { shift } => view.step_find(!*shift, cx),
                InputEvent::Blur | InputEvent::PasteImages(_) | InputEvent::PastePaths(_) => {}
            },
        );
        let controller_subscription = cx.subscribe_in(
            &controller,
            window,
            |view, controller, event: &ControllerEvent, window, cx| {
                view.handle_controller_event(controller, event, window, cx);
            },
        );
        let mut history_revision = recent_pages::revision(cx);
        cx.observe_global::<recent_pages::RecentPages>(move |view, cx| {
            let revision = recent_pages::revision(cx);
            if revision == history_revision {
                return;
            }
            history_revision = revision;
            let icons = view.current_history_icons(cx);
            if icons != view.history_icons {
                view.history_icons = icons;
                if view.visible {
                    cx.notify();
                }
            }
        })
        .detach();
        let focus_controller = controller.clone();
        let focus_pane = pane;
        let focus_in = window.on_focus_in(&focus_handle, cx, move |_, cx| {
            focus_controller.update(cx, |controller, _| {
                controller.set_focus(focus_pane, true);
            });
        });
        let blur_controller = controller.clone();
        let blur_pane = pane;
        let focus_out = window.on_focus_out(&focus_handle, cx, move |_, _, cx| {
            blur_controller.update(cx, |controller, _| {
                controller.set_focus(blur_pane, false);
            });
        });
        let window_visibility = cx.observe_window_visibility(window, |view, visibility, _, cx| {
            view.set_window_visible(visibility.is_visible(), cx);
        });
        let window_bounds = cx.observe_window_bounds(window, |view, window, cx| {
            if let Some(bounds) = view.content_bounds {
                view.update_content_bounds(bounds, window, cx);
            }
        });

        let error = controller.read(cx).startup_error();
        let recoverable = controller.read(cx).runtime_phase() == Some(RuntimePhase::Running);
        let viewport = Viewport {
            width: 800,
            height: 500,
            scale_factor: window.scale_factor(),
            window_zoom: window.zoom(),
            visible: false,
            ..Viewport::default()
        };
        #[cfg(any(target_os = "linux", target_os = "freebsd"))]
        let gpu_context = window
            .wgpu_device_context()
            .map(|gpu| BrowserGpuContext::new(gpu.device, gpu.queue));
        #[cfg(target_os = "windows")]
        let gpu_context = window
            .directx_device_context()
            .map(BrowserGpuContext::from_directx);
        #[cfg(not(any(target_os = "linux", target_os = "freebsd", target_os = "windows")))]
        let gpu_context = None;
        let request = browser_session_request(
            pane,
            initial_url.clone(),
            profile.clone(),
            viewport,
            1.0,
            gpu_context.clone(),
            &mux,
            cx,
        );
        controller.update(cx, |controller, cx| {
            controller.set_active_tab(pane, active_tab, cx);
            controller.request_browser(pane, active_tab, request, cx);
        });
        Self {
            pane,
            mux,
            controller,
            address,
            chrome,
            address_editing: false,
            omnibox: Omnibox::default(),
            focus_handle,
            chrome_opacity: 1.0,
            header_trailing_inset: px(0.0),
            window_corners: WindowCorners::NONE,
            content_bounds: None,
            page_buttons_down: 0,
            viewport,
            resize_held: Rc::new(Cell::new(false)),
            pane_canvas: Rc::default(),
            resize_cover: ResizeCover::None,
            resize_covers: 0,
            frame_size: (0, 0),
            visible: false,
            window_visible: window.is_visible(),
            image: None,
            retired_images: Vec::new(),
            #[cfg(target_os = "macos")]
            mac_surface: None,
            #[cfg(target_os = "macos")]
            mac_surface_cache: MacBrowserSurfaceCache::default(),
            #[cfg(target_os = "macos")]
            underlay: None,
            #[cfg(any(target_os = "linux", target_os = "freebsd"))]
            gpu_texture: None,
            #[cfg(target_os = "windows")]
            win_gpu_texture: None,
            gpu_context,
            frame_session: None,
            image_generation: 0,
            history_icons: None,
            current_url: initial_url,
            mux_tabs,
            mux_active,
            applying_mux: false,
            profile,
            page_zoom_factor: 1.0,
            page_zoom_percent: 100,
            title: "Browser".to_owned(),
            loading: false,
            loads: 0,
            loading_progress: None,
            status_message: None,
            find_input,
            find_open: false,
            find_case_sensitive: false,
            find_matches: None,
            active_downloads: HashMap::new(),
            can_go_back: false,
            can_go_forward: false,
            tabs,
            active_tab,
            next_tab_id,
            pending_history_uses: HashMap::new(),
            error,
            recoverable,
            chrome_profiles: Vec::new(),
            chrome_profile_discovery: ChromeProfileDiscovery::NotStarted,
            element_pick_active: false,
            context_menu: None,
            cursor: CursorStyle::Arrow,
            marked_text: None,
            prompt_composition: false,
            ime_bounds: Arc::default(),
            _subscriptions: vec![
                address_subscription,
                find_subscription,
                controller_subscription,
                focus_in,
                focus_out,
                window_visibility,
                window_bounds,
            ],
        }
    }

    pub(crate) fn focus_handle(&self) -> FocusHandle {
        self.focus_handle.clone()
    }

    fn set_window_visible(&mut self, window_visible: bool, cx: &mut Context<Self>) {
        if self.window_visible == window_visible {
            return;
        }
        self.window_visible = window_visible;
        let visible = self.visible && window_visible;
        if self.viewport.visible != visible {
            self.viewport.visible = visible;
            self.controller.update(cx, |controller, cx| {
                controller.set_viewport(self.pane, self.viewport, cx);
            });
        }
    }

    pub(crate) fn pane_focus_handle(&self, cx: &App) -> FocusHandle {
        if self.find_open {
            self.find_input.read(cx).focus_handle(cx)
        } else if self.address_editing || self.shows_empty_state() {
            self.address.read(cx).focus_handle(cx)
        } else {
            self.focus_handle.clone()
        }
    }

    fn shows_empty_state(&self) -> bool {
        self.error.is_none()
            && is_blank_url(&self.current_url)
            && !self.tabs[self.active_tab_index()].popup
    }

    fn current_history_icons(&self, cx: &App) -> HistoryIcons {
        if !self.visible {
            return None;
        }
        let mut urls = self
            .tabs
            .iter()
            .map(|tab| {
                if tab.id == self.active_tab {
                    self.current_url.clone()
                } else {
                    tab.url.clone()
                }
            })
            .collect::<Vec<_>>();
        if self.shows_empty_state() {
            urls.extend(
                dedupe_recents(
                    recent_pages::recent(&self.profile, cx, EMPTY_STATE_RECENT_LIMIT * 8),
                    EMPTY_STATE_RECENT_LIMIT,
                )
                .into_iter()
                .map(|page| page.url),
            );
        }
        if self.address_editing {
            urls.extend(
                self.omnibox
                    .suggestions
                    .iter()
                    .map(|suggestion| suggestion.url.clone()),
            );
        }
        let icons = urls
            .into_iter()
            .map(|url| {
                let icon = recent_pages::favicon(&self.profile, &url, cx);
                (url, icon)
            })
            .collect();
        Some((self.profile.clone(), icons))
    }

    fn set_address_editing(&mut self, editing: bool, cx: &mut Context<Self>) {
        let mut changed = false;
        if self.address_editing != editing {
            self.address_editing = editing;
            changed = true;
        }
        if !editing && !self.omnibox.is_clear() {
            self.omnibox.reset();
            changed = true;
        }
        if changed {
            cx.notify();
        }
    }

    pub(crate) fn set_window_corners(&mut self, corners: WindowCorners, cx: &mut Context<Self>) {
        if self.window_corners != corners {
            self.window_corners = corners;
            cx.notify();
        }
    }

    pub(crate) fn set_header_trailing_inset(&mut self, inset: Pixels, cx: &mut Context<Self>) {
        if self.header_trailing_inset != inset {
            self.header_trailing_inset = inset;
            cx.notify();
        }
    }

    pub(crate) fn set_chrome_dimmed(&mut self, dimmed: bool, opacity: f32, cx: &mut Context<Self>) {
        let opacity = if dimmed { opacity.clamp(0.0, 1.0) } else { 1.0 };
        if self.chrome_opacity.to_bits() != opacity.to_bits() {
            self.chrome_opacity = opacity;
            cx.notify();
        }
    }

    pub(crate) fn set_visible(
        &mut self,
        visible: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.visible == visible {
            return;
        }
        if !visible {
            self.cancel_element_pick(cx);
        }
        self.visible = visible;
        self.viewport.visible = visible && self.window_visible;
        self.controller.update(cx, |controller, cx| {
            controller.set_viewport(self.pane, self.viewport, cx);
            controller.set_focus(self.pane, visible);
        });
        if visible {
            let controller = self.controller.clone();
            let _ = self.consume_frame(&controller, cx);
            self.focus_handle.focus(window, cx);
        }
        cx.notify();
    }

    pub(crate) fn apply_command(&mut self, command: BrowserCommand, cx: &mut Context<Self>) {
        match command {
            BrowserCommand::Navigate(url) => self.submit_address(&url, cx),
            BrowserCommand::Reload => {
                self.cancel_element_pick(cx);
                self.pending_history_uses.remove(&self.active_tab);
                self.controller.update(cx, |controller, cx| {
                    controller.reload(self.pane, self.active_tab, cx);
                });
            }
            BrowserCommand::Back => {
                self.cancel_element_pick(cx);
                self.pending_history_uses.remove(&self.active_tab);
                self.controller.update(cx, |controller, cx| {
                    controller.go_back(self.pane, self.active_tab, cx);
                });
            }
            BrowserCommand::Forward => {
                self.cancel_element_pick(cx);
                self.pending_history_uses.remove(&self.active_tab);
                self.controller.update(cx, |controller, cx| {
                    controller.go_forward(self.pane, self.active_tab, cx);
                });
            }
            BrowserCommand::SendKeys(tokens) => {
                for token in tokens {
                    match token {
                        KeyToken::Literal(text) => self.controller.update(cx, |controller, cx| {
                            controller.send_text(self.pane, self.active_tab, &text, cx);
                        }),
                        KeyToken::Named(name) => {
                            if let Some(input) = browser_named_key(&name) {
                                self.controller.update(cx, |controller, cx| {
                                    controller.send_key(self.pane, self.active_tab, input, cx);
                                });
                            }
                        }
                        KeyToken::Raw(_) => {}
                    }
                }
            }
            BrowserCommand::SendKeysRepeated { keys, count } => {
                for _ in 0..count.min(MAX_BROWSER_KEY_REPEAT) {
                    for token in &keys {
                        match token {
                            KeyToken::Literal(text) => {
                                self.controller.update(cx, |controller, cx| {
                                    controller.send_text(self.pane, self.active_tab, text, cx);
                                });
                            }
                            KeyToken::Named(name) => {
                                if let Some(input) = browser_named_key(name) {
                                    self.controller.update(cx, |controller, cx| {
                                        controller.send_key(self.pane, self.active_tab, input, cx);
                                    });
                                }
                            }
                            KeyToken::Raw(_) => {}
                        }
                    }
                }
            }
            BrowserCommand::Key(input) => {
                let input = browser_input_from_terminal(&input);
                self.controller.update(cx, |controller, cx| {
                    controller.send_key(self.pane, self.active_tab, input, cx);
                });
            }
            BrowserCommand::Screenshot { request_id, path } => {
                self.screenshot(request_id, path, cx);
            }
        }
    }

    pub(crate) fn screenshot(&mut self, request_id: u64, path: String, cx: &mut Context<Self>) {
        let mux = self.mux.read(cx);
        let taken = self.frame().and_then(|frame| {
            if mux.takes_screenshots() {
                mux.send_screenshot(request_id, &frame.png()?)?;
                return Ok(None);
            }
            frame.write_png(Path::new(&path))?;
            Ok(Some(path))
        });
        match taken {
            Ok(None) => {}
            Ok(Some(output)) => {
                mux.respond_to_request(GuiResponse::Success { request_id, output });
            }
            Err(message) => mux.respond_to_request(GuiResponse::Error {
                request_id,
                message,
            }),
        }
    }

    fn frame(&self) -> Result<Screenshot, String> {
        if let Some(image) = &self.image {
            let size = image.size(0);
            let bytes = image
                .as_bytes(0)
                .ok_or_else(|| "the browser frame has no pixel data".to_owned())?;
            let width =
                u32::try_from(size.width.0).map_err(|_| "invalid frame width".to_owned())?;
            let height =
                u32::try_from(size.height.0).map_err(|_| "invalid frame height".to_owned())?;
            return Screenshot::from_bgra(width, height, bytes);
        }
        #[cfg(target_os = "macos")]
        if let Some(surface) = &self.mac_surface {
            return crate::browser::macos_surface::read_pixel_buffer(surface);
        }
        Err(
            "no CPU-readable frame: either this pane has not rendered yet, or it renders \
             through a GPU texture with no readback path (restart zz with \
             ZZ_BROWSER_SHARED_TEXTURE=0 to capture it)"
                .to_owned(),
        )
    }

    pub(crate) fn update_content_bounds(
        &mut self,
        bounds: Bounds<Pixels>,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let started = diagnostics::timer(DIAGNOSTIC_TARGET);
        let previous = self.viewport;
        self.content_bounds = Some(bounds);
        let window_bounds = window.bounds();
        let next = Viewport {
            width: rounded_dimension(bounds.size.width),
            height: rounded_dimension(bounds.size.height),
            scale_factor: window.scale_factor(),
            window_zoom: window.zoom(),
            screen_x: rounded_coordinate(window_bounds.origin.x + bounds.origin.x * window.zoom()),
            screen_y: rounded_coordinate(window_bounds.origin.y + bounds.origin.y * window.zoom()),
            visible: self.visible && self.window_visible,
        }
        .sanitized();
        let resized = (next.width, next.height) != (self.viewport.width, self.viewport.height);
        if resized && self.resize_held.get() {
            self.resize_cover = ResizeCover::Held;
            return;
        }
        if next != self.viewport {
            self.viewport = next;
            self.controller.update(cx, |controller, cx| {
                controller.set_viewport(self.pane, next, cx);
            });
        }
        if self.resize_cover == ResizeCover::Held {
            if resized {
                self.await_resized_frame(cx);
            } else {
                self.resize_cover = ResizeCover::None;
            }
        }
        log::trace!(
            target: "zz::diagnostics::browser_render",
            "content_bounds pane={} bounds={bounds:?} previous_viewport={previous:?} next_viewport={next:?} changed={} elapsed_us={}",
            self.pane,
            previous != next,
            diagnostics::elapsed_us(started),
        );
    }

    pub(crate) fn share_resize_hold(&mut self, held: Rc<Cell<bool>>) {
        self.resize_held = held;
    }

    pub(crate) fn share_pane_canvas(&mut self, canvas: Rc<Cell<Bounds<Pixels>>>) {
        self.pane_canvas = canvas;
    }

    pub(crate) fn resize_covered(&self) -> bool {
        self.resize_cover != ResizeCover::None
    }

    fn await_resized_frame(&mut self, cx: &mut Context<Self>) {
        self.resize_covers += 1;
        let epoch = self.resize_covers;
        self.resize_cover = ResizeCover::Awaiting(epoch);
        cx.spawn(async move |view, cx| {
            cx.background_executor().timer(RESIZE_COVER_TIMEOUT).await;
            view.update(cx, |view, cx| {
                if view.resize_cover == ResizeCover::Awaiting(epoch) {
                    view.resize_cover = ResizeCover::None;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    fn focus_page(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.set_address_editing(false, cx);
        self.select_pane(cx);
        self.focus_handle.focus(window, cx);
        self.controller.update(cx, |controller, _| {
            controller.set_focus(self.pane, true);
        });
    }

    fn select_pane(&self, cx: &App) {
        self.mux
            .read(cx)
            .execute(zz_protocol::CommandInvocation::new(
                "select-pane",
                ["-t", &self.pane.to_string()],
            ));
    }

    fn consume_frame(
        &mut self,
        controller: &Entity<BrowserController>,
        cx: &mut Context<Self>,
    ) -> bool {
        let started = diagnostics::timer(DIAGNOSTIC_TARGET);
        let Some(frame) = controller.read(cx).latest_frame(self.pane, self.active_tab) else {
            log::trace!(
                target: "zz::diagnostics::browser_render",
                "consume_frame pane={} skipped=no_frame elapsed_us={}",
                self.pane,
                diagnostics::elapsed_us(started),
            );
            return false;
        };
        if self.frame_session == Some(frame.session) && frame.generation <= self.image_generation {
            log::trace!(
                target: "zz::diagnostics::browser_render",
                "consume_frame pane={} skipped=stale frame_generation={} image_generation={} elapsed_us={}",
                self.pane,
                frame.generation,
                self.image_generation,
                diagnostics::elapsed_us(started),
            );
            return false;
        }
        let (tier, image_strong_count) = match &frame.content {
            BrowserPaneFrameContent::OwnedBgra(image) => {
                if let Some(previous) = self.image.replace(image.clone()) {
                    self.retired_images.push(previous);
                }
                #[cfg(target_os = "macos")]
                {
                    self.mac_surface = None;
                    self.mac_surface_cache.clear();
                    if self.underlay.take().is_some() {
                        let pane = self.pane;
                        controller.update(cx, |controller, _| {
                            controller.set_frame_presenter(pane, None);
                        });
                    }
                }
                #[cfg(any(target_os = "linux", target_os = "freebsd"))]
                {
                    self.gpu_texture = frame
                        .readback_texture
                        .as_ref()
                        .map(|texture| texture.texture().clone());
                }
                #[cfg(target_os = "windows")]
                {
                    self.win_gpu_texture = None;
                }
                ("owned_bgra", Some(Arc::strong_count(image)))
            }
            BrowserPaneFrameContent::Gpu(texture) => {
                if let Some(previous) = self.image.take() {
                    self.retired_images.push(previous);
                }
                #[cfg(any(target_os = "linux", target_os = "freebsd"))]
                {
                    self.gpu_texture = Some(texture.clone());
                    ("gpu", None)
                }
                #[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
                {
                    let _ = texture;
                    log::warn!("received a GPU browser frame without ala external-texture support");
                    return true;
                }
            }
            #[cfg(target_os = "macos")]
            BrowserPaneFrameContent::MacGpu(io_surface) => {
                if let Some(previous) = self.image.take() {
                    self.retired_images.push(previous);
                }
                let Some(pool_generation) = frame.pool_generation else {
                    log::error!("macOS GPU browser frame is missing its pool generation");
                    return true;
                };
                match self.mac_surface_cache.pixel_buffer(
                    frame.session,
                    pool_generation,
                    io_surface,
                ) {
                    Ok(surface) => {
                        self.mac_surface = Some(surface);
                        if self.underlay.is_none() {
                            let underlay = PaneUnderlay::new(io_surface);
                            let presenter = underlay.presenter();
                            let pane = self.pane;
                            controller.update(cx, |controller, _| {
                                controller.set_frame_presenter(pane, Some(presenter));
                            });
                            self.underlay = Some(underlay);
                        }
                        ("mac_gpu", None)
                    }
                    Err(error) => {
                        log::warn!(
                            target: "ala_cef::accelerated_paint",
                            "could not wrap browser IOSurface for ala: {error}"
                        );
                        let pane = self.pane;
                        let tab = self.active_tab;
                        let reason = error.to_string();
                        controller.update(cx, |controller, _| {
                            controller.force_readback(pane, tab, &reason);
                        });
                        return true;
                    }
                }
            }
            #[cfg(target_os = "windows")]
            BrowserPaneFrameContent::WinGpu(texture) => {
                if let Some(previous) = self.image.take() {
                    self.retired_images.push(previous);
                }
                self.win_gpu_texture = Some(texture.clone());
                ("win_gpu", None)
            }
        };
        self.frame_session = Some(frame.session);
        self.image_generation = frame.generation;
        self.frame_size = (frame.logical_width, frame.logical_height);
        log::trace!(
            target: "zz::diagnostics::browser_render",
            "consume_frame pane={} session={} generation={} delivery_generation={} tier={tier} logical={}x{} device={}x{} pool_generation={:?} sequence={:?} image_strong_count={image_strong_count:?} retired_images_len={} retired_images_capacity={} total_elapsed_us={}",
            self.pane,
            frame.session.0,
            frame.generation,
            frame.delivery_generation,
            frame.logical_width,
            frame.logical_height,
            frame.width,
            frame.height,
            frame.pool_generation,
            frame.sequence,
            self.retired_images.len(),
            self.retired_images.capacity(),
            diagnostics::elapsed_us(started),
        );
        true
    }

    fn on_back(&mut self, cx: &mut Context<Self>) {
        if self.can_go_back {
            self.cancel_element_pick(cx);
            self.pending_history_uses.remove(&self.active_tab);
            self.controller.update(cx, |controller, cx| {
                controller.go_back(self.pane, self.active_tab, cx);
            });
        }
        cx.stop_propagation();
    }

    fn on_forward(&mut self, cx: &mut Context<Self>) {
        if self.can_go_forward {
            self.cancel_element_pick(cx);
            self.pending_history_uses.remove(&self.active_tab);
            self.controller.update(cx, |controller, cx| {
                controller.go_forward(self.pane, self.active_tab, cx);
            });
        }
        cx.stop_propagation();
    }

    fn on_reload(&mut self, cx: &mut Context<Self>) {
        self.cancel_element_pick(cx);
        self.pending_history_uses.remove(&self.active_tab);
        self.error = None;
        self.controller.update(cx, |controller, cx| {
            controller.reload(self.pane, self.active_tab, cx);
        });
        cx.stop_propagation();
        cx.notify();
    }

    fn on_hard_reload(&mut self, cx: &mut Context<Self>) {
        self.cancel_element_pick(cx);
        self.pending_history_uses.remove(&self.active_tab);
        self.error = None;
        self.controller.update(cx, |controller, cx| {
            controller.reload_ignore_cache(self.pane, self.active_tab, cx);
        });
        cx.stop_propagation();
        cx.notify();
    }

    fn on_retry(&mut self, cx: &mut Context<Self>) {
        if self.recoverable {
            self.recreate_session(cx);
        }
        cx.stop_propagation();
    }

    fn reset_frame_state(&mut self) {
        if let Some(image) = self.image.take() {
            self.retired_images.push(image);
        }
        #[cfg(any(target_os = "linux", target_os = "freebsd"))]
        {
            self.gpu_texture = None;
        }
        #[cfg(target_os = "windows")]
        {
            self.win_gpu_texture = None;
        }
        #[cfg(target_os = "macos")]
        {
            self.mac_surface = None;
            self.mac_surface_cache.clear();
        }
        self.frame_session = None;
        self.image_generation = 0;
    }

    fn on_go_back(&mut self, _: &GoBack, _: &mut Window, cx: &mut Context<Self>) {
        self.on_back(cx);
    }

    fn on_go_forward(&mut self, _: &GoForward, _: &mut Window, cx: &mut Context<Self>) {
        self.on_forward(cx);
    }

    fn on_reload_action(&mut self, _: &Reload, _: &mut Window, cx: &mut Context<Self>) {
        self.on_reload(cx);
    }

    fn on_hard_reload_action(&mut self, _: &HardReload, _: &mut Window, cx: &mut Context<Self>) {
        self.on_hard_reload(cx);
    }

    fn on_browser_edit(&mut self, action: &BrowserEdit, _: &mut Window, cx: &mut Context<Self>) {
        self.edit(action.command, cx);
        cx.stop_propagation();
    }

    fn recreate_session(&mut self, cx: &mut Context<Self>) {
        self.cancel_element_pick(cx);
        self.pending_history_uses.remove(&self.active_tab);
        self.error = None;
        self.loading_progress = None;
        self.status_message = None;
        self.find_matches = None;
        self.reset_frame_state();
        let url = self.current_url.clone();
        let request = browser_session_request(
            self.pane,
            url,
            self.profile.clone(),
            self.viewport,
            self.page_zoom_factor,
            self.gpu_context.clone(),
            &self.mux,
            cx,
        );
        let tab = self.active_tab;
        self.controller.update(cx, |controller, cx| {
            controller.retry(self.pane, tab, request, cx);
        });
        cx.notify();
    }

    fn publish_loading(&mut self, cx: &mut Context<Self>) {
        let pane = self.pane.to_string();
        let mux = self.mux.read(cx);
        let set = |name: &str, value: String| {
            mux.execute(CommandInvocation::new(
                "set-option",
                ["-p", "-t", pane.as_str(), name, value.as_str()],
            ));
        };
        if !self.loading {
            self.loads = self.loads.wrapping_add(1);
            set("@browser_loaded", self.loads.to_string());
        }
        set("@browser_loading", u8::from(self.loading).to_string());
    }

    fn publish_tabs(&mut self, cx: &mut Context<Self>) {
        if self.applying_mux {
            return;
        }
        let tabs = self.tab_urls();
        let active = self.active_tab_index();
        if self.mux_tabs == tabs && self.mux_active == active {
            return;
        }
        self.mux_tabs.clone_from(&tabs);
        self.mux_active = active;
        let mut args = vec![
            "-t".to_owned(),
            self.pane.to_string(),
            "-a".to_owned(),
            active.to_string(),
            "--".to_owned(),
        ];
        args.extend(tabs);
        self.mux
            .read(cx)
            .execute(CommandInvocation::new("set-browser-tabs", args));
    }

    pub(crate) fn synchronize_profile(&mut self, profile: &str, cx: &mut Context<Self>) {
        let Ok(profile) = normalize_browser_profile_name(profile) else {
            return;
        };
        let egress = browser_egress_route_spec(&self.mux, self.pane, &profile, cx);
        self.controller.update(cx, |controller, _| {
            controller.refresh_egress(self.pane, egress);
        });
        if self.profile == profile {
            return;
        }
        self.pending_history_uses.clear();
        self.omnibox.reset();
        self.profile = profile;
        let removed: Vec<TabId> = self
            .tabs
            .iter()
            .map(|tab| tab.id)
            .filter(|id| *id != self.active_tab)
            .collect();
        if !removed.is_empty() {
            self.tabs.retain(|tab| tab.id == self.active_tab);
            self.controller.update(cx, |controller, _| {
                for tab in removed {
                    controller.close_tab(self.pane, tab);
                }
            });
        }
        self.recreate_session(cx);
    }

    fn on_zoom_in(&mut self, _: &ZoomIn, _: &mut Window, cx: &mut Context<Self>) {
        self.apply_zoom(ZoomOp::In, cx);
        cx.stop_propagation();
    }

    fn on_zoom_out(&mut self, _: &ZoomOut, _: &mut Window, cx: &mut Context<Self>) {
        self.apply_zoom(ZoomOp::Out, cx);
        cx.stop_propagation();
    }

    fn on_reset_zoom(&mut self, _: &ResetZoom, _: &mut Window, cx: &mut Context<Self>) {
        self.apply_zoom(ZoomOp::Reset, cx);
        cx.stop_propagation();
    }

    fn on_toggle_dev_tools(&mut self, _: &ToggleDevTools, _: &mut Window, cx: &mut Context<Self>) {
        self.toggle_dev_tools(cx);
        cx.stop_propagation();
    }

    fn on_configured_element_selector(
        &mut self,
        action: &ConfiguredElementSelector,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if crate::keymap::action_for(cx, BROWSER_TABLE, &action.chord)
            != Some(ChromeAction::BrowserElementSelector)
        {
            cx.propagate();
            return;
        }
        self.on_element_pick(window, cx);
    }

    fn toggle_dev_tools(&mut self, cx: &mut Context<Self>) {
        self.controller.update(cx, |controller, _| {
            controller.toggle_dev_tools(self.pane, self.active_tab);
        });
    }

    fn apply_zoom(&mut self, op: ZoomOp, cx: &mut Context<Self>) {
        let result = self.controller.update(cx, |controller, _| match op {
            ZoomOp::In => controller.zoom_in(self.pane, self.active_tab),
            ZoomOp::Out => controller.zoom_out(self.pane, self.active_tab),
            ZoomOp::Reset => controller.reset_zoom(self.pane, self.active_tab),
        });
        if let Some((factor, percent)) = result {
            self.page_zoom_factor = factor;
            self.page_zoom_percent = percent;
            if self.element_pick_active && !self.start_element_pick(cx) {
                self.element_pick_active = false;
                self.emit_notification(
                    "Element picker is unavailable",
                    ClientMessageKind::Error,
                    cx,
                );
            }
            cx.notify();
        }
    }

    fn start_element_pick(&self, cx: &mut Context<Self>) -> bool {
        let appearance = element_picker_appearance(cx.theme(), self.page_zoom_factor);
        self.controller.update(cx, |controller, _| {
            controller.start_element_pick(self.pane, self.active_tab, &appearance)
        })
    }

    fn on_element_pick(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.element_pick_active {
            self.cancel_element_pick(cx);
        } else if self.start_element_pick(cx) {
            self.element_pick_active = true;
            self.focus_handle.focus(window, cx);
            self.controller.update(cx, |controller, _| {
                controller.set_focus(self.pane, true);
            });
        } else {
            self.emit_notification(
                "Element picker is unavailable",
                ClientMessageKind::Error,
                cx,
            );
        }
        cx.stop_propagation();
        cx.notify();
    }

    fn on_root_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.keystroke.key != "escape" {
            return;
        }
        if self.element_pick_active {
            self.cancel_element_pick(cx);
            cx.stop_propagation();
            cx.notify();
            return;
        }
        if self.find_open {
            self.close_find(window, cx);
            cx.stop_propagation();
            return;
        }
        if !self.address_editing {
            return;
        }
        if self.omnibox.selected.take().is_some() {
            let query = self.omnibox.query.clone();
            self.address.update(cx, |address, cx| {
                address.set_value(query.clone(), window, cx);
                address.set_selected_range(query.len()..query.len(), cx);
            });
            cx.stop_propagation();
            cx.notify();
            return;
        }
        if !self.omnibox.suggestions.is_empty() {
            self.omnibox.suggestions.clear();
            cx.stop_propagation();
            cx.notify();
            return;
        }
        let current_url = if is_blank_url(&self.current_url) {
            String::new()
        } else {
            self.current_url.clone()
        };
        self.address.update(cx, |address, cx| {
            address.set_value(current_url, window, cx);
        });
        self.focus_page(window, cx);
        cx.stop_propagation();
    }

    fn cancel_element_pick(&mut self, cx: &mut Context<Self>) {
        if !self.element_pick_active {
            return;
        }
        self.controller.update(cx, |controller, _| {
            let _ = controller.cancel_element_pick(self.pane, self.active_tab);
        });
        self.element_pick_active = false;
    }

    fn switch_profile(&mut self, profile: String, cx: &mut Context<Self>) {
        if profile != self.profile {
            self.mux.read(cx).execute(CommandInvocation::new(
                "set-browser-profile",
                ["-t".to_owned(), self.pane.to_string(), profile],
            ));
        }
    }

    fn on_page_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus_page(window, cx);
        self.page_buttons_down |= pointer_button_bit(event.button);
        self.controller.update(cx, |controller, _| {
            if let Some(event) = self.pointer_event(
                event.position,
                PointerPhase::Down,
                Some(event.button),
                event.click_count,
                event.modifiers,
                Some(event.button),
            ) {
                controller.send_pointer(self.pane, self.active_tab, event);
            }
        });
        cx.stop_propagation();
    }

    fn on_page_mouse_up(&mut self, event: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.release_page_button(event, cx);
        cx.stop_propagation();
    }

    fn on_page_mouse_up_out(
        &mut self,
        event: &MouseUpEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.page_buttons_down != 0 {
            self.release_page_button(event, cx);
        }
    }

    fn release_page_button(&mut self, event: &MouseUpEvent, cx: &mut Context<Self>) {
        self.page_buttons_down &= !pointer_button_bit(event.button);
        if let Some(event) = self.pointer_event(
            event.position,
            PointerPhase::Up,
            Some(event.button),
            event.click_count,
            event.modifiers,
            None,
        ) {
            self.controller.update(cx, |controller, _| {
                controller.send_pointer(self.pane, self.active_tab, event);
            });
        }
    }

    fn on_page_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !owns_pressed_button(self.page_buttons_down, event.pressed_button) {
            return;
        }
        if let Some(event) = self.pointer_event(
            event.position,
            PointerPhase::Move,
            None,
            0,
            event.modifiers,
            event.pressed_button,
        ) {
            self.controller.update(cx, |controller, _| {
                controller.send_pointer(self.pane, self.active_tab, event);
            });
        }
    }

    #[allow(
        clippy::trivially_copy_pass_by_ref,
        reason = "ala's hover callback supplies &bool"
    )]
    fn on_page_hover(&mut self, hovered: &bool, window: &mut Window, cx: &mut Context<Self>) {
        if !hovered
            && let Some(event) = self.pointer_event(
                window.mouse_position(),
                PointerPhase::Leave,
                None,
                0,
                ala::Modifiers::default(),
                None,
            )
        {
            self.controller.update(cx, |controller, _| {
                controller.send_pointer(self.pane, self.active_tab, event);
            });
        }
    }

    fn on_page_scroll(&mut self, event: &ScrollWheelEvent, _: &mut Window, cx: &mut Context<Self>) {
        let Some((x, y)) = self.local_point(event.position) else {
            return;
        };
        let delta = event.delta.pixel_delta(px(20.0));
        let wheel = WheelEvent {
            x,
            y,
            delta_x: rounded_coordinate(delta.x),
            delta_y: rounded_coordinate(delta.y),
            precise: event.delta.precise(),
            modifiers: browser_modifiers(event.modifiers, None, false),
        };
        self.controller.update(cx, |controller, cx| {
            controller.send_wheel(self.pane, self.active_tab, wheel, cx);
        });
        cx.stop_propagation();
    }

    fn on_page_key_down(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let (input, allow_text_input) = browser_surface_key_input(
            self.pane,
            &event.keystroke,
            if event.is_held {
                TerminalKeyAction::Repeat
            } else {
                TerminalKeyAction::Press
            },
        );
        self.mux.read(cx).send_input(input);
        if !allow_text_input {
            cx.stop_propagation();
        }
    }

    fn on_page_key_up(&mut self, event: &KeyUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.mux
            .read(cx)
            .send_input(InputMessage::BrowserSurfaceKey {
                pane: self.pane,
                input: terminal_key_input(&event.keystroke, TerminalKeyAction::Release),
                text_follows: false,
            });
        cx.stop_propagation();
    }

    fn local_point(&self, point: Point<Pixels>) -> Option<(i32, i32)> {
        self.local_point_with_clamping(point, false)
    }

    fn local_point_with_clamping(&self, point: Point<Pixels>, clamp: bool) -> Option<(i32, i32)> {
        let bounds = self.content_bounds?;
        if !clamp && !bounds.contains(&point) {
            return None;
        }
        Some((
            rounded_coordinate(point.x.clamp(bounds.origin.x, bounds.right()) - bounds.origin.x),
            rounded_coordinate(point.y.clamp(bounds.origin.y, bounds.bottom()) - bounds.origin.y),
        ))
    }

    fn pointer_event(
        &self,
        position: Point<Pixels>,
        phase: PointerPhase,
        button: Option<MouseButton>,
        click_count: usize,
        modifiers: ala::Modifiers,
        pressed: Option<MouseButton>,
    ) -> Option<PointerEvent> {
        let (x, y) =
            self.local_point_with_clamping(position, pointer_needs_clamping(phase, pressed))?;
        Some(PointerEvent {
            x,
            y,
            phase,
            button: button.and_then(pointer_button),
            click_count: i32::try_from(click_count).unwrap_or(i32::MAX),
            modifiers: browser_modifiers(modifiers, pressed, false),
        })
    }

    fn render_recent_row(
        index: usize,
        page: &RecentPage,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let url = page.url.clone();
        let view = cx.entity();
        browser_recent_row(
            ("browser-recent", index),
            display_url(&page.url),
            page.favicon(),
            penna::blocks::picker::list_row_height(cx),
            cx,
        )
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            view.update(cx, |view, cx| {
                view.submit_address(&url, cx);
                view.focus_page(window, cx);
            });
            cx.stop_propagation();
        })
    }
}

impl Focusable for BrowserView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EntityInputHandler for BrowserView {
    fn text_for_range(
        &mut self,
        _: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        actual_range.replace(0..0);
        Some(String::new())
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: 0..0,
            reversed: false,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked_text
            .as_ref()
            .map(|text| 0..text.encode_utf16().count())
    }

    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.marked_text = None;
        if std::mem::take(&mut self.prompt_composition) {
            cx.notify();
            return;
        }
        self.controller.update(cx, |controller, cx| {
            controller.finish_composition(self.pane, self.active_tab, cx);
        });
        cx.notify();
    }

    fn replace_text_in_range(
        &mut self,
        _: Option<Range<usize>>,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let was_composing = self.marked_text.take().is_some();
        let prompt_composition = std::mem::take(&mut self.prompt_composition);
        let prompt_active = self.mux.read(cx).command_prompt().is_some();
        if prompt_composition || prompt_active {
            if !text.is_empty() {
                self.mux
                    .read(cx)
                    .send_input(browser_surface_text_input(self.pane, text));
            }
        } else if was_composing {
            self.controller.update(cx, |controller, cx| {
                controller.commit_composition(self.pane, self.active_tab, text, cx);
            });
        } else if !text.is_empty() {
            self.mux
                .read(cx)
                .send_input(browser_surface_text_input(self.pane, text));
        }
        window.invalidate_character_coordinates();
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        _: Option<Range<usize>>,
        text: &str,
        selected: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.marked_text.is_none() {
            self.prompt_composition = self.mux.read(cx).command_prompt().is_some();
        }
        self.marked_text = (!text.is_empty()).then(|| text.to_owned());
        if self.prompt_composition {
            if text.is_empty() {
                self.prompt_composition = false;
            }
            window.invalidate_character_coordinates();
            cx.notify();
            return;
        }
        let selection = selected.unwrap_or_else(|| {
            let end = text.encode_utf16().count();
            end..end
        });
        self.controller.update(cx, |controller, cx| {
            if text.is_empty() {
                controller.cancel_composition(self.pane, self.active_tab, cx);
            } else {
                controller.set_composition(self.pane, self.active_tab, text, selection, cx);
            }
        });
        window.invalidate_character_coordinates();
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        _: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let characters: &[ImeCharacterBounds] = if self.prompt_composition {
            &[]
        } else {
            &self.ime_bounds
        };
        self.content_bounds
            .map(|content| ime_window_bounds(content, characters, range_utf16))
    }

    fn character_index_for_point(
        &mut self,
        _: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        Some(0)
    }
}

impl ala::EventEmitter<PaneDrag> for BrowserView {}

fn ime_window_bounds(
    content: Bounds<Pixels>,
    characters: &[ImeCharacterBounds],
    range_utf16: Range<usize>,
) -> Bounds<Pixels> {
    match ime_range_bounds(characters, range_utf16) {
        Some(character) => Bounds::new(
            content.origin + point(px(character.x), px(character.y)),
            ala::size(px(character.width.max(1.0)), px(character.height)),
        ),
        None => Bounds::new(
            content.origin,
            ala::size(px(1.0), px(18.0).min(content.size.height)),
        ),
    }
}

impl Render for BrowserChromeView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let back_browser = self.browser.clone();
        let back = browser_toolbar_button(
            cx,
            "browser-back",
            IconName::ArrowLeft,
            "Back",
            !self.state.can_go_back,
            false,
        )
        .on_click(move |_, _, cx| {
            if let Some(browser) = back_browser.upgrade() {
                browser.update(cx, BrowserView::on_back);
            }
        });
        let forward_browser = self.browser.clone();
        let forward = browser_toolbar_button(
            cx,
            "browser-forward",
            IconName::ArrowRight,
            "Forward",
            !self.state.can_go_forward,
            false,
        )
        .on_click(move |_, _, cx| {
            if let Some(browser) = forward_browser.upgrade() {
                browser.update(cx, BrowserView::on_forward);
            }
        });
        let reload_browser = self.browser.clone();
        let reload = browser_toolbar_button(
            cx,
            "browser-reload",
            IconName::Redo2,
            "Reload",
            false,
            false,
        )
        .on_click(move |_, _, cx| {
            if let Some(browser) = reload_browser.upgrade() {
                browser.update(cx, BrowserView::on_reload);
            }
        });
        let picker_browser = self.browser.clone();
        let picker = browser_toolbar_button(
            cx,
            "browser-element-picker",
            IconName::Inspector,
            if self.state.element_pick_active {
                "Cancel element picker"
            } else {
                "Pick an element"
            },
            false,
            self.state.element_pick_active,
        )
        .on_click(move |_, window, cx| {
            if let Some(browser) = picker_browser.upgrade() {
                browser.update(cx, |view, cx| view.on_element_pick(window, cx));
            }
        });
        let menu_browser = self.browser.clone();
        let menu_url = self.state.current_url.clone();
        let menu_profile = self.state.profile.clone();
        let menu_chrome_profiles = self.state.chrome_profiles.clone();
        let menu_discovery = self.state.chrome_profile_discovery;
        let menu_zoom_percent = self.state.page_zoom_percent;
        let can_clear_site_data = has_http_origin(&self.state.current_url);
        let can_import_chrome_data = chrome_import::cookie::automatic_import_supported();
        let menu_picker_active = self.state.element_pick_active;
        let more = browser_toolbar_button(
            cx,
            "browser-more",
            IconName::EllipsisVertical,
            "More browser actions",
            false,
            false,
        )
        .dropdown_menu(move |menu, window, cx| {
            let open_url = menu_url.clone();
            let copy_url = menu_url.clone();
            let multiple_browsers =
                chrome_import::profiles::DetectedChromeProfile::has_multiple_browsers(
                    &menu_chrome_profiles,
                );
            let current_profile_label = menu_chrome_profiles
                .iter()
                .find(|profile| profile.profile == menu_profile)
                .map_or_else(
                    || {
                        if menu_profile == ala_cef::DEFAULT_BROWSER_PROFILE {
                            "Default zz profile".to_owned()
                        } else {
                            menu_profile.clone()
                        }
                    },
                    |profile| profile.menu_label_for_list(multiple_browsers),
                );
            let state = BrowserActionMenuState {
                engine: BrowserMenuEngine::Chromium,
                current_profile_label: current_profile_label.into(),
                selected_profile: menu_profile.clone().into(),
                default_profile: ala_cef::DEFAULT_BROWSER_PROFILE.into(),
                profiles: menu_chrome_profiles
                    .iter()
                    .map(|profile| {
                        BrowserMenuProfile::new(
                            profile.profile.clone(),
                            profile.menu_label_for_list(multiple_browsers),
                        )
                    })
                    .collect(),
                profile_discovery: match menu_discovery {
                    ChromeProfileDiscovery::Loading => BrowserProfileDiscoveryState::Loading,
                    ChromeProfileDiscovery::Failed => BrowserProfileDiscoveryState::Failed,
                    ChromeProfileDiscovery::NotStarted | ChromeProfileDiscovery::Loaded => {
                        BrowserProfileDiscoveryState::Ready
                    }
                },
                zoom_percent: menu_zoom_percent,
                can_import_chrome_data,
                can_clear_site_data,
                picker_active: menu_picker_active,
            };
            let open_browser = menu_browser.clone();
            let copy_browser = menu_browser.clone();
            let switch_browser = menu_browser.clone();
            let profile_refresh_browser = menu_browser.clone();
            let zoom_in_browser = menu_browser.clone();
            let zoom_out_browser = menu_browser.clone();
            let reset_zoom_browser = menu_browser.clone();
            let zoom_percent_browser = menu_browser.clone();
            let chrome_import_browser = menu_browser.clone();
            let file_import_browser = menu_browser.clone();
            let paste_import_browser = menu_browser.clone();
            let clear_browser = menu_browser.clone();
            let find_browser = menu_browser.clone();
            let reload_browser = menu_browser.clone();
            let hard_reload_browser = menu_browser.clone();
            let save_pdf_browser = menu_browser.clone();
            let picker_browser = menu_browser.clone();
            let dev_tools_browser = menu_browser.clone();
            let actions = BrowserMenuActions::new()
                .open_url(move |_, cx| {
                    if open_browser.upgrade().is_some() {
                        cx.open_url(&open_url);
                    }
                })
                .copy_url(move |_, cx| {
                    if copy_browser.upgrade().is_some() {
                        cx.write_to_clipboard(ClipboardItem::new_string(copy_url.clone()));
                    }
                })
                .switch_profile(move |profile, _, cx| {
                    if let Some(browser) = switch_browser.upgrade() {
                        browser.update(cx, |view, cx| {
                            view.switch_profile(profile.to_string(), cx);
                        });
                    }
                })
                .refresh_profiles(move |window, cx| {
                    let profile_refresh_browser = profile_refresh_browser.clone();
                    window.defer(cx, move |window, cx| {
                        if let Some(browser) = profile_refresh_browser.upgrade() {
                            browser.update(cx, |view, cx| {
                                view.refresh_chrome_profiles(true, window, cx);
                            });
                        }
                    });
                })
                .zoom_percent(move |cx| {
                    zoom_percent_browser
                        .upgrade()
                        .map_or(menu_zoom_percent, |browser| {
                            browser.read(cx).page_zoom_percent
                        })
                })
                .zoom_in(move |_, cx| {
                    if let Some(browser) = zoom_in_browser.upgrade() {
                        browser.update(cx, |view, cx| view.apply_zoom(ZoomOp::In, cx));
                    }
                })
                .zoom_out(move |_, cx| {
                    if let Some(browser) = zoom_out_browser.upgrade() {
                        browser.update(cx, |view, cx| view.apply_zoom(ZoomOp::Out, cx));
                    }
                })
                .reset_zoom(move |_, cx| {
                    if let Some(browser) = reset_zoom_browser.upgrade() {
                        browser.update(cx, |view, cx| view.apply_zoom(ZoomOp::Reset, cx));
                    }
                })
                .import_chrome_data(move |source_profile, window, cx| {
                    if let Some(browser) = chrome_import_browser.upgrade() {
                        browser.update(cx, |view, cx| {
                            view.import_chrome_data(source_profile.to_string(), window, cx);
                        });
                    }
                })
                .import_cookies(move |window, cx| {
                    if let Some(browser) = file_import_browser.upgrade() {
                        browser.update(cx, |_, cx| {
                            BrowserView::prompt_cookie_import(window, cx);
                        });
                    }
                })
                .paste_cookies(move |_, cx| {
                    if let Some(browser) = paste_import_browser.upgrade() {
                        browser.update(cx, BrowserView::paste_cookies);
                    }
                })
                .clear_site_data(move |window, cx| {
                    if let Some(browser) = clear_browser.upgrade() {
                        browser.update(cx, |_, cx| {
                            BrowserView::confirm_clear_site_data(window, cx);
                        });
                    }
                })
                .find_in_page(move |window, cx| {
                    if let Some(browser) = find_browser.upgrade() {
                        browser.update(cx, |view, cx| view.open_find(window, cx));
                    }
                })
                .reload(move |_, cx| {
                    if let Some(browser) = reload_browser.upgrade() {
                        browser.update(cx, BrowserView::on_reload);
                    }
                })
                .hard_reload(move |_, cx| {
                    if let Some(browser) = hard_reload_browser.upgrade() {
                        browser.update(cx, BrowserView::on_hard_reload);
                    }
                })
                .save_as_pdf(move |window, cx| {
                    if let Some(browser) = save_pdf_browser.upgrade() {
                        browser.update(cx, |view, cx| view.prompt_save_pdf(window, cx));
                    }
                })
                .toggle_picker(move |window, cx| {
                    if let Some(browser) = picker_browser.upgrade() {
                        browser.update(cx, |view, cx| view.on_element_pick(window, cx));
                    }
                })
                .dev_tools(move |_, cx| {
                    if let Some(browser) = dev_tools_browser.upgrade() {
                        browser.update(cx, BrowserView::toggle_dev_tools);
                    }
                });
            browser_action_menu(menu, window, cx, state, actions)
        })
        .anchor(Anchor::TopRight);
        let site_browser = self.browser.clone();
        let site_focus_browser = self.browser.clone();
        let site_controls = browser_site_controls_button(cx)
            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                if let Some(browser) = site_focus_browser.upgrade() {
                    browser.update(cx, |view, cx| view.select_pane(cx));
                }
            })
            .dropdown_menu(move |menu, _, cx| {
                let Some(browser) = site_browser.upgrade() else {
                    return menu;
                };
                let (state, pane, tab) = {
                    let view = browser.read(cx);
                    let controller = view.controller.read(cx);
                    let http = has_http_origin(&view.current_url);
                    let site = url::Url::parse(&view.current_url)
                        .ok()
                        .and_then(|url| url.host_str().map(str::to_owned))
                        .unwrap_or_else(|| "This page".to_owned());
                    (
                        BrowserSiteMenuState {
                            site: site.into(),
                            connection_secure: http
                                .then(|| {
                                    controller.site_connection_secure(view.pane, view.active_tab)
                                })
                                .flatten(),
                            audio_muted: controller.audio_muted(view.pane, view.active_tab),
                            can_clear_site_data: http,
                        },
                        view.pane,
                        view.active_tab,
                    )
                };
                let muted = state.audio_muted;
                let sound_browser = browser.downgrade();
                let clear_browser = browser.downgrade();
                browser_site_menu(
                    menu,
                    state,
                    move |_, cx| {
                        if let Some(browser) = sound_browser.upgrade()
                            && let Some(muted) = muted
                        {
                            browser.update(cx, |view, cx| {
                                view.controller.update(cx, |controller, cx| {
                                    controller.set_audio_muted(pane, tab, !muted, cx);
                                });
                            });
                        }
                    },
                    move |window, cx| {
                        if let Some(browser) = clear_browser.upgrade() {
                            browser.update(cx, |view, cx| {
                                if view.pane == pane && view.active_tab == tab {
                                    BrowserView::confirm_clear_site_data(window, cx);
                                }
                            });
                        }
                    },
                )
            });
        let address_browser = self.browser.clone();
        let activate_browser = self.browser.clone();
        let close_browser = self.browser.clone();
        let move_browser = self.browser.clone();
        let new_tab_browser = self.browser.clone();
        let address = browser_address(&self.address, site_controls, window, cx).on_mouse_down(
            MouseButton::Left,
            move |_, _, cx| {
                if let Some(browser) = address_browser.upgrade() {
                    browser.update(cx, |view, cx| view.select_pane(cx));
                }
            },
        );
        let tabs = BrowserTabStrip::new(self.state.tabs.clone(), self.state.active_tab_index)
            .on_activate(move |id, window, cx| {
                if let Some(browser) = activate_browser.upgrade() {
                    browser.update(cx, |view, cx| view.activate_tab(TabId(id), window, cx));
                }
            })
            .on_close(move |id, window, cx| {
                if let Some(browser) = close_browser.upgrade() {
                    browser.update(cx, |view, cx| view.close_tab_by_id(TabId(id), window, cx));
                }
            })
            .on_move(move |id, to, _, cx| {
                if let Some(browser) = move_browser.upgrade() {
                    browser.update(cx, |view, cx| view.move_tab(TabId(id), to, cx));
                }
            })
            .on_new_tab(move |window, cx| {
                if let Some(browser) = new_tab_browser.upgrade() {
                    browser.update(cx, |view, cx| view.open_tab(None, true, window, cx));
                }
            });
        let tabs = match self.browser.upgrade() {
            Some(browser) => {
                let drag_browser = self.browser.clone();
                tabs.title(
                    PaneDragSource::new(
                        browser.read(cx).pane,
                        self.state.drag_title.clone(),
                        self.state.can_drag || self.state.floats,
                        move |drag, _, cx| {
                            if let Some(browser) = drag_browser.upgrade() {
                                browser.update(cx, |_, cx| cx.emit(*drag));
                            }
                        },
                    )
                    .floating(self.state.floats),
                )
            }
            None => tabs,
        };
        let tabs = div()
            .id("browser-tab-strip-drag")
            .flex()
            .flex_1()
            .min_w_0()
            .h_full()
            .child(tabs);
        let tabs = if self.state.floats {
            tabs
        } else {
            window_drag_handle(
                (
                    "browser-header-window-drag",
                    self.browser.entity_id().as_u64(),
                ),
                tabs,
                window,
                cx,
            )
        };
        let browser = self.browser.clone();
        let actions = PaneActions::new(
            zz_ui::pane::pane_header_actions(self.state.place),
            move |action, window, cx| {
                if let Some(browser) = browser.upgrade() {
                    browser.update(cx, |view, cx| {
                        let mux = view.mux.read(cx);
                        let rows = crate::workspace::pip_fill_rows(
                            mux,
                            view.pane,
                            view.pane_canvas.get().size,
                            window,
                            cx,
                        );
                        for command in action.commands(view.pane, rows) {
                            mux.execute(command);
                        }
                    });
                }
            },
        );
        div()
            .h(BrowserHeader::HEIGHT)
            .w_full()
            .flex_none()
            .opacity(self.state.opacity)
            .child(
                BrowserHeader::new(
                    self.state.active,
                    tabs,
                    actions,
                    BrowserToolbar::new(back, forward, reload, address, picker, more)
                        .show_separator(!self.state.shows_empty_state),
                )
                .trailing_inset(self.state.trailing_inset)
                .loading_progress(self.state.loading_progress),
            )
    }
}

impl Render for BrowserView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let shows_empty_state = self.shows_empty_state();
        let shows_native_state = shows_empty_state || self.error.is_some();
        if self.chrome_profile_discovery == ChromeProfileDiscovery::NotStarted {
            self.refresh_chrome_profiles(false, window, cx);
        }
        let (tabs, active_tab_index) = self.tab_strip_state(cx);
        let (active, can_drag, floats, place, drag_title) = {
            let mux = self.mux.read(cx);
            let snapshot = mux.snapshot();
            let pane_window = snapshot
                .sessions
                .iter()
                .flat_map(|session| &session.windows)
                .find(|window| window.panes.contains_key(&self.pane));
            (
                pane_window.is_some_and(|window| window.active_pane == self.pane),
                mux.is_connected()
                    && pane_window.is_some_and(|window| {
                        window.zoomed_pane.is_none() && window.panes.len() > 1
                    }),
                zz_client::floating::pane_floats(&snapshot, self.pane),
                zz_client::floating::pane_state(&snapshot, self.pane),
                pane_window
                    .and_then(|window| window.panes.get(&self.pane))
                    .map_or_else(|| "Browser".to_owned(), zz_client::navigation::pane_label),
            )
        };
        let snapshot = ChromeState {
            active,
            can_drag,
            floats,
            place,
            drag_title,
            opacity: self.chrome_opacity,
            trailing_inset: self.header_trailing_inset,
            shows_empty_state,
            loading_progress: self.loading_progress,
            can_go_back: self.can_go_back,
            can_go_forward: self.can_go_forward,
            element_pick_active: self.element_pick_active,
            current_url: self.current_url.clone(),
            profile: self.profile.clone(),
            chrome_profiles: self.chrome_profiles.clone(),
            chrome_profile_discovery: self.chrome_profile_discovery,
            page_zoom_percent: self.page_zoom_percent,
            tabs,
            active_tab_index,
        };
        if self.chrome.read(cx).state != snapshot {
            self.chrome.update(cx, |chrome, cx| {
                chrome.state = snapshot;
                cx.notify();
            });
        }
        let content_radii = if floats {
            Corners::all(zz_ui::pip::pip_content_radius(cx))
        } else {
            pane_content_radii(cx, self.window_corners)
        };
        let surface_radii = Corners {
            top_left: px(0.0),
            top_right: px(0.0),
            ..content_radii
        };
        let started = diagnostics::timer(DIAGNOSTIC_TARGET);
        let retired_count = self.retired_images.len();
        let mut reclaimed = Vec::new();
        for image in self.retired_images.drain(..) {
            if let Err(error) = window.drop_image(image.clone()) {
                log::warn!("failed to release superseded browser image: {error}");
            }
            if let Ok(image) = Arc::try_unwrap(image) {
                reclaimed.extend(
                    image
                        .into_frames()
                        .into_iter()
                        .map(|frame| frame.into_buffer().into_raw()),
                );
            }
        }
        if !reclaimed.is_empty() {
            let pane = self.pane;
            let tab = self.active_tab;
            self.controller.update(cx, |controller, _| {
                for bgra in reclaimed {
                    controller.recycle_frame(pane, tab, bgra);
                }
            });
        }
        log::trace!(
            target: "zz::diagnostics::browser_render",
            "render pane={} viewport={:?} visible={} image_generation={} has_image={} has_gpu_texture={} has_mac_surface={} retired_images_released={} url={:?} title={:?} loading={} focused={} elapsed_before_tree_us={}",
            self.pane,
            self.viewport,
            self.visible,
            self.image_generation,
            self.image.is_some(),
            {
                #[cfg(any(target_os = "linux", target_os = "freebsd"))]
                {
                    self.gpu_texture.is_some()
                }
                #[cfg(target_os = "windows")]
                {
                    self.win_gpu_texture.is_some()
                }
                #[cfg(not(any(
                    target_os = "linux",
                    target_os = "freebsd",
                    target_os = "windows"
                )))]
                {
                    false
                }
            },
            {
                #[cfg(target_os = "macos")]
                {
                    self.mac_surface.is_some()
                }
                #[cfg(not(target_os = "macos"))]
                {
                    false
                }
            },
            retired_count,
            self.current_url,
            self.title,
            self.loading,
            self.focus_handle.is_focused(window),
            diagnostics::elapsed_us(started),
        );

        let browser_surface = div()
            .id("browser-content")
            .when(!shows_empty_state, |surface| {
                surface.bg(pane_color(cx.theme(), cx.theme().background))
            })
            .relative()
            .flex()
            .flex_1()
            .overflow_hidden()
            .cursor(self.cursor)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|view, _: &crate::menus::Undo, _, cx| {
                view.edit(EditCommand::Undo, cx);
            }))
            .on_action(cx.listener(|view, _: &crate::menus::Redo, _, cx| {
                view.edit(EditCommand::Redo, cx);
            }))
            .on_action(cx.listener(|view, _: &crate::menus::Cut, _, cx| {
                view.edit(EditCommand::Cut, cx);
            }))
            .on_action(cx.listener(|view, _: &crate::menus::Copy, _, cx| {
                view.edit(EditCommand::Copy, cx);
            }))
            .on_action(cx.listener(|view, _: &crate::menus::Paste, _, cx| {
                view.edit(EditCommand::Paste, cx);
            }))
            .on_action(cx.listener(|view, _: &crate::menus::SelectAll, _, cx| {
                view.edit(EditCommand::SelectAll, cx);
            }))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_page_mouse_down))
            .on_mouse_down(MouseButton::Right, cx.listener(Self::on_page_mouse_down))
            .on_mouse_down(MouseButton::Middle, cx.listener(Self::on_page_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_page_mouse_up))
            .on_mouse_up(MouseButton::Right, cx.listener(Self::on_page_mouse_up))
            .on_mouse_up(MouseButton::Middle, cx.listener(Self::on_page_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_page_mouse_up_out))
            .on_mouse_up_out(MouseButton::Right, cx.listener(Self::on_page_mouse_up_out))
            .on_mouse_up_out(MouseButton::Middle, cx.listener(Self::on_page_mouse_up_out))
            .on_mouse_move(cx.listener(Self::on_page_mouse_move))
            .on_hover(cx.listener(Self::on_page_hover))
            .on_scroll_wheel(cx.listener(Self::on_page_scroll))
            .on_key_down(cx.listener(Self::on_page_key_down))
            .on_key_up(cx.listener(Self::on_page_key_up));
        let visible_image = if shows_native_state {
            None
        } else {
            self.image.clone()
        };
        let browser_element = BrowserElement::new(cx.entity(), visible_image, surface_radii);
        #[cfg(any(target_os = "linux", target_os = "freebsd"))]
        let browser_element = browser_element.texture(
            self.gpu_texture
                .as_ref()
                .filter(|_| !shows_native_state)
                .map(|texture| texture.create_view(&wgpu::TextureViewDescriptor::default())),
        );
        #[cfg(target_os = "windows")]
        let browser_element =
            browser_element.texture(self.win_gpu_texture.clone().filter(|_| !shows_native_state));
        #[cfg(target_os = "macos")]
        let browser_element = browser_element
            .surface(if shows_native_state {
                None
            } else {
                self.mac_surface.clone()
            })
            .underlay(
                self.underlay
                    .as_ref()
                    .filter(|_| !shows_native_state && self.mac_surface.is_some())
                    .map(PaneUnderlay::handle),
            );
        let mut content = round_div_radii(browser_surface.child(browser_element), surface_radii);

        if let Some(error) = self.error.clone() {
            let retry = self.recoverable.then(|| {
                let retry_view = cx.entity();
                Button::new("browser-retry")
                    .primary()
                    .on(penna::blocks::feedback::alert_ground(cx))
                    .label("Try again")
                    .on_click(move |_, _, cx| {
                        retry_view.update(cx, BrowserView::on_retry);
                    })
            });
            let mut error_panel = BrowserErrorPanel::new(error.to_string());
            if let Some(retry) = retry {
                error_panel = error_panel.retry(retry);
            }
            let select_view = cx.entity();
            let address_focus_handle = self.address.read(cx).focus_handle(cx);
            content = content.child(round_div_radii(
                browser_start_surface(error_panel).on_mouse_down(
                    MouseButton::Left,
                    move |_, window, cx| {
                        select_view.update(cx, |view, cx| view.select_pane(cx));
                        address_focus_handle.focus(window, cx);
                    },
                ),
                surface_radii,
            ));
        }

        if shows_empty_state {
            content = content.child(self.render_empty_state(surface_radii, cx));
        }

        if self.find_open {
            content = content.child(
                pane_overlay_stack(PaneOverlayCorner::TopRight, [self.render_find_bar(cx)])
                    .left(px(8.0)),
            );
        }

        if self.element_pick_active {
            content = content.child(BrowserPickStatus::new("Select an element · Esc to cancel"));
        } else if !shows_native_state && let Some(status) = self.status_message.clone() {
            content = content.child(BrowserStatusChip::new(status));
        }

        let omnibox_results = self.render_omnibox_results(cx);
        round_div_radii(
            div()
                .id("browser-root")
                .key_context(BROWSER_KEY_CONTEXT)
                .relative()
                .flex()
                .flex_col()
                .size_full()
                .on_action(cx.listener(Self::on_zoom_in))
                .on_action(cx.listener(Self::on_zoom_out))
                .on_action(cx.listener(Self::on_reset_zoom))
                .on_action(cx.listener(Self::on_toggle_dev_tools))
                .on_action(cx.listener(Self::on_configured_element_selector))
                .on_action(cx.listener(Self::on_new_tab))
                .on_action(cx.listener(Self::on_close_pane))
                .on_action(cx.listener(Self::on_next_tab))
                .on_action(cx.listener(Self::on_previous_tab))
                .on_action(cx.listener(Self::on_select_tab))
                .on_action(cx.listener(Self::on_select_last_tab))
                .on_action(cx.listener(Self::on_go_back))
                .on_action(cx.listener(Self::on_go_forward))
                .on_action(cx.listener(Self::on_reload_action))
                .on_action(cx.listener(Self::on_hard_reload_action))
                .on_action(cx.listener(Self::on_find_action))
                .on_action(cx.listener(Self::on_focus_address))
                .on_action(cx.listener(Self::on_omnibox_next))
                .on_action(cx.listener(Self::on_omnibox_previous))
                .on_action(cx.listener(Self::on_omnibox_delete))
                .on_action(cx.listener(Self::on_browser_edit))
                .on_key_down(cx.listener(Self::on_root_key_down))
                .child(
                    div()
                        .relative()
                        .rounded_tl(content_radii.top_left)
                        .rounded_tr(content_radii.top_right)
                        .h(BrowserHeader::HEIGHT)
                        .w_full()
                        .flex_none()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|view, _, window, cx| {
                                view.focus_page(window, cx);
                            }),
                        )
                        .child(
                            AnyView::from(self.chrome.clone()).cached(
                                StyleRefinement::default()
                                    .h(BrowserHeader::HEIGHT)
                                    .w_full()
                                    .flex_none(),
                            ),
                        ),
                )
                .child(content)
                .children(omnibox_results)
                .children(self.context_menu.as_ref().map(|context_menu| {
                    deferred(
                        anchored()
                            .position(context_menu.position)
                            .snap_to_window_with_margin(px(8.0))
                            .child(context_menu.menu.clone()),
                    )
                    .with_priority(1)
                })),
            content_radii,
        )
    }
}

fn display_url(url: &str) -> String {
    diagnostic_url(url)
}

fn has_http_origin(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://")
}

#[allow(
    clippy::cast_precision_loss,
    reason = "pane coordinates stay far below f32's exact integer range"
)]
fn pane_offset(x: i32, y: i32) -> Point<Pixels> {
    point(px(x as f32), px(y as f32))
}

#[derive(Clone)]
struct JsDialogReply {
    answered: Rc<Cell<bool>>,
    controller: WeakEntity<BrowserController>,
    pane: PaneId,
    tab: TabId,
    id: u64,
}

impl JsDialogReply {
    fn respond(&self, accept: bool, user_input: Option<&str>, cx: &mut App) {
        if self.answered.replace(true) {
            return;
        }
        if let Some(controller) = self.controller.upgrade() {
            controller.update(cx, |controller, cx| {
                controller.respond_js_dialog(self.pane, self.tab, self.id, accept, user_input, cx);
            });
        }
    }
}

fn normalize_loading_progress(progress: f32) -> Option<f32> {
    (progress.is_finite() && progress > 0.0 && progress < 1.0).then_some(progress)
}

fn default_downloads_dir() -> PathBuf {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from);
    if let Some(home) = home {
        let downloads = home.join("Downloads");
        if downloads.is_dir() {
            return downloads;
        }
        if home.is_dir() {
            return home;
        }
    }
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

fn save_dialog_defaults(default_file_path: Option<&Path>) -> (PathBuf, Option<String>) {
    let Some(path) = default_file_path else {
        return (default_downloads_dir(), None);
    };
    let dir = path
        .parent()
        .filter(|parent| parent.is_dir())
        .map_or_else(default_downloads_dir, Path::to_path_buf);
    let suggested = path
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .map(str::to_owned);
    (dir, suggested)
}

fn suggested_pdf_name(title: &str, url: &str) -> String {
    let raw = if !title.trim().is_empty() && title != "Browser" {
        title.trim().to_owned()
    } else {
        url::Url::parse(url)
            .ok()
            .and_then(|parsed| parsed.host_str().map(str::to_owned))
            .unwrap_or_else(|| "page".to_owned())
    };
    let cleaned: String = raw
        .chars()
        .take(80)
        .map(|ch| {
            if ch.is_control() || matches!(ch, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
            {
                '-'
            } else {
                ch
            }
        })
        .collect();
    let stem = cleaned.trim_matches(|ch: char| ch.is_whitespace() || ch == '.' || ch == '-');
    let stem = if stem.is_empty() { "page" } else { stem };
    if stem
        .rsplit_once('.')
        .is_some_and(|(_, ext)| ext.eq_ignore_ascii_case("pdf"))
    {
        stem.to_owned()
    } else {
        format!("{stem}.pdf")
    }
}

fn js_dialog_title(kind: JsDialogKind, origin_url: &str) -> String {
    if matches!(kind, JsDialogKind::BeforeUnload { .. }) {
        return "Leave site?".to_owned();
    }
    url::Url::parse(origin_url)
        .ok()
        .and_then(|parsed| parsed.host_str().map(str::to_owned))
        .map_or_else(
            || "This page says".to_owned(),
            |host| format!("{host} says"),
        )
}

fn browser_menu_item(
    label: &'static str,
    enabled: bool,
    browser: &WeakEntity<BrowserView>,
    action: impl Fn(&mut BrowserView, &mut Context<BrowserView>) + 'static,
) -> PopupMenuItem {
    browser_menu_item_owned(label, enabled, browser, action)
}

fn browser_menu_item_owned(
    label: impl Into<SharedString>,
    enabled: bool,
    browser: &WeakEntity<BrowserView>,
    action: impl Fn(&mut BrowserView, &mut Context<BrowserView>) + 'static,
) -> PopupMenuItem {
    let browser = browser.clone();
    PopupMenuItem::new(label)
        .disabled(!enabled)
        .on_click(move |_, _, cx| {
            if let Some(browser) = browser.upgrade() {
                browser.update(cx, |view, cx| action(view, cx));
            }
        })
}

fn browser_window_menu_item(
    label: &'static str,
    enabled: bool,
    browser: &WeakEntity<BrowserView>,
    action: impl Fn(&mut BrowserView, &mut Window, &mut Context<BrowserView>) + 'static,
) -> PopupMenuItem {
    let browser = browser.clone();
    PopupMenuItem::new(label)
        .disabled(!enabled)
        .on_click(move |_, window, cx| {
            if let Some(browser) = browser.upgrade() {
                browser.update(cx, |view, cx| action(view, window, cx));
            }
        })
}

fn ignored_suffix(ignored: usize) -> String {
    if ignored > 0 {
        format!("; {ignored} skipped")
    } else {
        String::new()
    }
}

fn read_cookie_import(path: &Path) -> Result<CookieImportBatch, String> {
    let file = File::open(path).map_err(|error| format!("Could not read cookie file: {error}"))?;
    let mut input = String::new();
    file.take((MAX_COOKIE_IMPORT_BYTES + 1) as u64)
        .read_to_string(&mut input)
        .map_err(|error| format!("Could not read cookie file: {error}"))?;
    parse_cookie_import(&input).map_err(|error| format!("Could not import cookies: {error}"))
}

pub(crate) const fn pointer_needs_clamping(
    phase: PointerPhase,
    pressed: Option<MouseButton>,
) -> bool {
    match phase {
        PointerPhase::Up => true,
        PointerPhase::Move => pressed.is_some(),
        PointerPhase::Down | PointerPhase::Leave => false,
    }
}

pub(crate) const fn pointer_button_bit(button: MouseButton) -> u8 {
    match button {
        MouseButton::Left | MouseButton::Navigate(_) => 1,
        MouseButton::Right => 1 << 1,
        MouseButton::Middle => 1 << 2,
    }
}

pub(crate) const fn owns_pressed_button(buttons: u8, pressed: Option<MouseButton>) -> bool {
    match pressed {
        Some(button) => buttons & pointer_button_bit(button) != 0,
        None => true,
    }
}

pub(crate) fn pointer_button(button: MouseButton) -> Option<PointerButton> {
    match button {
        MouseButton::Left => Some(PointerButton::Left),
        MouseButton::Middle => Some(PointerButton::Middle),
        MouseButton::Right => Some(PointerButton::Right),
        MouseButton::Navigate(_) => None,
    }
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "CEF view dimensions are bounded integers derived from finite ala pixels"
)]
pub(crate) fn rounded_dimension(value: Pixels) -> u32 {
    f32::from(value).round().clamp(1.0, u32::MAX as f32) as u32
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "CEF input coordinates are saturating integer device-independent pixels"
)]
pub(crate) fn rounded_coordinate(value: Pixels) -> i32 {
    f32::from(value)
        .round()
        .clamp(i32::MIN as f32, i32::MAX as f32) as i32
}

fn browser_input_from_terminal(input: &TerminalKeyInput) -> KeyInput {
    super::browser_key_input(input)
}

pub(crate) fn browser_modifiers(
    value: ala::Modifiers,
    pressed: Option<MouseButton>,
    is_repeat: bool,
) -> Modifiers {
    let pressed = pressed.map(|button| match button {
        MouseButton::Left | MouseButton::Navigate(_) => PointerButton::Left,
        MouseButton::Right => PointerButton::Right,
        MouseButton::Middle => PointerButton::Middle,
    });
    Modifiers::new(value.shift, value.control, value.alt, value.platform)
        .with_pointer_button(pressed)
        .with_repeat(is_repeat)
}

pub(crate) fn browser_key(key: &str) -> BrowserKey {
    match key {
        "space" => BrowserKey::Space,
        "backspace" => BrowserKey::Backspace,
        "tab" => BrowserKey::Tab,
        "enter" => BrowserKey::Enter,
        "escape" => BrowserKey::Escape,
        "pageup" => BrowserKey::PageUp,
        "pagedown" => BrowserKey::PageDown,
        "end" => BrowserKey::End,
        "home" => BrowserKey::Home,
        "left" => BrowserKey::ArrowLeft,
        "up" => BrowserKey::ArrowUp,
        "right" => BrowserKey::ArrowRight,
        "down" => BrowserKey::ArrowDown,
        "insert" => BrowserKey::Insert,
        "delete" => BrowserKey::Delete,
        _ => {
            let mut characters = key.chars();
            if let (Some(character), None) = (characters.next(), characters.next()) {
                BrowserKey::Character(character)
            } else {
                key.strip_prefix('f')
                    .and_then(|number| number.parse::<u8>().ok())
                    .map_or(BrowserKey::Unidentified, BrowserKey::Function)
            }
        }
    }
}

fn browser_named_key(name: &str) -> Option<KeyInput> {
    ala_cef::named_key_input(name)
}

pub(crate) fn allows_text_input(key: BrowserKey, modifiers: ala::Modifiers) -> bool {
    !modifiers.control
        && !modifiers.alt
        && !modifiers.platform
        && matches!(
            key,
            BrowserKey::Character(_) | BrowserKey::Space | BrowserKey::Unidentified
        )
}

fn browser_surface_key_input(
    pane: PaneId,
    keystroke: &ala::Keystroke,
    action: TerminalKeyAction,
) -> (InputMessage, bool) {
    let text_follows = allows_text_input(browser_key(&keystroke.key), keystroke.modifiers);
    (
        InputMessage::BrowserSurfaceKey {
            pane,
            input: terminal_key_input(keystroke, action),
            text_follows,
        },
        text_follows,
    )
}

fn browser_surface_text_input(pane: PaneId, text: &str) -> InputMessage {
    InputMessage::BrowserSurfaceText {
        pane,
        text: text.to_owned(),
    }
}

pub(crate) fn cursor_style(cursor: BrowserCursor) -> CursorStyle {
    match cursor {
        BrowserCursor::Arrow | BrowserCursor::Wait | BrowserCursor::Help | BrowserCursor::None => {
            CursorStyle::Arrow
        }
        BrowserCursor::IBeam => CursorStyle::IBeam,
        BrowserCursor::PointingHand => CursorStyle::PointingHand,
        BrowserCursor::Crosshair => CursorStyle::Crosshair,
        BrowserCursor::Move | BrowserCursor::Grab => CursorStyle::OpenHand,
        BrowserCursor::Grabbing => CursorStyle::ClosedHand,
        BrowserCursor::ResizeHorizontal => CursorStyle::ResizeLeftRight,
        BrowserCursor::ResizeVertical => CursorStyle::ResizeUpDown,
        BrowserCursor::ResizeNorthEastSouthWest => CursorStyle::ResizeUpRightDownLeft,
        BrowserCursor::ResizeNorthWestSouthEast => CursorStyle::ResizeUpLeftDownRight,
        BrowserCursor::NotAllowed => CursorStyle::OperationNotAllowed,
    }
}

#[cfg(test)]
mod element_picker_tests;

#[cfg(test)]
mod keymap_tests;

#[cfg(test)]
mod pointer_tests;

#[cfg(test)]
mod session_tests;

#[cfg(test)]
mod test_support;

#[cfg(test)]
mod text_input_tests;

#[cfg(test)]
mod tests {
    use std::any::TypeId;

    use ala::{KeyContext, Keymap, Keystroke};

    use super::test_support::*;
    use super::*;

    ala::actions!(
        browser_view_test,
        [FocusNext, FocusPrevious, RootCopy, InputEdit]
    );

    fn assert_action_types(bindings: &[KeyBinding], expected: &[TypeId]) {
        let actual = bindings
            .iter()
            .map(|binding| binding.action().as_any().type_id())
            .collect::<Vec<_>>();
        assert_eq!(actual, expected);
    }

    fn edit_action_type(command: EditCommand) -> TypeId {
        match command {
            EditCommand::Undo => TypeId::of::<crate::menus::Undo>(),
            EditCommand::Redo => TypeId::of::<crate::menus::Redo>(),
            EditCommand::Cut => TypeId::of::<crate::menus::Cut>(),
            EditCommand::Copy => TypeId::of::<crate::menus::Copy>(),
            EditCommand::Paste => TypeId::of::<crate::menus::Paste>(),
            EditCommand::SelectAll => TypeId::of::<crate::menus::SelectAll>(),
            EditCommand::PasteAndMatchStyle => TypeId::of::<BrowserEdit>(),
        }
    }

    #[test]
    fn browser_copy_outranks_window_text_copy() {
        #[cfg(target_os = "macos")]
        let copy = "cmd-c";
        #[cfg(not(target_os = "macos"))]
        let copy = "ctrl-c";
        let mut bindings = vec![KeyBinding::new(
            copy,
            RootCopy,
            Some(penna::ROOT_KEY_CONTEXT),
        )];
        bindings.extend(browser_key_bindings());
        let keymap = Keymap::new(bindings);
        let bindings = bindings_for_contexts(
            &keymap,
            copy,
            &["Root", penna::ROOT_KEY_CONTEXT, BROWSER_KEY_CONTEXT],
        );

        assert_action_types(
            &bindings,
            &[TypeId::of::<crate::menus::Copy>(), TypeId::of::<RootCopy>()],
        );
    }

    #[test]
    fn focused_address_field_outranks_browser_page_edits() {
        #[cfg(target_os = "macos")]
        let edits = [
            ("cmd-z", EditCommand::Undo),
            ("cmd-shift-z", EditCommand::Redo),
            ("cmd-x", EditCommand::Cut),
            ("cmd-c", EditCommand::Copy),
            ("cmd-v", EditCommand::Paste),
            ("cmd-shift-v", EditCommand::PasteAndMatchStyle),
            ("cmd-a", EditCommand::SelectAll),
        ];
        #[cfg(not(target_os = "macos"))]
        let edits = [
            ("ctrl-z", EditCommand::Undo),
            ("ctrl-y", EditCommand::Redo),
            ("ctrl-shift-z", EditCommand::Redo),
            ("ctrl-x", EditCommand::Cut),
            ("ctrl-c", EditCommand::Copy),
            ("ctrl-v", EditCommand::Paste),
            ("ctrl-shift-v", EditCommand::PasteAndMatchStyle),
            ("ctrl-a", EditCommand::SelectAll),
        ];
        let mut bindings = edits
            .iter()
            .map(|(source, _)| KeyBinding::new(source, InputEdit, Some("PennaInput")))
            .collect::<Vec<_>>();
        #[cfg(target_os = "macos")]
        bindings.push(KeyBinding::new(
            "cmd-c",
            RootCopy,
            Some(penna::ROOT_KEY_CONTEXT),
        ));
        #[cfg(not(target_os = "macos"))]
        bindings.push(KeyBinding::new(
            "ctrl-c",
            RootCopy,
            Some(penna::ROOT_KEY_CONTEXT),
        ));
        bindings.extend(browser_key_bindings());
        let keymap = Keymap::new(bindings);
        let contexts = [
            "Root",
            penna::ROOT_KEY_CONTEXT,
            BROWSER_KEY_CONTEXT,
            "PennaInput",
        ];

        for (source, command) in edits {
            let resolved = bindings_for_contexts(&keymap, source, &contexts);
            let expected = if command == EditCommand::Copy {
                vec![
                    TypeId::of::<InputEdit>(),
                    edit_action_type(command),
                    TypeId::of::<RootCopy>(),
                ]
            } else {
                vec![TypeId::of::<InputEdit>(), edit_action_type(command)]
            };
            assert_action_types(&resolved, &expected);
            assert_eq!(edit_action_command(resolved[1].action()), Some(command));
        }
    }

    #[test]
    fn focused_omnibox_outranks_single_line_input_navigation() {
        let mut bindings = vec![
            KeyBinding::new("down", InputEdit, Some("PennaInput")),
            KeyBinding::new("up", InputEdit, Some("PennaInput")),
            KeyBinding::new("shift-delete", InputEdit, Some("PennaInput")),
        ];
        bindings.extend(browser_key_bindings());
        let keymap = Keymap::new(bindings);
        let contexts = [
            "Root",
            BROWSER_KEY_CONTEXT,
            zz_ui::browser::BROWSER_OMNIBOX_KEY_CONTEXT,
            "PennaInput",
        ];

        for (source, action) in [
            ("down", TypeId::of::<OmniboxNext>()),
            ("up", TypeId::of::<OmniboxPrevious>()),
            ("shift-delete", TypeId::of::<OmniboxDelete>()),
        ] {
            let resolved = bindings_for_contexts(&keymap, source, &contexts);
            assert_eq!(resolved[0].action().as_any().type_id(), action);
            assert_eq!(
                resolved[1].action().as_any().type_id(),
                TypeId::of::<InputEdit>()
            );
        }
    }

    #[test]
    fn browser_context_leaves_tab_for_chromium_raw_key_input() {
        let mut bindings = vec![
            KeyBinding::new("tab", FocusNext, Some("Root")),
            KeyBinding::new("shift-tab", FocusPrevious, Some("Root")),
        ];
        bindings.extend(browser_key_bindings());
        let keymap = Keymap::new(bindings);
        let root_context = KeyContext::parse("Root").expect("valid root key context");
        let browser_context =
            KeyContext::parse(BROWSER_KEY_CONTEXT).expect("valid browser key context");

        for source in ["tab", "shift-tab"] {
            let keystroke = Keystroke::parse(source).expect("valid tab keystroke");
            let (browser_bindings, pending) = keymap.bindings_for_input(
                std::slice::from_ref(&keystroke),
                &[root_context.clone(), browser_context.clone()],
            );
            assert!(browser_bindings.is_empty());
            assert!(!pending);

            let (root_bindings, pending) = keymap.bindings_for_input(
                std::slice::from_ref(&keystroke),
                std::slice::from_ref(&root_context),
            );
            assert_eq!(root_bindings.len(), 1);
            assert!(!pending);
        }
    }
}
