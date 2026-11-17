use ala::{App, Window};

pub(crate) fn set_up_agents(window: &mut Window, cx: &mut App) {
    let task = cx
        .background_executor()
        .spawn(async { zz_daemon::change_agent_hooks(zz_daemon::AgentHookChange::Install) });
    window
        .spawn(cx, async move |window| {
            let result = task.await;
            let _ = window.update(|_, cx| {
                crate::window::toast::push(
                    zz_ui::settings::providers::agent_setup_notification(result, None),
                    cx,
                );
            });
        })
        .detach();
}
