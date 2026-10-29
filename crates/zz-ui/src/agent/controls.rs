use std::f32::consts::PI;

use ala::{
    AnyElement, App, ElementId, IntoElement, PathBuilder, Role, SharedString, Window, canvas, div,
    point, prelude::*, px,
};

use crate::{
    ActiveTheme as _, Colorize as _, Disableable as _, Emphasis, Icon, IconName, PaintState,
    Sizable as _, SurfaceRole, Tone,
    button::{Button, ButtonVariants},
    h_flex, roles,
    tooltip::Tooltip,
};

use super::composer::{COMPOSER_BUTTON, COMPOSER_FOOTER_HEIGHT, composer_ground};

pub use zz_client::agent_timeline::{
    ComposerAction, composer_action, context_usage_fraction, context_usage_tooltip,
    git_file_count_label,
};

/// The round send, queue or stop button at the composer's end, on the
/// composer's ground so its fill shows against the pill.
pub fn composer_action_button(
    id: impl Into<ElementId>,
    action: ComposerAction,
    enabled: bool,
    cx: &App,
) -> Button {
    let (icon, tooltip) = match action {
        ComposerAction::Send => (IconName::ArrowUp, "Send message"),
        ComposerAction::Queue => (IconName::Plus, "Queue this as the next turn"),
        ComposerAction::Stop => (IconName::Stop, "Stop the current turn"),
    };
    Button::compact_icon(id, icon)
        .size(crate::rems_from_px(COMPOSER_BUTTON))
        .when(action != ComposerAction::Queue, ButtonVariants::primary)
        .when(action == ComposerAction::Queue, ButtonVariants::secondary)
        .surface_role(roles::AGENT_SEND)
        .on(composer_ground(cx))
        .tooltip(tooltip)
        .disabled(!enabled)
}

/// The round `+` at the composer's start, the size of its send button.
pub fn composer_add_button(id: impl Into<ElementId>, tooltip: &'static str, cx: &App) -> Button {
    Button::compact_icon(id, IconName::Plus)
        .size(crate::rems_from_px(COMPOSER_BUTTON))
        .secondary()
        .surface_role(roles::AGENT_SEND)
        .on(composer_ground(cx))
        .tooltip(tooltip)
}

pub fn agent_chrome_button(id: impl Into<ElementId>) -> Button {
    Button::new(id).ghost().small().h(px(24.0)).px_2()
}

/// How many prompts wait behind the live turn, as a chip that hands them back
/// to the composer when clicked.
pub fn agent_queue_chip(
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
            .surface_role(SurfaceRole::QUIET_BUTTON)
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

pub fn git_summary_footer(
    id: impl Into<ElementId>,
    branch: Option<SharedString>,
    changed_files: u32,
    additions: u32,
    deletions: u32,
    cx: &App,
) -> AnyElement {
    let files = git_file_count_label(changed_files);
    let tooltip = zz_client::agent_timeline::git_summary_tooltip(
        branch.as_deref(),
        changed_files,
        additions,
        deletions,
    );
    h_flex()
        .id(id)
        .min_w_0()
        .h(px(COMPOSER_FOOTER_HEIGHT))
        .items_center()
        .gap_2()
        .text_size(crate::rems_from_px(11.0))
        .tooltip(move |window, cx| Tooltip::new(tooltip.clone()).build(window, cx))
        .child(
            Icon::new(IconName::GitBranch)
                .small()
                .flex_none()
                .text_color(cx.theme().foreground.muted())
                .relative()
                .top(crate::icon::cap_center_drop(px(11.0), px(12.0), cx)),
        )
        .when_some(branch, |this, branch| {
            this.child(
                div()
                    .min_w_0()
                    .max_w(px(220.0))
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .text_color(cx.theme().foreground.muted())
                    .child(branch),
            )
        })
        .child(
            div()
                .flex_none()
                .text_color(cx.theme().foreground.muted())
                .child(files),
        )
        .child(
            div()
                .flex_none()
                .text_color(cx.theme().success)
                .child(format!("+{additions}")),
        )
        .child(
            div()
                .flex_none()
                .text_color(cx.theme().danger)
                .child(format!("-{deletions}")),
        )
        .into_any_element()
}

const CONTEXT_USAGE_RING_SIZE: f32 = 16.0;
const CONTEXT_USAGE_STROKE_WIDTH: f32 = 2.0;

#[allow(clippy::cast_possible_truncation)]
pub fn context_usage_meter(
    id: impl Into<ElementId>,
    used: u64,
    size: u64,
    cx: &ala::App,
) -> AnyElement {
    let progress = context_usage_fraction(used, size) as f32;
    let tooltip = context_usage_tooltip(used, size);
    let track_color = cx
        .theme()
        .paint(Tone::Neutral, Emphasis::Soft)
        .fill_at(PaintState::Active);
    let progress_color = cx.theme().foreground.muted();
    let ring = canvas(
        |_, _, _| (),
        move |bounds, (), window, _| {
            let stroke = px(CONTEXT_USAGE_STROKE_WIDTH);
            let radius = px((CONTEXT_USAGE_RING_SIZE - CONTEXT_USAGE_STROKE_WIDTH) / 2.0);
            let center_x = bounds.origin.x + bounds.size.width / 2.0;
            let center_y = bounds.origin.y + bounds.size.height / 2.0;

            let mut track = PathBuilder::stroke(stroke);
            track.move_to(point(center_x + radius, center_y));
            track.arc_to(
                point(radius, radius),
                px(0.0),
                false,
                true,
                point(center_x - radius, center_y),
            );
            track.arc_to(
                point(radius, radius),
                px(0.0),
                false,
                true,
                point(center_x + radius, center_y),
            );
            track.close();
            if let Ok(path) = track.build() {
                window.paint_path(path, track_color);
            }

            if progress <= 0.0 {
                return;
            }
            let mut fill = PathBuilder::stroke(stroke);
            if progress >= 0.999 {
                fill.move_to(point(center_x + radius, center_y));
                fill.arc_to(
                    point(radius, radius),
                    px(0.0),
                    false,
                    true,
                    point(center_x - radius, center_y),
                );
                fill.arc_to(
                    point(radius, radius),
                    px(0.0),
                    false,
                    true,
                    point(center_x + radius, center_y),
                );
                fill.close();
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
    .size(px(CONTEXT_USAGE_RING_SIZE));
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
