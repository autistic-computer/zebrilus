use ala::{App, Div, Window};
use penna::blocks::chat::{self, TaskItem, TaskTrayAction, TrayPanel, TrayPlan};
use zz_protocol::AgentTaskWire;

use crate::IconName;

pub fn task_kind_icon(kind: &str) -> IconName {
    match kind {
        "shell" => IconName::Terminal,
        "agent" => IconName::Bot,
        "monitor" => IconName::Inspector,
        _ => IconName::Cpu,
    }
}

pub use zz_client::agent_timeline::{plan_chip_label, task_tray_label};

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
    let plan = plan.and_then(|source| {
        plan_chip_label(source).map(|(label, current)| TrayPlan {
            label: label.into(),
            current: current.map(Into::into),
            items: super::plan_entries(source),
        })
    });
    let items: Vec<TaskItem> = tasks
        .iter()
        .map(|task| TaskItem {
            id: task.id.clone().into(),
            icon: task_kind_icon(&task.kind),
            description: task.description.clone().into(),
            reveal: task.tool_call_id.clone().map(Into::into),
        })
        .collect();
    chat::task_tray(
        id,
        plan,
        &items,
        task_tray_label(tasks.len()).into(),
        open,
        enabled,
        phase,
        on_action,
        cx,
    )
}
