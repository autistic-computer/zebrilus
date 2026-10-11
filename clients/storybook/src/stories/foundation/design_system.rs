use zz_gpui::{
    AnyElement, App, InteractiveElement as _, IntoElement, ParentElement as _, SharedString,
    Styled as _, Window, div, px,
};
use zz_ui::{
    ActiveTheme as _, Colorize as _, Disableable as _, Emphasis, IconName,
    InteractivePaintExt as _, Paint, PaintExt as _, PaintState, Sizable as _, StyledExt as _, Tone,
    badge::Badge,
    button::{Button, ButtonVariants as _},
    check::Check,
    h_flex,
    kbd::Keycap,
    v_flex,
};

use crate::story::{Section, Story, row, stateless, states};

pub const STORY: Story = Story {
    id: "design-system",
    name: "Design system",
    group: "Foundation",
    summary: "Every filled, tinted or outlined widget picks a tone (what it means) and an emphasis (how loud it is). The theme turns the pair into one Paint, so a badge, a button and a tile that mean the same thing are drawn alike. Try the Primary and Selection knobs.",
    sections: &[
        Section {
            id: "tones",
            name: "Tones",
            summary: "Neutral, primary, danger, warning and success. Primary is the foreground or the accent, as the look says.",
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
            summary: "Button::tone and Button::emphasis. These hover and press for real; the last block is today's ButtonVariant for comparison.",
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
            summary: "Selectable surfaces. Tiles are neutral soft, rows are neutral ghost; both select with the look's selection fill.",
            build: |_, cx| stateless(tiles_and_rows, cx),
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
        .rounded(cx.theme().control_radius())
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
                        .border_1()
                        .border_color(theme.border())
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
                            .small()
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
    let today = row()
        .child(Button::new("ds-today-default").small().label("Default"))
        .child(
            Button::new("ds-today-primary")
                .small()
                .primary()
                .label("Primary"),
        )
        .child(
            Button::new("ds-today-accent")
                .small()
                .accent()
                .label("Accent"),
        )
        .child(
            Button::new("ds-today-secondary")
                .small()
                .secondary()
                .label("Secondary"),
        )
        .child(Button::new("ds-today-ghost").small().ghost().label("Ghost"))
        .child(
            Button::new("ds-today-danger")
                .small()
                .danger()
                .label("Danger"),
        )
        .child(
            Button::new("ds-today-warning")
                .small()
                .warning()
                .label("Warning"),
        )
        .child(
            Button::new("ds-today-success")
                .small()
                .success()
                .label("Success"),
        );
    states()
        .state("enabled", grid(false))
        .state("disabled", grid(true))
        .state("today's ButtonVariant", today)
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
                .child(Badge::new("xsmall").xsmall())
                .child(Badge::new("small").small())
                .child(Badge::new("medium"))
                .child(Badge::new("large").large()),
        )
        .state(
            "in use",
            row()
                .child(Badge::new("2/3").small())
                .child(Badge::new("Base").small().emphasis(Emphasis::Outline))
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
    let caps = |emphasis: Emphasis| {
        h_flex()
            .gap_1()
            .child(Keycap::new("1").emphasis(emphasis))
            .child(Keycap::new("↵").emphasis(emphasis))
            .child(Keycap::new("⌘K").emphasis(emphasis))
            .into_any_element()
    };
    let columns = Emphasis::ALL.map(Emphasis::name);
    let theme = cx.theme();
    let inside = |id: &'static str, tone: Tone| {
        let ground = theme
            .paint(tone, Emphasis::Solid)
            .ground_at(PaintState::Rest);
        Button::new(id)
            .small()
            .label("Submit")
            .tone(tone)
            .emphasis(Emphasis::Solid)
            .child(Keycap::new("↵").on(ground))
    };
    states()
        .state(
            "on the window",
            matrix(
                &columns,
                vec![("neutral".into(), Vec::from(Emphasis::ALL.map(caps)))],
                cx,
            ),
        )
        .state(
            "inside a solid button",
            row()
                .child(inside("ds-keycap-primary", Tone::Primary))
                .child(inside("ds-keycap-danger", Tone::Danger))
                .child(inside("ds-keycap-success", Tone::Success))
                .child(
                    Button::new("ds-keycap-soft")
                        .small()
                        .label("Skip")
                        .emphasis(Emphasis::Ghost)
                        .child(Keycap::new("esc")),
                ),
        )
        .state(
            "sizes",
            row()
                .child(Keycap::new("1").small())
                .child(Keycap::new("1"))
                .child(Keycap::new("1").large()),
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
                .child(Check::checkbox(true).large())
                .child(Check::radio(true).small())
                .child(Check::radio(true))
                .child(Check::radio(true).large()),
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
            .rounded(theme.menu_radius())
            .painted_interactive(row_paint, selected, true)
            .text_size(px(13.0))
            .child(div().flex_1().child(label))
            .child(
                Keycap::new("⌘")
                    .emphasis(Emphasis::Ghost)
                    .on(row_paint.ground_at(state)),
            )
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
                .popover_style(cx)
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
    let raised = cx.theme().background.raised(1);
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
                .popover_style(cx)
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
    let raised = cx.theme().background.raised(1);
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
                .popover_style(cx)
                .child(caption("popover", cx))
                .child(ground_samples(cx)),
        )
        .into_any_element()
}
