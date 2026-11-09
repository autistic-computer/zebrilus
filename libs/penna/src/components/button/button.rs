use crate::Space;
use std::rc::Rc;

use ala::{
    AnyElement, App, ClickEvent, Div, ElementId, FULL_CORNER_RADIUS, Hsla, InteractiveElement,
    Interactivity, IntoElement, Keystroke, MouseButton, ParentElement, Pixels, RenderOnce, Role,
    SharedString, Stateful, StatefulInteractiveElement as _, StyleRefinement, Styled, Window, div,
    prelude::FluentBuilder as _, px,
};

use crate::{
    ActiveTheme as _, AuthoredElement as _, Colorize as _, Disableable, Emphasis, Ground, Icon,
    IconName, Kind, Layer, PaintState, Selectable, Sizable, Size, Smoothing, StyledExt as _,
    Surface, SurfaceRole, Tone,
    components::tooltip::Tooltip,
    components::{input::trailing_size, spinner::Spinner},
    foundation::roles,
    h_flex, rems_from_px,
};

use super::button_icon::ButtonIcon;

pub const COMPACT_ICON_BUTTON_SIZE: f32 = 24.0;
const COMPACT_ICON_DROP: Pixels = px(0.5);
pub const INSIDE_GAP: Pixels = px(4.0);

#[derive(Default, Clone, Copy)]
pub enum ButtonRounded {
    #[default]
    Medium,
    Size(Pixels),
    Inside(Surface, Pixels),
}

impl From<Pixels> for ButtonRounded {
    fn from(px: Pixels) -> Self {
        ButtonRounded::Size(px)
    }
}

pub trait ButtonVariants: Sized {
    fn with_variant(self, variant: ButtonVariant) -> Self;

    fn primary(self) -> Self {
        self.with_variant(ButtonVariant::Paint(Tone::Primary, Emphasis::Solid))
    }

    fn secondary(self) -> Self {
        self.with_variant(ButtonVariant::Paint(Tone::Neutral, Emphasis::Soft))
    }

    fn danger(self) -> Self {
        self.with_variant(ButtonVariant::Paint(Tone::Danger, Emphasis::Soft))
    }

    fn ghost(self) -> Self {
        self.with_variant(ButtonVariant::Paint(Tone::Neutral, Emphasis::Ghost))
    }

    fn link(self) -> Self {
        self.with_variant(ButtonVariant::Link)
    }

    fn text(self) -> Self {
        self.with_variant(ButtonVariant::Text)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ButtonLook {
    Icon,
    Quiet,
    Flat,
    Chrome,
    Bar,
    Toggle,
    RowIcon,
    Floating,
    Row,
    Send,
    FloatingChip,
    Tray,
}

impl ButtonLook {
    const ALL: [Self; 12] = [
        Self::Icon,
        Self::Quiet,
        Self::Flat,
        Self::Chrome,
        Self::Bar,
        Self::Toggle,
        Self::RowIcon,
        Self::Floating,
        Self::Row,
        Self::Send,
        Self::FloatingChip,
        Self::Tray,
    ];

    const fn role(self) -> SurfaceRole {
        match self {
            Self::Icon => SurfaceRole::ICON_BUTTON,
            Self::Quiet => SurfaceRole::QUIET_BUTTON,
            Self::Flat => SurfaceRole::FLAT_ICON_BUTTON,
            Self::Chrome => roles::CHROME_BUTTON,
            Self::Bar => roles::ICON_BUTTON_BAR,
            Self::Toggle => roles::ICON_BUTTON_TOGGLE,
            Self::RowIcon => roles::ICON_BUTTON_ROW,
            Self::Floating => roles::ICON_BUTTON_FLOATING,
            Self::Row => roles::BUTTON_ROW,
            Self::Send => roles::BUTTON_SEND,
            Self::FloatingChip => roles::CHIP_FLOATING,
            Self::Tray => roles::TAB_TRAY,
        }
    }
}

pub(crate) fn rings_focus(role: SurfaceRole) -> bool {
    [
        Kind::Button.role(),
        SurfaceRole::OUTLINE_BUTTON,
        roles::PHONE_BUTTON,
        SurfaceRole::MENU_ITEM,
        roles::MENU_ITEM_CARD,
        roles::MENU_ITEM_TINT,
        roles::SIDEBAR_ROW,
        roles::ROW_NAV,
        roles::ROW_CURSOR,
        roles::TAB_WINDOW,
        roles::TAB_PAGE,
        roles::BUTTON_TITLE,
        SurfaceRole::CHIP,
        SurfaceRole::OUTLINE_CHIP,
        roles::CHIP_BAR,
        roles::CHIP_MARKER,
        roles::CHIP_DRAG,
        roles::BADGE_ICON,
        SurfaceRole::CARD_TILE,
        SurfaceRole::SCREEN_TILE,
        roles::TILE_MINI,
    ]
    .into_iter()
    .chain(ButtonLook::ALL.into_iter().map(ButtonLook::role))
    .any(|drawn| drawn.id == role.id)
}

impl From<ButtonLook> for Surface {
    fn from(look: ButtonLook) -> Self {
        Self::new(look.role())
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ButtonVariant {
    Paint(Tone, Emphasis),
    Link,
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

    fn surface(
        &self,
        role: Option<SurfaceRole>,
        icon_only: bool,
        on: Option<Ground>,
    ) -> Option<Surface> {
        let Self::Paint(tone, emphasis) = *self else {
            return None;
        };
        let role = role.unwrap_or(match emphasis {
            Emphasis::Ghost if icon_only => SurfaceRole::ICON_BUTTON,
            Emphasis::Ghost => SurfaceRole::QUIET_BUTTON,
            Emphasis::Outline => SurfaceRole::OUTLINE_BUTTON,
            Emphasis::Solid | Emphasis::Soft => Kind::Button.role(),
        });
        Some(
            Surface::new(role)
                .tone(tone)
                .prominent(emphasis == Emphasis::Solid)
                .on_maybe(on),
        )
    }
}

fn text_layer(variant: ButtonVariant, state: PaintState, ground: Ground, cx: &App) -> Layer {
    let theme = cx.theme();
    let foreground = theme.foreground;
    Layer {
        fill: theme.transparent,
        glass: None,
        border: theme.transparent,
        border_width: px(0.0),
        shadow: Vec::new(),
        mark: None,
        text: match (variant, state) {
            (_, PaintState::Disabled) => foreground.faint(),
            (ButtonVariant::Text, PaintState::Active) => foreground.muted(),
            _ => foreground,
        },
        radius: px(0.0),
        share: None,
        smoothing: crate::CORNER_SMOOTHING,
        ground,
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
    surface_role: Option<SurfaceRole>,
    on: Option<Ground>,
    hover_bg: Option<Hsla>,
    rounded: ButtonRounded,
    flat: bool,
    dropdown_caret: bool,
    size: Size,
    compact: bool,
    tooltip: Option<SharedString>,
    tooltip_keys: Vec<Keystroke>,
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
            base: div().flex_shrink_0().author_id(id),
            style: StyleRefinement::default(),
            icon: None,
            label: None,
            disabled: false,
            selected: false,
            variant: ButtonVariant::default(),
            surface_role: None,
            on: None,
            hover_bg: None,
            rounded: ButtonRounded::Medium,
            size: Size::Medium,
            tooltip: None,
            tooltip_keys: Vec::new(),
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

    #[must_use]
    pub fn inside(self, around: impl Into<Surface>, height: Pixels) -> Self {
        let side = height - INSIDE_GAP * 2.0;
        let icon = (side - px(8.0)).clamp(px(10.0), px(14.0));
        self.rounded(ButtonRounded::Inside(around.into(), height))
            .with_size(Size::Size(icon / 0.75))
            .size(side)
    }

    pub fn flat(mut self) -> Self {
        self.flat = true;
        self
    }

    #[must_use]
    pub fn hover_bg(mut self, bg: Hsla) -> Self {
        self.hover_bg = Some(bg);
        self
    }

    pub fn rounded(mut self, rounded: impl Into<ButtonRounded>) -> Self {
        self.rounded = rounded.into();
        self
    }

    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn icon(mut self, icon: impl Into<Icon>) -> Self {
        self.icon = Some(ButtonIcon::new(icon));
        self
    }

    pub fn tooltip(mut self, tooltip: impl Into<SharedString>) -> Self {
        self.tooltip = Some(tooltip.into());
        self
    }

    #[must_use]
    pub fn tooltip_keys(mut self, keys: impl IntoIterator<Item = Keystroke>) -> Self {
        self.tooltip_keys = keys.into_iter().collect();
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

    pub fn on_hover(mut self, handler: impl Fn(&bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_hover = Some(Rc::new(handler));
        self
    }

    pub fn tab_index(mut self, tab_index: isize) -> Self {
        self.tab_index = tab_index;
        self
    }

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
    #[must_use]
    pub fn tone(mut self, tone: Tone) -> Self {
        let emphasis = match self.variant {
            ButtonVariant::Paint(_, emphasis) => emphasis,
            _ => Emphasis::default(),
        };
        self.variant = ButtonVariant::Paint(tone, emphasis);
        self
    }

    #[must_use]
    pub fn emphasis(mut self, emphasis: Emphasis) -> Self {
        let tone = match self.variant {
            ButtonVariant::Paint(tone, _) => tone,
            _ => Tone::default(),
        };
        self.variant = ButtonVariant::Paint(tone, emphasis);
        self
    }

    #[must_use]
    pub(crate) fn surface_role(mut self, role: SurfaceRole) -> Self {
        self.surface_role = Some(role);
        self
    }

    #[must_use]
    pub fn look(self, look: ButtonLook) -> Self {
        self.surface_role(look.role())
    }

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

impl RenderOnce for Button {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let variant = self.variant;
        let ground = self.on.unwrap_or_else(|| cx.theme().ground());
        let icon_only = self.label.is_none() && self.children.is_empty();
        let surface = variant.surface(self.surface_role, icon_only, self.on);
        let flat = self.flat;
        let inactive = if cfg!(target_family = "wasm") || window.is_window_active() || self.selected
        {
            0.0
        } else {
            cx.theme()
                .glass
                .map_or(0.0, |glass| glass.inactive_flattening.clamp(0.0, 1.0))
        };
        let layer_at = move |state: PaintState, cx: &App| -> Layer {
            let Some(surface) = surface else {
                return text_layer(variant, state, ground, cx);
            };
            let mut layer = cx.theme().layer(surface, state);
            if flat {
                layer.border = cx.theme().transparent;
                layer.shadow.clear();
            }
            if inactive > 0.0
                && matches!(variant, ButtonVariant::Paint(Tone::Neutral, _))
                && (layer.glass.is_some() || ground.glass)
            {
                layer.text = layer.text.opacity(1.0 - 0.65 * inactive);
            }
            layer
        };
        let clickable = self.clickable();
        let is_disabled = self.disabled;
        let hoverable = self.hoverable();
        let rest = layer_at(
            if self.disabled {
                PaintState::Disabled
            } else if self.selected {
                PaintState::Current
            } else {
                PaintState::Rest
            },
            cx,
        );
        let icon_size = match self.size {
            Size::Size(v) => Size::Size(v * 0.75),
            _ => self.size,
        };

        let focus_handle = window
            .use_keyed_state(self.id.clone(), cx, |_, cx| cx.focus_handle())
            .read(cx)
            .clone();
        let is_focused = focus_handle.is_focused(window);

        let row = surface.is_some_and(|surface| surface.role.row);
        let (rounding, smoothing) = match self.rounded {
            ButtonRounded::Medium if row => (
                crate::foundation::row_radius(
                    &rest,
                    match self.size {
                        Size::Size(height) => height,
                        size => crate::rems_from_px(size.control_px()).to_pixels(window.rem_size()),
                    },
                ),
                rest.smoothing,
            ),
            ButtonRounded::Medium => (rest.radius, rest.smoothing),
            ButtonRounded::Size(radius) if radius >= FULL_CORNER_RADIUS => {
                (radius, Smoothing::CIRCLE.exponent())
            }
            ButtonRounded::Size(radius) => (
                radius,
                surface.map_or(crate::CORNER_SMOOTHING, |surface| {
                    cx.theme().style_of(surface).smoothing.exponent()
                }),
            ),
            ButtonRounded::Inside(around, height) => {
                let container = cx.theme().layer(around, PaintState::Rest);
                let radius = crate::blocks::touch::concentric(
                    crate::foundation::row_radius(&container, height),
                    INSIDE_GAP,
                );
                if radius >= FULL_CORNER_RADIUS || radius * 2.0 >= height - INSIDE_GAP * 2.0 {
                    (FULL_CORNER_RADIUS, Smoothing::CIRCLE.exponent())
                } else {
                    (radius, container.smoothing)
                }
            }
        };
        let coarse = crate::blocks::touch::CoarsePointer::get(cx);
        let slop = self.hit_slop.filter(|_| coarse);
        let press = (coarse && clickable)
            .then(|| crate::blocks::touch::press_feedback(self.id.clone(), window, cx));
        let pressed = press.as_ref().map_or(0.0, |press| press.amount);
        let press_listener = press.map(crate::blocks::touch::PressFeedback::listener);
        let press_fill = layer_at(PaintState::Active, cx).fill;
        let hover = layer_at(PaintState::Hover, cx);
        let active = layer_at(PaintState::Active, cx);
        let hover_bg = self.hover_bg;
        let no_icon = self.icon.is_none();
        let plain_label = no_icon
            && !self.loading
            && self.children.is_empty()
            && !self.dropdown_caret
            && pressed == 0.0;
        let on_mouse_down = move |_: &ala::MouseDownEvent, window: &mut Window, cx: &mut App| {
            if is_disabled {
                cx.stop_propagation();
                return;
            }

            window.prevent_default();

            crate::components::text::suppress_text_selection(cx);
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
            .map(|this| {
                if is_disabled {
                    this.cursor_default()
                } else {
                    this.click_cursor()
                }
            })
            .flex()
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .when(!variant.no_padding(), |this| {
                let height = self.size.control_h();
                if icon_only {
                    this.size(height)
                } else {
                    let this = this.h(height);
                    match self.size {
                        Size::Size(size) => this.px(size * 0.2),
                        size if self.compact => this.min_w(height).px(size.icon_gap()),
                        size => this.px(size.inset()),
                    }
                }
            })
            .rounded(rounding)
            .when(row, |this| {
                this.corner_radius_mode(ala::CornerRadiusMode::Fixed)
            })
            .when_some(
                rest.share
                    .filter(|_| !row && matches!(self.rounded, ButtonRounded::Medium)),
                Styled::corner_share,
            )
            .corner_smoothing(smoothing)
            .border(rest.border_width)
            .border_color(rest.border)
            .shadow(rest.shadow.clone())
            .when_some(rest.mark, Styled::edge_mark)
            .text_color(rest.text)
            .type_style(self.size.control_text(), cx)
            .map(|this| match rest.glass {
                Some(material) => this.glass(material),
                None => this.bg(rest.fill),
            })
            .when(variant.is_link() && !self.disabled, |this| {
                this.text_decoration_1()
            })
            .hover({
                let hoverable = !self.disabled && !self.selected && !coarse;
                move |style| match (hoverable, hover_bg) {
                    (false, _) => style,
                    (true, Some(bg)) => style
                        .border_color(hover.border)
                        .shadow(hover.shadow.clone())
                        .text_color(hover.text)
                        .bg(bg),
                    (true, None) => crate::foundation::restyle(style, &hover),
                }
            })
            .when(!self.disabled && !self.selected && !coarse, |this| {
                this.active(move |style| crate::foundation::restyle(style, &active))
            })
            .refine_style(&self.style)
            .when(pressed > 0.0, |this| {
                this.child(
                    div()
                        .absolute()
                        .inset_0()
                        .rounded(rounding)
                        .corner_smoothing(smoothing)
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
                    crate::blocks::touch::hit_area("touch", x, y)
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
            .map(|this| match (plain_label, self.label) {
                (true, Some(label)) => this.child(div().flex_none().child(label)),
                (_, text) => this.child({
                    let label = h_flex()
                        .id("label")
                        .size_full()
                        .items_center()
                        .justify_center();

                    label
                        .when(pressed > 0.0, |this| this.opacity(1.0 - 0.35 * pressed))
                        .gap(match self.size {
                            Size::Size(_) => Space::S6,
                            size => size.icon_gap(),
                        })
                        .when_some(self.icon, |this, icon| {
                            this.child(icon.loading(self.loading).with_size(icon_size))
                        })
                        .when(self.loading && no_icon, |this| {
                            this.child(Spinner::new().with_size(icon_size))
                        })
                        .when_some(text, |this, label| {
                            this.child(div().flex_none().child(label))
                        })
                        .children(self.children)
                        .when(self.dropdown_caret, |this| {
                            this.justify_between().child(
                                Icon::new(IconName::ChevronDown)
                                    .with_size(trailing_size(self.size))
                                    .text_color(match self.disabled {
                                        true => rest.text.opacity(0.3),
                                        false => rest.text.opacity(0.5),
                                    }),
                            )
                        })
                }),
            })
            .when(self.loading && !self.disabled, |this| {
                this.bg(rest.fill.opacity(0.8))
                    .border_color(rest.border.opacity(0.8))
                    .text_color(rest.text.opacity(0.8))
            })
            .when_some(self.tooltip.clone(), |this, tooltip| {
                let keys = self.tooltip_keys.clone();
                this.tooltip(move |window, cx| {
                    Tooltip::new(tooltip.clone())
                        .keys(keys.clone())
                        .build(window, cx)
                })
            });
        let element = crate::components::tooltip::with_focus_tooltip(
            element,
            &self.id,
            is_focused,
            self.tooltip,
            &self.tooltip_keys,
            window,
            cx,
        );

        let tone = match variant {
            ButtonVariant::Paint(tone, _) => tone,
            _ => Tone::Neutral,
        };
        let focus_paint = cx.theme().paint_on(ground, tone, Emphasis::Ghost);
        crate::foundation::focus::with_focus_ring(
            element,
            is_focused,
            px(0.),
            &focus_paint,
            smoothing,
            window,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[ala::test]
    fn a_selected_button_keeps_tracking_hover(cx: &mut ala::TestAppContext) {
        use ala::{
            Context, IntoElement, Modifiers, ParentElement as _, Render, Styled as _, point, px,
        };

        struct Host {
            selected: bool,
        }

        impl Render for Host {
            fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
                div().p(Space::S10).child(
                    Button::new("trigger")
                        .label("Trigger")
                        .selected(self.selected),
                )
            }
        }

        cx.update(crate::init);
        let (host, cx) = cx.add_window_view(|_, _| Host { selected: false });
        let draw = |cx: &mut ala::VisualTestContext| {
            cx.run_until_parked();
            cx.update(|window, cx| window.draw(cx).clear(cx));
        };
        draw(cx);
        cx.simulate_mouse_move(point(px(16.), px(16.)), None, Modifiers::default());
        draw(cx);
        host.update(cx, |host, cx| {
            host.selected = true;
            cx.notify();
        });
        draw(cx);
        cx.simulate_mouse_move(point(px(400.), px(400.)), None, Modifiers::default());
        draw(cx);
        host.update(cx, |host, cx| {
            host.selected = false;
            cx.notify();
        });
        draw(cx);

        let notified = Rc::new(std::cell::Cell::new(false));
        let _subscription = cx.update(|_, cx| {
            let notified = notified.clone();
            cx.observe(&host, move |_, _| notified.set(true))
        });
        cx.simulate_mouse_move(point(px(410.), px(410.)), None, Modifiers::default());
        assert!(
            !notified.get(),
            "the button came out of its selected state still thinking it was hovered"
        );
    }

    #[ala::test]
    fn test_button_clickable_logic(_cx: &mut ala::TestAppContext) {
        let clickable = Button::new("test").on_click(|_, _, _| {});
        assert!(clickable.clickable());

        let disabled = Button::new("test").disabled(true).on_click(|_, _, _| {});
        assert!(!disabled.clickable());

        let loading = Button::new("test").loading(true).on_click(|_, _, _| {});
        assert!(!loading.clickable());
    }

    #[ala::test]
    fn test_button_hoverable_logic(_cx: &mut ala::TestAppContext) {
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

    #[ala::test]
    fn test_button_variant_methods(_cx: &mut ala::TestAppContext) {
        assert!(ButtonVariant::Link.is_link());
        assert!(ButtonVariant::Text.is_text());

        assert!(ButtonVariant::Link.no_padding());
        assert!(ButtonVariant::Text.no_padding());
        assert!(!ButtonVariant::default().no_padding());
        assert_eq!(
            ButtonVariant::default()
                .surface(None, false, None)
                .map(|surface| surface.role),
            Some(Kind::Button.role())
        );
        assert_eq!(
            Button::new("b")
                .ghost()
                .variant
                .surface(None, true, None)
                .map(|surface| surface.role),
            Some(SurfaceRole::ICON_BUTTON)
        );
    }

    #[test]
    fn an_outline_button_draws_a_one_px_outline_on_every_builtin_theme() {
        use crate::{
            ThemeMode, foundation::interface_theme::builtin_themes,
            foundation::theme_css::kit_theme,
        };

        for (id, interface) in builtin_themes().iter() {
            for mode in [ThemeMode::Light, ThemeMode::Dark] {
                let theme = kit_theme(interface, mode);
                let layer = |emphasis| {
                    let surface = ButtonVariant::Paint(Tone::Neutral, emphasis)
                        .surface(None, false, None)
                        .unwrap();
                    theme.layer(surface, PaintState::Rest)
                };
                let outline = layer(Emphasis::Outline);
                assert_eq!(outline.border_width, px(1.0), "{id} {mode:?}");
                assert_eq!(outline.border.a, 0.3, "{id} {mode:?}");
                assert_eq!(outline.fill.a, 0.0, "{id} {mode:?}");
                let ghost = layer(Emphasis::Ghost);
                assert_eq!(ghost.border_width, px(0.0), "{id} {mode:?}");
                assert_eq!(ghost.fill.a, 0.0, "{id} {mode:?}");
            }
        }
    }

    #[ala::test]
    fn the_shorthands_name_a_tone_and_an_emphasis(_cx: &mut ala::TestAppContext) {
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

    #[ala::test]
    fn a_button_draws_its_themes_corner_smoothing(cx: &mut ala::TestAppContext) {
        use ala::{Context, IntoElement, ParentElement as _, Render, Styled as _};

        struct Host(Option<ButtonLook>);

        impl Render for Host {
            fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
                let button = Button::new("b").label("Button");
                div().p(Space::S10).child(match self.0 {
                    Some(look) => button.look(look),
                    None => button,
                })
            }
        }

        cx.update(crate::init);
        for (id, look, smoothing) in [
            ("default", None, 3.0),
            ("yaru", None, 2.0),
            ("macos", Some(ButtonLook::Send), 2.0),
        ] {
            cx.update(|cx| {
                let theme = &crate::foundation::interface_theme::builtin_themes()
                    .iter()
                    .find(|(theme, _)| *theme == id)
                    .expect("builtin theme")
                    .1;
                *crate::Theme::global_mut(cx) =
                    crate::foundation::theme_css::kit_theme(theme, crate::ThemeMode::Dark);
            });
            let (_, cx) = cx.add_window_view(|window, _| {
                window.set_default_corner_smoothing(crate::CORNER_SMOOTHING);
                Host(look)
            });
            cx.update(|window, cx| {
                _ = window.draw(cx);
                let drawn = window
                    .painted_quads()
                    .iter()
                    .filter(|quad| quad.corner_radii.top_left.0 > 0.0)
                    .map(|quad| quad.corner_smoothing)
                    .collect::<Vec<_>>();
                assert!(!drawn.is_empty(), "{id}");
                assert!(
                    drawn.iter().all(|drawn| *drawn == smoothing),
                    "{id}: {drawn:?}"
                );
            });
        }
    }

    #[ala::test]
    fn buttons_report_their_id_as_the_accessibility_author_id(cx: &mut ala::TestAppContext) {
        use ala::{Context, IntoElement, ParentElement as _, Render};

        struct Host;

        impl Render for Host {
            fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
                div()
                    .child(Button::new("variant-Primary").label("Primary"))
                    .child(Button::new(("tab", 3usize)).label("Tab"))
            }
        }

        cx.update(crate::init);
        let (_, cx) = cx.add_window_view(|_, _| Host);
        let ids = cx.update(|window, cx| {
            window.set_a11y_forced(true);
            window.draw(cx).clear(cx);
            let tree = window.a11y_tree().expect("forced a11y builds a tree");
            let mut ids = tree
                .nodes
                .iter()
                .filter(|(_, node)| node.role() == Role::Button)
                .map(|(_, node)| node.author_id().map(str::to_owned))
                .collect::<Vec<_>>();
            ids.sort();
            ids
        });
        assert_eq!(
            ids,
            [Some("tab-3".to_owned()), Some("variant-Primary".to_owned())]
        );
    }
}
