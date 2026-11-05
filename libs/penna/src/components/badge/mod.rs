use crate::Space;
use ala::{
    AnyElement, App, IntoElement, ParentElement, RenderOnce, SharedString, StyleRefinement, Styled,
    Window, prelude::FluentBuilder as _,
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
        let (line, text, padding) = match self.size.rung() {
            Size::XSmall | Size::Small => (TextStyle::Small, TextStyle::Micro, Space::S4),
            _ => (TextStyle::Body, TextStyle::Caption, Space::S6),
        };
        let layer = self.surface_layer(cx.theme());
        h_flex()
            .flex_none()
            .h(line.line_height())
            .gap(Space::S4)
            .px(padding)
            .layer(&layer)
            .type_style_strong(text, cx)
            .whitespace_nowrap()
            .refine_style(&self.style)
            .children(self.icon.map(|icon| icon.with_size(Size::XSmall)))
            .when(!self.label.is_empty(), |badge| badge.child(self.label))
            .children(self.children)
    }
}

#[cfg(test)]
mod tests {
    use ala::{Context, InteractiveElement as _, Render, TestAppContext, px, size};

    use super::*;
    use crate::{IconName, v_flex};

    fn ghost() -> Badge {
        Badge::empty()
            .icon(IconName::Check)
            .emphasis(Emphasis::Ghost)
    }

    struct Badges;

    impl Render for Badges {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            v_flex()
                .items_start()
                .child(h_flex().debug_selector(|| "medium".into()).child(ghost()))
                .child(
                    h_flex()
                        .debug_selector(|| "small".into())
                        .child(ghost().small()),
                )
        }
    }

    #[ala::test]
    fn a_badge_and_its_icon_grow_with_the_root_rem(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let (_, cx) = cx.add_window_view(|_, _| Badges);
        cx.update(|window, cx| {
            window.set_rem_size(px(20.));
            window.draw(cx).clear(cx);
        });
        let medium = cx.debug_bounds("medium").map(|bounds| bounds.size);
        let small = cx.debug_bounds("small").map(|bounds| bounds.size);
        assert_eq!(medium, Some(size(px(30.), px(22.5))));
        assert_eq!(small, Some(size(px(25.), px(20.))));
    }
}
