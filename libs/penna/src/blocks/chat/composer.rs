use std::f32::consts::PI;

use ala::{
    AnyElement, App, ElementId, Entity, Focusable as _, Hsla, IntoElement, PathBuilder, Pixels,
    RenderOnce, Role, SharedString, Window, canvas, div, point, prelude::*, px,
};

use crate::{
    ActiveTheme as _, CHROME_GAP, Colorize as _, Disableable as _, Edge, Emphasis, Ground,
    IconName, Label, PaintState, RuleLook, Sizable as _, StyledExt as _, Surface, TextStyle, Tone,
    components::button::{Button, ButtonLook, ButtonVariants},
    components::input::{
        FieldLook, Input, InputContentType, InputState, field_chrome, field_underlay,
    },
    components::tooltip::Tooltip,
    h_flex, muted_on, rems_from_px, v_flex,
};

use super::ask::{ASK_GAP, ASK_GUTTER, ASK_RADIUS, ASK_ROW_PADDING};
use super::message::CHAT_CONTENT_MAX_WIDTH;

pub const COMPOSER_FOOTER_HEIGHT: f32 = 28.0;
pub const COMPOSER_OUTER_PADDING: f32 = 12.0;
pub const COMPOSER_SECTION_GAP: f32 = 8.0;
pub const COMPOSER_MAX_WIDTH: f32 = CHAT_CONTENT_MAX_WIDTH + 2.0;
pub const COMPOSER_BUTTON: f32 = 28.0;
pub const COMPOSER_INSET: f32 = 6.0;
pub(super) const COMPOSER_FOOTER_INSET: f32 = 4.0;
pub const COMPOSER_HEIGHT: f32 = COMPOSER_BUTTON + 2.0 * COMPOSER_INSET;
const COMPOSER_RADIUS: f32 = COMPOSER_HEIGHT / 2.0;
const COMPOSER_BACKING_INSET: f32 = 1.0;
const COMPOSER_TEXT_PADDING: f32 = (COMPOSER_BUTTON - TextStyle::Body.line_height_px()) / 2.0;
const CONTEXT_RING_SIZE: f32 = 16.0;
const CONTEXT_STROKE_WIDTH: f32 = 2.0;

#[must_use]
pub fn composer_ground(on: Ground, cx: &App) -> Ground {
    cx.theme()
        .ground_of(Surface::from(FieldLook::Composer).on(on))
}

fn composer_corner(asking: bool, cx: &App) -> Pixels {
    let radius = composer_radius(cx);
    if asking {
        radius.min(px(ASK_RADIUS))
    } else {
        radius
    }
}

fn composer_radius(cx: &App) -> Pixels {
    let radius = cx.theme().radius_of(FieldLook::Composer);
    if radius <= px(0.0) || radius >= ala::FULL_CORNER_RADIUS {
        return radius.min(px(COMPOSER_RADIUS));
    }
    (cx.theme().control_corner(px(COMPOSER_BUTTON)) + px(COMPOSER_INSET)).min(px(COMPOSER_RADIUS))
}

#[derive(IntoElement)]
pub struct Composer {
    pub input: Entity<InputState>,
    pub leading: Option<AnyElement>,
    pub usage: Option<AnyElement>,
    pub action: AnyElement,
    pub attachments: Option<AnyElement>,
    pub command_hint: Option<SharedString>,
    pub ask: Option<AnyElement>,
    pub secret: bool,
    pub ground: Ground,
}

impl RenderOnce for Composer {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = if self.input.read(cx).focus_handle(cx).is_focused(window) {
            PaintState::Focused
        } else {
            PaintState::Rest
        };
        let inside = cx
            .theme()
            .ground_at(Surface::from(FieldLook::Composer).on(self.ground), state);
        let leading = self.leading.is_some();
        let asking = self.ask.is_some();
        let radius = composer_corner(asking, cx);
        field_chrome(
            v_flex()
                .debug_selector(|| "chat-composer".to_owned())
                .w_full()
                .min_h(px(COMPOSER_HEIGHT)),
            FieldLook::Composer,
            Some(self.ground),
            state,
            cx,
        )
        .rounded(radius)
        .corner_radius_mode(ala::CornerRadiusMode::Fixed)
        .children(self.ask)
        .child(
            h_flex()
                .debug_selector(|| "chat-composer-row".to_owned())
                .w_full()
                .flex_none()
                .items_end()
                .map(|row| {
                    if asking {
                        row.gap(px(ASK_GAP))
                            .px(px(ASK_GUTTER))
                            .py(px(ASK_ROW_PADDING))
                            .rule(RuleLook::List, Edge::Top, cx)
                    } else {
                        row.gap(px(COMPOSER_INSET)).p(px(COMPOSER_INSET))
                    }
                })
                .children(self.leading)
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .children(self.attachments)
                        .child(
                            Input::new(&self.input)
                                .w_full()
                                .min_w_0()
                                .h_auto()
                                .py(px(COMPOSER_TEXT_PADDING))
                                .on(inside)
                                .when(leading || asking, |input| input.pl(px(0.0)))
                                .when(self.secret, |input| {
                                    input.content_type(InputContentType::Password)
                                })
                                .appearance(false)
                                .bordered(false)
                                .focus_bordered(false),
                        )
                        .when_some(self.command_hint, |column, hint| {
                            column.child(
                                Label::new(TextStyle::Micro, hint)
                                    .pb_1()
                                    .when(!leading, ala::Styled::px_2)
                                    .text_color(muted_on(inside, cx)),
                            )
                        }),
                )
                .children(self.usage)
                .child(self.action),
        )
    }
}

#[derive(IntoElement)]
pub struct ChatComposer {
    pub composer: Composer,
    pub prefix: Vec<AnyElement>,
    pub footer: Vec<AnyElement>,
    pub controls: Vec<AnyElement>,
}

impl RenderOnce for ChatComposer {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let has_prefix = !self.prefix.is_empty();
        let inset = px(COMPOSER_BACKING_INSET);
        let radius = composer_corner(self.composer.ask.is_some(), cx);
        let backing = field_underlay(FieldLook::Composer, inset, cx).map(|underlay| {
            underlay
                .rounded(radius - inset)
                .corner_radius_mode(ala::CornerRadiusMode::Fixed)
        });
        v_flex()
            .flex_none()
            .w_full()
            .when(!has_prefix, |this| this.mt(px(-COMPOSER_OUTER_PADDING)))
            .child(
                v_flex()
                    .w_full()
                    .px(px(COMPOSER_OUTER_PADDING))
                    .when(has_prefix, |this| this.pt(px(COMPOSER_OUTER_PADDING)))
                    .child(
                        v_flex()
                            .w_full()
                            .max_w(px(COMPOSER_MAX_WIDTH))
                            .mx_auto()
                            .gap(px(COMPOSER_SECTION_GAP))
                            .children(self.prefix)
                            .child(
                                div()
                                    .relative()
                                    .w_full()
                                    .children(backing)
                                    .child(self.composer),
                            ),
                    ),
            )
            .child(
                v_flex()
                    .w_full()
                    .px(px(COMPOSER_OUTER_PADDING))
                    .pt(px(COMPOSER_SECTION_GAP))
                    .pb(px(COMPOSER_OUTER_PADDING))
                    .child(
                        h_flex()
                            .w_full()
                            .max_w(px(COMPOSER_MAX_WIDTH))
                            .mx_auto()
                            .h(px(COMPOSER_FOOTER_HEIGHT))
                            .items_center()
                            .gap(px(CHROME_GAP))
                            .px(px(COMPOSER_FOOTER_INSET))
                            .child(
                                h_flex()
                                    .min_w_0()
                                    .flex_1()
                                    .gap(px(CHROME_GAP))
                                    .overflow_hidden()
                                    .children(self.footer),
                            )
                            .when(!self.controls.is_empty(), |footer| {
                                footer.child(
                                    h_flex()
                                        .flex_none()
                                        .gap(px(CHROME_GAP))
                                        .items_center()
                                        .children(self.controls),
                                )
                            }),
                    ),
            )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComposerAction {
    Send,
    Queue,
    Stop,
}

pub fn composer_action_button(
    id: impl Into<ElementId>,
    action: ComposerAction,
    enabled: bool,
    on: Ground,
) -> Button {
    let (icon, tooltip) = match action {
        ComposerAction::Send => (IconName::ArrowUp, "Send message"),
        ComposerAction::Queue => (IconName::Plus, "Queue this as the next turn"),
        ComposerAction::Stop => (IconName::Stop, "Stop the current turn"),
    };
    Button::compact_icon(id, icon)
        .size(rems_from_px(COMPOSER_BUTTON))
        .when(
            enabled && action != ComposerAction::Queue,
            ButtonVariants::primary,
        )
        .when(
            !enabled || action == ComposerAction::Queue,
            ButtonVariants::secondary,
        )
        .look(ButtonLook::Send)
        .on(on)
        .tooltip(tooltip)
        .disabled(!enabled)
}

pub fn composer_add_button(id: impl Into<ElementId>, tooltip: &'static str, on: Ground) -> Button {
    Button::compact_icon(id, IconName::Plus)
        .size(rems_from_px(COMPOSER_BUTTON))
        .secondary()
        .look(ButtonLook::Send)
        .on(on)
        .tooltip(tooltip)
}

pub fn chrome_button(id: impl Into<ElementId>) -> Button {
    Button::new(id).ghost().small().h(px(24.0)).px_2()
}

pub fn queue_chip(
    id: impl Into<ElementId>,
    queued: impl std::fmt::Display,
    enabled: bool,
    on_click: impl Fn(&mut Window, &mut App) + 'static,
    cx: &App,
) -> ala::Div {
    h_flex().w_full().justify_end().child(
        Button::new(id)
            .ghost()
            .small()
            .icon(IconName::Undo2)
            .label(format!("{queued} queued"))
            .tooltip("Return the queued prompts to the composer")
            .text_color(cx.theme().foreground.muted())
            .disabled(!enabled)
            .on_click(move |_, window, cx| {
                on_click(window, cx);
                cx.stop_propagation();
            }),
    )
}

pub fn context_meter(
    id: impl Into<ElementId>,
    fraction: f32,
    tooltip: impl Into<SharedString>,
    cx: &App,
) -> AnyElement {
    let progress = fraction.clamp(0.0, 1.0);
    let tooltip = tooltip.into();
    let ring = context_ring(progress, cx.theme().foreground.muted(), cx);
    let aria_value = format!("{:.0}%", f64::from(progress) * 100.0);
    let hover_tooltip = tooltip.clone();
    div()
        .id(id)
        .role(Role::ProgressIndicator)
        .aria_label(tooltip)
        .aria_value(aria_value)
        .flex()
        .flex_none()
        .size(px(COMPOSER_BUTTON))
        .items_center()
        .justify_center()
        .tooltip(move |window, cx| Tooltip::new(hover_tooltip.clone()).build(window, cx))
        .child(ring)
        .into_any_element()
}

pub fn context_ring(fraction: f32, progress_color: Hsla, cx: &App) -> impl IntoElement + use<> {
    let progress = fraction.clamp(0.0, 1.0);
    let track_color = cx
        .theme()
        .paint(Tone::Neutral, Emphasis::Soft)
        .fill_at(PaintState::Active);
    canvas(
        |_, _, _| (),
        move |bounds, (), window, _| {
            let stroke = px(CONTEXT_STROKE_WIDTH);
            let radius = px((CONTEXT_RING_SIZE - CONTEXT_STROKE_WIDTH) / 2.0);
            let center_x = bounds.origin.x + bounds.size.width / 2.0;
            let center_y = bounds.origin.y + bounds.size.height / 2.0;
            let full_circle = |path: &mut PathBuilder| {
                path.move_to(point(center_x + radius, center_y));
                path.arc_to(
                    point(radius, radius),
                    px(0.0),
                    false,
                    true,
                    point(center_x - radius, center_y),
                );
                path.arc_to(
                    point(radius, radius),
                    px(0.0),
                    false,
                    true,
                    point(center_x + radius, center_y),
                );
                path.close();
            };
            let mut track = PathBuilder::stroke(stroke);
            full_circle(&mut track);
            if let Ok(path) = track.build() {
                window.paint_path(path, track_color);
            }
            if progress <= 0.0 {
                return;
            }
            let mut fill = PathBuilder::stroke(stroke);
            if progress >= 0.999 {
                full_circle(&mut fill);
            } else {
                fill.move_to(point(center_x, center_y - radius));
                let angle = -PI / 2.0 + progress * 2.0 * PI;
                fill.arc_to(
                    point(radius, radius),
                    px(0.0),
                    progress > 0.5,
                    true,
                    point(
                        center_x + radius * angle.cos(),
                        center_y + radius * angle.sin(),
                    ),
                );
            }
            if let Ok(path) = fill.build() {
                window.paint_path(path, progress_color);
            }
        },
    )
    .flex_none()
    .size(px(CONTEXT_RING_SIZE))
}
