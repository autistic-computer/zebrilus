use ala::{Hsla, Pixels, StatefulInteractiveElement, Styled, px};

use super::{
    Colorize as _, Theme,
    color::{faint_weight, ink_on, muted_weight, readable_on},
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
    pub solid: bool,
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
        let ground = self.ground_at(state);
        if ground.solid {
            return self.text_at(state);
        }
        ink_on(self.text_at(state), ground.fill, muted_weight())
    }

    pub fn faint_at(&self, state: PaintState) -> Hsla {
        ink_on(
            self.text_at(state),
            self.ground_at(state).fill,
            faint_weight(),
        )
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
        let fill = self.fill_at(state);
        Ground {
            fill: self.ground.fill.blend(fill),
            ink: self.text_at(state),
            solid: self.ground.solid || fill.a > OPAQUE_ENOUGH,
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
            Tone::Neutral | Tone::Primary => self.foreground,
            Tone::Danger => self.danger,
            Tone::Warning => self.warning,
            Tone::Success => self.success,
            Tone::Accent => self.accent,
        }
    }

    pub(crate) fn field_emphasis(&self, role: super::SurfaceRole) -> Emphasis {
        let style = self.style(role);
        match style.fill {
            super::Fill::None if style.outline => Emphasis::Outline,
            super::Fill::None => Emphasis::Ghost,
            _ => Emphasis::Soft,
        }
    }

    pub fn ground(&self) -> Ground {
        Ground {
            fill: self
                .background
                .alpha(self.background.a * self.window_opacity().clamp(0.0, 1.0)),
            ink: self.foreground,
            elevation: Elevation::Window,
            glass: false,
            solid: false,
        }
    }

    pub fn raise(&self, color: Hsla, level: u8) -> Hsla {
        color.raised(level)
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
            solid: false,
        }
    }

    pub fn paint(&self, tone: Tone, emphasis: Emphasis) -> Paint {
        self.paint_on(self.ground(), tone, emphasis)
    }

    pub fn paint_on(&self, ground: Ground, tone: Tone, emphasis: Emphasis) -> Paint {
        let tone = if emphasis != Emphasis::Solid && ground.solid {
            Tone::Neutral
        } else {
            tone
        };
        let neutral = matches!(tone, Tone::Neutral | Tone::Primary);
        let ink = ground.ink;
        let color = if neutral { ink } else { self.tone(tone) };
        let clear = self.transparent;
        let focus = color.opacity(FOCUS_ALPHA);
        let soft = |step: u8| {
            if neutral {
                ground.fill.washed(step + 1)
            } else {
                color.opacity(SOFT_ALPHA[usize::from(step)])
            }
        };
        let selected = soft(2);
        let readable = |fill: Hsla, text: Hsla| {
            if fill.a > OPAQUE_ENOUGH {
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
        let solid_text = if tone == Tone::Accent {
            self.accent_ink()
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
    use ala::hsla;

    fn accent_theme(accent: &str, on_accent: Option<Hsla>) -> Theme {
        let mut theme = Theme::from(&*ThemeColor::dark());
        theme.accent = crate::parse_hex(accent).unwrap();
        theme.colors.on_accent = on_accent;
        theme
    }

    #[test]
    fn accent_fills_take_the_themes_accent_text() {
        let ink = hsla(0.5, 0.5, 0.5, 1.0);
        let theme = accent_theme("#3183ff", Some(ink));
        assert_eq!(theme.paint(Tone::Accent, Emphasis::Solid).text, ink);
        assert_ne!(theme.paint(Tone::Danger, Emphasis::Solid).text, ink);
    }

    #[test]
    fn a_vivid_accent_carries_the_light_ink_and_a_pale_one_the_dark() {
        let light = readable_on(hsla(0., 0., 0., 1.));
        let dark = readable_on(hsla(0., 0., 1., 1.));
        let vivid = accent_theme("#3183ff", None);
        assert_eq!(readable_on(vivid.accent), dark);
        assert_eq!(vivid.paint(Tone::Accent, Emphasis::Solid).text, light);
        let pale = accent_theme("#ffe680", None);
        assert_eq!(pale.paint(Tone::Accent, Emphasis::Solid).text, dark);
    }

    #[test]
    fn primary_is_the_ink_whatever_the_accent() {
        let theme = accent_theme("#3183ff", None);
        let primary = theme.paint(Tone::Primary, Emphasis::Solid);
        assert_eq!(primary.fill, theme.foreground);
        assert_eq!(primary.text, readable_on(theme.foreground));
    }
}
