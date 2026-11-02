use ala::{
    AnyElement, App, Div, ElementId, Hsla, IntoElement, ParentElement, Pixels, RenderOnce,
    SharedString, Styled as _, TextRun, Window, div, px,
};

use crate::{
    ActiveTheme as _, Colorize as _, Emphasis, Fill, Ground, Icon, Label, PaintState, Selectable,
    Sizable, Size, Space, StyledExt as _, Surface, SurfaceRole, TextStyle, Tone,
    plate::{Plate, plate_element},
    rems_from_px, roles,
};

const DOT: f32 = 6.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ChipLook {
    #[default]
    Plain,
    Bar,
    Floating,
    Marker,
    Drag,
    Icon,
}

impl ChipLook {
    const fn role(self) -> SurfaceRole {
        match self {
            Self::Plain => SurfaceRole::CHIP,
            Self::Bar => roles::CHIP_BAR,
            Self::Floating => roles::CHIP_FLOATING,
            Self::Marker => roles::CHIP_MARKER,
            Self::Drag => roles::CHIP_DRAG,
            Self::Icon => roles::BADGE_ICON,
        }
    }
}

impl From<ChipLook> for Surface {
    fn from(look: ChipLook) -> Self {
        Self::new(look.role())
    }
}

#[derive(IntoElement)]
pub struct Chip {
    plate: Plate,
    look: ChipLook,
    label: Option<SharedString>,
    dot: Option<Tone>,
    icon: Option<Icon>,
    detail: Option<SharedString>,
    tone: Tone,
    emphasis: Emphasis,
    ring: Option<Hsla>,
    size: Size,
    children: Vec<AnyElement>,
}

impl Chip {
    #[must_use]
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self {
            label: Some(label.into()),
            ..Self::empty(id)
        }
    }

    #[must_use]
    pub fn empty(id: impl Into<ElementId>) -> Self {
        Self {
            plate: Plate::new(id, SurfaceRole::CHIP),
            look: ChipLook::Plain,
            label: None,
            dot: None,
            icon: None,
            detail: None,
            tone: Tone::Neutral,
            emphasis: Emphasis::Soft,
            ring: None,
            size: Size::Medium,
            children: Vec::new(),
        }
    }

    #[must_use]
    pub fn look(mut self, look: ChipLook) -> Self {
        self.look = look;
        self
    }

    #[must_use]
    pub fn state(mut self, state: PaintState) -> Self {
        self.plate.state = state;
        self
    }

    #[must_use]
    pub fn on(mut self, ground: Ground) -> Self {
        self.plate.surface = self.plate.surface.on(ground);
        self
    }

    #[must_use]
    pub fn ring(mut self, color: Hsla) -> Self {
        self.ring = Some(color);
        self
    }

    #[must_use]
    pub fn interactive(mut self, interactive: bool) -> Self {
        self.plate.interactive = interactive;
        self
    }

    #[must_use]
    pub fn dot(mut self, tone: Tone) -> Self {
        self.dot = Some(tone);
        self
    }

    #[must_use]
    pub fn icon(mut self, icon: impl Into<Icon>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    #[must_use]
    pub fn detail(mut self, detail: impl Into<SharedString>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    #[must_use]
    pub fn tone(mut self, tone: Tone) -> Self {
        self.tone = tone;
        self
    }

    #[must_use]
    pub fn emphasis(mut self, emphasis: Emphasis) -> Self {
        self.emphasis = emphasis;
        self
    }

    pub fn width(&self, window: &mut Window, cx: &App) -> Pixels {
        let (_, padding, text) = metrics(self.size);
        let rem = window.rem_size();
        let gap = Space::S3.rems().to_pixels(rem);
        let label = self.label.clone().unwrap_or_default();
        let mut font = ala::font(cx.theme().font_family.clone());
        font.weight = text.strong_weight();
        let line = window.text_system().shape_line(
            label.clone(),
            text.size().to_pixels(rem),
            &[TextRun {
                len: label.len(),
                font,
                color: cx.theme().foreground,
                background_color: None,
                underline: None,
                strikethrough: None,
            }],
            None,
        );
        let lead = if self.dot.is_some() {
            rems_from_px(DOT).to_pixels(rem) + gap
        } else if self.icon.is_some() {
            px(text.design_px() + 1.0) + gap
        } else {
            px(0.0)
        };
        line.width + padding.rems().to_pixels(rem) * 2.0 + lead
    }

    fn paint_surface(&self) -> Surface {
        let role = self.look.role();
        let role = match self.emphasis {
            Emphasis::Solid | Emphasis::Soft => role,
            Emphasis::Outline => role.with(role.patch.fill(Fill::None)),
            Emphasis::Ghost => role.with(role.patch.fill(Fill::None).outline(false)),
        };
        Surface::new(role)
            .tone(self.tone)
            .prominent(self.emphasis == Emphasis::Solid)
            .on_maybe(self.plate.surface.ground)
    }
}

impl Selectable for Chip {
    fn selected(mut self, selected: bool) -> Self {
        self.plate.state = if selected {
            PaintState::Selected
        } else {
            PaintState::Rest
        };
        self
    }

    fn is_selected(&self) -> bool {
        self.plate.state == PaintState::Selected
    }
}

impl Sizable for Chip {
    fn with_size(mut self, size: impl Into<Size>) -> Self {
        self.size = size.into();
        self
    }
}

impl ParentElement for Chip {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

plate_element!(Chip, children);

impl RenderOnce for Chip {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let surface = self.paint_surface();
        let Self {
            mut plate,
            label,
            dot: lead_dot,
            icon,
            detail,
            ring,
            size,
            children,
            ..
        } = self;
        let theme = cx.theme();
        let ground = theme.ground_at(surface, plate.state);
        let accent = theme.accent.opaque();
        let (height, padding, text) = metrics(size);
        plate.surface = surface;
        plate.base = plate
            .base
            .h_flex()
            .min_w_0()
            .h(rems_from_px(height))
            .px(padding)
            .gap(Space::S3);
        plate.children = [
            lead_dot.map(|tone| dot(tone, ground, cx).into_any_element()),
            icon.map(|icon| {
                icon.with_size(px(text.design_px() + 1.0))
                    .into_any_element()
            }),
            label.map(|label| {
                Label::new(text, label)
                    .strong()
                    .flex_none()
                    .whitespace_nowrap()
                    .into_any_element()
            }),
            detail.map(|detail| {
                Label::new(text, format!("· {detail}"))
                    .muted()
                    .on(ground)
                    .min_w_0()
                    .truncate()
                    .into_any_element()
            }),
        ]
        .into_iter()
        .flatten()
        .chain(children)
        .collect();
        plate.paint_with(cx, move |layer| {
            if let Some(ring) = ring {
                for shadow in &mut layer.shadow {
                    if shadow.color == accent {
                        shadow.color = ring;
                    }
                }
            }
        })
    }
}

const fn metrics(size: Size) -> (f32, Space, TextStyle) {
    match size {
        Size::Small => (20.0, Space::S4, TextStyle::Caption),
        Size::Medium | Size::Size(_) => (24.0, Space::S5, TextStyle::Small),
    }
}

#[derive(IntoElement)]
pub struct Dot {
    tone: Tone,
    ground: Option<Ground>,
}

impl Dot {
    #[must_use]
    pub fn new(tone: Tone) -> Self {
        Self { tone, ground: None }
    }

    #[must_use]
    pub fn on(mut self, ground: Ground) -> Self {
        self.ground = Some(ground);
        self
    }
}

impl RenderOnce for Dot {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let ground = self.ground.unwrap_or_else(|| cx.theme().ground());
        dot(self.tone, ground, cx)
    }
}

pub(crate) fn dot(tone: Tone, ground: Ground, cx: &App) -> Div {
    div()
        .flex_none()
        .size(rems_from_px(DOT))
        .rounded_full()
        .bg(dot_color(tone, ground, cx))
}

fn dot_color(tone: Tone, ground: Ground, cx: &App) -> Hsla {
    let theme = cx.theme();
    match tone {
        Tone::Neutral => theme
            .paint_on(ground, Tone::Neutral, Emphasis::Ghost)
            .muted_at(PaintState::Rest),
        tone => theme.tone(tone),
    }
}
