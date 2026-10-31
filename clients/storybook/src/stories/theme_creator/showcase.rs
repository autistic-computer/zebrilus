use ala::{
    AnyElement, App, AppContext as _, Context, Entity, IntoElement, Keystroke, ParentElement as _,
    Render, SharedString, Styled as _, Window, div, px,
};
use penna::{
    Emphasis, IconName, Tone,
    badge::Badge,
    button::{Button, ButtonVariants as _},
    h_flex,
    kbd::Kbd,
    notification::Notification,
    overlay::{dialog_description, dialog_footer, dialog_gutter, dialog_surface, dialog_title},
    slider::DiscreteSlider,
    tooltip::Tooltip,
    v_flex,
};
use penna_storybook::creator::{card, heading, toggle};
use penna_storybook::kit::menus::SplitRight;

const SCROLLBACK: [&str; 5] = ["1k", "10k", "50k", "100k", "∞"];

pub(crate) struct Preferences {
    restore: bool,
    animations: bool,
    sound: bool,
    scrollback: usize,
}

impl Preferences {
    pub(crate) fn view(cx: &mut App) -> Entity<Self> {
        cx.new(|_| Self {
            restore: true,
            animations: true,
            sound: false,
            scrollback: 2,
        })
    }
}

impl Render for Preferences {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let this = cx.entity();
        card("showcase-preferences", cx)
            .child(heading("Behavior", "How sessions and panes behave.", cx))
            .child(toggle(
                "showcase-restore",
                "Restore sessions",
                "Bring windows and panes back after a restart",
                self.restore,
                cx.listener(|this, checked: &bool, _, cx| {
                    this.restore = *checked;
                    cx.notify();
                }),
                cx,
            ))
            .child(toggle(
                "showcase-animations",
                "Animations",
                "Menus and dialogs ease in",
                self.animations,
                cx.listener(|this, checked: &bool, _, cx| {
                    this.animations = *checked;
                    cx.notify();
                }),
                cx,
            ))
            .child(toggle(
                "showcase-sound",
                "Bell sound",
                "Play a sound when a pane rings",
                self.sound,
                cx.listener(|this, checked: &bool, _, cx| {
                    this.sound = *checked;
                    cx.notify();
                }),
                cx,
            ))
            .child(DiscreteSlider::new(
                "showcase-scrollback",
                "Scrollback",
                SCROLLBACK
                    .iter()
                    .map(|label| SharedString::from(*label))
                    .collect(),
                self.scrollback,
                move |index, _, cx| {
                    this.update(cx, |this, cx| {
                        this.scrollback = index;
                        cx.notify();
                    });
                },
            ))
    }
}

pub(crate) struct Toasts {
    toasts: [Entity<Notification>; 2],
    tooltip: Entity<Tooltip>,
}

impl Toasts {
    pub(crate) fn new(cx: &mut App) -> Self {
        let toast = |notification: Notification, cx: &mut App| {
            cx.new(|_| notification.autohide(false).width(px(320.0)))
        };
        let toasts = [
            toast(
                Notification::success("3 windows and 7 panes came back.").title("Session restored"),
                cx,
            ),
            toast(
                Notification::error("ssh devbox exited with status 255.").title("Connection lost"),
                cx,
            ),
        ];
        let tooltip = cx.new(|_| Tooltip::new("Split right").action(&SplitRight, None));
        Self { toasts, tooltip }
    }

    pub(crate) fn stack(&self) -> ala::Div {
        v_flex()
            .gap_3()
            .items_center()
            .children(self.toasts.iter().cloned())
            .child(self.tooltip.clone())
    }
}

pub(crate) fn buttons(cx: &App) -> AnyElement {
    let keys = ["cmd-k", "cmd-shift-d", "ctrl-a"];
    card("showcase-buttons", cx)
        .child(heading("Actions", "Buttons, tags and key hints.", cx))
        .child(
            h_flex()
                .flex_wrap()
                .gap_2()
                .child(
                    Button::new("showcase-primary")
                        .primary()
                        .label("New session"),
                )
                .child(Button::new("showcase-default").label("Detach"))
                .child(Button::new("showcase-ghost").ghost().label("Rename"))
                .child(Button::new("showcase-danger").danger().label("Kill")),
        )
        .child(
            h_flex()
                .gap_2()
                .child(Button::new("showcase-split-right").icon(IconName::PanelRight))
                .child(Button::new("showcase-split-down").icon(IconName::PanelBottom))
                .child(
                    Button::new("showcase-search")
                        .ghost()
                        .icon(IconName::Search),
                )
                .child(
                    Button::new("showcase-settings")
                        .ghost()
                        .icon(IconName::Settings),
                )
                .child(div().flex_1())
                .child(Button::new("showcase-small").label("Small")),
        )
        .child(
            h_flex()
                .flex_wrap()
                .items_center()
                .gap_2()
                .child(Badge::new("connected").tone(Tone::Success))
                .child(
                    Badge::new("main")
                        .tone(Tone::Primary)
                        .emphasis(Emphasis::Solid),
                )
                .child(Badge::new("3 panes"))
                .children(
                    keys.into_iter()
                        .map(|key| Kbd::new(Keystroke::parse(key).unwrap_or_default())),
                ),
        )
        .into_any_element()
}

pub(crate) fn dialog(cx: &App) -> AnyElement {
    dialog_surface(cx)
        .w_full()
        .max_w(px(360.0))
        .child(dialog_gutter().child(dialog_title().child("Kill session \"work\"?")))
        .child(
            dialog_gutter().child(
                dialog_description(cx).child(
                    "Its 3 windows and 7 panes stop, and their scrollback is gone for good.",
                ),
            ),
        )
        .child(
            dialog_gutter().child(
                dialog_footer(cx)
                    .child(Button::new("showcase-dialog-cancel").label("Cancel"))
                    .child(
                        Button::new("showcase-dialog-kill")
                            .danger()
                            .label("Kill session"),
                    ),
            ),
        )
        .into_any_element()
}
