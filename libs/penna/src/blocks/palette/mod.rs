mod vim;

pub use vim::{PaletteVim, PaletteVimMode, Twin, VimCommand};

use std::ops::Range;

use ala::{
    AnyElement, App, ClickEvent, Div, ElementId, Entity, FontWeight, HighlightStyle, Hsla,
    IntoElement, Keystroke, MouseButton, ParentElement as _, Pixels, RenderOnce, SharedString,
    StyledText, Window, deferred, div, prelude::*, px, relative,
};

use crate::{
    ActiveTheme as _, Colorize as _, Edge, Emphasis, Ground, Icon, IconName, Label, PaintState,
    Panel, PanelLook, RowLook, RuleLook, Sizable as _, StyledExt as _, TextStyle, Tone,
    blocks::sidebar::status_dot,
    components::badge::Badge,
    components::input::{Input, InputState},
    components::kbd::{Kbd, KeyGlyphs, Keycap},
    components::list::ListItem,
    h_flex, muted_on,
};

pub const PALETTE_MAX_WIDTH: f32 = 560.0;
pub const PALETTE_INSET: f32 = 8.0;
pub const PALETTE_ROW_HEIGHT: f32 = 28.0;
pub const PALETTE_INPUT_HEIGHT: f32 = 40.0;
pub const PALETTE_FOOTER_HEIGHT: f32 = 32.0;
const PALETTE_LIST_MAX: f32 = 440.0;

#[must_use]
pub fn palette_list_height(viewport: Pixels, usage: bool) -> f32 {
    (f32::from(viewport) * 0.88 - if usage { 175.0 } else { 140.0 }).clamp(40.0, PALETTE_LIST_MAX)
}

#[must_use]
pub fn palette_row_height() -> f32 {
    px(PALETTE_ROW_HEIGHT).as_f32()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PaletteStatus {
    Online,
    Running,
    Waiting,
    Offline,
}

impl PaletteStatus {
    #[must_use]
    pub fn color(self, cx: &App) -> Hsla {
        match self {
            Self::Online => cx.theme().success,
            Self::Running => cx.theme().foreground.muted(),
            Self::Waiting => cx.theme().warning,
            Self::Offline => cx.theme().foreground.faint(),
        }
    }
}

pub enum PalettePill {
    Mode {
        prefix: SharedString,
        label: SharedString,
    },
    Status {
        label: SharedString,
        status: PaletteStatus,
    },
    Code(SharedString),
    Crumb(SharedString),
    Action(SharedString),
}

#[derive(Clone, Debug)]
pub enum PaletteShortcut {
    Keys(Vec<Keystroke>),
    Text(SharedString),
}

#[derive(Clone, Debug, Default)]
pub struct PaletteRow {
    pub label: SharedString,
    pub detail: SharedString,
    pub right: SharedString,
    pub matches: Vec<Range<usize>>,
    pub muted_prefix: usize,
    pub status: Option<PaletteStatus>,
    pub running: bool,
    pub indent: f32,
    pub shortcut: Option<PaletteShortcut>,
    pub icon: Option<IconName>,
    pub expanded: Option<bool>,
}

#[derive(Clone, Copy)]
pub struct PaletteHint {
    pub key: &'static str,
    pub label: &'static str,
}

fn palette_status_dot(status: PaletteStatus, cx: &App) -> Div {
    status_dot(status.color(cx))
}

#[allow(
    clippy::disallowed_methods,
    reason = "a ring in the row's ink keeps a status dot legible on the selection; it is the dot's color, not an edge"
)]
fn ringed(dot: Div, ring: Hsla, cx: &App) -> Div {
    let dot = dot.border_color(ring);
    if cx.theme().outlined() {
        dot.border(cx.theme().outline)
    } else {
        dot
    }
}

fn pill(pill: PalettePill, cx: &App) -> Badge {
    let tag = Badge::empty().flex_none().gap(px(6.0)).px(px(8.0));
    match pill {
        PalettePill::Mode { prefix, label } => tag
            .child(
                div()
                    .font_family(cx.theme().mono_font_family.clone())
                    .text_color(cx.theme().paint(Tone::Primary, Emphasis::Ghost).text)
                    .child(prefix),
            )
            .child(label),
        PalettePill::Status { label, status } => {
            tag.child(palette_status_dot(status, cx)).child(label)
        }
        PalettePill::Code(label) => tag
            .font_family(cx.theme().mono_font_family.clone())
            .child(label),
        PalettePill::Crumb(label) => tag
            .flex_shrink(u16::try_from(label.chars().count()).map_or(f32::MAX, f32::from))
            .min_w(px(28.0))
            .child(div().min_w_0().truncate().child(label)),
        PalettePill::Action(label) => tag
            .text_color(cx.theme().paint(Tone::Primary, Emphasis::Ghost).text)
            .child(label),
    }
}

#[must_use]
pub fn palette_vim_badge(mode: PaletteVimMode, pending: Option<char>, cx: &App) -> Badge {
    let label: SharedString = match pending {
        Some(key) => format!("{} {key}", mode.label()).into(),
        None => mode.label().into(),
    };
    let badge = Badge::new(label)
        .flex_none()
        .font_family(cx.theme().mono_font_family.clone());
    match mode {
        PaletteVimMode::Normal => badge.tone(Tone::Primary).emphasis(Emphasis::Solid),
        PaletteVimMode::Insert => badge,
    }
}

#[must_use]
pub fn palette_kind_badge(
    label: impl Into<SharedString>,
    font_family: impl Into<SharedString>,
) -> Badge {
    Badge::new(label).small().font_family(font_family.into())
}

pub fn palette_input(
    input: &Entity<InputState>,
    pills: impl IntoIterator<Item = PalettePill>,
    cx: &App,
) -> Input {
    let pills: Vec<_> = pills.into_iter().collect();
    Input::new(input)
        .appearance(false)
        .w_full()
        .h(px(PALETTE_INPUT_HEIGHT))
        .px(px(14.0))
        .py(px(8.0))
        .gap(px(6.0))
        .font_family(cx.theme().font_family.clone())
        .type_style(TextStyle::Small, cx)
        .when(!pills.is_empty(), |input| {
            input.prefix(
                div()
                    .flex()
                    .min_w_0()
                    .items_center()
                    .gap(px(6.0))
                    .children(pills.into_iter().map(|value| pill(value, cx))),
            )
        })
}

pub fn palette_prompt_input(
    input: &Entity<InputState>,
    prompt: impl Into<SharedString>,
    font_family: impl Into<SharedString>,
    cx: &App,
) -> Input {
    let font_family = font_family.into();
    Input::new(input)
        .w_full()
        .appearance(false)
        .h(px(PALETTE_INPUT_HEIGHT))
        .px(px(14.0))
        .py(px(8.0))
        .gap(px(8.0))
        .font_family(font_family.clone())
        .type_style(TextStyle::Small, cx)
        .prefix(
            Label::new(TextStyle::Small, prompt)
                .font_family(font_family)
                .text_color(cx.theme().foreground),
        )
}

#[must_use]
pub fn palette_shortcut(shortcut: &PaletteShortcut, ground: Ground, cx: &App) -> AnyElement {
    match shortcut {
        PaletteShortcut::Keys(keys) if !keys.is_empty() => h_flex()
            .flex_none()
            .gap(px(2.0))
            .children(
                keys.iter()
                    .map(|key| Kbd::new(key.clone()).lowercase().small().on(ground)),
            )
            .into_any_element(),
        PaletteShortcut::Keys(_) => div().into_any_element(),
        PaletteShortcut::Text(text) => Keycap::new(text.clone())
            .small()
            .on(ground)
            .font_family(cx.theme().mono_font_family.clone())
            .into_any_element(),
    }
}

#[must_use]
pub fn palette_row(
    id: impl Into<ElementId>,
    row: &PaletteRow,
    selected: bool,
    cx: &App,
) -> ListItem {
    palette_entry(id, row, selected, None, None, cx)
}

pub fn palette_tree_row(
    id: impl Into<ElementId>,
    row: &PaletteRow,
    selected: bool,
    on_toggle: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    cx: &App,
) -> ListItem {
    palette_entry(id, row, selected, None, Some(Box::new(on_toggle)), cx)
}

pub fn palette_badged_row(
    id: impl Into<ElementId>,
    row: &PaletteRow,
    selected: bool,
    badge: Badge,
    cx: &App,
) -> ListItem {
    palette_entry(id, row, selected, Some(badge), None, cx)
}

type ToggleHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

fn palette_entry(
    id: impl Into<ElementId>,
    row: &PaletteRow,
    selected: bool,
    badge: Option<Badge>,
    on_toggle: Option<ToggleHandler>,
    cx: &App,
) -> ListItem {
    let state = if selected {
        PaintState::Selected
    } else {
        PaintState::Rest
    };
    let ground = cx.theme().ground_at(RowLook::Menu, state);
    let foreground = ground.ink;
    let secondary = muted_on(ground, cx);
    let strong = TextStyle::Small.strong_weight();
    let highlights = row
        .label
        .char_indices()
        .map(|(start, character)| {
            let end = start + character.len_utf8();
            let matched = row.matches.iter().any(|range| range.contains(&start));
            let color = if matched {
                Some(foreground)
            } else {
                (start < row.muted_prefix).then_some(secondary)
            };
            (
                start..end,
                HighlightStyle {
                    color,
                    font_weight: Some(if matched {
                        FontWeight::SEMIBOLD
                    } else {
                        strong
                    }),
                    ..Default::default()
                },
            )
        })
        .collect::<Vec<_>>();

    let height = px(if cx.theme().inset_rows() {
        palette_row_height() - 2.0
    } else {
        palette_row_height()
    });
    ListItem::new(id)
        .row_height(height)
        .w_full()
        .h(height)
        .pl(px(12.0 + row.indent))
        .pr(px(12.0))
        .py(px(4.0))
        .selected(selected)
        .child(
            div()
                .w_full()
                .min_w_0()
                .flex()
                .items_center()
                .gap(px(8.0))
                .font_family(cx.theme().font_family.clone())
                .type_style(TextStyle::Small, cx)
                .when(row.expanded.is_some() || row.indent > 0.0, |element| {
                    element.child(
                        div()
                            .id("disclosure")
                            .debug_selector(|| "palette-disclosure".to_owned())
                            .flex_none()
                            .size(px(12.0))
                            .relative()
                            .top(px(0.5))
                            .when_some(row.expanded, |element, expanded| {
                                element
                                    .cursor_pointer()
                                    .child(
                                        Icon::new(if expanded {
                                            IconName::ChevronDown
                                        } else {
                                            IconName::ChevronRight
                                        })
                                        .size(px(12.0)),
                                    )
                                    .when_some(on_toggle, |element, on_toggle| {
                                        element
                                            .on_mouse_down(MouseButton::Left, |_, window, cx| {
                                                window.prevent_default();
                                                cx.stop_propagation();
                                            })
                                            .on_click(move |event, window, cx| {
                                                on_toggle(event, window, cx);
                                                cx.stop_propagation();
                                            })
                                    })
                            }),
                    )
                })
                .when_some(row.icon.clone(), |element, icon| {
                    element.child(
                        div()
                            .flex_none()
                            .relative()
                            .top(px(0.5))
                            .child(Icon::new(icon).size(px(12.0))),
                    )
                })
                .children(row.status.map(|status| {
                    if ground.solid {
                        status_dot(secondary)
                    } else {
                        palette_status_dot(status, cx)
                            .when(selected, |dot| ringed(dot, secondary, cx))
                    }
                }))
                .child(
                    div()
                        .min_w_0()
                        .when(row.detail.is_empty(), ala::Styled::flex_1)
                        .when(!row.detail.is_empty(), |label| {
                            label.flex_shrink_0().max_w(relative(0.6))
                        })
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(StyledText::new(row.label.clone()).with_highlights(highlights)),
                )
                .when(!row.detail.is_empty(), |element| {
                    element.child(
                        Label::new(TextStyle::Micro, row.detail.clone())
                            .muted()
                            .on(ground)
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .line_height(px(16.0)),
                    )
                })
                .when(!row.right.is_empty(), |element| {
                    element.child(
                        Label::new(TextStyle::Micro, row.right.clone())
                            .muted()
                            .on(ground)
                            .flex_none()
                            .line_height(px(16.0)),
                    )
                })
                .when_some(row.shortcut.as_ref(), |element, shortcut| {
                    element.child(palette_shortcut(shortcut, ground, cx))
                })
                .children(badge.map(|badge| badge.on(ground)))
                .when(row.running, |element| element.child(status_dot(secondary))),
        )
}

pub fn palette_section(
    label: impl Into<SharedString>,
    hint: impl Into<SharedString>,
    cx: &App,
) -> Div {
    div()
        .h(px(palette_row_height()))
        .flex()
        .items_center()
        .justify_between()
        .px(px(10.0))
        .pt(px(4.0))
        .type_style(TextStyle::Caption, cx)
        .text_color(cx.theme().foreground.muted())
        .child(label.into())
        .child(Label::new(TextStyle::Micro, hint).mono())
}

pub fn palette_empty(label: impl Into<SharedString>, cx: &App) -> Div {
    div()
        .py(px(16.0))
        .text_center()
        .type_style(TextStyle::Small, cx)
        .text_color(cx.theme().foreground.muted())
        .child(label.into())
}

pub fn palette_hint(hint: PaletteHint, cx: &App) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap(px(5.0))
        .whitespace_nowrap()
        .child(
            h_flex()
                .gap(px(3.0))
                .text_color(cx.theme().foreground)
                .children(
                    hint.key
                        .split([' ', '/'])
                        .filter(|key| !key.is_empty())
                        .map(|key| match Keystroke::parse(key) {
                            Ok(stroke) => KeyGlyphs::new(stroke).into_any_element(),
                            Err(_) => key.to_owned().into_any_element(),
                        }),
                ),
        )
        .child(hint.label)
}

pub fn palette_key_hint(
    keys: impl IntoIterator<Item = &'static str>,
    label: impl Into<SharedString>,
) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap(px(crate::CHROME_GAP))
        .children(
            keys.into_iter()
                .filter_map(|key| Keystroke::parse(key).ok())
                .map(Kbd::new),
        )
        .child(label.into())
}

#[derive(IntoElement)]
pub struct PaletteSurface {
    input: AnyElement,
    usage: Option<SharedString>,
    reserve_usage: bool,
    rows: Option<AnyElement>,
    hints: Vec<PaletteHint>,
    revision: u64,
    input_last: bool,
    over_input: Option<AnyElement>,
    footer_leading: Option<AnyElement>,
    max_width: Pixels,
}

impl PaletteSurface {
    pub fn new(input: impl IntoElement, revision: u64) -> Self {
        Self {
            input: input.into_any_element(),
            usage: None,
            reserve_usage: false,
            rows: None,
            hints: Vec::new(),
            revision,
            input_last: false,
            over_input: None,
            footer_leading: None,
            max_width: px(PALETTE_MAX_WIDTH),
        }
    }

    #[must_use]
    pub fn footer_leading(mut self, element: impl IntoElement) -> Self {
        self.footer_leading = Some(element.into_any_element());
        self
    }

    #[must_use]
    pub fn over_input(mut self, element: impl IntoElement) -> Self {
        self.over_input = Some(element.into_any_element());
        self
    }

    #[must_use]
    pub const fn input_last(mut self) -> Self {
        self.input_last = true;
        self
    }

    #[must_use]
    pub const fn max_width(mut self, width: Pixels) -> Self {
        self.max_width = width;
        self
    }

    #[must_use]
    pub fn rows(mut self, rows: impl IntoElement) -> Self {
        self.rows = Some(rows.into_any_element());
        self
    }

    #[must_use]
    pub fn hints(mut self, hints: impl IntoIterator<Item = PaletteHint>) -> Self {
        self.hints = hints.into_iter().collect();
        self
    }

    #[must_use]
    pub const fn reserve_usage(mut self) -> Self {
        self.reserve_usage = true;
        self
    }

    #[must_use]
    pub fn usage(mut self, usage: impl Into<SharedString>) -> Self {
        let usage = usage.into();
        self.usage = (!usage.is_empty()).then_some(usage);
        self
    }
}

impl RenderOnce for PaletteSurface {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let (input, input_last) = if self.input_last {
            (None, Some(self.input))
        } else {
            (Some(self.input), None)
        };
        let surface = Panel::new("palette-surface", PanelLook::Popover)
            .inset(px(PALETTE_INSET))
            .relative()
            .w_full()
            .max_w(self.max_width)
            .flex()
            .flex_col()
            .overflow_hidden()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .children(input)
            .children((self.usage.is_some() || self.reserve_usage).then(|| {
                div()
                    .h(px(24.0))
                    .min_w_0()
                    .flex_none()
                    .px(px(18.0))
                    .pb(px(8.0))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .type_style_mono(TextStyle::Micro, cx)
                    .line_height(px(16.0))
                    .text_color(cx.theme().foreground.muted())
                    .children(self.usage)
            }))
            .children(self.rows.map(|rows| {
                div()
                    .list_inset(cx)
                    .pt(px(if self.input_last { PALETTE_INSET } else { 2.0 }))
                    .pb(px(if self.input_last { 0.0 } else { PALETTE_INSET }))
                    .child(rows)
            }))
            .when_some(input_last, |surface, input| {
                surface.child(div().flex_none().relative().child(input).children(
                    self.over_input.map(|over| {
                        deferred(
                            div()
                                .absolute()
                                .left_0()
                                .right_0()
                                .bottom(relative(1.0))
                                .child(over),
                        )
                        .with_priority(1)
                    }),
                ))
            })
            .child(
                div()
                    .min_h(px(PALETTE_FOOTER_HEIGHT))
                    .flex_none()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .justify_end()
                    .gap(px(14.0))
                    .px(px(18.0))
                    .py(px(8.0))
                    .rule(RuleLook::Bar, Edge::Top, cx)
                    .type_style(TextStyle::Micro, cx)
                    .line_height(px(16.0))
                    .text_color(cx.theme().foreground.muted())
                    .children(self.footer_leading.map(|leading| {
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .items_center()
                            .child(leading)
                    }))
                    .children(self.hints.into_iter().map(|hint| palette_hint(hint, cx))),
            );
        crate::foundation::surface_enter(
            surface,
            ElementId::NamedInteger("palette-open".into(), self.revision),
            px(0.0),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Elevation, Root, Theme, ThemeMode};
    use ala::{Context, Focusable as _, Render, TestAppContext, Window};

    struct Preview {
        input: Entity<InputState>,
        width: f32,
        expanded: bool,
        activations: usize,
    }

    impl Render for Preview {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let toggle = cx.entity();
            let activate = cx.entity();
            div().w(px(self.width)).px(px(16.0)).child(
                PaletteSurface::new(
                    palette_input(
                        &self.input,
                        [
                            PalettePill::Status {
                                label: "gpu-box".into(),
                                status: PaletteStatus::Online,
                            },
                            PalettePill::Mode {
                                prefix: ":".into(),
                                label: "Command".into(),
                            },
                            PalettePill::Code("join-pane".into()),
                        ],
                        cx,
                    ),
                    0,
                )
                .usage("join-pane [-bdhv] [-s src-pane] [-t dst-pane]")
                .rows(
                    div().child(
                        palette_tree_row(
                            "selected",
                            &PaletteRow {
                                label: "zz-dev / editor".into(),
                                muted_prefix: 9,
                                status: Some(PaletteStatus::Running),
                                right: "Running".into(),
                                icon: Some(IconName::Folder),
                                expanded: Some(self.expanded),
                                ..Default::default()
                            },
                            true,
                            move |_, _, cx| {
                                toggle.update(cx, |preview, cx| {
                                    preview.expanded = !preview.expanded;
                                    cx.notify();
                                });
                            },
                            cx,
                        )
                        .on_click(move |_, _, cx| {
                            activate.update(cx, |preview, _| preview.activations += 1);
                        }),
                    ),
                )
                .hints([
                    PaletteHint {
                        key: "up down",
                        label: "navigate",
                    },
                    PaletteHint {
                        key: "enter",
                        label: "run",
                    },
                    PaletteHint {
                        key: "backspace",
                        label: "back",
                    },
                ]),
            )
        }
    }

    #[allow(
        clippy::disallowed_methods,
        reason = "the test finds the selected row by the menu-item fill"
    )]
    fn selected_row_fill(cx: &App) -> Hsla {
        cx.theme().layer(RowLook::Menu, PaintState::Selected).fill
    }

    #[ala::test]
    fn palette_fits_its_container_and_tracks_light_and_dark_themes(cx: &mut TestAppContext) {
        for (width, mode, radius) in [
            (720.0_f32, ThemeMode::Dark, 12.0),
            (440.0, ThemeMode::Light, 25.0),
        ] {
            cx.update(|cx| {
                crate::init(cx);
                cx.set_reduce_motion(true);
                crate::foundation::interface_theme::InterfaceTheme {
                    glass: Some(crate::foundation::interface_theme::Glass::None),
                    ..crate::foundation::interface_theme::InterfaceTheme::default()
                }
                .activate(Some(mode), None, cx);
                Theme::global_mut(cx).radius = px(radius);
            });
            let (_, cx) = cx.add_window_view(|window, cx| {
                let preview = cx.new(|cx| Preview {
                    input: cx.new(|cx| InputState::new(window, cx).placeholder("Target pane")),
                    width,
                    expanded: false,
                    activations: 0,
                });
                preview.read(cx).input.focus_handle(cx).focus(window, cx);
                Root::new(preview, window, cx)
            });
            cx.run_until_parked();
            cx.update(|window, cx| {
                _ = window.draw(cx);
                let quads = window.painted_quads();
                let surface = quads
                    .iter()
                    .find(|quad| {
                        quad.background
                            == ala::solid_background(
                                cx.theme().surface(Elevation::Popover).fill.opaque(),
                            )
                            && quad.bounds.size.width.0 > 300.0 * window.scale_factor()
                    })
                    .expect("palette surface");
                let selection = quads
                    .iter()
                    .find(|quad| {
                        quad.background == ala::solid_background(selected_row_fill(cx))
                            && quad.bounds.size.width.0 > 100.0 * window.scale_factor()
                    })
                    .expect("selected row");
                assert_eq!(
                    surface.bounds.size.width.0 / window.scale_factor(),
                    (width - 32.0).min(560.0)
                );
                assert!(surface.bounds.contains(&selection.bounds.origin));
                assert!(surface.bounds.contains(&selection.bounds.bottom_right()));
                assert!(selection.corner_radii.top_left.0 < selection.bounds.size.height.0 / 2.0);
                assert!(
                    quads.iter().any(|quad| {
                        quad.border_color
                            == muted_on(
                                cx.theme().ground_at(RowLook::Menu, PaintState::Selected),
                                cx,
                            )
                            && quad.bounds.size.width.0
                                == crate::blocks::sidebar::STATUS_DOT_SIZE * window.scale_factor()
                            && quad.border_widths.top.0 > 0.0
                    }),
                    "selected running agent remains visible on the accent selection"
                );
            });
        }
    }

    #[ala::test]
    fn disclosure_toggles_without_activating_or_taking_input_focus(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let (preview, cx) = cx.add_window_view(|window, cx| {
            let input = cx.new(|cx| InputState::new(window, cx));
            input.focus_handle(cx).focus(window, cx);
            Preview {
                input,
                width: 560.0,
                expanded: false,
                activations: 0,
            }
        });
        cx.run_until_parked();
        let disclosure = cx.debug_bounds("palette-disclosure").unwrap();
        cx.simulate_click(disclosure.center(), ala::Modifiers::default());
        cx.run_until_parked();
        preview.read_with(cx, |preview, _| {
            assert!(preview.expanded);
            assert_eq!(preview.activations, 0);
        });
        assert!(cx.update(|window, cx| preview.read(cx).input.focus_handle(cx).is_focused(window)));
        cx.simulate_click(
            disclosure.center() + ala::point(px(100.0), px(0.0)),
            ala::Modifiers::default(),
        );
        cx.run_until_parked();
        assert_eq!(preview.read_with(cx, |preview, _| preview.activations), 1);
    }
}
