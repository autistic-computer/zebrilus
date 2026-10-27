use std::{rc::Rc, sync::Arc};

use crate::{
    ActiveTheme as _, Colorize as _, Disableable as _, Edge, Icon, IconName, PaintState,
    Selectable as _, Sizable as _, StatefulSurfaceExt as _, StyledExt as _, SurfaceRole,
    badge::Badge,
    button::{Button, ButtonVariants as _},
    feedback::{alert_actions, inline_alert},
    input::{Input, InputContentType, InputState, field_chrome},
    menu::{PopupMenu, PopupMenuItem},
    separator::Separator,
    tooltip::Tooltip,
};
use zz_gpui::{
    AnyElement, App, Context, Entity, Focusable as _, IntoElement, MouseButton, ParentElement as _,
    Pixels, RenderOnce, SharedString, Styled as _, StyledImage as _, Window, canvas, deferred, div,
    prelude::*, px,
};

pub const BROWSER_OMNIBOX_KEY_CONTEXT: &str = "BrowserOmnibox";
const BROWSER_CONTROL_HEIGHT: f32 = 28.0;
const COMPACT_BAR_HEIGHT: f32 = 52.0;
const COMPACT_CONTROL_HEIGHT: f32 = 40.0;
/// The header's first row: the title, the tabs and New tab, the tabs as
/// tall as the address bar and as far in from the pane's top as a pane
/// header's controls.
const BROWSER_TAB_ROW_HEIGHT: f32 = BROWSER_CONTROL_HEIGHT + 2.0 * crate::CHROME_GAP;
const BROWSER_TAB_MAX_WIDTH: f32 = 180.0;
/// A tab narrowed to its favicon alone.
const BROWSER_TAB_MIN_WIDTH: f32 = 36.0;
/// Below this width a tab drops its label and shows only its favicon.
const BROWSER_TAB_LABEL_WIDTH: f32 = 84.0;
const BROWSER_TAB_GAP: f32 = 4.0;
/// Room around the tabs, inside the row's clip, for the active tab's rim.
const BROWSER_TAB_RIM_ROOM: f32 = 2.0;
const SITE_CONTROLS_INSET: Pixels = px(1.5);
const ADDRESS_EDGE_WIDTH: Pixels = px(1.0);

#[derive(IntoElement)]
pub struct BrowserHeader {
    active: bool,
    tabs: AnyElement,
    actions: AnyElement,
    toolbar: BrowserToolbar,
}

impl BrowserHeader {
    /// The tab row, then the toolbar with the header's inset below it.
    pub const HEIGHT: Pixels =
        px(BROWSER_TAB_ROW_HEIGHT + BROWSER_CONTROL_HEIGHT + crate::CHROME_GAP);

    pub fn new(
        active: bool,
        tabs: impl IntoElement,
        actions: impl IntoElement,
        toolbar: BrowserToolbar,
    ) -> Self {
        Self {
            active,
            tabs: tabs.into_any_element(),
            actions: actions.into_any_element(),
            toolbar,
        }
    }
}

impl RenderOnce for BrowserHeader {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let show_separator = self.toolbar.show_separator;
        div()
            .id("browser-pane-header")
            .group("browser-pane-header")
            .relative()
            .flex()
            .flex_col()
            .w_full()
            .h(Self::HEIGHT)
            .flex_none()
            .surface(crate::roles::PANE_HEADER, cx)
            .pb(px(crate::CHROME_GAP))
            .child(
                div()
                    .id("browser-tab-row")
                    .debug_selector(|| "browser-tab-row".into())
                    .flex()
                    .items_center()
                    .h(px(BROWSER_TAB_ROW_HEIGHT))
                    .flex_none()
                    .gap_2()
                    .px(px(crate::CHROME_GAP))
                    .font_family(cx.theme().font_family.clone())
                    .text_size(crate::rems_from_px(13.0))
                    .line_height(px(16.0))
                    .text_color(cx.theme().foreground.muted())
                    .child(self.tabs)
                    .child(
                        div()
                            .flex_none()
                            .when(!self.active, |actions| {
                                actions
                                    .invisible()
                                    .group_hover("browser-pane-header", zz_gpui::Styled::visible)
                            })
                            .child(self.actions),
                    ),
            )
            .child(self.toolbar.show_separator(false))
            .when(show_separator, |header| {
                header.child(
                    Separator::horizontal()
                        .surface_role(crate::roles::BAR_RULE)
                        .absolute()
                        .bottom_0()
                        .h(crate::RULE_WIDTH),
                )
            })
    }
}

/// The phone's browser bar: one row with the address field, the tabs button
/// and the element picker. Back and forward are edge swipes on the page, and
/// closing the pane is in the pill's menu.
#[derive(IntoElement)]
pub struct CompactBrowserBar {
    address: AnyElement,
    tabs: AnyElement,
    picker: AnyElement,
}

impl CompactBrowserBar {
    pub const HEIGHT: Pixels = px(COMPACT_BAR_HEIGHT);
    pub const CONTROL: Pixels = px(COMPACT_CONTROL_HEIGHT);

    pub fn new(
        address: impl IntoElement,
        tabs: impl IntoElement,
        picker: impl IntoElement,
    ) -> Self {
        Self {
            address: address.into_any_element(),
            tabs: tabs.into_any_element(),
            picker: picker.into_any_element(),
        }
    }
}

impl RenderOnce for CompactBrowserBar {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        div()
            .id("compact-browser-bar")
            .debug_selector(|| "compact-browser-bar".into())
            .flex()
            .flex_none()
            .items_center()
            .gap(px(8.0))
            .w_full()
            .h(Self::HEIGHT)
            .px(px(12.0))
            .surface(crate::roles::PANE_HEADER, cx)
            .text_color(cx.theme().foreground)
            .child(self.address)
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(px(4.0))
                    .child(self.tabs)
                    .child(self.picker),
            )
    }
}

#[derive(IntoElement)]
pub struct BrowserToolbar {
    back: AnyElement,
    forward: Option<AnyElement>,
    reload: Option<AnyElement>,
    address: AnyElement,
    picker: AnyElement,
    more: AnyElement,
    show_separator: bool,
}

impl BrowserToolbar {
    /// Fixed toolbar height. A host mounting the toolbar behind a cached-view
    /// boundary must give the wrapper this exact height.
    pub const HEIGHT: Pixels = px(BROWSER_CONTROL_HEIGHT);

    pub fn new(
        back: impl IntoElement,
        forward: impl IntoElement,
        reload: impl IntoElement,
        address: impl IntoElement,
        picker: impl IntoElement,
        more: impl IntoElement,
    ) -> Self {
        Self {
            back: back.into_any_element(),
            forward: Some(forward.into_any_element()),
            reload: Some(reload.into_any_element()),
            address: address.into_any_element(),
            picker: picker.into_any_element(),
            more: more.into_any_element(),
            show_separator: true,
        }
    }

    #[must_use]
    pub fn show_separator(mut self, show_separator: bool) -> Self {
        self.show_separator = show_separator;
        self
    }

    #[must_use]
    pub fn back_only(mut self) -> Self {
        self.forward = None;
        self.reload = None;
        self
    }
}

impl RenderOnce for BrowserToolbar {
    fn render(self, _: &mut zz_gpui::Window, cx: &mut App) -> impl IntoElement {
        div()
            .id("browser-navigation-row")
            .debug_selector(|| "browser-navigation-row".into())
            .h(Self::HEIGHT)
            .w_full()
            .flex_none()
            .flex()
            .items_center()
            .gap_1()
            .px(px(crate::CHROME_GAP))
            .when(self.show_separator, |toolbar| {
                toolbar.rule_on(crate::roles::BAR_RULE, Edge::Bottom, cx)
            })
            .text_color(cx.theme().foreground)
            .child(
                browser_toolbar_cluster()
                    .child(self.back)
                    .children(self.forward)
                    .children(self.reload),
            )
            .child(self.address)
            .child(
                browser_toolbar_cluster()
                    .child(self.picker)
                    .child(self.more),
            )
    }
}

fn browser_toolbar_cluster() -> zz_gpui::Div {
    div().flex().flex_none().items_center().gap_1()
}

pub fn browser_address(
    address: &Entity<InputState>,
    site_controls: impl IntoElement,
    window: &Window,
    cx: &App,
) -> zz_gpui::Stateful<zz_gpui::Div> {
    let focus_handle = address.read(cx).focus_handle(cx);
    let role = crate::roles::ADDRESS_BAR;
    let state = if focus_handle.is_focused(window) {
        PaintState::Focused
    } else {
        PaintState::Rest
    };
    let input = Input::new(address)
        .small()
        .h(px(BROWSER_CONTROL_HEIGHT))
        .flex_1()
        .min_w_0()
        .px_2()
        .py_0()
        .text_size(crate::rems_from_px(13.0))
        .line_height(px(16.0))
        .appearance(false)
        .content_type(InputContentType::Url);

    let shell = div()
        .id(("browser-address", address.entity_id()))
        .debug_selector(|| "browser-address".into())
        .track_focus(&focus_handle)
        .key_context(BROWSER_OMNIBOX_KEY_CONTEXT)
        .h(px(BROWSER_CONTROL_HEIGHT))
        .flex()
        .flex_1()
        .min_w_0()
        .items_center();
    field_chrome(shell, role, state, true, true, cx)
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            focus_handle.focus(window, cx);
            cx.stop_propagation();
        })
        .child(
            div()
                .id(("browser-site-controls-slot", address.entity_id()))
                .flex_none()
                .pl(SITE_CONTROLS_INSET)
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(site_controls),
        )
        .child(input)
}

pub fn browser_site_controls_button(cx: &App) -> Button {
    let address = cx
        .theme()
        .layer(crate::roles::ADDRESS_BAR, PaintState::Rest)
        .radius;
    Button::compact_icon("browser-site-controls", IconName::SiteControls)
        .surface_role(crate::SurfaceRole::FLAT_ICON_BUTTON)
        .rounded(if address >= zz_gpui::FULL_CORNER_RADIUS {
            address
        } else {
            (address - SITE_CONTROLS_INSET - ADDRESS_EDGE_WIDTH).max(px(0.0))
        })
        .text_color(cx.theme().foreground.muted())
        .tooltip("Site controls")
        .debug_selector(|| "browser-site-controls".into())
}

pub struct BrowserSiteMenuState {
    pub site: SharedString,
    pub connection_secure: Option<bool>,
    pub audio_muted: Option<bool>,
    pub can_clear_site_data: bool,
}

pub fn browser_site_menu(
    menu: PopupMenu,
    state: BrowserSiteMenuState,
    toggle_sound: impl Fn(&mut Window, &mut App) + 'static,
    clear_site_data: impl Fn(&mut Window, &mut App) + 'static,
) -> PopupMenu {
    let connection = match state.connection_secure {
        Some(true) => "Connection is secure",
        Some(false) => "Connection is not secure",
        None => "Connection information unavailable",
    };
    menu.min_w(px(260.0))
        .item(PopupMenuItem::label(state.site))
        .separator()
        .item(
            PopupMenuItem::new(connection)
                .icon(IconName::Info)
                .disabled(true),
        )
        .separator()
        .item(
            PopupMenuItem::new("Sound")
                .checked(state.audio_muted == Some(false))
                .disabled(state.audio_muted.is_none())
                .on_click(move |_, window, cx| toggle_sound(window, cx)),
        )
        .separator()
        .item(
            PopupMenuItem::new("Clear cookies and site data…")
                .icon(IconName::HardDrive)
                .disabled(!state.can_clear_site_data)
                .on_click(move |_, window, cx| clear_site_data(window, cx)),
        )
}

#[derive(Clone, PartialEq, Eq)]
pub struct BrowserTabInfo {
    /// Stable per-pane tab id, chosen by the caller.
    pub id: u64,
    /// Short pill text, typically the page host.
    pub label: SharedString,
    /// Pill tooltip, typically the page title or the full URL.
    pub detail: SharedString,
    pub favicon: Option<Arc<[u8]>>,
}

impl BrowserTabInfo {
    pub fn new(id: u64, label: impl Into<SharedString>, detail: impl Into<SharedString>) -> Self {
        Self {
            id,
            label: label.into(),
            detail: detail.into(),
            favicon: None,
        }
    }
    #[must_use]
    pub fn favicon(mut self, favicon: Option<Arc<[u8]>>) -> Self {
        self.favicon = favicon;
        self
    }
}

type BrowserTabAction = Rc<dyn Fn(u64, &mut Window, &mut App)>;

/// A browser pane's tabs, sharing the row's width like a desktop browser's:
/// each as wide as it can be up to its full width, down to its favicon
/// alone, with New tab right after the last one. Given a [`Self::title`], the pane's
/// title leads the row.
#[derive(IntoElement)]
pub struct BrowserTabStrip {
    tabs: Vec<BrowserTabInfo>,
    active: usize,
    on_activate: BrowserTabAction,
    on_close: BrowserTabAction,
    on_new_tab: BrowserMenuAction,
    title: Option<(crate::pane::PaneDragSource, BrowserMenuAction)>,
}

impl BrowserTabStrip {
    pub fn new(tabs: Vec<BrowserTabInfo>, active: usize) -> Self {
        Self {
            tabs,
            active,
            on_activate: Rc::new(|_, _, _| {}),
            on_close: Rc::new(|_, _, _| {}),
            on_new_tab: Rc::new(|_, _| {}),
            title: None,
        }
    }

    /// Leads the row with the pane's title, the way other panes title their
    /// header: dragged by `drag`, its X closes the pane through
    /// `on_close_pane`.
    #[must_use]
    pub fn title(
        mut self,
        drag: crate::pane::PaneDragSource,
        on_close_pane: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.title = Some((drag, Rc::new(on_close_pane)));
        self
    }

    #[must_use]
    pub fn on_activate(mut self, action: impl Fn(u64, &mut Window, &mut App) + 'static) -> Self {
        self.on_activate = Rc::new(action);
        self
    }

    #[must_use]
    pub fn on_close(mut self, action: impl Fn(u64, &mut Window, &mut App) + 'static) -> Self {
        self.on_close = Rc::new(action);
        self
    }

    #[must_use]
    pub fn on_new_tab(mut self, action: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_new_tab = Rc::new(action);
        self
    }
}

impl RenderOnce for BrowserTabStrip {
    fn render(self, window: &mut zz_gpui::Window, cx: &mut App) -> impl IntoElement {
        let Self {
            tabs,
            active,
            on_activate,
            on_close,
            on_new_tab,
            title,
        } = self;
        let title = title.map(|(drag, close_pane)| {
            crate::pane::PaneTitle::new("browser-pane-title", IconName::Globe, "Browser", drag)
                .on_close(move |window, cx| close_pane(window, cx))
        });
        let width = window.use_keyed_state("browser-tab-strip-width", cx, |_, _| None::<Pixels>);
        let count = f32::from(u16::try_from(tabs.len().max(1)).unwrap_or(u16::MAX));
        let narrow = width.read(cx).is_some_and(|width| {
            (width - px(BROWSER_TAB_GAP) * (count - 1.0)) / count < px(BROWSER_TAB_LABEL_WIDTH)
        });
        let measure = canvas(
            move |bounds, window, cx| {
                let tabs = bounds.size.width - px(BROWSER_TAB_RIM_ROOM * 2.0);
                let changed = width.update(cx, |width, _| width.replace(tabs) != Some(tabs));
                if changed {
                    window.request_animation_frame();
                }
            },
            |_, (), _, _| {},
        )
        .absolute()
        .size_full();
        let new_tab = browser_toolbar_button(
            cx,
            "browser-new-tab",
            IconName::Plus,
            "New tab",
            false,
            false,
        )
        .debug_selector(|| "browser-new-tab".into())
        .on_click(move |_, window, cx| on_new_tab(window, cx));
        let closable = tabs.len() > 1;
        div()
            .h_full()
            .flex()
            .flex_1()
            .min_w_0()
            .items_center()
            .gap_2()
            .children(title)
            .child(
                div()
                    .flex()
                    .flex_initial()
                    .min_w_0()
                    .items_center()
                    .gap(px(BROWSER_TAB_GAP))
                    .child(
                        div()
                            .id("browser-tabs")
                            .debug_selector(|| "browser-tabs".into())
                            .relative()
                            .flex()
                            .flex_initial()
                            .min_w_0()
                            .overflow_hidden()
                            .p(px(BROWSER_TAB_RIM_ROOM))
                            .m(px(-BROWSER_TAB_RIM_ROOM))
                            .items_center()
                            .gap(px(BROWSER_TAB_GAP))
                            .child(measure)
                            .children(tabs.into_iter().enumerate().map(|(index, tab)| {
                                browser_tab_pill(
                                    tab,
                                    index == active,
                                    closable,
                                    narrow,
                                    &on_activate,
                                    &on_close,
                                    cx,
                                )
                            })),
                    )
                    .child(new_tab),
            )
    }
}

fn browser_tab_pill(
    tab: BrowserTabInfo,
    active: bool,
    closable: bool,
    narrow: bool,
    on_activate: &BrowserTabAction,
    on_close: &BrowserTabAction,
    cx: &App,
) -> impl IntoElement {
    let BrowserTabInfo {
        id,
        label,
        detail,
        favicon,
    } = tab;
    let activate = Rc::clone(on_activate);
    let close_middle = Rc::clone(on_close);
    let state = if active {
        PaintState::Current
    } else {
        PaintState::Rest
    };
    let favicon = div()
        .flex_none()
        .relative()
        .top(px(0.5))
        .child(browser_favicon(favicon, cx));
    div()
        .id(("browser-tab", id))
        .debug_selector(move || format!("browser-tab-{id}"))
        .group(browser_tab_group(id))
        .h(px(BROWSER_CONTROL_HEIGHT))
        .w(px(BROWSER_TAB_MAX_WIDTH))
        .min_w(px(BROWSER_TAB_MIN_WIDTH))
        .flex()
        .items_center()
        .surface_interactive(crate::roles::BROWSER_TAB, state, cx)
        .when(!active, |tab| {
            tab.text_color(crate::picker::muted_on(
                &cx.theme().layer(crate::roles::BROWSER_TAB, state),
                cx,
            ))
        })
        .map(|tab| {
            if narrow {
                tab.justify_center().child(
                    div()
                        .relative()
                        .flex()
                        .flex_none()
                        .size(px(BROWSER_CONTROL_HEIGHT))
                        .items_center()
                        .justify_center()
                        .child(favicon.when(closable, |favicon| {
                            favicon.group_hover(browser_tab_group(id), zz_gpui::Styled::invisible)
                        }))
                        .when(closable, |slot| {
                            slot.child(
                                div()
                                    .absolute()
                                    .inset_0()
                                    .child(browser_tab_close_button(id, on_close, cx)),
                            )
                        }),
                )
            } else {
                tab.relative()
                    .gap(px(6.0))
                    .pl(px(12.0))
                    .pr(px(if closable {
                        BROWSER_CONTROL_HEIGHT
                    } else {
                        12.0
                    }))
                    .child(favicon)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(crate::rems_from_px(13.0))
                            .line_height(px(16.0))
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_fade(crate::LABEL_FADE)
                            .child(label),
                    )
                    .when(closable, |tab| {
                        tab.child(
                            div()
                                .absolute()
                                .top_0()
                                .bottom_0()
                                .right_0()
                                .flex()
                                .items_center()
                                .child(browser_tab_close_button(id, on_close, cx)),
                        )
                    })
            }
        })
        .cursor_pointer()
        .when(!detail.is_empty(), |this| {
            this.tooltip(move |window, cx| Tooltip::new(detail.clone()).build(window, cx))
        })
        .on_click(move |_, window, cx| activate(id, window, cx))
        .on_mouse_down(MouseButton::Middle, move |_, window, cx| {
            close_middle(id, window, cx);
        })
}

fn browser_tab_group(id: u64) -> SharedString {
    format!("browser-tab-{id}").into()
}

fn browser_tab_close_button(id: u64, on_close: &BrowserTabAction, cx: &App) -> impl IntoElement {
    let close = Rc::clone(on_close);
    div()
        .flex_none()
        .invisible()
        .group_hover(browser_tab_group(id), zz_gpui::Styled::visible)
        .child(
            Button::compact_icon(("browser-tab-close", id), IconName::Xmark)
                .size(crate::rems_from_px(BROWSER_CONTROL_HEIGHT))
                .text()
                .text_color(cx.theme().foreground.muted())
                .debug_selector(move || format!("browser-tab-close-{id}"))
                .on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    close(id, window, cx);
                }),
        )
}

pub fn browser_toolbar_button(
    cx: &App,
    id: &'static str,
    icon: IconName,
    tooltip: &'static str,
    disabled: bool,
    selected: bool,
) -> Button {
    Button::compact_icon(id, icon)
        .surface_role(crate::roles::BROWSER_BUTTON)
        .size(crate::rems_from_px(BROWSER_CONTROL_HEIGHT))
        .when(!disabled, |this| {
            this.text_color(cx.theme().foreground.muted())
        })
        .disabled(disabled)
        .selected(selected)
        .tooltip(tooltip)
}

pub fn browser_start_surface(content: impl IntoElement) -> zz_gpui::Div {
    div()
        .absolute()
        .inset_0()
        .occlude()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .px(px(12.0))
        .child(content)
}

/// Blank-browser hint shown before any recent pages exist.
#[derive(IntoElement)]
pub struct BrowserEmptyHint;

impl RenderOnce for BrowserEmptyHint {
    fn render(self, _: &mut zz_gpui::Window, cx: &mut App) -> impl IntoElement {
        div()
            .max_w(px(440.0))
            .flex()
            .flex_col()
            .items_center()
            .gap(px(10.0))
            .p(px(20.0))
            .child(
                div()
                    .size(px(32.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .surface(crate::roles::BROWSER_EMPTY_ICON, cx)
                    .child(
                        Icon::new(IconName::Globe)
                            .small()
                            .text_color(cx.theme().foreground.muted()),
                    ),
            )
            .child(
                div()
                    .text_center()
                    .text_size(crate::rems_from_px(13.0))
                    .font_semibold()
                    .child("Where to?"),
            )
            .child(
                div()
                    .text_center()
                    .text_size(crate::rems_from_px(11.0))
                    .text_color(cx.theme().foreground.muted())
                    .child("Type a URL to get started. Pages you visit will show up here."),
            )
    }
}

pub fn browser_favicon(png: Option<Arc<[u8]>>, cx: &App) -> AnyElement {
    let foreground = cx.theme().foreground.muted();
    let fallback = move || {
        Icon::new(IconName::Globe)
            .size(px(14.0))
            .text_color(foreground)
            .into_any_element()
    };
    match png {
        Some(png) => zz_gpui::img(Arc::new(zz_gpui::Image::from_bytes(
            zz_gpui::ImageFormat::Png,
            png.to_vec(),
        )))
        .size(px(14.0))
        .flex_none()
        .with_fallback(fallback)
        .with_loading(fallback)
        .into_any_element(),
        None => fallback(),
    }
}

pub fn browser_recent_row(
    id: impl Into<zz_gpui::ElementId>,
    url: impl Into<SharedString>,
    favicon: Option<Arc<[u8]>>,
    cx: &App,
) -> zz_gpui::Stateful<zz_gpui::Div> {
    let url = url.into();
    let height = crate::picker::LIST_ROW_HEIGHT;
    crate::picker::row_surface(
        div().id(id).flex().w_full().h(height).flex_none(),
        crate::roles::BROWSER_SUGGESTION,
        PaintState::Rest,
        height,
        cx,
    )
    .items_center()
    .gap(px(8.0))
    .px(px(12.0))
    .cursor_pointer()
    .text_size(crate::rems_from_px(12.0))
    .child(browser_favicon(favicon, cx))
    .child(
        div()
            .min_w_0()
            .flex_1()
            .whitespace_nowrap()
            .text_ellipsis()
            .overflow_hidden()
            .child(url),
    )
}

pub fn browser_omnibox_panel(rows: Vec<AnyElement>, cx: &App) -> impl IntoElement {
    deferred(
        div()
            .id("browser-omnibox-results")
            .debug_selector(|| "browser-omnibox-results".to_owned())
            .absolute()
            .top(BrowserHeader::HEIGHT + px(4.0))
            .left(px(8.0))
            .right(px(8.0))
            .occlude()
            .overflow_hidden()
            .surface(SurfaceRole::POPOVER, cx)
            .py(px(4.0))
            .list_inset(cx)
            .children(rows),
    )
    .with_priority(2)
}

pub fn browser_omnibox_row(
    index: usize,
    title: impl Into<SharedString>,
    url: impl Into<SharedString>,
    selected: bool,
    favicon: Option<Arc<[u8]>>,
    cx: &App,
) -> zz_gpui::Stateful<zz_gpui::Div> {
    let title = title.into();
    let url = url.into();
    let muted = crate::picker::muted_on(
        &cx.theme().layer(
            SurfaceRole::MENU_ITEM,
            if selected {
                PaintState::Selected
            } else {
                PaintState::Rest
            },
        ),
        cx,
    );
    crate::picker::list_row(
        ("browser-omnibox-suggestion", index),
        px(32.0),
        selected,
        true,
        cx,
    )
    .debug_selector(move || format!("browser-omnibox-suggestion-{index}"))
    .w_full()
    .gap(px(8.0))
    .px(px(8.0))
    .overflow_hidden()
    .text_size(crate::rems_from_px(13.0))
    .line_height(px(16.0))
    .cursor_pointer()
    .child(
        div()
            .flex_none()
            .relative()
            .top(px(0.5))
            .child(browser_favicon(favicon, cx)),
    )
    .child(
        div()
            .min_w_0()
            .flex_1()
            .flex()
            .items_center()
            .gap(px(6.0))
            .when(!title.is_empty(), |this| {
                this.child(
                    div()
                        .min_w_0()
                        .max_w(zz_gpui::relative(0.55))
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .overflow_hidden()
                        .child(title),
                )
                .child(div().flex_none().text_color(muted).child("–"))
            })
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .overflow_hidden()
                    .text_color(muted)
                    .child(url),
            ),
    )
}

#[derive(Clone)]
pub struct BrowserMenuProfile {
    pub id: SharedString,
    pub label: SharedString,
}

impl BrowserMenuProfile {
    pub fn new(id: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
        }
    }
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum BrowserProfileDiscoveryState {
    Loading,
    #[default]
    Ready,
    Failed,
}

#[derive(Clone)]
pub struct BrowserActionMenuState {
    pub current_profile_label: SharedString,
    pub selected_profile: SharedString,
    pub default_profile: SharedString,
    pub profiles: Vec<BrowserMenuProfile>,
    pub profile_discovery: BrowserProfileDiscoveryState,
    pub zoom_percent: u16,
    pub can_import_chrome_data: bool,
    pub can_clear_site_data: bool,
    pub picker_active: bool,
}

type BrowserMenuAction = Rc<dyn Fn(&mut Window, &mut App)>;
type BrowserProfileAction = Rc<dyn Fn(SharedString, &mut Window, &mut App)>;

#[derive(Clone)]
#[must_use]
pub struct BrowserMenuActions {
    open_url: BrowserMenuAction,
    copy_url: BrowserMenuAction,
    switch_profile: BrowserProfileAction,
    refresh_profiles: BrowserMenuAction,
    zoom_in: BrowserMenuAction,
    zoom_out: BrowserMenuAction,
    reset_zoom: BrowserMenuAction,
    zoom_percent: Option<Rc<dyn Fn(&App) -> u16>>,
    import_chrome_data: BrowserProfileAction,
    import_cookies: BrowserMenuAction,
    clear_site_data: BrowserMenuAction,
    reload: BrowserMenuAction,
    toggle_picker: BrowserMenuAction,
    dev_tools: BrowserMenuAction,
}

impl Default for BrowserMenuActions {
    fn default() -> Self {
        let noop: BrowserMenuAction = Rc::new(|_, _| {});
        Self {
            open_url: Rc::clone(&noop),
            copy_url: Rc::clone(&noop),
            switch_profile: Rc::new(|_, _, _| {}),
            refresh_profiles: Rc::clone(&noop),
            zoom_in: Rc::clone(&noop),
            zoom_out: Rc::clone(&noop),
            reset_zoom: Rc::clone(&noop),
            zoom_percent: None,
            import_chrome_data: Rc::new(|_, _, _| {}),
            import_cookies: Rc::clone(&noop),
            clear_site_data: Rc::clone(&noop),
            reload: Rc::clone(&noop),
            toggle_picker: Rc::clone(&noop),
            dev_tools: noop,
        }
    }
}

impl BrowserMenuActions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn open_url(mut self, action: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.open_url = Rc::new(action);
        self
    }

    pub fn copy_url(mut self, action: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.copy_url = Rc::new(action);
        self
    }

    pub fn switch_profile(
        mut self,
        action: impl Fn(SharedString, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.switch_profile = Rc::new(action);
        self
    }

    pub fn refresh_profiles(mut self, action: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.refresh_profiles = Rc::new(action);
        self
    }

    pub fn zoom_in(mut self, action: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.zoom_in = Rc::new(action);
        self
    }

    pub fn zoom_out(mut self, action: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.zoom_out = Rc::new(action);
        self
    }

    pub fn reset_zoom(mut self, action: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.reset_zoom = Rc::new(action);
        self
    }

    pub fn zoom_percent(mut self, read: impl Fn(&App) -> u16 + 'static) -> Self {
        self.zoom_percent = Some(Rc::new(read));
        self
    }

    pub fn import_chrome_data(
        mut self,
        action: impl Fn(SharedString, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.import_chrome_data = Rc::new(action);
        self
    }

    pub fn import_cookies(mut self, action: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.import_cookies = Rc::new(action);
        self
    }

    pub fn clear_site_data(mut self, action: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.clear_site_data = Rc::new(action);
        self
    }

    pub fn reload(mut self, action: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.reload = Rc::new(action);
        self
    }

    pub fn toggle_picker(mut self, action: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.toggle_picker = Rc::new(action);
        self
    }

    pub fn dev_tools(mut self, action: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.dev_tools = Rc::new(action);
        self
    }
}

/// The browser action-menu hierarchy. The caller provides state and callbacks.
// Both by value: their fields move into the item callbacks.
#[allow(clippy::needless_pass_by_value)]
pub fn browser_action_menu(
    menu: PopupMenu,
    window: &mut Window,
    cx: &mut Context<PopupMenu>,
    state: BrowserActionMenuState,
    actions: BrowserMenuActions,
) -> PopupMenu {
    let open_url = Rc::clone(&actions.open_url);
    let copy_url = Rc::clone(&actions.copy_url);
    let menu = menu
        .min_w(px(310.0))
        .item(
            PopupMenuItem::new("Open in default browser")
                .icon(IconName::ExternalLink)
                .on_click(move |_, window, cx| open_url(window, cx)),
        )
        .item(
            PopupMenuItem::new("Copy URL")
                .icon(IconName::Copy)
                .on_click(move |_, window, cx| copy_url(window, cx)),
        )
        .separator();

    let selected_profile = state.selected_profile.clone();
    let default_profile = state.default_profile.clone();
    let profiles = state.profiles.clone();
    let profile_discovery = state.profile_discovery;
    let profile_action = Rc::clone(&actions.switch_profile);
    let refresh_profiles = Rc::clone(&actions.refresh_profiles);
    let menu = menu.submenu_with_icon(
        Some(Icon::new(IconName::User)),
        format!("Profile · {}", state.current_profile_label),
        window,
        cx,
        move |profile_menu, _, _| {
            let default_action = Rc::clone(&profile_action);
            let default_id = default_profile.clone();
            let mut profile_menu = profile_menu
                .min_w(px(310.0))
                .item(
                    PopupMenuItem::new("Default zz profile")
                        .checked(selected_profile == default_profile)
                        .disabled(selected_profile == default_profile)
                        .on_click(move |_, window, cx| {
                            default_action(default_id.clone(), window, cx);
                        }),
                )
                .separator()
                .item(PopupMenuItem::label(
                    "Chrome profiles · isolated zz storage",
                ));

            if profile_discovery == BrowserProfileDiscoveryState::Loading {
                profile_menu = profile_menu
                    .item(PopupMenuItem::new("Finding Chrome profiles…").disabled(true));
            } else if profiles.is_empty()
                && profile_discovery == BrowserProfileDiscoveryState::Ready
            {
                profile_menu = profile_menu
                    .item(PopupMenuItem::new("No Chrome profiles found").disabled(true));
            } else {
                for profile in &profiles {
                    let action = Rc::clone(&profile_action);
                    let id = profile.id.clone();
                    profile_menu = profile_menu.item(
                        PopupMenuItem::new(profile.label.clone())
                            .checked(selected_profile == profile.id)
                            .disabled(selected_profile == profile.id)
                            .on_click(move |_, window, cx| {
                                action(id.clone(), window, cx);
                            }),
                    );
                }
            }
            if profile_discovery == BrowserProfileDiscoveryState::Failed {
                let refresh_profiles = Rc::clone(&refresh_profiles);
                profile_menu = profile_menu.separator().item(
                    PopupMenuItem::new("Retry Chrome profile discovery")
                        .icon(IconName::Redo2)
                        .on_click(move |_, window, cx| refresh_profiles(window, cx)),
                );
            }
            profile_menu
        },
    );

    let zoom_in = Rc::clone(&actions.zoom_in);
    let zoom_out = Rc::clone(&actions.zoom_out);
    let reset_zoom = Rc::clone(&actions.reset_zoom);
    let import_chrome_data = Rc::clone(&actions.import_chrome_data);
    let import_cookies = Rc::clone(&actions.import_cookies);
    let clear_site_data = Rc::clone(&actions.clear_site_data);
    let reload = Rc::clone(&actions.reload);
    let toggle_picker = Rc::clone(&actions.toggle_picker);
    let dev_tools = Rc::clone(&actions.dev_tools);
    let zoom_percent = actions.zoom_percent.clone();
    let menu = menu.item(PopupMenuItem::stepper(
        "Page zoom",
        move |cx| {
            format!(
                "{}%",
                zoom_percent
                    .as_ref()
                    .map_or(state.zoom_percent, |read| read(cx))
            )
            .into()
        },
        move |window, cx| zoom_out(window, cx),
        move |window, cx| reset_zoom(window, cx),
        move |window, cx| zoom_in(window, cx),
    ));

    let import_profiles = state.profiles.clone();
    let import_profile_discovery = state.profile_discovery;
    let can_import_chrome_data = state.can_import_chrome_data;
    let refresh_profiles = Rc::clone(&actions.refresh_profiles);
    let menu = menu.separator().submenu_with_icon(
        Some(Icon::new(IconName::Globe)),
        "Import Chrome data",
        window,
        cx,
        move |import_menu, _, _| {
            let mut import_menu = import_menu
                .min_w(px(310.0))
                .item(PopupMenuItem::label("Cookies and history · source profile"));
            if !can_import_chrome_data {
                return import_menu
                    .item(PopupMenuItem::new("Not supported on this platform").disabled(true));
            }
            if import_profile_discovery == BrowserProfileDiscoveryState::Loading {
                import_menu =
                    import_menu.item(PopupMenuItem::new("Finding Chrome profiles…").disabled(true));
            } else if import_profiles.is_empty()
                && import_profile_discovery == BrowserProfileDiscoveryState::Ready
            {
                import_menu =
                    import_menu.item(PopupMenuItem::new("No Chrome profiles found").disabled(true));
            } else {
                for profile in &import_profiles {
                    let action = Rc::clone(&import_chrome_data);
                    let id = profile.id.clone();
                    import_menu = import_menu.item(
                        PopupMenuItem::new(profile.label.clone()).on_click(move |_, window, cx| {
                            action(id.clone(), window, cx);
                        }),
                    );
                }
            }
            if import_profile_discovery == BrowserProfileDiscoveryState::Failed {
                let refresh_profiles = Rc::clone(&refresh_profiles);
                import_menu = import_menu.separator().item(
                    PopupMenuItem::new("Retry Chrome profile discovery")
                        .icon(IconName::Redo2)
                        .on_click(move |_, window, cx| refresh_profiles(window, cx)),
                );
            }
            import_menu
        },
    );

    menu.item(
        PopupMenuItem::new("Import cookie file…")
            .icon(IconName::File)
            .on_click(move |_, window, cx| import_cookies(window, cx)),
    )
    .item(
        PopupMenuItem::new("Clear site data…")
            .icon(IconName::Xmark)
            .disabled(!state.can_clear_site_data)
            .on_click(move |_, window, cx| clear_site_data(window, cx)),
    )
    .separator()
    .item(
        PopupMenuItem::new("Reload")
            .icon(IconName::Redo2)
            .on_click(move |_, window, cx| reload(window, cx)),
    )
    .item(
        PopupMenuItem::new(if state.picker_active {
            "Cancel element picker"
        } else {
            "Pick an element"
        })
        .icon(IconName::Inspector)
        .on_click(move |_, window, cx| toggle_picker(window, cx)),
    )
    .item(
        PopupMenuItem::new("Developer tools")
            .icon(IconName::SquareTerminal)
            .on_click(move |_, window, cx| dev_tools(window, cx)),
    )
}

/// Inline alert shown in place of a page that failed. The caller supplies the optional retry control.
#[derive(IntoElement)]
pub struct BrowserErrorPanel {
    message: SharedString,
    retry: Option<AnyElement>,
}

impl BrowserErrorPanel {
    pub fn new(message: impl Into<SharedString>) -> Self {
        Self {
            message: message.into(),
            retry: None,
        }
    }

    #[must_use]
    pub fn retry(mut self, retry: impl IntoElement) -> Self {
        self.retry = Some(retry.into_any_element());
        self
    }
}

impl RenderOnce for BrowserErrorPanel {
    fn render(self, _: &mut zz_gpui::Window, cx: &mut App) -> impl IntoElement {
        inline_alert(Some("This page couldn’t load".into()), self.message, cx)
            .id("browser-error")
            .max_w(px(360.0))
            .children(self.retry.map(|retry| alert_actions(cx).child(retry)))
    }
}

/// Status pill overlaid while the browser element picker is active.
#[derive(IntoElement)]
pub struct BrowserPickStatus {
    message: SharedString,
}

impl BrowserPickStatus {
    pub fn new(message: impl Into<SharedString>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl RenderOnce for BrowserPickStatus {
    fn render(self, _: &mut zz_gpui::Window, _: &mut App) -> impl IntoElement {
        div()
            .absolute()
            .left(px(16.0))
            .right(px(16.0))
            .bottom(px(14.0))
            .flex()
            .justify_center()
            .child(
                Badge::empty()
                    .surface_role(crate::roles::PANE_CHIP)
                    .font_weight(zz_gpui::FontWeight::NORMAL)
                    .max_w_full()
                    .h(px(28.0))
                    .px(px(10.0))
                    .text_size(px(12.0))
                    .line_height(px(16.0))
                    .gap(px(6.0))
                    .child(
                        div()
                            .flex_none()
                            .relative()
                            .top(px(0.5))
                            .child(Icon::new(IconName::Inspector).size(px(12.0))),
                    )
                    .child(div().min_w_0().child(self.message)),
            )
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use zz_gpui::{Context, Modifiers, Render, TestAppContext, VisualTestContext, point};

    use super::*;
    use crate::menu::DropdownMenu as _;

    struct BrowserTabStripTest {
        width: f32,
        active: usize,
        activated: Arc<Mutex<Vec<u64>>>,
        closed: Arc<Mutex<Vec<u64>>>,
    }

    struct BrowserOmniboxTest {
        accepted: Arc<Mutex<Vec<usize>>>,
    }

    const PANE_CLOSED: u64 = u64::MAX;

    struct BrowserHeaderTest {
        drags: Arc<Mutex<Vec<crate::pane::PaneDrag>>>,
        closed: Arc<Mutex<Vec<u64>>>,
        site_controls_opened: Arc<Mutex<bool>>,
        address: Entity<InputState>,
        address_clicked: bool,
        active: usize,
    }

    impl Render for BrowserHeaderTest {
        fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let button = |id, icon| {
                browser_toolbar_button(cx, id, icon, id, false, false)
                    .debug_selector(move || id.into())
            };
            let opened = self.site_controls_opened.clone();
            let site_controls =
                browser_site_controls_button(cx).dropdown_menu(move |menu, _, _| {
                    *opened.lock().unwrap() = true;
                    browser_site_menu(
                        menu,
                        BrowserSiteMenuState {
                            site: "example.com".into(),
                            connection_secure: Some(true),
                            audio_muted: Some(false),
                            can_clear_site_data: false,
                        },
                        |_, _| {},
                        |_, _| {},
                    )
                });
            let drags = self.drags.clone();
            let closed = self.closed.clone();
            div().w(px(600.0)).child(BrowserHeader::new(
                true,
                BrowserTabStrip::new(
                    (0..8)
                        .map(|id| BrowserTabInfo::new(id, "Page", "Page title"))
                        .collect(),
                    self.active,
                )
                .title(
                    crate::pane::PaneDragSource::new(
                        zz_protocol::PaneId(3),
                        "Browser",
                        true,
                        move |drag, _, _| drags.lock().unwrap().push(*drag),
                    ),
                    move |_, _| closed.lock().unwrap().push(PANE_CLOSED),
                ),
                browser_toolbar_cluster()
                    .child(
                        crate::pane::pane_header_icon_button(
                            "split-bottom",
                            IconName::PanelBottom,
                            true,
                            cx,
                        )
                        .debug_selector(|| "split-bottom".into()),
                    )
                    .child(
                        crate::pane::pane_header_icon_button(
                            "split-right",
                            IconName::PanelRight,
                            true,
                            cx,
                        )
                        .debug_selector(|| "split-right".into()),
                    ),
                BrowserToolbar::new(
                    button("back", IconName::ArrowLeft),
                    button("forward", IconName::ArrowRight),
                    button("reload", IconName::Redo2),
                    div()
                        .id("test-address")
                        .debug_selector(|| "test-address".into())
                        .flex_1()
                        .min_w_0()
                        .h_full()
                        .child(
                            browser_address(&self.address, site_controls, window, cx)
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|view, _, _, _| view.address_clicked = true),
                                ),
                        ),
                    button("picker", IconName::Inspector),
                    button("more", IconName::EllipsisVertical),
                ),
            ))
        }
    }

    #[zz_gpui::test]
    fn narrow_header_keeps_controls_visible_and_address_focusable(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let (view, cx) = cx.add_window_view(|window, cx| BrowserHeaderTest {
            drags: Arc::new(Mutex::new(Vec::new())),
            closed: Arc::new(Mutex::new(Vec::new())),
            site_controls_opened: Arc::new(Mutex::new(false)),
            address: cx.new(|cx| InputState::new(window, cx)),
            address_clicked: false,
            active: 0,
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
        let tabs = cx.debug_bounds("browser-tab-row").unwrap();
        let navigation = cx.debug_bounds("browser-navigation-row").unwrap();
        assert_eq!(tabs.size.height, px(BROWSER_TAB_ROW_HEIGHT));
        assert_eq!(navigation.origin.y, tabs.bottom());
        assert_eq!(
            BrowserHeader::HEIGHT - navigation.bottom(),
            px(crate::CHROME_GAP)
        );
        for id in ["pane-title", "split-bottom", "split-right"] {
            let bounds = cx.debug_bounds(id).unwrap();
            assert_eq!(
                bounds.size.height,
                crate::PANE_HEADER_CONTROL_HEIGHT,
                "{id}"
            );
            assert_eq!(bounds.center().y, tabs.center().y, "{id}");
        }
        for id in ["browser-new-tab", "browser-tab-0", "browser-tab-7"] {
            let bounds = cx.debug_bounds(id).unwrap();
            assert!(bounds.origin.x >= tabs.origin.x && bounds.right() <= tabs.right());
            assert_eq!(bounds.size.height, px(BROWSER_CONTROL_HEIGHT), "{id}");
            assert_eq!(
                bounds.origin.y - tabs.origin.y,
                px(crate::CHROME_GAP),
                "{id}"
            );
        }
        for id in [
            "back",
            "forward",
            "reload",
            "test-address",
            "picker",
            "more",
        ] {
            let bounds = cx.debug_bounds(id).unwrap();
            assert_eq!(bounds.size.height, px(BROWSER_CONTROL_HEIGHT));
            assert!(bounds.origin.x >= navigation.origin.x && bounds.right() <= navigation.right());
            assert!(
                bounds.origin.y >= navigation.origin.y && bounds.bottom() <= navigation.bottom()
            );
        }
        let title = cx.debug_bounds("pane-title").unwrap();
        let strip = cx.debug_bounds("browser-tabs").unwrap();
        let new_tab = cx.debug_bounds("browser-new-tab").unwrap();
        assert!(title.right() <= strip.origin.x);
        assert!(strip.right() <= new_tab.origin.x);
        assert!(new_tab.right() <= cx.debug_bounds("split-bottom").unwrap().origin.x);
        let first = cx.debug_bounds("browser-tab-0").unwrap();
        let last = cx.debug_bounds("browser-tab-7").unwrap();
        assert_eq!(first.size.width, last.size.width);
        assert!(first.size.width >= px(BROWSER_TAB_MIN_WIDTH));
        assert!(first.size.width < px(BROWSER_TAB_LABEL_WIDTH));
        assert!(last.right() <= strip.right());
        cx.simulate_mouse_move(title.center(), None, Modifiers::none());
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
        let close = cx.debug_bounds("pane-title-close").unwrap();
        cx.simulate_click(close.center(), Modifiers::none());
        assert_eq!(
            view.read_with(cx, |view, _| view.closed.lock().unwrap().clone()),
            [PANE_CLOSED]
        );
        cx.simulate_mouse_down(title.center(), MouseButton::Left, Modifiers::none());
        let target = title.center() + point(px(40.0), px(0.0));
        cx.simulate_mouse_move(target, MouseButton::Left, Modifiers::none());
        cx.run_until_parked();
        assert!(cx.update(|_, cx| cx.has_active_drag()));
        assert_eq!(
            view.read_with(cx, |view, _| view.drags.lock().unwrap().clone()),
            [crate::pane::PaneDrag {
                pane: zz_protocol::PaneId(3),
                requires_prefix: false
            }]
        );
        cx.simulate_mouse_up(target, MouseButton::Left, Modifiers::none());
        cx.run_until_parked();
        let address = cx.debug_bounds("test-address").unwrap();
        assert!(address.size.width > px(100.0));
        let site_controls = cx.debug_bounds("browser-site-controls").unwrap();
        assert!(site_controls.origin.x > address.origin.x);
        assert!(site_controls.right() < address.right());
        cx.simulate_click(site_controls.center(), Modifiers::none());
        cx.update(|_, cx| {
            assert!(!view.read(cx).address_clicked);
            assert!(*view.read(cx).site_controls_opened.lock().unwrap());
        });
        cx.simulate_keystrokes("escape");
        cx.simulate_click(address.center(), Modifiers::none());
        cx.update(|window, cx| {
            assert!(view.read(cx).address_clicked);
            assert!(
                view.read(cx)
                    .address
                    .read(cx)
                    .focus_handle(cx)
                    .is_focused(window)
            );
        });
        view.update(cx, |view, cx| {
            view.active = 7;
            cx.notify();
        });
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
        assert!(cx.debug_bounds("browser-tab-7").is_some());
    }

    impl Render for BrowserOmniboxTest {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let rows = [
                ("GitHub", "github.com", true),
                ("Rust", "github.com/rust-lang/rust", false),
            ]
            .into_iter()
            .enumerate()
            .map(|(index, (title, url, selected))| {
                let accepted = Arc::clone(&self.accepted);
                browser_omnibox_row(index, title, url, selected, None, cx)
                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                        accepted.lock().unwrap().push(index);
                        cx.stop_propagation();
                    })
                    .into_any_element()
            })
            .collect();
            div()
                .relative()
                .size_full()
                .child(browser_omnibox_panel(rows, cx))
        }
    }

    impl Render for BrowserTabStripTest {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let activated = Arc::clone(&self.activated);
            let closed = Arc::clone(&self.closed);
            div().w(px(self.width)).h(px(BROWSER_CONTROL_HEIGHT)).child(
                BrowserTabStrip::new(
                    vec![
                        BrowserTabInfo::new(1, "Active", "Active tab"),
                        BrowserTabInfo::new(2, "Inactive", "Inactive tab"),
                    ],
                    self.active,
                )
                .on_activate(move |id, _, _| activated.lock().unwrap().push(id))
                .on_close(move |id, _, _| closed.lock().unwrap().push(id)),
            )
        }
    }

    #[zz_gpui::test]
    fn active_and_inactive_tabs_reveal_working_close_buttons_on_hover(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let activated = Arc::new(Mutex::new(Vec::new()));
        let closed = Arc::new(Mutex::new(Vec::new()));
        let activated_for_view = Arc::clone(&activated);
        let closed_for_view = Arc::clone(&closed);
        let (_, cx) = cx.add_window_view(move |_, _| BrowserTabStripTest {
            width: 420.0,
            active: 0,
            activated: activated_for_view,
            closed: closed_for_view,
        });
        let cx: &mut VisualTestContext = cx;
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });

        let active_tab = cx
            .debug_bounds("browser-tab-1")
            .expect("the active tab renders");
        let inactive_tab = cx
            .debug_bounds("browser-tab-2")
            .expect("the inactive tab renders");
        assert_eq!(inactive_tab.size.width, px(BROWSER_TAB_MAX_WIDTH));
        assert_eq!(
            cx.debug_bounds("browser-new-tab").unwrap().origin.x,
            inactive_tab.right() + px(BROWSER_TAB_GAP)
        );

        cx.simulate_mouse_move(active_tab.center(), None, Modifiers::none());
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
        let active = cx
            .debug_bounds("browser-tab-close-1")
            .expect("the active tab reveals its close button on hover");
        assert!(
            active.origin.x >= active_tab.origin.x
                && active.right() <= active_tab.right()
                && active.origin.y >= active_tab.origin.y
                && active.bottom() <= active_tab.bottom(),
            "the active close button must stay inside its tab surface"
        );
        cx.simulate_click(active.center(), Modifiers::none());

        cx.simulate_mouse_move(inactive_tab.center(), None, Modifiers::none());
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
        let inactive = cx
            .debug_bounds("browser-tab-close-2")
            .expect("the inactive tab reveals its close button on hover");
        assert_eq!(active.size, inactive.size);
        assert_eq!(
            active_tab.right() - active.right(),
            inactive_tab.right() - inactive.right()
        );
        assert_eq!(
            active.origin.y - active_tab.origin.y,
            inactive.origin.y - inactive_tab.origin.y
        );
        cx.simulate_click(inactive.center(), Modifiers::none());

        assert_eq!(*closed.lock().unwrap(), [1, 2]);
        assert!(activated.lock().unwrap().is_empty());
    }

    #[zz_gpui::test]
    fn switching_active_tab_preserves_equal_tab_widths(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let (view, cx) = cx.add_window_view(move |_, _| BrowserTabStripTest {
            width: 420.0,
            active: 0,
            activated: Arc::default(),
            closed: Arc::default(),
        });
        let cx: &mut VisualTestContext = cx;
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });

        let first_before = cx.debug_bounds("browser-tab-1").expect("first tab renders");
        let second_before = cx
            .debug_bounds("browser-tab-2")
            .expect("second tab renders");
        assert_eq!(first_before.size.width, second_before.size.width);

        view.update(cx, |view, cx| {
            view.active = 1;
            cx.notify();
        });
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });

        let first_after = cx.debug_bounds("browser-tab-1").expect("first tab renders");
        let second_after = cx
            .debug_bounds("browser-tab-2")
            .expect("second tab renders");
        assert_eq!(first_after.size.width, second_after.size.width);
        assert_eq!(first_before.size.width, first_after.size.width);
        assert_eq!(second_before.size.width, second_after.size.width);
        assert_eq!(first_after.size.width, px(BROWSER_TAB_MAX_WIDTH));
        view.update(cx, |view, cx| {
            view.width = 300.0;
            cx.notify();
        });
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
        let first = cx.debug_bounds("browser-tab-1").unwrap();
        let second = cx.debug_bounds("browser-tab-2").unwrap();
        let strip = cx.debug_bounds("browser-tabs").unwrap();
        assert_eq!(first.size.width, second.size.width);
        assert!(first.size.width < px(BROWSER_TAB_MAX_WIDTH));
        assert!(first.size.width >= px(BROWSER_TAB_MIN_WIDTH));
        assert!(first.origin.x >= strip.origin.x && second.right() <= strip.right());

        view.update(cx, |view, cx| {
            view.width = 120.0;
            view.active = 0;
            cx.notify();
        });
        for _ in 0..2 {
            cx.update(|window, cx| {
                _ = window.draw(cx);
            });
        }
        let first = cx.debug_bounds("browser-tab-1").unwrap();
        let second = cx.debug_bounds("browser-tab-2").unwrap();
        let strip = cx.debug_bounds("browser-tabs").unwrap();
        assert_eq!(first.size.width, second.size.width);
        assert!(first.size.width >= px(BROWSER_TAB_MIN_WIDTH));
        assert!(first.size.width < px(BROWSER_TAB_LABEL_WIDTH));
        assert!(first.origin.x >= strip.origin.x && second.right() <= strip.right());
        cx.simulate_mouse_move(second.center(), None, Modifiers::none());
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
        let close = cx.debug_bounds("browser-tab-close-2").unwrap();
        assert!(close.origin.x >= second.origin.x && close.right() <= second.right());
        cx.simulate_click(close.center(), Modifiers::none());
        assert_eq!(
            view.read_with(cx, |view, _| view.closed.lock().unwrap().clone()),
            [2]
        );
        view.update(cx, |view, cx| {
            view.width = 420.0;
            cx.notify();
        });
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
        let first = cx.debug_bounds("browser-tab-1").unwrap();
        let second = cx.debug_bounds("browser-tab-2").unwrap();
        let strip = cx.debug_bounds("browser-tabs").unwrap();
        assert_eq!(first.size.width, px(BROWSER_TAB_MAX_WIDTH));
        assert_eq!(second.size.width, px(BROWSER_TAB_MAX_WIDTH));
        assert!(first.origin.x >= strip.origin.x && second.right() <= strip.right());
    }

    #[zz_gpui::test]
    fn omnibox_results_render_above_the_toolbar_and_accept_pointer_input(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let accepted = Arc::new(Mutex::new(Vec::new()));
        let accepted_for_view = Arc::clone(&accepted);
        let (_, cx) = cx.add_window_view(move |_, _| BrowserOmniboxTest {
            accepted: accepted_for_view,
        });
        let cx: &mut VisualTestContext = cx;
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });

        let panel = cx
            .debug_bounds("browser-omnibox-results")
            .expect("the omnibox panel renders");
        let first = cx
            .debug_bounds("browser-omnibox-suggestion-0")
            .expect("the first suggestion renders");
        let second = cx
            .debug_bounds("browser-omnibox-suggestion-1")
            .expect("the second suggestion renders");
        assert!(panel.origin.y >= BrowserHeader::HEIGHT);
        assert!(first.bottom() <= second.origin.y);
        for row in [first, second] {
            assert!(row.origin.x > panel.origin.x && row.right() < panel.right());
            assert!(row.origin.y > panel.origin.y && row.bottom() < panel.bottom());
            assert_eq!(row.size.height, px(32.0));
        }

        cx.simulate_click(second.center(), Modifiers::none());
        assert_eq!(*accepted.lock().unwrap(), [1]);
    }
}
