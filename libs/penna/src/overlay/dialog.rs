use std::{rc::Rc, time::Duration};

use crate::foundation::{SURFACE_ENTER_DURATION, surface_enter};
use crate::window_paddings;
use ala::{
    Animation, AnimationExt as _, AnyElement, App, ClickEvent, Div, FocusHandle,
    InteractiveElement as _, IntoElement, MouseButton, ParentElement, Pixels, RenderOnce, Role,
    SharedString, StatefulInteractiveElement as _, StyleRefinement, Styled, Window,
    WindowControlArea, anchored, div, ease_out_quint, point, prelude::FluentBuilder as _, px,
    relative, size,
};

use crate::{
    ActiveTheme as _, Emphasis, IconName, StyledExt as _, Surface, SurfaceRole, Tone,
    button::{Button, ButtonVariant, ButtonVariants as _},
    h_flex,
    scroll::ScrollableElement as _,
    v_flex,
};

use super::{
    Root,
    actions::{CancelDialog, ConfirmDialog},
    window_ext::WindowExt as _,
};
use crate::Colorize as _;

pub(super) const CONTEXT: &str = "PennaDialog";

pub(super) const ANIMATION_DURATION: Duration = SURFACE_ENTER_DURATION;

pub(super) const CONTENT_PADDING: Pixels = px(12.);
const MIN_HEIGHT: Pixels = px(80.);
const LAYER_OFFSET: f32 = 16.;
const EDGE_MARGIN: Pixels = px(16.);
pub(super) const DEFAULT_WIDTH: Pixels = px(400.);
pub(super) const TITLE_TEXT_SIZE: Pixels = px(13.);
pub(super) const DESCRIPTION_TEXT_SIZE: Pixels = px(12.);

const OK_LABEL: &str = "OK";
const CANCEL_LABEL: &str = "Cancel";

type ConfirmHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App) -> bool>;
type CloseHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

#[derive(Clone)]
pub struct DialogButtonProps {
    ok_text: Option<SharedString>,
    ok_variant: ButtonVariant,
    cancel_text: Option<SharedString>,
    show_cancel: bool,
    on_ok: ConfirmHandler,
    on_cancel: ConfirmHandler,
    on_close: CloseHandler,
}

impl Default for DialogButtonProps {
    fn default() -> Self {
        Self {
            ok_text: None,
            ok_variant: ButtonVariant::Paint(Tone::Primary, Emphasis::Solid),
            cancel_text: None,
            show_cancel: false,
            on_ok: Rc::new(|_, _, _| true),
            on_cancel: Rc::new(|_, _, _| true),
            on_close: Rc::new(|_, _, _| {}),
        }
    }
}

impl DialogButtonProps {
    #[must_use]
    pub fn ok_text(mut self, ok_text: impl Into<SharedString>) -> Self {
        self.ok_text = Some(ok_text.into());
        self
    }

    #[must_use]
    pub fn ok_variant(mut self, ok_variant: ButtonVariant) -> Self {
        self.ok_variant = ok_variant;
        self
    }

    #[must_use]
    pub fn cancel_text(mut self, cancel_text: impl Into<SharedString>) -> Self {
        self.cancel_text = Some(cancel_text.into());
        self
    }

    #[must_use]
    pub fn show_cancel(mut self, show_cancel: bool) -> Self {
        self.show_cancel = show_cancel;
        self
    }

    #[must_use]
    pub fn on_ok(
        mut self,
        on_ok: impl Fn(&ClickEvent, &mut Window, &mut App) -> bool + 'static,
    ) -> Self {
        self.on_ok = Rc::new(on_ok);
        self
    }

    #[must_use]
    pub fn on_cancel(
        mut self,
        on_cancel: impl Fn(&ClickEvent, &mut Window, &mut App) -> bool + 'static,
    ) -> Self {
        self.on_cancel = Rc::new(on_cancel);
        self
    }

    fn render_ok(&self) -> AnyElement {
        let on_ok = self.on_ok.clone();
        let on_close = self.on_close.clone();
        let label = self.ok_text.clone().unwrap_or_else(|| OK_LABEL.into());

        Button::new("ok")
            .label(label)
            .with_variant(self.ok_variant)
            .on_click(move |_, window, cx| {
                if on_ok(&ClickEvent::default(), window, cx) {
                    window.close_dialog(cx);
                    on_close(&ClickEvent::default(), window, cx);
                }
            })
            .into_any_element()
    }

    fn render_cancel(&self) -> AnyElement {
        let on_cancel = self.on_cancel.clone();
        let on_close = self.on_close.clone();
        let label = self
            .cancel_text
            .clone()
            .unwrap_or_else(|| CANCEL_LABEL.into());

        Button::new("cancel")
            .label(label)
            .on_click(move |_, window, cx| {
                if on_cancel(&ClickEvent::default(), window, cx) {
                    window.close_dialog(cx);
                    on_close(&ClickEvent::default(), window, cx);
                }
            })
            .into_any_element()
    }
}

#[derive(Clone)]
pub(crate) struct DialogProps {
    width: Pixels,
    close_button: bool,
    overlay_closable: bool,
    pub(crate) overlay_visible: bool,
    keyboard: bool,
}

impl Default for DialogProps {
    fn default() -> Self {
        Self {
            width: DEFAULT_WIDTH,
            close_button: true,
            overlay_closable: true,
            overlay_visible: false,
            keyboard: true,
        }
    }
}

#[derive(IntoElement)]
pub struct Dialog {
    style: StyleRefinement,
    children: Vec<AnyElement>,
    title: Option<AnyElement>,
    header: Option<AnyElement>,
    footer: Option<AnyElement>,
    button_props: DialogButtonProps,
    show_button_row: bool,
    a11y_role: Role,
    pub(crate) props: DialogProps,
    pub(crate) focus_handle: FocusHandle,
    pub(crate) layer_ix: usize,
    pub(crate) instance_id: u64,
}

impl Dialog {
    #[must_use]
    pub fn new(cx: &mut App) -> Self {
        Self {
            style: StyleRefinement::default(),
            children: Vec::new(),
            title: None,
            header: None,
            footer: None,
            button_props: DialogButtonProps::default(),
            show_button_row: false,
            a11y_role: Role::Dialog,
            props: DialogProps::default(),
            focus_handle: cx.focus_handle(),
            layer_ix: 0,
            instance_id: 0,
        }
    }

    #[must_use]
    pub fn title(mut self, title: impl IntoElement) -> Self {
        self.title = Some(title.into_any_element());
        self
    }

    #[must_use]
    pub fn footer(mut self, footer: impl IntoElement) -> Self {
        self.footer = Some(footer.into_any_element());
        self
    }

    #[must_use]
    pub fn button_props(mut self, button_props: DialogButtonProps) -> Self {
        self.button_props = button_props;
        self.show_button_row = true;
        self
    }

    #[must_use]
    pub fn on_close(
        mut self,
        on_close: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.button_props.on_close = Rc::new(on_close);
        self
    }

    #[must_use]
    pub fn on_ok(
        mut self,
        on_ok: impl Fn(&ClickEvent, &mut Window, &mut App) -> bool + 'static,
    ) -> Self {
        self.button_props = self.button_props.on_ok(on_ok);
        self.show_button_row = true;
        self
    }

    #[must_use]
    pub fn on_cancel(
        mut self,
        on_cancel: impl Fn(&ClickEvent, &mut Window, &mut App) -> bool + 'static,
    ) -> Self {
        self.button_props = self.button_props.on_cancel(on_cancel);
        self.show_button_row = true;
        self
    }

    #[must_use]
    pub fn close_button(mut self, close_button: bool) -> Self {
        self.props.close_button = close_button;
        self
    }

    #[must_use]
    pub fn overlay_closable(mut self, overlay_closable: bool) -> Self {
        self.props.overlay_closable = overlay_closable;
        self
    }

    #[must_use]
    pub fn keyboard(mut self, keyboard: bool) -> Self {
        self.props.keyboard = keyboard;
        self
    }

    #[must_use]
    pub fn width(mut self, width: impl Into<Pixels>) -> Self {
        self.props.width = width.into();
        self
    }

    #[must_use]
    fn header(mut self, header: impl IntoElement) -> Self {
        self.header = Some(header.into_any_element());
        self
    }

    #[must_use]
    fn alert_role(mut self) -> Self {
        self.a11y_role = Role::AlertDialog;
        self
    }
}

impl ParentElement for Dialog {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl Styled for Dialog {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Dialog {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let layer_ix = self.layer_ix;
        let on_ok = self.button_props.on_ok.clone();
        let on_cancel = self.button_props.on_cancel.clone();
        let on_close = self.button_props.on_close.clone();
        let button_props = self.button_props.clone();
        let show_default_footer = self.show_button_row && self.footer.is_none();
        let overlay_closable = self.props.overlay_closable;

        let is_topmost = self.props.overlay_visible;

        let paddings = window_paddings(window);
        let view_size = window.viewport_size()
            - size(
                paddings.left + paddings.right,
                paddings.top + paddings.bottom,
            );
        let width = self
            .props
            .width
            .min((view_size.width - EDGE_MARGIN * 2.).max(px(0.)));
        let x = (view_size.width - width) / 2.;
        #[allow(clippy::cast_precision_loss)]
        let layer_offset = px(layer_ix as f32 * LAYER_OFFSET);
        let top_room = (view_size.height / 10. - EDGE_MARGIN).max(px(0.));
        let max_height = (view_size.height - EDGE_MARGIN * 2. - layer_offset).max(px(0.));

        let animation = Animation::new(ANIMATION_DURATION).with_easing(ease_out_quint());
        let scrim = cx.theme().scrim;

        let body = dialog_frame(SurfaceRole::DIALOG, cx)
            .id(layer_ix)
            .debug_selector(move || format!("dialog-{layer_ix}"))
            .role(self.a11y_role)
            .track_focus(&self.focus_handle)
            .min_h(MIN_HEIGHT)
            .refine_style(&self.style)
            .px_0()
            .key_context(CONTEXT)
            .when(self.props.keyboard, |this| {
                this.on_action({
                    let on_cancel = on_cancel.clone();
                    let on_close = on_close.clone();
                    move |_: &CancelDialog, window, cx| {
                        if on_cancel(&ClickEvent::default(), window, cx) {
                            window.close_dialog(cx);
                            on_close(&ClickEvent::default(), window, cx);
                        }
                    }
                })
                .on_action({
                    let on_ok = on_ok.clone();
                    let on_close = on_close.clone();
                    move |_: &ConfirmDialog, window, cx| {
                        if on_ok(&ClickEvent::default(), window, cx) {
                            Root::update(window, cx, |root, window, cx| {
                                root.defer_close_dialog(window, cx);
                            });
                            on_close(&ClickEvent::default(), window, cx);
                        }
                    }
                })
            })
            .absolute()
            .occlude()
            .relative()
            .flex_shrink_0()
            .left(x)
            .w(width)
            .max_h(max_height)
            .child(
                v_flex()
                    .flex_1()
                    .overflow_hidden()
                    .gap_y_2()
                    .when_some(self.header, |this, header| {
                        this.child(dialog_gutter().child(header))
                    })
                    .when_some(self.title, |this, title| {
                        this.child(dialog_gutter().child(dialog_title().child(title)))
                    })
                    .when(!self.children.is_empty(), |this| {
                        this.child(
                            div().flex_1().overflow_hidden().child(
                                v_flex()
                                    .size_full()
                                    .overflow_y_scrollbar()
                                    .pl(CONTENT_PADDING)
                                    .pr(CONTENT_PADDING)
                                    .children(self.children),
                            ),
                        )
                    }),
            )
            .when_some(self.footer, |this, footer| {
                this.child(dialog_gutter().child(footer))
            })
            .when(show_default_footer, |this| {
                this.child(
                    dialog_gutter().child(
                        dialog_footer(cx)
                            .when(button_props.show_cancel, |this| {
                                this.child(button_props.render_cancel())
                            })
                            .child(button_props.render_ok()),
                    ),
                )
            })
            .children(self.props.close_button.then(|| {
                let on_cancel = on_cancel.clone();
                let on_close = on_close.clone();
                Button::new("close")
                    .absolute()
                    .top(px(8.))
                    .right(px(8.))
                    .ghost()
                    .icon(IconName::Xmark)
                    .on_click(move |_, window, cx| {
                        window.close_dialog(cx);
                        on_cancel(&ClickEvent::default(), window, cx);
                        on_close(&ClickEvent::default(), window, cx);
                    })
            }));
        let body = surface_enter(body, "dialog-open", px(0.));

        anchored()
            .position(point(paddings.left, paddings.top))
            .snap_to_window()
            .child(
                div()
                    .id(("dialog", self.instance_id))
                    .occlude()
                    .w(view_size.width)
                    .h(view_size.height)
                    .flex()
                    .flex_col()
                    .pt(EDGE_MARGIN + layer_offset)
                    .pb(EDGE_MARGIN)
                    .when(is_topmost, |this| {
                        this.window_control_area(WindowControlArea::Drag)
                            .on_any_mouse_down(move |event, window, cx| {
                                if event.position.y < cx.theme().title_bar_height() {
                                    return;
                                }

                                cx.stop_propagation();
                                if overlay_closable
                                    && event.button == MouseButton::Left
                                    && on_cancel(&ClickEvent::default(), window, cx)
                                {
                                    on_close(&ClickEvent::default(), window, cx);
                                    window.close_dialog(cx);
                                }
                            })
                    })
                    .child(div().flex_shrink_1().flex_basis(top_room))
                    .child(body)
                    .with_animation(
                        ("dialog-backdrop", self.instance_id),
                        animation,
                        move |this, delta| {
                            this.when(is_topmost, |this| this.bg(scrim.opacity(delta)))
                        },
                    ),
            )
    }
}

pub fn dialog_surface(cx: &App) -> Div {
    dialog_frame(SurfaceRole::DIALOG, cx)
}

fn dialog_frame(role: SurfaceRole, cx: &App) -> Div {
    v_flex()
        .surface(Surface::new(role), cx)
        .pt(CONTENT_PADDING)
        .pb(CONTENT_PADDING)
        .gap(CONTENT_PADDING)
}

pub fn dialog_gutter() -> Div {
    div().pl(CONTENT_PADDING).pr(CONTENT_PADDING)
}

pub fn dialog_title() -> Div {
    div()
        .text_size(TITLE_TEXT_SIZE)
        .font_semibold()
        .line_height(relative(1.))
}

pub fn dialog_description(cx: &App) -> Div {
    div()
        .text_size(DESCRIPTION_TEXT_SIZE)
        .text_color(cx.theme().foreground.muted())
}

pub fn dialog_footer(cx: &App) -> Div {
    h_flex()
        .gap_2()
        .justify_end()
        .line_height(relative(1.))
        .rounded_b(cx.theme().radius)
}

pub struct AlertDialog {
    base: Dialog,
    icon: Option<AnyElement>,
    title: Option<AnyElement>,
    description: Option<AnyElement>,
    button_props: DialogButtonProps,
}

impl AlertDialog {
    #[must_use]
    pub fn new(cx: &mut App) -> Self {
        Self {
            base: Dialog::new(cx).overlay_closable(false).close_button(false),
            icon: None,
            title: None,
            description: None,
            button_props: DialogButtonProps::default(),
        }
    }

    #[must_use]
    pub fn icon(mut self, icon: impl IntoElement) -> Self {
        self.icon = Some(icon.into_any_element());
        self
    }

    #[must_use]
    pub fn title(mut self, title: impl IntoElement) -> Self {
        self.title = Some(title.into_any_element());
        self
    }

    #[must_use]
    pub fn description(mut self, description: impl IntoElement) -> Self {
        self.description = Some(description.into_any_element());
        self
    }

    #[must_use]
    pub fn button_props(mut self, button_props: DialogButtonProps) -> Self {
        self.button_props = button_props;
        self
    }

    #[must_use]
    pub fn on_ok(
        mut self,
        on_ok: impl Fn(&ClickEvent, &mut Window, &mut App) -> bool + 'static,
    ) -> Self {
        self.button_props = self.button_props.on_ok(on_ok);
        self
    }

    #[must_use]
    pub fn on_cancel(
        mut self,
        on_cancel: impl Fn(&ClickEvent, &mut Window, &mut App) -> bool + 'static,
    ) -> Self {
        self.button_props = self.button_props.on_cancel(on_cancel);
        self
    }

    #[must_use]
    pub fn on_close(
        mut self,
        on_close: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.button_props.on_close = Rc::new(on_close);
        self
    }

    pub(super) fn into_dialog(self, cx: &App) -> Dialog {
        let props = self.button_props;
        let has_header = self.icon.is_some() || self.title.is_some() || self.description.is_some();

        self.base
            .button_props(props.clone())
            .alert_role()
            .when(has_header, |this| {
                this.header(
                    h_flex()
                        .gap_2()
                        .items_start()
                        .when_some(self.icon, |row, icon| row.child(icon))
                        .child(
                            v_flex()
                                .flex_1()
                                .min_w_0()
                                .gap_1()
                                .when_some(self.title, |this, title| {
                                    this.child(dialog_title().child(title))
                                })
                                .when_some(self.description, |this, description| {
                                    this.child(dialog_description(cx).child(description))
                                }),
                        ),
                )
            })
            .footer(
                dialog_footer(cx)
                    .when(props.show_cancel, |this| this.child(props.render_cancel()))
                    .child(props.render_ok()),
            )
    }
}

#[cfg(test)]
mod tests {
    use ala::{
        App, AppContext as _, Bounds, Context, InteractiveElement as _, IntoElement,
        ParentElement as _, Pixels, Render, Styled as _, TestAppContext, VisualTestContext, Window,
        div, px, size,
    };

    use super::{Dialog, DialogButtonProps, EDGE_MARGIN, Root};
    use crate::WindowExt as _;

    struct DialogHost;

    impl Render for DialogHost {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().size_full()
        }
    }

    fn window(cx: &mut TestAppContext, viewport: ala::Size<Pixels>) -> &mut VisualTestContext {
        cx.update(|cx| {
            crate::init(cx);
            cx.set_reduce_motion(true);
        });
        let (_, cx) = cx.add_window_view(|window, cx| {
            let view = cx.new(|_| DialogHost);
            Root::new(view, window, cx)
        });
        cx.simulate_resize(viewport);
        cx.update(|window, cx| _ = window.draw(cx));
        cx
    }

    fn open(
        cx: &mut VisualTestContext,
        build: impl Fn(Dialog, &mut Window, &mut App) -> Dialog + 'static,
    ) {
        cx.update(|window, cx| window.open_dialog(cx, build));
        cx.run_until_parked();
        cx.update(|window, cx| _ = window.draw(cx));
        cx.update(|window, cx| _ = window.draw(cx));
    }

    fn surface(cx: &mut VisualTestContext, layer_ix: usize) -> Bounds<Pixels> {
        cx.debug_bounds(["dialog-0", "dialog-1"][layer_ix])
            .unwrap_or_else(|| panic!("dialog layer {layer_ix} was not painted"))
    }

    #[ala::test]
    fn a_dialog_that_fits_keeps_its_width_and_top_offset(cx: &mut TestAppContext) {
        let cx = window(cx, size(px(1000.), px(800.)));
        open(cx, |dialog, _, _| dialog.title("Fits").child("body"));

        let bounds = surface(cx, 0);
        assert_eq!(bounds.size.width, px(400.));
        assert_eq!(bounds.origin.x, px(300.));
        assert_eq!(bounds.origin.y, px(80.));
    }

    #[ala::test]
    fn a_dialog_larger_than_the_window_stays_inside_it(cx: &mut TestAppContext) {
        let viewport = size(px(360.), px(300.));
        let cx = window(cx, viewport);
        open(cx, |dialog, _, _| {
            dialog
                .width(px(800.))
                .title("Too big")
                .child(div().h(px(1000.)).child("tall body"))
                .footer(div().h(px(32.)).debug_selector(|| "footer-probe".into()))
        });

        let bounds = surface(cx, 0);
        let footer = cx.debug_bounds("footer-probe").expect("footer");
        assert_eq!(bounds.origin.x, EDGE_MARGIN);
        assert_eq!(bounds.right(), viewport.width - EDGE_MARGIN);
        assert!(
            bounds.bottom() <= viewport.height - EDGE_MARGIN,
            "{bounds:?}"
        );
        assert!(footer.bottom() <= bounds.bottom(), "{footer:?} {bounds:?}");
    }

    #[ala::test]
    fn an_oversized_dialog_moves_up_to_the_edge_margin(cx: &mut TestAppContext) {
        let viewport = size(px(360.), px(600.));
        let cx = window(cx, viewport);
        open(cx, |dialog, _, _| {
            dialog.title("Tall").child(div().h(px(1000.)))
        });

        let bounds = surface(cx, 0);
        assert_eq!(bounds.origin.y, EDGE_MARGIN);
        assert_eq!(bounds.bottom(), viewport.height - EDGE_MARGIN);
    }

    #[ala::test]
    fn a_dialog_that_almost_fits_rises_only_as_far_as_it_must(cx: &mut TestAppContext) {
        let viewport = size(px(1000.), px(800.));
        let cx = window(cx, viewport);
        open(cx, |dialog, _, _| {
            dialog
                .title("Nearly")
                .child(div().h(px(650.)))
                .footer(div().h(px(32.)))
        });

        let bounds = surface(cx, 0);
        assert!(bounds.origin.y > EDGE_MARGIN, "{bounds:?}");
        assert!(bounds.origin.y < px(80.), "{bounds:?}");
        assert_eq!(bounds.bottom(), viewport.height - EDGE_MARGIN);
    }

    #[ala::test]
    fn stacked_dialogs_each_fit_the_window(cx: &mut TestAppContext) {
        let viewport = size(px(360.), px(300.));
        let cx = window(cx, viewport);
        open(cx, |dialog, _, _| {
            dialog.title("First").child(div().h(px(1000.)))
        });
        open(cx, |dialog, _, _| {
            dialog.title("Second").child(div().h(px(1000.)))
        });

        let first = surface(cx, 0);
        let second = surface(cx, 1);
        assert_eq!(second.origin.y, first.origin.y + px(16.));
        assert!(first.bottom() <= viewport.height - EDGE_MARGIN, "{first:?}");
        assert!(
            second.bottom() <= viewport.height - EDGE_MARGIN,
            "{second:?}"
        );
    }

    #[ala::test]
    fn escape_cancels_and_enter_confirms_the_top_dialog(cx: &mut TestAppContext) {
        use std::{cell::RefCell, rc::Rc};

        let cx = window(cx, size(px(1000.), px(800.)));
        let calls = Rc::new(RefCell::new(Vec::new()));
        let dialog = |calls: &Rc<RefCell<Vec<&'static str>>>| {
            let (ok, cancel) = (calls.clone(), calls.clone());
            move |dialog: Dialog, _: &mut Window, _: &mut App| {
                let (ok, cancel) = (ok.clone(), cancel.clone());
                dialog
                    .title("Keys")
                    .on_ok(move |_, _, _| {
                        ok.borrow_mut().push("ok");
                        true
                    })
                    .on_cancel(move |_, _, _| {
                        cancel.borrow_mut().push("cancel");
                        true
                    })
            }
        };

        open(cx, dialog(&calls));
        cx.simulate_keystrokes("escape");
        cx.run_until_parked();
        assert_eq!(*calls.borrow(), ["cancel"]);
        assert!(!cx.update(|window, cx| window.has_active_dialog(cx)));

        open(cx, dialog(&calls));
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        assert_eq!(*calls.borrow(), ["cancel", "ok"]);
        assert!(!cx.update(|window, cx| window.has_active_dialog(cx)));
    }

    #[ala::test]
    fn custom_dialog_actions_enable_the_visible_default_footer(cx: &mut TestAppContext) {
        cx.update(|cx| {
            assert!(!Dialog::new(cx).show_button_row);
            assert!(
                Dialog::new(cx)
                    .button_props(DialogButtonProps::default().show_cancel(true))
                    .show_button_row
            );
            assert!(Dialog::new(cx).on_ok(|_, _, _| false).show_button_row);
        });
    }
}
