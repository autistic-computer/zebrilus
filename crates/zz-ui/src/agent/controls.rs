use std::{
    rc::Rc,
    sync::{Arc, LazyLock},
};

use ala::{
    Anchor, AnyElement, App, Div, ElementId, FontFeatures, IntoElement, Pixels, ScrollHandle,
    SharedString, Window, div, prelude::*, px,
};

use crate::{
    ActiveTheme as _, Colorize as _, Disableable as _, Emphasis, Ground, Icon, IconName,
    RULE_WIDTH, Sizable as _, StyledExt as _, Tone,
    button::{Button, ButtonVariant, ButtonVariants as _},
    h_flex,
    tooltip::Tooltip,
};
use penna::{
    Label, Row, RowLook, Size, TextStyle,
    components::{
        popover::{Popover, PopoverState},
        progress::Progress,
        separator::Separator,
        slider::StepRail,
    },
};
use zz_client::agent_settings::{self, Choice, LensSettings, ModeRisk};
use zz_client::agent_timeline::{
    QUOTA_WARNING, context_usage_detail, percent_label, quota_reset_detail, quota_warns, unix_now,
};
use zz_protocol::{AgentAccount, AgentProvider, AgentQuota, QuotaWindow, QuotaWindows};

use penna::blocks::chat::{self, COMPOSER_FOOTER_HEIGHT, suggestion_row};

pub use zz_client::agent_timeline::{
    ComposerAction, ContextUsage, composer_action, context_usage_fraction, context_usage_tooltip,
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
    provider: Option<AgentProvider>,
    settings: &LensSettings,
    enabled: bool,
    on_pick: impl Fn(&'static str, String, &mut Window, &mut App) + 'static,
    cx: &App,
) -> AnyElement {
    let id = id.into();
    let choices =
        !settings.models.is_empty() || !settings.efforts.is_empty() || !settings.modes.is_empty();
    let label = if settings.model_label.is_empty() {
        "Model".to_owned()
    } else {
        settings.model_label.clone()
    };
    let mode = provider
        .filter(|_| !settings.mode.is_empty())
        .map(|provider| (provider, settings.mode.as_str()));
    let tooltip = if mode.is_some() {
        format!(
            "Choose the model, effort and permissions ({}): Shift-Tab for the next mode",
            settings.mode_name()
        )
    } else {
        "Choose the model and effort".to_owned()
    };
    let trigger = chat::chrome_button(SharedString::from(format!("{id}-trigger")))
        .text_color(cx.theme().foreground.rest())
        .disabled(!enabled || !choices)
        .tooltip(tooltip)
        .when_some(mode, |button, (provider, mode)| {
            button.child(mode_glyph(provider, mode, cx))
        })
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
    let gap = cx.theme().row_gap();
    let gap_height = gap.map_or(px(0.0), |gap| gap.rems().to_pixels(window.rem_size()));
    let list = choice_rows(
        "agent-model",
        &settings.models,
        &settings.model,
        agent_settings::MODEL,
        on_pick,
        cx,
    )
    .id("agent-models")
    .max_h(px(STEP * LIST_ROWS) + gap_height * (LIST_ROWS - 1.0))
    .overflow_y_scroll()
    .track_scroll(&scroll);
    crate::v_flex()
        .w(px(PANEL))
        .p(px(6.0))
        .child(head("Model", cx))
        .child(list)
        .when(!settings.efforts.is_empty(), |panel| {
            panel.child(rule()).child(
                div()
                    .px(px(10.0))
                    .pb(px(10.0))
                    .child(effort_rail(settings, on_pick)),
            )
        })
        .when(!settings.modes.is_empty(), |panel| {
            panel
                .child(rule())
                .child(head("Permissions", cx))
                .child(choice_rows(
                    "agent-mode",
                    &settings.modes,
                    &settings.mode,
                    agent_settings::MODE,
                    on_pick,
                    cx,
                ))
        })
        .into_any_element()
}

fn choice_rows(
    id: &str,
    choices: &[Choice],
    current: &str,
    option: &'static str,
    on_pick: &Pick,
    cx: &App,
) -> Div {
    let rows = choices.iter().map(|choice| {
        let pick = on_pick.clone();
        let value = choice.value.clone();
        let chosen = choice.value == current;
        step_row(
            SharedString::from(format!("{id}-{}", choice.value)),
            choice.name.clone(),
            chosen,
        )
        .on_click(move |_, window, cx| {
            pick(option, value.clone(), window, cx);
        })
        .when(chosen, |row| {
            row.child(Icon::new(IconName::Check).with_size(px(12.0)))
        })
    });
    crate::v_flex()
        .flex_none()
        .when_some(cx.theme().row_gap(), Styled::gap_y)
        .children(rows)
}

fn rule() -> Separator {
    Separator::horizontal()
        .w_auto()
        .h(RULE_WIDTH)
        .mx(px(10.0))
        .my(px(6.0))
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AccountAction {
    Refresh,
    Use(Option<String>),
    Add,
}

type OnAccount = Rc<dyn Fn(AccountAction, &mut Window, &mut App)>;

const RING: f32 = 16.0;
const PILL_GAP: f32 = 6.0;
const METER_GAP: f32 = 12.0;
const METER_LINE: f32 = 16.0;

static TABULAR: LazyLock<FontFeatures> =
    LazyLock::new(|| FontFeatures(Arc::new(vec![("tnum".into(), 1)])));

pub struct UsagePill<'a> {
    pub provider: AgentProvider,
    pub account: Option<&'a str>,
    pub accounts: Option<&'a [AgentAccount]>,
    pub quotas: &'a [AgentQuota],
    pub context: ContextUsage,
    pub enabled: bool,
}

pub fn agent_usage_pill(
    id: impl Into<ElementId>,
    pill: &UsagePill<'_>,
    on_action: impl Fn(AccountAction, &mut Window, &mut App) + 'static,
    cx: &App,
) -> AnyElement {
    let id = id.into();
    let on_action: OnAccount = Rc::new(on_action);
    let refresh = Rc::clone(&on_action);
    let quota = agent_settings::account_quota(pill.quotas, pill.provider, pill.account, unix_now());
    let warns = quota_warns(quota.as_ref());
    let context = pill.context;
    let arc = if warns {
        cx.theme().paint(Tone::Warning, Emphasis::Soft).text
    } else {
        cx.theme().foreground.muted()
    };
    let text = TextStyle::Small.design_px();
    let trigger = chat::chrome_button(SharedString::from(format!("{id}-trigger")))
        .disabled(!pill.enabled)
        .tooltip(context.tooltip())
        .map(|button| {
            if warns {
                button.with_variant(ButtonVariant::Paint(Tone::Warning, Emphasis::Soft))
            } else {
                button.text_color(cx.theme().foreground.muted())
            }
        })
        .child(
            h_flex()
                .gap(px(PILL_GAP))
                .child(
                    div()
                        .flex_none()
                        .size(px(RING))
                        .relative()
                        .top(crate::icon::cap_center_drop(px(text), px(RING), cx))
                        .child(chat::context_ring(context.fraction() as f32, arc, cx)),
                )
                .child(
                    div()
                        .flex_none()
                        .font_features(TABULAR.clone())
                        .child(context.label()),
                ),
        )
        .on_click(move |_, window, cx| refresh(AccountAction::Refresh, window, cx));
    let accounts = pill.accounts.map(|accounts| {
        accounts
            .iter()
            .filter(|account| account.provider == pill.provider)
            .cloned()
            .collect::<Vec<_>>()
    });
    let current = pill.account.map(str::to_owned);
    Popover::new(id)
        .anchor(Anchor::BottomRight)
        .p(px(0.0))
        .trigger(trigger)
        .content(move |_, _, cx| {
            usage_panel(
                &usage_meters(context, quota),
                accounts.as_deref().unwrap_or_default(),
                current.as_deref(),
                &on_action,
                cx,
            )
        })
        .into_any_element()
}

struct UsageMeter {
    label: &'static str,
    detail: Option<String>,
    fraction: f64,
    warns: bool,
}

fn usage_meters(context: ContextUsage, quota: Option<QuotaWindows>) -> Vec<UsageMeter> {
    let window = |label, window: Option<QuotaWindow>| {
        window.map(|window| UsageMeter {
            label,
            detail: reset_detail(window.resets_at),
            fraction: f64::from(window.used),
            warns: window.used >= QUOTA_WARNING,
        })
    };
    let detail = match context {
        ContextUsage::Known { used, size } => Some(Some(context_usage_detail(used, size))),
        ContextUsage::EmptyChat => Some(None),
        ContextUsage::Unknown => None,
    };
    detail
        .map(|detail| UsageMeter {
            label: "Context",
            detail,
            fraction: context.fraction(),
            warns: false,
        })
        .into_iter()
        .chain(quota.and_then(|quota| window("5-hour", quota.five_hour)))
        .chain(quota.and_then(|quota| window("Week", quota.week)))
        .collect()
}

fn reset_detail(resets_at: Option<u64>) -> Option<String> {
    let offset = resets_at
        .and_then(|at| chrono::DateTime::from_timestamp(i64::try_from(at).ok()?, 0))
        .map_or(0, |at| {
            i64::from(at.with_timezone(&chrono::Local).offset().local_minus_utc())
        });
    quota_reset_detail(resets_at, unix_now(), offset)
}

fn usage_panel(
    meters: &[UsageMeter],
    accounts: &[AgentAccount],
    current: Option<&str>,
    on_action: &OnAccount,
    cx: &mut ala::Context<PopoverState>,
) -> AnyElement {
    let popover = cx.entity();
    let pick = |action: Option<AccountAction>| {
        let on_action = Rc::clone(on_action);
        let popover = popover.clone();
        move |_: &ala::ClickEvent, window: &mut Window, cx: &mut App| {
            popover.update(cx, |popover, cx| popover.dismiss(window, cx));
            if let Some(action) = action.clone() {
                on_action(action, window, cx);
            }
        }
    };
    let rows = accounts.iter().enumerate().map(|(index, account)| {
        let chosen = account.id.as_deref() == current;
        suggestion_row(
            ("agent-account-row", index),
            agent_settings::account_name(account),
            agent_settings::account_detail(account).map(SharedString::from),
            None,
            chosen,
            cx,
        )
        .px(px(10.0))
        .on_click(pick(
            (!chosen).then(|| AccountAction::Use(account.id.clone())),
        ))
        .when(chosen, |row| {
            row.child(Icon::new(IconName::Check).with_size(px(12.0)))
        })
    });
    let metered = !meters.is_empty();
    let meters = meters
        .iter()
        .enumerate()
        .map(|(index, meter)| usage_meter(("agent-usage-meter", index), meter, cx));
    crate::v_flex()
        .w(px(PANEL))
        .p(px(6.0))
        .when(metered, |panel| {
            panel
                .child(
                    crate::v_flex()
                        .gap(px(METER_GAP))
                        .px(px(10.0))
                        .pt(px(8.0))
                        .pb(px(6.0))
                        .children(meters),
                )
                .child(rule())
        })
        .when(!accounts.is_empty(), |panel| {
            panel
                .child(
                    crate::v_flex()
                        .when_some(cx.theme().row_gap(), Styled::gap_y)
                        .children(rows),
                )
                .child(rule())
        })
        .child(add_account_row(cx).on_click(pick(Some(AccountAction::Add))))
        .into_any_element()
}

fn usage_meter(id: impl Into<ElementId>, meter: &UsageMeter, cx: &App) -> Div {
    let label = TextStyle::Small;
    let note = TextStyle::Caption;
    crate::v_flex()
        .gap(px(6.0))
        .child(
            h_flex()
                .h(px(METER_LINE))
                .gap(px(4.0))
                .child(
                    Label::new(label, meter.label)
                        .flex_none()
                        .line_height(px(METER_LINE)),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .children(meter.detail.as_ref().map(|detail| {
                            Label::new(note, format!("· {detail}"))
                                .muted()
                                .truncate()
                                .line_height(px(METER_LINE))
                                .relative()
                                .top(baseline_drop(label, note, cx))
                        })),
                )
                .child(
                    Label::new(label, percent_label(meter.fraction))
                        .flex_none()
                        .line_height(px(METER_LINE))
                        .font_features(TABULAR.clone())
                        .when(meter.warns, |pct| pct.text_color(cx.theme().warning)),
                ),
        )
        .child(
            Progress::new(id, meter.fraction as f32)
                .label(meter.label)
                .with_size(Size::Small),
        )
}

fn baseline_drop(line: TextStyle, own: TextStyle, cx: &App) -> Pixels {
    let text = cx.text_system();
    let font = text.resolve_font(&ala::font(cx.theme().font_family.clone()));
    let middle = |size: f32| (text.ascent(font, px(size)) + text.descent(font, px(size))) / 2.0;
    middle(line.design_px()) - middle(own.design_px())
}

fn add_account_row(cx: &App) -> Row {
    let muted = cx.theme().foreground.muted();
    Row::new("agent-account-add", RowLook::Menu)
        .state(crate::row_state(false, true))
        .row_height(px(STEP))
        .h_flex()
        .flex_none()
        .h(px(STEP))
        .w_full()
        .gap(px(8.0))
        .px(px(10.0))
        .click_cursor()
        .child(
            Icon::new(IconName::Plus)
                .with_size(px(12.0))
                .flex_none()
                .text_color(muted)
                .relative()
                .top(crate::icon::cap_center_drop(px(12.0), px(12.0), cx)),
        )
        .child(
            Label::new(TextStyle::Small, "Add account")
                .flex_1()
                .min_w_0()
                .text_color(muted),
        )
}

const fn mode_tone(risk: ModeRisk) -> Option<Tone> {
    match risk {
        ModeRisk::Asks => None,
        ModeRisk::ReadOnly => Some(Tone::Success),
        ModeRisk::Unasked => Some(Tone::Warning),
        ModeRisk::Unguarded => Some(Tone::Danger),
    }
}

fn mode_icon(provider: AgentProvider, mode: &str) -> IconName {
    match (provider, mode) {
        (AgentProvider::ClaudeCode, "default") => IconName::Hand,
        (AgentProvider::ClaudeCode, "acceptEdits") | (AgentProvider::Codex, "auto") => {
            IconName::Pencil
        }
        (AgentProvider::ClaudeCode, "plan") => IconName::Map,
        (AgentProvider::ClaudeCode, "auto") => IconName::Bolt,
        (AgentProvider::ClaudeCode, "bypassPermissions")
        | (AgentProvider::Codex, "full-access") => IconName::ShieldOff,
        (AgentProvider::Codex, "read-only") => IconName::Eye,
        _ => match agent_settings::mode_risk(provider, mode) {
            ModeRisk::Asks => IconName::Hand,
            ModeRisk::ReadOnly => IconName::Eye,
            ModeRisk::Unasked => IconName::Bolt,
            ModeRisk::Unguarded => IconName::ShieldOff,
        },
    }
}

fn mode_glyph(provider: AgentProvider, mode: &str, cx: &App) -> Icon {
    let risk = agent_settings::mode_risk(provider, mode);
    let color = mode_tone(risk).map_or_else(
        || cx.theme().foreground.muted(),
        |tone| cx.theme().paint(tone, Emphasis::Soft).text,
    );
    let size = px(TextStyle::Small.design_px());
    Icon::new(mode_icon(provider, mode))
        .with_size(size)
        .flex_none()
        .text_color(color)
        .relative()
        .top(crate::icon::cap_center_drop(size, size, cx))
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
