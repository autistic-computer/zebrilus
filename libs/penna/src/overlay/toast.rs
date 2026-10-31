use ala::{App, Global, WindowHandle};

use super::{notification::Notification, root::Root};

struct ToastHost(WindowHandle<Root>);

impl Global for ToastHost {}

pub fn set_toast_host(handle: WindowHandle<Root>, cx: &mut App) {
    cx.set_global(ToastHost(handle));
}

pub fn push_toast(notification: Notification, cx: &mut App) {
    let Some(host) = cx.try_global::<ToastHost>().map(|host| host.0) else {
        log::warn!("push_toast needs set_toast_host first; the notification was dropped");
        return;
    };
    cx.defer(move |cx| {
        let _ = host.update(cx, |root, window, cx| {
            root.push_notification(notification, window, cx);
        });
    });
}

#[cfg(test)]
mod tests {
    use ala::{AppContext as _, Context, Render, TestAppContext, Window, div};

    use super::*;
    use crate::WindowExt as _;

    struct Content;

    impl Render for Content {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl ala::IntoElement {
            div()
        }
    }

    #[ala::test]
    fn notifications_survive_an_active_host_window_update(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let (_, cx) = cx.add_window_view(|window, cx| {
            let content = cx.new(|_| Content);
            Root::new(content, window, cx)
        });
        cx.update(|window, cx| set_toast_host(window.window_handle().downcast().unwrap(), cx));
        cx.update(|_, cx| push_toast(Notification::info("Settings feedback"), cx));
        cx.run_until_parked();
        assert_eq!(cx.update(|window, cx| window.notifications(cx).len()), 1);
    }
}
