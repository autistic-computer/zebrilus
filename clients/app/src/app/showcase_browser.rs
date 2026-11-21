use std::rc::Rc;

use ala::{
    AnyElement, App, Context, Corners, Entity, FocusHandle, Focusable, InteractiveElement as _,
    IntoElement, ParentElement as _, Pixels, Render, SharedString, StatefulInteractiveElement as _,
    Styled as _, Subscription, Window, div, prelude::*, px,
};
use penna::{
    ActiveTheme as _, Card, CardLook, Chip, Colorize as _, Emphasis, IconName, Label, Notice,
    Sizable as _, Space, StyledExt as _, TextStyle, Tone,
    components::button::{Button, ButtonVariants as _},
    components::input::{InputEvent, InputState},
    components::menu::DropdownMenu as _,
    h_flex, v_flex,
};
use zz_protocol::{BrowserDescriptor, PaneId};
use zz_ui::{
    browser::{
        BrowserActionMenuState, BrowserHeader, BrowserMenuActions, BrowserMenuEngine,
        BrowserMenuProfile, BrowserPickStatus, BrowserProfileDiscoveryState, BrowserSiteMenuState,
        BrowserTabInfo, BrowserTabStrip, BrowserToolbar, browser_action_menu, browser_address,
        browser_recent_row, browser_site_controls_button, browser_site_menu, browser_start_surface,
        browser_toolbar_button,
    },
    pane::{PaneActions, PaneDragSource, PaneState, TerminalPaneAction, pane_header_actions},
};

use crate::connection::Connection;

struct MockTab {
    id: u64,
    url: String,
    history: Vec<String>,
    cursor: usize,
}

impl MockTab {
    fn new(id: u64, url: impl Into<String>) -> Self {
        let url = url.into();
        Self {
            id,
            history: vec![url.clone()],
            url,
            cursor: 0,
        }
    }

    fn navigate(&mut self, url: String) {
        if self.url == url {
            return;
        }
        self.history.truncate(self.cursor + 1);
        self.history.push(url.clone());
        self.cursor = self.history.len() - 1;
        self.url = url;
    }

    fn can_back(&self) -> bool {
        self.cursor > 0
    }

    fn can_forward(&self) -> bool {
        self.cursor + 1 < self.history.len()
    }

    fn back(&mut self) -> bool {
        if !self.can_back() {
            return false;
        }
        self.cursor -= 1;
        self.url.clone_from(&self.history[self.cursor]);
        true
    }

    fn forward(&mut self) -> bool {
        if !self.can_forward() {
            return false;
        }
        self.cursor += 1;
        self.url.clone_from(&self.history[self.cursor]);
        true
    }
}

pub(super) struct ShowcaseBrowserPane {
    pane: PaneId,
    connection: Entity<Connection>,
    focus: FocusHandle,
    address: Entity<InputState>,
    _address_events: Subscription,
    tabs: Vec<MockTab>,
    active_tab: usize,
    next_tab_id: u64,
    active: bool,
    radii: Corners<Pixels>,
    picking: bool,
    picked_selector: Option<SharedString>,
    zoom_percent: u16,
    clicks: u32,
    reloads: u32,
    remote_egress: bool,
    pane_state: PaneState,
    float_rows: Option<u16>,
    on_drag_start: Option<Rc<dyn Fn(zz_ui::pane::PaneDrag, &mut Window, &mut App)>>,
}

impl ShowcaseBrowserPane {
    pub(super) fn new(
        pane: PaneId,
        descriptor: &BrowserDescriptor,
        connection: Entity<Connection>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let initial_urls: Vec<String> = if descriptor.tabs.is_empty() {
            vec!["http://localhost:3000".to_owned()]
        } else {
            descriptor.tabs.clone()
        };
        let active_tab = descriptor.active_tab.min(initial_urls.len() - 1);
        let initial_url = initial_urls[active_tab].clone();
        let tabs: Vec<MockTab> = initial_urls
            .into_iter()
            .enumerate()
            .map(|(index, url)| MockTab::new(index as u64 + 1, url))
            .collect();
        let next_tab_id = tabs.len() as u64 + 1;
        let remote_egress = pane == PaneId(11);

        let address = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Search or enter address")
                .default_value(initial_url)
        });
        let address_events = cx.subscribe_in(
            &address,
            window,
            |this, input, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) {
                    let raw = input.read(cx).value().trim().to_owned();
                    let url = normalize_url(&raw);
                    this.navigate_to(url, window, cx);
                }
            },
        );

        Self {
            pane,
            connection,
            focus: cx.focus_handle(),
            address,
            _address_events: address_events,
            tabs,
            active_tab,
            next_tab_id,
            active: false,
            radii: Corners::all(px(0.0)),
            picking: false,
            picked_selector: None,
            zoom_percent: 100,
            clicks: 1,
            reloads: 0,
            remote_egress,
            pane_state: PaneState::default(),
            float_rows: None,
            on_drag_start: None,
        }
    }

    pub(super) fn synchronize(
        &mut self,
        active: bool,
        radii: Corners<Pixels>,
        pane_state: PaneState,
        float_rows: Option<u16>,
        on_drag_start: Rc<dyn Fn(zz_ui::pane::PaneDrag, &mut Window, &mut App)>,
        cx: &mut Context<Self>,
    ) {
        self.active = active;
        self.radii = radii;
        self.pane_state = pane_state;
        self.float_rows = float_rows;
        self.on_drag_start = Some(on_drag_start);
        cx.notify();
    }

    pub(super) fn focus_handle(&self) -> FocusHandle {
        self.focus.clone()
    }

    fn current_url(&self) -> &str {
        self.tabs
            .get(self.active_tab)
            .map_or("about:blank", |tab| tab.url.as_str())
    }

    fn sync_address(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let url = self.current_url().to_owned();
        self.address
            .update(cx, |input, cx| input.set_value(url, window, cx));
    }

    fn navigate_to(&mut self, url: String, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(tab) = self.tabs.get_mut(self.active_tab) {
            tab.navigate(url.clone());
        }
        self.picked_selector = None;
        self.sync_address(window, cx);
        self.connection.update(cx, |connection, cx| {
            connection.command(
                "set-browser-tabs",
                vec!["-t".to_owned(), self.pane.to_string(), url],
                cx,
            );
        });
        cx.notify();
    }

    fn activate_tab(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(index) = self.tabs.iter().position(|tab| tab.id == id) {
            self.active_tab = index;
            self.picked_selector = None;
            self.sync_address(window, cx);
            cx.notify();
        }
    }

    fn close_tab(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        if self.tabs.len() <= 1 {
            return;
        }
        if let Some(index) = self.tabs.iter().position(|tab| tab.id == id) {
            self.tabs.remove(index);
            if self.active_tab >= self.tabs.len() {
                self.active_tab = self.tabs.len() - 1;
            }
            self.picked_selector = None;
            self.sync_address(window, cx);
            cx.notify();
        }
    }

    fn open_new_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let id = self.next_tab_id;
        self.next_tab_id += 1;
        self.tabs.push(MockTab::new(id, "about:blank"));
        self.active_tab = self.tabs.len() - 1;
        self.picked_selector = None;
        self.sync_address(window, cx);
        cx.notify();
    }

    fn go_back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self
            .tabs
            .get_mut(self.active_tab)
            .is_some_and(MockTab::back)
        {
            self.picked_selector = None;
            self.sync_address(window, cx);
            cx.notify();
        }
    }

    fn go_forward(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self
            .tabs
            .get_mut(self.active_tab)
            .is_some_and(MockTab::forward)
        {
            self.picked_selector = None;
            self.sync_address(window, cx);
            cx.notify();
        }
    }

    fn reload(&mut self, cx: &mut Context<Self>) {
        self.reloads = self.reloads.saturating_add(1);
        cx.notify();
    }

    fn pick_element(&mut self, selector: &'static str, cx: &mut Context<Self>) {
        if !self.picking {
            return;
        }
        self.picking = false;
        self.picked_selector = Some(SharedString::from(selector));
        cx.write_to_clipboard(ala::ClipboardItem::new_string(selector.to_owned()));
        cx.notify();
    }

    fn pickable_card(
        &self,
        id: &'static str,
        selector: &'static str,
        cx: &mut Context<Self>,
    ) -> Card {
        let picking = self.picking;
        let accent = cx.theme().accent;
        let hover_fill = cx.theme().paint(Tone::Accent, Emphasis::Soft).fill;
        let selected = self.picked_selector.as_deref() == Some(selector);
        Card::new(id, CardLook::Activity)
            .when(selected, |card| card.tone(Tone::Accent))
            .when(picking, |card| {
                card.cursor_pointer()
                    .hover(move |s| s.border_color(accent).bg(hover_fill))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.pick_element(selector, cx);
                    }))
            })
    }

    fn render_page(&self, cx: &mut Context<Self>) -> AnyElement {
        let url = self.current_url();
        if url == "about:blank" || url.is_empty() {
            return self.render_start_page(cx);
        }
        if url.contains("autistic.computer") {
            return self.render_autistic_page(cx);
        }
        self.render_dev_page(url, cx)
    }

    fn render_start_page(&self, cx: &mut Context<Self>) -> AnyElement {
        let recent = ["http://localhost:3000", "https://autistic.computer"];
        let rows: Vec<AnyElement> = recent
            .into_iter()
            .enumerate()
            .map(|(index, url)| {
                let target = url.to_owned();
                div()
                    .id(("showcase-recent", index))
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.navigate_to(target.clone(), window, cx);
                    }))
                    .child(browser_recent_row(
                        ("showcase-recent-row", index),
                        url,
                        None,
                        px(34.0),
                        cx,
                    ))
                    .into_any_element()
            })
            .collect();
        browser_start_surface(
            v_flex()
                .w_full()
                .max_w(px(420.0))
                .gap(Space::S8)
                .child(Label::new(TextStyle::Caption, "Recent").muted())
                .children(rows),
        )
        .into_any_element()
    }

    fn render_dev_page(&self, url: &str, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let card_ground = theme.ground_of(CardLook::Activity);
        let success = theme.success;
        let clicks = self.clicks;
        let reloads = self.reloads;
        let remote = self.remote_egress;

        let badge = if reloads > 0 {
            format!("hmr #{reloads}")
        } else if remote {
            "ssh -D".to_owned()
        } else {
            "CEF".to_owned()
        };

        let heading = if remote {
            "Remote dev server, local GPU"
        } else {
            "Chromium on the same GPU surface"
        };
        let subtitle = if remote {
            "Traffic tunnels through desktop over ssh -D."
        } else {
            "Split, float into PiP, or inspect elements."
        };

        v_flex()
            .size_full()
            .p(Space::S16)
            .gap(Space::S12)
            .overflow_hidden()
            .child(
                self.pickable_card("mock-dev-bar", "header.dev-bar", cx)
                    .flex()
                    .flex_row()
                    .w_full()
                    .items_center()
                    .justify_between()
                    .gap(Space::S8)
                    .px(Space::S12)
                    .py(Space::S8)
                    .child(
                        h_flex()
                            .items_center()
                            .gap(Space::S8)
                            .child(div().w(px(8.0)).h(px(8.0)).rounded_full().bg(success))
                            .child(
                                Label::new(TextStyle::Small, host_label(url).to_owned())
                                    .on(card_ground),
                            ),
                    )
                    .child(
                        h_flex()
                            .items_center()
                            .gap(Space::S6)
                            .child(
                                Chip::new("mock-mode-chip", badge)
                                    .tone(Tone::Accent)
                                    .xsmall(),
                            )
                            .child(
                                Button::new("mock-hot-reload")
                                    .xsmall()
                                    .label(format!("Ping ({clicks})"))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        if this.picking {
                                            this.pick_element("button.ping-demo", cx);
                                        } else {
                                            this.clicks = this.clicks.saturating_add(1);
                                            cx.notify();
                                        }
                                    })),
                            ),
                    ),
            )
            .child(
                self.pickable_card("mock-app-card", "main.app-preview", cx)
                    .flex()
                    .flex_col()
                    .w_full()
                    .gap(Space::S6)
                    .p(Space::S16)
                    .child(Label::new(TextStyle::Body, heading).on(card_ground))
                    .child(
                        Label::new(TextStyle::Small, subtitle)
                            .muted()
                            .on(card_ground),
                    ),
            )
            .into_any_element()
    }

    fn render_autistic_page(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let card_ground = theme.ground_of(CardLook::Activity);
        let accent = theme.accent;

        v_flex()
            .size_full()
            .p(Space::S16)
            .gap(Space::S12)
            .justify_center()
            .child(
                self.pickable_card("mock-ac-card", "main.autistic-computer", cx)
                    .flex()
                    .flex_col()
                    .gap(Space::S10)
                    .p(Space::S16)
                    .child(
                        div()
                            .type_style_mono(TextStyle::Small, cx)
                            .whitespace_nowrap()
                            .child("autistic ─┐\n          ├─ .computer\n    adhd ─┘"),
                    )
                    .child(
                        Label::new(TextStyle::Caption, "software on a spectrum")
                            .mono()
                            .muted()
                            .on(card_ground),
                    )
                    .child(
                        h_flex()
                            .items_center()
                            .gap(Space::S8)
                            .child(
                                div()
                                    .type_style_mono(TextStyle::Caption, cx)
                                    .text_color(accent)
                                    .child("-> zebrilus /zz"),
                            )
                            .child(
                                Button::new("mock-open-ac")
                                    .xsmall()
                                    .icon(IconName::ExternalLink)
                                    .label("Open")
                                    .on_click(|_, _, cx| {
                                        cx.open_url("https://autistic.computer");
                                    }),
                            ),
                    ),
            )
            .into_any_element()
    }
}

impl Focusable for ShowcaseBrowserPane {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for ShowcaseBrowserPane {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity();
        let pane_id = self.pane;
        let active = self.active;
        let picking = self.picking;
        let url = self.current_url().to_owned();
        let site = host_label(&url).to_owned();
        let secure = url.starts_with("https://");
        let can_back = self
            .tabs
            .get(self.active_tab)
            .is_some_and(MockTab::can_back);
        let can_forward = self
            .tabs
            .get(self.active_tab)
            .is_some_and(MockTab::can_forward);

        let tab_infos: Vec<BrowserTabInfo> = self
            .tabs
            .iter()
            .map(|tab| BrowserTabInfo::new(tab.id, tab_title(&tab.url), host_label(&tab.url)))
            .collect();

        let on_activate = {
            let entity = entity.clone();
            move |id, window: &mut Window, cx: &mut App| {
                entity.update(cx, |this, cx| this.activate_tab(id, window, cx));
            }
        };
        let on_close = {
            let entity = entity.clone();
            move |id, window: &mut Window, cx: &mut App| {
                entity.update(cx, |this, cx| this.close_tab(id, window, cx));
            }
        };
        let on_new_tab = {
            let entity = entity.clone();
            move |window: &mut Window, cx: &mut App| {
                entity.update(cx, |this, cx| this.open_new_tab(window, cx));
            }
        };

        let on_drag = self.on_drag_start.clone();
        let drag_source = PaneDragSource::new(
            pane_id,
            site.clone(),
            on_drag.is_some(),
            move |drag, window, cx| {
                if let Some(callback) = &on_drag {
                    callback(*drag, window, cx);
                }
            },
        );

        let tab_strip = BrowserTabStrip::new(tab_infos, self.active_tab)
            .title(drag_source)
            .on_activate(on_activate)
            .on_close(on_close)
            .on_new_tab(on_new_tab);

        let connection = self.connection.clone();
        let float_rows = self.float_rows;
        let pane_actions = PaneActions::new(
            pane_header_actions(self.pane_state),
            move |action: TerminalPaneAction, _, cx| {
                connection.update(cx, |connection, cx| {
                    if !connection.connected || connection.core.attached_read_only() {
                        return;
                    }
                    for command in action.commands(pane_id, float_rows) {
                        connection.invoke(command, cx);
                    }
                });
            },
        );

        let site_controls = browser_site_controls_button(cx).dropdown_menu({
            let site = site.clone();
            move |menu, _, _| {
                browser_site_menu(
                    menu,
                    BrowserSiteMenuState {
                        site: site.clone().into(),
                        connection_secure: Some(secure),
                        audio_muted: None,
                        can_clear_site_data: false,
                    },
                    |_, _| {},
                    |_, _| {},
                )
            }
        });

        let address = browser_address(&self.address, site_controls, window, cx).into_any_element();

        let back_btn = browser_toolbar_button(
            cx,
            "showcase-browser-back",
            IconName::ArrowLeft,
            "Back",
            !can_back,
            false,
        )
        .on_click(cx.listener(|this, _, window, cx| this.go_back(window, cx)));

        let forward_btn = browser_toolbar_button(
            cx,
            "showcase-browser-fwd",
            IconName::ArrowRight,
            "Forward",
            !can_forward,
            false,
        )
        .on_click(cx.listener(|this, _, window, cx| this.go_forward(window, cx)));

        let reload_btn = browser_toolbar_button(
            cx,
            "showcase-browser-reload",
            IconName::Redo2,
            "Reload",
            false,
            false,
        )
        .on_click(cx.listener(|this, _, _, cx| this.reload(cx)));

        let picker_btn = browser_toolbar_button(
            cx,
            "showcase-browser-picker",
            IconName::Inspector,
            if picking {
                "Cancel element picker"
            } else {
                "Pick an element"
            },
            false,
            picking,
        )
        .on_click(cx.listener(|this, _, _, cx| {
            this.picking = !this.picking;
            if this.picking {
                this.picked_selector = None;
            }
            cx.notify();
        }));

        let menu_url = url.clone();
        let zoom_percent = self.zoom_percent;
        let more_entity = entity.clone();
        let more_btn = browser_toolbar_button(
            cx,
            "showcase-browser-more",
            IconName::EllipsisVertical,
            "More browser actions",
            false,
            false,
        )
        .dropdown_menu_with_anchor(ala::Anchor::TopRight, move |menu, window, cx| {
            let open_url = menu_url.clone();
            let copy_url = menu_url.clone();
            let e_reload = more_entity.clone();
            let e_pick = more_entity.clone();
            let e_in = more_entity.clone();
            let e_out = more_entity.clone();
            let e_reset = more_entity.clone();
            let e_read = more_entity.clone();
            let actions = BrowserMenuActions::new()
                .open_url(move |_, cx| cx.open_url(&open_url))
                .copy_url(move |_, cx| {
                    cx.write_to_clipboard(ala::ClipboardItem::new_string(copy_url.clone()));
                })
                .reload(move |_, cx| e_reload.update(cx, ShowcaseBrowserPane::reload))
                .toggle_picker(move |_, cx| {
                    e_pick.update(cx, |this, cx| {
                        this.picking = !this.picking;
                        cx.notify();
                    });
                })
                .zoom_in(move |_, cx| {
                    e_in.update(cx, |this, cx| {
                        this.zoom_percent = (this.zoom_percent + 10).min(200);
                        cx.notify();
                    });
                })
                .zoom_out(move |_, cx| {
                    e_out.update(cx, |this, cx| {
                        this.zoom_percent = this.zoom_percent.saturating_sub(10).max(50);
                        cx.notify();
                    });
                })
                .reset_zoom(move |_, cx| {
                    e_reset.update(cx, |this, cx| {
                        this.zoom_percent = 100;
                        cx.notify();
                    });
                })
                .zoom_percent(move |cx| e_read.read(cx).zoom_percent);

            browser_action_menu(
                menu,
                window,
                cx,
                BrowserActionMenuState {
                    engine: BrowserMenuEngine::Chromium,
                    current_profile_label: "Default zz profile".into(),
                    selected_profile: "default".into(),
                    default_profile: "default".into(),
                    profiles: vec![BrowserMenuProfile::new("default", "Default zz profile")],
                    profile_discovery: BrowserProfileDiscoveryState::Ready,
                    zoom_percent,
                    can_import_chrome_data: false,
                    can_clear_site_data: false,
                    picker_active: picking,
                },
                actions,
            )
        });

        let toolbar = BrowserToolbar::new(
            back_btn,
            forward_btn,
            reload_btn,
            address,
            picker_btn,
            more_btn,
        );

        let header = BrowserHeader::new(active, tab_strip, pane_actions, toolbar);

        let picked_banner = self.picked_selector.clone().map(|selector| {
            div().px(Space::S12).pt(Space::S8).child(
                Notice::new(Tone::Accent, format!("Picked {selector} — selector copied"))
                    .icon(IconName::Inspector)
                    .action(
                        Button::new("dismiss-picked")
                            .xsmall()
                            .ghost()
                            .label("Dismiss")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.picked_selector = None;
                                cx.notify();
                            })),
                    ),
            )
        });

        div()
            .id(("showcase-browser-pane", pane_id.0))
            .track_focus(&self.focus)
            .flex()
            .flex_col()
            .size_full()
            .overflow_hidden()
            .rounded_tl(self.radii.top_left)
            .rounded_tr(self.radii.top_right)
            .rounded_bl(self.radii.bottom_left)
            .rounded_br(self.radii.bottom_right)
            .bg(cx.theme().background.opaque())
            .child(header)
            .when(self.picking, |el| {
                el.child(BrowserPickStatus::new(
                    "Click an element to copy its selector",
                ))
            })
            .children(picked_banner)
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .child(self.render_page(cx)),
            )
    }
}

fn normalize_url(raw: &str) -> String {
    if raw.is_empty() || raw == "about:blank" {
        return "about:blank".to_owned();
    }
    if raw.starts_with("http://") || raw.starts_with("https://") {
        return raw.to_owned();
    }
    if raw.starts_with("localhost") || raw.starts_with("127.0.0.1") {
        return format!("http://{raw}");
    }
    format!("https://{raw}")
}

fn host_label(url: &str) -> &str {
    if url == "about:blank" || url.is_empty() {
        return "New tab";
    }
    let trimmed = url
        .trim_start_matches("https://")
        .trim_start_matches("http://");
    trimmed.split('/').next().unwrap_or(trimmed)
}

fn tab_title(url: &str) -> &'static str {
    if url == "about:blank" || url.is_empty() {
        "New tab"
    } else if url.contains("autistic.computer") {
        "autistic.computer"
    } else if url.contains("localhost:3000") {
        "localhost:3000"
    } else {
        "Web page"
    }
}
