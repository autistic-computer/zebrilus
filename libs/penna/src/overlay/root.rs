use std::rc::Rc;

use crate::{window_border, window_paddings};
use ala::{
    Anchor, AnyView, App, AppContext as _, ClipboardItem, Context, Edges, Entity, FocusHandle,
    InteractiveElement as _, IntoElement, ParentElement as _, Render, RenderOnce, StyleRefinement,
    Styled, WeakFocusHandle, Window, div, prelude::FluentBuilder as _, px,
};

use crate::{
    ADAPTIVE_CORNER_FRACTION, ActiveTheme as _, CORNER_SMOOTHING, StyledExt as _,
    text::{self, SelectionScope, TextSelectionController, WindowTextSelection},
};

use super::{
    actions::{Tab, TabPrev},
    dialog::{ANIMATION_DURATION, Dialog},
    notification::{Notification, NotificationList},
};

pub(super) const CONTEXT: &str = text::ROOT_KEY_CONTEXT;

const MAX_FOCUS_STEPS: usize = 100;

#[derive(Clone)]
struct ActiveDialog {
    instance_id: u64,
    focus_handle: FocusHandle,
    previous_focused_handle: Option<WeakFocusHandle>,
    builder: Rc<dyn Fn(Dialog, &mut Window, &mut App) -> Dialog>,
}

pub struct Root {
    style: StyleRefinement,
    view: AnyView,
    bordered: bool,
    own_layers: bool,
    active_dialogs: Vec<ActiveDialog>,
    next_dialog_id: u64,
    notification: Entity<NotificationList>,
    pending_focus_restore: Option<WeakFocusHandle>,
    pub(crate) text_selection: WindowTextSelection,
}

impl Root {
    #[must_use]
    pub fn new(view: impl Into<AnyView>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        window.set_default_corner_smoothing(CORNER_SMOOTHING);
        window.set_adaptive_corner_fraction(Some(ADAPTIVE_CORNER_FRACTION));
        Self {
            style: StyleRefinement::default(),
            view: view.into(),
            bordered: true,
            own_layers: true,
            active_dialogs: Vec::new(),
            next_dialog_id: 0,
            notification: cx.new(|_| NotificationList::new()),
            pending_focus_restore: None,
            text_selection: WindowTextSelection::default(),
        }
    }

    #[must_use]
    pub fn bordered(mut self, bordered: bool) -> Self {
        self.bordered = bordered;
        self
    }

    #[must_use]
    pub fn own_layers(mut self, own_layers: bool) -> Self {
        self.own_layers = own_layers;
        self
    }

    #[must_use]
    pub fn view(&self) -> &AnyView {
        &self.view
    }

    pub fn update<F, R>(window: &mut Window, cx: &mut App, f: F) -> Option<R>
    where
        F: FnOnce(&mut Self, &mut Window, &mut Context<Self>) -> R,
    {
        let root = window.root::<Self>().flatten()?;
        Some(root.update(cx, |root, cx| f(root, window, cx)))
    }

    #[must_use]
    pub fn read<'a>(window: &'a Window, cx: &'a App) -> Option<&'a Self> {
        Some(window.root::<Self>().flatten()?.read(cx))
    }

    pub fn render_notification_layer(
        window: &mut Window,
        cx: &mut App,
    ) -> Option<impl IntoElement + use<>> {
        let root = window.root::<Self>()??;
        let placement = cx.theme().notification.placement;

        Some(
            div()
                .absolute()
                .when(matches!(placement, Anchor::TopRight), |this| {
                    this.top_0().right_0()
                })
                .when(matches!(placement, Anchor::TopLeft), |this| {
                    this.top_0().left_0()
                })
                .when(matches!(placement, Anchor::TopCenter), |this| {
                    this.top_0().left_0().right_0().flex().justify_center()
                })
                .when(matches!(placement, Anchor::BottomRight), |this| {
                    this.bottom_0().right_0()
                })
                .when(matches!(placement, Anchor::BottomLeft), |this| {
                    this.bottom_0().left_0()
                })
                .when(matches!(placement, Anchor::BottomCenter), |this| {
                    this.bottom_0().mx_auto()
                })
                .child(root.read(cx).notification.clone()),
        )
    }

    pub fn render_dialog_layer(
        window: &mut Window,
        cx: &mut App,
    ) -> Option<impl IntoElement + use<>> {
        let root = window.root::<Self>()??;
        let active_dialogs = root.read(cx).active_dialogs.clone();
        let topmost = active_dialogs.len().checked_sub(1)?;

        let dialogs = active_dialogs
            .iter()
            .enumerate()
            .map(|(ix, active)| {
                let dialog = Dialog::new(cx);
                let mut dialog = (active.builder)(dialog, window, cx);

                dialog.focus_handle = active.focus_handle.clone();
                dialog.layer_ix = ix;
                dialog.instance_id = active.instance_id;
                dialog.props.overlay_visible = ix == topmost;
                dialog
            })
            .collect::<Vec<_>>();

        Some(div().children(dialogs))
    }

    pub fn open_dialog<F>(&mut self, build: F, window: &mut Window, cx: &mut Context<Self>)
    where
        F: Fn(Dialog, &mut Window, &mut App) -> Dialog + 'static,
    {
        let previous_focused_handle = self
            .pending_focus_restore
            .take()
            .or_else(|| window.focused(cx).map(|handle| handle.downgrade()));

        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);

        let instance_id = self.next_dialog_id;
        self.next_dialog_id = self.next_dialog_id.wrapping_add(1);
        self.active_dialogs.push(ActiveDialog {
            instance_id,
            focus_handle,
            previous_focused_handle,
            builder: Rc::new(build),
        });
        self.text_selection.clear(cx);
        cx.notify();
    }

    pub fn close_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(handle) = self.pop_dialog() {
            window.focus(&handle, cx);
        }
        self.text_selection.clear(cx);
        cx.notify();
    }

    pub(super) fn defer_close_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(handle) = self.pop_dialog() {
            let depth = self.active_dialogs.len();
            self.pending_focus_restore = Some(handle.downgrade());

            let duration = ANIMATION_DURATION;
            cx.spawn_in(window, async move |this, cx| {
                cx.background_executor().timer(duration).await;
                let _ = this.update_in(cx, |this, window, cx| {
                    if this.active_dialogs.len() == depth {
                        window.focus(&handle, cx);
                    }
                    this.pending_focus_restore = None;
                });
            })
            .detach();
        }
        self.text_selection.clear(cx);
        cx.notify();
    }

    pub fn close_all_dialogs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let restore = self
            .active_dialogs
            .first()
            .and_then(|dialog| dialog.previous_focused_handle.clone());
        self.active_dialogs.clear();

        if let Some(handle) = restore.and_then(|handle| handle.upgrade()) {
            window.focus(&handle, cx);
        }
        self.text_selection.clear(cx);
        cx.notify();
    }

    #[must_use]
    pub fn has_active_dialog(&self) -> bool {
        !self.active_dialogs.is_empty()
    }

    fn top_dialog_focus(&self) -> Option<FocusHandle> {
        self.active_dialogs
            .last()
            .map(|dialog| dialog.focus_handle.clone())
    }

    fn pop_dialog(&mut self) -> Option<FocusHandle> {
        self.active_dialogs
            .pop()
            .and_then(|dialog| dialog.previous_focused_handle)
            .and_then(|handle| handle.upgrade())
    }

    pub fn push_notification(
        &mut self,
        notification: Notification,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.notification
            .update(cx, |list, cx| list.push(notification, window, cx));
        cx.notify();
    }

    pub fn dismiss_notification(
        &mut self,
        key: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let dismissed = self
            .notification
            .update(cx, |list, cx| list.dismiss_key(key, window, cx));
        if dismissed {
            cx.notify();
        }
        dismissed
    }

    pub fn clear_notifications(&mut self, cx: &mut Context<Self>) {
        self.notification.update(cx, NotificationList::clear);
        cx.notify();
    }

    #[must_use]
    pub fn notifications(&self, cx: &App) -> Vec<Entity<Notification>> {
        self.notification.read(cx).notifications()
    }

    fn on_action_tab(&mut self, _: &Tab, window: &mut Window, cx: &mut Context<Self>) {
        self.cycle_focus(true, window, cx);
    }

    fn on_action_tab_prev(&mut self, _: &TabPrev, window: &mut Window, cx: &mut Context<Self>) {
        self.cycle_focus(false, window, cx);
    }

    fn cycle_focus(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(container) = self.top_dialog_focus() else {
            Self::step_focus(forward, window, cx);
            return;
        };

        let before = window.focused(cx);
        Self::step_focus(forward, window, cx);

        let mut steps = 0;
        while !container.contains_focused(window, cx) && steps < MAX_FOCUS_STEPS {
            Self::step_focus(forward, window, cx);
            steps += 1;

            if window.focused(cx) == before {
                break;
            }
        }
    }

    fn step_focus(forward: bool, window: &mut Window, cx: &mut App) {
        if forward {
            window.focus_next(cx);
        } else {
            window.focus_prev(cx);
        }
    }
}

impl Root {
    pub(crate) fn text_selection_scope(&self) -> SelectionScope {
        match self.active_dialogs.len() {
            0 => SelectionScope::Base,
            n => SelectionScope::Dialog(n - 1),
        }
    }

    #[must_use]
    pub fn window_selected_text(&self, cx: &App) -> String {
        self.text_selection
            .selected_text(self.text_selection_scope(), cx)
    }

    #[must_use]
    pub fn has_text_selection(&self, cx: &App) -> bool {
        self.text_selection.has_selection(cx)
    }

    pub fn clear_text_selection(&mut self, cx: &mut App) {
        self.text_selection.clear(cx);
    }

    pub fn end_text_selection(&mut self, cx: &mut App) {
        self.text_selection.end(cx);
    }

    fn on_action_copy(&mut self, _: &text::Copy, _: &mut Window, cx: &mut Context<Self>) {
        let selected_text = self.window_selected_text(cx);
        let text = text::clipboard_selection_text(&selected_text);
        if text.is_empty() {
            cx.propagate();
            return;
        }
        cx.write_to_clipboard(ClipboardItem::new_string(text.to_string()));
    }
}

impl Styled for Root {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl Render for Root {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        window.set_rem_size(cx.theme().font_size);

        let inner = div()
            .id("root")
            .key_context(CONTEXT)
            .on_action(cx.listener(Self::on_action_tab))
            .on_action(cx.listener(Self::on_action_tab_prev))
            .on_action(cx.listener(Self::on_action_copy))
            .relative()
            .size_full()
            .font_family(cx.theme().font_family.clone())
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .refine_style(&self.style)
            .child(TextSelectionController)
            .child(self.view.clone())
            .when(self.own_layers, |this| {
                this.child(Layers {
                    inset: !self.bordered,
                })
            });

        if self.bordered {
            window_border().child(inner).into_any_element()
        } else {
            inner.into_any_element()
        }
    }
}

#[derive(IntoElement)]
struct Layers {
    inset: bool,
}

impl RenderOnce for Layers {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let inset = if self.inset {
            window_paddings(window)
        } else {
            Edges::all(px(0.0))
        };
        div()
            .absolute()
            .top(inset.top)
            .right(inset.right)
            .bottom(inset.bottom)
            .left(inset.left)
            .children(Root::render_dialog_layer(window, cx))
            .children(Root::render_notification_layer(window, cx))
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::Cell, rc::Rc};

    use super::{Dialog, Root};
    use crate::{
        ADAPTIVE_CORNER_FRACTION, CORNER_SMOOTHING, WindowExt as _, notification::Notification,
    };
    use ala::{
        App, AppContext as _, Context, IntoElement, ParentElement as _, Render, TestAppContext,
        VisualTestContext, Window, div,
    };

    struct TestView;

    impl Render for TestView {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
        }
    }

    struct LayeredView;

    impl Render for LayeredView {
        fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            div().children(Root::render_dialog_layer(window, cx))
        }
    }

    fn noop_dialog(dialog: Dialog, _: &mut Window, _: &mut App) -> Dialog {
        dialog
    }

    fn root_window(
        cx: &mut TestAppContext,
        own_layers: bool,
        view: impl Fn(&mut App) -> ala::AnyView + 'static,
    ) -> &mut VisualTestContext {
        cx.update(|cx| {
            crate::init(cx);
            cx.set_reduce_motion(true);
        });
        let (_, cx) = cx.add_window_view(move |window, cx| {
            Root::new(view(cx), window, cx).own_layers(own_layers)
        });
        cx
    }

    fn draw(cx: &mut VisualTestContext) {
        cx.run_until_parked();
        cx.update(|window, cx| _ = window.draw(cx));
        cx.update(|window, cx| _ = window.draw(cx));
    }

    fn counted_dialog(
        count: &Rc<Cell<usize>>,
    ) -> impl Fn(Dialog, &mut Window, &mut App) -> Dialog + 'static {
        let count = count.clone();
        move |dialog, _, _| {
            count.set(count.get() + 1);
            dialog.title("Counted")
        }
    }

    #[ala::test]
    fn root_draws_dialogs_and_toasts_without_app_code(cx: &mut TestAppContext) {
        let cx = root_window(cx, true, |cx| cx.new(|_| TestView).into());
        let builds = Rc::new(Cell::new(0));
        let build = counted_dialog(&builds);
        cx.update(|window, cx| {
            window.open_dialog(cx, build);
            window.push_notification(Notification::info("Saved"), cx);
        });
        draw(cx);
        builds.set(0);
        cx.update(|window, cx| _ = window.draw(cx));
        assert_eq!(builds.get(), 1, "the dialog layer is drawn once a frame");
        assert!(cx.debug_bounds("dialog-0").is_some());
        assert!(cx.debug_bounds("notification-message").is_some());
    }

    #[ala::test]
    fn a_root_without_its_layers_leaves_them_to_the_app(cx: &mut TestAppContext) {
        let cx = root_window(cx, false, |cx| cx.new(|_| TestView).into());
        let builds = Rc::new(Cell::new(0));
        let build = counted_dialog(&builds);
        cx.update(|window, cx| window.open_dialog(cx, build));
        draw(cx);
        assert_eq!(builds.get(), 0);
        assert!(cx.debug_bounds("dialog-0").is_none());
    }

    #[ala::test]
    fn an_app_that_draws_the_layers_itself_still_draws_one(cx: &mut TestAppContext) {
        let cx = root_window(cx, false, |cx| cx.new(|_| LayeredView).into());
        let builds = Rc::new(Cell::new(0));
        let build = counted_dialog(&builds);
        cx.update(|window, cx| window.open_dialog(cx, build));
        draw(cx);
        builds.set(0);
        cx.update(|window, cx| _ = window.draw(cx));
        assert_eq!(builds.get(), 1);
        assert!(cx.debug_bounds("dialog-0").is_some());
    }

    #[ala::test]
    fn root_sets_the_window_corners(cx: &mut TestAppContext) {
        let cx = root_window(cx, true, |cx| cx.new(|_| TestView).into());
        cx.update(|window, _| {
            assert_eq!(window.default_corner_smoothing(), CORNER_SMOOTHING);
            assert_eq!(
                window.adaptive_corner_fraction(),
                Some(ADAPTIVE_CORNER_FRACTION)
            );
        });
    }

    #[ala::test]
    fn overlay_calls_on_a_window_without_a_root_do_nothing(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let (_, cx) = cx.add_window_view(|_, _| TestView);
        cx.update(|window, cx| {
            window.push_notification(Notification::info("Lost"), cx);
            window.open_dialog(cx, noop_dialog);
            window.close_dialog(cx);
            assert!(!window.has_active_dialog(cx));
            assert!(window.notifications(cx).is_empty());
            assert!(!window.dismiss_notification("missing", cx));
            assert!(window.selected_text(cx).is_empty());
            assert!(Root::read(window, cx).is_none());
            assert!(Root::update(window, cx, |_, _, _| ()).is_none());
        });
    }

    #[ala::test]
    fn dialogs_stack_and_unwind(cx: &mut TestAppContext) {
        cx.update(crate::init);

        let (root, cx) = cx.add_window_view(|window, cx| {
            let view = cx.new(|_| TestView);
            Root::new(view, window, cx)
        });

        root.update_in(cx, |root, window, cx| {
            root.open_dialog(noop_dialog, window, cx);
            root.open_dialog(noop_dialog, window, cx);
        });
        assert_eq!(root.read_with(cx, |root, _| root.active_dialogs.len()), 2);

        root.update_in(cx, |root, window, cx| root.close_dialog(window, cx));
        assert_eq!(root.read_with(cx, |root, _| root.active_dialogs.len()), 1);

        root.update_in(cx, |root, window, cx| root.close_all_dialogs(window, cx));
        assert!(root.read_with(cx, |root, _| !root.has_active_dialog()));
    }
}
