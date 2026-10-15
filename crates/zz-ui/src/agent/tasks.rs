use std::rc::Rc;

use zz_gpui::{App, Div, SharedString, Window, div, prelude::*, px};
use zz_protocol::AgentTaskWire;

use crate::{
    ActiveTheme as _, Colorize as _, Disableable as _, Emphasis, Icon, IconName, PaintState,
    StatefulSurfaceExt as _, StyledExt as _, SurfaceRole, Tone,
    button::Button,
    h_flex,
    picker::{muted_on, row_surface},
    roles, v_flex,
};

use super::controls::agent_chrome_button;

const TASK_ROW_HEIGHT: f32 = 28.0;
const TASK_ROW_FONT_SIZE: f32 = 12.0;
const TASK_ROW_LINE_HEIGHT: f32 = 16.0;
const TASK_ICON_SIZE: f32 = 13.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayPanel {
    Plan,
    Tasks,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TaskTrayAction {
    Toggle(TrayPanel),
    Stop(String),
    Reveal(String),
}

pub fn task_kind_icon(kind: &str) -> IconName {
    match kind {
        "shell" => IconName::Terminal,
        "agent" => IconName::Bot,
        "monitor" => IconName::Inspector,
        _ => IconName::Cpu,
    }
}

pub fn task_tray_label(count: usize) -> String {
    format!("{count} running")
}

pub fn plan_chip_label(source: &str) -> Option<(String, Option<String>)> {
    let (done, total, current) = super::plan_progress(source);
    (total > 0).then(|| (format!("Plan {done}/{total}"), current.map(str::to_owned)))
}

#[allow(clippy::too_many_arguments)]
pub fn task_tray(
    id: &str,
    plan: Option<&str>,
    tasks: &[AgentTaskWire],
    open: Option<TrayPanel>,
    enabled: bool,
    phase: f32,
    on_action: impl Fn(TaskTrayAction, &mut Window, &mut App) + 'static,
    cx: &App,
) -> Option<Div> {
    let plan = plan.and_then(|source| plan_chip_label(source).map(|label| (source, label)));
    if tasks.is_empty() && plan.is_none() {
        return None;
    }
    let on_action: Rc<dyn Fn(TaskTrayAction, &mut Window, &mut App)> = Rc::new(on_action);
    let toggle = Rc::clone(&on_action);
    let row_paint = cx.theme().paint(Tone::Neutral, Emphasis::Ghost);
    let task_drop = crate::icon::cap_center_drop(px(TASK_ROW_FONT_SIZE), px(TASK_ICON_SIZE), cx);
    let chip_drop =
        crate::icon::cap_center_drop(px(TASK_ROW_FONT_SIZE), px(TASK_ROW_FONT_SIZE), cx);
    let rows = tasks.iter().enumerate().map(|(index, task)| {
        let stop = Rc::clone(&on_action);
        let reveal = Rc::clone(&on_action);
        let task_id = task.id.clone();
        let tool_call = task.tool_call_id.clone();
        h_flex()
            .id((SharedString::from(format!("{id}-task")), index))
            .w_full()
            .h(px(TASK_ROW_HEIGHT))
            .flex_none()
            .gap_2()
            .px((px(super::presentation::CARD_GUTTER) - cx.theme().row_inset).max(px(0.0)))
            .overflow_hidden()
            .text_size(crate::rems_from_px(TASK_ROW_FONT_SIZE))
            .line_height(px(TASK_ROW_LINE_HEIGHT))
            .when_some(tool_call, |row, tool_call| {
                row_surface(
                    row,
                    SurfaceRole::MENU_ITEM,
                    PaintState::Rest,
                    px(TASK_ROW_HEIGHT),
                    cx,
                )
                .cursor_pointer()
                .on_click(move |_, window, cx| {
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
                    .child(Icon::new(task_kind_icon(&task.kind)).size(px(TASK_ICON_SIZE))),
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
                super::presentation::card_button(
                    Button::compact_icon(
                        (SharedString::from(format!("{id}-task-stop")), index),
                        IconName::Stop,
                    ),
                    cx,
                )
                .debug_selector(move || format!("agent-task-stop-{index}"))
                .tooltip("Stop")
                .disabled(!enabled)
                .on_click(move |_, window, cx| {
                    stop(TaskTrayAction::Stop(task_id.clone()), window, cx);
                    cx.stop_propagation();
                }),
            )
    });
    let plan_toggle = Rc::clone(&on_action);
    let panel = match open {
        Some(TrayPanel::Tasks) if !tasks.is_empty() => Some(v_flex().children(rows)),
        Some(TrayPanel::Plan) => plan.as_ref().map(|(source, _)| {
            v_flex().child(
                div()
                    .px((px(super::presentation::CARD_GUTTER) - cx.theme().row_inset).max(px(0.0)))
                    .py_1()
                    .child(super::render_plan_items(source, cx)),
            )
        }),
        _ => None,
    };
    let plan_chip = plan.map(|(_, (label, current))| {
        let open = open == Some(TrayPanel::Plan);
        let state = if open {
            PaintState::Current
        } else {
            PaintState::Rest
        };
        let muted = muted_on(&cx.theme().layer(roles::AGENT_TRAY, state), cx);
        h_flex()
            .id(SharedString::from(format!("{id}-plan")))
            .debug_selector(|| "agent-plan-chip".to_owned())
            .min_w_0()
            .h(px(24.0))
            .px_2()
            .gap_1p5()
            .cursor_pointer()
            .text_size(crate::rems_from_px(TASK_ROW_FONT_SIZE))
            .line_height(px(TASK_ROW_LINE_HEIGHT))
            .surface_interactive(roles::AGENT_TRAY, state, cx)
            .child(
                div()
                    .flex_none()
                    .relative()
                    .top(chip_drop)
                    .child(Icon::new(IconName::CircleCheck).size(px(TASK_ROW_FONT_SIZE))),
            )
            .child(div().flex_none().child(label))
            .when_some(current, |chip, current| {
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
        agent_chrome_button(SharedString::from(format!("{id}-tasks")))
            .debug_selector(|| "agent-task-tray".to_owned())
            .surface_role(roles::AGENT_TRAY)
            .flex_none()
            .icon(super::presentation::spinner(phase))
            .label(task_tray_label(tasks.len()))
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
                tray.child(
                    panel
                        .w_full()
                        .list_inset(cx)
                        .py(px(4.0))
                        .surface(roles::INLINE_PANEL, cx),
                )
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
