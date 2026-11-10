use ala::{App, Div, Window};
use penna::blocks::chat::{self, TrayPlan};

pub use zz_client::agent_timeline::plan_chip_label;

pub fn plan_tray(
    id: &str,
    plan: &str,
    open: bool,
    on_toggle: impl Fn(&mut Window, &mut App) + 'static,
    cx: &App,
) -> Option<Div> {
    let (label, current) = plan_chip_label(plan)?;
    let plan = TrayPlan {
        label: label.into(),
        current: current.map(Into::into),
        items: super::plan_entries(plan),
    };
    Some(chat::plan_tray(id, plan, open, on_toggle, cx))
}
