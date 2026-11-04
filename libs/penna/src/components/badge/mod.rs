use crate::Space;
use ala::{
    AnyElement, App, IntoElement, ParentElement, RenderOnce, SharedString, StyleRefinement, Styled,
    Window, prelude::FluentBuilder as _, px,
};

use crate::{
    ActiveTheme as _, Emphasis, Fill, Ground, Icon, Layer, PaintState, Sizable, Size,
    StyledExt as _, Surface, SurfaceRole, TextStyle, Theme, Tone, h_flex,
};

#[derive(IntoElement)]
pub struct Badge {
    label: SharedString,
    icon: Option<Icon>,
    tone: Tone,
    emphasis: Emphasis,
    ground: Option<Ground>,
    opaque: bool,
    size: Size,
    style: StyleRefinement,
    children: Vec<AnyElement>,
}

impl Badge {
    #[must_use]
    pub fn new(label: impl Into<SharedString>) -> Self {
        Self {
            label: label.into(),
            icon: None,
            tone: Tone::default(),
            emphasis: Emphasis::default(),
            ground: None,
            opaque: false,
            size: Size::default(),
            style: StyleRefinement::default(),
            children: Vec::new(),
        }
    }

    #[must_use]
    pub fn empty() -> Self {
        Self::new(SharedString::default())
    }

    #[must_use]
    pub fn opaque(mut self) -> Self {
        self.opaque = true;
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

    #[must_use]
    pub fn on(mut self, ground: Ground) -> Self {
        self.ground = Some(ground);
        self
    }

    fn surface_layer(&self, theme: &Theme) -> Layer {
        let role = SurfaceRole::BADGE;
        let role = match self.emphasis {
            Emphasis::Solid | Emphasis::Soft => role,
            Emphasis::Outline => SurfaceRole::OUTLINE_BADGE,
            Emphasis::Ghost => role
                .with(role.patch.fill(Fill::None).outline(false))
                .fixed_emphasis(),
        };
        let ground = self.ground.unwrap_or_else(|| theme.ground());
        let surface = Surface::new(role)
            .tone(self.tone)
            .prominent(self.emphasis == Emphasis::Solid)
            .on(ground);
        let mut layer = theme.layer(surface, PaintState::Rest);
        if self.opaque {
            layer.fill = layer.ground.fill;
        }
        layer
    }

    #[must_use]
    pub fn icon(mut self, icon: impl Into<Icon>) -> Self {
        self.icon = Some(icon.into());
        self
    }
}

impl Sizable for Badge {
    fn with_size(mut self, size: impl Into<Size>) -> Self {
        self.size = size.into();
        self
    }
}

impl ParentElement for Badge {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl Styled for Badge {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Badge {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let (height, text, padding) = match self.size {
            Size::XSmall | Size::Small => (16.0, TextStyle::Micro, Space::S4),
            _ => (18.0, TextStyle::Caption, Space::S6),
        };
        let layer = self.surface_layer(cx.theme());
        h_flex()
            .flex_none()
            .h(px(height))
            .gap(Space::S4)
            .px(padding)
            .layer(&layer)
            .type_style_strong(text, cx)
            .whitespace_nowrap()
            .refine_style(&self.style)
            .children(self.icon.map(|icon| icon.size(px(text.design_px() + 1.0))))
            .when(!self.label.is_empty(), |badge| badge.child(self.label))
            .children(self.children)
    }
}
