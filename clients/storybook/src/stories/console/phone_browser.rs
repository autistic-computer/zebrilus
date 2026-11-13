use ala::{
    AnyElement, AnyView, App, AppContext as _, Context, Entity, IntoElement, ParentElement as _,
    Render, SharedString, Styled as _, Window, div, prelude::*, px,
};
use penna::{
    ActiveTheme as _, Colorize as _, IconName, Space, StyledExt as _, TextStyle, Tone,
    blocks::compact::{PillLabel, compact_floating_bar, compact_floating_pill},
    blocks::touch::{
        KEY_ROW_MARGIN_X, KEY_ROW_MARGIN_Y, PhoneFind, StackClose, StackGroup, StackMenu, page_dots,
    },
    components::button::{Button, ButtonVariants as _},
    components::input::InputState,
    components::progress::Progress,
    h_flex, v_flex,
};
use zz_ui::compact::{
    BACK, BrowserMenu, PaneMenu, TabRow, browser_menu, failed_page, pane_menu, tabs_sheet,
};

use super::phone::{groups, phone, placement};
use crate::story::states;

const HEIGHT: f32 = 720.0;
const TOP: f32 = 16.0;
const TITLE: &str = "Draw the browser pane at touch sizes · Pull Request #412";
const HOST: &str = "github.com";
const URL: &str = "https://github.com/autistic-computer/zebrilus/pull/412";
const TYPED: &str = "penna";
const ERROR: &str = "A server with the specified hostname could not be found.";

const COMMENTS: [(&str, &str, &str); 2] = [
    (
        "fabrico",
        "2 hours ago",
        "The browser pane drops its top bar on the phone. The pill is the bar: its stack leads with Back, Forward and Reload, and the address sits one chip above them.",
    ),
    (
        "deadbeef-hexlord",
        "40 minutes ago",
        "Back one stroke from the pill reads well. The edge swipe still goes back on iOS, so nothing is lost there.",
    ),
];

fn site_page(partial: bool, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let muted = theme.foreground.muted();
    let comment = |(who, when, text): (&'static str, &'static str, &'static str)| {
        v_flex()
            .gap(Space::S6)
            .child(
                h_flex()
                    .gap(Space::S8)
                    .type_style(TextStyle::Small, cx)
                    .child(div().text_color(theme.foreground).child(who))
                    .child(div().text_color(muted).child(when)),
            )
            .child(div().type_style(TextStyle::TouchBody, cx).child(text))
    };
    v_flex()
        .size_full()
        .text_color(theme.foreground)
        .child(
            h_flex()
                .h(px(48.0))
                .px(Space::S16)
                .gap(Space::S8)
                .type_style(TextStyle::TouchSmall, cx)
                .text_color(muted)
                .child("autistic-computer /")
                .child(div().text_color(theme.foreground).child("zebrilus")),
        )
        .child(
            v_flex()
                .px(Space::S16)
                .gap(Space::S16)
                .child(
                    div()
                        .type_style_strong(TextStyle::Title, cx)
                        .child("Draw the browser pane at touch sizes #412"),
                )
                .child(
                    div()
                        .type_style(TextStyle::TouchSmall, cx)
                        .text_color(muted)
                        .child("fabrico wants to merge 3 commits into main from browser-pill"),
                )
                .when(!partial, |page| page.children(COMMENTS.map(comment))),
        )
}

fn label(meta: &str) -> PillLabel {
    PillLabel {
        icon: IconName::Globe,
        title: TITLE.into(),
        word: SharedString::default(),
        meta: SharedString::from(meta.to_owned()),
        tone: Tone::Neutral,
    }
}

fn pill_bar(
    id: &'static str,
    label: PillLabel,
    menu: Option<&Entity<StackMenu>>,
    cx: &App,
) -> AnyElement {
    let placement = placement();
    let mut pill = compact_floating_pill(id, label, page_dots(&groups(), cx), &placement, cx);
    if let Some(menu) = menu {
        pill = pill
            .opacity(1.0 - menu.read(cx).close_shown())
            .child(menu.read(cx).track_anchor());
    }
    div()
        .absolute()
        .left_0()
        .right_0()
        .bottom_0()
        .child(compact_floating_bar(pill, &placement, cx))
        .into_any_element()
}

fn frame(id: &'static str, page: AnyElement, over: Vec<AnyElement>, cx: &App) -> ala::Div {
    phone(
        div()
            .id(id)
            .relative()
            .h(px(HEIGHT))
            .overflow_hidden()
            .pt(px(TOP))
            .bg(zz_ui::pane_look::pane_color(
                cx.theme(),
                cx.theme().background,
            ))
            .child(div().relative().size_full().child(page))
            .children(over),
        cx,
    )
}

fn load_line(value: f32) -> AnyElement {
    div()
        .absolute()
        .top(px(TOP))
        .left_0()
        .right_0()
        .child(Progress::new("browser-load", value).line().label("Loading"))
        .into_any_element()
}

fn browser(back: bool, forward: bool, loading: bool) -> BrowserMenu<'static> {
    BrowserMenu {
        address: URL,
        back,
        forward,
        loading,
        tabs: 3,
        open_on_phone: true,
    }
}

fn open(
    groups: Vec<StackGroup>,
    fold: Option<usize>,
    cx: &mut Context<BrowserStates>,
) -> Entity<StackMenu> {
    let menu = cx.new(|cx| {
        let mut menu = StackMenu::new(StackClose::Disc, cx);
        menu.set_groups(groups, cx);
        menu.set_fold(fold, cx);
        menu.open(cx);
        menu
    });
    cx.observe(&menu, |_, _, cx| cx.notify()).detach();
    menu
}

pub(super) struct BrowserStates {
    stack: Entity<StackMenu>,
    first: Entity<StackMenu>,
    more: Entity<StackMenu>,
    held: Entity<StackMenu>,
    loading: Entity<StackMenu>,
    address: Entity<InputState>,
    typed: Entity<InputState>,
    tabs: bool,
}

pub(super) fn browser_states(window: &mut Window, cx: &mut App) -> AnyView {
    let address = cx.new(|cx| InputState::new(window, cx).default_value(URL));
    let typed = cx.new(|cx| InputState::new(window, cx).default_value(TYPED));
    cx.new(|cx| {
        let (groups, fold) = browser_menu(browser(true, false, false), "zz");
        let (first, _) = browser_menu(browser(false, false, false), "zz");
        let (loading, _) = browser_menu(browser(true, false, true), "zz");
        let more = pane_menu(PaneMenu {
            session: "zz",
            terminal: false,
        });
        let held = cx.new(|cx| {
            let mut menu = StackMenu::new(StackClose::Disc, cx);
            menu.set_groups(groups.clone(), cx);
            menu.set_fold(Some(fold), cx);
            menu.lift(BACK, cx);
            menu
        });
        cx.observe(&held, |_, _, cx| cx.notify()).detach();
        BrowserStates {
            stack: open(groups, Some(fold), cx),
            first: open(first, Some(fold), cx),
            more: open(more, None, cx),
            held,
            loading: open(loading, Some(fold), cx),
            address,
            typed,
            tabs: true,
        }
    })
    .into()
}

impl BrowserStates {
    fn stack_frame(&self, id: &'static str, menu: &Entity<StackMenu>, cx: &App) -> ala::Div {
        frame(
            id,
            site_page(false, cx).into_any_element(),
            vec![
                pill_bar(id, label(HOST), Some(menu), cx),
                menu.clone().into_any_element(),
            ],
            cx,
        )
    }
}

impl Render for BrowserStates {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let page = |cx: &App| site_page(false, cx).into_any_element();
        let docked = |field: PhoneFind| {
            div()
                .absolute()
                .left_0()
                .right_0()
                .bottom_0()
                .px(px(KEY_ROW_MARGIN_X))
                .pb(px(KEY_ROW_MARGIN_Y))
                .child(field)
                .into_any_element()
        };
        let address = |id: &'static str, input: &Entity<InputState>, cx: &App| {
            PhoneFind::new(id, input, cx.theme().ground())
                .on_close(|_, _| {})
                .on_clear(|_, _| {})
        };
        let view = cx.entity().downgrade();
        let tabs = self.tabs.then(|| {
            tabs_sheet(
                "browser-tabs-sheet",
                &[
                    TabRow {
                        id: 0,
                        title: TITLE.into(),
                        host: HOST.into(),
                        current: true,
                    },
                    TabRow {
                        id: 1,
                        title: "Phone · penna".into(),
                        host: "localhost:8102".into(),
                        current: false,
                    },
                    TabRow {
                        id: 2,
                        title: "Stack menu · penna".into(),
                        host: "localhost:8102".into(),
                        current: false,
                    },
                ],
                |_, _, _| {},
                |_, _, _| {},
                |_, _| {},
                px(34.0),
                move |_, cx| {
                    let _ = view.update(cx, |this, cx| {
                        this.tabs = false;
                        cx.notify();
                    });
                },
                cx,
            )
            .into_any_element()
        });
        let reopen = cx.entity().downgrade();
        let tabs_frame = frame("browser-tabs", page(cx), tabs.into_iter().collect(), cx).when(
            !self.tabs,
            |frame| {
                frame.child(
                    div()
                        .id("browser-tabs-again")
                        .absolute()
                        .inset_0()
                        .on_click(move |_, _, cx| {
                            let _ = reopen.update(cx, |this, cx| {
                                this.tabs = true;
                                cx.notify();
                            });
                        }),
                )
            },
        );
        let failed = failed_page(ERROR, Some(Button::new("browser-retry").primary()));
        states()
            .columns(2)
            .state(
                "at rest: the page to the edges, the pill its bar, the title over the host",
                frame(
                    "browser-rest",
                    page(cx),
                    vec![pill_bar("browser-rest-pill", label(HOST), None, cx)],
                    cx,
                ),
            )
            .state(
                "loading: the line along the top fills to the page's estimate, Reload reads Stop and Pick element is off",
                frame(
                    "browser-loading",
                    site_page(true, cx).into_any_element(),
                    vec![
                        load_line(0.62),
                        pill_bar("browser-loading-pill", label(HOST), Some(&self.loading), cx),
                        self.loading.clone().into_any_element(),
                    ],
                    cx,
                ),
            )
            .state(
                "the stack: Back, Forward and Reload nearest, the address, then Tabs, Pick element and Open on this phone; the pane's own chips behind More",
                self.stack_frame("browser-stack", &self.stack, cx),
            )
            .state(
                "no history: Back and Forward off",
                self.stack_frame("browser-first", &self.first, cx),
            )
            .state(
                "held: Back under the finger, one stroke from the pill",
                self.stack_frame("browser-held", &self.held, cx),
            )
            .state(
                "More picked: Overview, Rename, Close and New in zz",
                self.stack_frame("browser-more", &self.more, cx),
            )
            .state(
                "address: the field takes the pill's place over the keyboard",
                frame(
                    "browser-address",
                    page(cx),
                    vec![docked(address("browser-address-field", &self.address, cx))],
                    cx,
                ),
            )
            .state(
                "address, typing: the clear at the end",
                frame(
                    "browser-typing",
                    page(cx),
                    vec![docked(address("browser-typing-field", &self.typed, cx))],
                    cx,
                ),
            )
            .state("tabs: the current one checked, a close on each, New tab", tabs_frame)
            .state(
                "a page that failed: the empty state and Try again over the pill",
                frame(
                    "browser-failed",
                    div()
                        .size_full()
                        .pb(placement().zone())
                        .child(failed)
                        .into_any_element(),
                    vec![pill_bar(
                        "browser-failed-pill",
                        PillLabel {
                            title: "docs.zzmux.sh/phone.html".into(),
                            ..label("docs.zzmux.sh")
                        },
                        None,
                        cx,
                    )],
                    cx,
                ),
            )
            .state(
                "picking an element: the pill says so, a tap on it stops",
                frame(
                    "browser-picking",
                    page(cx),
                    vec![pill_bar("browser-picking-pill", label("Picking an element"), None, cx)],
                    cx,
                ),
            )
            .state(
                "copied: the pill says so with a check",
                frame(
                    "browser-picked",
                    page(cx),
                    vec![pill_bar(
                        "browser-picked-pill",
                        PillLabel {
                            icon: IconName::Check,
                            title: "Element context + screenshot copied".into(),
                            ..label(HOST)
                        },
                        None,
                        cx,
                    )],
                    cx,
                ),
            )
    }
}
