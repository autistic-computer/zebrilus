use std::rc::Rc;

use ala::{
    App, Div, ElementId, IntoElement, ParentElement as _, Pixels, SharedString, Styled as _,
    Window, div, px,
};
use penna::{
    Icon, Sizable as _, Size, Space,
    blocks::sheet::{
        BottomSheet, bottom_sheet, close_sheet, sheet_close, sheet_ground, sheet_groups,
    },
    blocks::touch::{
        ACTION_BAR_BOTTOM, PhoneEmpty, TOUCH_TARGET, TouchGroup, TouchRow, action_bar,
    },
    components::button::{Button, ButtonLook, ButtonVariants as _},
    v_flex,
};

use crate::IconName;

pub const FAILED_TOP: f32 = 176.0;
const CLOSE_GLYPH: f32 = 20.0;

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
