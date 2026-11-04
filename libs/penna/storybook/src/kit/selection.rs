use ala::{
    AnyView, App, AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render,
    SharedString, Styled as _, Subscription, Window, div, px,
};
use penna::{
    Disableable as _, IndexPath, Sizable as _, Size,
    components::segmented::SegmentedControl,
    components::select::{Select, SelectState},
    components::slider::DiscreteSlider,
    components::switch::Switch,
    h_flex,
};

use super::support::{Probe, after_first_frame, click, dispatch, when_visible};
use crate::story::{Section, Story, row, states};

pub const STORY: Story = Story {
    id: "choices",
    name: "Choices",
    group: "Kit",
    summary: "Controls that pick a value: the dropdown Select, the SegmentedControl, the Switch and the DiscreteSlider.",
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
            id: "switch",
            name: "Switch",
            summary: "Off, on, small and disabled, then every other size on the ladder. A press flips the enabled ones; the thumb slides between states and holds still with motion off.",
            build: |window, cx| Switches::view(window, cx),
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
            summary: "A press on the slider focuses it; the picked pill then carries a border in the focus color and the arrow keys step it.",
            build: |window, cx| FocusedSlider::view(window, cx),
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
    label: Probe,
    _bounds: Subscription,
}

impl FocusedSlider {
    fn view(window: &mut Window, cx: &mut App) -> AnyView {
        let label = Probe::default();
        let probe = label.clone();
        when_visible(&label, window, move |window, cx| click(&probe, window, cx));
        cx.new(|cx| Self {
            picked: 1,
            label,
            _bounds: cx.observe_window_bounds(window, |this: &mut Self, window, cx| {
                click(&this.label, window, cx);
            }),
        })
        .into()
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
            div()
                .relative()
                .w(px(360.0))
                .child(DiscreteSlider::new(
                    "slider-focused",
                    "Scrollback",
                    labels(&SCROLLBACK),
                    self.picked,
                    move |index, window, cx| pick(&index, window, cx),
                ))
                .child(
                    div()
                        .absolute()
                        .top_0()
                        .left_0()
                        .w(px(24.0))
                        .h_full()
                        .child(self.label.measure()),
                ),
        )
    }
}
