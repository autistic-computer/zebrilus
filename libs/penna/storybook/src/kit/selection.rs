use ala::{
    AnyView, App, AppContext as _, Context, Entity, IntoElement, MouseButton, ParentElement as _,
    Render, SharedString, Styled as _, Subscription, Window, div, px,
};
use penna::{
    Disableable as _, IndexPath, Sizable as _, Size,
    components::segmented::SegmentedControl,
    components::select::{Select, SelectState},
    components::slider::{DiscreteSlider, Slider},
    components::switch::Switch,
    h_flex,
};

use super::support::{Probe, after_first_frame, dispatch, hover, press, tab, when_visible};
use crate::story::{Section, Story, row, states};

pub const STORY: Story = Story {
    id: "choices",
    name: "Choices",
    group: "Kit",
    summary: "Controls that pick a value: the dropdown Select, the SegmentedControl, the Switch, the Slider and the DiscreteSlider.",
    sections: &[
        Section {
            id: "select",
            name: "Select",
            summary: "The trigger is a Button with a dropdown caret. One SelectState per Select, since the element writes its options into the state.",
            build: |window, cx| cx.new(|cx| Selects::new(window, cx)).into(),
        },
        Section {
            id: "select-open",
            name: "Select, open",
            summary: "Focus plus the Confirm action opens the menu: a scrollable PopupMenu as wide as the trigger, with the picked row checked.",
            build: |window, cx| OpenSelect::view(window, cx),
        },
        Section {
            id: "segmented",
            name: "Segmented control",
            summary: "One of a few short choices in a field-styled track. The picked one sits on a quiet button's selected fill, rounded concentric with the track, which slides to the next pick.",
            build: |window, cx| Segmented::view(window, cx),
        },
        Section {
            id: "segmented-focused",
            name: "Segmented control, focused",
            summary: "Tab focus rings the whole track. The arrow keys, Home and End move the pick; a click picks without focusing.",
            build: |window, cx| FocusedSegmented::view(window, cx),
        },
        Section {
            id: "segmented-sizes",
            name: "Segmented control, sizes",
            summary: "Every rung of the control ladder. The track takes the rung's height, the labels its text style and each segment its field inset plus space-2 at both sides.",
            build: |window, cx| SegmentedSizes::view(window, cx),
        },
        Section {
            id: "segmented-disabled",
            name: "Segmented control, disabled",
            summary: "The track takes the field's disabled paint, the pill and the labels their disabled step. A press or a key changes nothing, and it leaves the focus order.",
            build: |window, cx| DisabledSegmented::view(window, cx),
        },
        Section {
            id: "switch",
            name: "Switch",
            summary: "Off, on, small and disabled, then every other size on the ladder. A press flips the enabled ones; the thumb slides between states and holds still with motion off.",
            build: |window, cx| Switches::view(window, cx),
        },
        Section {
            id: "switch-focused",
            name: "Switch, focused",
            summary: "Tab focus rings the track; Space or Enter flips it. A click flips it on release without focusing.",
            build: |window, cx| FocusedSwitch::view(window, cx),
        },
        Section {
            id: "switch-hovered",
            name: "Switch, hovered",
            summary: "The pointer rests on the first switch: its track takes one fill step deeper, and the thumb doesn't change.",
            build: |window, cx| PointedSwitch::view(Gesture::Hover, window, cx),
        },
        Section {
            id: "switch-pressed",
            name: "Switch, pressed",
            summary: "The first switch is held down: an on track takes the primary's active fill, an off one a second step deeper. It flips on release.",
            build: |window, cx| PointedSwitch::view(Gesture::Press, window, cx),
        },
        Section {
            id: "slider",
            name: "Discrete slider",
            summary: "A row of pills up to the picked step, with the label on the left and the value on the right.",
            build: |window, cx| Sliders::view(window, cx),
        },
        Section {
            id: "slider-focused",
            name: "Discrete slider, focused",
            summary: "Tab focuses the slider: a ring outside the picked pill, and the arrow keys step it. A press focuses it too, with no ring until a key moves it.",
            build: |window, cx| FocusedSlider::view(window, cx),
        },
        Section {
            id: "slider-continuous",
            name: "Slider",
            summary: "A value on a range. A press moves the knob there and focuses it with no ring, a drag follows, and the arrow keys step it with the ring showing.",
            build: |window, cx| ContinuousSliders::view(window, cx),
        },
        Section {
            id: "slider-sizes",
            name: "Sliders, sizes",
            summary: "Both kinds at every rung. The box and each pill's hit box take the rung's height, the rail is 4 at xs and sm and 6 from md up, the knob grows with the rung, and stepped text is caption at xs and sm.",
            build: |window, cx| SliderSizes::view(window, cx),
        },
        Section {
            id: "slider-disabled",
            name: "Sliders, disabled",
            summary: "Rail, pills and knob at half strength, the stepped label and value faint. A press or a key changes nothing, and neither is in the focus order.",
            build: |window, cx| DisabledSliders::view(window, cx),
        },
    ],
};

const LANGUAGES: [&str; 5] = ["Rust", "Go", "Zig", "Swift", "OCaml"];

fn select(
    items: Vec<&'static str>,
    picked: Option<usize>,
    window: &mut Window,
    cx: &mut App,
) -> Entity<SelectState<Vec<&'static str>>> {
    cx.new(|cx| SelectState::new(items, picked.map(IndexPath::new), window, cx))
}

struct Selects {
    placeholder: Entity<SelectState<Vec<&'static str>>>,
    picked: Entity<SelectState<Vec<&'static str>>>,
    disabled: Entity<SelectState<Vec<&'static str>>>,
    sizes: Vec<(&'static str, Size, Entity<SelectState<Vec<&'static str>>>)>,
    wide: Entity<SelectState<Vec<&'static str>>>,
    empty: Entity<SelectState<Vec<&'static str>>>,
}

impl Selects {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            placeholder: select(LANGUAGES.to_vec(), None, window, cx),
            picked: select(LANGUAGES.to_vec(), Some(0), window, cx),
            disabled: select(LANGUAGES.to_vec(), Some(2), window, cx),
            sizes: [
                ("xsmall", Size::XSmall),
                ("small", Size::Small),
                ("large", Size::Large),
                ("xlarge", Size::XLarge),
                ("touch", Size::Touch),
            ]
            .into_iter()
            .map(|(name, size)| (name, size, select(LANGUAGES.to_vec(), Some(1), window, cx)))
            .collect(),
            wide: select(LANGUAGES.to_vec(), Some(4), window, cx),
            empty: select(Vec::new(), None, window, cx),
        }
    }
}

impl Render for Selects {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let states = states()
            .columns(3)
            .state(
                "placeholder",
                Select::new(&self.placeholder).placeholder("Language"),
            )
            .state("picked", Select::new(&self.picked))
            .state("disabled", Select::new(&self.disabled).disabled(true));
        self.sizes
            .iter()
            .fold(states, |states, (name, size, state)| {
                states.state(*name, Select::new(state).with_size(*size))
            })
            .state("fixed width", Select::new(&self.wide).w(px(200.0)))
            .state(
                "empty delegate",
                Select::new(&self.empty).placeholder("No languages"),
            )
    }
}

struct OpenSelect {
    state: Entity<SelectState<Vec<&'static str>>>,
}

impl OpenSelect {
    fn view(window: &mut Window, cx: &mut App) -> AnyView {
        let state = select(LANGUAGES.to_vec(), Some(0), window, cx);
        let target = state.clone();
        after_first_frame(window, move |window, cx| {
            let handle = ala::Focusable::focus_handle(target.read(cx), cx);
            handle.focus(window, cx);
            dispatch("penna_select::Confirm", window, cx);
        });
        cx.new(|_| Self { state }).into()
    }
}

impl Render for OpenSelect {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().h(px(300.0)).child(states().state(
            "open",
            h_flex().child(Select::new(&self.state).w(px(200.0))),
        ))
    }
}

struct Segmented {
    picked: [usize; 2],
}

impl Segmented {
    fn view(_: &mut Window, cx: &mut App) -> AnyView {
        cx.new(|_| Self { picked: [0, 1] }).into()
    }
}

impl Render for Segmented {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let control = |id: &'static str, segments: &[&'static str], at: usize| {
            let pick = cx.listener(move |this, index: &usize, _, cx| {
                this.picked[at] = *index;
                cx.notify();
            });
            row().child(
                SegmentedControl::new(id, labels(segments))
                    .selected(self.picked[at])
                    .on_select(move |index, window, cx| pick(&index, window, cx)),
            )
        };
        states()
            .state(
                "three, press one to slide",
                control("segmented-modes", &["System", "Light", "Dark"], 0),
            )
            .state("two", control("segmented-two", &["Tiled", "Floating"], 1))
    }
}

struct FocusedSegmented {
    picked: usize,
}

impl FocusedSegmented {
    fn view(window: &mut Window, cx: &mut App) -> AnyView {
        after_first_frame(window, Window::focus_next);
        cx.new(|_| Self { picked: 1 }).into()
    }
}

impl Render for FocusedSegmented {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let pick = cx.listener(|this, index: &usize, _, cx| {
            this.picked = *index;
            cx.notify();
        });
        states().state(
            "focused",
            row().child(
                SegmentedControl::new("segmented-focused", labels(&["System", "Light", "Dark"]))
                    .selected(self.picked)
                    .on_select(move |index, window, cx| pick(&index, window, cx)),
            ),
        )
    }
}

const SIZES: [(&str, Size); 6] = [
    ("xsmall", Size::XSmall),
    ("small", Size::Small),
    ("medium", Size::Medium),
    ("large", Size::Large),
    ("xlarge", Size::XLarge),
    ("touch", Size::Touch),
];

struct SegmentedSizes {
    picked: [usize; 6],
}

impl SegmentedSizes {
    fn view(_: &mut Window, cx: &mut App) -> AnyView {
        cx.new(|_| Self { picked: [1; 6] }).into()
    }
}

impl Render for SegmentedSizes {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        SIZES
            .into_iter()
            .enumerate()
            .fold(states().columns(3), |states, (at, (name, size))| {
                let pick = cx.listener(move |this, index: &usize, _, cx| {
                    this.picked[at] = *index;
                    cx.notify();
                });
                states.state(
                    name,
                    row().child(
                        SegmentedControl::new(
                            SharedString::from(format!("segmented-{name}")),
                            labels(&["System", "Light", "Dark"]),
                        )
                        .selected(self.picked[at])
                        .with_size(size)
                        .on_select(move |index, window, cx| pick(&index, window, cx)),
                    ),
                )
            })
    }
}

struct DisabledSegmented {
    picked: [usize; 2],
}

impl DisabledSegmented {
    fn view(_: &mut Window, cx: &mut App) -> AnyView {
        cx.new(|_| Self { picked: [1, 0] }).into()
    }
}

impl Render for DisabledSegmented {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let control = |id: &'static str, segments: &[&'static str], at: usize| {
            let pick = cx.listener(move |this, index: &usize, _, cx| {
                this.picked[at] = *index;
                cx.notify();
            });
            row().child(
                SegmentedControl::new(id, labels(segments))
                    .selected(self.picked[at])
                    .disabled(true)
                    .on_select(move |index, window, cx| pick(&index, window, cx)),
            )
        };
        states()
            .state(
                "disabled",
                control("segmented-disabled", &["System", "Light", "Dark"], 0),
            )
            .state(
                "disabled, first picked",
                control("segmented-disabled-first", &["Tiled", "Floating"], 1),
            )
    }
}

struct Switches {
    on: [bool; 12],
}

impl Switches {
    fn view(_: &mut Window, cx: &mut App) -> AnyView {
        cx.new(|_| Self {
            on: std::array::from_fn(|at| at % 2 == 1),
        })
        .into()
    }
}

impl Render for Switches {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let switch = |id: &'static str, at: usize| {
            Switch::new(id).checked(self.on[at]).on_click(cx.listener(
                move |this, on: &bool, _, cx| {
                    this.on[at] = *on;
                    cx.notify();
                },
            ))
        };
        states()
            .columns(3)
            .state(
                "off, on",
                row()
                    .child(switch("switch-off", 0))
                    .child(switch("switch-on", 1)),
            )
            .state(
                "small",
                row()
                    .child(switch("switch-small-off", 2).small())
                    .child(switch("switch-small-on", 3).small()),
            )
            .state(
                "disabled",
                row()
                    .child(Switch::new("switch-disabled-off").disabled(true))
                    .child(
                        Switch::new("switch-disabled-on")
                            .checked(true)
                            .disabled(true),
                    ),
            )
            .state(
                "xsmall",
                row()
                    .child(switch("switch-xsmall-off", 4).xsmall())
                    .child(switch("switch-xsmall-on", 5).xsmall()),
            )
            .state(
                "large",
                row()
                    .child(switch("switch-large-off", 6).large())
                    .child(switch("switch-large-on", 7).large()),
            )
            .state(
                "xlarge",
                row()
                    .child(switch("switch-xlarge-off", 8).xlarge())
                    .child(switch("switch-xlarge-on", 9).xlarge()),
            )
            .state(
                "touch",
                row()
                    .child(switch("switch-touch-off", 10).touch())
                    .child(switch("switch-touch-on", 11).touch()),
            )
    }
}

struct FocusedSwitch {
    on: [bool; 2],
}

impl FocusedSwitch {
    fn view(window: &mut Window, cx: &mut App) -> AnyView {
        after_first_frame(window, Window::focus_next);
        cx.new(|_| Self { on: [false; 2] }).into()
    }
}

impl Render for FocusedSwitch {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let switch = |id: &'static str, at: usize| {
            Switch::new(id).checked(self.on[at]).on_click(cx.listener(
                move |this, on: &bool, _, cx| {
                    this.on[at] = *on;
                    cx.notify();
                },
            ))
        };
        states().state(
            "focused, resting",
            row()
                .child(switch("switch-focused", 0))
                .child(switch("switch-resting", 1)),
        )
    }
}

#[derive(Clone, Copy)]
enum Gesture {
    Hover,
    Press,
}

struct PointedSwitch {
    gesture: Gesture,
    on: [bool; 2],
    target: Probe,
    _bounds: Subscription,
}

impl PointedSwitch {
    fn view(gesture: Gesture, window: &mut Window, cx: &mut App) -> AnyView {
        let target = Probe::default();
        let probe = target.clone();
        when_visible(&target, window, move |window, cx| match gesture {
            Gesture::Hover => hover(&probe, window, cx),
            Gesture::Press => press(&probe, MouseButton::Left, window, cx),
        });
        cx.new(|cx| Self {
            gesture,
            on: [matches!(gesture, Gesture::Press); 2],
            _bounds: cx.observe_window_bounds(window, |this: &mut Self, window, cx| {
                hover(&this.target, window, cx);
            }),
            target,
        })
        .into()
    }
}

impl Render for PointedSwitch {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (caption, target, rest) = match self.gesture {
            Gesture::Hover => (
                "hovered, resting",
                "switch-hover-target",
                "switch-hover-rest",
            ),
            Gesture::Press => (
                "pressed, resting",
                "switch-press-target",
                "switch-press-rest",
            ),
        };
        let switch = |id: &'static str, at: usize| {
            Switch::new(id).checked(self.on[at]).on_click(cx.listener(
                move |this, on: &bool, _, cx| {
                    this.on[at] = *on;
                    cx.notify();
                },
            ))
        };
        states().state(
            caption,
            row()
                .child(
                    div()
                        .relative()
                        .child(switch(target, 0))
                        .child(self.target.measure()),
                )
                .child(switch(rest, 1)),
        )
    }
}

fn labels(names: &[&'static str]) -> Vec<SharedString> {
    names.iter().copied().map(SharedString::from).collect()
}

const SCROLLBACK: [&str; 5] = ["1k", "5k", "10k", "50k", "100k"];
const OPACITY: [&str; 11] = [
    "0%", "10%", "20%", "30%", "40%", "50%", "60%", "70%", "80%", "90%", "100%",
];

struct Sliders {
    picked: [usize; 4],
}

impl Sliders {
    fn view(_: &mut Window, cx: &mut App) -> AnyView {
        cx.new(|_| Self {
            picked: [0, 2, 4, 7],
        })
        .into()
    }
}

impl Render for Sliders {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let slider = |id: &'static str, label: &'static str, steps: &[&'static str], at: usize| {
            let pick = cx.listener(move |this, index: &usize, _, cx| {
                this.picked[at] = *index;
                cx.notify();
            });
            DiscreteSlider::new(
                id,
                label,
                labels(steps),
                self.picked[at],
                move |index, window, cx| {
                    pick(&index, window, cx);
                },
            )
        };
        states()
            .columns(2)
            .state(
                "first step",
                slider("slider-first", "Scrollback", &SCROLLBACK, 0),
            )
            .state(
                "middle step",
                slider("slider-middle", "Scrollback", &SCROLLBACK, 1),
            )
            .state(
                "last step",
                slider("slider-last", "Scrollback", &SCROLLBACK, 2),
            )
            .state(
                "eleven steps",
                slider("slider-many", "Opacity", &OPACITY, 3),
            )
    }
}

struct FocusedSlider {
    picked: usize,
}

impl FocusedSlider {
    fn view(window: &mut Window, cx: &mut App) -> AnyView {
        after_first_frame(window, tab);
        cx.new(|_| Self { picked: 1 }).into()
    }
}

impl Render for FocusedSlider {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let pick = cx.listener(|this, index: &usize, _, cx| {
            this.picked = *index;
            cx.notify();
        });
        states().state(
            "focused",
            div().w(px(360.0)).child(DiscreteSlider::new(
                "slider-focused",
                "Scrollback",
                labels(&SCROLLBACK),
                self.picked,
                move |index, window, cx| pick(&index, window, cx),
            )),
        )
    }
}

struct ContinuousSliders {
    values: [f32; 2],
}

impl ContinuousSliders {
    fn view(_: &mut Window, cx: &mut App) -> AnyView {
        cx.new(|_| Self {
            values: [40.0, 0.0],
        })
        .into()
    }
}

impl Render for ContinuousSliders {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let slider = |id: &'static str, label: &'static str, step: f32, at: usize| {
            let set = cx.listener(move |this, value: &f32, _, cx| {
                this.values[at] = *value;
                cx.notify();
            });
            Slider::new(
                id,
                label,
                self.values[at],
                0.0..=100.0,
                move |value, window, cx| set(&value, window, cx),
            )
            .step(step)
        };
        states()
            .columns(2)
            .state(
                "press, drag or use the keys",
                slider("slider-volume", "Volume", 1.0, 0),
            )
            .state(
                "steps of 25",
                slider("slider-quarters", "Quarters", 25.0, 1),
            )
    }
}

struct SliderSizes {
    values: [f32; 6],
    picked: [usize; 6],
}

impl SliderSizes {
    fn view(_: &mut Window, cx: &mut App) -> AnyView {
        cx.new(|_| Self {
            values: [40.0; 6],
            picked: [2; 6],
        })
        .into()
    }
}

impl Render for SliderSizes {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        SIZES
            .into_iter()
            .enumerate()
            .fold(states().columns(2), |states, (at, (name, size))| {
                let set = cx.listener(move |this, value: &f32, _, cx| {
                    this.values[at] = *value;
                    cx.notify();
                });
                let pick = cx.listener(move |this, index: &usize, _, cx| {
                    this.picked[at] = *index;
                    cx.notify();
                });
                states
                    .state(
                        name,
                        Slider::new(
                            SharedString::from(format!("slider-{name}")),
                            "Volume",
                            self.values[at],
                            0.0..=100.0,
                            move |value, window, cx| set(&value, window, cx),
                        )
                        .step(1.0)
                        .with_size(size),
                    )
                    .state(
                        format!("{name}, stepped"),
                        DiscreteSlider::new(
                            SharedString::from(format!("steps-{name}")),
                            "Scrollback",
                            labels(&SCROLLBACK),
                            self.picked[at],
                            move |index, window, cx| pick(&index, window, cx),
                        )
                        .with_size(size),
                    )
            })
    }
}

struct DisabledSliders {
    value: f32,
    picked: usize,
}

impl DisabledSliders {
    fn view(_: &mut Window, cx: &mut App) -> AnyView {
        cx.new(|_| Self {
            value: 40.0,
            picked: 2,
        })
        .into()
    }
}

impl Render for DisabledSliders {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let set = cx.listener(|this, value: &f32, _, cx| {
            this.value = *value;
            cx.notify();
        });
        let pick = cx.listener(|this, index: &usize, _, cx| {
            this.picked = *index;
            cx.notify();
        });
        states()
            .columns(2)
            .state(
                "continuous",
                Slider::new(
                    "slider-disabled",
                    "Volume",
                    self.value,
                    0.0..=100.0,
                    move |value, window, cx| set(&value, window, cx),
                )
                .step(1.0)
                .disabled(true),
            )
            .state(
                "stepped",
                DiscreteSlider::new(
                    "steps-disabled",
                    "Scrollback",
                    labels(&SCROLLBACK),
                    self.picked,
                    move |index, window, cx| pick(&index, window, cx),
                )
                .disabled(true),
            )
    }
}
