use ala::{
    AnyElement, AnyView, App, AppContext as _, Context, IntoElement, ParentElement as _, Render,
    Styled as _, Window, div, px,
};
use penna::{
    Disableable as _, Sizable as _, Tone,
    components::check::{Check, Checkbox, RadioGroup},
};

use super::support::after_first_frame;
use crate::story::{Section, Story, row, stateless, states};

pub const STORY: Story = Story {
    id: "checks",
    name: "Checks",
    group: "Kit",
    summary: "Checkbox and RadioGroup: a box beside its label for one option, a set of radios for one of a few, and the bare Check mark a row or tile holds.",
    sections: &[
        Section {
            id: "checkbox",
            name: "Checkbox",
            summary: "Off, on, mixed, disabled and error. The box lifts while the pointer is over the box or the label and darkens while pressed; a press flips it on release, and a mixed one turns on.",
            build: |_, cx| cx.new(|_| Checkboxes::default()).into(),
        },
        Section {
            id: "checkbox-sizes",
            name: "Checkbox, sizes",
            summary: "A 14 box beside caption and small text at xsmall and small, a 16 box beside body from medium up.",
            build: |_, cx| stateless(checkbox_sizes, cx),
        },
        Section {
            id: "checkbox-focused",
            name: "Checkbox, focused",
            summary: "Tab focus rings the box, one ring width further out once it is on. Space or Enter flips it; a click flips it without focusing.",
            build: |window, cx| FocusedCheckbox::view(window, cx),
        },
        Section {
            id: "radio",
            name: "Radio group",
            summary: "Stacked 8 apart, or in a row 16 apart. A press picks a radio; disabled and error groups.",
            build: |_, cx| cx.new(|_| Radios::default()).into(),
        },
        Section {
            id: "radio-focused",
            name: "Radio group, focused",
            summary: "One tab stop. The ring sits on the picked radio, or the first while none is picked; the arrows, Home and End pick, and stop at the ends.",
            build: |window, cx| FocusedRadios::view(window, cx),
        },
        Section {
            id: "mark",
            name: "Mark",
            summary: "The bare Check a list row, task list or tile holds: it draws the state and leaves the press to its row.",
            build: |_, cx| stateless(marks, cx),
        },
    ],
};

const START: [&str; 3] = ["Last session", "Home folder", "Empty window"];
const LAYOUT: [&str; 3] = ["Tiled", "Floating", "Stacked"];

struct Checkboxes {
    checked: [bool; 5],
    mixed: bool,
}

impl Default for Checkboxes {
    fn default() -> Self {
        Self {
            checked: [false, true, false, false, true],
            mixed: true,
        }
    }
}

impl Render for Checkboxes {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let checkbox = |id: &'static str, label: &'static str, at: usize| {
            Checkbox::new(id)
                .label(label)
                .checked(self.checked[at])
                .on_click(cx.listener(move |this, on: &bool, _, cx| {
                    this.checked[at] = *on;
                    cx.notify();
                }))
        };
        let off = checkbox("checkbox-off", "Restore sessions", 0);
        let on = checkbox("checkbox-on", "Restore sessions", 1);
        let wrap = checkbox(
            "checkbox-wrap",
            "Reopen every pane where it was when the window closed",
            2,
        );
        let error_off = checkbox("checkbox-error-off", "Accept the terms", 3).tone(Tone::Danger);
        let error_on = checkbox("checkbox-error-on", "Accept the terms", 4).tone(Tone::Danger);
        let mixed = Checkbox::new("checkbox-mixed")
            .label("All panes")
            .indeterminate(self.mixed)
            .checked(!self.mixed)
            .on_click(cx.listener(|this, on: &bool, _, cx| {
                this.mixed = !*on;
                cx.notify();
            }));
        states()
            .columns(3)
            .state("off", off)
            .state("on", on)
            .state("mixed", mixed)
            .state(
                "disabled",
                row()
                    .child(
                        Checkbox::new("checkbox-disabled-off")
                            .label("Sync")
                            .disabled(true),
                    )
                    .child(
                        Checkbox::new("checkbox-disabled-on")
                            .label("Sync")
                            .checked(true)
                            .disabled(true),
                    ),
            )
            .state("error", row().child(error_off).child(error_on))
            .state("long label wraps", div().w(px(200.0)).child(wrap))
    }
}

fn checkbox_sizes(_: &mut Window, _: &mut App) -> AnyElement {
    let checkbox = |id: &'static str| Checkbox::new(id).label("Restore sessions").checked(true);
    states()
        .columns(4)
        .state("xsmall", checkbox("checkbox-xsmall").xsmall())
        .state("small", checkbox("checkbox-small").small())
        .state("medium", checkbox("checkbox-medium"))
        .state(
            "large draws like medium",
            checkbox("checkbox-large").large(),
        )
        .into_any_element()
}

#[derive(Default)]
struct FocusedCheckbox {
    checked: [bool; 2],
}

impl FocusedCheckbox {
    fn view(window: &mut Window, cx: &mut App) -> AnyView {
        after_first_frame(window, Window::focus_next);
        cx.new(|_| Self::default()).into()
    }
}

impl Render for FocusedCheckbox {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let checkbox = |id: &'static str, label: &'static str, at: usize| {
            Checkbox::new(id)
                .label(label)
                .checked(self.checked[at])
                .on_click(cx.listener(move |this, on: &bool, _, cx| {
                    this.checked[at] = *on;
                    cx.notify();
                }))
        };
        states().state(
            "focused, resting",
            row()
                .child(checkbox("checkbox-focused", "Restore sessions", 0))
                .child(checkbox("checkbox-resting", "Share clipboard", 1)),
        )
    }
}

struct Radios {
    picked: [Option<usize>; 3],
}

impl Default for Radios {
    fn default() -> Self {
        Self {
            picked: [Some(0), Some(1), None],
        }
    }
}

impl Render for Radios {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let group = |id: &'static str, labels: [&'static str; 3], at: usize| {
            let pick = cx.listener(move |this, index: &usize, _, cx| {
                this.picked[at] = Some(*index);
                cx.notify();
            });
            RadioGroup::new(id, labels)
                .selected(self.picked[at])
                .on_select(move |index, window, cx| pick(&index, window, cx))
        };
        let start = group("radio-start", START, 0);
        let layout = group("radio-layout", LAYOUT, 1).horizontal();
        let none = group("radio-none", START, 2);
        states()
            .columns(3)
            .state("vertical", start)
            .state("horizontal", layout)
            .state("none picked", none)
            .state(
                "disabled",
                RadioGroup::new("radio-disabled", START)
                    .selected(0)
                    .disabled(true),
            )
            .state(
                "error",
                RadioGroup::new("radio-error", START)
                    .selected(1)
                    .tone(Tone::Danger),
            )
            .state(
                "small",
                RadioGroup::new("radio-small", START).selected(0).small(),
            )
    }
}

#[derive(Default)]
struct FocusedRadios {
    picked: Option<usize>,
}

impl FocusedRadios {
    fn view(window: &mut Window, cx: &mut App) -> AnyView {
        after_first_frame(window, Window::focus_next);
        cx.new(|_| Self::default()).into()
    }
}

impl Render for FocusedRadios {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let pick = cx.listener(|this, index: &usize, _, cx| {
            this.picked = Some(*index);
            cx.notify();
        });
        states().state(
            "focused, none picked",
            RadioGroup::new("radio-focused", START)
                .selected(self.picked)
                .on_select(move |index, window, cx| pick(&index, window, cx)),
        )
    }
}

fn marks(_: &mut Window, _: &mut App) -> AnyElement {
    states()
        .columns(3)
        .state(
            "checkbox: off, on, mixed",
            row()
                .child(Check::checkbox(false))
                .child(Check::checkbox(true))
                .child(Check::checkbox(false).indeterminate(true)),
        )
        .state(
            "radio: off, on",
            row().child(Check::radio(false)).child(Check::radio(true)),
        )
        .state(
            "small",
            row()
                .child(Check::checkbox(true).small())
                .child(Check::radio(true).small()),
        )
        .state(
            "disabled",
            row()
                .child(Check::checkbox(false).disabled(true))
                .child(Check::checkbox(true).disabled(true))
                .child(Check::radio(true).disabled(true)),
        )
        .state(
            "danger",
            row()
                .child(Check::checkbox(false).tone(Tone::Danger))
                .child(Check::checkbox(true).tone(Tone::Danger))
                .child(Check::radio(true).tone(Tone::Danger)),
        )
        .into_any_element()
}
