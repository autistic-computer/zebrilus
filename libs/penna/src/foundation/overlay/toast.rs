use ala::{App, Global, WindowHandle};

use crate::{blocks::notification::Notification, foundation::overlay::root::Root};

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

pub fn replace_toast(notification: Notification, cx: &mut App) {
    let Some(host) = cx.try_global::<ToastHost>().map(|host| host.0) else {
        return;
    };
    cx.defer(move |cx| {
        let _ = host.update(cx, |root, window, cx| {
            root.replace_notification(notification, window, cx);
        });
    });
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

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

    #[ala::test]
    fn a_keyed_toast_is_replaced_in_place(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let (_, cx) = cx.add_window_view(|window, cx| {
            let content = cx.new(|_| Content);
            Root::new(content, window, cx)
        });
        cx.update(|window, cx| set_toast_host(window.window_handle().downcast().unwrap(), cx));
        cx.update(|_, cx| replace_toast(Notification::info("Nothing to replace").key("job"), cx));
        cx.run_until_parked();
        assert_eq!(cx.update(|window, cx| window.notifications(cx).len()), 0);

        cx.update(|_, cx| push_toast(Notification::info("Downloading").key("job"), cx));
        cx.run_until_parked();
        let first = cx.update(|window, cx| window.notifications(cx));
        cx.update(|_, cx| replace_toast(Notification::info("Verifying").key("job"), cx));
        cx.update(|_, cx| push_toast(Notification::info("Ready").key("job"), cx));
        cx.run_until_parked();
        let last = cx.update(|window, cx| window.notifications(cx));
        assert_eq!(last.len(), 1);
        assert_eq!(last[0].entity_id(), first[0].entity_id());
        assert_eq!(
            cx.update(|_, cx| last[0].read(cx).message.clone()),
            Some("Ready".into())
        );
    }

    #[ala::test]
    fn an_unkeyed_repeat_replaces_the_toast_showing(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let (_, cx) = cx.add_window_view(|window, cx| {
            let content = cx.new(|_| Content);
            Root::new(content, window, cx)
        });
        cx.update(|window, cx| set_toast_host(window.window_handle().downcast().unwrap(), cx));
        cx.update(|_, cx| push_toast(Notification::success("Reloaded"), cx));
        cx.run_until_parked();
        let first = cx.update(|window, cx| window.notifications(cx));

        cx.executor().advance_clock(Duration::from_secs(4));
        cx.update(|_, cx| push_toast(Notification::success("Reloaded"), cx));
        cx.run_until_parked();
        let repeated = cx.update(|window, cx| window.notifications(cx));
        assert_eq!(repeated.len(), 1);
        assert_eq!(repeated[0].entity_id(), first[0].entity_id());

        cx.executor().advance_clock(Duration::from_secs(4));
        cx.run_until_parked();
        assert_eq!(cx.update(|window, cx| window.notifications(cx).len()), 1);

        cx.update(|_, cx| push_toast(Notification::warning("Reloaded"), cx));
        cx.update(|_, cx| push_toast(Notification::success("Reloaded").key("reload"), cx));
        cx.run_until_parked();
        assert_eq!(cx.update(|window, cx| window.notifications(cx).len()), 3);

        cx.executor().advance_clock(Duration::from_millis(1200));
        cx.run_until_parked();
        assert_eq!(cx.update(|window, cx| window.notifications(cx).len()), 2);
    }
}
