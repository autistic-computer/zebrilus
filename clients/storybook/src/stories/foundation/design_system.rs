use ala::{
    AnyElement, App, AppContext as _, Context, Entity, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, SharedString, Styled as _, Window, div,
    prelude::FluentBuilder as _, px,
};
use penna::{
    ActiveTheme as _, Colorize as _, Disableable as _, Elevation, Emphasis, Fill, IconName,
    InteractivePaintExt as _, Kind, Paint, PaintExt as _, PaintState, Sizable as _, StylePatch,
    StyledExt as _, Tone,
    badge::Badge,
    button::Button,
    check::Check,
    h_flex,
    input::{Input, InputState, NumberInput},
    kbd::Keycap,
    select::{Select, SelectState},
    slider::DiscreteSlider,
    switch::Switch,
    v_flex,
};

use crate::story::Framed as _;
use crate::story::{Section, Story, row, stateless, states};

pub const STORY: Story = Story {
    id: "design-system",
    name: "Design system",
    group: "Foundation",
    summary: "Every filled, tinted or outlined widget picks a tone (what it means) and an emphasis (how loud it is). The theme turns the pair into one Paint, so a badge, a button and a tile that mean the same thing are drawn alike. Which fill, edge and shadow each piece takes comes from its role; try the Roles step of the theme creator.",
    sections: &[
        Section {
            id: "tones",
            name: "Tones",
            summary: "Neutral, primary, danger, warning and success. Primary is the foreground or the accent, as the theme's materials say.",
            build: |_, cx| stateless(tones, cx),
        },
        Section {
            id: "paint",
            name: "Paint",
            summary: "Every tone at every emphasis, in each state. Hover and pressed are drawn still here so they can be compared.",
            build: |_, cx| stateless(paint, cx),
        },
        Section {
            id: "buttons",
            name: "Buttons",
            summary: "Button::tone and Button::emphasis; the shorthands primary, secondary, ghost and danger name the common pairs. These hover and press for real.",
            build: |_, cx| stateless(buttons, cx),
        },
        Section {
            id: "badges",
            name: "Badges",
            summary: "One Badge for chips, pills, status and counts.",
            build: |_, cx| stateless(badges, cx),
        },
        Section {
            id: "keycaps",
            name: "Keycaps",
            summary: "A key as a cap. On a solid fill it takes the ink that reads on that fill.",
            build: |_, cx| stateless(keycaps, cx),
        },
        Section {
            id: "checks",
            name: "Checks",
            summary: "Checkbox and radio: a neutral outline when off, the tone solid when on.",
            build: |_, cx| stateless(checks, cx),
        },
        Section {
            id: "tiles-and-rows",
            name: "Tiles and rows",
            summary: "Selectable surfaces. Tiles are neutral soft, rows are neutral ghost; both select with their role's selected highlight.",
            build: |_, cx| stateless(tiles_and_rows, cx),
        },
        Section {
            id: "fields",
            name: "Fields",
            summary: "Text, number and select fields draw as the theme's field role says, ghost when they should show their edge only on hover or focus, danger when invalid. Focus turns the border to the primary tone. A field inside a tile sits on the tile's ground.",
            build: |window, cx| cx.new(|cx| Fields::new(window, cx)).into(),
        },
        Section {
            id: "text",
            name: "Text",
            summary: "Default, muted, faint, and the ink on a solid fill.",
            build: |_, cx| stateless(text, cx),
        },
        Section {
            id: "grounds",
            name: "Grounds",
            summary: "The same paints on the window, a raised panel and a popover. Soft fills tint whatever is under them, so one recipe serves all three.",
            build: |_, cx| stateless(grounds, cx),
        },
    ],
};

fn caption(text: impl Into<SharedString>, cx: &App) -> impl IntoElement {
    div()
        .text_size(px(11.0))
        .line_height(px(14.0))
        .font_family(cx.theme().mono_font_family.clone())
        .text_color(cx.theme().foreground.muted())
        .child(text.into())
}

fn matrix(columns: &[&str], rows: Vec<(SharedString, Vec<AnyElement>)>, cx: &App) -> AnyElement {
    let count = u16::try_from(columns.len() + 1).unwrap_or(1);
    div()
        .grid()
        .grid_cols(count)
        .gap_x_4()
        .gap_y_3()
        .items_center()
        .child(div())
        .children(
            columns.iter().map(|column| {
                caption(SharedString::from((*column).to_owned()), cx).into_any_element()
            }),
        )
        .children(rows.into_iter().flat_map(|(name, cells)| {
            std::iter::once(caption(name, cx).into_any_element()).chain(cells)
        }))
        .into_any_element()
}

fn sample(label: &'static str, paint: Paint, state: PaintState, cx: &App) -> AnyElement {
    h_flex()
        .h(px(28.0))
        .px_3()
        .justify_center()
        .rounded(cx.theme().radius)
        .painted(paint, state)
        .text_size(px(13.0))
        .child(label)
        .into_any_element()
}

fn title(tone: Tone) -> String {
    let name = tone.name();
    name[..1].to_uppercase() + &name[1..]
}

fn tones(_: &mut Window, cx: &mut App) -> AnyElement {
    let theme = cx.theme();
    h_flex()
        .flex_wrap()
        .gap_3()
        .children(Tone::ALL.map(|tone| {
            v_flex()
                .w(px(104.0))
                .gap_1p5()
                .child(
                    div()
                        .h(px(44.0))
                        .rounded(theme.radius)
                        .framed(cx)
                        .bg(theme.tone(tone)),
                )
                .child(div().text_size(px(12.0)).child(tone.name()))
        }))
        .into_any_element()
}

fn paint(_: &mut Window, cx: &mut App) -> AnyElement {
    let theme = cx.theme();
    let columns = PaintState::ALL.map(PaintState::name);
    Emphasis::ALL
        .iter()
        .fold(states(), |block, emphasis| {
            let rows = Tone::ALL
                .iter()
                .map(|tone| {
                    let paint = theme.paint(*tone, *emphasis);
                    (
                        SharedString::from(tone.name()),
                        PaintState::ALL
                            .iter()
                            .map(|state| sample("Label", paint, *state, cx))
                            .collect(),
                    )
                })
                .collect();
            block.state(emphasis.name(), matrix(&columns, rows, cx))
        })
        .into_any_element()
}

fn buttons(_: &mut Window, cx: &mut App) -> AnyElement {
    let columns = Emphasis::ALL.map(Emphasis::name);
    let grid = |disabled: bool| {
        let rows = Tone::ALL
            .iter()
            .map(|tone| {
                (
                    SharedString::from(tone.name()),
                    Emphasis::ALL
                        .iter()
                        .map(|emphasis| {
                            Button::new(SharedString::from(format!(
                                "ds-button-{}-{}-{disabled}",
                                tone.name(),
                                emphasis.name()
                            )))
                            .label(title(*tone))
                            .tone(*tone)
                            .emphasis(*emphasis)
                            .disabled(disabled)
                            .into_any_element()
                        })
                        .collect(),
                )
            })
            .collect();
        matrix(&columns, rows, cx)
    };
    states()
        .state("enabled", grid(false))
        .state("disabled", grid(true))
        .into_any_element()
}

fn badges(_: &mut Window, cx: &mut App) -> AnyElement {
    let columns = Emphasis::ALL.map(Emphasis::name);
    let rows = Tone::ALL
        .iter()
        .map(|tone| {
            (
                SharedString::from(tone.name()),
                Emphasis::ALL
                    .iter()
                    .map(|emphasis| {
                        h_flex()
                            .child(Badge::new(title(*tone)).tone(*tone).emphasis(*emphasis))
                            .into_any_element()
                    })
                    .collect(),
            )
        })
        .collect();
    states()
        .state("tone × emphasis", matrix(&columns, rows, cx))
        .state(
            "sizes",
            row()
                .child(Badge::new("small").small())
                .child(Badge::new("medium")),
        )
        .state(
            "in use",
            row()
                .child(Badge::new("2/3"))
                .child(Badge::new("Base").emphasis(Emphasis::Outline))
                .child(
                    Badge::new("3 failing")
                        .icon(IconName::CircleX)
                        .tone(Tone::Danger),
                )
                .child(Badge::new("Waiting").tone(Tone::Warning))
                .child(
                    Badge::new("Ready")
                        .tone(Tone::Success)
                        .emphasis(Emphasis::Solid),
                )
                .child(
                    Badge::new("New")
                        .tone(Tone::Primary)
                        .emphasis(Emphasis::Solid),
                ),
        )
        .into_any_element()
}

fn keycaps(_: &mut Window, cx: &mut App) -> AnyElement {
    let theme = cx.theme();
    let inside = |id: &'static str, tone: Tone| {
        let ground = theme
            .paint(tone, Emphasis::Solid)
            .ground_at(PaintState::Rest);
        Button::new(id)
            .label("Submit")
            .tone(tone)
            .emphasis(Emphasis::Solid)
            .child(Keycap::new("↵").on(ground))
    };
    states()
        .state(
            "on the window",
            row()
                .child(Keycap::new("1"))
                .child(Keycap::new("↵"))
                .child(Keycap::new("⌘K")),
        )
        .state(
            "inside a solid button",
            row()
                .child(inside("ds-keycap-primary", Tone::Primary))
                .child(inside("ds-keycap-danger", Tone::Danger))
                .child(inside("ds-keycap-success", Tone::Success))
                .child(
                    Button::new("ds-keycap-soft")
                        .label("Skip")
                        .emphasis(Emphasis::Ghost)
                        .child(Keycap::new("esc")),
                ),
        )
        .state(
            "sizes",
            row()
                .child(Keycap::new("1").small())
                .child(Keycap::new("1")),
        )
        .into_any_element()
}

fn checks(_: &mut Window, cx: &mut App) -> AnyElement {
    let columns = ["off", "on", "on, danger", "off, disabled", "on, disabled"];
    let marks = |make: fn(bool) -> Check| {
        vec![
            make(false).into_any_element(),
            make(true).into_any_element(),
            make(true).tone(Tone::Danger).into_any_element(),
            make(false).disabled(true).into_any_element(),
            make(true).disabled(true).into_any_element(),
        ]
    };
    states()
        .state(
            "kinds",
            matrix(
                &columns,
                vec![
                    ("checkbox".into(), marks(Check::checkbox)),
                    ("radio".into(), marks(Check::radio)),
                ],
                cx,
            ),
        )
        .state(
            "sizes",
            row()
                .child(Check::checkbox(true).small())
                .child(Check::checkbox(true))
                .child(Check::radio(true).small())
                .child(Check::radio(true)),
        )
        .into_any_element()
}

fn tiles_and_rows(_: &mut Window, cx: &mut App) -> AnyElement {
    let theme = cx.theme();
    let tile_paint = theme.paint(Tone::Neutral, Emphasis::Soft);
    let row_paint = theme.paint(Tone::Neutral, Emphasis::Ghost);
    let tile =
        |index: usize, label: &'static str, detail: &'static str, selected: bool, enabled: bool| {
            let state = match (enabled, selected) {
                (false, _) => PaintState::Disabled,
                (true, true) => PaintState::Selected,
                (true, false) => PaintState::Rest,
            };
            let ground = tile_paint.ground_at(state);
            h_flex()
                .id(("ds-tile", index))
                .w_full()
                .items_start()
                .gap_2()
                .px_3()
                .py(px(9.0))
                .rounded(px(8.0))
                .painted_interactive(tile_paint, selected, enabled)
                .text_size(px(13.0))
                .line_height(px(18.0))
                .child(
                    v_flex().flex_1().min_w_0().child(label).child(
                        div()
                            .text_size(px(12.0))
                            .line_height(px(16.0))
                            .text_color(tile_paint.muted_at(state))
                            .child(detail),
                    ),
                )
                .child(if selected {
                    Check::radio(true).on(ground).into_any_element()
                } else {
                    Keycap::new(format!("{}", index + 1))
                        .on(ground)
                        .into_any_element()
                })
        };
    let list_row = |index: usize, label: &'static str, selected: bool| {
        let state = if selected {
            PaintState::Selected
        } else {
            PaintState::Rest
        };
        h_flex()
            .id(("ds-row", index))
            .w_full()
            .h(px(28.0))
            .px_2()
            .gap_2()
            .rounded(theme.shape_radius(penna::SurfaceRole::MENU_ITEM, penna::Shape::Round))
            .painted_interactive(row_paint, selected, true)
            .text_size(px(13.0))
            .child(div().flex_1().child(label))
            .child(Keycap::new("⌘").on(row_paint.ground_at(state)))
    };
    h_flex()
        .items_start()
        .gap_8()
        .child(
            v_flex()
                .w(px(320.0))
                .gap(px(6.0))
                .child(caption("tiles: rest, selected, rest, disabled", cx))
                .child(tile(0, "main", "Ships with the next nightly", false, true))
                .child(tile(
                    1,
                    "release/0.16",
                    "Backport to the current release",
                    true,
                    true,
                ))
                .child(tile(2, "release/0.15", "Out of support", false, true))
                .child(tile(3, "Frozen", "Read-only", false, false)),
        )
        .child(
            v_flex()
                .w(px(240.0))
                .gap(px(2.0))
                .p_1()
                .surface(penna::SurfaceRole::POPOVER, cx)
                .child(div().px_2().py_1().child(caption("rows in a popover", cx)))
                .child(list_row(0, "New tab", false))
                .child(list_row(1, "Split right", true))
                .child(list_row(2, "Split down", false))
                .child(list_row(3, "Close pane", false)),
        )
        .into_any_element()
}

fn text_column(cx: &App) -> impl IntoElement {
    let foreground = cx.theme().foreground;
    let solid = cx.theme().paint(Tone::Primary, Emphasis::Solid);
    v_flex()
        .gap_1()
        .text_size(px(13.0))
        .child(div().text_color(foreground).child("Default text"))
        .child(
            div()
                .text_color(foreground.muted())
                .child("Muted: descriptions"),
        )
        .child(
            div()
                .text_color(foreground.faint())
                .child("Faint: placeholders, hints"),
        )
        .child(
            h_flex().child(
                div()
                    .px_2()
                    .rounded(px(4.0))
                    .painted(solid, PaintState::Rest)
                    .child("On a solid fill"),
            ),
        )
}

fn text(_: &mut Window, cx: &mut App) -> AnyElement {
    let raised = cx.theme().surface(Elevation::Panel).fill;
    h_flex()
        .items_start()
        .gap_6()
        .child(
            v_flex()
                .gap_2()
                .child(caption("window", cx))
                .child(text_column(cx)),
        )
        .child(
            v_flex()
                .gap_2()
                .p_3()
                .rounded(px(8.0))
                .bg(raised)
                .child(caption("raised", cx))
                .child(text_column(cx)),
        )
        .child(
            v_flex()
                .gap_2()
                .p_3()
                .surface(penna::SurfaceRole::POPOVER, cx)
                .child(caption("popover", cx))
                .child(text_column(cx)),
        )
        .into_any_element()
}

fn ground_samples(cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let samples = [
        ("soft", Tone::Neutral, Emphasis::Soft, PaintState::Rest),
        (
            "soft hover",
            Tone::Neutral,
            Emphasis::Soft,
            PaintState::Hover,
        ),
        (
            "selected",
            Tone::Neutral,
            Emphasis::Soft,
            PaintState::Selected,
        ),
        (
            "outline",
            Tone::Neutral,
            Emphasis::Outline,
            PaintState::Rest,
        ),
        (
            "ghost hover",
            Tone::Neutral,
            Emphasis::Ghost,
            PaintState::Hover,
        ),
        ("primary", Tone::Primary, Emphasis::Solid, PaintState::Rest),
        (
            "danger soft",
            Tone::Danger,
            Emphasis::Soft,
            PaintState::Rest,
        ),
    ];
    v_flex()
        .gap_2()
        .children(samples.map(|(label, tone, emphasis, state)| {
            sample(label, theme.paint(tone, emphasis), state, cx)
        }))
}

fn grounds(_: &mut Window, cx: &mut App) -> AnyElement {
    let raised = cx.theme().surface(Elevation::Panel).fill;
    h_flex()
        .items_start()
        .gap_6()
        .child(
            v_flex()
                .w(px(180.0))
                .gap_2()
                .child(caption("window", cx))
                .child(ground_samples(cx)),
        )
        .child(
            v_flex()
                .w(px(204.0))
                .gap_2()
                .p_3()
                .rounded(px(8.0))
                .bg(raised)
                .child(caption("raised", cx))
                .child(ground_samples(cx)),
        )
        .child(
            v_flex()
                .w(px(204.0))
                .gap_2()
                .p_3()
                .surface(penna::SurfaceRole::POPOVER, cx)
                .child(caption("popover", cx))
                .child(ground_samples(cx)),
        )
        .into_any_element()
}

const P: StylePatch = StylePatch::EMPTY;

const FIELD_ROWS: [(&str, Option<StylePatch>); 4] = [
    ("the theme's", None),
    ("soft", Some(P.fill(Fill::Tint).outline(true))),
    ("outline", Some(P.fill(Fill::None).outline(true))),
    ("ghost", Some(P.fill(Fill::None).outline(false))),
];

struct Fields {
    grid: Vec<[Entity<InputState>; 4]>,
    focused: Entity<InputState>,
    number: Entity<InputState>,
    select: Entity<SelectState<Vec<&'static str>>>,
    empty_select: Entity<SelectState<Vec<&'static str>>>,
    in_tile: Entity<InputState>,
    in_selected_tile: Entity<InputState>,
}

impl Fields {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut field = |value: &str| {
            let value = value.to_owned();
            cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("Placeholder")
                    .default_value(value)
            })
        };
        let grid = FIELD_ROWS
            .iter()
            .map(|_| {
                [
                    field(""),
                    field("main"),
                    field("not a branch name"),
                    field("main"),
                ]
            })
            .collect();
        let focused = field("release/0.16");
        let number = field("12");
        let in_tile = field("");
        let in_selected_tile = field("release/0.15");
        let languages = vec!["Rust", "Go", "Zig"];
        let select = cx.new(|cx| {
            SelectState::new(
                languages.clone(),
                Some(penna::IndexPath::new(0)),
                window,
                cx,
            )
        });
        let empty_select = cx.new(|cx| SelectState::new(languages, None, window, cx));
        let target = focused.clone();
        window.on_next_frame(move |window, _| {
            window.on_next_frame(move |window, cx| {
                target.update(cx, |state, cx| state.focus(window, cx));
            });
        });
        Self {
            grid,
            focused,
            number,
            select,
            empty_select,
            in_tile,
            in_selected_tile,
        }
    }
}

impl Render for Fields {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let columns = ["empty", "filled", "invalid", "disabled"];
        let rows = FIELD_ROWS
            .iter()
            .zip(&self.grid)
            .map(|((name, patch), states)| {
                let cell = |index: usize| {
                    Input::new(&states[index])
                        .when_some(*patch, |input, patch| {
                            input.surface_role(Kind::Field.role().with(patch))
                        })
                        .when(index == 2, |input| input.tone(Tone::Danger))
                        .disabled(index == 3)
                        .into_any_element()
                };
                (
                    SharedString::from(*name),
                    vec![cell(0), cell(1), cell(2), cell(3)],
                )
            })
            .collect();
        let tile_paint = cx.theme().paint(Tone::Neutral, Emphasis::Soft);
        let tile = |selected: bool, input: AnyElement| {
            let state = if selected {
                PaintState::Selected
            } else {
                PaintState::Rest
            };
            v_flex()
                .w(px(280.0))
                .gap_2()
                .px_3()
                .py(px(9.0))
                .rounded(px(8.0))
                .painted(tile_paint, state)
                .text_size(px(13.0))
                .child("Other")
                .child(input)
        };
        let selected_ground = tile_paint.ground_at(PaintState::Selected);
        states()
            .state("text fields", matrix(&columns, rows, cx))
            .state(
                "focused",
                h_flex().w(px(280.0)).child(Input::new(&self.focused)),
            )
            .state(
                "number, select",
                row()
                    .child(div().w(px(160.0)).child(NumberInput::new(&self.number)))
                    .child(Select::new(&self.select))
                    .child(Select::new(&self.empty_select).placeholder("Language"))
                    .child(Select::new(&self.select).disabled(true)),
            )
            .state(
                "switch, slider",
                row()
                    .child(Switch::new("ds-switch-off"))
                    .child(Switch::new("ds-switch-on").checked(true))
                    .child(
                        Switch::new("ds-switch-disabled")
                            .checked(true)
                            .disabled(true),
                    )
                    .child(
                        div().w(px(280.0)).child(DiscreteSlider::new(
                            "ds-slider",
                            "Scrollback",
                            ["1k", "10k", "50k", "100k"]
                                .map(SharedString::from)
                                .to_vec(),
                            1,
                            |_, _, _| {},
                        )),
                    ),
            )
            .state(
                "on a tile: rest, selected",
                h_flex()
                    .items_start()
                    .gap_4()
                    .child(tile(
                        false,
                        Input::new(&self.in_tile)
                            .on(tile_paint.ground_at(PaintState::Rest))
                            .into_any_element(),
                    ))
                    .child(tile(
                        true,
                        Input::new(&self.in_selected_tile)
                            .on(selected_ground)
                            .into_any_element(),
                    )),
            )
    }
}
