use ala::{
    AnyElement, App, AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render,
    Styled as _, Window, div, px,
};
use penna::{
    ActiveTheme as _, IconName,
    blocks::chat::{
        ActivityEnd, ActivityMark, AskCard, ComposerAction, PhoneComposer, activity_row,
        composer_ground, phone_action_button, phone_close_button, phone_jump_button,
        phone_plus_button, phone_queued_row,
    },
    blocks::touch::{TouchInk, TouchRow, TouchTrailing},
    components::input::InputState,
    v_flex,
};

use crate::story::{Section, Story, stateless, states};

pub const STORY: Story = Story {
    id: "phone-chat",
    name: "Phone chat",
    group: "Kit",
    summary: "The chat's phone pieces: the composer row, the ask card that takes its place, and the transcript's touch rows.",
    sections: &[
        Section {
            id: "composer",
            name: "Composer row",
            summary: "PhoneComposer: the field.composer surface at least 56 tall, a 44 plus, the field in touch-body, a 44 action, all inset 6; its corner is its buttons' plus the 6. Send is solid and off while the field is empty, stop is solid, queue is soft. Something else swaps the plus for a close and puts the question over the field.",
            build: |window, cx| cx.new(|cx| Composers::new(window, cx)).into(),
        },
        Section {
            id: "ask-card",
            name: "Ask card",
            summary: "AskCard: the field.composer surface at the phone container corner, its rows 4 in. What is asked in strong touch-body over the command in touch-small mono, one line until a tap; a count at the end of the first line; then touch rows, a quiet one for Something else and an accent one that sends.",
            build: |_, cx| stateless(cards, cx),
        },
        Section {
            id: "steps",
            name: "Transcript rows",
            summary: "ActivityMark::touch: a step at least 44 tall, a 20 glyph and its label in touch-small muted; the queued row, 44 tall; jump to latest, a 44 icon button on the floating surface.",
            build: |_, cx| stateless(steps, cx),
        },
    ],
};

const WIDTH: f32 = 370.0;

fn column(child: impl IntoElement) -> impl IntoElement {
    div().w(px(WIDTH)).child(child)
}

struct Composers {
    empty: Entity<InputState>,
    typed: Entity<InputState>,
    long: Entity<InputState>,
    other: Entity<InputState>,
}

fn input(value: &str, window: &mut Window, cx: &mut App) -> Entity<InputState> {
    let value = value.to_owned();
    cx.new(|cx| {
        InputState::new(window, cx)
            .placeholder("Message Claude Code…")
            .auto_grow(1, 5)
            .default_value(value)
    })
}

impl Composers {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            empty: input("", window, cx),
            typed: input("Run the menu story too", window, cx),
            long: input(
                "Put New chat last in the New group so it sits nearest the thumb, keep the label chip at 28 and muted with no glyph, and run the pill tests and the menu story when you are done.",
                window,
                cx,
            ),
            other: input("Ask me first", window, cx),
        }
    }
}

impl Render for Composers {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ground = cx.theme().ground();
        let on = composer_ground(ground, cx);
        let row = |id: &'static str, input: &Entity<InputState>, action, enabled| PhoneComposer {
            input: input.clone(),
            leading: phone_plus_button((id, 0usize), on).into_any_element(),
            action: phone_action_button((id, 1usize), action, enabled, on).into_any_element(),
            thumbnails: Vec::new(),
            above: None,
            ground,
        };
        states()
            .columns(2)
            .state(
                "empty: send off",
                column(row(
                    "composer-empty",
                    &self.empty,
                    ComposerAction::Send,
                    false,
                )),
            )
            .state(
                "typed: send",
                column(row(
                    "composer-typed",
                    &self.typed,
                    ComposerAction::Send,
                    true,
                )),
            )
            .state(
                "working: stop",
                column(row(
                    "composer-stop",
                    &self.empty,
                    ComposerAction::Stop,
                    true,
                )),
            )
            .state(
                "typing while it works: queue",
                column(row(
                    "composer-queue",
                    &self.typed,
                    ComposerAction::Queue,
                    true,
                )),
            )
            .state(
                "five lines, then it scrolls",
                column(row("composer-long", &self.long, ComposerAction::Send, true)),
            )
            .state(
                "Something else",
                column(PhoneComposer {
                    input: self.other.clone(),
                    leading: phone_close_button("composer-other-close", on).into_any_element(),
                    action: phone_action_button(
                        "composer-other-send",
                        ComposerAction::Send,
                        true,
                        on,
                    )
                    .into_any_element(),
                    thumbnails: Vec::new(),
                    above: Some("Which port should it listen on?".into()),
                    ground,
                }),
            )
    }
}

fn cards(_: &mut Window, cx: &mut App) -> AnyElement {
    let ground = cx.theme().ground();
    let approval = |id: &'static str, expanded: bool| {
        AskCard::new(id, "Run this command?", ground)
            .detail("cargo test -p penna pill_menu --all-features")
            .expanded(expanded)
            .rows([
                TouchRow::new((id, 0usize), "Allow once"),
                TouchRow::new((id, 1usize), "Always allow cargo test"),
                TouchRow::new((id, 2usize), "Deny"),
            ])
    };
    let question = AskCard::new(
        "card-question",
        "Should the daemon embed the tailnet node, or talk to tailscaled?",
        ground,
    )
    .count("1 of 2")
    .rows([
        TouchRow::new("card-question-embed", "Embed it").detail("One process, no system service"),
        TouchRow::new("card-question-talk", "Talk to tailscaled")
            .detail("Uses the node the machine already runs"),
        TouchRow::new("card-question-other", "Something else").ink(TouchInk::Quiet),
    ]);
    let several = AskCard::new("card-several", "Which checks should run first?", ground).rows([
        TouchRow::new("card-several-test", "cargo test").trailing(TouchTrailing::Check),
        TouchRow::new("card-several-clippy", "cargo clippy").trailing(TouchTrailing::Check),
        TouchRow::new("card-several-web", "just web build"),
        TouchRow::new("card-several-send", "Send 2 answers").ink(TouchInk::Accent),
    ]);
    states()
        .columns(2)
        .state("approval", column(approval("card-approval", false)))
        .state(
            "approval, tapped open",
            column(approval("card-opened", true)),
        )
        .state("a question with details", column(question))
        .state("several answers", column(several))
        .into_any_element()
}

fn steps(_: &mut Window, cx: &mut App) -> AnyElement {
    let step = |id: &'static str, icon, label: &'static str, detail: Option<&'static str>| {
        let mut mark = ActivityMark::plain(icon);
        mark.touch = true;
        mark.detail = detail.map(Into::into);
        activity_row(id, mark, label.into(), ActivityEnd::None, cx)
    };
    let mut running = ActivityMark::plain(IconName::SquareTerminal);
    running.touch = true;
    running.spin = Some(0.25);
    let mut folded = ActivityMark::bare();
    folded.touch = true;
    states()
        .columns(2)
        .state(
            "steps",
            column(
                v_flex()
                    .child(step("step-read", IconName::File, "Read pill_menu.rs", None))
                    .child(step(
                        "step-edit",
                        IconName::Pencil,
                        "Edited pill_menu.rs",
                        Some("+14 −3"),
                    ))
                    .child(activity_row(
                        "step-running",
                        running,
                        "cargo test -p penna pill".into(),
                        ActivityEnd::None,
                        cx,
                    ))
                    .child(activity_row(
                        "step-folded",
                        folded,
                        "Worked for 19s".into(),
                        ActivityEnd::Disclosure(false),
                        cx,
                    )),
            ),
        )
        .state(
            "queued and jump to latest",
            column(
                v_flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(phone_jump_button("step-jump", cx.theme().ground()))
                    .child(phone_queued_row(
                        "step-queued",
                        "2 queued",
                        true,
                        |_, _| {},
                        cx,
                    )),
            ),
        )
        .into_any_element()
}
