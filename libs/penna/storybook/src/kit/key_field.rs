use ala::{
    AnyView, App, AppContext as _, Context, Entity, IntoElement, Keystroke, ParentElement as _,
    Render, SharedString, Styled as _, Window, div, px,
};
use penna::{
    Disableable as _, Sizable as _, Size,
    blocks::settings::{SettingEntry, SettingsStack, settings_card_ground},
    components::key_field::{KeyField, KeyFieldState},
};

use super::support::after_first_frame;
use crate::story::{Section, Story, states};

pub const STORY: Story = Story {
    id: "key-field",
    name: "Key field",
    group: "Kit",
    summary: "A field that records one keystroke. A click, Enter or Space starts recording; the next key is taken before any key binding and emitted as KeyFieldEvent::Record. Escape or leaving the field cancels.",
    sections: &[
        Section {
            id: "key-field",
            name: "Key field",
            summary: "KeyField::new(&state) on a KeyFieldState. At rest the keystroke sits centered as a keycap, or the placeholder (None) in the faint ink. A shifted symbol records as the symbol.",
            build: |window, cx| cx.new(|cx| Fields::new(window, cx)).into(),
        },
        Section {
            id: "key-field-settings",
            name: "On a settings card",
            summary: "At the end of a settings row, on the card's ground (.on(ground)), as wide as the caller makes it.",
            build: |window, cx| cx.new(|cx| OnCard::new(window, cx)).into(),
        },
        Section {
            id: "key-field-recording",
            name: "Recording",
            summary: "While recording the field holds focus and its focused edge, with the prompt at its start instead of the value.",
            build: |window, cx| Recording::view(window, cx),
        },
    ],
};

fn key(spelling: &str) -> Option<Keystroke> {
    Keystroke::parse(spelling).ok()
}

fn field(spelling: Option<&str>, window: &mut Window, cx: &mut App) -> Entity<KeyFieldState> {
    cx.new(|cx| KeyFieldState::new(window, cx).value(spelling.and_then(key)))
}

fn sized(element: KeyField) -> impl IntoElement {
    div().w(px(120.0)).child(element)
}

struct Fields {
    rows: Vec<(&'static str, &'static str, Entity<KeyFieldState>)>,
    disabled: Entity<KeyFieldState>,
    unbound: Entity<KeyFieldState>,
    sizes: Vec<(&'static str, Size, Entity<KeyFieldState>)>,
}

impl Fields {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let rows = [
            ("empty", "key-empty", None),
            ("a key", "key-control", Some("ctrl-b")),
            ("a shifted symbol", "key-symbol", Some("\"")),
            ("a chord", "key-chord", Some("cmd-shift-p")),
        ]
        .into_iter()
        .map(|(label, id, spelling)| (label, id, field(spelling, window, cx)))
        .collect();
        let sizes = [
            ("xsmall", Size::XSmall),
            ("small", Size::Small),
            ("large", Size::Large),
        ]
        .into_iter()
        .map(|(name, size)| (name, size, field(Some("ctrl-a"), window, cx)))
        .collect();
        Self {
            rows,
            disabled: field(Some("ctrl-b"), window, cx),
            unbound: field(None, window, cx),
            sizes,
        }
    }
}

impl Render for Fields {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let states = self
            .rows
            .iter()
            .fold(states().columns(3), |states, (label, id, state)| {
                states.state(*label, sized(KeyField::new(state).id(*id)))
            })
            .state(
                "disabled",
                sized(
                    KeyField::new(&self.disabled)
                        .id("key-disabled")
                        .disabled(true),
                ),
            )
            .state(
                "placeholder",
                sized(
                    KeyField::new(&self.unbound)
                        .id("key-unbound")
                        .placeholder("Unbound"),
                ),
            );
        self.sizes
            .iter()
            .fold(states, |states, (name, size, state)| {
                states.state(
                    *name,
                    sized(
                        KeyField::new(state)
                            .id(SharedString::from(format!("key-{name}")))
                            .with_size(*size),
                    ),
                )
            })
    }
}

struct OnCard {
    prefix: Entity<KeyFieldState>,
    split: Entity<KeyFieldState>,
}

impl OnCard {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            prefix: field(Some("ctrl-b"), window, cx),
            split: field(Some("%"), window, cx),
        }
    }
}

impl Render for OnCard {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ground = settings_card_ground(cx);
        states().columns(1).state(
            "prefix and split keys",
            SettingsStack::titled("Multiplexer")
                .id("stack-keys")
                .child(
                    SettingEntry::new("Prefix")
                        .id("entry-prefix")
                        .description("Pressed before every multiplexer shortcut")
                        .control(
                            div()
                                .w(px(96.0))
                                .child(KeyField::new(&self.prefix).id("key-prefix").on(ground)),
                        ),
                )
                .child(
                    SettingEntry::new("Split right").id("entry-split").control(
                        div()
                            .w(px(96.0))
                            .child(KeyField::new(&self.split).id("key-split").on(ground)),
                    ),
                ),
        )
    }
}

struct Recording {
    live: Entity<KeyFieldState>,
    resting: Entity<KeyFieldState>,
}

impl Recording {
    fn view(window: &mut Window, cx: &mut App) -> AnyView {
        let live = field(Some("ctrl-b"), window, cx);
        let resting = field(Some("ctrl-b"), window, cx);
        let target = live.clone();
        after_first_frame(window, move |window, cx| {
            target.update(cx, |state, cx| state.record(window, cx));
        });
        cx.new(|_| Self { live, resting }).into()
    }
}

impl Render for Recording {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        states()
            .columns(3)
            .state(
                "recording",
                sized(KeyField::new(&self.live).id("key-recording")),
            )
            .state(
                "resting",
                sized(KeyField::new(&self.resting).id("key-resting")),
            )
    }
}
