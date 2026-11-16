use ala::{AnyElement, App, Div, Pixels, Window};
use penna::blocks::chat::{self, TrayPlan};

pub use zz_client::agent_timeline::plan_chip_label;

fn tray_plan(plan: &str) -> Option<TrayPlan> {
    let (label, current) = plan_chip_label(plan)?;
    Some(TrayPlan {
        label: label.into(),
        current: current.map(Into::into),
        items: super::plan_entries(plan),
    })
}

pub fn plan_tray(
    id: &str,
    plan: &str,
    open: bool,
    on_toggle: impl Fn(&mut Window, &mut App) + 'static,
    cx: &App,
) -> Option<Div> {
    Some(chat::plan_tray(id, tray_plan(plan)?, open, on_toggle, cx))
}

pub fn phone_plan_tray(
    id: &str,
    plan: &str,
    open: bool,
    max_list_height: Pixels,
    on_toggle: impl Fn(&mut Window, &mut App) + 'static,
    cx: &App,
) -> Option<AnyElement> {
    Some(chat::phone_plan_tray(
        id,
        tray_plan(plan)?,
        open,
        max_list_height,
        on_toggle,
        cx,
    ))
}
