use ala::{
    AnyElement, App, IntoElement, ParentElement as _, SharedString, Styled as _, Window, px,
};
use penna::{
    ActiveTheme as _, Chip, ChipLook, Emphasis, IconName, Label, Notice, PaintState,
    Selectable as _, Sizable as _, Space, Steps, TextStyle, Tone,
    button::{Button, ButtonVariants as _},
    h_flex, v_flex,
};

use crate::story::{Section, Story, row, stateless, states};

pub const STORY: Story = Story {
    id: "status",
    name: "Status",
    group: "Kit",
    summary: "Pieces that say what state something is in: a chip names it, a step strip says how far a setup has come, a notice says what needs doing.",
    sections: &[
        Section {
            id: "chips",
            name: "Chips",
            summary: "Chip::new(id, label) with a dot or an icon in front and a muted detail after it. Selected tints it; tone and emphasis paint it like a badge.",
            build: |_, cx| stateless(chips, cx),
        },
        Section {
            id: "chip-looks",
            name: "Chip looks",
            summary: "Chip::new(id, label).look(look).state(state) for status in a title bar, over a pane, the display-panes markers, the chip that follows a dragged pane and the disc behind a blank page's icon. Chip::empty(id) takes its own children; .ring(color) recolors the ring a look draws; .interactive(true) lights it on hover.",
            build: |_, cx| stateless(chip_looks, cx),
        },
        Section {
            id: "steps",
            name: "Steps",
            summary: "Steps::new(labels).current(index). Done steps show a check, the current one is selected, later ones are muted.",
            build: |_, cx| stateless(steps, cx),
        },
        Section {
            id: "notices",
            name: "Notices",
            summary: "Notice::new(tone, title) with a detail and an action. The icon follows the tone unless .icon or .dot(tone) replaces it. Buttons on it pass Notice::ground(tone, cx).",
            build: |_, cx| stateless(notices, cx),
        },
        Section {
            id: "stacked",
            name: "Stacked",
            summary: ".stacked() puts the title on its own line and the action under the detail, for narrow panels.",
            build: |_, cx| stateless(stacked, cx),
        },
    ],
};

fn chips(_: &mut Window, _: &mut App) -> AnyElement {
    states()
        .state(
            "dots",
            row().children(
                [
                    (Tone::Success, "Ready"),
                    (Tone::Accent, "On ubuntu"),
                    (Tone::Warning, "Checking"),
                    (Tone::Danger, "Needs attention"),
                    (Tone::Neutral, "Paused"),
                ]
                .map(|(tone, label)| Chip::new(label, label).dot(tone)),
            ),
        )
        .state(
            "detail",
            h_flex().w(Space::S16.rems() * 12.0).child(
                Chip::new("detail", "Ready")
                    .dot(Tone::Success)
                    .detail("ubuntu is to the right, past the edge of the built-in display"),
            ),
        )
        .state(
            "icon",
            row()
                .child(
                    Chip::new("macbook", "macbook")
                        .icon(IconName::Laptop)
                        .detail("this Mac"),
                )
                .child(Chip::new("unzoom", "Unzoom").icon(IconName::WindowRestore)),
        )
        .state(
            "selected",
            row()
                .child(Chip::new("rest", "ubuntu").dot(Tone::Success))
                .child(
                    Chip::new("selected", "ubuntu")
                        .dot(Tone::Success)
                        .selected(true),
                )
                .child(
                    Chip::new("primary", "On ubuntu")
                        .dot(Tone::Accent)
                        .tone(Tone::Primary)
                        .selected(true),
                ),
        )
        .state(
            "emphasis",
            row()
                .children(Emphasis::ALL.map(|emphasis| {
                    Chip::new(emphasis.name(), emphasis.name()).emphasis(emphasis)
                })),
        )
        .state(
            "tones, solid",
            row().children(Tone::ALL.map(|tone| {
                Chip::new(tone.name(), tone.name())
                    .tone(tone)
                    .emphasis(Emphasis::Solid)
            })),
        )
        .state(
            "small",
            row()
                .child(Chip::new("small", "ubuntu").dot(Tone::Success).small())
                .child(
                    Chip::new("xps", "xps")
                        .dot(Tone::Neutral)
                        .small()
                        .detail("offline"),
                ),
        )
        .into_any_element()
}

const CHIP_LOOKS: [(ChipLook, &str); 6] = [
    (ChipLook::Plain, "plain"),
    (ChipLook::Bar, "bar"),
    (ChipLook::Floating, "floating"),
    (ChipLook::Marker, "marker"),
    (ChipLook::Drag, "drag"),
    (ChipLook::Icon, "icon"),
];

const CHIP_STATES: [(PaintState, &str); 4] = [
    (PaintState::Rest, "Rest"),
    (PaintState::Hover, "Hover"),
    (PaintState::Selected, "Selected"),
    (PaintState::Disabled, "Disabled"),
];

fn chip_looks(_: &mut Window, cx: &mut App) -> AnyElement {
    let ring = cx.theme().success;
    CHIP_LOOKS
        .iter()
        .fold(states().columns(2), |states, (look, name)| {
            states.state(
                *name,
                row().children(CHIP_STATES.iter().map(|(state, label)| {
                    Chip::new(SharedString::from(format!("chip-{name}-{label}")), *label)
                        .look(*look)
                        .state(*state)
                })),
            )
        })
        .state(
            "marker with a ring colour, pointed at to light",
            row()
                .child(
                    Chip::new("marker-live", "1")
                        .look(ChipLook::Marker)
                        .interactive(true),
                )
                .child(
                    Chip::new("marker-ring", "2")
                        .look(ChipLook::Marker)
                        .state(PaintState::Current)
                        .ring(ring),
                ),
        )
        .state(
            "empty with children",
            row()
                .child(
                    Chip::empty("chip-empty-body")
                        .look(ChipLook::Floating)
                        .px(Space::S5)
                        .gap(Space::S3)
                        .child(Label::new(TextStyle::Small, "%3").strong())
                        .child(Label::new(TextStyle::Small, "vim")),
                )
                .child(
                    Chip::empty("chip-empty-icon")
                        .look(ChipLook::Icon)
                        .size(px(32.0))
                        .px_0()
                        .justify_center()
                        .child(penna::Icon::new(IconName::Globe).small()),
                )
                .child(
                    Chip::new("chip-vim-mode", "INSERT")
                        .look(ChipLook::Floating)
                        .tone(Tone::Primary)
                        .emphasis(Emphasis::Solid),
                ),
        )
        .into_any_element()
}

fn steps(_: &mut Window, _: &mut App) -> AnyElement {
    let labels = ["Allow", "Find", "Place"];
    (0..=labels.len())
        .fold(states(), |states, current| {
            let name = labels
                .get(current)
                .map_or_else(|| "all done".to_owned(), |label| format!("on {label}"));
            states.state(name, Steps::new(labels).current(current))
        })
        .into_any_element()
}

fn notices(_: &mut Window, cx: &mut App) -> AnyElement {
    let warning = Notice::ground(Tone::Warning, cx);
    states()
        .state(
            "tones",
            v_flex().gap(Space::S4).children(
                [Tone::Danger, Tone::Warning, Tone::Success, Tone::Primary].map(|tone| {
                    Notice::new(tone, format!("A {} notice", tone.name()))
                        .detail("What happened and what to do about it.")
                }),
            ),
        )
        .state(
            "with an action",
            Notice::new(Tone::Warning, "Nidus can't move the pointer on this Mac")
                .detail("macOS needs Accessibility turned on for Nidus.")
                .action(
                    Button::new("allow")
                        .small()
                        .primary()
                        .label("Allow…")
                        .on(warning),
                ),
        )
        .state(
            "title only",
            Notice::new(Tone::Neutral, "Nobody new around").icon(IconName::Search),
        )
        .into_any_element()
}

fn stacked(_: &mut Window, cx: &mut App) -> AnyElement {
    let warning = Notice::ground(Tone::Warning, cx);
    states()
        .state(
            "with a dot and an action",
            v_flex().w(Space::S16.rems() * 11.0).child(
                Notice::new(Tone::Warning, "Needs one thing")
                    .dot(Tone::Warning)
                    .stacked()
                    .detail("Nidus can't move the pointer on this Mac, so nobody can come here.")
                    .action(
                        Button::new("allow-stacked")
                            .small()
                            .primary()
                            .label("Allow…")
                            .on(warning),
                    ),
            ),
        )
        .state(
            "ready",
            v_flex().w(Space::S16.rems() * 11.0).child(
                Notice::new(Tone::Success, "Ready")
                    .dot(Tone::Success)
                    .stacked()
                    .detail("Push past the right edge to reach ubuntu."),
            ),
        )
        .into_any_element()
}
