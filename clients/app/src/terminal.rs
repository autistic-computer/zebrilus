use std::{cell::Cell, collections::HashSet, ops::Range, rc::Rc, sync::Arc, time::Duration};

use ala::{
    AnyElement, App, Bounds, ClipboardEntry, ClipboardItem, Context, Corners, ElementInputHandler,
    Entity, EntityInputHandler, FocusHandle, Focusable, Font, Hsla, KeyDownEvent, KeyUpEvent,
    Keystroke, ModifiersChangedEvent, MouseButton, MouseDownEvent, MouseExitEvent, MouseMoveEvent,
    MouseUpEvent, Pixels, Point, Render, ScrollDelta, ScrollWheelEvent, Subscription, Task,
    TextInputAction, TextInputConfiguration, UTF16Selection, Window, canvas, div, prelude::*, px,
};
use penna::{
    ActiveTheme, IconName,
    components::input::{InputEvent, InputState},
};
use zz_client::{
    ChromeAction, ClientCore, CoreEvent, TERMINAL_TABLE,
    local_scroll::{
        LOCAL_SCROLL_DEBOUNCE, LOCAL_SCROLL_TIMEOUT, LocalScrollEffect, LocalScrollState,
        LocalScrollStep, LocalScrollSync, local_scroll_available, scroll_fraction_offset,
    },
    scrollback::RetainedTerminalViewport,
    terminal_view::{
        PointerGrid, accumulate_scroll, mode_text as terminal_mode_text, oversized_paste_warning,
        pasted_image_number, presented_uri, search_key_action, selection_has_extent,
        status_text as terminal_status_text, terminal_shortcut,
    },
};
use zz_protocol::{InputMessage, PaneId, TerminalTheme, TerminalUiCommand};
use zz_terminal::{
    AppearanceConfigKey, AppearanceSource, Cursor, CursorBlinkPolicy, CursorStyle, KeyAction,
    KeyCode, KeyInput, LocalColors, PointerCellEvent, SearchCase, SearchDirection, SearchMode,
    SearchQuery, SearchStatus, TerminalAppearance, TerminalMode, TerminalMouseButton,
    TerminalMouseInput, TerminalMousePhase, TerminalPalette, TerminalViewAction, TerminalViewport,
};
use zz_ui::{
    pane::{
        HeaderBand, PaneOverlayCorner, pane_overlay_stack, pane_status_badge, pane_sync_badge,
        terminal_link_popup, terminal_mode_indicator, terminal_status_popup,
    },
    pane_look::pane_color,
    terminal::{
        CURSOR_BLINK_TIMEOUT, CopyFlash, CopyFlashTracker, FindBarAction, GridSize, PaintState,
        RowRenderCache, ScrollbarFade, TerminalRenderInput, cursor_should_blink, paint_copy_flash,
        selection_autoscroll_lines, terminal_font_for_style,
    },
};

use crate::{connection::Connection, preferences::ChromeKeymaps};

#[derive(Clone, PartialEq)]
pub(crate) struct TerminalDisplayPreferences {
    pub font_family: Option<String>,
    pub font_scale: f32,
    pub contrast: Option<bool>,
    pub cursor_style: Option<CursorStyle>,
    pub cursor_blink: Option<CursorBlinkPolicy>,
    pub padding_x: Option<f32>,
    pub padding_y: Option<f32>,
    pub light_theme: Option<TerminalTheme>,
    pub dark_theme: Option<TerminalTheme>,
}

pub(crate) const TERMINAL_CONTRAST_ON: f32 = 4.5;

impl ala::Global for TerminalDisplayPreferences {}

pub(crate) fn shown_terminal_theme(cx: &App) -> Option<&TerminalTheme> {
    if crate::app::phone() {
        return None;
    }
    let preferences = cx.try_global::<TerminalDisplayPreferences>()?;
    if cx.theme().mode.is_dark() {
        preferences.dark_theme.as_ref()
    } else {
        preferences.light_theme.as_ref()
    }
}

pub(crate) fn phone_colors(
    viewport: &TerminalViewport,
    palette: &TerminalPalette,
    cx: &App,
) -> Option<LocalColors> {
    crate::app::phone().then(|| {
        LocalColors::new(
            viewport.foreground,
            viewport.background,
            palette,
            zz_ui::terminal::theme_terminal_colors(cx),
        )
    })
}

pub(crate) fn localized_font_appearance(
    core: &ClientCore,
    available_fonts: &[String],
    fallback: &str,
    cx: &App,
) -> TerminalAppearance {
    let mut appearance = core.appearance().cloned().unwrap_or_default();
    for (families, key) in [
        (
            &mut appearance.font_families,
            AppearanceConfigKey::FontFamily,
        ),
        (
            &mut appearance.font_families_bold,
            AppearanceConfigKey::FontFamilyBold,
        ),
        (
            &mut appearance.font_families_italic,
            AppearanceConfigKey::FontFamilyItalic,
        ),
        (
            &mut appearance.font_families_bold_italic,
            AppearanceConfigKey::FontFamilyBoldItalic,
        ),
    ] {
        localize_font_stack(
            families,
            core.appearance_provenance().source(key),
            available_fonts,
        );
    }
    if appearance.font_families.is_empty() {
        appearance.font_families.push(fallback.to_owned());
    }
    if let Some(preferences) = cx.try_global::<TerminalDisplayPreferences>() {
        if let Some(family) = preferences.font_family.as_ref().filter(|family| {
            available_fonts
                .iter()
                .any(|available| available.eq_ignore_ascii_case(family))
        }) {
            appearance.font_families = vec![family.clone()];
            appearance.font_families_bold.clear();
            appearance.font_families_italic.clear();
            appearance.font_families_bold_italic.clear();
        }
        appearance.font_size_points =
            (appearance.font_size_points * preferences.font_scale).clamp(7.0, 48.0);
        if crate::app::phone() {
            return appearance;
        }
        if let Some(contrast) = preferences.contrast {
            appearance.minimum_contrast = if contrast { TERMINAL_CONTRAST_ON } else { 1.0 };
        }
        if let Some(style) = preferences.cursor_style {
            appearance.cursor_style = style;
        }
        if let Some(blink) = preferences.cursor_blink {
            appearance.cursor_blink_policy = blink;
        }
        if let Some(padding) = preferences.padding_x {
            appearance.padding_left = padding;
            appearance.padding_right = padding;
        }
        if let Some(padding) = preferences.padding_y {
            appearance.padding_top = padding;
            appearance.padding_bottom = padding;
        }
    }
    if let Some(theme) = shown_terminal_theme(cx) {
        theme.recolor(&mut appearance);
    }
    appearance
}

pub(crate) fn tile_cell_size(
    core: &ClientCore,
    available_fonts: &[String],
    window: &Window,
    cx: &App,
) -> ala::Size<Pixels> {
    let fallback = &cx.theme().mono_font_family;
    let appearance = localized_font_appearance(core, available_fonts, fallback, cx);
    zz_ui::terminal::terminal_cell_size(
        &terminal_font_for_style(&appearance, fallback, false, false),
        px(appearance.font_size_points.clamp(7., 48.)),
        &appearance,
        window,
    )
}

pub(crate) fn device_cursor(
    cursor: Option<Cursor>,
    host: CursorStyle,
    device: CursorStyle,
) -> Option<Cursor> {
    cursor
        .filter(|cursor| device != host && cursor.style() == host)
        .map(|cursor| {
            Cursor::new(
                cursor.column(),
                cursor.row(),
                cursor.visible(),
                cursor.blinking(),
                cursor.at_wide_tail(),
                device,
                cursor.color(),
            )
        })
}

fn cursor_blink_policy(core: &ClientCore, cx: &App) -> CursorBlinkPolicy {
    cx.try_global::<TerminalDisplayPreferences>()
        .filter(|_| !crate::app::phone())
        .and_then(|preferences| preferences.cursor_blink)
        .or_else(|| {
            core.appearance()
                .map(|appearance| appearance.cursor_blink_policy)
        })
        .unwrap_or_default()
}

fn localize_font_stack(families: &mut Vec<String>, source: AppearanceSource, available: &[String]) {
    if source == AppearanceSource::Default {
        families.clear();
    } else {
        families.retain(|family| {
            available
                .iter()
                .any(|name| name.eq_ignore_ascii_case(family))
        });
    }
}

#[derive(Clone)]
pub struct TerminalText {
    pub origin: Point<Pixels>,
    pub line_height: Pixels,
    pub rows: u16,
    pub font: Font,
    pub font_size: Pixels,
}

pub struct TerminalPane {
    pane: PaneId,
    connection: Entity<Connection>,
    focus: FocusHandle,
    bounds: Bounds<Pixels>,
    cursor_bounds: Bounds<Pixels>,
    surface_bounds: Bounds<Pixels>,
    cell_width: Pixels,
    line_height: Pixels,
    floating: bool,
    text_font: Option<(Font, Pixels)>,
    scale: f32,
    force_local_selection: bool,
    available_fonts: Vec<String>,
    text_opacity: f32,
    pane_status: (Option<String>, bool),
    corner_radii: Corners<Pixels>,
    header: Option<HeaderBand>,
    rows_above: bool,
    rows_below: bool,
    resize_suppressed: Rc<Cell<bool>>,
    moving: Rc<Cell<bool>>,
    extra_height: Pixels,
    content_offset: Pixels,
    scroll_rows: f32,
    scroll: LocalScrollState,
    overscroll: ala::RubberBand,
    geometry: Option<(GridSize, u64)>,
    cache: RowRenderCache,
    copy_flash: CopyFlashTracker,
    observed_copy_generation: Option<u64>,
    painted_copy_flash: Option<(CopyFlash, TerminalAppearance)>,
    focused: bool,
    cursor_blink_visible: bool,
    cursor_blink_task: Option<Task<()>>,
    cursor_blink_idle_since: web_time::Instant,
    cursor_blink_expired: bool,
    painted: bool,
    selection_dragging: bool,
    scrollbar_dragging: bool,
    scrollbar_fade: ScrollbarFade,
    scrollbar_fade_task: Option<Task<()>>,
    pressed_buttons: HashSet<MouseButton>,
    pointer: Option<Point<Pixels>>,
    hovered_image_uri: Option<Arc<str>>,
    image_hover_ready: bool,
    image_hover_task: Option<Task<()>>,
    link_hover_bounds: Option<Bounds<Pixels>>,
    image_preview_press: Option<(u32, Bounds<Pixels>)>,
    selection_anchor: Option<PointerCellEvent>,
    selection_pointer: Option<PointerCellEvent>,
    selection_autoscroll_lines: i32,
    selection_autoscroll_task: Option<Task<()>>,
    focus_subscriptions: Vec<Subscription>,
    forwarded: HashSet<String>,
    marked_text: Option<String>,
    surface: TerminalSurface,
    search: Option<SearchQuery>,
    search_input: Option<Entity<InputState>>,
    search_focus_pending: bool,
    search_accept_on_enter: bool,
    swallowed_search_key: Option<KeyCode>,
    compact: bool,
    _subscription: Subscription,
}

pub struct TerminalTyped;

impl ala::EventEmitter<TerminalTyped> for TerminalPane {}

pub struct TerminalFontStep(pub f32);

impl ala::EventEmitter<TerminalFontStep> for TerminalPane {}

const TERMINAL_FONT_STEP: f32 = 0.1;

pub struct PhoneFindState {
    pub input: Entity<InputState>,
    pub count: String,
    pub found: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TerminalSurface {
    Pane,
    CommandOutput,
}

impl TerminalPane {
    pub fn new(pane: PaneId, connection: Entity<Connection>, cx: &mut Context<Self>) -> Self {
        let subscription = cx.subscribe(
            &connection,
            move |this, connection, event: &CoreEvent, cx| match event {
                CoreEvent::ViewportChanged { pane: changed, .. } if *changed == pane => {
                    let restore =
                        connection
                            .read(cx)
                            .core
                            .retained_viewport(pane)
                            .and_then(|retained| {
                                this.scroll
                                    .observe(retained, false, web_time::Instant::now())
                                    .restore
                            });
                    if let Some(target) = restore {
                        this.view(TerminalViewAction::ScrollToOffset(target), cx);
                    }
                    cx.notify();
                }
                CoreEvent::KittyImageChunk { pane: changed, .. }
                | CoreEvent::KittyImagesRemoved { pane: changed, .. }
                    if *changed == pane =>
                {
                    cx.notify();
                }
                CoreEvent::Message(message)
                    if matches!(message.as_ref(),
                    zz_protocol::ProtocolMessage::PastedImageChunk { pane: changed, .. }
                    | zz_protocol::ProtocolMessage::PastedImageUnavailable { pane: changed, .. }
                    if *changed == pane) =>
                {
                    cx.notify();
                }
                CoreEvent::Attached { .. } | CoreEvent::AppearanceChanged => {
                    this.observe_image_hover(None, cx);
                    if matches!(event, CoreEvent::Attached { .. }) {
                        this.scroll.cancel();
                    }
                    this.geometry = None;
                    this.cursor_blink_visible = true;
                    this.cursor_blink_task = None;
                    cx.notify();
                }
                CoreEvent::SnapshotChanged
                    if this.surface == TerminalSurface::Pane
                        && this.geometry.is_some_and(|(_, generation)| {
                            generation != connection.read(cx).core.layout_generation()
                        }) =>
                {
                    cx.notify();
                }
                CoreEvent::CommandOutputChanged
                    if this.surface == TerminalSurface::CommandOutput =>
                {
                    cx.notify();
                }
                CoreEvent::TerminalUiCommand {
                    pane: changed,
                    command: TerminalUiCommand::BeginSearch { direction },
                } if *changed == pane => {
                    let output_pane = connection
                        .read(cx)
                        .core
                        .command_output()
                        .map(|(pane, _)| pane);
                    if this.surface != TerminalSurface::Pane || output_pane != Some(pane) {
                        this.begin_search(
                            SearchQuery {
                                direction: *direction,
                                ..SearchQuery::default()
                            },
                            true,
                            cx,
                        );
                    }
                }
                _ => {}
            },
        );
        Self {
            pane,
            connection,
            focus: cx.focus_handle(),
            bounds: Bounds::default(),
            cursor_bounds: Bounds::default(),
            surface_bounds: Bounds::default(),
            cell_width: px(8.),
            line_height: px(18.),
            floating: false,
            text_font: None,
            scale: 1.0,
            force_local_selection: false,
            available_fonts: cx.text_system().all_font_names(),
            text_opacity: 1.0,
            pane_status: (None, false),
            corner_radii: Corners::default(),
            header: None,
            rows_above: false,
            rows_below: false,
            resize_suppressed: Rc::default(),
            moving: Rc::default(),
            extra_height: Pixels::ZERO,
            content_offset: Pixels::ZERO,
            scroll_rows: 0.,
            scroll: LocalScrollState::default(),
            overscroll: ala::RubberBand::default(),
            geometry: None,
            cache: RowRenderCache::default(),
            copy_flash: CopyFlashTracker::new(),
            observed_copy_generation: None,
            painted_copy_flash: None,
            focused: false,
            cursor_blink_visible: true,
            cursor_blink_task: None,
            cursor_blink_idle_since: cx.background_executor().now(),
            cursor_blink_expired: false,
            painted: false,
            selection_dragging: false,
            scrollbar_dragging: false,
            scrollbar_fade: ScrollbarFade::default(),
            scrollbar_fade_task: None,
            pressed_buttons: HashSet::new(),
            pointer: None,
            hovered_image_uri: None,
            image_hover_ready: false,
            image_hover_task: None,
            link_hover_bounds: None,
            image_preview_press: None,
            selection_anchor: None,
            selection_pointer: None,
            selection_autoscroll_lines: 0,
            selection_autoscroll_task: None,
            focus_subscriptions: Vec::new(),
            forwarded: HashSet::new(),
            marked_text: None,
            surface: TerminalSurface::Pane,
            search: None,
            search_input: None,
            search_focus_pending: false,
            search_accept_on_enter: false,
            swallowed_search_key: None,
            compact: false,
            _subscription: subscription,
        }
    }

    #[must_use]
    pub fn with_resize_suppression(mut self, suppressed: Rc<Cell<bool>>) -> Self {
        self.resize_suppressed = suppressed;
        self
    }

    #[must_use]
    pub const fn cell_size(&self) -> (Pixels, Pixels) {
        (self.cell_width, self.line_height)
    }

    pub fn set_floating(&mut self, floating: bool, cx: &mut Context<Self>) {
        if self.floating != floating {
            self.floating = floating;
            cx.notify();
        }
    }

    pub fn set_compact(&mut self, compact: bool, cx: &mut Context<Self>) {
        if self.compact != compact {
            self.compact = compact;
            cx.notify();
        }
    }

    pub fn phone_find(&self, cx: &App) -> Option<PhoneFindState> {
        let query = self.search.as_ref()?;
        let input = self.search_input.clone()?;
        let status = self
            .viewport(&self.connection.read(cx).core)
            .and_then(|viewport| viewport.search);
        let (count, found) = zz_ui::compact::find_count(&query.text, status);
        Some(PhoneFindState {
            input,
            count,
            found,
        })
    }

    pub fn open_phone_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.search.is_none() {
            self.open_search(window, cx);
        } else if let Some(input) = &self.search_input {
            input.read(cx).focus_handle(cx).focus(window, cx);
        }
        window.request_virtual_keyboard();
    }

    pub fn close_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.search.is_some() {
            self.close_search(window, cx);
        }
    }

    pub fn find_step(&mut self, backward: bool, cx: &mut Context<Self>) {
        self.step_search(backward, cx);
    }

    pub fn scrolled_back(&self, cx: &App) -> bool {
        let core = &self.connection.read(cx).core;
        let Some(viewport) = self.viewport(core) else {
            return false;
        };
        let bar = viewport.scrollbar;
        let offset = self.scroll.target().unwrap_or(bar.offset);
        offset.saturating_add(bar.len) < bar.total
    }

    pub fn jump_to_bottom(&mut self, cx: &mut Context<Self>) {
        self.cancel_local_scroll(cx);
        self.scroll_rows = 0.;
        self.view(TerminalViewAction::ScrollBottom, cx);
        cx.notify();
    }

    pub fn new_command_output(
        pane: PaneId,
        connection: Entity<Connection>,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut this = Self::new(pane, connection, cx);
        this.surface = TerminalSurface::CommandOutput;
        this
    }

    pub fn set_text_dimmed(&mut self, dimmed: bool, opacity: f32, cx: &mut Context<Self>) {
        let opacity = if dimmed { opacity.clamp(0.0, 1.0) } else { 1.0 };
        if self.text_opacity.to_bits() != opacity.to_bits() {
            self.text_opacity = opacity;
            cx.notify();
        }
    }

    pub fn set_pane_status(
        &mut self,
        dead: Option<String>,
        synchronized: bool,
        cx: &mut Context<Self>,
    ) {
        let status = (dead, synchronized);
        if self.pane_status != status {
            self.pane_status = status;
            cx.notify();
        }
    }

    pub fn set_corner_radii(&mut self, radii: Corners<Pixels>, cx: &mut Context<Self>) {
        if self.corner_radii != radii {
            self.corner_radii = radii;
            cx.notify();
        }
    }

    #[must_use]
    pub fn text(&self) -> Option<TerminalText> {
        let (font, font_size) = self.text_font.clone()?;
        let (grid, _) = self.geometry?;
        Some(TerminalText {
            origin: self.bounds.origin,
            line_height: self.line_height,
            rows: grid.rows,
            font,
            font_size,
        })
    }

    pub fn set_header_band(&mut self, header: Option<HeaderBand>, cx: &mut Context<Self>) {
        if self.header != header {
            self.header = header;
            cx.notify();
        }
    }

    fn header_band(&self) -> Option<HeaderBand> {
        self.header.filter(|_| !self.floating)
    }

    pub fn set_rows_above(&mut self, rows_above: bool, cx: &mut Context<Self>) {
        if self.rows_above != rows_above {
            self.rows_above = rows_above;
            cx.notify();
        }
    }

    pub fn set_rows_below(&mut self, rows_below: bool, cx: &mut Context<Self>) {
        if self.rows_below != rows_below {
            self.rows_below = rows_below;
            cx.notify();
        }
    }

    pub(crate) fn rows_above_shown(&self, cx: &App) -> bool {
        self.rows_above
            && (self.overscroll.is_stretched()
                || self.scroll.target().is_some()
                || self.scroll.sub_row() > 0.0
                || self
                    .viewport(&self.connection.read(cx).core)
                    .is_some_and(|viewport| viewport.scrollbar.offset > 0))
    }

    pub fn set_extra_height(&mut self, height: Pixels, cx: &mut Context<Self>) {
        if self.extra_height != height {
            self.extra_height = height;
            cx.notify();
        }
    }

    pub(crate) fn moving(&self) -> Rc<Cell<bool>> {
        Rc::clone(&self.moving)
    }

    pub(crate) fn fill(&self, cx: &App) -> Hsla {
        let core = &self.connection.read(cx).core;
        let background = self.viewport(core).filter(|_| !crate::app::phone()).map_or(
            cx.theme().background,
            |viewport| {
                zz_ui::terminal::terminal_background(
                    shown_terminal_theme(cx).map_or(viewport.background, |theme| theme.background),
                    core.appearance()
                        .map_or(1.0, |appearance| appearance.background_opacity),
                )
            },
        );
        pane_color(cx.theme(), background)
    }

    fn observe_image_hover(&mut self, uri: Option<Arc<str>>, cx: &mut Context<Self>) {
        let uri = uri.filter(|uri| pasted_image_number(uri).is_some());
        if self.hovered_image_uri == uri {
            return;
        }
        self.hovered_image_uri.clone_from(&uri);
        self.image_hover_ready = false;
        self.image_hover_task = None;
        if let Some(uri) = uri {
            let number = pasted_image_number(&uri).unwrap();
            self.connection.update(cx, |connection, cx| {
                connection.fetch_pasted_image(self.pane, number, cx);
            });
            self.image_hover_task = Some(cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(250))
                    .await;
                let _ = this.update(cx, |this, cx| {
                    if this.hovered_image_uri.as_deref() == Some(uri.as_ref()) {
                        this.image_hover_ready = true;
                        cx.notify();
                    }
                });
            }));
        }
        cx.notify();
    }

    fn visible_image_hover(&self, cx: &App) -> Option<(u32, Bounds<Pixels>, Arc<ala::Image>)> {
        if !self.image_hover_ready {
            return None;
        }
        let bounds = self.link_hover_bounds?;
        let number = pasted_image_number(self.hovered_image_uri.as_deref()?)?;
        let image = self.connection.read(cx).pasted_image(self.pane, number)?;
        Some((number, bounds, image))
    }

    fn image_hover_popover(&self, cx: &App) -> Option<AnyElement> {
        let (_, bounds, image) = self.visible_image_hover(cx)?;
        Some(zz_ui::terminal::pasted_image_preview(bounds, image, cx))
    }

    fn viewport<'a>(&self, core: &'a zz_client::ClientCore) -> Option<&'a TerminalViewport> {
        match self.surface {
            TerminalSurface::CommandOutput => core.command_output().map(|(_, viewport)| viewport),
            TerminalSurface::Pane => core.viewport(self.pane),
        }
    }

    fn send(&self, input: InputMessage, cx: &mut Context<Self>) {
        self.connection
            .update(cx, |connection, cx| connection.input(self.pane, input, cx));
    }

    pub(crate) fn view(&self, action: TerminalViewAction, cx: &mut Context<Self>) {
        self.send(
            match self.surface {
                TerminalSurface::Pane => InputMessage::TerminalView {
                    pane: self.pane,
                    action,
                },
                TerminalSurface::CommandOutput => InputMessage::CommandOutputView { action },
            },
            cx,
        );
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.search.is_some() && !self.focus.is_focused(window) {
            return;
        }
        self.reset_cursor_blink(cx);
        let sticky;
        let event = if !penna::blocks::touch::StickyModifiers::get(cx).is_empty()
            && !matches!(
                event.keystroke.key.as_str(),
                "shift" | "control" | "alt" | "platform" | "function"
            ) {
            sticky = KeyDownEvent {
                keystroke: penna::blocks::touch::StickyModifiers::apply(&event.keystroke, cx),
                is_held: event.is_held,
                prefer_character_input: event.prefer_character_input,
            };
            window.refresh();
            &sticky
        } else {
            event
        };
        let input = key_input(event);
        if input.key == KeyCode::Unidentified {
            return;
        }
        if self.swallowed_search_key == Some(input.key) {
            cx.stop_propagation();
            return;
        }
        let (search_shortcut, shortcut) = {
            let fallback;
            let chrome = if let Some(chrome) = cx.try_global::<ChromeKeymaps>() {
                chrome
            } else {
                fallback = ChromeKeymaps::default();
                &fallback
            };
            let core = &self.connection.read(cx).core;
            (
                chrome
                    .native
                    .resolve(TERMINAL_TABLE, &input)
                    .or_else(|| chrome.apple.resolve(TERMINAL_TABLE, &input)),
                terminal_shortcut(
                    [&chrome.native, &chrome.apple],
                    core.claims_prefix_input(&input),
                    core.modal_capture(),
                    &input,
                ),
            )
        };
        if self.search.is_some() {
            if search_shortcut == Some(ChromeAction::TerminalSearch) {
                self.open_search(window, cx);
            } else if let Some(input) = &self.search_input {
                input.read(cx).focus_handle(cx).focus(window, cx);
            }
            cx.stop_propagation();
            return;
        }
        match shortcut {
            Some(
                action @ (ChromeAction::TerminalFontIncrease | ChromeAction::TerminalFontDecrease),
            ) => {
                cx.emit(TerminalFontStep(
                    if action == ChromeAction::TerminalFontIncrease {
                        TERMINAL_FONT_STEP
                    } else {
                        -TERMINAL_FONT_STEP
                    },
                ));
                cx.stop_propagation();
                return;
            }
            Some(ChromeAction::TerminalCopy) => {
                self.copy_selection(zz_terminal::ClipboardTarget::Clipboard, cx);
                cx.stop_propagation();
                return;
            }
            Some(ChromeAction::TerminalSelectAll) => {
                self.view(TerminalViewAction::SelectAll, cx);
                cx.stop_propagation();
                return;
            }
            Some(ChromeAction::TerminalClearHistory) => {
                self.view(TerminalViewAction::ClearHistory, cx);
                cx.stop_propagation();
                return;
            }
            Some(ChromeAction::TerminalPaste) => {
                #[cfg(any(target_os = "ios", target_os = "android"))]
                {
                    ala_platform::mobile::request_paste();
                    cx.stop_propagation();
                }
                return;
            }
            Some(ChromeAction::TerminalSearch) => {
                self.open_search(window, cx);
                cx.stop_propagation();
                return;
            }
            _ => {}
        }
        #[cfg(any(target_os = "ios", target_os = "android"))]
        if self.surface == TerminalSurface::Pane
            && matches!(input.key, KeyCode::Character('v' | 'V'))
            && input.modifiers.control()
            && !input.modifiers.shift()
            && !input.modifiers.alt()
            && !input.modifiers.platform()
            && let Some(image) = ala_platform::mobile::clipboard_image()
        {
            self.cancel_local_scroll(cx);
            self.connection.update(cx, |connection, cx| {
                connection.upload_image(self.pane, &image, cx);
            });
            cx.stop_propagation();
            return;
        }
        let raw = cfg!(any(target_os = "ios", target_os = "android"))
            || self
                .viewport(&self.connection.read(cx).core)
                .is_some_and(|viewport| viewport.kitty_keyboard)
            || !matches!(input.key, KeyCode::Character(_))
            || input.modifiers.control()
            || input.modifiers.alt()
            || input.modifiers.platform();
        if raw {
            self.cancel_local_scroll(cx);
            self.forwarded.insert(event.keystroke.key.clone());
            self.send(
                InputMessage::Key {
                    pane: self.pane,
                    input,
                    text_follows: false,
                },
                cx,
            );
            cx.stop_propagation();
        }
    }

    fn copy_selection(&self, target: zz_terminal::ClipboardTarget, cx: &mut Context<Self>) {
        self.view(
            TerminalViewAction::CopySelection {
                request_id: 1,
                target,
            },
            cx,
        );
    }

    pub(crate) fn open_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.begin_search(SearchQuery::default(), false, cx);
        self.sync_search_input(window, cx);
    }

    fn begin_search(&mut self, query: SearchQuery, accept_on_enter: bool, cx: &mut Context<Self>) {
        self.cancel_local_scroll(cx);
        self.search = Some(query.clone());
        self.search_focus_pending = true;
        self.search_accept_on_enter = accept_on_enter;
        self.marked_text = None;
        self.view(TerminalViewAction::SearchBegin(query), cx);
        cx.notify();
    }

    fn close_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search = None;
        self.search_focus_pending = false;
        self.marked_text = None;
        self.view(TerminalViewAction::SearchClose, cx);
        window.focus(&self.focus, cx);
        cx.notify();
    }

    fn sync_search_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.search.is_none() {
            return;
        }
        if self.search_input.is_none() {
            let input = cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder(if self.compact {
                        "Find"
                    } else {
                        "Find in this pane…"
                    })
                    .context_menu(true)
            });
            cx.subscribe_in(
                &input,
                window,
                |view, input, event: &InputEvent, window, cx| match event {
                    InputEvent::Change => {
                        if let Some(query) = view.search.as_mut() {
                            query.text = input.read(cx).value().to_string();
                            let query = query.clone();
                            view.cancel_local_scroll(cx);
                            view.view(TerminalViewAction::SearchUpdate(query), cx);
                            cx.notify();
                        }
                    }
                    InputEvent::Focus if view.surface == TerminalSurface::Pane => {
                        view.connection.update(cx, |connection, cx| {
                            connection.command(
                                "select-pane",
                                vec!["-Z".into(), "-t".into(), view.pane.to_string()],
                                cx,
                            );
                        });
                    }
                    InputEvent::PressEnter { shift } if view.search.is_some() => {
                        if view.search_accept_on_enter {
                            view.search = None;
                            view.search_focus_pending = false;
                            view.marked_text = None;
                            view.focus.focus(window, cx);
                            view.swallowed_search_key = Some(KeyCode::Enter);
                            cx.notify();
                        } else {
                            let backward =
                                view.search.as_ref().is_some_and(|query| {
                                    query.direction == SearchDirection::Backward
                                }) ^ shift;
                            view.step_search(backward, cx);
                        }
                    }
                    _ => {}
                },
            )
            .detach();
            self.search_input = Some(input);
        }
        if self.search_focus_pending {
            self.search_focus_pending = false;
            let input = self.search_input.as_ref().unwrap();
            let query = self.search.as_ref().unwrap();
            input.update(cx, |input, cx| {
                input.set_value(query.text.clone(), window, cx);
            });
            input.read(cx).focus_handle(cx).focus(window, cx);
        }
    }

    fn step_search(&mut self, backward: bool, cx: &mut Context<Self>) {
        self.cancel_local_scroll(cx);
        self.view(
            if backward {
                TerminalViewAction::SearchPrevious
            } else {
                TerminalViewAction::SearchNext
            },
            cx,
        );
    }

    fn search_bar(&self, status: Option<SearchStatus>, cx: &mut Context<Self>) -> AnyElement {
        let query = self.search.as_ref().unwrap();
        let view = cx.entity();
        zz_ui::terminal::terminal_find_bar(
            self.search_input.as_ref().unwrap(),
            query,
            status,
            move |action, window, cx| {
                view.update(cx, |view, cx| match action {
                    FindBarAction::ToggleRegex | FindBarAction::ToggleCase => {
                        if let Some(query) = view.search.as_mut() {
                            if action == FindBarAction::ToggleRegex {
                                query.mode = match query.mode {
                                    SearchMode::Literal => SearchMode::Regex,
                                    SearchMode::Regex => SearchMode::Literal,
                                };
                            } else {
                                query.case = match query.case {
                                    SearchCase::Sensitive => SearchCase::Insensitive,
                                    _ => SearchCase::Sensitive,
                                };
                            }
                            let query = query.clone();
                            view.view(TerminalViewAction::SearchUpdate(query), cx);
                            cx.notify();
                        }
                    }
                    FindBarAction::Previous => view.step_search(true, cx),
                    FindBarAction::Next => view.step_search(false, cx),
                    FindBarAction::Close => view.close_search(window, cx),
                });
            },
            cx,
        )
        .capture_key_down(cx.listener(|view, event: &KeyDownEvent, _, cx| {
            if let Some(query) = view.search.as_mut()
                && let Some(action) = search_key_action(query, &key_input(event))
            {
                view.view(action, cx);
                cx.notify();
                cx.stop_propagation();
            }
        }))
        .on_action(
            cx.listener(|view, _: &penna::components::input::Escape, window, cx| {
                view.close_search(window, cx);
                view.swallowed_search_key = Some(KeyCode::Escape);
                cx.stop_propagation();
            }),
        )
        .on_action(|_: &penna::components::input::Enter, _, cx| cx.stop_propagation())
        .into_any_element()
    }

    fn on_key_up(&mut self, event: &KeyUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.swallowed_search_key == Some(key_code(&event.keystroke.key)) {
            self.swallowed_search_key = None;
            cx.stop_propagation();
            return;
        }
        if self.search.is_some() {
            self.forwarded.remove(&event.keystroke.key);
            cx.stop_propagation();
            return;
        }
        if self.forwarded.remove(&event.keystroke.key) {
            self.send(
                InputMessage::Key {
                    pane: self.pane,
                    input: keystroke_input(&event.keystroke, KeyAction::Release),
                    text_follows: false,
                },
                cx,
            );
            cx.stop_propagation();
        }
    }

    fn mouse(
        &self,
        position: Point<Pixels>,
        modifiers: ala::Modifiers,
        phase: TerminalMousePhase,
        button: Option<TerminalMouseButton>,
        count: usize,
    ) -> TerminalMouseInput {
        PointerGrid {
            left: f32::from(self.bounds.origin.x),
            top: f32::from(self.bounds.origin.y),
            width: f32::from(self.bounds.size.width),
            height: f32::from(self.bounds.size.height),
            cell_width: f32::from(self.cell_width),
            line_height: f32::from(self.line_height),
            content_offset: f32::from(self.content_offset),
            scale: self.scale,
            columns: self.geometry.map_or(0, |(grid, _)| grid.columns),
            rows: self.geometry.map_or(0, |(grid, _)| grid.rows),
        }
        .input(
            f32::from(position.x),
            f32::from(position.y),
            phase,
            button,
            count,
            wire_modifiers(modifiers),
            modifiers.shift || self.force_local_selection,
        )
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus, cx);
        self.sync_focus(window, cx);
        self.reset_cursor_blink(cx);
        self.pointer = Some(event.position);
        if self.surface == TerminalSurface::Pane {
            self.connection.update(cx, |connection, cx| {
                connection.command(
                    "select-pane",
                    vec!["-Z".into(), "-t".into(), self.pane.to_string()],
                    cx,
                );
            });
        }
        if event.button == MouseButton::Left && self.scrollbar_hit(event.position, cx) {
            self.end_drag();
            self.scrollbar_dragging = true;
            self.view(TerminalViewAction::ClearLinkHover, cx);
            self.scroll_to_pointer(event.position, cx);
            cx.stop_propagation();
            return;
        }
        if !self.bounds.contains(&event.position) {
            return;
        }
        if event.button == MouseButton::Left {
            window.request_virtual_keyboard();
            self.force_local_selection =
                event.click_count >= 3 && (event.modifiers.control || event.modifiers.platform);
            let tracking = self
                .viewport(&self.connection.read(cx).core)
                .is_some_and(|viewport| viewport.mouse_tracking);
            self.selection_dragging =
                !tracking || event.modifiers.shift || self.force_local_selection;
        }
        self.pressed_buttons.insert(event.button);
        let link_tap = (self.compact
            || (cfg!(any(target_os = "ios", target_os = "android"))
                && window.last_input_was_touch()))
            && self.surface == TerminalSurface::Pane
            && event.button == MouseButton::Left
            && event.click_count == 1
            && event.modifiers == ala::Modifiers::default()
            && !self
                .viewport(&self.connection.read(cx).core)
                .is_some_and(|viewport| viewport.mouse_tracking);
        if link_tap {
            let pane = self.pane;
            self.connection
                .update(cx, |connection, _| connection.expect_link_tap(pane));
        }
        self.image_preview_press = (!link_tap
            && event.button == MouseButton::Left
            && event.click_count == 1
            && event.modifiers == ala::Modifiers::default())
        .then(|| self.visible_image_hover(cx))
        .flatten()
        .filter(|(_, bounds, _)| bounds.contains(&event.position))
        .map(|(number, bounds, _)| (number, bounds));
        let input = self.mouse(
            event.position,
            if link_tap {
                ala::Modifiers::control()
            } else {
                event.modifiers
            },
            TerminalMousePhase::Press,
            mouse_button(event.button),
            event.click_count,
        );
        self.selection_pointer = self.selection_dragging.then_some(input.cell);
        self.selection_anchor = self.selection_pointer;
        self.flush_local_scroll(cx);
        self.view(TerminalViewAction::Mouse(input), cx);
        cx.stop_propagation();
    }

    fn on_mouse_up(&mut self, event: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        let image_press = (event.button == MouseButton::Left)
            .then(|| self.image_preview_press.take())
            .flatten();
        let dragged = self
            .selection_anchor
            .zip(self.selection_pointer)
            .is_some_and(|(anchor, pointer)| selection_has_extent(anchor, pointer));
        self.release_pointer(event, !self.compact, cx);
        if let Some((number, bounds)) = image_press
            && !dragged
            && event.click_count == 1
            && event.modifiers == ala::Modifiers::default()
            && bounds.contains(&event.position)
        {
            let pane = self.pane;
            self.connection.update(cx, |connection, cx| {
                connection.preview_pasted_image(pane, number, cx);
            });
        }
    }

    fn release_pointer(
        &mut self,
        event: &MouseUpEvent,
        copy_on_select: bool,
        cx: &mut Context<Self>,
    ) {
        self.reset_cursor_blink(cx);
        if self.scrollbar_dragging && event.button == MouseButton::Left {
            self.scroll_to_pointer(event.position, cx);
            self.end_drag();
            cx.stop_propagation();
            return;
        }
        if self.pressed_buttons.remove(&event.button) {
            let input = self.mouse(
                event.position,
                event.modifiers,
                TerminalMousePhase::Release,
                mouse_button(event.button),
                event.click_count,
            );
            let copies = copy_on_select
                && event.button == MouseButton::Left
                && self.selection_dragging
                && (self.selection_autoscroll_lines != 0
                    || self
                        .selection_anchor
                        .zip(self.selection_pointer)
                        .is_some_and(|(anchor, pointer)| selection_has_extent(anchor, pointer)));
            self.view(TerminalViewAction::Mouse(input), cx);
            if copies {
                self.copy_selection(zz_terminal::ClipboardTarget::Primary, cx);
            }
            if event.button == MouseButton::Left {
                self.end_drag();
            }
            cx.stop_propagation();
        }
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.pointer = Some(event.position);
        if event.pressed_button.is_none() {
            self.end_drag();
            self.pressed_buttons.clear();
        }
        if self.scrollbar_dragging {
            self.scroll_to_pointer(event.position, cx);
            cx.stop_propagation();
            return;
        }
        if event.pressed_button.is_none() && self.scrollbar_hit(event.position, cx) {
            self.show_scrollbar(cx);
        }
        if event
            .pressed_button
            .is_some_and(|button| !self.pressed_buttons.contains(&button))
        {
            return;
        }
        if !self.selection_dragging && !self.bounds.contains(&event.position) {
            self.view(TerminalViewAction::ClearLinkHover, cx);
            return;
        }
        let input = self.mouse(
            event.position,
            event.modifiers,
            TerminalMousePhase::Motion,
            event.pressed_button.and_then(mouse_button),
            1,
        );
        if self.selection_dragging {
            self.selection_pointer = Some(input.cell);
            self.selection_autoscroll_lines =
                selection_autoscroll_lines(self.bounds, self.line_height, event.position);
            self.ensure_selection_autoscroll(cx);
        }
        self.view(TerminalViewAction::Mouse(input), cx);
        if event.pressed_button.is_some() {
            cx.stop_propagation();
        }
    }

    fn on_mouse_exit(&mut self, _: &MouseExitEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.pointer = None;
        self.observe_image_hover(None, cx);
        self.view(TerminalViewAction::ClearLinkHover, cx);
    }

    fn on_modifiers_changed(
        &mut self,
        event: &ModifiersChangedEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(position) = self
            .pointer
            .filter(|position| self.bounds.contains(position))
        {
            self.view(
                TerminalViewAction::Mouse(self.mouse(
                    position,
                    event.modifiers,
                    TerminalMousePhase::Motion,
                    None,
                    1,
                )),
                cx,
            );
        }
    }

    fn overscroll(
        &mut self,
        delta: Pixels,
        phase: ala::TouchPhase,
        local: bool,
        window: &Window,
        cx: &App,
    ) -> Pixels {
        if window.gesture_tuning().overscroll != ala::Overscroll::Bounce {
            return delta;
        }
        let core = &self.connection.read(cx).core;
        let Some(viewport) = self.viewport(core) else {
            return delta;
        };
        let bar = viewport.scrollbar;
        if viewport.mouse_tracking
            || bar.total <= bar.len
            || matches!(viewport.mode, TerminalMode::Copy { .. })
        {
            return delta;
        }
        let (forward, back) = match core.retained_viewport(self.pane).filter(|_| local) {
            Some(retained) => {
                let (newer, older) = self
                    .scroll
                    .overscroll_room(retained, f32::from(self.line_height));
                (px(-newer), px(older))
            }
            None => (
                if bar.offset + bar.len < bar.total {
                    px(f32::MIN)
                } else {
                    Pixels::ZERO
                },
                if bar.offset > 0 {
                    px(f32::MAX)
                } else {
                    Pixels::ZERO
                },
            ),
        };
        self.overscroll.scroll(
            Pixels::ZERO,
            delta,
            forward,
            back,
            self.surface_bounds.size.height,
            phase,
        )
    }

    fn show_scrollbar(&mut self, cx: &mut Context<Self>) {
        if self
            .viewport(&self.connection.read(cx).core)
            .is_some_and(|viewport| viewport.scrollbar.total > viewport.scrollbar.len)
        {
            self.scrollbar_fade.show(web_time::Instant::now());
            cx.notify();
        }
    }

    fn scrollbar_opacity(&mut self, cx: &mut Context<Self>) -> f32 {
        let Some(mut scrollbar) = self
            .viewport(&self.connection.read(cx).core)
            .map(|viewport| viewport.scrollbar)
        else {
            return 0.0;
        };
        if let Some(offset) = self.scroll.target() {
            scrollbar.offset = offset;
        }
        let now = web_time::Instant::now();
        if self.scrollbar_dragging {
            self.scrollbar_fade.show(now);
        }
        let frame = self
            .scrollbar_fade
            .frame(scrollbar, now, cx.reduce_motion());
        self.scrollbar_fade_task = frame.map(|(_, wake)| {
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(wake).await;
                let _ = this.update(cx, |_, cx| cx.notify());
            })
        });
        frame.map_or(0.0, |(opacity, _)| opacity)
    }

    fn scrollbar_hit(&self, position: Point<Pixels>, cx: &App) -> bool {
        self.viewport(&self.connection.read(cx).core)
            .is_some_and(|viewport| {
                zz_ui::terminal::scrollbar_strip_hit(
                    self.surface_bounds,
                    viewport.scrollbar,
                    position,
                    penna::components::scroll::GUTTER_WIDTH.to_pixels(cx.theme().font_size),
                )
            })
    }

    fn scroll_to_pointer(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let fraction = zz_ui::terminal::scroll_fraction(self.surface_bounds, position);
        if self.scroll.clear_sub_row() {
            cx.notify();
        }
        let maximum = self
            .local_retained(cx)
            .map(|retained| retained.viewport.scrollbar)
            .map(|bar| bar.total.saturating_sub(bar.len));
        match maximum {
            Some(maximum) => self.scroll_locally_to(scroll_fraction_offset(fraction, maximum), cx),
            None => self.view(TerminalViewAction::ScrollToFraction(fraction), cx),
        }
    }

    fn end_drag(&mut self) {
        self.selection_dragging = false;
        self.scrollbar_dragging = false;
        self.force_local_selection = false;
        self.selection_anchor = None;
        self.selection_pointer = None;
        self.selection_autoscroll_lines = 0;
        self.selection_autoscroll_task = None;
    }

    fn ensure_selection_autoscroll(&mut self, cx: &mut Context<Self>) {
        if self.selection_autoscroll_lines == 0 || !self.selection_dragging {
            self.selection_autoscroll_task = None;
            return;
        }
        if self.selection_autoscroll_task.is_some() {
            return;
        }
        self.selection_autoscroll_task = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(33))
                    .await;
                let Ok(keep_running) = this.update(cx, |view, cx| {
                    let Some(pointer) = view.selection_pointer else {
                        return false;
                    };
                    if !view.selection_dragging || view.selection_autoscroll_lines == 0 {
                        return false;
                    }
                    view.view(
                        TerminalViewAction::SelectionAutoscroll {
                            lines: view.selection_autoscroll_lines,
                            pointer,
                        },
                        cx,
                    );
                    true
                }) else {
                    break;
                };
                if !keep_running {
                    break;
                }
            }
        }));
    }

    fn local_retained<'a>(&self, cx: &'a App) -> Option<&'a RetainedTerminalViewport> {
        if self.surface != TerminalSurface::Pane {
            return None;
        }
        self.connection
            .read(cx)
            .core
            .retained_viewport(self.pane)
            .filter(|retained| local_scroll_available(retained))
    }

    fn cancel_local_scroll(&mut self, cx: &mut Context<Self>) {
        if self.scroll.cancel() {
            cx.notify();
        }
    }

    fn flush_local_scroll(&mut self, cx: &mut Context<Self>) {
        if let Some(target) = self.scroll.flush() {
            self.view(TerminalViewAction::ScrollToOffset(target), cx);
        }
    }

    fn scroll_by_pixels(&mut self, delta: Pixels, cx: &mut Context<Self>) -> bool {
        let Some(retained) = self.local_retained(cx) else {
            return false;
        };
        let Some(step) = self.scroll.scroll_by_pixels(
            f32::from(delta),
            f32::from(self.line_height),
            retained,
            web_time::Instant::now(),
        ) else {
            return false;
        };
        self.apply_local_scroll(step, cx);
        true
    }

    fn scroll_locally_by(&mut self, delta: i64, cx: &mut Context<Self>) {
        let Some(retained) = self.local_retained(cx) else {
            return;
        };
        let step = self
            .scroll
            .scroll_by_rows(delta, retained, web_time::Instant::now());
        self.apply_local_scroll(step, cx);
    }

    fn scroll_locally_to(&mut self, target: u32, cx: &mut Context<Self>) {
        let Some(retained) = self.local_retained(cx) else {
            return;
        };
        let step = self
            .scroll
            .scroll_to(target, retained, web_time::Instant::now());
        self.apply_local_scroll(step, cx);
    }

    fn apply_local_scroll(&mut self, step: LocalScrollStep, cx: &mut Context<Self>) {
        for effect in step.effects {
            match effect {
                LocalScrollEffect::ScrollToOffset(offset) => {
                    self.view(TerminalViewAction::ScrollToOffset(offset), cx);
                }
                LocalScrollEffect::Prefetch(target) => {
                    let pane = self.pane;
                    self.connection.update(cx, |connection, cx| {
                        connection.request_history_prefetch(pane, target, cx);
                    });
                }
                LocalScrollEffect::Sync(generation) => {
                    self.schedule_local_scroll_sync(generation, cx);
                }
            }
        }
        if step.redraw {
            cx.notify();
        }
    }

    fn schedule_local_scroll_sync(&self, generation: u64, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(LOCAL_SCROLL_DEBOUNCE).await;
            let Ok(active) = this.update(cx, |view, cx| {
                let sync = match view.connection.read(cx).core.retained_viewport(view.pane) {
                    Some(retained) => view.scroll.sync(generation, retained),
                    None => view.scroll.clear().then_some(LocalScrollSync::Retired),
                };
                match sync {
                    None => false,
                    Some(LocalScrollSync::Retired) => {
                        cx.notify();
                        false
                    }
                    Some(LocalScrollSync::Requested(target)) => {
                        view.view(TerminalViewAction::ScrollToOffset(target), cx);
                        true
                    }
                }
            }) else {
                return;
            };
            if !active {
                return;
            }
            cx.background_executor()
                .timer(LOCAL_SCROLL_TIMEOUT.saturating_sub(LOCAL_SCROLL_DEBOUNCE))
                .await;
            let _ = this.update(cx, |view, cx| {
                if view.scroll.expire(generation, web_time::Instant::now()) {
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn on_scroll(&mut self, event: &ScrollWheelEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.show_scrollbar(cx);
        let delta = event.delta.pixel_delta(self.line_height);
        let pixels = matches!(event.delta, ScrollDelta::Pixels(_));
        let local = pixels && self.local_retained(cx).is_some();
        let stretched = self.overscroll.is_stretched();
        let moved = delta.y != Pixels::ZERO;
        let delta = self.overscroll(delta.y, event.touch_phase, local, window, cx);
        if stretched || self.overscroll.is_stretched() {
            cx.notify();
            cx.stop_propagation();
        }
        if pixels && delta == Pixels::ZERO {
            if moved {
                cx.stop_propagation();
            }
            return;
        }
        if local && self.scroll_by_pixels(delta, cx) {
            self.scroll_rows = 0.;
            cx.stop_propagation();
            return;
        }
        if self.scroll.clear_sub_row() {
            cx.notify();
        }
        let lines = accumulate_scroll(
            &mut self.scroll_rows,
            -f32::from(delta) / f32::from(self.line_height),
        );
        if lines == 0 {
            return;
        }
        if lines > 0 && self.local_retained(cx).is_some() {
            self.scroll_locally_by(i64::from(lines), cx);
        } else {
            if lines < 0
                && let Some(target) = self.scroll.release()
            {
                self.view(TerminalViewAction::ScrollToOffset(target), cx);
                cx.notify();
            }
            self.view(
                TerminalViewAction::ScrollWheel {
                    lines,
                    input: self.mouse(
                        event.position,
                        event.modifiers,
                        TerminalMousePhase::Press,
                        Some(if lines < 0 {
                            TerminalMouseButton::ScrollUp
                        } else {
                            TerminalMouseButton::ScrollDown
                        }),
                        1,
                    ),
                },
                cx,
            );
        }
        cx.stop_propagation();
    }

    fn prepare(
        &mut self,
        bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<PaintState> {
        let displacement = self.overscroll.displacement(bounds.size.height);
        if self.overscroll.is_animating() {
            window.request_animation_frame();
        }
        let rows_above = self
            .rows_above_shown(cx)
            .then(|| window.content_mask().bounds.origin.y);
        let rows_below = self
            .rows_below
            .then(|| window.content_mask().bounds.bottom());
        let header = self
            .header_band()
            .map(|band| band.scrim(window.content_mask().bounds));
        let scrollbar_opacity = self.scrollbar_opacity(cx);
        let connection = self.connection.clone();
        let flash_copies = !self.compact && !cx.reduce_motion();
        let (paint, attached, layout_generation) = connection.update(cx, |connection, cx| {
            for image in connection.take_retired_terminal_images() {
                let _ = window.drop_image(image);
            }
            let retained = match self.surface {
                TerminalSurface::CommandOutput => connection
                    .core
                    .retained_command_output()
                    .map(|(_, retained)| retained),
                TerminalSurface::Pane => connection.core.retained_viewport(self.pane),
            }?;
            let mut appearance = localized_font_appearance(
                &connection.core,
                &self.available_fonts,
                &cx.theme().mono_font_family,
                cx,
            );
            if let Some(local) = phone_colors(&retained.viewport, &appearance.palette, cx) {
                appearance.local_colors = Some(local);
            }
            let mut themed = appearance.local_colors.map(|local| {
                let mut viewport = local.viewport(&retained.viewport);
                if let Some(theme) = shown_terminal_theme(cx) {
                    viewport.cursor = viewport
                        .cursor
                        .map(|cursor| cursor.with_color(theme.cursor));
                }
                viewport
            });
            if let Some(cursor) = connection.core.appearance().and_then(|host| {
                device_cursor(
                    themed
                        .as_ref()
                        .map_or(retained.viewport.cursor, |viewport| viewport.cursor),
                    host.cursor_style,
                    appearance.cursor_style,
                )
            }) {
                themed
                    .get_or_insert_with(|| retained.viewport.clone())
                    .cursor = Some(cursor);
            }
            let paint = self.cache.prepaint(
                TerminalRenderInput {
                    viewport: themed.as_ref().unwrap_or(&retained.viewport),
                    row_revisions: &retained.row_revisions,
                    revision_epoch: retained.row_revision_epoch,
                    history: Some(retained),
                    images: if self.surface == TerminalSurface::Pane {
                        connection
                            .terminal_images(self.pane)
                            .map(|images| images as &dyn zz_ui::terminal::TerminalImageSource)
                    } else {
                        None
                    },
                    local_scroll_target: self.scroll.target(),
                    scroll_pixel_offset: px(self.scroll.sub_row()),
                    overscroll: displacement,
                    scrollbar_opacity,
                    extra_height: self.extra_height,
                    command_output: self.surface == TerminalSurface::CommandOutput,
                    appearance: &appearance,
                    appearance_hash: appearance.stable_hash(),
                    text_opacity: self.text_opacity,
                    focused: self.focused,
                    cursor_blink_visible: self.cursor_blink_visible,
                    marked_text: self
                        .search
                        .is_none()
                        .then_some(self.marked_text.as_deref())
                        .flatten(),
                    rows_above,
                    rows_below,
                    header,
                },
                bounds,
                window,
                cx,
            );
            let copies = match self.surface {
                TerminalSurface::Pane => connection.core.copy_generation(self.pane),
                TerminalSurface::CommandOutput => 0,
            };
            if self
                .observed_copy_generation
                .replace(copies)
                .is_some_and(|seen| seen != copies)
                && flash_copies
            {
                self.copy_flash.start();
            }
            self.copy_flash.record(paint.selection_bounds());
            self.painted_copy_flash = self
                .copy_flash
                .live()
                .zip(flash_copies.then_some(appearance));
            Some((
                paint,
                connection.core.attached_session().is_some(),
                connection.core.layout_generation(),
            ))
        })?;
        let geometry = paint.geometry;
        self.link_hover_bounds = geometry.link_hover_bounds;
        self.bounds = geometry.grid_bounds;
        self.surface_bounds = geometry.surface_bounds;
        self.cell_width = geometry.cell_width;
        self.line_height = geometry.line_height;
        self.content_offset = geometry.content_offset;
        self.scale = window.scale_factor();
        let input_bounds = paint.geometry.input_bounds.unwrap_or(geometry.grid_bounds);
        if self.search.is_none() && self.cursor_bounds != input_bounds {
            self.cursor_bounds = input_bounds;
            window.invalidate_character_coordinates();
        }
        let measured = geometry.grid;
        let reported = (
            measured,
            if self.surface == TerminalSurface::Pane {
                layout_generation
            } else {
                0
            },
        );
        let measurable =
            bounds.size.width >= geometry.cell_width && bounds.size.height >= geometry.line_height;
        if measurable
            && self.geometry != Some(reported)
            && attached
            && (self.surface != TerminalSurface::Pane
                || !(self.resize_suppressed.get() || self.moving.get()))
        {
            self.geometry = Some(reported);
            match self.surface {
                TerminalSurface::Pane => self.send(
                    InputMessage::ResizeTerminalV2 {
                        pane: self.pane,
                        columns: measured.columns,
                        rows: measured.rows,
                        cell_width_px: measured.cell_width_px,
                        cell_height_px: measured.cell_height_px,
                        layout_generation: reported.1,
                    },
                    cx,
                ),
                TerminalSurface::CommandOutput => self.send(
                    InputMessage::ResizeCommandOutput {
                        columns: measured.columns,
                        rows: measured.rows,
                        cell_width_px: measured.cell_width_px,
                        cell_height_px: measured.cell_height_px,
                    },
                    cx,
                ),
            }
        }
        Some(paint)
    }

    fn paint(
        &mut self,
        bounds: Bounds<Pixels>,
        paint: &mut Option<PaintState>,
        hitbox: &ala::Hitbox,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.handle_input(
            &self.focus,
            ElementInputHandler::new(bounds, cx.entity()),
            cx,
        );
        if let Some(paint) = paint {
            let flash = self.painted_copy_flash.take();
            paint_copy_flash(
                flash
                    .as_ref()
                    .map(|(flash, appearance)| (flash, appearance)),
                bounds,
                ala::size(self.cell_width, self.line_height),
                window,
                |window| self.cache.paint(paint, bounds, window, cx),
            );
        }
        {
            let view = cx.entity().downgrade();
            let press_hitbox = hitbox.clone();
            window.on_mouse_event(move |event: &ala::LongPressEvent, phase, window, cx| {
                if !phase.bubble() {
                    return;
                }
                let _ = view.update(cx, |this, cx| {
                    let entity = cx.entity();
                    if event.phase == ala::TouchPhase::Started {
                        if window.default_prevented()
                            || !press_hitbox.is_hovered_at(event.start_position, window)
                        {
                            return;
                        }
                        window.capture_long_press(&entity);
                        window.prevent_default();
                    } else if !window.has_long_press_capture(&entity) {
                        return;
                    }
                    this.touch_selection(event.phase, event.position, window, cx);
                    cx.stop_propagation();
                });
            });
            let view = cx.entity().downgrade();
            let drag_hitbox = hitbox.clone();
            window.on_mouse_event(move |event: &ala::TouchDragEvent, phase, window, cx| {
                if !phase.bubble() {
                    return;
                }
                let _ = view.update(cx, |this, cx| {
                    if event.phase == ala::TouchPhase::Started {
                        let tracking = this
                            .viewport(&this.connection.read(cx).core)
                            .is_some_and(|viewport| viewport.mouse_tracking);
                        if window.default_prevented()
                            || !drag_hitbox.is_hovered_at(event.start_position, window)
                            || (!tracking && !this.scrollbar_hit(event.start_position, cx))
                        {
                            return;
                        }
                        this.on_mouse_down(
                            &MouseDownEvent {
                                button: MouseButton::Left,
                                position: event.start_position,
                                modifiers: ala::Modifiers::default(),
                                click_count: 1,
                                first_mouse: false,
                            },
                            window,
                            cx,
                        );
                        window.prevent_default();
                    } else if this.scrollbar_dragging
                        || this.pressed_buttons.contains(&MouseButton::Left)
                    {
                        match event.phase {
                            ala::TouchPhase::Moved => this.on_mouse_move(
                                &MouseMoveEvent {
                                    position: event.position,
                                    pressed_button: Some(MouseButton::Left),
                                    modifiers: ala::Modifiers::default(),
                                },
                                window,
                                cx,
                            ),
                            ala::TouchPhase::Ended | ala::TouchPhase::Cancelled => this
                                .on_mouse_up(
                                    &MouseUpEvent {
                                        button: MouseButton::Left,
                                        position: event.position,
                                        modifiers: ala::Modifiers::default(),
                                        click_count: 1,
                                    },
                                    window,
                                    cx,
                                ),
                            ala::TouchPhase::Started => {}
                        }
                    } else {
                        return;
                    }
                    cx.stop_propagation();
                });
            });
        }
    }

    fn touch_selection(
        &mut self,
        phase: ala::TouchPhase,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match phase {
            ala::TouchPhase::Started => {
                self.on_mouse_down(
                    &MouseDownEvent {
                        button: MouseButton::Left,
                        position,
                        modifiers: ala::Modifiers {
                            shift: true,
                            ..Default::default()
                        },
                        click_count: 2,
                        first_mouse: false,
                    },
                    window,
                    cx,
                );
                self.force_local_selection = true;
            }
            ala::TouchPhase::Moved => self.on_mouse_move(
                &MouseMoveEvent {
                    position,
                    pressed_button: Some(MouseButton::Left),
                    modifiers: ala::Modifiers::default(),
                },
                window,
                cx,
            ),
            ala::TouchPhase::Ended | ala::TouchPhase::Cancelled => {
                self.release_pointer(
                    &MouseUpEvent {
                        button: MouseButton::Left,
                        position,
                        modifiers: ala::Modifiers::default(),
                        click_count: 1,
                    },
                    false,
                    cx,
                );
                #[cfg(any(target_os = "ios", target_os = "android"))]
                if phase == ala::TouchPhase::Ended {
                    ala_platform::mobile::show_edit_menu(
                        f32::from(position.x) * window.zoom(),
                        f32::from(position.y) * window.zoom(),
                    );
                }
            }
        }
    }

    fn sync_focus(&mut self, window: &Window, cx: &mut Context<Self>) {
        let focused = self.focus.is_focused(window) && window.is_window_active();
        if self.focused != focused {
            self.focused = focused;
            self.view(TerminalViewAction::Focus(focused), cx);
            if !focused {
                self.observe_image_hover(None, cx);
                self.end_drag();
                self.pressed_buttons.clear();
                #[cfg(any(target_os = "ios", target_os = "android"))]
                for key in self.forwarded.drain().collect::<Vec<_>>() {
                    self.send(
                        InputMessage::Key {
                            pane: self.pane,
                            input: keystroke_input(
                                &Keystroke {
                                    key,
                                    modifiers: ala::Modifiers::default(),
                                    key_char: None,
                                },
                                KeyAction::Release,
                            ),
                            text_follows: false,
                        },
                        cx,
                    );
                }
                self.forwarded.clear();
                self.swallowed_search_key = None;
                self.view(TerminalViewAction::ClearLinkHover, cx);
            }
            self.reset_cursor_blink(cx);
            cx.notify();
        }
        self.ensure_cursor_blink(cx);
    }

    fn should_blink(&self, cx: &App) -> bool {
        let core = &self.connection.read(cx).core;
        self.viewport(core).is_some_and(|viewport| {
            cursor_should_blink(viewport.cursor, cursor_blink_policy(core, cx), self.focused)
        })
    }

    fn reset_cursor_blink(&mut self, cx: &mut Context<Self>) {
        let hidden = !self.cursor_blink_visible;
        self.cursor_blink_visible = true;
        self.cursor_blink_task = None;
        self.cursor_blink_idle_since = cx.background_executor().now();
        self.cursor_blink_expired = false;
        self.ensure_cursor_blink(cx);
        if hidden {
            cx.notify();
        }
    }

    fn ensure_cursor_blink(&mut self, cx: &mut Context<Self>) {
        if !self.should_blink(cx) {
            self.cursor_blink_visible = true;
            self.cursor_blink_task = None;
            return;
        }
        if self.cursor_blink_task.is_some() {
            return;
        }
        if self.cursor_blink_expired {
            self.cursor_blink_visible = true;
            return;
        }
        let interval = Duration::from_millis(u64::from(
            self.connection
                .read(cx)
                .core
                .appearance()
                .map_or(500, |appearance| appearance.cursor_blink_interval_ms),
        ));
        self.cursor_blink_task = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(interval).await;
                let Ok(keep_running) = this.update(cx, |view, cx| {
                    if !view.painted || !view.should_blink(cx) {
                        view.cursor_blink_visible = true;
                        view.cursor_blink_task = None;
                        return false;
                    }
                    view.painted = false;
                    view.cursor_blink_visible = !view.cursor_blink_visible;
                    cx.notify();
                    if view.cursor_blink_visible
                        && cx.background_executor().now() - view.cursor_blink_idle_since
                            >= CURSOR_BLINK_TIMEOUT
                    {
                        view.cursor_blink_task = None;
                        view.cursor_blink_expired = true;
                        return false;
                    }
                    true
                }) else {
                    break;
                };
                if !keep_running {
                    break;
                }
            }
        }));
    }

    #[cfg_attr(not(target_os = "ios"), allow(dead_code))]
    pub(crate) fn typing(&self, window: &Window) -> bool {
        self.focus.is_focused(window)
    }
}

impl Focusable for TerminalPane {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        if self.search.is_some()
            && let Some(input) = &self.search_input
        {
            input.read(cx).focus_handle(cx)
        } else {
            self.focus.clone()
        }
    }
}

#[allow(
    clippy::disallowed_methods,
    reason = "terminal cells take the font and size of the user's terminal appearance"
)]
pub(crate) fn terminal_text<E: Styled>(element: E, font: Font, size: Pixels) -> E {
    element.font(font).text_size(size)
}

impl Render for TerminalPane {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.focus_subscriptions.is_empty() {
            self.focus_subscriptions.push(cx.on_focus_in(
                &self.focus,
                window,
                |view, window, cx| {
                    view.sync_focus(window, cx);
                },
            ));
            self.focus_subscriptions.push(cx.on_focus_out(
                &self.focus,
                window,
                |view, _, window, cx| {
                    view.sync_focus(window, cx);
                },
            ));
            self.focus_subscriptions.push(cx.observe_window_activation(
                window,
                |view, window, cx| {
                    view.sync_focus(window, cx);
                },
            ));
        }
        self.painted = true;
        self.sync_search_input(window, cx);
        self.sync_focus(window, cx);
        let appearance = localized_font_appearance(
            &self.connection.read(cx).core,
            &self.available_fonts,
            &cx.theme().mono_font_family,
            cx,
        );
        let font = terminal_font_for_style(&appearance, &cx.theme().mono_font_family, false, false);
        let font_size = px(appearance.font_size_points.clamp(7., 48.));
        self.text_font = Some((font.clone(), font_size));
        let header_inset = self.header_band().map_or(Pixels::ZERO, |band| band.height);
        let prepare = cx.entity();
        let paint = cx.entity();
        let mut mode = None;
        let mut hovered_uri = None;
        let mut search_status = None;
        let mut top_right: Vec<AnyElement> = Vec::new();
        let mut bottom_right: Vec<AnyElement> = Vec::new();
        if let Some(viewport) = self.viewport(&self.connection.read(cx).core) {
            hovered_uri.clone_from(&viewport.presentation.hovered_uri);
            search_status = viewport.search;
            if let Some(uri) = &hovered_uri
                && !self.compact
                && pasted_image_number(uri).is_none()
            {
                bottom_right.push(terminal_link_popup(presented_uri(uri), cx).into_any_element());
            }
            mode = terminal_mode_text(viewport.mode, viewport.unseen_output)
                .filter(|(label, _)| !self.compact || label.is_some());
            if let Some(status) = terminal_status_text(&viewport.status) {
                bottom_right.push(terminal_status_popup(status, cx).into_any_element());
            }
        }
        self.observe_image_hover(
            if self.pointer.is_some() && self.surface == TerminalSurface::Pane {
                hovered_uri.clone()
            } else {
                None
            },
            cx,
        );
        if self.search.is_some() && !self.compact {
            top_right.push(self.search_bar(search_status, cx));
        }
        let mut root = terminal_text(div().id(("terminal", self.pane.0)), font, font_size)
            .relative()
            .size_full()
            .when(
                !self.rows_above && !self.rows_below,
                Styled::overflow_hidden,
            )
            .rounded_bl(self.corner_radii.bottom_left)
            .rounded_br(self.corner_radii.bottom_right)
            .when(header_inset > Pixels::ZERO, |root| {
                root.rounded_tl(self.corner_radii.top_left)
                    .rounded_tr(self.corner_radii.top_right)
            })
            .when(!self.floating, |root| {
                root.pl(px(appearance.padding_left))
                    .pr(px(appearance.padding_right))
                    .pt(px(appearance.padding_top) + header_inset)
                    .pb(px(appearance.padding_bottom))
            })
            .key_context("Terminal")
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::on_key_down))
            .on_key_up(cx.listener(Self::on_key_up))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_down(MouseButton::Middle, cx.listener(Self::on_mouse_down))
            .on_mouse_down(MouseButton::Right, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up(MouseButton::Middle, cx.listener(Self::on_mouse_up))
            .on_mouse_up(MouseButton::Right, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Middle, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Right, cx.listener(Self::on_mouse_up))
            .on_mouse_exit(cx.listener(Self::on_mouse_exit))
            .on_modifiers_changed(cx.listener(Self::on_modifiers_changed))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_scroll_wheel(cx.listener(Self::on_scroll))
            .child(
                canvas(
                    move |bounds, window, cx| {
                        let hitbox = window.insert_hitbox(bounds, ala::HitboxBehavior::Normal);
                        (
                            prepare.update(cx, |view, cx| view.prepare(bounds, window, cx)),
                            hitbox,
                        )
                    },
                    move |bounds, (mut state, hitbox), window, cx| {
                        paint.update(cx, |view, cx| {
                            view.paint(bounds, &mut state, &hitbox, window, cx);
                        });
                    },
                )
                .size_full(),
            );
        if hovered_uri.is_some() {
            root = root.cursor_pointer();
        }
        if let Some((label, detail)) = mode {
            top_right.push(
                terminal_mode_indicator("terminal-mode", label, detail, cx).into_any_element(),
            );
        }
        if let Some(dead) = self.pane_status.0.as_ref().filter(|_| !self.compact) {
            top_right
                .push(pane_status_badge(IconName::CircleX, dead.clone(), cx).into_any_element());
        }
        if self.pane_status.1 {
            top_right.push(pane_sync_badge(cx).into_any_element());
        }
        if !top_right.is_empty() {
            root = root.child(
                pane_overlay_stack(PaneOverlayCorner::TopRight, top_right)
                    .top(px(8.0) + header_inset)
                    .left(px(8.0))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation()),
            );
        }
        if let Some(popover) = self.image_hover_popover(cx) {
            root = root.child(popover);
        }
        if !bottom_right.is_empty() {
            root = root.child(pane_overlay_stack(
                PaneOverlayCorner::BottomRight,
                bottom_right,
            ));
        }
        root
    }
}

impl EntityInputHandler for TerminalPane {
    fn paste(&mut self, item: ClipboardItem, window: &mut Window, cx: &mut Context<Self>) {
        if self.search.is_some()
            && let Some(input) = self.search_input.clone()
        {
            input.update(cx, |input, cx| input.paste(item, window, cx));
            return;
        }
        if self.surface == TerminalSurface::Pane
            && self.search.is_none()
            && let Some(image) = item.entries().iter().find_map(|entry| match entry {
                ClipboardEntry::Image(image) => Some(image),
                _ => None,
            })
        {
            self.connection.update(cx, |connection, cx| {
                connection.upload_image(self.pane, image, cx);
            });
            return;
        }
        if let Some(text) = item.text() {
            self.replace_text_in_range(None, &text, window, cx);
        }
    }

    fn text_for_range(
        &mut self,
        _: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let text = self.marked_text.clone()?;
        *adjusted = Some(0..text.encode_utf16().count());
        Some(text)
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
        cx.notify();
    }
    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.search.is_some()
            && let Some(input) = self.search_input.clone()
        {
            input.update(cx, |input, cx| {
                input.replace_text_in_range(range, text, window, cx);
            });
            return;
        }
        self.reset_cursor_blink(cx);
        let composed = self.marked_text.take().is_some();
        if !composed
            && self.search.is_none()
            && let Some(warning) = oversized_paste_warning(text)
        {
            self.connection
                .update(cx, |connection, cx| connection.notify_warning(warning, cx));
            return;
        }
        if !text.is_empty() {
            cx.emit(TerminalTyped);
            self.cancel_local_scroll(cx);
            if let Some(query) = self.search.as_mut() {
                query.text.push_str(text);
                let query = query.clone();
                self.view(TerminalViewAction::SearchUpdate(query), cx);
            } else if !composed && (text.contains(['\n', '\r']) || text.chars().count() > 1) {
                self.view(TerminalViewAction::Paste(text.into()), cx);
            } else if let Some(keystroke) = sticky_keystroke(text, cx) {
                window.refresh();
                self.send(
                    InputMessage::Key {
                        pane: self.pane,
                        input: key_input(&KeyDownEvent {
                            keystroke,
                            is_held: false,
                            prefer_character_input: false,
                        }),
                        text_follows: false,
                    },
                    cx,
                );
            } else {
                self.send(
                    InputMessage::Text {
                        pane: self.pane,
                        text: text.into(),
                    },
                    cx,
                );
            }
        }
        window.invalidate_character_coordinates();
        cx.notify();
    }
    fn replace_and_mark_text_in_range(
        &mut self,
        _: Option<Range<usize>>,
        text: &str,
        _: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.reset_cursor_blink(cx);
        self.marked_text = (!text.is_empty()).then(|| text.into());
        window.invalidate_character_coordinates();
        cx.notify();
    }
    fn bounds_for_range(
        &mut self,
        _: Range<usize>,
        _: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        Some(self.cursor_bounds)
    }
    fn character_index_for_point(
        &mut self,
        _: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        Some(0)
    }
    fn text_input_configuration(
        &mut self,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> TextInputConfiguration {
        TextInputConfiguration {
            input_action: TextInputAction::Enter,
            ..TextInputConfiguration::default()
        }
    }
}

fn sticky_keystroke(text: &str, cx: &mut App) -> Option<ala::Keystroke> {
    let mut characters = text.chars();
    let (Some(character), None) = (characters.next(), characters.next()) else {
        return None;
    };
    if penna::blocks::touch::StickyModifiers::get(cx).is_empty() {
        return None;
    }
    let keystroke = ala::Keystroke {
        modifiers: ala::Modifiers {
            shift: character.is_uppercase(),
            ..ala::Modifiers::default()
        },
        key: if character == ' ' {
            "space".to_owned()
        } else {
            character.to_lowercase().collect()
        },
        key_char: Some(text.to_owned()),
    };
    Some(penna::blocks::touch::StickyModifiers::apply(&keystroke, cx))
}

#[must_use]
pub fn key_input(event: &KeyDownEvent) -> KeyInput {
    keystroke_input(
        &event.keystroke,
        if event.is_held {
            KeyAction::Repeat
        } else {
            KeyAction::Press
        },
    )
}

pub(crate) fn keystroke_input(keystroke: &Keystroke, action: KeyAction) -> KeyInput {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    {
        crate::input::key_input(keystroke, action)
    }
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    {
        let key = key_code(&keystroke.key);
        let character = if let KeyCode::Character(value) = key {
            Some(value)
        } else {
            None
        };
        KeyInput {
            action,
            key,
            modifiers: wire_modifiers(keystroke.modifiers),
            text: keystroke
                .key_char
                .clone()
                .or_else(|| character.map(|value| value.to_string()))
                .filter(|text| !text.chars().any(char::is_control))
                .map(String::into_boxed_str),
            unshifted_codepoint: character,
        }
    }
}

fn key_code(key: &str) -> KeyCode {
    match key {
        "space" => KeyCode::Character(' '),
        "backspace" => KeyCode::Backspace,
        "enter" => KeyCode::Enter,
        "tab" => KeyCode::Tab,
        "escape" => KeyCode::Escape,
        "delete" => KeyCode::Delete,
        "insert" => KeyCode::Insert,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "pageup" => KeyCode::PageUp,
        "pagedown" => KeyCode::PageDown,
        "up" => KeyCode::ArrowUp,
        "down" => KeyCode::ArrowDown,
        "left" => KeyCode::ArrowLeft,
        "right" => KeyCode::ArrowRight,
        value if value.chars().count() == 1 => KeyCode::Character(value.chars().next().unwrap()),
        value => value
            .strip_prefix('f')
            .and_then(|number| number.parse().ok())
            .map_or(KeyCode::Unidentified, KeyCode::Function),
    }
}

fn wire_modifiers(modifiers: ala::Modifiers) -> zz_terminal::Modifiers {
    zz_terminal::Modifiers::new(
        modifiers.shift,
        modifiers.control,
        modifiers.alt,
        modifiers.platform,
    )
}

fn mouse_button(button: MouseButton) -> Option<TerminalMouseButton> {
    match button {
        MouseButton::Left => Some(TerminalMouseButton::Left),
        MouseButton::Middle => Some(TerminalMouseButton::Middle),
        MouseButton::Right => Some(TerminalMouseButton::Right),
        MouseButton::Navigate(_) => None,
    }
}

#[cfg(all(test, not(target_family = "wasm")))]
mod tests {
    use super::*;
    use zz_client::{ChromeKeymap, ChromeProfile};

    #[test]
    fn the_device_cursor_replaces_only_the_hosts_default_shape() {
        let color = zz_terminal::Color::rgb(0xff, 0x79, 0xc6);
        let cursor = |style| Some(Cursor::new(4, 2, true, false, false, style, color));
        let restyled = device_cursor(
            cursor(CursorStyle::Block),
            CursorStyle::Block,
            CursorStyle::Underline,
        )
        .unwrap();
        assert_eq!(restyled, cursor(CursorStyle::Underline).unwrap());
        assert_eq!(
            device_cursor(
                cursor(CursorStyle::Bar),
                CursorStyle::Block,
                CursorStyle::Underline
            ),
            None
        );
        assert_eq!(
            device_cursor(
                cursor(CursorStyle::Block),
                CursorStyle::Block,
                CursorStyle::Block
            ),
            None
        );
    }

    fn attached_terminal(
        cx: &mut ala::TestAppContext,
        viewport: TerminalViewport,
    ) -> (
        Entity<TestHost>,
        &mut ala::VisualTestContext,
        Entity<Connection>,
        Rc<std::cell::RefCell<Vec<zz_protocol::ProtocolMessage>>>,
    ) {
        use zz_protocol::{
            Batch, ClientView, Event, EventPayload, LayoutNode, MuxSnapshot, PaneKindSnapshot,
            PaneSnapshot, PaneSummary, ProtocolMessage, SessionId, SessionSnapshot, WindowId,
            WindowSnapshot,
        };
        cx.update(zz_ui::init);
        let connection_slot = Rc::new(std::cell::RefCell::new(None));
        let captured_connection = Rc::clone(&connection_slot);
        let (terminal, cx) = cx.add_window_view(move |_, cx| {
            let connection = cx.new(Connection::new);
            connection.update(cx, |connection, cx| {
                let snapshot = MuxSnapshot {
                    generation: 1,
                    sessions: vec![SessionSnapshot {
                        id: SessionId(1),
                        name: "test".into(),
                        active_window: WindowId(1),
                        viewers: Vec::new(),
                        windows: vec![WindowSnapshot {
                            id: WindowId(1),
                            index: 0,
                            name: "test".into(),
                            automatic_rename: true,
                            active_pane: PaneId(1),
                            zoomed_pane: None,
                            layout: LayoutNode::Pane(PaneId(1)),
                            panes: [(
                                PaneId(1),
                                PaneSnapshot {
                                    id: PaneId(1),
                                    title: "test".into(),
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
                                    summary: PaneSummary::default(),
                                },
                            )]
                            .into(),
                            layout_dump: String::new(),
                            visible_layout_dump: String::new(),
                            status_label: String::new(),
                            activity: false,
                            silence: false,
                            pane_border_status: zz_protocol::PaneBorderStatus::default(),
                            pane_border_lines: zz_protocol::PaneBorderLines::default(),
                            pane_border_indicators: zz_protocol::PaneBorderIndicators::default(),
                            pane_order: vec![PaneId(1)],
                            pane_z_order: vec![PaneId(1)],
                            floating: Vec::new(),
                            modal: None,
                            sx: 0,
                            sy: 0,
                            pane_base_index: 0,
                        }],
                    }],
                    focused_window: Some(WindowId(1)),
                };
                let messages = [
                    EventPayload::Snapshot(snapshot),
                    EventPayload::ClientView(ClientView {
                        session: Some(SessionId(1)),
                        focused_window: Some(WindowId(1)),
                        attachment_generation: 1,
                        layout_generation: 1,
                        ..Default::default()
                    }),
                    EventPayload::TerminalViewport {
                        pane: PaneId(1),
                        viewport,
                    },
                ]
                .into_iter()
                .enumerate()
                .map(|(index, payload)| {
                    ProtocolMessage::Event(Event {
                        sequence: index as u64 + 1,
                        payload,
                    })
                });
                connection.handle_message_for_test(
                    ProtocolMessage::Batch(Batch::from_messages(1, messages).unwrap()),
                    cx,
                );
            });
            let sent = connection.update(cx, |connection, _| connection.record_input_for_test());
            captured_connection.replace(Some((connection.clone(), sent)));
            TestHost {
                terminal: cx.new(|cx| TerminalPane::new(PaneId(1), connection, cx)),
                shown: true,
            }
        });
        let (connection, sent) = connection_slot.borrow().clone().unwrap();
        (terminal, cx, connection, sent)
    }

    struct TestHost {
        terminal: Entity<TerminalPane>,
        shown: bool,
    }

    impl Render for TestHost {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
                .size_full()
                .when(self.shown, |this| this.child(self.terminal.clone()))
        }
    }

    #[ala::test]
    fn shared_terminal_geometry_retries_the_same_grid_after_a_new_layout_view(
        cx: &mut ala::TestAppContext,
    ) {
        use zz_protocol::{ClientView, Event, EventPayload, ProtocolMessage, SessionId, WindowId};
        use zz_terminal::SessionStatus;

        let (host, cx, connection, sent) =
            attached_terminal(cx, TerminalViewport::blank(80, 24, SessionStatus::Running));
        let reports = || {
            sent.borrow()
                .iter()
                .filter_map(|message| {
                    if let ProtocolMessage::Input(InputMessage::ResizeTerminalV2 {
                        columns,
                        rows,
                        cell_width_px,
                        cell_height_px,
                        layout_generation,
                        ..
                    }) = message
                    {
                        Some((
                            *columns,
                            *rows,
                            *cell_width_px,
                            *cell_height_px,
                            *layout_generation,
                        ))
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
        };
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert_eq!(reports().len(), 1);
        connection.update(cx, |connection, cx| {
            connection.handle_message_for_test(
                ProtocolMessage::Event(Event {
                    sequence: 4,
                    payload: EventPayload::ClientView(ClientView {
                        session: Some(SessionId(1)),
                        focused_window: Some(WindowId(1)),
                        attachment_generation: 1,
                        layout_generation: 2,
                        ..Default::default()
                    }),
                }),
                cx,
            );
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let resized = reports();
        assert_eq!(resized.len(), 2);
        assert_eq!(resized[0].4, 1);
        assert_eq!(resized[1].4, 2);
        assert_eq!(resized[0].0, resized[1].0);
        assert_eq!(resized[0].1, resized[1].1);
        assert_eq!(resized[0].2, resized[1].2);
        assert_eq!(resized[0].3, resized[1].3);
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert_eq!(reports().len(), 2);
        drop(host);
    }

    #[ala::test]
    fn cursor_blink_settles_after_idle_and_stops_while_hidden(cx: &mut ala::TestAppContext) {
        let mut viewport = TerminalViewport::blank(80, 24, zz_terminal::SessionStatus::Running);
        viewport.cursor = Some(zz_terminal::Cursor::new(
            0,
            0,
            true,
            true,
            false,
            zz_terminal::CursorStyle::Block,
            zz_terminal::Color::rgb(1, 2, 3),
        ));
        let (host, cx, _connection, _sent) = attached_terminal(cx, viewport);
        let terminal = host.read_with(cx, |host, _| host.terminal.clone());
        let blinking = |cx: &mut ala::VisualTestContext| {
            terminal.read_with(cx, |view, _| {
                (view.cursor_blink_task.is_some(), view.cursor_blink_visible)
            })
        };
        let tick = |cx: &mut ala::VisualTestContext| {
            cx.executor().advance_clock(Duration::from_millis(500));
            cx.run_until_parked();
        };
        cx.update(|window, cx| {
            window.activate_window();
            terminal.update(cx, |view, cx| window.focus(&view.focus, cx));
        });
        cx.run_until_parked();
        assert_eq!(blinking(cx), (true, true));
        tick(cx);
        assert_eq!(blinking(cx), (true, false));
        host.update(cx, |host, cx| {
            host.shown = false;
            cx.notify();
        });
        cx.run_until_parked();
        tick(cx);
        tick(cx);
        assert_eq!(blinking(cx), (false, true));
        host.update(cx, |host, cx| {
            host.shown = true;
            cx.notify();
        });
        cx.run_until_parked();
        assert_eq!(blinking(cx), (true, true));
        for _ in 0..CURSOR_BLINK_TIMEOUT.as_millis() / 500 + 2 {
            tick(cx);
        }
        assert_eq!(blinking(cx), (false, true));
        terminal.update(cx, |view, cx| {
            assert!(view.cursor_blink_expired);
            view.reset_cursor_blink(cx);
        });
        assert_eq!(blinking(cx), (true, true));
    }

    #[test]
    fn terminal_search_keys_preserve_direction_and_cycle_regex_and_case() {
        let mut query = SearchQuery {
            text: "café".into(),
            direction: SearchDirection::Backward,
            ..SearchQuery::default()
        };
        let key = |value, shift, alt| {
            keystroke_input(
                &Keystroke {
                    key: value,
                    key_char: None,
                    modifiers: ala::Modifiers {
                        shift,
                        alt,
                        ..Default::default()
                    },
                },
                KeyAction::Press,
            )
        };
        assert_eq!(
            search_key_action(&mut query, &key("enter".into(), false, false)),
            None
        );
        assert_eq!(
            search_key_action(&mut query, &key("enter".into(), true, false)),
            None
        );
        let _ = search_key_action(&mut query, &key("r".into(), false, true));
        assert_eq!(query.mode, SearchMode::Regex);
        let _ = search_key_action(&mut query, &key("r".into(), false, true));
        assert_eq!(query.mode, SearchMode::Literal);
        for expected in [
            SearchCase::Sensitive,
            SearchCase::Insensitive,
            SearchCase::Smart,
        ] {
            let _ = search_key_action(&mut query, &key("c".into(), false, true));
            assert_eq!(query.case, expected);
        }
        assert_eq!(
            search_key_action(&mut query, &key("backspace".into(), false, false)),
            None
        );
        assert_eq!(query.text, "café");
        assert_eq!(query.direction, SearchDirection::Backward);
        assert_eq!(
            search_key_action(&mut query, &key("x".into(), false, false)),
            None
        );
    }

    #[test]
    fn terminal_mode_feedback_honors_hidden_copy_position() {
        assert_eq!(terminal_mode_text(TerminalMode::Live, 0), None);
        assert_eq!(
            terminal_mode_text(
                TerminalMode::Copy {
                    position: 12,
                    total: 20,
                    hide_position: true,
                },
                3,
            ),
            Some((Some("Copy mode"), "+3 output".into()))
        );
        assert_eq!(
            terminal_mode_text(
                TerminalMode::View {
                    position: 12,
                    total: 20,
                },
                0,
            ),
            Some((Some("View mode"), "12 / 20".into()))
        );
    }

    #[test]
    fn hovered_uri_presentation_preserves_character_boundaries() {
        let uri = format!("https://example.com/{}", "界".repeat(300));
        let presented = presented_uri(&uri);
        assert_eq!(presented.chars().count(), 241);
        assert!(presented.ends_with('…'));
        assert_eq!(presented_uri("https://example.com"), "https://example.com");
    }

    #[test]
    fn terminal_fonts_keep_available_overrides_and_drop_remote_defaults() {
        let available = vec!["Lilex".to_owned(), "Local Mono".to_owned()];
        let mut explicit = vec![
            "Missing Mono".to_owned(),
            "local mono".to_owned(),
            "Lilex".to_owned(),
        ];
        localize_font_stack(&mut explicit, AppearanceSource::Override, &available);
        assert_eq!(explicit, ["local mono", "Lilex"]);
        localize_font_stack(&mut explicit, AppearanceSource::Default, &available);
        assert!(explicit.is_empty());
    }

    #[test]
    fn trackpad_deltas_accumulate_without_losing_direction_changes() {
        let mut remainder = 0.;
        assert_eq!(accumulate_scroll(&mut remainder, -0.25), 0);
        assert_eq!(accumulate_scroll(&mut remainder, -0.25), 0);
        assert_eq!(accumulate_scroll(&mut remainder, -0.75), -1);
        assert_eq!(accumulate_scroll(&mut remainder, 0.5), 0);
        assert_eq!(accumulate_scroll(&mut remainder, 0.75), 1);
        assert_eq!(remainder, 0.);
    }

    #[test]
    fn a_capture_keys_modal_takes_terminal_shortcuts_from_the_chrome() {
        let desktop = ChromeKeymap::for_profile(ChromeProfile::Desktop);
        let apple = ChromeKeymap::for_profile(ChromeProfile::DesktopApple);
        for (key, action) in [
            ("ctrl-shift-f", ChromeAction::TerminalSearch),
            ("ctrl-shift-c", ChromeAction::TerminalCopy),
            ("ctrl-shift-v", ChromeAction::TerminalPaste),
        ] {
            let input = keystroke_input(&Keystroke::parse(key).unwrap(), KeyAction::Press);
            assert_eq!(
                terminal_shortcut([&desktop, &apple], false, None, &input),
                Some(action),
                "{key}"
            );
            assert_eq!(
                terminal_shortcut([&desktop, &apple], false, Some(PaneId(4)), &input),
                None,
                "{key}"
            );
            assert_eq!(
                terminal_shortcut([&desktop, &apple], true, None, &input),
                None
            );
        }
    }

    #[test]
    fn shifted_punctuation_and_releases_preserve_the_shared_key_contract() {
        let keystroke = Keystroke {
            key: "/".into(),
            key_char: Some("?".into()),
            modifiers: ala::Modifiers {
                shift: true,
                ..Default::default()
            },
        };
        let input = keystroke_input(&keystroke, KeyAction::Repeat);
        assert_eq!(input.key, KeyCode::Character('/'));
        assert_eq!(input.text.as_deref(), Some("?"));
        assert_eq!(input.unshifted_codepoint, Some('/'));
        assert!(input.modifiers.shift());
        assert_eq!(input.action, KeyAction::Repeat);
        let released = keystroke_input(&keystroke, KeyAction::Release);
        assert_eq!(released.action, KeyAction::Release);
        assert_eq!(released.key, input.key);
    }
}
