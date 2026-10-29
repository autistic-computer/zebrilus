pub mod floating;
mod gui_level;
mod level;
mod palette;
mod palette_model;
mod palette_places;
mod palette_view;
mod palette_vim;

pub use gui_level::{GuiLevel, theme_offer};
pub use level::{
    ActionContext, ActionKey, CopyAddress, LevelEffect, PaletteActionLevel, install_recents,
    palette_recents, record_recent,
};
pub use palette::{
    PalettePill, PaletteRow, PaletteStatus, command_palette_empty, command_palette_entry,
    command_palette_section, command_palette_tree_entry, unified_command_palette_input,
};
pub use palette_model::{
    PaletteHostId, PaletteMode, PaletteSettings, PaletteTarget, PaletteTree, PaletteTreeHost,
    PaletteTreePane, PaletteTreeSession, PaletteTreeWindow,
};
pub use palette_places::palette_fleet;
pub use palette_view::{
    CommandPaletteEvent, CommandPaletteView, FolderPick, GuiEvent, PaletteBackend, target_hosts,
    target_picker,
};

pub(crate) use palette_model::fuzzy_match;
pub use palette_vim::PaletteVimMode;

use crate::{
    ActiveTheme as _, CHROME_GAP, Colorize as _, Edge, PaintState, Sizable as _, StyledExt as _,
    badge::Badge,
    h_flex,
    input::{Input, InputState},
    kbd::{Kbd, KeyGlyphs},
    list::ListItem,
};
use ala::{
    AnyElement, App, ElementId, Entity, IntoElement, Keystroke, ParentElement as _, Pixels,
    RenderOnce, SharedString, Styled as _, deferred, div, prelude::*, px, relative,
};

pub const COMMAND_PALETTE_MAX_WIDTH: f32 = 560.0;
pub const COMMAND_PALETTE_INSET: f32 = 8.0;
pub const COMMAND_PALETTE_ROW_HEIGHT: f32 = 28.0;
pub const COMMAND_PALETTE_INPUT_HEIGHT: f32 = 40.0;
pub const COMMAND_PALETTE_FOOTER_HEIGHT: f32 = 32.0;
/// The tallest the rows grow, about sixteen of them.
const COMMAND_PALETTE_LIST_MAX: f32 = 440.0;

/// How tall the palette's rows may grow in a window `viewport` tall: up to
/// [`COMMAND_PALETTE_LIST_MAX`] and short of the window's bottom, less a line
/// when the palette shows a command's `usage`.
pub fn command_palette_list_height(viewport: Pixels, usage: bool) -> f32 {
    (f32::from(viewport) * 0.88 - if usage { 175.0 } else { 140.0 })
        .clamp(40.0, COMMAND_PALETTE_LIST_MAX)
}

/// The height of a palette row at the theme's density.
pub fn command_palette_row_height() -> f32 {
    px(COMMAND_PALETTE_ROW_HEIGHT).as_f32()
}

/// The palette surface's radius: its role's, opened by
/// [`COMMAND_PALETTE_INSET`] to stay concentric with the children's corners.
pub fn command_palette_radius(cx: &App) -> Pixels {
    crate::picker::outer_radius(
        &cx.theme().layer(crate::roles::PALETTE, PaintState::Rest),
        px(COMMAND_PALETTE_INSET),
    )
}

#[derive(Clone, Copy)]
pub struct PaletteHint {
    pub key: &'static str,
    pub label: &'static str,
}

pub fn command_palette_input(
    input: &Entity<InputState>,
    prompt: impl Into<SharedString>,
    font_family: impl Into<SharedString>,
    cx: &App,
) -> Input {
    let font_family = font_family.into();
    Input::new(input)
        .w_full()
        .appearance(false)
        .h(px(COMMAND_PALETTE_INPUT_HEIGHT))
        .px(px(14.0))
        .py(px(8.0))
        .gap(px(8.0))
        .font_family(font_family.clone())
        .text_size(crate::rems_from_px(12.0))
        .line_height(px(16.0))
        .prefix(
            div()
                .font_family(font_family)
                .text_size(crate::rems_from_px(12.0))
                .text_color(cx.theme().foreground)
                .child(prompt.into()),
        )
}

/// A completion row. The caller attaches event handlers to the returned
/// `ListItem`.
pub fn command_palette_row(
    id: impl Into<ala::ElementId>,
    label: impl Into<SharedString>,
    detail: impl Into<SharedString>,
    badge: Option<AnyElement>,
    selected: bool,
    cx: &App,
) -> ListItem {
    palette::palette_entry(
        id,
        &PaletteRow {
            label: label.into(),
            detail: detail.into(),
            ..Default::default()
        },
        selected,
        badge,
        None,
        cx,
    )
}

pub fn command_kind_badge(
    label: impl Into<SharedString>,
    font_family: impl Into<SharedString>,
) -> Badge {
    Badge::new(label)
        .small()
        .font_family(font_family.into())
        .text_size(crate::rems_from_px(9.0))
}

/// The palette surface. Input and virtualized rows are caller-supplied slots.
#[derive(IntoElement)]
pub struct CommandPaletteSurface {
    input: AnyElement,
    usage: Option<SharedString>,
    rows: Option<AnyElement>,
    hints: Vec<PaletteHint>,
    revision: u64,
    input_last: bool,
    over_input: Option<AnyElement>,
    footer_leading: Option<AnyElement>,
    max_width: Pixels,
}

impl CommandPaletteSurface {
    pub fn new(input: impl IntoElement, revision: u64) -> Self {
        Self {
            input: input.into_any_element(),
            usage: None,
            rows: None,
            hints: Vec::new(),
            revision,
            input_last: false,
            over_input: None,
            footer_leading: None,
            max_width: px(COMMAND_PALETTE_MAX_WIDTH),
        }
    }

    /// Put `element` at the left of the footer, before the keys.
    #[must_use]
    pub fn footer_leading(mut self, element: impl IntoElement) -> Self {
        self.footer_leading = Some(element.into_any_element());
        self
    }

    /// Float `element` over the rows, its bottom on the input's top edge and
    /// as wide as the palette, such as a picker for what is being typed.
    /// Only with [`Self::input_last`].
    #[must_use]
    pub fn over_input(mut self, element: impl IntoElement) -> Self {
        self.over_input = Some(element.into_any_element());
        self
    }

    /// Put the input under the rows, the way a chat keeps its composer.
    #[must_use]
    pub fn input_last(mut self) -> Self {
        self.input_last = true;
        self
    }

    #[must_use]
    pub fn max_width(mut self, width: Pixels) -> Self {
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
    pub fn usage(mut self, usage: impl Into<SharedString>) -> Self {
        let usage = usage.into();
        self.usage = (!usage.is_empty()).then_some(usage);
        self
    }
}

impl RenderOnce for CommandPaletteSurface {
    fn render(self, _: &mut ala::Window, cx: &mut App) -> impl IntoElement {
        let (input, input_last) = if self.input_last {
            (None, Some(self.input))
        } else {
            (Some(self.input), None)
        };
        let surface = div()
            .id("command-palette-surface")
            .relative()
            .w_full()
            .max_w(self.max_width)
            .flex()
            .flex_col()
            .overflow_hidden()
            .surface(crate::roles::PALETTE, cx)
            .rounded(command_palette_radius(cx))
            .on_mouse_down(ala::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .children(input)
            .children(self.usage.map(|usage| {
                div()
                    .h(px(24.0))
                    .min_w_0()
                    .flex_none()
                    .px(px(18.0))
                    .pb(px(8.0))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .font_family(cx.theme().mono_font_family.clone())
                    .text_size(crate::rems_from_px(10.0))
                    .line_height(px(16.0))
                    .text_color(cx.theme().foreground.muted())
                    .child(usage)
            }))
            .children(self.rows.map(|rows| {
                div()
                    .list_inset(cx)
                    .pt(px(if self.input_last {
                        COMMAND_PALETTE_INSET
                    } else {
                        2.0
                    }))
                    .pb(px(if self.input_last {
                        0.0
                    } else {
                        COMMAND_PALETTE_INSET
                    }))
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
                    .min_h(px(COMMAND_PALETTE_FOOTER_HEIGHT))
                    .flex_none()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .justify_end()
                    .gap(px(14.0))
                    .px(px(18.0))
                    .py(px(8.0))
                    .rule_on(crate::roles::BAR_RULE, Edge::Top, cx)
                    .text_size(crate::rems_from_px(10.0))
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
        penna::foundation::surface_enter(
            surface,
            ElementId::NamedInteger("command-palette-open".into(), self.revision),
            px(0.0),
        )
    }
}

/// A footer hint: its keys in the platform's glyphs, then what they do.
/// `hint.key` holds ala keystrokes apart by spaces or `/`, such as
/// `up down` or `ctrl-c`.
pub(crate) fn palette_hint(hint: PaletteHint, cx: &App) -> impl IntoElement {
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

/// A shortcut in tmux's spelling (`C-b c`) as one cap per key in the
/// platform's glyphs, or as written when a key does not parse.
pub fn tmux_shortcut(shortcut: &SharedString, ground: crate::Ground, cx: &App) -> AnyElement {
    let keys: Option<Vec<Keystroke>> = shortcut.split_whitespace().map(tmux_keystroke).collect();
    match keys {
        Some(keys) if !keys.is_empty() => h_flex()
            .flex_none()
            .gap(px(2.0))
            .children(
                keys.into_iter()
                    .map(|key| Kbd::new(key).lowercase().small().on(ground)),
            )
            .into_any_element(),
        _ => crate::kbd::Keycap::new(shortcut.clone())
            .small()
            .on(ground)
            .font_family(cx.theme().mono_font_family.clone())
            .into_any_element(),
    }
}

/// A key in tmux's spelling (`C-b`, `M-Up`, `%`, `L`) as a ala keystroke, for
/// drawing in the platform's glyphs. `None` for a key ala cannot name.
#[must_use]
pub fn tmux_keystroke(spelling: &str) -> Option<Keystroke> {
    let (source, base) = zz_client::palette::tmux_keystroke_source(spelling)?;
    let base = base.as_str();
    let mut keystroke = Keystroke::parse(&source).ok()?;
    if base == "minus" {
        keystroke.key = "-".into();
    }
    if keystroke.key_char.is_none()
        && !keystroke.modifiers.control
        && !keystroke.modifiers.alt
        && !keystroke.modifiers.platform
        && base.chars().count() == 1
    {
        keystroke.key_char = Some(base.to_owned());
    }
    Some(keystroke)
}

/// Keyboard shortcut hint styling, shared with palette-adjacent surfaces.
///
/// # Panics
///
/// If a key is not a parseable [`Keystroke`].
pub fn palette_shortcut_hint(
    keys: impl IntoIterator<Item = &'static str>,
    label: impl Into<SharedString>,
) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap(px(CHROME_GAP))
        .children(
            keys.into_iter()
                .map(|key| Kbd::new(Keystroke::parse(key).expect("static palette keystroke"))),
        )
        .child(label.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tmux_spellings_become_ala_keystrokes() {
        let control = tmux_keystroke("C-b").expect("prefix");
        assert!(control.modifiers.control);
        assert_eq!(control.key, "b");
        let percent = tmux_keystroke("%").expect("percent");
        assert_eq!(percent.key, "%");
        assert_eq!(percent.key_char.as_deref(), Some("%"));
        let upper = tmux_keystroke("L").expect("upper");
        assert!(upper.modifiers.shift);
        assert_eq!(upper.key, "l");
        assert_eq!(
            tmux_keystroke("Escape").map(|key| key.key),
            Some("escape".into())
        );
        assert_eq!(
            tmux_keystroke("M-Up").map(|key| (key.key, key.modifiers.alt)),
            Some(("up".into(), true))
        );
        assert!(tmux_keystroke("F13-nope").is_none());
    }
}
