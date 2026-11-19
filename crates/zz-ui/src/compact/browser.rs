use std::rc::Rc;

use ala::{
    AnyElement, App, Div, ElementId, IntoElement, ParentElement as _, Pixels, SharedString,
    Styled as _, Window, div, prelude::FluentBuilder as _, px,
};
use penna::{
    ActiveTheme as _, Emphasis, Ground, Icon, Sizable as _, Size, Space, StyledExt as _, TextStyle,
    Tone,
    blocks::sheet::{
        BottomSheet, bottom_sheet, close_sheet, sheet_close, sheet_ground, sheet_groups,
    },
    blocks::touch::{
        ACTION_BAR_BOTTOM, PhoneEmpty, TOUCH_TARGET, TouchGroup, TouchHeader, TouchRow, action_bar,
        phone_field_at_rest, phone_suggestions,
    },
    components::button::{Button, ButtonLook, ButtonVariants as _},
    v_flex,
};

use crate::IconName;

pub const FAILED_TOP: f32 = 176.0;
const CLOSE_GLYPH: f32 = 20.0;
const MARK: f32 = 20.0;
const NEW_TAB_GAP: f32 = 24.0;
pub const RECENT_ROWS: usize = 5;
pub const SUGGESTED_ROWS: usize = 4;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PageRow {
    pub title: SharedString,
    pub url: SharedString,
    pub host: SharedString,
}

type PickPage = Rc<dyn Fn(SharedString, &mut Window, &mut App)>;

fn site_mark(host: &str, cx: &App) -> Div {
    let letter = host
        .trim_start_matches("www.")
        .chars()
        .find(|character| character.is_alphanumeric())
        .map_or_else(String::new, |character| character.to_uppercase().collect());
    let theme = cx.theme();
    let paint = theme.paint(Tone::Neutral, Emphasis::Soft);
    div()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(px(MARK))
        .rounded(theme.control_corner(px(MARK)))
        .bg(paint.fill)
        .type_style_strong(TextStyle::Small, cx)
        .text_color(paint.text)
        .child(letter)
}

fn page_row(id: &'static str, index: usize, page: &PageRow, pick: &PickPage) -> TouchRow {
    let pick = Rc::clone(pick);
    let url = page.url.clone();
    TouchRow::new((id, index), page.title.clone())
        .detail(page.host.clone())
        .on_click(move |_, window, cx| pick(url.clone(), window, cx))
}

#[must_use]
pub fn new_tab_page(
    recents: &[PageRow],
    resting: bool,
    on_pick: impl Fn(SharedString, &mut Window, &mut App) + 'static,
    on_address: impl Fn(&mut Window, &mut App) + 'static,
    ground: Ground,
    cx: &App,
) -> Div {
    let pick: PickPage = Rc::new(on_pick);
    let rows: Vec<TouchRow> = recents
        .iter()
        .take(RECENT_ROWS)
        .enumerate()
        .map(|(index, page)| {
            page_row("browser-recent", index, page, &pick).lead(site_mark(&page.host, cx), MARK)
        })
        .collect();
    v_flex()
        .size_full()
        .child(
            v_flex()
                .flex_1()
                .min_h_0()
                .justify_center()
                .gap(px(NEW_TAB_GAP))
                .px(Space::S16)
                .child(PhoneEmpty::new(IconName::Globe, "New tab"))
                .when(!rows.is_empty(), |page| {
                    page.child(
                        v_flex()
                            .w_full()
                            .child(TouchHeader::new("Recent"))
                            .child(TouchGroup::new().ruled().rows(rows)),
                    )
                }),
        )
        .when(resting, |page| {
            page.child(div().flex_none().px(Space::S16).child(phone_field_at_rest(
                "browser-address-rest",
                IconName::Globe,
                "Search or enter address",
                ground,
                move |_, window, cx| on_address(window, cx),
                cx,
            )))
        })
}

#[must_use]
pub fn address_suggestions(
    pages: &[PageRow],
    on_pick: impl Fn(SharedString, &mut Window, &mut App) + 'static,
    ground: Ground,
    cx: &App,
) -> Option<AnyElement> {
    let pick: PickPage = Rc::new(on_pick);
    let rows = pages
        .iter()
        .take(SUGGESTED_ROWS)
        .enumerate()
        .rev()
        .map(|(index, page)| {
            let row = page_row("browser-suggestion", index, page, &pick);
            row.icon(IconName::History)
        })
        .collect();
    phone_suggestions(rows, ground, cx)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TabRow {
    pub id: u64,
    pub title: SharedString,
    pub host: SharedString,
    pub current: bool,
}

#[must_use]
pub fn failed_page(error: impl Into<SharedString>, retry: Option<Button>) -> Div {
    v_flex()
        .size_full()
        .pt(px(FAILED_TOP))
        .child(
            PhoneEmpty::new(IconName::TriangleAlert, "This page couldn\u{2019}t load").line(error),
        )
        .child(div().flex_1())
        .children(retry.map(|retry| {
            div().flex_none().px(Space::S16).child(
                retry
                    .label("Try again")
                    .with_size(Size::TouchLarge)
                    .w_full(),
            )
        }))
}

type TabHandler = Rc<dyn Fn(u64, &mut Window, &mut App)>;

#[allow(clippy::too_many_arguments)]
#[must_use]
pub fn tabs_sheet(
    id: impl Into<ElementId>,
    tabs: &[TabRow],
    on_select: impl Fn(u64, &mut Window, &mut App) + 'static,
    on_close: impl Fn(u64, &mut Window, &mut App) + 'static,
    on_new: impl Fn(&mut Window, &mut App) + 'static,
    bottom_inset: Pixels,
    on_dismiss: impl Fn(&mut Window, &mut App) + 'static,
    cx: &App,
) -> BottomSheet {
    let id = id.into();
    let ground = sheet_ground(cx);
    let on_select: TabHandler = Rc::new(on_select);
    let on_close: TabHandler = Rc::new(on_close);
    let closable = tabs.len() > 1;
    let rows: Vec<TouchRow> = tabs
        .iter()
        .map(|tab| {
            let tab_id = tab.id;
            let row =
                TouchRow::new(("browser-tab", tab_id), tab.title.clone()).detail(tab.host.clone());
            let row = if tab.current {
                row.checked()
            } else {
                row.slot()
            };
            let row = if closable {
                let on_close = Rc::clone(&on_close);
                row.end(
                    Button::new(("browser-tab-close", tab_id))
                        .ghost()
                        .look(ButtonLook::Quiet)
                        .compact()
                        .tab_stop(false)
                        .size(px(TOUCH_TARGET))
                        .child(Icon::new(IconName::Xmark).size(px(CLOSE_GLYPH)))
                        .on(ground)
                        .on_click(move |_, window, cx| {
                            cx.stop_propagation();
                            on_close(tab_id, window, cx);
                        }),
                )
            } else {
                row
            };
            let on_select = Rc::clone(&on_select);
            let picked = id.clone();
            row.on_click(move |_, window, cx| {
                on_select(tab_id, window, cx);
                close_sheet(picked.clone(), cx);
            })
        })
        .collect();
    let made = id.clone();
    let closed = id.clone();
    bottom_sheet(
        id,
        "Tabs",
        [sheet_close("browser-tabs-close", cx)
            .on_click(move |_, _, cx| close_sheet(closed.clone(), cx))
            .into_any_element()],
        div()
            .flex()
            .flex_col()
            .child(sheet_groups().child(TouchGroup::new().ruled().rows(rows)))
            .child(
                action_bar(px(ACTION_BAR_BOTTOM)).child(
                    Button::new("browser-new-tab")
                        .primary()
                        .label("New tab")
                        .with_size(Size::TouchLarge)
                        .w_full()
                        .on(ground)
                        .on_click(move |_, window, cx| {
                            on_new(window, cx);
                            close_sheet(made.clone(), cx);
                        }),
                ),
            ),
        bottom_inset,
        on_dismiss,
    )
}
