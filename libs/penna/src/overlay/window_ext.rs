use ala::{App, Context, Entity, Window};

use super::{
    dialog::{AlertDialog, Dialog},
    notification::Notification,
    root::Root,
};

pub trait WindowExt: Sized {
    fn open_dialog<F>(&mut self, cx: &mut App, build: F)
    where
        F: Fn(Dialog, &mut Window, &mut App) -> Dialog + 'static;

    fn open_alert_dialog<F>(&mut self, cx: &mut App, build: F)
    where
        F: Fn(AlertDialog, &mut Window, &mut App) -> AlertDialog + 'static;

    fn has_active_dialog(&mut self, cx: &mut App) -> bool;

    fn close_dialog(&mut self, cx: &mut App);

    fn close_all_dialogs(&mut self, cx: &mut App);

    fn push_notification(&mut self, notification: Notification, cx: &mut App);

    fn dismiss_notification(&mut self, key: &str, cx: &mut App) -> bool;

    fn clear_notifications(&mut self, cx: &mut App);

    fn notifications(&mut self, cx: &mut App) -> Vec<Entity<Notification>>;

    fn selected_text(&mut self, cx: &mut App) -> String;

    fn has_text_selection(&mut self, cx: &mut App) -> bool;

    fn clear_text_selection(&mut self, cx: &mut App);

    fn end_text_selection(&mut self, cx: &mut App);
}

fn update_root<R>(
    window: &mut Window,
    cx: &mut App,
    call: &str,
    f: impl FnOnce(&mut Root, &mut Window, &mut Context<Root>) -> R,
) -> Option<R> {
    let result = Root::update(window, cx, f);
    if result.is_none() {
        log::warn!("{call} needs a window whose root view is a penna::Root; nothing was done");
    }
    result
}

fn read_root<'a>(window: &'a Window, cx: &'a App, call: &str) -> Option<&'a Root> {
    let root = Root::read(window, cx);
    if root.is_none() {
        log::warn!("{call} needs a window whose root view is a penna::Root; nothing was read");
    }
    root
}

impl WindowExt for Window {
    fn open_dialog<F>(&mut self, cx: &mut App, build: F)
    where
        F: Fn(Dialog, &mut Window, &mut App) -> Dialog + 'static,
    {
        update_root(self, cx, "open_dialog", move |root, window, cx| {
            root.open_dialog(build, window, cx);
        });
    }

    fn open_alert_dialog<F>(&mut self, cx: &mut App, build: F)
    where
        F: Fn(AlertDialog, &mut Window, &mut App) -> AlertDialog + 'static,
    {
        self.open_dialog(cx, move |_, window, cx| {
            build(AlertDialog::new(cx), window, cx).into_dialog(cx)
        });
    }

    fn has_active_dialog(&mut self, cx: &mut App) -> bool {
        Root::read(self, cx).is_some_and(Root::has_active_dialog)
    }

    fn close_dialog(&mut self, cx: &mut App) {
        update_root(self, cx, "close_dialog", |root, window, cx| {
            root.close_dialog(window, cx);
        });
    }

    fn close_all_dialogs(&mut self, cx: &mut App) {
        update_root(self, cx, "close_all_dialogs", |root, window, cx| {
            root.close_all_dialogs(window, cx);
        });
    }

    fn push_notification(&mut self, notification: Notification, cx: &mut App) {
        update_root(self, cx, "push_notification", |root, window, cx| {
            root.push_notification(notification, window, cx);
        });
    }

    fn dismiss_notification(&mut self, key: &str, cx: &mut App) -> bool {
        update_root(self, cx, "dismiss_notification", |root, window, cx| {
            root.dismiss_notification(key, window, cx)
        })
        .unwrap_or(false)
    }

    fn clear_notifications(&mut self, cx: &mut App) {
        update_root(self, cx, "clear_notifications", |root, _, cx| {
            root.clear_notifications(cx);
        });
    }

    fn notifications(&mut self, cx: &mut App) -> Vec<Entity<Notification>> {
        read_root(self, cx, "notifications")
            .map(|root| root.notifications(cx))
            .unwrap_or_default()
    }

    fn selected_text(&mut self, cx: &mut App) -> String {
        read_root(self, cx, "selected_text")
            .map(|root| root.window_selected_text(cx))
            .unwrap_or_default()
    }

    fn has_text_selection(&mut self, cx: &mut App) -> bool {
        Root::read(self, cx).is_some_and(|root| root.has_text_selection(cx))
    }

    fn clear_text_selection(&mut self, cx: &mut App) {
        update_root(self, cx, "clear_text_selection", |root, _, cx| {
            root.clear_text_selection(cx);
        });
    }

    fn end_text_selection(&mut self, cx: &mut App) {
        update_root(self, cx, "end_text_selection", |root, _, cx| {
            root.end_text_selection(cx);
        });
    }
}
