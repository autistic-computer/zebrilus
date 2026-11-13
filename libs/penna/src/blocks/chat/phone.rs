use std::{rc::Rc, sync::Arc};

use ala::{
    AnyElement, App, ClickEvent, ElementId, Entity, Focusable as _, Image, ImageSource,
    IntoElement, ObjectFit, Pixels, RenderOnce, Role, SharedString, Window, div, img, prelude::*,
    px,
};

use crate::{
    ActiveTheme as _, Colorize as _, Disableable as _, Ground, Icon, IconName, Kind, Layer,
    PaintState, Sizable as _, Size, Space, StyledExt as _, Surface, TextStyle,
    blocks::touch::{Nest, TOUCH_TARGET, TouchRow, concentric, touch_row_frame},
    components::button::{Button, ButtonLook, ButtonVariants},
    components::input::{FieldLook, Input, InputState, field_chrome, field_underlay},
    foundation::roles,
    h_flex, muted_on, rems_from_px, v_flex,
};

use super::composer::{ComposerAction, composer_ground};

pub const PHONE_COMPOSER_BUTTON: f32 = TOUCH_TARGET;
pub const PHONE_COMPOSER_INSET: f32 = 6.0;
pub const PHONE_COMPOSER_HEIGHT: f32 = PHONE_COMPOSER_BUTTON + 2.0 * PHONE_COMPOSER_INSET;
pub const PHONE_COMPOSER_ROWS: usize = 5;
pub const PHONE_THUMBNAIL: f32 = 88.0;
pub const PHONE_QUEUED_HEIGHT: f32 = TOUCH_TARGET;
const GLYPH: f32 = 20.0;
const BADGE: f32 = 24.0;
const BADGE_GLYPH: f32 = 12.0;
const BADGE_INSET: f32 = 4.0;
const CARD_INSET: f32 = 4.0;
const WHAT_INSET: f32 = 12.0;
const WHAT_TOP: f32 = 12.0;
const WHAT_BOTTOM: f32 = 8.0;
const TEXT_PADDING: f32 = (PHONE_COMPOSER_BUTTON - TextStyle::TouchBody.line_height_px()) / 2.0;

#[must_use]
pub fn phone_composer_corner(cx: &App) -> Pixels {
    let half = px(PHONE_COMPOSER_HEIGHT / 2.0);
    let radius = cx.theme().radius_of(FieldLook::Composer);
    if radius <= px(0.0) {
        return px(0.0);
    }
    if radius >= ala::FULL_CORNER_RADIUS {
        return half;
    }
    (cx.theme().control_corner(px(PHONE_COMPOSER_BUTTON)) + px(PHONE_COMPOSER_INSET)).min(half)
}

fn phone_button(id: impl Into<ElementId>, icon: IconName) -> Button {
    Button::new(id)
        .compact()
        .tab_stop(false)
        .look(ButtonLook::Send)
        .size(rems_from_px(PHONE_COMPOSER_BUTTON))
        .child(Icon::new(icon).size(rems_from_px(GLYPH)))
}

pub fn phone_plus_button(id: impl Into<ElementId>, on: Ground) -> Button {
    phone_button(id, IconName::Plus).secondary().on(on)
}

pub fn phone_close_button(id: impl Into<ElementId>, on: Ground) -> Button {
    phone_button(id, IconName::Xmark).ghost().on(on)
}

pub fn phone_action_button(
    id: impl Into<ElementId>,
    action: ComposerAction,
    enabled: bool,
    on: Ground,
) -> Button {
    let icon = match action {
        ComposerAction::Send => IconName::ArrowUp,
        ComposerAction::Queue => IconName::Clock,
        ComposerAction::Stop => IconName::Stop,
    };
    let solid = enabled && action != ComposerAction::Queue;
    phone_button(id, icon)
        .when(solid, ButtonVariants::primary)
        .when(!solid, ButtonVariants::secondary)
        .on(on)
        .disabled(!enabled)
}

#[derive(IntoElement)]
pub struct PhoneComposer {
    pub input: Entity<InputState>,
    pub leading: AnyElement,
    pub action: AnyElement,
    pub thumbnails: Vec<AnyElement>,
    pub above: Option<SharedString>,
    pub ground: Ground,
}

impl RenderOnce for PhoneComposer {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = if self.input.read(cx).focus_handle(cx).is_focused(window) {
            PaintState::Focused
        } else {
            PaintState::Rest
        };
        let inside = cx
            .theme()
            .ground_at(Surface::from(FieldLook::Composer).on(self.ground), state);
        let radius = phone_composer_corner(cx);
        let inset = px(1.0);
        let backing = field_underlay(FieldLook::Composer, inset, cx).map(|underlay| {
            underlay
                .rounded(radius - inset)
                .corner_radius_mode(ala::CornerRadiusMode::Fixed)
        });
        let muted = muted_on(inside, cx);
        let field = field_chrome(
            v_flex()
                .debug_selector(|| "chat-phone-composer".to_owned())
                .w_full()
                .min_h(px(PHONE_COMPOSER_HEIGHT)),
            FieldLook::Composer,
            Some(self.ground),
            state,
            cx,
        )
        .rounded(radius)
        .corner_radius_mode(ala::CornerRadiusMode::Fixed)
        .when(!self.thumbnails.is_empty(), |field| {
            field.child(
                h_flex()
                    .flex_wrap()
                    .gap(Space::GAP_RELATED)
                    .px(px(PHONE_COMPOSER_INSET))
                    .pt(px(PHONE_COMPOSER_INSET))
                    .children(self.thumbnails),
            )
        })
        .when_some(self.above, |field, above| {
            field.child(
                div()
                    .min_w_0()
                    .px(Space::S12)
                    .pt(Space::S10)
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .type_style(TextStyle::TouchSmall, cx)
                    .text_color(muted)
                    .child(above),
            )
        })
        .child(
            h_flex()
                .debug_selector(|| "chat-phone-composer-row".to_owned())
                .w_full()
                .flex_none()
                .items_end()
                .gap(px(PHONE_COMPOSER_INSET))
                .p(px(PHONE_COMPOSER_INSET))
                .child(div().flex_none().child(self.leading))
                .child(
                    div().flex_1().min_w_0().child(
                        Input::new(&self.input)
                            .with_size(Size::TouchLarge)
                            .w_full()
                            .min_w_0()
                            .h_auto()
                            .px(px(0.0))
                            .py(px(TEXT_PADDING))
                            .on(inside)
                            .appearance(false)
                            .bordered(false),
                    ),
                )
                .child(div().flex_none().child(self.action)),
        );
        div().relative().w_full().children(backing).child(field)
    }
}

pub fn phone_thumbnail(
    id: impl Into<ElementId>,
    image: Arc<Image>,
    on_remove: impl Fn(&mut Window, &mut App) + 'static,
    cx: &App,
) -> AnyElement {
    let id = id.into();
    let theme = cx.theme();
    let corner = concentric(phone_composer_corner(cx), px(PHONE_COMPOSER_INSET));
    let disc = Layer {
        radius: theme.control_corner(px(BADGE)),
        share: None,
        ..theme.layer(Surface::new(Kind::Floating.role()), PaintState::Rest)
    };
    let reach = BADGE_INSET + (BADGE - TOUCH_TARGET) / 2.0;
    div()
        .id(id.clone())
        .relative()
        .flex_none()
        .size(px(PHONE_THUMBNAIL))
        .child(
            div().size_full().rounded(corner).overflow_hidden().child(
                img(ImageSource::Image(image))
                    .size_full()
                    .object_fit(ObjectFit::Cover),
            ),
        )
        .child(
            div()
                .id(SharedString::from(format!("{id}-remove")))
                .role(Role::Button)
                .aria_label("Remove the image")
                .absolute()
                .top(px(reach))
                .right(px(reach))
                .flex()
                .items_center()
                .justify_center()
                .size(px(TOUCH_TARGET))
                .click_cursor()
                .on_click(move |_, window, cx| {
                    on_remove(window, cx);
                    cx.stop_propagation();
                })
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_center()
                        .size(px(BADGE))
                        .layer(&disc)
                        .text_color(disc.text)
                        .child(Icon::new(IconName::Xmark).size(px(BADGE_GLYPH))),
                ),
        )
        .into_any_element()
}

type WhatHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct AskCard {
    id: ElementId,
    title: SharedString,
    detail: Option<SharedString>,
    count: Option<SharedString>,
    expanded: bool,
    rows: Vec<TouchRow>,
    ground: Ground,
    max_height: Option<Pixels>,
    on_what: Option<WhatHandler>,
}

impl AskCard {
    #[must_use]
    pub fn new(id: impl Into<ElementId>, title: impl Into<SharedString>, ground: Ground) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            detail: None,
            count: None,
            expanded: false,
            rows: Vec::new(),
            ground,
            max_height: None,
            on_what: None,
        }
    }

    #[must_use]
    pub fn detail(mut self, detail: impl Into<SharedString>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    #[must_use]
    pub fn count(mut self, count: impl Into<SharedString>) -> Self {
        self.count = Some(count.into());
        self
    }

    #[must_use]
    pub fn expanded(mut self, expanded: bool) -> Self {
        self.expanded = expanded;
        self
    }

    #[must_use]
    pub fn rows(mut self, rows: impl IntoIterator<Item = TouchRow>) -> Self {
        self.rows.extend(rows);
        self
    }

    #[must_use]
    pub fn max_height(mut self, height: Pixels) -> Self {
        self.max_height = Some(height);
        self
    }

    #[must_use]
    pub fn on_what(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_what = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for AskCard {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let outer = theme.phone_radius();
        let inside = composer_ground(self.ground, cx);
        let muted = muted_on(inside, cx);
        let nest = Nest {
            outer,
            inset: px(CARD_INSET),
        };
        let expanded = self.expanded;
        let title = div()
            .flex_1()
            .min_w_0()
            .type_style_strong(TextStyle::TouchBody, cx)
            .when(!expanded, |title| title.line_clamp(2))
            .child(self.title.clone());
        let what = v_flex()
            .id(SharedString::from(format!("{}-what", self.id)))
            .debug_selector(|| "chat-ask-card-what".to_owned())
            .flex_none()
            .min_w_0()
            .gap(Space::S2)
            .px(px(WHAT_INSET))
            .pt(px(WHAT_TOP))
            .pb(px(WHAT_BOTTOM))
            .child(
                h_flex()
                    .items_start()
                    .gap(Space::GAP_RELATED)
                    .child(title)
                    .children(self.count.map(|count| {
                        div()
                            .flex_none()
                            .type_style(TextStyle::TouchSmall, cx)
                            .line_height(px(TextStyle::TouchBody.line_height_px()))
                            .text_color(muted)
                            .child(count)
                    })),
            )
            .children(self.detail.map(|detail| {
                div()
                    .min_w_0()
                    .type_style_mono(TextStyle::TouchSmall, cx)
                    .text_color(muted)
                    .when(!expanded, |detail| {
                        detail.overflow_hidden().whitespace_nowrap().text_ellipsis()
                    })
                    .child(detail)
            }))
            .when_some(self.on_what, |what, on_what| {
                what.click_cursor()
                    .on_click(move |event, window, cx| on_what(event, window, cx))
            });
        let rows = v_flex()
            .id(SharedString::from(format!("{}-rows", self.id)))
            .flex_shrink_1()
            .min_h_0()
            .gap(Space::S2)
            .overflow_y_scroll()
            .children(self.rows.into_iter().map(|row| row.nested(nest, inside)));
        field_chrome(
            v_flex()
                .id(self.id)
                .debug_selector(|| "chat-ask-card".to_owned())
                .w_full()
                .p(px(CARD_INSET))
                .gap(Space::S2),
            FieldLook::Composer,
            Some(self.ground),
            PaintState::Rest,
            cx,
        )
        .rounded(outer)
        .corner_radius_mode(ala::CornerRadiusMode::Fixed)
        .when_some(self.max_height, Styled::max_h)
        .text_color(theme.foreground)
        .child(what)
        .child(rows)
    }
}

pub fn phone_queued_row(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    enabled: bool,
    on_click: impl Fn(&mut Window, &mut App) + 'static,
    cx: &App,
) -> AnyElement {
    let surface = Surface::new(roles::PHONE_ROW);
    let state = if enabled {
        PaintState::Rest
    } else {
        PaintState::Disabled
    };
    let muted = cx.theme().foreground.muted();
    let id = id.into();
    let author = crate::foundation::author_name(&id);
    touch_row_frame(id, surface, px(PHONE_QUEUED_HEIGHT), None, state, cx)
        .role(Role::Button)
        .accessibility_id(author)
        .type_style(TextStyle::TouchSmall, cx)
        .text_color(muted)
        .child(Icon::new(IconName::List).size(px(GLYPH)).flex_none())
        .child(div().flex_1().min_w_0().child(label.into()))
        .when(enabled, |row| {
            row.on_click(move |_, window, cx| on_click(window, cx))
        })
        .into_any_element()
}

pub fn phone_jump_button(id: impl Into<ElementId>, on: Ground) -> Button {
    Button::new(id)
        .compact()
        .secondary()
        .tab_stop(false)
        .look(ButtonLook::Floating)
        .size(rems_from_px(TOUCH_TARGET))
        .child(Icon::new(IconName::ArrowDown).size(rems_from_px(GLYPH)))
        .on(on)
        .tooltip("Jump to latest")
}
