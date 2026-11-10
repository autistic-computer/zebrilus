use ala::{App, Div, SharedString, Window, div, prelude::*, px};

use crate::{
    ActiveTheme as _, Icon, IconName, PaintState, Panel, PanelLook, StyledExt as _, Tab, TabLook,
    TextStyle, components::icon::cap_center_drop, h_flex, muted_on, v_flex,
};

use super::{
    card::CARD_GUTTER,
    message::{PlanState, plan_list},
};

const CHIP_FONT_SIZE: f32 = 12.0;

#[derive(Clone, Debug)]
pub struct TrayPlan {
    pub label: SharedString,
    pub current: Option<SharedString>,
    pub items: Vec<(PlanState, SharedString)>,
}

pub fn plan_tray(
    id: &str,
    plan: TrayPlan,
    open: bool,
    on_toggle: impl Fn(&mut Window, &mut App) + 'static,
    cx: &App,
) -> Div {
    let chip_drop = cap_center_drop(px(CHIP_FONT_SIZE), px(CHIP_FONT_SIZE), cx);
    let inset = (px(CARD_GUTTER) - cx.theme().row_inset).max(px(0.0));
    let panel = open.then(|| {
        Panel::new(
            SharedString::from(format!("{id}-tray-panel")),
            PanelLook::Inline,
        )
        .v_flex()
        .child(div().px(inset).py_1().child(plan_list(&plan.items, cx)))
    });
    let chip = {
        let state = if open {
            PaintState::Current
        } else {
            PaintState::Rest
        };
        let muted = muted_on(cx.theme().ground_at(TabLook::Tray, state), cx);
        Tab::new(SharedString::from(format!("{id}-plan")), TabLook::Tray)
            .state(state)
            .debug_selector(|| "chat-plan-chip".to_owned())
            .h_flex()
            .min_w_0()
            .h(px(24.0))
            .px_2()
            .gap_1p5()
            .click_cursor()
            .type_style(TextStyle::Small, cx)
            .child(
                div()
                    .flex_none()
                    .relative()
                    .top(chip_drop)
                    .child(Icon::new(IconName::CircleCheck).size(px(CHIP_FONT_SIZE))),
            )
            .child(div().flex_none().child(plan.label))
            .when_some(plan.current, |chip, current| {
                chip.child(
                    div()
                        .min_w_0()
                        .overflow_hidden()
                        .text_ellipsis()
                        .whitespace_nowrap()
                        .text_color(muted)
                        .child(current),
                )
            })
            .child(
                div()
                    .flex_none()
                    .relative()
                    .top(chip_drop)
                    .text_color(muted)
                    .child(
                        Icon::new(if open {
                            IconName::ChevronDown
                        } else {
                            IconName::ChevronUp
                        })
                        .size(px(CHIP_FONT_SIZE)),
                    ),
            )
            .on_click(move |_, window, cx| {
                on_toggle(window, cx);
                cx.stop_propagation();
            })
    };
    v_flex()
        .w_full()
        .gap_1()
        .when_some(panel, |tray, panel| {
            tray.child(panel.w_full().list_inset(cx).py(px(4.0)))
        })
        .child(h_flex().w_full().child(div().min_w_0().child(chip)))
}
