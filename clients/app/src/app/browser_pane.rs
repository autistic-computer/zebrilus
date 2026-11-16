mod history;

use std::{cell::Cell, collections::VecDeque, rc::Rc, sync::Arc, time::Duration};

use crate::native_browser::{
    BrowserEvent, BrowserMount, BrowserProfile, BrowserView, ElementPickerAppearance,
};
use ala::{
    Anchor, AnyElement, App, Bounds, ClipboardEntry, ClipboardItem, ClipboardString, Context,
    Corners, Entity, FocusHandle, Focusable, Hsla, Image, ImageFormat, KeyBinding, MouseButton,
    Pixels, Render, RenderImage, Rgba, Subscription, TextInputMode, TouchDragEvent, TouchPhase,
    Window, canvas, div, point, prelude::*, px, size,
};
use futures::StreamExt as _;
use penna::{
    ActiveTheme as _, Colorize as _, ElementExt as _, Elevation, IconName, Label, PaintState,
    PanelLook, TextStyle, WindowExt as _,
    blocks::compact::pill_backdrop_reach,
    blocks::sheet::{floating_sheet, sheet_inset},
    blocks::touch::TOUCH_TARGET,
    components::button::{Button, ButtonVariants as _},
    components::input::{InputEvent, InputState, SelectAll},
    components::menu::DropdownMenu as _,
    components::progress::Progress,
};
use zz_client::{
    ChromeAction,
    recent_pages::{Omnibox, OmniboxUse},
    url_input::diagnostic_url,
};
use zz_protocol::{
    BrowserCommand, BrowserDescriptor, DEFAULT_BROWSER_PROFILE, PaneId, PaneKindSnapshot,
};
use zz_ui::{
    browser::{
        BrowserActionMenuState, BrowserEmptyHint, BrowserErrorPanel, BrowserHeader,
        BrowserMenuActions, BrowserMenuEngine, BrowserMenuProfile, BrowserProfileDiscoveryState,
        BrowserSiteMenuState, BrowserTabInfo, BrowserTabStrip, BrowserToolbar, browser_action_menu,
        browser_address, browser_omnibox_panel, browser_omnibox_row, browser_recent_row,
        browser_site_controls_button, browser_site_menu, browser_start_surface,
        browser_toolbar_button,
    },
    compact::{TabRow, failed_page, tabs_sheet},
    dialogs::browser_clear_site_data_alert,
    pane::{PaneDrag, PaneDragSource, pane_header_icon_button},
    pane_look::PaneLook,
};

use super::compact::PillNotice;
use crate::connection::Connection;

struct Tab {
    id: u64,
    view: Rc<BrowserView>,
    url: String,
    title: String,
    loading: bool,
    back: bool,
    forward: bool,
    pending_load: bool,
    navigating: bool,
    error: Option<String>,
    progress: f32,
    picking: bool,
    held: bool,
    pick_status: Option<String>,
    page_zoom: f64,
    muted: bool,
    visited: String,
    history_use: Option<OmniboxUse>,
    history_started: bool,
    _events: ala::Task<()>,
}

pub(super) struct BrowserPane {
    pane: PaneId,
    host: String,
    profile_name: String,
    profile: Rc<BrowserProfile>,
    connection: Entity<Connection>,
    address: Entity<InputState>,
    focus: FocusHandle,
    tabs: Vec<Tab>,
    active: usize,
    next_tab: u64,
    received: BrowserDescriptor,
    pending: VecDeque<BrowserDescriptor>,
    visible: bool,
    active_pane: bool,
    compact: bool,
    tabs_sheet: bool,
    address_open: bool,
    editing: bool,
    omnibox: Omnibox,
    suggestions_hole: Rc<Cell<Option<(Bounds<Pixels>, Pixels)>>>,
    holding_pick: bool,
    pick_reached_page: bool,
    covered_bottom: Pixels,
    pill_hole: Option<(Bounds<Pixels>, Pixels)>,
    backdrop: Option<(u64, Bounds<Pixels>, Arc<RenderImage>, Option<Hsla>)>,
    frozen: bool,
    still: Option<(u64, Arc<RenderImage>)>,
    radii: Corners<Pixels>,
    header_drag_handler: Option<Rc<dyn Fn(&PaneDrag, &mut Window, &mut App)>>,
    float_rows_handler: Option<Rc<dyn Fn(&App) -> Option<u16>>>,
    header_touch_drag_handler: Option<Rc<dyn Fn(&ala::TouchDragEvent, &mut Window, &mut App)>>,
    notice_handler: Option<Rc<dyn Fn(&'static str, PillNotice, &mut Window, &mut App)>>,
    _subscriptions: Vec<Subscription>,
    _backdrop_tick: ala::Task<()>,
}

const BACKDROP_TICK: Duration = Duration::from_secs(1);
const LOAD_LINE: f32 = 2.0;
const SUGGESTION_LIMIT: usize = 8;
const RECENT_LIMIT: usize = 8;
const OMNIBOX_INPUT: &str = "BrowserOmnibox > PennaInput";

ala::actions!(
    browser,
    [OmniboxNext, OmniboxPrevious, OmniboxDelete, OmniboxEscape]
);

fn init(cx: &mut App) {
    if cx.has_global::<history::BrowserHistory>() {
        return;
    }
    cx.set_global(history::BrowserHistory::default());
    cx.bind_keys([
        KeyBinding::new("down", OmniboxNext, Some(OMNIBOX_INPUT)),
        KeyBinding::new("up", OmniboxPrevious, Some(OMNIBOX_INPUT)),
        KeyBinding::new("shift-delete", OmniboxDelete, Some(OMNIBOX_INPUT)),
        KeyBinding::new("escape", OmniboxEscape, Some(OMNIBOX_INPUT)),
    ]);
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct PhoneBrowser {
    pub url: String,
    pub back: bool,
    pub forward: bool,
    pub loading: bool,
    pub tabs: usize,
    pub picking: bool,
    pub writable: bool,
}

impl BrowserPane {
    pub(super) fn new(
        pane: PaneId,
        descriptor: &BrowserDescriptor,
        connection: Entity<Connection>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        init(cx);
        let host = connection
            .read(cx)
            .endpoint()
            .unwrap_or_default()
            .to_owned();
        let profile = BrowserProfile::new(&host, &descriptor.profile);
        profile.set_client(connection.read(cx).browser_client());
        let address = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Search or enter address")
                .input_mode(TextInputMode::Url)
        });
        let submit = cx.subscribe_in(
            &address,
            window,
            |this: &mut Self, input, event, window, cx| match event {
                InputEvent::PressEnter { .. } => {
                    let value = input.read(cx).value().to_string();
                    let open = this.address_open;
                    if this.compact {
                        this.navigate(&value, window, cx);
                    } else {
                        this.accept_omnibox(&value, window, cx);
                    }
                    if open {
                        this.address_open = false;
                        window.dismiss_virtual_keyboard();
                        cx.notify();
                    }
                }
                InputEvent::Change if this.address_open => cx.notify(),
                InputEvent::Change if !this.compact => {
                    let value = input.read(cx).value().to_string();
                    this.update_omnibox(&value, window, cx);
                }
                InputEvent::Focus if !this.compact => {
                    this.omnibox.begin(&input.read(cx).value());
                    this.editing = true;
                    cx.notify();
                }
                InputEvent::Blur => this.stop_editing(cx),
                _ => {}
            },
        );
        let commands = cx.subscribe_in(
            &connection,
            window,
            |this: &mut Self, _, event: &zz_client::CoreEvent, window, cx| {
                if let zz_client::CoreEvent::BrowserCommand { pane, command } = event {
                    if *pane != this.pane || !this.writable(cx) {
                        return;
                    }
                    match command {
                        BrowserCommand::Navigate(url) => {
                            this.navigate(url, window, cx);
                        }
                        BrowserCommand::Reload => this.reload(cx),
                        BrowserCommand::Back => this.go_back(),
                        BrowserCommand::Forward => this.go_forward(),
                        _ => {}
                    }
                }
            },
        );
        let descriptor = normalized(descriptor);
        let mut this = Self {
            pane,
            host,
            profile_name: descriptor.profile.clone(),
            profile,
            connection,
            address,
            focus: cx.focus_handle(),
            tabs: Vec::new(),
            active: descriptor.active_tab,
            next_tab: 0,
            received: descriptor.clone(),
            pending: VecDeque::new(),
            visible: false,
            active_pane: false,
            compact: false,
            tabs_sheet: false,
            address_open: false,
            editing: false,
            omnibox: Omnibox::default(),
            suggestions_hole: Rc::default(),
            holding_pick: false,
            pick_reached_page: false,
            covered_bottom: px(0.0),
            pill_hole: None,
            backdrop: None,
            frozen: false,
            still: None,
            radii: Corners::default(),
            header_drag_handler: None,
            float_rows_handler: None,
            header_touch_drag_handler: None,
            notice_handler: None,
            _subscriptions: vec![submit, commands],
            _backdrop_tick: cx.spawn(async move |this, cx| {
                loop {
                    cx.background_executor().timer(BACKDROP_TICK).await;
                    if this.update(cx, |this, _| this.tick_backdrop()).is_err() {
                        break;
                    }
                }
            }),
        };
        for url in &descriptor.tabs {
            let tab = this.new_tab(url.clone(), window, cx);
            this.tabs.push(tab);
        }
        this.update_address(window, cx);
        this.resume();
        this
    }

    pub(super) fn matches(&self, host: &str, profile: &str) -> bool {
        self.host == host && self.profile_name == profile
    }

    pub(super) fn belongs_to_host(&self, host: &str) -> bool {
        self.host == host
    }

    pub(super) fn set_header_drag_handler(
        &mut self,
        handler: impl Fn(&PaneDrag, &mut Window, &mut App) + 'static,
    ) {
        self.header_drag_handler = Some(Rc::new(handler));
    }

    pub(super) fn set_float_rows_handler(
        &mut self,
        handler: impl Fn(&App) -> Option<u16> + 'static,
    ) {
        self.float_rows_handler = Some(Rc::new(handler));
    }

    pub(super) fn set_header_touch_drag_handler(
        &mut self,
        handler: impl Fn(&ala::TouchDragEvent, &mut Window, &mut App) + 'static,
    ) {
        self.header_touch_drag_handler = Some(Rc::new(handler));
    }

    pub(super) fn set_notice_handler(
        &mut self,
        handler: impl Fn(&'static str, PillNotice, &mut Window, &mut App) + 'static,
    ) {
        self.notice_handler = Some(Rc::new(handler));
    }

    pub(super) fn set_pill(
        &mut self,
        covered_bottom: Pixels,
        hole: Option<(Bounds<Pixels>, Pixels)>,
        cx: &mut Context<Self>,
    ) {
        if self.covered_bottom != covered_bottom || self.pill_hole != hole {
            self.covered_bottom = covered_bottom;
            self.pill_hole = hole;
            cx.notify();
        }
    }

    pub(super) fn set_frozen(&mut self, frozen: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.frozen == frozen {
            return;
        }
        self.frozen = frozen;
        if frozen {
            self.tabs[self.active].view.snapshot_still();
        } else if let Some((_, old)) = self.still.take() {
            let _ = window.drop_image(old);
        }
        cx.notify();
    }

    pub(super) fn still_ready(&self) -> bool {
        self.still
            .as_ref()
            .is_some_and(|(id, _)| *id == self.tabs[self.active].id)
    }

    pub(super) fn backdrop_color(&self) -> Option<Hsla> {
        let (id, _, _, color) = self.backdrop.as_ref()?;
        let tab = &self.tabs[self.active];
        let shown = self.visible
            && self.profile.available()
            && tab.error.is_none()
            && tab.url != "about:blank"
            && !(self.compact && self.tabs_sheet);
        color.filter(|_| shown && self.pill_hole.is_some() && *id == tab.id)
    }

    fn tick_backdrop(&self) {
        if self.visible && self.pill_hole.is_some() {
            self.tabs[self.active].view.refresh_backdrop();
        }
    }

    fn writable(&self, cx: &App) -> bool {
        self.connection.read(cx).connected && !self.connection.read(cx).core.attached_read_only()
    }

    fn toggle_element_pick(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.visible {
            self.switch_element_pick(window, cx);
        }
    }

    fn switch_element_pick(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.writable(cx) {
            return;
        }
        let tab = &mut self.tabs[self.active];
        tab.pick_status = None;
        if tab.picking {
            tab.view.cancel_element_pick();
            tab.picking = false;
        } else {
            self.start_element_pick(cx);
        }
        self.focus.focus(window, cx);
        cx.notify();
    }

    fn held_pick(
        &mut self,
        event: &TouchDragEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match event.phase {
            TouchPhase::Started => {
                if !self.writable(cx) || !self.visible || self.tabs[self.active].picking {
                    return false;
                }
                self.tabs[self.active].pick_status = None;
                self.holding_pick = self.start_element_pick(cx);
                self.tabs[self.active].held = self.holding_pick;
                self.pick_reached_page = false;
                if self.holding_pick {
                    self.focus.focus(window, cx);
                    cx.notify();
                }
                self.holding_pick
            }
            TouchPhase::Moved => {
                if self.holding_pick {
                    self.pick_reached_page |= self.tabs[self.active]
                        .view
                        .point_element_pick(event.position);
                }
                true
            }
            TouchPhase::Ended | TouchPhase::Cancelled => {
                if !std::mem::take(&mut self.holding_pick) {
                    return true;
                }
                let tab = &mut self.tabs[self.active];
                if event.phase == TouchPhase::Ended && self.pick_reached_page {
                    tab.view.point_element_pick(event.position);
                    tab.view.release_element_pick();
                } else {
                    tab.view.cancel_element_pick();
                    tab.picking = false;
                    if event.phase == TouchPhase::Ended {
                        self.pick_notice(
                            "Hold and drag onto the page",
                            PillNotice::Hint,
                            window,
                            cx,
                        );
                    }
                }
                cx.notify();
                true
            }
        }
    }

    fn pick_notice(
        &mut self,
        text: &'static str,
        kind: PillNotice,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.notice_handler.clone().filter(|_| self.compact) {
            Some(handler) => handler(text, kind, window, cx),
            None => self.tabs[self.active].pick_status = Some(text.into()),
        }
    }

    #[allow(
        clippy::disallowed_methods,
        reason = "the picker overlay is page CSS, not chrome"
    )]
    fn start_element_pick(&mut self, cx: &mut Context<Self>) -> bool {
        let tab = &mut self.tabs[self.active];
        if tab.loading {
            return false;
        }
        let theme = cx.theme();
        let appearance = ElementPickerAppearance {
            highlight_outline: penna::to_hex(theme.foreground),
            highlight_fill: penna::to_hex(theme.foreground.fill()),
            highlight_contrast: penna::to_hex(theme.background.opaque()),
            preview_background: penna::to_hex(theme.surface(Elevation::Panel).fill.opaque()),
            preview_foreground: penna::to_hex(theme.foreground),
            preview_border: penna::to_hex(theme.border()),
            shadow: theme
                .casts_shadow(penna::PanelLook::Popover)
                .then(|| penna::to_hex(theme.shadow_color)),
            radius: f32::from(theme.radius),
            font_family: theme.mono_font_family.to_string(),
            page_zoom: if cfg!(target_os = "ios") {
                tab.page_zoom
            } else {
                1.0
            },
        };
        match tab.view.start_element_pick(&appearance) {
            Ok(()) => tab.picking = true,
            Err(error) => tab.pick_status = Some(error),
        }
        tab.held = false;
        tab.picking
    }

    fn new_tab(&mut self, url: String, window: &mut Window, cx: &mut Context<Self>) -> Tab {
        let id = self.next_tab;
        self.next_tab += 1;
        let (view, mut events) = BrowserView::new(self.profile.clone());
        let task = cx.spawn_in(window, async move |this, cx| {
            while let Some(event) = events.next().await {
                if this
                    .update_in(cx, |this, window, cx| this.event(id, event, window, cx))
                    .is_err()
                {
                    break;
                }
            }
        });
        Tab {
            id,
            view,
            url,
            title: String::new(),
            loading: false,
            back: false,
            forward: false,
            pending_load: true,
            navigating: false,
            error: None,
            progress: 0.0,
            picking: false,
            held: false,
            pick_status: None,
            page_zoom: 1.0,
            muted: false,
            visited: String::new(),
            history_use: None,
            history_started: false,
            _events: task,
        }
    }

    fn event(&mut self, id: u64, event: BrowserEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.tabs.iter().position(|tab| tab.id == id) else {
            return;
        };
        match event {
            BrowserEvent::Changed {
                url,
                title,
                loading,
                can_go_back,
                can_go_forward,
                committed,
                progress,
            } => {
                self.record_history(index, url.as_deref(), &title, loading, committed, cx);
                let tab = &mut self.tabs[index];
                tab.progress = progress;
                if committed {
                    tab.navigating = false;
                    tab.error = None;
                    if tab.muted {
                        tab.view.evaluate(&sound_script(true));
                    }
                }
                if !tab.pending_load
                    && !tab.navigating
                    && let Some(url) = url
                {
                    tab.url = url;
                }
                tab.title = title;
                tab.loading = loading;
                tab.back = can_go_back;
                tab.forward = can_go_forward;
                self.update_address(window, cx);
                self.publish(cx);
            }
            BrowserEvent::Error(error) => {
                self.tabs[index].error = Some(error);
                self.tabs[index].history_use = None;
            }
            BrowserEvent::ElementPicked { text, screenshot } => {
                self.tabs[index].picking = false;
                if index != self.active || !self.visible || !self.writable(cx) {
                    return;
                }
                let mut entries = vec![ClipboardEntry::String(ClipboardString::new(
                    text.to_string(),
                ))];
                let status = if let Some(png) = screenshot {
                    entries.push(ClipboardEntry::Image(Image::from_bytes(
                        ImageFormat::Png,
                        png,
                    )));
                    "Element context + screenshot copied"
                } else {
                    "Element context copied"
                };
                cx.write_to_clipboard(ClipboardItem { entries });
                self.pick_notice(status, PillNotice::Done, window, cx);
            }
            BrowserEvent::ElementPickCancelled => self.tabs[index].picking = false,
            #[cfg(target_os = "ios")]
            BrowserEvent::SiteDataCleared => {
                self.tabs[index].view.reload();
                if index == self.active {
                    self.pick_notice("Site data cleared", PillNotice::Done, window, cx);
                }
            }
            BrowserEvent::ElementPickFailed => {
                self.tabs[index].picking = false;
                if index == self.active {
                    self.pick_notice(
                        "Could not inspect that element. Try again.",
                        PillNotice::Failed,
                        window,
                        cx,
                    );
                }
            }
            BrowserEvent::Backdrop { bounds, image } => {
                let hole = self.pill_hole.filter(|_| index == self.active);
                let next = hole.map(|(hole, _)| {
                    let color = average_under(&image, bounds, hole);
                    (self.tabs[index].id, bounds, image, color)
                });
                if let Some((_, _, old, _)) = std::mem::replace(&mut self.backdrop, next) {
                    let _ = window.drop_image(old);
                }
            }
            BrowserEvent::Still(image) => {
                if !self.frozen || index != self.active {
                    let _ = window.drop_image(image);
                    return;
                }
                if let Some((_, old)) = self.still.replace((self.tabs[index].id, image)) {
                    let _ = window.drop_image(old);
                }
            }
            BrowserEvent::Refresh => {
                if index == self.active {
                    self.refresh(cx);
                } else {
                    self.tabs[index].view.end_refresh();
                }
            }
            BrowserEvent::Open(url) => {
                if self.writable(cx) {
                    self.add_tab(url, window, cx);
                }
            }
            BrowserEvent::Focused => {
                if std::mem::take(&mut self.address_open) {
                    self.update_address(window, cx);
                }
                self.focus.focus(window, cx);
                self.connection.update(cx, |connection, cx| {
                    connection.command(
                        "select-pane",
                        vec!["-Z".into(), "-t".into(), self.pane.to_string()],
                        cx,
                    );
                });
            }
        }
        cx.notify();
    }

    pub(super) fn synchronize(
        &mut self,
        descriptor: &BrowserDescriptor,
        active: bool,
        radii: Corners<Pixels>,
        compact: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let changed = self.active_pane != active
            || self.radii != radii
            || self.compact != compact
            || normalized(descriptor) != self.received;
        self.active_pane = active;
        self.radii = radii;
        self.compact = compact;
        self.tabs_sheet &= compact;
        self.address_open &= compact;
        if !compact {
            self.covered_bottom = px(0.0);
            self.pill_hole = None;
        }
        let descriptor = normalized(descriptor);
        if descriptor != self.received {
            self.received = descriptor.clone();
            if let Some(index) = self
                .pending
                .iter()
                .position(|pending| *pending == descriptor)
            {
                self.pending.drain(..=index);
            } else {
                self.pending.clear();
                for (index, url) in descriptor.tabs.iter().enumerate() {
                    if let Some(tab) = self.tabs.get_mut(index) {
                        if tab.url != *url {
                            tab.url.clone_from(url);
                            tab.pending_load = true;
                            tab.view.stop();
                        }
                    } else {
                        let tab = self.new_tab(url.clone(), window, cx);
                        self.tabs.push(tab);
                    }
                }
                self.tabs.truncate(descriptor.tabs.len());
                self.active = descriptor.active_tab;
            }
            self.update_address(window, cx);
        }
        self.resume();
        if changed {
            cx.notify();
        }
    }

    pub(super) fn shown_page(&self) -> (&str, &str) {
        let tab = &self.tabs[self.active];
        (&tab.title, &tab.url)
    }

    pub(super) fn set_visible(&mut self, visible: bool, cx: &mut Context<Self>) {
        let changed_route = self
            .profile
            .set_client(self.connection.read(cx).browser_client());
        let available = self.profile.available();
        if changed_route {
            for tab in &mut self.tabs {
                tab.view.stop();
                tab.pending_load = true;
            }
            if available {
                self.resume();
            }
        }
        let writable = self.writable(cx);
        for (index, tab) in self.tabs.iter().enumerate() {
            tab.view
                .set_visible(visible && available && index == self.active);
            tab.view.set_interactive(writable);
        }
        if !visible && !self.frozen {
            self.address_open = false;
        }
        if self.visible != visible || changed_route {
            self.visible = visible;
            cx.notify();
        }
    }

    fn resume(&mut self) {
        if !self.profile.available() {
            return;
        }
        let tab = &mut self.tabs[self.active];
        if tab.pending_load {
            tab.navigating = true;
            tab.error = tab.view.load(&tab.url).err();
            if tab.error.is_some() {
                tab.navigating = false;
            }
            tab.pending_load = false;
        }
    }

    fn update_address(&self, window: &mut Window, cx: &mut Context<Self>) {
        if self.address.read(cx).focus_handle(cx).is_focused(window) {
            return;
        }
        let url = &self.tabs[self.active].url;
        let value = if url == "about:blank" { "" } else { url };
        if self.address.read(cx).value().as_str() != value {
            self.address
                .update(cx, |input, cx| input.set_value(value, window, cx));
        }
    }

    fn navigate(&mut self, value: &str, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if !self.writable(cx) {
            return false;
        }
        let search = cx
            .try_global::<crate::preferences::BrowserSearch>()
            .map_or_else(Default::default, |search| search.0);
        let Ok(url) = zz_client::url_input::resolve_address(value, search) else {
            return false;
        };
        let tab = &mut self.tabs[self.active];
        tab.url = url;
        tab.pick_status = None;
        tab.pending_load = true;
        tab.error = None;
        tab.history_use = None;
        self.focus.focus(window, cx);
        self.resume();
        self.update_address(window, cx);
        self.publish(cx);
        cx.notify();
        true
    }

    fn record_history(
        &mut self,
        index: usize,
        url: Option<&str>,
        title: &str,
        loading: bool,
        committed: bool,
        cx: &mut Context<Self>,
    ) {
        let (host, profile) = (self.host.clone(), self.profile_name.clone());
        let tab = &mut self.tabs[index];
        tab.history_started |= loading && tab.history_use.is_some();
        let Some(url) = url else {
            return;
        };
        let settled = (committed || !loading) && tab.error.is_none();
        let visit = settled && tab.visited != url;
        if visit {
            url.clone_into(&mut tab.visited);
        }
        let history_use = if settled && committed && !loading && tab.history_started {
            tab.history_use.take()
        } else {
            None
        };
        history::update(&host, cx, |pages| {
            let mut changed = visit && pages.record_visit(&profile, url);
            if let Some(history_use) = history_use {
                changed |= pages.record_omnibox_use(
                    &profile,
                    &history_use.input,
                    url,
                    history_use.selected,
                );
            }
            pages.record_title(&profile, url, title) || changed
        });
    }

    fn update_omnibox(&mut self, value: &str, window: &mut Window, cx: &mut Context<Self>) {
        if !self.editing {
            let url = &self.tabs[self.active].url;
            self.omnibox
                .begin(if url == "about:blank" { "" } else { url });
            self.editing = true;
        }
        let suggestions =
            history::suggestions(&self.host, &self.profile_name, value, SUGGESTION_LIMIT, cx);
        if let Some(completion) = self.omnibox.update(value, suggestions) {
            let query = self.omnibox.query.clone();
            let address = self.address.clone();
            cx.defer_in(window, move |this, window, cx| {
                if !this.editing || this.omnibox.query != query || this.omnibox.selected != Some(0)
                {
                    return;
                }
                address.update(cx, |address, cx| {
                    address.set_value(completion.clone(), window, cx);
                    address.set_selected_range(query.len()..completion.len(), cx);
                });
            });
        }
        cx.notify();
    }

    fn accept_omnibox(&mut self, typed: &str, window: &mut Window, cx: &mut Context<Self>) {
        let (value, history_use) = self.omnibox.accept(typed);
        if self.navigate(&value, window, cx) {
            let tab = &mut self.tabs[self.active];
            tab.history_use = history_use;
            tab.history_started = false;
        }
    }

    fn accept_suggestion(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if index < self.omnibox.suggestions.len() {
            self.omnibox.selected = Some(index);
            let typed = self.address.read(cx).value().to_string();
            self.accept_omnibox(&typed, window, cx);
        }
    }

    fn stop_editing(&mut self, cx: &mut Context<Self>) {
        if self.editing || !self.omnibox.is_clear() {
            self.editing = false;
            self.omnibox.reset();
            cx.notify();
        }
    }

    fn show_in_address(&self, value: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.address.update(cx, |address, cx| {
            if address.value().as_ref() != value {
                address.set_value(value.to_owned(), window, cx);
            }
            address.set_selected_range(value.len()..value.len(), cx);
        });
    }

    fn step_suggestion(&mut self, direction: i64, window: &mut Window, cx: &mut Context<Self>) {
        let Some(preview) = (self.editing && !self.compact)
            .then(|| self.omnibox.step(direction))
            .flatten()
        else {
            cx.propagate();
            return;
        };
        self.show_in_address(&preview, window, cx);
        cx.notify();
    }

    fn delete_suggestion(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(url) = self
            .omnibox
            .selected
            .and_then(|index| self.omnibox.suggestions.get(index))
            .map(|suggestion| suggestion.url.clone())
        else {
            cx.propagate();
            return;
        };
        let (host, profile) = (self.host.clone(), self.profile_name.clone());
        history::update(&host, cx, |pages| pages.remove(&profile, &url));
        let query = self.omnibox.query.clone();
        self.omnibox.suggestions =
            history::suggestions(&host, &profile, &query, SUGGESTION_LIMIT, cx);
        self.omnibox.selected = None;
        self.show_in_address(&query, window, cx);
        cx.notify();
    }

    fn escape_omnibox(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.omnibox.selected.take().is_some() {
            let query = self.omnibox.query.clone();
            self.show_in_address(&query, window, cx);
        } else if !self.omnibox.suggestions.is_empty() {
            self.omnibox.suggestions.clear();
        } else {
            cx.propagate();
            return;
        }
        cx.notify();
    }

    #[allow(
        clippy::disallowed_methods,
        reason = "the page is cut out to the popover's own border box"
    )]
    fn suggestions_panel(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.editing || self.compact || self.omnibox.suggestions.is_empty() {
            self.suggestions_hole.set(None);
            return None;
        }
        let pane = cx.weak_entity();
        let mut rows: Vec<AnyElement> = self
            .omnibox
            .suggestions
            .iter()
            .enumerate()
            .map(|(index, suggestion)| {
                let pane = pane.clone();
                browser_omnibox_row(
                    index,
                    suggestion.title.clone(),
                    suggestion.display_url.clone(),
                    self.omnibox.selected == Some(index),
                    None,
                    px(TOUCH_TARGET),
                    cx,
                )
                .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                    let _ = pane.update(cx, |this, cx| this.accept_suggestion(index, window, cx));
                    cx.stop_propagation();
                })
                .into_any_element()
            })
            .collect();
        let layer = cx.theme().layer(PanelLook::Popover, PaintState::Rest);
        let (edge, radius) = (layer.border_width, layer.radius);
        let hole = self.suggestions_hole.clone();
        rows.push(
            canvas(
                move |bounds, window, _| {
                    let next = Some((bounds, radius));
                    if hole.replace(next) != next {
                        window.on_next_frame(move |_, cx| {
                            let _ = pane.update(cx, |_, cx| cx.notify());
                        });
                    }
                },
                |_, (), _, _| {},
            )
            .absolute()
            .top(-edge)
            .left(-edge)
            .right(-edge)
            .bottom(-edge)
            .into_any_element(),
        );
        Some(browser_omnibox_panel(rows, cx).into_any_element())
    }

    fn start_page(&self, cx: &mut Context<Self>) -> AnyElement {
        let recents = history::recent(&self.host, &self.profile_name, RECENT_LIMIT, cx);
        let content = if recents.is_empty() {
            BrowserEmptyHint.into_any_element()
        } else {
            div()
                .flex()
                .flex_col()
                .w_full()
                .max_w(px(360.0))
                .gap(px(4.0))
                .children(recents.into_iter().enumerate().map(|(index, page)| {
                    browser_recent_row(
                        ("ios-browser-recent", index),
                        diagnostic_url(&page.url),
                        None,
                        px(TOUCH_TARGET),
                        cx,
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, window, cx| {
                            this.navigate(&page.url, window, cx);
                            cx.stop_propagation();
                        }),
                    )
                }))
                .into_any_element()
        };
        browser_start_surface(content)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    if this.writable(cx) {
                        this.focus_address(window, cx);
                    }
                }),
            )
            .into_any_element()
    }

    fn reload(&mut self, cx: &mut Context<Self>) {
        if !self.writable(cx) {
            return;
        }
        let tab = &mut self.tabs[self.active];
        tab.history_use = None;
        if tab.error.take().is_some() {
            tab.pending_load = true;
        }
        if tab.loading {
            tab.view.stop();
        } else if tab.pending_load {
            self.resume();
        } else {
            tab.view.reload();
        }
        cx.notify();
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        let writable = self.writable(cx);
        let tab = &mut self.tabs[self.active];
        if !writable {
            tab.view.end_refresh();
            return;
        }
        tab.pick_status = None;
        if tab.error.take().is_some() {
            tab.pending_load = true;
        }
        if tab.pending_load {
            self.resume();
        } else {
            tab.view.reload();
        }
        let tab = &self.tabs[self.active];
        if tab.error.is_some() {
            tab.view.end_refresh();
        }
        cx.notify();
    }

    fn select(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        if !self.writable(cx) {
            return;
        }
        let Some(index) = self.tabs.iter().position(|tab| tab.id == id) else {
            return;
        };
        self.tabs[self.active].view.set_visible(false);
        self.active = index;
        self.focus.focus(window, cx);
        self.resume();
        self.update_address(window, cx);
        self.publish(cx);
        cx.notify();
    }

    fn add_tab(&mut self, url: String, window: &mut Window, cx: &mut Context<Self>) {
        if !self.writable(cx) {
            return;
        }
        self.tabs[self.active].view.set_visible(false);
        let blank = url == "about:blank";
        let tab = self.new_tab(url, window, cx);
        self.tabs.push(tab);
        self.active = self.tabs.len() - 1;
        self.focus.focus(window, cx);
        self.update_address(window, cx);
        self.resume();
        self.publish(cx);
        if blank && self.compact {
            self.open_address(window, cx);
        } else if blank {
            self.focus_address(window, cx);
        }
        cx.notify();
    }

    fn focus_address(&self, window: &mut Window, cx: &mut App) {
        self.address.read(cx).focus_handle(cx).focus(window, cx);
        #[cfg(target_os = "ios")]
        crate::native_browser::take_keyboard(window);
    }

    fn close_tab(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        if !self.writable(cx) || self.tabs.len() == 1 {
            return;
        }
        let Some(index) = self.tabs.iter().position(|tab| tab.id == id) else {
            return;
        };
        self.tabs.remove(index);
        if index < self.active {
            self.active -= 1;
        }
        self.active = self.active.min(self.tabs.len() - 1);
        self.focus.focus(window, cx);
        self.resume();
        self.update_address(window, cx);
        self.publish(cx);
        cx.notify();
    }

    pub(super) fn phone_state(&self, cx: &App) -> PhoneBrowser {
        let tab = &self.tabs[self.active];
        let writable = self.writable(cx);
        PhoneBrowser {
            url: if tab.url == "about:blank" {
                String::new()
            } else {
                tab.url.clone()
            },
            back: writable && tab.back,
            forward: writable && tab.forward,
            loading: tab.loading,
            tabs: self.tabs.len(),
            picking: tab.picking,
            writable,
        }
    }

    pub(super) fn go_back(&mut self) {
        let tab = &mut self.tabs[self.active];
        tab.history_use = None;
        tab.view.back();
    }

    pub(super) fn go_forward(&mut self) {
        let tab = &mut self.tabs[self.active];
        tab.history_use = None;
        tab.view.forward();
    }

    pub(super) fn reload_page(&mut self, cx: &mut Context<Self>) {
        if !self.tabs[self.active].loading {
            self.reload(cx);
        }
    }

    pub(super) fn stop_page(&mut self, cx: &mut Context<Self>) {
        if self.tabs[self.active].loading {
            self.reload(cx);
        }
    }

    pub(super) fn chrome_action(
        &mut self,
        action: ChromeAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let count = self.tabs.len();
        let tab = &self.tabs[self.active];
        let writable = self.writable(cx);
        match action {
            ChromeAction::BrowserZoomIn => self.zoom(Some(1), cx),
            ChromeAction::BrowserZoomOut => self.zoom(Some(-1), cx),
            ChromeAction::BrowserZoomReset => self.zoom(None, cx),
            ChromeAction::ClosePane if count > 1 => self.close_tab(tab.id, window, cx),
            ChromeAction::BrowserNextTab => {
                self.select_index((self.active + 1) % count, window, cx);
            }
            ChromeAction::BrowserPreviousTab => {
                self.select_index((self.active + count - 1) % count, window, cx);
            }
            ChromeAction::BrowserSelectTab(index) => {
                self.select_index(usize::from(index), window, cx);
            }
            ChromeAction::BrowserSelectLastTab => self.select_index(count - 1, window, cx),
            ChromeAction::BrowserNewTab => self.add_tab("about:blank".into(), window, cx),
            ChromeAction::BrowserBack if writable => self.go_back(),
            ChromeAction::BrowserForward if writable => self.go_forward(),
            ChromeAction::BrowserReload if writable && tab.loading => tab.view.reload(),
            ChromeAction::BrowserReload => self.reload(cx),
            ChromeAction::BrowserFocusAddress if self.compact => self.open_address(window, cx),
            ChromeAction::BrowserFocusAddress if writable => {
                self.focus_address(window, cx);
                cx.on_next_frame(window, |_, window, cx| {
                    window.dispatch_action(Box::new(SelectAll), cx);
                });
            }
            ChromeAction::BrowserElementSelector if !tab.loading || tab.picking => {
                self.toggle_element_pick(window, cx);
            }
            ChromeAction::BrowserBack
            | ChromeAction::BrowserForward
            | ChromeAction::BrowserFocusAddress
            | ChromeAction::BrowserElementSelector => {}
            _ => return false,
        }
        true
    }

    fn profiles(&self, cx: &App) -> Vec<String> {
        let mut profiles: Vec<String> = self
            .connection
            .read(cx)
            .core
            .snapshot()
            .sessions
            .iter()
            .flat_map(|session| &session.windows)
            .flat_map(|window| window.panes.values())
            .filter_map(|pane| match &pane.kind {
                PaneKindSnapshot::Browser(descriptor) => Some(descriptor.profile.clone()),
                _ => None,
            })
            .chain(std::iter::once(self.profile_name.clone()))
            .filter(|profile| profile != DEFAULT_BROWSER_PROFILE)
            .collect();
        profiles.sort();
        profiles.dedup();
        profiles
    }

    fn switch_profile(&mut self, profile: &str, cx: &mut Context<Self>) {
        if profile == self.profile_name || !self.writable(cx) {
            return;
        }
        let args = vec!["-t".into(), self.pane.to_string(), profile.to_owned()];
        self.connection.update(cx, |connection, cx| {
            connection.command("set-browser-profile", args, cx);
        });
    }

    fn toggle_sound(&mut self, cx: &mut Context<Self>) {
        let tab = &mut self.tabs[self.active];
        tab.muted = !tab.muted;
        tab.view.evaluate(&sound_script(tab.muted));
        cx.notify();
    }

    fn can_clear_site_data(&self) -> bool {
        cfg!(target_os = "ios") && http_origin(&self.tabs[self.active].url)
    }

    fn confirm_clear_site_data(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.writable(cx) || !self.can_clear_site_data() {
            return;
        }
        let pane = cx.weak_entity();
        let id = self.tabs[self.active].id;
        window.open_alert_dialog(cx, move |alert, _, cx| {
            let pane = pane.clone();
            browser_clear_site_data_alert(alert, cx).on_ok(move |_, _, cx| {
                let _ = pane.update(cx, |this, _| this.clear_site_data(id));
                true
            })
        });
    }

    fn clear_site_data(&self, id: u64) {
        let Some(tab) = self.tabs.iter().find(|tab| tab.id == id) else {
            return;
        };
        let Some(site) = site_host(&tab.url) else {
            return;
        };
        #[cfg(target_os = "ios")]
        tab.view.clear_site_data(&site);
        #[cfg(not(target_os = "ios"))]
        let _ = site;
    }

    fn connection_secure(tab: &Tab) -> Option<bool> {
        if tab.url.starts_with("http://") {
            return Some(false);
        }
        if !tab.url.starts_with("https://") {
            return None;
        }
        #[cfg(target_os = "ios")]
        return Some(tab.view.secure_content());
        #[cfg(not(target_os = "ios"))]
        None
    }

    fn site_controls(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let pane = cx.weak_entity();
        browser_site_controls_button(cx).dropdown_menu(move |menu, _, cx| {
            let Some(browser) = pane.upgrade() else {
                return menu;
            };
            let state = {
                let this = browser.read(cx);
                let tab = &this.tabs[this.active];
                BrowserSiteMenuState {
                    site: site_host(&tab.url)
                        .unwrap_or_else(|| "This page".to_owned())
                        .into(),
                    connection_secure: Self::connection_secure(tab),
                    audio_muted: Some(tab.muted),
                    can_clear_site_data: this.can_clear_site_data() && this.writable(cx),
                }
            };
            let sound = browser.downgrade();
            let clear = browser.downgrade();
            browser_site_menu(
                menu,
                state,
                move |_, cx| {
                    let _ = sound.update(cx, Self::toggle_sound);
                },
                move |window, cx| {
                    let _ = clear.update(cx, |this, cx| this.confirm_clear_site_data(window, cx));
                },
            )
        })
    }

    fn more_menu(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let pane = cx.weak_entity();
        browser_toolbar_button(
            cx,
            "ios-browser-more",
            IconName::EllipsisVertical,
            "More browser actions",
            false,
            false,
        )
        .dropdown_menu(move |menu, window, cx| {
            let Some(browser) = pane.upgrade() else {
                return menu;
            };
            let (state, url, picking) = {
                let this = browser.read(cx);
                let tab = &this.tabs[this.active];
                let label = |profile: &str| {
                    if profile == DEFAULT_BROWSER_PROFILE {
                        "Default zz profile".to_owned()
                    } else {
                        profile.to_owned()
                    }
                };
                (
                    BrowserActionMenuState {
                        engine: BrowserMenuEngine::WebView,
                        current_profile_label: label(&this.profile_name).into(),
                        selected_profile: this.profile_name.clone().into(),
                        default_profile: DEFAULT_BROWSER_PROFILE.into(),
                        profiles: this
                            .profiles(cx)
                            .into_iter()
                            .map(|profile| BrowserMenuProfile::new(profile.clone(), profile))
                            .collect(),
                        profile_discovery: BrowserProfileDiscoveryState::Ready,
                        zoom_percent: zoom_percent(tab.page_zoom),
                        can_import_chrome_data: false,
                        can_clear_site_data: this.can_clear_site_data() && this.writable(cx),
                        picker_active: tab.picking,
                    },
                    tab.url.clone(),
                    tab.picking,
                )
            };
            let open_url = url.clone();
            let weak = browser.downgrade();
            let (switch, zoom_in, zoom_out, reset, read_zoom, clear, reload, picker) = (
                weak.clone(),
                weak.clone(),
                weak.clone(),
                weak.clone(),
                weak.clone(),
                weak.clone(),
                weak.clone(),
                weak,
            );
            let actions = BrowserMenuActions::new()
                .open_url(move |_, cx| cx.open_url(&open_url))
                .copy_url(move |_, cx| {
                    cx.write_to_clipboard(ala::ClipboardItem::new_string(url.clone()));
                })
                .switch_profile(move |profile, _, cx| {
                    let _ = switch.update(cx, |this, cx| this.switch_profile(&profile, cx));
                })
                .zoom_percent(move |cx| {
                    read_zoom.upgrade().map_or(100, |browser| {
                        let this = browser.read(cx);
                        zoom_percent(this.tabs[this.active].page_zoom)
                    })
                })
                .zoom_in(move |_, cx| {
                    let _ = zoom_in.update(cx, |this, cx| this.zoom(Some(1), cx));
                })
                .zoom_out(move |_, cx| {
                    let _ = zoom_out.update(cx, |this, cx| this.zoom(Some(-1), cx));
                })
                .reset_zoom(move |_, cx| {
                    let _ = reset.update(cx, |this, cx| this.zoom(None, cx));
                })
                .clear_site_data(move |window, cx| {
                    let _ = clear.update(cx, |this, cx| this.confirm_clear_site_data(window, cx));
                })
                .reload(move |_, cx| {
                    let _ = reload.update(cx, Self::reload);
                })
                .toggle_picker(move |window, cx| {
                    let _ = picker.update(cx, |this, cx| {
                        if this.tabs[this.active].picking == picking {
                            this.switch_element_pick(window, cx);
                        }
                    });
                });
            browser_action_menu(menu, window, cx, state, actions)
        })
        .anchor(Anchor::TopRight)
    }

    fn select_index(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if index == self.active {
            return;
        }
        if let Some(id) = self.tabs.get(index).map(|tab| tab.id) {
            self.select(id, window, cx);
        }
    }

    fn zoom(&mut self, direction: Option<i8>, cx: &mut Context<Self>) {
        let tab = &mut self.tabs[self.active];
        tab.page_zoom = direction.map_or(1.0, |direction| {
            ala_cef::next_page_zoom_factor(tab.page_zoom, direction)
        });
        tab.view.set_page_zoom(tab.page_zoom);
        if std::mem::take(&mut tab.picking) && !self.start_element_pick(cx) {
            self.tabs[self.active].view.cancel_element_pick();
        }
        cx.notify();
    }

    pub(super) fn tabs_open(&self) -> bool {
        self.compact && self.tabs_sheet
    }

    pub(super) fn open_tabs(&mut self, cx: &mut Context<Self>) {
        if self.writable(cx) {
            self.tabs_sheet = true;
            cx.notify();
        }
    }

    pub(super) fn start_pick(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.tabs[self.active].picking {
            self.toggle_element_pick(window, cx);
        }
    }

    pub(super) fn end_pick(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.tabs[self.active].picking {
            self.toggle_element_pick(window, cx);
        }
    }

    pub(super) fn address_field(&self) -> Option<Entity<InputState>> {
        self.address_open.then(|| self.address.clone())
    }

    pub(super) fn open_address(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.writable(cx) {
            return;
        }
        self.address_open = true;
        let url = &self.tabs[self.active].url;
        let value = if url == "about:blank" {
            String::new()
        } else {
            url.clone()
        };
        self.address
            .update(cx, |input, cx| input.set_value(value, window, cx));
        let input = self.address.clone();
        cx.on_next_frame(window, move |_, window, cx| {
            input.update(cx, |input, cx| input.focus(window, cx));
            cx.on_next_frame(window, |_, window, cx| {
                window.dispatch_action(Box::new(SelectAll), cx);
            });
        });
        window.request_virtual_keyboard();
        cx.notify();
    }

    pub(super) fn close_address(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !std::mem::take(&mut self.address_open) {
            return;
        }
        self.focus.focus(window, cx);
        window.dismiss_virtual_keyboard();
        self.update_address(window, cx);
        cx.notify();
    }

    pub(super) fn clear_address(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.address
            .update(cx, |input, cx| input.set_value("", window, cx));
        cx.notify();
    }

    fn publish(&mut self, cx: &mut Context<Self>) {
        if !self.writable(cx) {
            return;
        }
        let descriptor = BrowserDescriptor {
            tabs: self.tabs.iter().map(|tab| tab.url.clone()).collect(),
            active_tab: self.active,
            profile: self.profile_name.clone(),
        };
        if &descriptor == self.pending.back().unwrap_or(&self.received) {
            return;
        }
        self.pending.push_back(descriptor.clone());
        let mut args = vec![
            "-t".into(),
            self.pane.to_string(),
            "-a".into(),
            self.active.to_string(),
            "--".into(),
        ];
        args.extend(descriptor.tabs);
        self.connection.update(cx, |connection, cx| {
            connection.command("set-browser-tabs", args, cx);
        });
    }
}

impl Focusable for BrowserPane {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl BrowserPane {
    fn tab_row(&self, writable: bool, cx: &mut Context<Self>) -> (BrowserTabStrip, ala::Div) {
        let tabs = self
            .tabs
            .iter()
            .map(|tab| BrowserTabInfo::new(tab.id, tab_title(tab).to_owned(), tab.url.clone()))
            .collect();
        let pane = self.pane;
        let (can_drag, drag_title) = {
            let snapshot = self.connection.read(cx).core.snapshot();
            let pane_window = snapshot
                .sessions
                .iter()
                .flat_map(|session| &session.windows)
                .find(|window| window.panes.contains_key(&pane));
            (
                writable
                    && self.header_drag_handler.is_some()
                    && pane_window.is_some_and(|window| {
                        window.zoomed_pane.is_none() && window.panes.len() > 1
                    }),
                pane_window
                    .and_then(|window| window.panes.get(&pane))
                    .map_or_else(|| "Browser".to_owned(), zz_client::navigation::pane_label),
            )
        };
        let place = zz_client::floating::pane_state(self.connection.read(cx).core.snapshot(), pane);
        let floats = place.floats;
        let drag_handler = self.header_drag_handler.clone();
        let mut drag = PaneDragSource::new(
            pane,
            drag_title,
            can_drag || (writable && floats),
            move |drag, window, cx| {
                if let Some(handler) = &drag_handler {
                    handler(drag, window, cx);
                }
            },
        )
        .floating(floats);
        if let Some(handler) = self.header_touch_drag_handler.clone() {
            drag = drag.touch(super::touch_drag_handle(move |event, window, cx| {
                handler(event, window, cx);
            }));
        }
        let weak = cx.weak_entity();
        let close = weak.clone();
        let new = weak.clone();
        let strip = BrowserTabStrip::new(tabs, self.active)
            .on_activate(move |id, window, cx| {
                let _ = weak.update(cx, |this, cx| this.select(id, window, cx));
            })
            .on_close(move |id, window, cx| {
                let _ = close.update(cx, |this, cx| this.close_tab(id, window, cx));
            })
            .on_new_tab(move |window, cx| {
                let _ = new.update(cx, |this, cx| {
                    this.add_tab("about:blank".into(), window, cx);
                });
            })
            .title(drag);
        let actions =
            div()
                .flex()
                .gap_1()
                .children(zz_ui::pane::pane_header_actions(place).into_iter().map(
                    |(action, icon, label)| {
                        let connection = self.connection.clone();
                        let float_rows = self.float_rows_handler.clone();
                        pane_header_icon_button(label, icon, writable, cx)
                            .tooltip(label)
                            .on_click(move |_, _, cx| {
                                let rows = float_rows.as_ref().and_then(|rows| rows(cx));
                                connection.update(cx, |connection, cx| {
                                    for command in action.commands(pane, rows) {
                                        connection.invoke(command, cx);
                                    }
                                });
                            })
                    },
                ));
        (strip, actions)
    }

    fn tab_drawer(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let tabs: Vec<TabRow> = self
            .tabs
            .iter()
            .enumerate()
            .map(|(index, tab)| TabRow {
                id: tab.id,
                title: tab_title(tab).to_owned().into(),
                host: host(&tab.url).to_owned().into(),
                current: index == self.active,
            })
            .collect();
        let select = cx.weak_entity();
        let close = cx.weak_entity();
        let new = cx.weak_entity();
        let dismiss = cx.weak_entity();
        floating_sheet(
            tabs_sheet(
                ("ios-browser-tabs-sheet", self.pane.0),
                &tabs,
                move |id, window, cx| {
                    let _ = select.update(cx, |this, cx| this.select(id, window, cx));
                },
                move |id, window, cx| {
                    let _ = close.update(cx, |this, cx| this.close_tab(id, window, cx));
                },
                move |window, cx| {
                    let _ = new.update(cx, |this, cx| {
                        this.add_tab("about:blank".into(), window, cx);
                    });
                },
                sheet_inset(window),
                move |_, cx| {
                    let _ = dismiss.update(cx, |this, cx| {
                        this.tabs_sheet = false;
                        cx.notify();
                    });
                },
                cx,
            ),
            window,
        )
    }
}

impl Render for BrowserPane {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let writable = self.writable(cx);
        let pane = self.pane;
        let tab = &self.tabs[self.active];
        let header = (!self.compact).then(|| {
            let address = browser_address(&self.address, self.site_controls(cx), window, cx);
            let picker = div()
                .relative()
                .flex_none()
                .child(
                    browser_toolbar_button(
                        cx,
                        "ios-browser-element-picker",
                        IconName::Inspector,
                        if tab.picking {
                            "Cancel element picker"
                        } else {
                            "Pick an element"
                        },
                        !writable || tab.loading,
                        tab.picking,
                    )
                    .on_click(
                        cx.listener(|this, _, window, cx| this.toggle_element_pick(window, cx)),
                    ),
                )
                .child(super::touch_drag_handle({
                    let pane = cx.weak_entity();
                    move |event, window, cx| {
                        let claimed = pane
                            .update(cx, |this, cx| this.held_pick(event, window, cx))
                            .unwrap_or(false);
                        if claimed && event.phase == TouchPhase::Started {
                            window.prevent_default();
                        }
                    }
                }));
            let toolbar = BrowserToolbar::new(
                browser_toolbar_button(
                    cx,
                    "ios-browser-back",
                    IconName::ArrowLeft,
                    "Back",
                    !writable || !tab.back,
                    false,
                )
                .on_click(cx.listener(|this, _, _, _| this.go_back())),
                browser_toolbar_button(
                    cx,
                    "ios-browser-forward",
                    IconName::ArrowRight,
                    "Forward",
                    !writable || !tab.forward,
                    false,
                )
                .on_click(cx.listener(|this, _, _, _| this.go_forward())),
                browser_toolbar_button(
                    cx,
                    "ios-browser-reload",
                    if tab.loading {
                        IconName::Xmark
                    } else {
                        IconName::Redo2
                    },
                    "Reload",
                    !writable,
                    false,
                )
                .on_click(cx.listener(|this, _, _, cx| this.reload(cx))),
                address,
                picker,
                self.more_menu(cx),
            );
            let (strip, actions) = self.tab_row(writable, cx);
            BrowserHeader::new(self.active_pane, strip, actions, toolbar)
        });
        let compact = self.compact;
        let drawer = compact && self.tabs_sheet;
        let covered_bottom = self.covered_bottom;
        let hole = self.pill_hole;
        let line = self.tabs[self.active].progress.clamp(0.0, 1.0);
        let line = (compact && self.tabs[self.active].loading && line > 0.0).then_some(line);
        let reach = hole.and_then(|_| pill_backdrop_reach(cx));
        let tab_drawer = drawer.then(|| self.tab_drawer(window, cx));
        let tab = &self.tabs[self.active];
        let view = tab.view.clone();
        let backdrop = self
            .backdrop
            .clone()
            .filter(|(id, _, _, _)| reach.is_some() && *id == tab.id);
        let still = self
            .still
            .clone()
            .filter(|(id, _)| self.frozen && *id == tab.id)
            .map(|(_, image)| image);
        let visible = self.visible && self.profile.available();
        let theme = cx.theme();
        let edge = PaneLook::global(cx).edge_width(theme);
        let radius = (self.radii.bottom_left.min(self.radii.bottom_right) - edge).max(px(0.0));
        let waiting = !self.profile.available();
        let error = tab.error.clone().filter(|_| !waiting);
        let failed = error.is_some();
        let blank = !failed && !waiting && tab.url == "about:blank";
        let retry = self.writable(cx).then(|| {
            Button::new("ios-browser-retry")
                .primary()
                .when(!compact, |retry| {
                    retry
                        .on(penna::blocks::feedback::alert_ground(cx))
                        .label("Try again")
                })
                .on_click(cx.listener(|this, _, _, cx| this.reload(cx)))
        });
        let start_page = (blank && !compact).then(|| self.start_page(cx));
        let suggestions = self.suggestions_panel(cx);
        let suggestions_hole = self.suggestions_hole.clone();
        let tab = &self.tabs[self.active];
        let picking = tab.picking && !tab.held;
        let pick_status = if compact {
            None
        } else if picking {
            Some("Tap an element to copy its context and screenshot".to_owned())
        } else {
            tab.pick_status.clone()
        };
        div()
            .id(("ios-browser", pane.0))
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &OmniboxNext, window, cx| {
                this.step_suggestion(1, window, cx);
            }))
            .on_action(cx.listener(|this, _: &OmniboxPrevious, window, cx| {
                this.step_suggestion(-1, window, cx);
            }))
            .on_action(cx.listener(|this, _: &OmniboxDelete, window, cx| {
                this.delete_suggestion(window, cx);
            }))
            .on_action(cx.listener(|this, _: &OmniboxEscape, window, cx| {
                this.escape_omnibox(window, cx);
            }))
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .overflow_hidden()
            .rounded_tl(self.radii.top_left)
            .rounded_tr(self.radii.top_right)
            .text_color(cx.theme().foreground)
            .children(header)
            .when_some(pick_status, |pane, status| {
                pane.child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .min_h(px(32.0))
                        .px_2()
                        .gap_2()
                        .child(Label::new(TextStyle::Lead, status).muted())
                        .when(picking, |row| {
                            row.child(
                                Button::new("ios-browser-cancel-picker")
                                    .label("Cancel")
                                    .ghost()
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.toggle_element_pick(window, cx);
                                    })),
                            )
                        }),
                )
            })
            .when(waiting, |pane| {
                pane.child(
                    Label::new(TextStyle::Lead, "Waiting for the host connection…")
                        .muted()
                        .px_2()
                        .py_1(),
                )
            })
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .on_prepaint(move |bounds, window, _| {
                        let bounds = Bounds::new(
                            point(bounds.left() + edge, bounds.top()),
                            size(
                                (bounds.size.width - edge * 2.0).max(px(0.0)),
                                (bounds.size.height - edge).max(px(0.0)),
                            ),
                        );
                        let backdrop = reach.map(|reach| {
                            let top = (bounds.bottom() - covered_bottom - reach).max(bounds.top());
                            Bounds::from_corners(point(bounds.left(), top), bounds.bottom_right())
                        });
                        let strip = line.map(|line| {
                            (
                                Bounds::new(
                                    bounds.origin,
                                    size(bounds.size.width * line, px(LOAD_LINE)),
                                ),
                                px(0.0),
                            )
                        });
                        view.mount(
                            BrowserMount {
                                bounds,
                                radius,
                                covered_bottom,
                                holes: hole
                                    .into_iter()
                                    .chain(strip)
                                    .chain(suggestions_hole.get())
                                    .collect(),
                                backdrop,
                            },
                            window,
                        );
                        view.set_visible(
                            visible
                                && !failed
                                && !drawer
                                && !blank
                                && bounds.size.width > px(0.0)
                                && bounds.size.height > px(0.0),
                        );
                    })
                    .when_some(
                        backdrop.filter(|_| visible && !failed && !drawer && !blank),
                        |page, (_, bounds, image, _)| {
                            page.child(
                                canvas(
                                    |_, _, _| {},
                                    move |area, (), window, _| {
                                        let at = Bounds::new(
                                            point(area.left() + edge, bounds.origin.y),
                                            bounds.size,
                                        );
                                        let _ = window.paint_image(
                                            at,
                                            at,
                                            Corners::default(),
                                            image,
                                            0,
                                            false,
                                        );
                                    },
                                )
                                .absolute()
                                .size_full(),
                            )
                        },
                    )
                    .when_some(still.filter(|_| !failed && !blank), |page, image| {
                        page.child(
                            canvas(
                                |_, _, _| {},
                                move |area, (), window, _| {
                                    let at = Bounds::new(
                                        point(area.left() + edge, area.top()),
                                        size(
                                            (area.size.width - edge * 2.0).max(px(0.0)),
                                            (area.size.height - edge).max(px(0.0)),
                                        ),
                                    );
                                    let _ = window.paint_image(
                                        at,
                                        at,
                                        Corners {
                                            top_left: px(0.0),
                                            top_right: px(0.0),
                                            bottom_left: radius,
                                            bottom_right: radius,
                                        },
                                        image,
                                        0,
                                        false,
                                    );
                                },
                            )
                            .absolute()
                            .size_full(),
                        )
                    })
                    .children(start_page)
                    .when_some(error, |page, error| {
                        if compact {
                            return page.child(
                                div()
                                    .absolute()
                                    .inset_0()
                                    .pb(covered_bottom)
                                    .child(failed_page(error, retry)),
                            );
                        }
                        let mut panel = BrowserErrorPanel::new(error);
                        if let Some(retry) = retry {
                            panel = panel.retry(retry);
                        }
                        page.child(browser_start_surface(panel))
                    })
                    .when_some(line.filter(|_| visible && !failed), |page, line| {
                        page.child(
                            div().absolute().top_0().left(edge).right(edge).child(
                                Progress::new(("ios-browser-load", pane.0), line)
                                    .line()
                                    .label("Loading"),
                            ),
                        )
                    }),
            )
            .children(tab_drawer)
            .children(suggestions)
    }
}

fn average_under(image: &RenderImage, strip: Bounds<Pixels>, hole: Bounds<Pixels>) -> Option<Hsla> {
    let bytes = image.as_bytes(0)?;
    let size = image.size(0);
    let (width, height) = (
        usize::try_from(size.width.0).ok()?,
        usize::try_from(size.height.0).ok()?,
    );
    let area = hole.intersect(&strip);
    let column = |at: Pixels| {
        ((f32::from(at - strip.left()) / f32::from(strip.size.width) * width as f32) as usize)
            .min(width)
    };
    let row = |at: Pixels| {
        ((f32::from(at - strip.top()) / f32::from(strip.size.height) * height as f32) as usize)
            .min(height)
    };
    let mut sum = [0u64; 3];
    let mut count = 0u64;
    for y in (row(area.top())..row(area.bottom())).step_by(2) {
        for x in (column(area.left())..column(area.right())).step_by(2) {
            let pixel = bytes.get((y * width + x) * 4..(y * width + x) * 4 + 3)?;
            sum[0] += u64::from(pixel[2]);
            sum[1] += u64::from(pixel[1]);
            sum[2] += u64::from(pixel[0]);
            count += 1;
        }
    }
    let channel = |sum: u64| sum as f32 / count as f32 / 255.0;
    (count > 0).then(|| {
        Hsla::from(Rgba {
            r: channel(sum[0]),
            g: channel(sum[1]),
            b: channel(sum[2]),
            a: 1.0,
        })
    })
}

fn tab_title(tab: &Tab) -> &str {
    if !tab.title.is_empty() {
        &tab.title
    } else if tab.url == "about:blank" {
        "New tab"
    } else {
        &tab.url
    }
}

pub(super) fn host(url: &str) -> &str {
    if url == "about:blank" {
        return "";
    }
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    rest.split(['/', '?', '#']).next().unwrap_or(rest)
}

fn site_host(url: &str) -> Option<String> {
    url::Url::parse(url)
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
}

fn http_origin(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://")
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "page zoom stays on the 25% to 500% ladder"
)]
fn zoom_percent(factor: f64) -> u16 {
    (factor * 100.0).round() as u16
}

fn sound_script(muted: bool) -> String {
    format!("{SOUND_SCRIPT}({muted});")
}

const SOUND_SCRIPT: &str = r"(muted => {
  const sound = globalThis.__zzSound ??= (() => {
    const state = { muted: false, held: new Set() };
    state.hush = media => {
      if (state.muted && media instanceof HTMLMediaElement && !media.muted) {
        state.held.add(media);
        media.muted = true;
      }
    };
    const play = HTMLMediaElement.prototype.play;
    HTMLMediaElement.prototype.play = function (...args) {
      state.hush(this);
      return play.apply(this, args);
    };
    for (const type of ['play', 'playing', 'volumechange']) {
      document.addEventListener(type, event => state.hush(event.target), true);
    }
    state.contexts = new Set();
    state.quiet = context => {
      if (state.muted && context.state !== 'closed') {
        state.held.add(context);
        context.suspend();
      }
    };
    const Audio = globalThis.AudioContext;
    if (Audio) {
      const resume = Audio.prototype.resume;
      Audio.prototype.resume = function (...args) {
        if (state.muted) {
          state.held.add(this);
          return Promise.resolve();
        }
        return resume.apply(this, args);
      };
      globalThis.AudioContext = new Proxy(Audio, {
        construct(target, args, newTarget) {
          const context = Reflect.construct(target, args, newTarget);
          state.contexts.add(context);
          state.quiet(context);
          return context;
        },
      });
    }
    return state;
  })();
  sound.muted = muted;
  if (muted) {
    document.querySelectorAll('audio, video').forEach(sound.hush);
    sound.contexts.forEach(context => {
      if (context.state === 'closed') {
        sound.contexts.delete(context);
      } else if (context.state === 'running') {
        sound.quiet(context);
      }
    });
  } else {
    sound.held.forEach(held => {
      if (held instanceof HTMLMediaElement) {
        held.muted = false;
      } else if (held.state !== 'closed') {
        held.resume();
      }
    });
    sound.held.clear();
  }
})";

fn normalized(descriptor: &BrowserDescriptor) -> BrowserDescriptor {
    let mut descriptor = descriptor.clone();
    if descriptor.tabs.is_empty() {
        descriptor.tabs.push("about:blank".into());
    }
    descriptor.active_tab = descriptor.active_tab.min(descriptor.tabs.len() - 1);
    descriptor
}
