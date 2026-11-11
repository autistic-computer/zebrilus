use std::rc::Rc;

use ala::{
    Anchor, AnyElement, App, ElementId, IntoElement, ScrollHandle, SharedString, Window, div,
    prelude::*, px,
};

use crate::{
    ActiveTheme as _, Colorize as _, Disableable as _, Emphasis, Ground, Icon, IconName,
    Sizable as _, StyledExt as _, Tone,
    button::{Button, ButtonVariant, ButtonVariants as _},
    h_flex,
    tooltip::Tooltip,
};
use penna::{
    Label, Row, RowLook, TextStyle,
    components::{popover::Popover, slider::StepRail},
};
use zz_client::agent_settings::{self, LensSettings, ModeRisk};

use penna::blocks::chat::{self, COMPOSER_FOOTER_HEIGHT};

pub use zz_client::agent_timeline::{
    ComposerAction, composer_action, context_usage_fraction, context_usage_tooltip,
    git_file_count_label,
};

pub fn composer_action_button(
    id: impl Into<ElementId>,
    action: ComposerAction,
    enabled: bool,
    on: Ground,
) -> Button {
    let action = match action {
        ComposerAction::Send => chat::ComposerAction::Send,
        ComposerAction::Queue => chat::ComposerAction::Queue,
        ComposerAction::Stop => chat::ComposerAction::Stop,
    };
    chat::composer_action_button(id, action, enabled, on)
}

const STEP: f32 = 32.0;
const HEAD: f32 = 26.0;
const PANEL: f32 = 264.0;
const LIST_ROWS: f32 = 6.0;

pub fn agent_model_picker(
    id: impl Into<ElementId>,
    settings: &LensSettings,
    enabled: bool,
    on_pick: impl Fn(&'static str, String, &mut Window, &mut App) + 'static,
    cx: &App,
) -> AnyElement {
    let id = id.into();
    let choices = !settings.models.is_empty() || !settings.efforts.is_empty();
    let label = if settings.model_label.is_empty() {
        "Model".to_owned()
    } else {
        settings.model_label.clone()
    };
    let trigger = chat::chrome_button(SharedString::from(format!("{id}-trigger")))
        .text_color(cx.theme().foreground.rest())
        .disabled(!enabled || !choices)
        .tooltip("Choose the model and effort")
        .child(label)
        .when(!settings.effort.is_empty(), |button| {
            button.child(
                div()
                    .text_color(cx.theme().foreground.muted())
                    .child(settings.effort_name().to_owned()),
            )
        })
        .child(
            Icon::new(IconName::ChevronDown)
                .small()
                .text_color(cx.theme().foreground.muted()),
        );
    let settings = settings.clone();
    let on_pick: Pick = Rc::new(on_pick);
    Popover::new(id)
        .anchor(Anchor::BottomRight)
        .p(px(0.0))
        .trigger(trigger)
        .content(move |_, window, cx| model_panel(&settings, &on_pick, window, cx))
        .into_any_element()
}

type Pick = Rc<dyn Fn(&'static str, String, &mut Window, &mut App)>;

fn model_panel(
    settings: &LensSettings,
    on_pick: &Pick,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let current = settings
        .models
        .iter()
        .position(|choice| choice.value == settings.model);
    let scroll = window
        .use_keyed_state("agent-model-scroll", cx, |window, _| {
            let scroll = ScrollHandle::new();
            if let Some(current) = current {
                let painted = scroll.clone();
                window.on_next_frame(move |window, _| {
                    painted.scroll_to_item(current);
                    window.refresh();
                });
            }
            scroll
        })
        .read(cx)
        .clone();
    let rows = settings.models.iter().map(|choice| {
        let pick = on_pick.clone();
        let value = choice.value.clone();
        let chosen = choice.value == settings.model;
        step_row(
            SharedString::from(format!("agent-model-{}", choice.value)),
            choice.name.clone(),
            chosen,
        )
        .on_click(move |_, window, cx| {
            pick(agent_settings::MODEL, value.clone(), window, cx);
        })
        .when(chosen, |row| {
            row.child(Icon::new(IconName::Check).with_size(px(12.0)))
        })
    });
    let list = crate::v_flex()
        .id("agent-models")
        .flex_none()
        .max_h(px(STEP * LIST_ROWS))
        .overflow_y_scroll()
        .track_scroll(&scroll)
        .children(rows);
    let panel = crate::v_flex()
        .w(px(PANEL))
        .p(px(6.0))
        .child(head("Model", cx))
        .child(list);
    if settings.efforts.is_empty() {
        return panel.into_any_element();
    }
    panel
        .child(
            div()
                .flex_none()
                .h(px(1.0))
                .mx(px(-6.0))
                .my(px(6.0))
                .bg(cx.theme().border()),
        )
        .child(
            div()
                .px(px(10.0))
                .pb(px(10.0))
                .child(effort_rail(settings, on_pick)),
        )
        .into_any_element()
}

fn head(text: &'static str, cx: &App) -> Label {
    Label::new(TextStyle::Caption, text)
        .strong()
        .flex_none()
        .h(px(HEAD))
        .px(px(10.0))
        .text_color(cx.theme().foreground.muted())
}

fn step_row(id: impl Into<ElementId>, name: String, chosen: bool) -> Row {
    Row::new(id, RowLook::Menu)
        .state(crate::row_state(chosen, true))
        .row_height(px(STEP))
        .h_flex()
        .flex_none()
        .h(px(STEP))
        .w_full()
        .gap(px(8.0))
        .px(px(10.0))
        .click_cursor()
        .child(
            Label::new(TextStyle::Small, name)
                .truncate()
                .flex_1()
                .min_w_0(),
        )
}

fn effort_rail(settings: &LensSettings, on_pick: &Pick) -> StepRail {
    let pick = on_pick.clone();
    let values = settings
        .efforts
        .iter()
        .map(|choice| choice.value.clone())
        .collect::<Vec<_>>();
    let rail = StepRail::new(
        "agent-effort",
        "Effort",
        settings
            .efforts
            .iter()
            .map(|choice| SharedString::from(choice.name.clone()))
            .collect(),
        values.iter().position(|value| *value == settings.effort),
        move |index, window, cx| {
            if let Some(value) = values.get(index) {
                pick(agent_settings::EFFORT, value.clone(), window, cx);
            }
        },
    );
    if settings.effort_starts_unset() {
        rail.open_start()
    } else {
        rail
    }
}

#[must_use]
pub const fn mode_tone(risk: ModeRisk) -> Option<Tone> {
    match risk {
        ModeRisk::Asks => None,
        ModeRisk::ReadOnly => Some(Tone::Success),
        ModeRisk::Unasked => Some(Tone::Warning),
        ModeRisk::Unguarded => Some(Tone::Danger),
    }
}

const fn mode_icon(risk: ModeRisk) -> IconName {
    match risk {
        ModeRisk::Asks => IconName::CircleUser,
        ModeRisk::ReadOnly => IconName::Eye,
        ModeRisk::Unasked => IconName::Pencil,
        ModeRisk::Unguarded => IconName::TriangleAlert,
    }
}

pub fn agent_mode_chip(
    id: impl Into<ElementId>,
    provider: zz_protocol::AgentProvider,
    settings: &LensSettings,
    enabled: bool,
    on_pick: impl Fn(String, &mut Window, &mut App) + 'static,
    cx: &App,
) -> Option<AnyElement> {
    let next = settings.next_mode()?.to_owned();
    let risk = agent_settings::mode_risk(provider, &settings.mode);
    let chip = chat::chrome_button(id)
        .icon(mode_icon(risk))
        .tooltip("Permissions: click or Shift-Tab for the next mode")
        .disabled(!enabled)
        .child(settings.mode_name().to_owned())
        .on_click(move |_, window, cx| on_pick(next.clone(), window, cx));
    Some(
        match mode_tone(risk) {
            Some(tone) => chip.with_variant(ButtonVariant::Paint(tone, Emphasis::Soft)),
            None => chip.text_color(cx.theme().foreground.muted()),
        }
        .into_any_element(),
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
        .type_style(TextStyle::Caption, cx)
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

#[allow(clippy::cast_possible_truncation)]
pub fn context_usage_meter(id: impl Into<ElementId>, used: u64, size: u64, cx: &App) -> AnyElement {
    chat::context_meter(
        id,
        context_usage_fraction(used, size) as f32,
        context_usage_tooltip(used, size),
        cx,
    )
}
