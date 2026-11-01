use ala::{
    AnyElement, AnyView, App, AppContext as _, Context, IntoElement, MouseButton,
    ParentElement as _, Render, Styled as _, Subscription, Window, div, px,
};
use penna::{
    ActiveTheme as _, Colorize as _, Disableable as _, Emphasis, IconName, Selectable as _,
    Sizable as _, Tone,
    button::{Button, ButtonLook, ButtonRounded, ButtonVariant, ButtonVariants as _},
};

use super::support::{Probe, after_first_frame, hover, press, when_visible};
use crate::story::{Section, Story, row, stateless, states};

pub const STORY: Story = Story {
    id: "buttons",
    name: "Buttons",
    group: "Kit",
    summary: "One button, drawn as a tone at an emphasis (see the Design system story) or as a link or plain text. The shorthands name the common pairs: primary is the main action, secondary and ghost the quiet ones.",
    sections: &[
        Section {
            id: "variants",
            name: "Variants",
            summary: "The shorthands at the default size, and the link and text forms.",
            build: |_, cx| stateless(variants, cx),
        },
        Section {
            id: "sizes",
            name: "Sizes",
            summary: "Sizable: small (24px) and medium (28px, the default), with and without labels.",
            build: |_, cx| stateless(sizes, cx),
        },
        Section {
            id: "states",
            name: "States",
            summary: "Disabled, selected and loading, plus icons, carets, outlines and tooltips.",
            build: |_, cx| stateless(button_states, cx),
        },
        Section {
            id: "custom-and-shapes",
            name: "Shapes",
            summary: "Flat buttons without a resting surface, compact padding and rounded overrides.",
            build: |_, cx| stateless(custom_and_shapes, cx),
        },
        Section {
            id: "looks",
            name: "Looks",
            summary: "Button::look(look) for buttons set into the app's chrome: icon, quiet and flat buttons, the window chrome's, pane header and flip buttons, the actions that show on a hovered row, the jump-to-latest button, host actions, the composer's send button and a pressable pane chip. Each row is resting, selected and disabled; point at a resting one to see its hover.",
            build: |_, cx| stateless(looks, cx),
        },
        Section {
            id: "hovered",
            name: "Hovered",
            summary: "The pointer rests on the first button. Neutral variants lift with a washed tint and a highlight ring.",
            build: |window, cx| Pointer::view(Gesture::Hover, window, cx),
        },
        Section {
            id: "pressed",
            name: "Pressed",
            summary: "The primary button is held down and shows its active fill next to a resting copy.",
            build: |window, cx| Pointer::view(Gesture::Press, window, cx),
        },
        Section {
            id: "focused",
            name: "Focused",
            summary: "Keyboard focus draws an accent ring outside the border. Only tab focus reaches it; a click does not.",
            build: |window, cx| Pointer::view(Gesture::Focus, window, cx),
        },
    ],
};

const VARIANTS: [(&str, ButtonVariant); 8] = [
    (
        "Primary",
        ButtonVariant::Paint(Tone::Primary, Emphasis::Solid),
    ),
    (
        "Secondary",
        ButtonVariant::Paint(Tone::Neutral, Emphasis::Soft),
    ),
    (
        "Ghost",
        ButtonVariant::Paint(Tone::Neutral, Emphasis::Ghost),
    ),
    ("Danger", ButtonVariant::Paint(Tone::Danger, Emphasis::Soft)),
    (
        "Success",
        ButtonVariant::Paint(Tone::Success, Emphasis::Soft),
    ),
    (
        "Warning",
        ButtonVariant::Paint(Tone::Warning, Emphasis::Soft),
    ),
    ("Link", ButtonVariant::Link),
    ("Text", ButtonVariant::Text),
];

fn variants(_: &mut Window, _: &mut App) -> AnyElement {
    row()
        .children(VARIANTS.iter().map(|(name, variant)| {
            Button::new(format!("variant-{name}"))
                .label(*name)
                .with_variant(*variant)
        }))
        .into_any_element()
}

fn sizes(_: &mut Window, _: &mut App) -> AnyElement {
    states()
        .state(
            "label",
            row()
                .child(Button::new("size-sm").label("Small").small())
                .child(Button::new("size-md").label("Medium")),
        )
        .state(
            "icon",
            row()
                .child(
                    Button::new("icon-sm")
                        .icon(IconName::Bell)
                        .small()
                        .tooltip("Notifications"),
                )
                .child(
                    Button::new("icon-md")
                        .icon(IconName::Bell)
                        .tooltip("Notifications"),
                )
                .child(Button::compact_icon("icon-compact", IconName::Ellipsis).tooltip("More")),
        )
        .into_any_element()
}

fn button_states(_: &mut Window, _: &mut App) -> AnyElement {
    states()
        .columns(2)
        .state(
            "disabled",
            row().children(VARIANTS.iter().take(5).map(|(name, variant)| {
                Button::new(format!("disabled-{name}"))
                    .label(*name)
                    .with_variant(*variant)
                    .disabled(true)
            })),
        )
        .state(
            "selected",
            row()
                .child(
                    Button::new("selected-ghost")
                        .label("Ghost")
                        .ghost()
                        .selected(true),
                )
                .child(
                    Button::new("selected-default")
                        .label("Default")
                        .selected(true),
                ),
        )
        .state(
            "loading",
            row()
                .child(Button::new("loading-default").label("Saving").loading(true))
                .child(
                    Button::new("loading-accent")
                        .label("Sending")
                        .primary()
                        .loading(true),
                ),
        )
        .state(
            "icon and label",
            row()
                .child(Button::new("copy").icon(IconName::Copy).label("Copy"))
                .child(
                    Button::new("open")
                        .icon(IconName::ExternalLink)
                        .label("Open")
                        .ghost(),
                ),
        )
        .state(
            "dropdown caret",
            row()
                .child(Button::new("caret").label("Model").dropdown_caret(true))
                .child(
                    Button::new("caret-ghost")
                        .label("Branch")
                        .ghost()
                        .dropdown_caret(true),
                ),
        )
        .state(
            "outline",
            row()
                .child(
                    Button::new("outline")
                        .label("Outline")
                        .emphasis(Emphasis::Outline),
                )
                .child(
                    Button::new("outline-danger")
                        .label("Delete")
                        .danger()
                        .emphasis(Emphasis::Outline),
                ),
        )
        .into_any_element()
}

fn custom_and_shapes(_: &mut Window, cx: &mut App) -> AnyElement {
    let theme = cx.theme();
    states()
        .columns(2)
        .state(
            "flat",
            row()
                .child(Button::new("flat-default").label("Default").flat())
                .child(Button::new("flat-ghost").label("Ghost").ghost().flat())
                .child(
                    Button::new("flat-secondary")
                        .label("Secondary")
                        .secondary()
                        .flat(),
                ),
        )
        .state(
            "compact",
            row()
                .child(Button::new("compact-sm").label("Run").compact())
                .child(Button::new("compact-md").label("Run").compact())
                .child(Button::new("normal-md").label("Run"))
                .child(Button::compact_icon("compact-icon", IconName::Plus).tooltip("Add")),
        )
        .state(
            "rounded",
            row()
                .child(
                    Button::new("rounded-0")
                        .label("Square")
                        .rounded(ButtonRounded::Size(px(0.0))),
                )
                .child(Button::new("rounded-theme").label("Theme radius"))
                .child(
                    Button::new("rounded-pill")
                        .label("Pill")
                        .rounded(ButtonRounded::Size(px(999.0))),
                )
                .child(
                    Button::new("rounded-icon")
                        .icon(IconName::Plus)
                        .rounded(ButtonRounded::Size(px(999.0))),
                ),
        )
        .state(
            "loading with an icon",
            row()
                .child(
                    Button::new("loading-icon")
                        .icon(IconName::Copy)
                        .label("Copying")
                        .loading(true),
                )
                .child(
                    Button::new("loading-icon-only")
                        .icon(IconName::Copy)
                        .loading(true),
                ),
        )
        .state(
            "children",
            row().child(
                Button::new("children").child(
                    div().flex().items_center().gap_2().child("Branch").child(
                        div()
                            .text_xs()
                            .text_color(theme.foreground.muted())
                            .child("main"),
                    ),
                ),
            ),
        )
        .into_any_element()
}

const LOOKS: [(ButtonLook, &str, bool); 11] = [
    (ButtonLook::Icon, "icon", true),
    (ButtonLook::Quiet, "quiet", false),
    (ButtonLook::Flat, "flat", true),
    (ButtonLook::Chrome, "chrome", true),
    (ButtonLook::Bar, "bar", true),
    (ButtonLook::Toggle, "toggle", true),
    (ButtonLook::RowIcon, "row icon", true),
    (ButtonLook::Floating, "floating", true),
    (ButtonLook::Row, "row", false),
    (ButtonLook::Send, "send", true),
    (ButtonLook::FloatingChip, "floating chip", false),
];

fn looks(_: &mut Window, _: &mut App) -> AnyElement {
    LOOKS
        .iter()
        .fold(states().columns(3), |states, (look, name, icon_only)| {
            let button = |state: &str| {
                let button = Button::new(format!("look-{name}-{state}")).look(*look);
                if *icon_only {
                    button.ghost().icon(IconName::Plus).tooltip(*name)
                } else {
                    button.ghost().label("Connect")
                }
            };
            states.state(
                *name,
                row()
                    .child(button("rest").on_click(|_, _, _| {}))
                    .child(button("selected").selected(true))
                    .child(button("disabled").disabled(true)),
            )
        })
        .into_any_element()
}

#[derive(Clone, Copy, PartialEq)]
enum Gesture {
    Hover,
    Press,
    Focus,
}

struct Pointer {
    gesture: Gesture,
    target: Probe,
    _bounds: Subscription,
}

impl Pointer {
    fn view(gesture: Gesture, window: &mut Window, cx: &mut App) -> AnyView {
        let target = Probe::default();
        let probe = target.clone();
        match gesture {
            Gesture::Hover => when_visible(&target, window, move |window, cx| {
                hover(&probe, window, cx);
            }),
            Gesture::Press => when_visible(&target, window, move |window, cx| {
                press(&probe, MouseButton::Left, window, cx);
            }),
            Gesture::Focus => after_first_frame(window, Window::focus_next),
        }
        cx.new(|cx| Self {
            gesture,
            _bounds: cx.observe_window_bounds(window, |this: &mut Self, window, cx| {
                if this.gesture != Gesture::Focus {
                    hover(&this.target, window, cx);
                }
            }),
            target,
        })
        .into()
    }

    fn pair(&self, label: &str, variant: ButtonVariant) -> impl IntoElement {
        row()
            .child(
                div()
                    .relative()
                    .child(
                        Button::new("pointer-target")
                            .label(label.to_string())
                            .with_variant(variant),
                    )
                    .child(self.target.measure()),
            )
            .child(
                Button::new("pointer-rest")
                    .label(label.to_string())
                    .with_variant(variant)
                    .tab_stop(false),
            )
    }
}

impl Render for Pointer {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let (label, variant) = match self.gesture {
            Gesture::Hover => ("Secondary", ButtonVariant::default()),
            Gesture::Press => (
                "Primary",
                ButtonVariant::Paint(Tone::Primary, Emphasis::Solid),
            ),
            Gesture::Focus => ("Focused", ButtonVariant::default()),
        };
        let caption = match self.gesture {
            Gesture::Hover => "hovered, resting",
            Gesture::Press => "pressed, resting",
            Gesture::Focus => "focused, resting",
        };
        states().state(caption, self.pair(label, variant))
    }
}
