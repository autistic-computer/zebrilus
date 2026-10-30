mod plates;

use ala::{
    AnyElement, App, AppContext as _, Context, ElementId, Entity, IntoElement, Keystroke,
    ParentElement as _, Render, SharedString, Styled as _, Window, div, prelude::*, px,
};
use penna::{
    ActiveTheme as _, Colorize as _, Elevation, Emphasis, IconName, Tone,
    badge::Badge,
    button::{Button, ButtonVariants as _},
    h_flex,
    kbd::Kbd,
    notification::Notification,
    overlay::{dialog_description, dialog_footer, dialog_gutter, dialog_surface, dialog_title},
    slider::DiscreteSlider,
    switch::Switch,
    tooltip::Tooltip,
    v_flex,
};

use crate::kit::menus::SplitRight;
use crate::story::{Framed as _, Section, Story};

pub use plates::{CONTENT, CONTROLS, FLOATING, PHONE, WINDOW};

pub const ID: &str = "theme-creator";

pub const PLATES: &[Section] = &[WINDOW, CONTROLS, FLOATING, CONTENT, PHONE];

pub const WIDE: f32 = 1040.0;
pub const MEDIUM: f32 = 680.0;
pub const GAP: f32 = 16.0;
pub const PLATE_PADDING: f32 = 20.0;
pub const PHONE_WIDTH: f32 = 390.0;
pub const SCREEN_RADIUS: f32 = 55.0;
pub const SAFE_BOTTOM: f32 = 34.0;

pub const READING: &str = "Pinned tabs keep a `28px` floor, and the rest share what is left:\n\n\
| Tabs | Pinned | Unpinned |\n\
| ---: | ---: | ---: |\n\
| 10 | 28px | 96px |\n\
| 30 | 28px | 31px |\n\n\
```rust\n\
let floor = px(28.0) * pinned;\n\
```";

const SCROLLBACK: [&str; 5] = ["1k", "10k", "50k", "100k", "∞"];

pub(crate) const fn story(plates: &'static [Section]) -> Story {
    Story {
        id: ID,
        name: "Theme creator",
        group: "Foundation",
        summary: "Everything a theme touches, region by region. Tune the knobs, then copy the theme as JSON.",
        sections: plates,
    }
}

pub fn tiles(window: &Window, columns: Vec<Vec<AnyElement>>) -> impl IntoElement {
    let width = window.viewport_size().width.as_f32();
    let count = if width >= WIDE {
        3
    } else if width >= MEDIUM {
        2
    } else {
        1
    }
    .min(columns.len());
    let mut stacks: Vec<Vec<AnyElement>> = (0..count).map(|_| Vec::new()).collect();
    let mut next = 0;
    for (group, column) in columns.into_iter().enumerate() {
        for tile in column {
            let index = if count == 3 {
                group % count
            } else {
                next % count
            };
            next += 1;
            stacks[index].push(tile);
        }
    }
    h_flex().w_full().items_start().gap(px(GAP)).children(
        stacks
            .into_iter()
            .map(|stack| v_flex().flex_1().min_w_0().gap(px(GAP)).children(stack)),
    )
}

pub fn card(id: &'static str, cx: &App) -> ala::Stateful<ala::Div> {
    v_flex()
        .id(id)
        .w_full()
        .p(px(16.0))
        .gap_3()
        .rounded(cx.theme().radius)
        .framed(cx)
        .bg(cx.theme().background)
}

pub fn over_backdrop(id: &'static str, content: impl IntoElement, cx: &App) -> AnyElement {
    let backdrop = crate::backdrop::backdrop(crate::knobs().backdrop, cx);
    div()
        .id(id)
        .relative()
        .w_full()
        .overflow_hidden()
        .rounded(cx.theme().radius)
        .framed(cx)
        .bg(cx.theme().surface(Elevation::Panel).fill)
        .children(backdrop)
        .child(
            h_flex()
                .relative()
                .w_full()
                .p(px(PLATE_PADDING))
                .justify_center()
                .child(content),
        )
        .into_any_element()
}

pub fn heading(title: &'static str, description: &'static str, cx: &App) -> impl IntoElement {
    v_flex()
        .gap_0p5()
        .child(
            div()
                .text_sm()
                .font_weight(ala::FontWeight::SEMIBOLD)
                .child(title),
        )
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().foreground.muted())
                .child(description),
        )
}

pub fn labeled(label: &'static str, control: impl IntoElement, cx: &App) -> impl IntoElement {
    v_flex()
        .gap_1p5()
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().foreground.muted())
                .child(label),
        )
        .child(control)
}

pub fn toggle(
    id: &'static str,
    title: &'static str,
    description: &'static str,
    checked: bool,
    on_click: impl Fn(&bool, &mut Window, &mut App) + 'static,
    cx: &App,
) -> impl IntoElement {
    h_flex()
        .gap_3()
        .justify_between()
        .child(
            v_flex()
                .min_w_0()
                .child(div().text_sm().child(title))
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().foreground.muted())
                        .child(description),
                ),
        )
        .child(
            Switch::new(ElementId::from(id))
                .checked(checked)
                .on_click(on_click),
        )
}

pub fn phone_frame(id: &'static str, cx: &App) -> ala::Stateful<ala::Div> {
    let theme = cx.theme();
    v_flex()
        .id(id)
        .w_full()
        .max_w(px(PHONE_WIDTH))
        .overflow_hidden()
        .rounded(theme.radius)
        .framed(cx)
        .bg(theme.background)
}

pub struct Preferences {
    restore: bool,
    animations: bool,
    sound: bool,
    scrollback: usize,
}

impl Preferences {
    pub fn view(cx: &mut App) -> Entity<Self> {
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

pub struct Toasts {
    toasts: [Entity<Notification>; 2],
    tooltip: Entity<Tooltip>,
}

impl Toasts {
    pub fn new(cx: &mut App) -> Self {
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

    pub fn stack(&self) -> ala::Div {
        v_flex()
            .gap_3()
            .items_center()
            .children(self.toasts.iter().cloned())
            .child(self.tooltip.clone())
    }
}

pub fn buttons(cx: &App) -> AnyElement {
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

pub fn dialog(cx: &App) -> AnyElement {
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
