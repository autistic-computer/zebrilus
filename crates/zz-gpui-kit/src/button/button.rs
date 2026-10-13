//! The [`Button`] element and its variant/state color matrix.

use std::rc::Rc;

use zz_gpui::{
    AnyElement, App, ClickEvent, Corners, Div, Edges, ElementId, Hsla, InteractiveElement,
    Interactivity, IntoElement, MouseButton, ParentElement, Pixels, RenderOnce, Role, SharedString,
    Stateful, StatefulInteractiveElement as _, StyleRefinement, Styled, Window, div,
    prelude::FluentBuilder as _, px, relative, transparent_white,
};

use crate::{
    ActiveTheme as _, Colorize as _, Disableable, Elevation, Emphasis, FOCUS_RING_WIDTH, Ground,
    Icon, IconName, Paint, PaintState, Selectable, Sizable, Size, StyledExt as _, Tone,
    control_shadow, h_flex, rems_from_px, tooltip::Tooltip,
};

use super::button_icon::ButtonIcon;

pub const COMPACT_ICON_BUTTON_SIZE: f32 = 24.0;
const COMPACT_ICON_DROP: Pixels = px(0.5);

/// Corner radius of a [`Button`]: the theme radius, or an explicit override.
#[derive(Default, Clone, Copy)]
pub enum ButtonRounded {
    #[default]
    Medium,
    Size(Pixels),
}

impl From<Pixels> for ButtonRounded {
    fn from(px: Pixels) -> Self {
        ButtonRounded::Size(px)
    }
}

/// The variant selectors, as a trait so wrappers around a Button can forward
/// them (see [`ButtonVariant`] for the variants themselves).
pub trait ButtonVariants: Sized {
    fn with_variant(self, variant: ButtonVariant) -> Self;

    /// The main action of a surface: primary, solid.
    fn primary(self) -> Self {
        self.with_variant(ButtonVariant::Paint(Tone::Primary, Emphasis::Solid))
    }

    /// An action beside the main one: neutral, soft.
    fn secondary(self) -> Self {
        self.with_variant(ButtonVariant::Paint(Tone::Neutral, Emphasis::Soft))
    }

    fn danger(self) -> Self {
        self.with_variant(ButtonVariant::Paint(Tone::Danger, Emphasis::Soft))
    }

    fn warning(self) -> Self {
        self.with_variant(ButtonVariant::Paint(Tone::Warning, Emphasis::Soft))
    }

    fn success(self) -> Self {
        self.with_variant(ButtonVariant::Paint(Tone::Success, Emphasis::Soft))
    }

    /// A quiet action: neutral, ghost.
    fn ghost(self) -> Self {
        self.with_variant(ButtonVariant::Paint(Tone::Neutral, Emphasis::Ghost))
    }

    fn link(self) -> Self {
        self.with_variant(ButtonVariant::Link)
    }

    /// Unpadded, styled as plain text.
    fn text(self) -> Self {
        self.with_variant(ButtonVariant::Text)
    }
}

/// How a button is drawn: a tone at an emphasis, or one of the two
/// unpadded forms that read as text.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ButtonVariant {
    /// A tone at an emphasis, drawn from the theme's [`Paint`].
    Paint(Tone, Emphasis),
    /// Underlined text.
    Link,
    /// Plain text with no padding.
    Text,
}

impl Default for ButtonVariant {
    fn default() -> Self {
        Self::Paint(Tone::Neutral, Emphasis::Soft)
    }
}

impl ButtonVariant {
    #[inline]
    pub fn is_link(&self) -> bool {
        matches!(self, Self::Link)
    }

    #[inline]
    pub fn is_text(&self) -> bool {
        matches!(self, Self::Text)
    }

    #[inline]
    fn no_padding(&self) -> bool {
        self.is_link() || self.is_text()
    }

    /// The neutral soft button rests on the look's control surface (its
    /// hairline and shadow); the neutral ghost one shows it on hover.
    fn rests_on_surface(&self) -> bool {
        matches!(self, Self::Paint(Tone::Neutral, Emphasis::Soft))
    }

    fn neutral(&self) -> bool {
        matches!(
            self,
            Self::Paint(Tone::Neutral, Emphasis::Soft | Emphasis::Ghost)
        )
    }

    /// The paint, filled over the window when the button rests on the
    /// control surface, so the surface's drop shadow does not show through
    /// a translucent fill.
    fn paint(&self, on: Option<Ground>, cx: &App) -> Option<Paint> {
        match self {
            Self::Paint(tone, emphasis) => {
                let theme = cx.theme();
                let paint = theme.paint_on(on.unwrap_or_else(|| theme.ground()), *tone, *emphasis);
                Some(if self.rests_on_surface() {
                    paint.opaque()
                } else {
                    paint
                })
            }
            _ => None,
        }
    }

    fn style_at(&self, state: PaintState, on: Option<Ground>, cx: &App) -> ButtonVariantStyle {
        if let Some(paint) = self.paint(on, cx) {
            return ButtonVariantStyle {
                bg: paint.fill_at(state),
                border: paint.border_at(state),
                fg: paint.text_at(state),
                underline: false,
                shadow: self.rests_on_surface() && state != PaintState::Disabled,
            };
        }
        let foreground = cx.theme().foreground;
        ButtonVariantStyle {
            bg: cx.theme().transparent,
            border: cx.theme().transparent,
            fg: match (self, state) {
                (_, PaintState::Disabled) => foreground.faint(),
                (Self::Text, PaintState::Active) => foreground.muted(),
                _ => foreground,
            },
            underline: self.is_link(),
            shadow: false,
        }
    }
}

#[derive(IntoElement)]
pub struct Button {
    id: ElementId,
    base: Stateful<Div>,
    style: StyleRefinement,
    icon: Option<ButtonIcon>,
    label: Option<SharedString>,
    children: Vec<AnyElement>,
    disabled: bool,
    selected: bool,
    variant: ButtonVariant,
    on: Option<Ground>,
    hover_bg: Option<Hsla>,
    rounded: ButtonRounded,
    flat: bool,
    dropdown_caret: bool,
    size: Size,
    compact: bool,
    tooltip: Option<SharedString>,
    on_click: Option<Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>>,
    on_hover: Option<Rc<dyn Fn(&bool, &mut Window, &mut App)>>,
    loading: bool,

    tab_index: isize,
    tab_stop: bool,
    hit_slop: Option<(f32, f32)>,
}

impl From<Button> for AnyElement {
    fn from(button: Button) -> Self {
        button.into_any_element()
    }
}

impl Button {
    pub fn new(id: impl Into<ElementId>) -> Self {
        let id = id.into();

        Self {
            id: id.clone(),
            base: div().flex_shrink_0().id(id),
            style: StyleRefinement::default(),
            icon: None,
            label: None,
            disabled: false,
            selected: false,
            variant: ButtonVariant::default(),
            on: None,
            hover_bg: None,
            rounded: ButtonRounded::Medium,
            size: Size::Medium,
            tooltip: None,
            on_click: None,
            on_hover: None,
            loading: false,
            compact: false,
            flat: false,
            children: Vec::new(),
            dropdown_caret: false,
            tab_index: 0,
            tab_stop: true,
            hit_slop: None,
        }
    }

    pub fn compact_icon(id: impl Into<ElementId>, icon: impl Into<Icon>) -> Self {
        Self::new(id)
            .ghost()
            .compact()
            .size(rems_from_px(COMPACT_ICON_BUTTON_SIZE))
            .icon(icon.into().relative().top(COMPACT_ICON_DROP))
    }

    pub fn flat(mut self) -> Self {
        self.flat = true;
        self
    }

    /// Replace the variant's hovered fill. Setting `hover` from outside panics,
    /// because the variant matrix already claimed it.
    #[must_use]
    pub fn hover_bg(mut self, bg: Hsla) -> Self {
        self.hover_bg = Some(bg);
        self
    }

    pub fn rounded(mut self, rounded: impl Into<ButtonRounded>) -> Self {
        self.rounded = rounded.into();
        self
    }

    /// Set the label. A button with no label renders as an icon button.
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Set the leading icon.
    pub fn icon(mut self, icon: impl Into<Icon>) -> Self {
        self.icon = Some(ButtonIcon::new(icon));
        self
    }

    pub fn tooltip(mut self, tooltip: impl Into<SharedString>) -> Self {
        self.tooltip = Some(tooltip.into());
        self
    }

    pub fn loading(mut self, loading: bool) -> Self {
        self.loading = loading;
        self
    }

    pub fn compact(mut self) -> Self {
        self.compact = true;
        self
    }

    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Rc::new(handler));
        self
    }

    /// Called with `true` when the pointer enters, `false` when it leaves.
    pub fn on_hover(mut self, handler: impl Fn(&bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_hover = Some(Rc::new(handler));
        self
    }

    /// Defaults to 0.
    pub fn tab_index(mut self, tab_index: isize) -> Self {
        self.tab_index = tab_index;
        self
    }

    /// Defaults to true.
    pub fn tab_stop(mut self, tab_stop: bool) -> Self {
        self.tab_stop = tab_stop;
        self
    }

    #[must_use]
    pub fn hit_slop(mut self, horizontal: f32, vertical: f32) -> Self {
        self.hit_slop = Some((horizontal, vertical));
        self
    }

    pub fn dropdown_caret(mut self, dropdown_caret: bool) -> Self {
        self.dropdown_caret = dropdown_caret;
        self
    }

    #[inline]
    fn clickable(&self) -> bool {
        !(self.disabled || self.loading) && self.on_click.is_some()
    }

    #[inline]
    fn hoverable(&self) -> bool {
        !(self.disabled || self.loading) && self.on_hover.is_some()
    }
}

impl Button {
    /// Draw the button in `tone`, keeping its emphasis if it has one and
    /// soft otherwise.
    #[must_use]
    pub fn tone(mut self, tone: Tone) -> Self {
        let emphasis = match self.variant {
            ButtonVariant::Paint(_, emphasis) => emphasis,
            _ => Emphasis::default(),
        };
        self.variant = ButtonVariant::Paint(tone, emphasis);
        self
    }

    /// Draw the button at `emphasis`, keeping its tone if it has one and
    /// neutral otherwise.
    #[must_use]
    pub fn emphasis(mut self, emphasis: Emphasis) -> Self {
        let tone = match self.variant {
            ButtonVariant::Paint(tone, _) => tone,
            _ => Tone::default(),
        };
        self.variant = ButtonVariant::Paint(tone, emphasis);
        self
    }

    /// Paint the button on `ground`, the surface it sits on, instead of the
    /// window.
    #[must_use]
    pub fn on(mut self, ground: Ground) -> Self {
        self.on = Some(ground);
        self
    }
}

impl Disableable for Button {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl Selectable for Button {
    fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    fn is_selected(&self) -> bool {
        self.selected
    }
}

impl Sizable for Button {
    fn with_size(mut self, size: impl Into<Size>) -> Self {
        self.size = size.into();
        self
    }
}

impl ButtonVariants for Button {
    fn with_variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self
    }
}

impl Styled for Button {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl ParentElement for Button {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements)
    }
}

impl InteractiveElement for Button {
    fn interactivity(&mut self) -> &mut Interactivity {
        self.base.interactivity()
    }
}

fn with_focus_ring<T: ParentElement + Styled + Sized>(
    mut this: T,
    is_focused: bool,
    margins: Pixels,
    paint: &Paint,
    window: &Window,
) -> T {
    if !is_focused {
        return this;
    }

    let rem_size = window.rem_size();
    let style = this.style();

    let border_widths = Edges::<Pixels> {
        top: style
            .border_widths
            .top
            .map(|v| v.to_pixels(rem_size))
            .unwrap_or_default(),
        bottom: style
            .border_widths
            .bottom
            .map(|v| v.to_pixels(rem_size))
            .unwrap_or_default(),
        left: style
            .border_widths
            .left
            .map(|v| v.to_pixels(rem_size))
            .unwrap_or_default(),
        right: style
            .border_widths
            .right
            .map(|v| v.to_pixels(rem_size))
            .unwrap_or_default(),
    };

    let radius = Corners::<Pixels> {
        top_left: style
            .corner_radii
            .top_left
            .map(|v| v.to_pixels(rem_size))
            .unwrap_or_default(),
        top_right: style
            .corner_radii
            .top_right
            .map(|v| v.to_pixels(rem_size))
            .unwrap_or_default(),
        bottom_left: style
            .corner_radii
            .bottom_left
            .map(|v| v.to_pixels(rem_size))
            .unwrap_or_default(),
        bottom_right: style
            .corner_radii
            .bottom_right
            .map(|v| v.to_pixels(rem_size))
            .unwrap_or_default(),
    }
    .map(|v| *v + FOCUS_RING_WIDTH);

    let mut inner_style = StyleRefinement::default();
    inner_style.corner_radii.top_left = Some(radius.top_left.into());
    inner_style.corner_radii.top_right = Some(radius.top_right.into());
    inner_style.corner_radii.bottom_left = Some(radius.bottom_left.into());
    inner_style.corner_radii.bottom_right = Some(radius.bottom_right.into());

    let inset = FOCUS_RING_WIDTH + margins;

    this.child(
        div()
            .flex_none()
            .absolute()
            .top(-(inset + border_widths.top))
            .left(-(inset + border_widths.left))
            .right(-(inset + border_widths.right))
            .bottom(-(inset + border_widths.bottom))
            .focus_ring(paint)
            .refine_style(&inner_style),
    )
}

impl RenderOnce for Button {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let style: ButtonVariant = self.variant;
        let on = self.on;
        let ground = on.unwrap_or_else(|| cx.theme().ground());
        let clickable = self.clickable();
        let is_disabled = self.disabled;
        let hoverable = self.hoverable();
        let normal_style = style.style_at(PaintState::Rest, on, cx);
        let paint = style.paint(on, cx);
        let neutral_surface = !self.flat && style.neutral();
        let resting_surface = neutral_surface && normal_style.shadow;
        let highlight_ring = neutral_surface && !normal_style.shadow;
        let casts_shadow = (highlight_ring || resting_surface)
            && cx.theme().control_shadows()
            && ground.elevation == Elevation::Window;
        let lifted = move |fill: Hsla| {
            if casts_shadow {
                ground.fill.blend(fill)
            } else {
                fill
            }
        };
        let icon_size = match self.size {
            Size::Size(v) => Size::Size(v * 0.75),
            _ => self.size,
        };

        let focus_handle = window
            .use_keyed_state(self.id.clone(), cx, |_, cx| cx.focus_handle())
            .read(cx)
            .clone();
        let is_focused = focus_handle.is_focused(window);

        let rounding = match self.rounded {
            ButtonRounded::Medium => cx.theme().control_radius(),
            ButtonRounded::Size(px) => px,
        };
        let coarse = crate::touch::CoarsePointer::get(cx);
        let slop = self.hit_slop.filter(|_| coarse);
        let press = (coarse && clickable)
            .then(|| crate::touch::press_feedback(self.id.clone(), window, cx));
        let pressed = press.as_ref().map_or(0.0, |press| press.amount);
        let press_listener = press.map(crate::touch::PressFeedback::listener);
        let press_fill = style.style_at(PaintState::Active, on, cx).bg;
        let on_mouse_down =
            move |_: &zz_gpui::MouseDownEvent, window: &mut Window, cx: &mut App| {
                if is_disabled {
                    cx.stop_propagation();
                    return;
                }

                window.prevent_default();

                crate::text::suppress_text_selection(cx);
            };
        let on_click = self.on_click.map(|on_click| {
            move |event: &ClickEvent, window: &mut Window, cx: &mut App| {
                if !clickable {
                    cx.stop_propagation();
                    return;
                }

                on_click(event, window, cx);
            }
        });

        let element = self
            .base
            .role(if self.variant.is_link() {
                Role::Link
            } else {
                Role::Button
            })
            .when_some(
                self.label.as_ref().or(self.tooltip.as_ref()),
                |this, label| this.aria_label(label.clone()),
            )
            .aria_selected(self.selected)
            .aria_disabled(is_disabled)
            .when(!self.disabled, |this| {
                this.track_focus(
                    &focus_handle
                        .tab_index(self.tab_index)
                        .tab_stop(self.tab_stop),
                )
            })
            .cursor_default()
            .flex()
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .cursor_default()
            .when(self.variant.is_link(), |this| this.cursor_pointer())
            .when(
                cx.theme().control_shadows()
                    && normal_style.shadow
                    && !self.flat
                    && !resting_surface
                    && ground.elevation == Elevation::Window,
                |this| this.shadow(control_shadow(cx)),
            )
            .when(!style.no_padding(), |this| {
                let height = self.size.control_h();
                if self.label.is_none() && self.children.is_empty() {
                    this.size(height)
                } else {
                    let this = this.h(height);
                    match self.size {
                        Size::Size(size) => this.px(size * 0.2),
                        Size::Small => this
                            .px_2()
                            .when(self.compact, |this| this.min_w(height).px_1()),
                        Size::Medium => this
                            .px_3()
                            .when(self.compact, |this| this.min_w(height).px_1p5()),
                    }
                }
            })
            .rounded(rounding)
            .when(highlight_ring && cx.theme().outlined(), |this| {
                this.border(cx.theme().outline)
            })
            .when_some(
                paint.filter(|paint| paint.border_width > px(0.0)),
                |this, paint| this.border(paint.border_width),
            )
            .text_color(normal_style.fg)
            .when(self.selected, |this| {
                let selected_style = style.style_at(PaintState::Current, on, cx);
                this.bg(lifted(selected_style.bg))
                    .border_color(selected_style.border)
                    .text_color(selected_style.fg)
                    .when(highlight_ring || resting_surface, |this| {
                        this.control_highlight_on(ground, cx)
                    })
            })
            .when(!self.disabled && !self.selected, |this| {
                this.border_color(if highlight_ring {
                    transparent_white()
                } else {
                    normal_style.border
                })
                .bg(if resting_surface {
                    lifted(normal_style.bg)
                } else {
                    normal_style.bg
                })
                .when(normal_style.underline, |this| this.text_decoration_1())
                .when(!coarse, |this| {
                    this.hover(|this| {
                        let hover_style = style.style_at(PaintState::Hover, on, cx);
                        let this = this
                            .bg(self.hover_bg.unwrap_or(lifted(hover_style.bg)))
                            .border_color(hover_style.border)
                            .text_color(hover_style.fg);
                        if highlight_ring || resting_surface {
                            this.control_highlight_on(ground, cx)
                        } else {
                            this
                        }
                    })
                })
                .when(!coarse, |this| {
                    this.active(|this| {
                        let active_style = style.style_at(PaintState::Active, on, cx);
                        let this = this
                            .bg(lifted(active_style.bg))
                            .border_color(active_style.border)
                            .text_color(active_style.fg);
                        if highlight_ring || resting_surface {
                            this.control_highlight_on(ground, cx)
                        } else {
                            this
                        }
                    })
                })
            })
            .when(resting_surface, |this| this.control_surface_on(ground, cx))
            .when(self.disabled, |this| {
                let disabled_style = style.style_at(PaintState::Disabled, on, cx);
                this.bg(disabled_style.bg)
                    .text_color(disabled_style.fg)
                    .border_color(if highlight_ring {
                        transparent_white()
                    } else {
                        disabled_style.border
                    })
                    .shadow_none()
            })
            .refine_style(&self.style)
            .when(pressed > 0.0, |this| {
                this.child(
                    div()
                        .absolute()
                        .inset_0()
                        .rounded(rounding)
                        .bg(press_fill)
                        .opacity(pressed),
                )
            })
            .map(|this| match slop {
                None => this
                    .on_mouse_down(MouseButton::Left, on_mouse_down)
                    .when_some(on_click, |this, on_click| this.on_click(on_click))
                    .children(press_listener),
                Some((x, y)) => this.child(
                    crate::touch::hit_area("touch", x, y)
                        .children(press_listener)
                        .on_mouse_down(MouseButton::Left, on_mouse_down)
                        .when_some(on_click, |this, on_click| {
                            this.on_click(move |event, window, cx| {
                                on_click(event, window, cx);
                                cx.stop_propagation();
                            })
                        }),
                ),
            })
            .when_some(self.on_hover.filter(|_| hoverable), |this, on_hover| {
                this.on_hover(move |hovered, window, cx| {
                    on_hover(hovered, window, cx);
                })
            })
            .child({
                let label = h_flex()
                    .id("label")
                    .size_full()
                    .items_center()
                    .justify_center();

                label
                    .text_size(self.size.control_text())
                    .when(pressed > 0.0, |this| this.opacity(1.0 - 0.35 * pressed))
                    .map(|this| match self.size {
                        Size::Small => this.gap_1(),
                        _ => this.gap_1p5(),
                    })
                    .when_some(self.icon, |this, icon| {
                        this.child(icon.loading(self.loading).with_size(icon_size))
                    })
                    .when_some(self.label, |this, label| {
                        this.child(div().flex_none().line_height(relative(1.)).child(label))
                    })
                    .children(self.children)
                    .when(self.dropdown_caret, |this| {
                        this.justify_between().child(
                            Icon::new(IconName::ChevronDown).small().text_color(
                                match self.disabled {
                                    true => normal_style.fg.opacity(0.3),
                                    false => normal_style.fg.opacity(0.5),
                                },
                            ),
                        )
                    })
            })
            .when(self.loading && !self.disabled, |this| {
                this.bg(normal_style.bg.opacity(0.8))
                    .border_color(normal_style.border.opacity(0.8))
                    .text_color(normal_style.fg.opacity(0.8))
            })
            .when_some(self.tooltip, |this, tooltip| {
                this.tooltip(move |window, cx| Tooltip::new(tooltip.clone()).build(window, cx))
            });

        let focus_paint =
            paint.unwrap_or_else(|| cx.theme().paint_on(ground, Tone::Neutral, Emphasis::Ghost));
        with_focus_ring(element, is_focused, px(0.), &focus_paint, window)
    }
}

struct ButtonVariantStyle {
    bg: Hsla,
    border: Hsla,
    fg: Hsla,
    underline: bool,
    shadow: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[zz_gpui::test]
    fn test_button_clickable_logic(_cx: &mut zz_gpui::TestAppContext) {
        let clickable = Button::new("test").on_click(|_, _, _| {});
        assert!(clickable.clickable());

        let disabled = Button::new("test").disabled(true).on_click(|_, _, _| {});
        assert!(!disabled.clickable());

        let loading = Button::new("test").loading(true).on_click(|_, _, _| {});
        assert!(!loading.clickable());
    }

    #[zz_gpui::test]
    fn test_button_hoverable_logic(_cx: &mut zz_gpui::TestAppContext) {
        assert!(!Button::new("test").hoverable());

        assert!(Button::new("test").on_hover(|_, _, _| {}).hoverable());

        assert!(
            !Button::new("test")
                .disabled(true)
                .on_hover(|_, _, _| {})
                .hoverable()
        );

        assert!(
            !Button::new("test")
                .loading(true)
                .on_hover(|_, _, _| {})
                .hoverable()
        );
    }

    #[zz_gpui::test]
    fn test_button_variant_methods(_cx: &mut zz_gpui::TestAppContext) {
        assert!(ButtonVariant::Link.is_link());
        assert!(ButtonVariant::Text.is_text());

        assert!(ButtonVariant::Link.no_padding());
        assert!(ButtonVariant::Text.no_padding());
        assert!(!ButtonVariant::default().no_padding());
        assert!(ButtonVariant::default().rests_on_surface());
    }

    #[zz_gpui::test]
    fn the_shorthands_name_a_tone_and_an_emphasis(_cx: &mut zz_gpui::TestAppContext) {
        let variant = |button: Button| button.variant;
        assert_eq!(
            variant(Button::new("b").primary()),
            ButtonVariant::Paint(Tone::Primary, Emphasis::Solid)
        );
        assert_eq!(
            variant(Button::new("b").ghost()),
            ButtonVariant::Paint(Tone::Neutral, Emphasis::Ghost)
        );
        assert_eq!(
            variant(Button::new("b").danger().emphasis(Emphasis::Outline)),
            ButtonVariant::Paint(Tone::Danger, Emphasis::Outline)
        );
    }
}
