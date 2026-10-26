use super::AGENT_CONTENT_MAX_WIDTH;
use crate::input::{Input, InputState, field_chrome, field_paint};
use crate::{
    ActiveTheme as _, CHROME_GAP, Colorize as _, Ground, PaintState, Surface, h_flex, roles, v_flex,
};
use zz_gpui::{
    AnyElement, App, Entity, Focusable as _, IntoElement, RenderOnce, SharedString, Window, div,
    prelude::*, px,
};

pub const COMPOSER_FOOTER_HEIGHT: f32 = 28.0;
pub const COMPOSER_OUTER_PADDING: f32 = 12.0;
pub const COMPOSER_SECTION_GAP: f32 = 8.0;
pub const COMPOSER_MAX_WIDTH: f32 = AGENT_CONTENT_MAX_WIDTH + 2.0;
/// The round buttons in the composer, and the line you type, which sits on
/// their center.
pub const COMPOSER_BUTTON: f32 = 28.0;
/// The gap between the composer's edge and its buttons, the same all round.
pub const COMPOSER_INSET: f32 = 6.0;
/// The composer with one line typed.
pub const COMPOSER_HEIGHT: f32 = COMPOSER_BUTTON + 2.0 * COMPOSER_INSET;
/// The corner a one-line pill has, which a taller draft keeps.
const COMPOSER_RADIUS: f32 = COMPOSER_HEIGHT / 2.0;
const COMPOSER_TEXT: f32 = 13.0;
const COMPOSER_TEXT_PADDING: f32 = (COMPOSER_BUTTON - COMPOSER_TEXT * 1.25) / 2.0;

/// What the composer's controls and attachments sit on.
pub fn composer_ground(cx: &App) -> Ground {
    cx.theme().ground_of(roles::AGENT_COMPOSER)
}

/// The box you write in, the agent lens's and the Driver chat's alike: a pill
/// with a round button at its start (`+`), the field, the context ring and the
/// round send button, each inset the same from the edge. A longer draft grows
/// the pill and keeps the buttons on its last line.
#[derive(IntoElement)]
pub struct Composer {
    pub input: Entity<InputState>,
    pub leading: Option<AnyElement>,
    pub usage: Option<AnyElement>,
    pub action: AnyElement,
    /// What goes with the message, over the line you type.
    pub attachments: Option<AnyElement>,
    /// What the slash command being typed does, under the line you type.
    pub command_hint: Option<SharedString>,
    /// The paint the pill sits on, when it is not the window's.
    pub ground: Option<Ground>,
}

impl RenderOnce for Composer {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = if self.input.read(cx).focus_handle(cx).is_focused(window) {
            PaintState::Focused
        } else {
            PaintState::Rest
        };
        let paint = field_paint(roles::AGENT_COMPOSER, cx);
        let leading = self.leading.is_some();
        let surface = Surface::new(roles::AGENT_COMPOSER).on_maybe(self.ground);
        let radius = cx
            .theme()
            .layer(surface, state)
            .radius
            .min(px(COMPOSER_RADIUS));
        field_chrome(
            h_flex()
                .debug_selector(|| "composer".to_owned())
                .w_full()
                .min_h(px(COMPOSER_HEIGHT))
                .items_end()
                .gap(px(COMPOSER_INSET))
                .p(px(COMPOSER_INSET)),
            surface,
            state,
            true,
            true,
            cx,
        )
        .rounded(radius)
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
                        .when(leading, |input| input.pl(px(0.0)))
                        .text_size(crate::rems_from_px(COMPOSER_TEXT))
                        .appearance(false)
                        .bordered(false)
                        .focus_bordered(false),
                )
                .when_some(self.command_hint, |column, hint| {
                    column.child(
                        div()
                            .pb_1()
                            .when(!leading, zz_gpui::Styled::px_2)
                            .text_size(crate::rems_from_px(10.0))
                            .text_color(paint.muted_at(state))
                            .child(hint),
                    )
                }),
        )
        .children(self.usage)
        .child(self.action)
    }
}

/// The agent lens's composer: what waits on you over it (an approval, the
/// queue, the task tray, an error, the slash menu), the [`Composer`], and the
/// branch with its changes under it.
#[derive(IntoElement)]
pub struct AgentComposer {
    pub composer: Composer,
    pub git: Option<AnyElement>,
    pub prefix: Vec<AnyElement>,
}

impl RenderOnce for AgentComposer {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let has_prefix = !self.prefix.is_empty();
        let layer = cx.theme().layer(roles::AGENT_COMPOSER, PaintState::Rest);
        let backing = layer
            .glass
            .is_none()
            .then(|| cx.theme().background.opaque());
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
                                    .w_full()
                                    .rounded(layer.radius.min(px(COMPOSER_RADIUS)))
                                    .when_some(backing, Styled::bg)
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
                            .px_1()
                            .child(
                                h_flex()
                                    .min_w_0()
                                    .flex_1()
                                    .overflow_hidden()
                                    .children(self.git),
                            ),
                    ),
            )
    }
}
