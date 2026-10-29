use ala::{App, Window};
use zz_ui::notification::Notification;

pub(crate) fn set_up_agents(window: &mut Window, cx: &mut App) {
    let task = cx
        .background_executor()
        .spawn(async { zz_cli::install_agent_hooks() });
    window
        .spawn(cx, async move |window| {
            let result = task.await;
            let _ = window.update(|_, cx| {
                let notification = match result {
                    Ok(report) => Notification::success(report).title("Agents set up"),
                    Err(report) => Notification::error(report).title("Agent setup failed"),
                };
                crate::window::toast::push(notification.autohide(false), cx);
            });
        })
        .detach();
}
