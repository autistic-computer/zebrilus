use serde::{Deserialize, Serialize};
use zz_gpui::{Hsla, Pixels, StatefulInteractiveElement, Styled, px};

use super::{
    Colorize as _, Theme,
    color::{faint_weight, muted_weight, raised_bright, readable_on},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Tone {
    #[default]
    Neutral,
    Primary,
    Danger,
    Warning,
    Success,
    Accent,
}

impl Tone {
    pub const ALL: [Self; 6] = [
        Self::Neutral,
        Self::Primary,
        Self::Danger,
        Self::Warning,
        Self::Success,
        Self::Accent,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Neutral => "neutral",
            Self::Primary => "primary",
            Self::Danger => "danger",
            Self::Warning => "warning",
            Self::Success => "success",
            Self::Accent => "accent",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Emphasis {
    Solid,
    #[default]
    Soft,
    Outline,
    Ghost,
}

impl Emphasis {
    pub const ALL: [Self; 4] = [Self::Solid, Self::Soft, Self::Outline, Self::Ghost];

    pub fn name(self) -> &'static str {
        match self {
            Self::Solid => "solid",
            Self::Soft => "soft",
            Self::Outline => "outline",
            Self::Ghost => "ghost",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PrimaryStyle {
    #[default]
    Foreground,
    Accent,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PaintState {
    #[default]
    Rest,
    Hover,
    Active,
    Selected,
    Current,
    Focused,
    Disabled,
}

impl PaintState {
    pub const ALL: [Self; 7] = [
        Self::Rest,
        Self::Hover,
        Self::Active,
        Self::Selected,
        Self::Current,
        Self::Focused,
        Self::Disabled,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Rest => "rest",
            Self::Hover => "hover",
            Self::Active => "pressed",
            Self::Selected => "selected",
            Self::Current => "current",
            Self::Focused => "focused",
            Self::Disabled => "disabled",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Paint {
    pub fill: Hsla,
    pub hover: Hsla,
    pub active: Hsla,
    pub selected: Hsla,
    pub current: Hsla,
    pub border: Hsla,
    pub border_width: Pixels,
    pub focus: Hsla,
    pub text: Hsla,
    pub selected_text: Hsla,
    pub ground: Ground,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Elevation {
    #[default]
    Window,
    Panel,
    Popover,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ground {
    pub fill: Hsla,
    pub ink: Hsla,
    pub elevation: Elevation,
    pub glass: bool,
}

impl Ground {
    pub fn opaque(self) -> Self {
        Self {
            fill: self.fill.opaque(),
            ..self
        }
    }
}

impl Paint {
    pub fn fill_at(&self, state: PaintState) -> Hsla {
        match state {
            PaintState::Rest | PaintState::Focused => self.fill,
            PaintState::Hover => self.hover,
            PaintState::Active => self.active,
            PaintState::Selected => self.selected,
            PaintState::Current => self.current,
            PaintState::Disabled => self.fill.opacity(DISABLED),
        }
    }

    pub fn border_at(&self, state: PaintState) -> Hsla {
        match state {
            PaintState::Focused => self.focus,
            PaintState::Disabled => self.border.opacity(DISABLED),
            _ => self.border,
        }
    }

    pub fn text_at(&self, state: PaintState) -> Hsla {
        match state {
            PaintState::Selected => self.selected_text,
            PaintState::Disabled => self.text.opacity(DISABLED),
            _ => self.text,
        }
    }

    pub fn muted_at(&self, state: PaintState) -> Hsla {
        self.text_at(state)
            .mix_oklab(self.ground_at(state).fill, 1.0 - muted_weight())
    }

    pub fn faint_at(&self, state: PaintState) -> Hsla {
        self.text_at(state)
            .mix_oklab(self.ground_at(state).fill, 1.0 - faint_weight())
    }

    pub fn opaque(self) -> Self {
        let ground = self.ground.fill;
        Self {
            fill: ground.blend(self.fill),
            hover: ground.blend(self.hover),
            active: ground.blend(self.active),
            selected: ground.blend(self.selected),
            current: ground.blend(self.current),
            ..self
        }
    }

    pub fn ground_at(&self, state: PaintState) -> Ground {
        Ground {
            fill: self.ground.fill.blend(self.fill_at(state)),
            ink: self.text_at(state),
            ..self.ground
        }
    }
}

const DISABLED: f32 = 0.5;
const OPAQUE_ENOUGH: f32 = 0.5;
const FOCUS_ALPHA: f32 = 0.8;
const SOFT_ALPHA: [f32; 3] = [0.12, 0.18, 0.24];
const NEUTRAL_OUTLINE_ALPHA: f32 = 0.3;
const TONE_OUTLINE_ALPHA: f32 = 0.55;
const SOFT_OUTLINE_SHARE: f32 = 1.0 / 3.0;
const TONE_TEXT_WEIGHT: f32 = 0.7;

impl Theme {
    pub fn tone(&self, tone: Tone) -> Hsla {
        match tone {
            Tone::Neutral => self.foreground,
            Tone::Primary => match self.primary {
                PrimaryStyle::Foreground => self.foreground,
                PrimaryStyle::Accent => self.accent,
            },
            Tone::Danger => self.danger,
            Tone::Warning => self.warning,
            Tone::Success => self.success,
            Tone::Accent => self.accent,
        }
    }

    pub fn field_emphasis(&self, role: super::SurfaceRole) -> Emphasis {
        let style = self.style(role);
        match style.fill {
            super::Fill::None if style.outline => Emphasis::Outline,
            super::Fill::None => Emphasis::Ghost,
            _ => Emphasis::Soft,
        }
    }

    pub fn ground(&self) -> Ground {
        Ground {
            fill: self.background,
            ink: self.foreground,
            elevation: Elevation::Window,
            glass: false,
        }
    }

    pub fn raise(&self, color: Hsla, level: u8) -> Hsla {
        if self.bright_surfaces {
            raised_bright(color, level)
        } else {
            color.raised(level)
        }
    }

    pub fn surface(&self, elevation: Elevation) -> Ground {
        let fill = match elevation {
            Elevation::Window => return self.ground(),
            Elevation::Panel => self.raise(self.background, 1),
            Elevation::Popover => self.raise(self.background, 2),
        };
        Ground {
            fill,
            ink: self.foreground,
            elevation,
            glass: false,
        }
    }

    pub fn paint(&self, tone: Tone, emphasis: Emphasis) -> Paint {
        self.paint_on(self.ground(), tone, emphasis)
    }

    pub fn paint_on(&self, ground: Ground, tone: Tone, emphasis: Emphasis) -> Paint {
        let neutral = tone == Tone::Neutral
            || (tone == Tone::Primary && self.primary == PrimaryStyle::Foreground);
        let ink = ground.ink;
        let color = if neutral { ink } else { self.tone(tone) };
        let window = ground == self.ground();
        let clear = self.transparent;
        let focus = match tone {
            Tone::Danger | Tone::Warning | Tone::Success | Tone::Accent => color,
            Tone::Neutral | Tone::Primary => match self.primary {
                PrimaryStyle::Foreground => ink,
                PrimaryStyle::Accent => self.accent,
            },
        }
        .opacity(FOCUS_ALPHA);
        let soft = |step: u8| {
            if neutral {
                ground.fill.washed(step + 1)
            } else {
                color.opacity(SOFT_ALPHA[usize::from(step)])
            }
        };
        let selected = if neutral && window {
            self.selection_background()
        } else {
            soft(2)
        };
        let on_accent = self
            .on_accent
            .unwrap_or_else(|| readable_on(self.accent.opaque()));
        let accent_selection = neutral && window;
        let readable = |fill: Hsla, text: Hsla| {
            if accent_selection {
                on_accent
            } else if fill.a > OPAQUE_ENOUGH {
                readable_on(fill)
            } else {
                text
            }
        };
        let text = if neutral {
            ink
        } else {
            color.mix_oklab(ink, TONE_TEXT_WEIGHT)
        };
        let outline = if neutral {
            ink.opacity(NEUTRAL_OUTLINE_ALPHA)
        } else {
            color.opacity(TONE_OUTLINE_ALPHA)
        };
        let solid_text = if tone == Tone::Accent
            || (tone == Tone::Primary && self.primary == PrimaryStyle::Accent)
        {
            on_accent
        } else {
            readable_on(color)
        };
        match emphasis {
            Emphasis::Solid => Paint {
                fill: color,
                hover: color.hover(),
                active: color.active(),
                selected: color.active(),
                current: color.active(),
                border: clear,
                border_width: px(0.0),
                focus,
                text: solid_text,
                selected_text: solid_text,
                ground,
            },
            Emphasis::Soft => {
                let hairline = self.outlined();
                Paint {
                    fill: soft(0),
                    hover: soft(1),
                    active: soft(2),
                    selected,
                    current: soft(2),
                    border: if hairline {
                        outline.opacity(SOFT_OUTLINE_SHARE)
                    } else {
                        clear
                    },
                    border_width: if hairline { self.outline } else { px(0.0) },
                    focus,
                    text,
                    selected_text: readable(selected, text),
                    ground,
                }
            }
            Emphasis::Outline => Paint {
                fill: clear,
                hover: soft(0),
                active: soft(1),
                selected,
                current: soft(2),
                border: outline,
                border_width: px(1.0),
                focus,
                text,
                selected_text: readable(selected, text),
                ground,
            },
            Emphasis::Ghost => Paint {
                fill: clear,
                hover: soft(1),
                active: soft(2),
                selected,
                current: soft(2),
                border: clear,
                border_width: px(0.0),
                focus,
                text,
                selected_text: readable(selected, text),
                ground,
            },
        }
    }
}

pub trait PaintExt: Styled + Sized {
    fn painted(self, paint: Paint, state: PaintState) -> Self {
        self.bg(paint.fill_at(state))
            .border(paint.border_width)
            .border_color(paint.border_at(state))
            .text_color(paint.text_at(state))
    }
}

impl<E: Styled> PaintExt for E {}

pub trait InteractivePaintExt: StatefulInteractiveElement + Styled + Sized {
    fn painted_interactive(self, paint: Paint, selected: bool, enabled: bool) -> Self {
        let state = match (enabled, selected) {
            (false, _) => PaintState::Disabled,
            (true, true) => PaintState::Selected,
            (true, false) => PaintState::Rest,
        };
        let this = self.painted(paint, state);
        if !enabled || selected {
            return this;
        }
        this.hover(move |style| style.bg(paint.hover))
            .active(move |style| style.bg(paint.active))
    }
}

impl<E: StatefulInteractiveElement + Styled> InteractivePaintExt for E {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ThemeColor;
    use zz_gpui::hsla;

    fn accent_theme(on_accent: Option<Hsla>) -> Theme {
        let mut theme = Theme::from(&*ThemeColor::dark());
        theme.primary = PrimaryStyle::Accent;
        theme.colors.on_accent = on_accent;
        theme
    }

    #[test]
    fn accent_fills_take_the_themes_accent_text() {
        let ink = hsla(0.5, 0.5, 0.5, 1.0);
        let theme = accent_theme(Some(ink));
        assert_eq!(theme.paint(Tone::Primary, Emphasis::Solid).text, ink);
        assert_eq!(
            theme.paint(Tone::Neutral, Emphasis::Ghost).selected_text,
            ink
        );
        assert_ne!(theme.paint(Tone::Danger, Emphasis::Solid).text, ink);
    }

    #[test]
    fn accent_text_reads_on_the_accent_when_the_theme_names_none() {
        let theme = accent_theme(None);
        let expected = readable_on(theme.accent.opaque());
        assert_eq!(theme.paint(Tone::Primary, Emphasis::Solid).text, expected);
        assert_eq!(
            theme.paint(Tone::Neutral, Emphasis::Ghost).selected_text,
            expected
        );
    }
}
