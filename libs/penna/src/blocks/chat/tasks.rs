use std::rc::Rc;

use ala::{App, Div, SharedString, Window, div, prelude::*, px};

use crate::{
    ActiveTheme as _, Colorize as _, Disableable as _, Emphasis, Icon, IconName, PaintState, Panel,
    PanelLook, Row, RowLook, StyledExt as _, Tab, TabLook, TextStyle, Tone,
    components::button::{Button, ButtonLook},
    components::icon::cap_center_drop,
    h_flex, muted_on, v_flex,
};

use super::{
    card::{CARD_GUTTER, card_button},
    composer::chrome_button,
    message::{PlanState, plan_list},
};

const TASK_ROW_HEIGHT: f32 = 28.0;
const TASK_ROW_FONT_SIZE: f32 = 12.0;
const TASK_ICON_SIZE: f32 = 13.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayPanel {
    Plan,
    Tasks,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TaskTrayAction {
    Toggle(TrayPanel),
    Stop(SharedString),
    Reveal(SharedString),
}

#[derive(Clone, Debug)]
pub struct TaskItem {
    pub id: SharedString,
    pub icon: IconName,
    pub description: SharedString,
    pub reveal: Option<SharedString>,
}

#[derive(Clone, Debug)]
pub struct TrayPlan {
    pub label: SharedString,
    pub current: Option<SharedString>,
    pub items: Vec<(PlanState, SharedString)>,
}

#[allow(clippy::too_many_arguments)]
pub fn task_tray(
    id: &str,
    plan: Option<TrayPlan>,
    tasks: &[TaskItem],
    tasks_label: SharedString,
    open: Option<TrayPanel>,
    enabled: bool,
    phase: f32,
    on_action: impl Fn(TaskTrayAction, &mut Window, &mut App) + 'static,
    cx: &App,
) -> Option<Div> {
    if tasks.is_empty() && plan.is_none() {
        return None;
    }
    let on_action: Rc<dyn Fn(TaskTrayAction, &mut Window, &mut App)> = Rc::new(on_action);
    let toggle = Rc::clone(&on_action);
    let row_paint = cx.theme().paint(Tone::Neutral, Emphasis::Ghost);
    let task_drop = cap_center_drop(px(TASK_ROW_FONT_SIZE), px(TASK_ICON_SIZE), cx);
    let chip_drop = cap_center_drop(px(TASK_ROW_FONT_SIZE), px(TASK_ROW_FONT_SIZE), cx);
    let inset = (px(CARD_GUTTER) - cx.theme().row_inset).max(px(0.0));
    let rows = tasks.iter().enumerate().map(|(index, task)| {
        let stop = Rc::clone(&on_action);
        let reveal = Rc::clone(&on_action);
        let task_id = task.id.clone();
        let tool_call = task.reveal.clone();
        Row::new(
            (SharedString::from(format!("{id}-task")), index),
            RowLook::Menu,
        )
        .row_height(px(TASK_ROW_HEIGHT))
        .interactive(tool_call.is_some())
        .h_flex()
        .w_full()
        .h(px(TASK_ROW_HEIGHT))
        .flex_none()
        .gap_2()
        .px(inset)
        .overflow_hidden()
        .type_style(TextStyle::Small, cx)
        .when_some(tool_call, |row, tool_call| {
            row.cursor_pointer().on_click(move |_, window, cx| {
                reveal(TaskTrayAction::Reveal(tool_call.clone()), window, cx);
                cx.stop_propagation();
            })
        })
        .text_color(row_paint.muted_at(PaintState::Rest))
        .child(
            div()
                .flex_none()
                .relative()
                .top(task_drop)
                .child(Icon::new(task.icon.clone()).size(px(TASK_ICON_SIZE))),
        )
        .child(
            div()
                .min_w_0()
                .flex_1()
                .overflow_hidden()
                .text_ellipsis()
                .whitespace_nowrap()
                .child(task.description.clone()),
        )
        .child(
            card_button(
                Button::compact_icon(
                    (SharedString::from(format!("{id}-task-stop")), index),
                    IconName::Stop,
                ),
                cx,
            )
            .debug_selector(move || format!("chat-task-stop-{index}"))
            .tooltip("Stop")
            .disabled(!enabled)
            .on_click(move |_, window, cx| {
                stop(TaskTrayAction::Stop(task_id.clone()), window, cx);
                cx.stop_propagation();
            }),
        )
    });
    let plan_toggle = Rc::clone(&on_action);
    let tray_panel = || {
        Panel::new(
            SharedString::from(format!("{id}-tray-panel")),
            PanelLook::Inline,
        )
        .v_flex()
    };
    let panel = match open {
        Some(TrayPanel::Tasks) if !tasks.is_empty() => Some(tray_panel().children(rows)),
        Some(TrayPanel::Plan) => plan.as_ref().map(|plan| {
            tray_panel().child(div().px(inset).py_1().child(plan_list(&plan.items, cx)))
        }),
        _ => None,
    };
    let plan_chip = plan.map(|plan| {
        let open = open == Some(TrayPanel::Plan);
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
            .cursor_pointer()
            .type_style(TextStyle::Small, cx)
            .child(
                div()
                    .flex_none()
                    .relative()
                    .top(chip_drop)
                    .child(Icon::new(IconName::CircleCheck).size(px(TASK_ROW_FONT_SIZE))),
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
                        .size(px(TASK_ROW_FONT_SIZE)),
                    ),
            )
            .on_click(move |_, window, cx| {
                plan_toggle(TaskTrayAction::Toggle(TrayPanel::Plan), window, cx);
                cx.stop_propagation();
            })
    });
    let tasks_open = open == Some(TrayPanel::Tasks);
    let tasks_chip = (!tasks.is_empty()).then(|| {
        chrome_button(SharedString::from(format!("{id}-tasks")))
            .debug_selector(|| "chat-task-tray".to_owned())
            .look(ButtonLook::Tray)
            .flex_none()
            .icon(super::spinner(phase))
            .label(tasks_label)
            .tooltip(if tasks_open {
                "Hide background tasks"
            } else {
                "Show background tasks"
            })
            .text_color(cx.theme().foreground.muted())
            .on_click(move |_, window, cx| {
                toggle(TaskTrayAction::Toggle(TrayPanel::Tasks), window, cx);
                cx.stop_propagation();
            })
    });
    Some(
        v_flex()
            .w_full()
            .gap_1()
            .when_some(panel, |tray, panel| {
                tray.child(panel.w_full().list_inset(cx).py(px(4.0)))
            })
            .child(
                h_flex()
                    .w_full()
                    .gap_2()
                    .justify_between()
                    .child(div().min_w_0().children(plan_chip))
                    .children(tasks_chip),
            ),
    )
}
