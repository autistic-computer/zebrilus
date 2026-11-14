use std::rc::Rc;

use ala::{
    AnyElement, App, ClipboardItem, ElementId, IntoElement, Pixels, SharedString, Window, div,
    prelude::*,
};
use penna::{
    ActiveTheme as _, Disableable as _, Emphasis, Ground, IconName, Space, StyledExt as _,
    TextStyle, Tone, WindowExt as _,
    blocks::chat::AskCard,
    blocks::notification::Notification,
    blocks::sheet::{
        BottomSheet, bottom_sheet, close_sheet, sheet_back, sheet_close, sheet_ground, sheet_groups,
    },
    blocks::touch::{TouchGroup, TouchInk, TouchRow, TouchTrailing},
    h_flex,
};
use zz_client::agent_settings::{self, LensSettings};
use zz_client::agent_timeline::{ContextUsage, QUOTA_WARNING, percent_label, unix_now};
use zz_protocol::{AgentAccount, AgentProvider, AgentQuota, QuotaWindows, chat::ItemId};

use super::{
    ask::{ApprovalAction, AskChoice, AskSubject},
    chat::HeldMessage,
    controls::{AccountAction, mode_icon},
    question::{QuestionCardAction, QuestionCardState},
};
use crate::compact::hold_sheet;
use crate::target::TargetItem;

pub const CHAT_SHEET: &str = "phone-chat-sheet";
pub const HOLD_SHEET: &str = "phone-chat-hold";
pub const SOMETHING_ELSE: &str = "Something else";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlusPage {
    Root,
    Setting(&'static str),
    Account,
    Mention(Vec<TargetItem>),
}

impl PlusPage {
    #[must_use]
    pub fn depth(&self) -> usize {
        match self {
            Self::Root => 0,
            Self::Setting(_) | Self::Account => 1,
            Self::Mention(drilled) => 1 + drilled.len(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlusAction {
    Open(PlusPage),
    Back,
    Set(&'static str, String),
    Account(AccountAction),
    Mention(TargetItem),
    Drill(TargetItem),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UsagePart {
    pub label: &'static str,
    pub value: String,
    pub warns: bool,
}

#[must_use]
pub fn usage_parts(context: ContextUsage, quota: Option<QuotaWindows>) -> Vec<UsagePart> {
    let context = (context != ContextUsage::Unknown).then(|| UsagePart {
        label: "Context",
        value: context.label(),
        warns: false,
    });
    let window = |label, window: Option<zz_protocol::QuotaWindow>| {
        window.map(|window| UsagePart {
            label,
            value: percent_label(f64::from(window.used)),
            warns: window.used >= QUOTA_WARNING,
        })
    };
    context
        .into_iter()
        .chain(quota.and_then(|quota| window("5-hour", quota.five_hour)))
        .chain(quota.and_then(|quota| window("Week", quota.week)))
        .collect()
}

pub struct PlusSheet<'a> {
    pub agent: SharedString,
    pub provider: Option<AgentProvider>,
    pub settings: Option<&'a LensSettings>,
    pub account: Option<&'a str>,
    pub accounts: Option<&'a [AgentAccount]>,
    pub quotas: &'a [AgentQuota],
    pub context: ContextUsage,
    pub mention: &'a [TargetItem],
    pub enabled: bool,
}

type OnPlus = Rc<dyn Fn(PlusAction, &mut Window, &mut App)>;

fn valued(row: TouchRow, value: String) -> TouchRow {
    if value.is_empty() {
        row
    } else {
        row.trailing(TouchTrailing::Value(value.into()))
    }
}

fn act(
    on_action: &OnPlus,
    action: PlusAction,
) -> impl Fn(&ala::ClickEvent, &mut Window, &mut App) + use<> {
    let on_action = Rc::clone(on_action);
    move |_, window, cx| on_action(action.clone(), window, cx)
}

impl PlusSheet<'_> {
    fn accounts(&self) -> Vec<AgentAccount> {
        let Some(provider) = self.provider else {
            return Vec::new();
        };
        self.accounts
            .unwrap_or_default()
            .iter()
            .filter(|account| account.provider == provider)
            .cloned()
            .collect()
    }

    fn account_name(&self) -> String {
        self.accounts()
            .iter()
            .find(|account| account.id.as_deref() == self.account)
            .and_then(|account| {
                if account.signed_in {
                    account.email.clone().or_else(|| account.id.clone())
                } else {
                    Some(agent_settings::SIGNED_OUT.to_owned())
                }
            })
            .or_else(|| self.account.map(str::to_owned))
            .unwrap_or_default()
    }

    fn title(&self, page: &PlusPage) -> SharedString {
        match page {
            PlusPage::Root => self.agent.clone(),
            PlusPage::Setting(agent_settings::MODEL) => "Model".into(),
            PlusPage::Setting(agent_settings::EFFORT) => "Effort".into(),
            PlusPage::Setting(_) => "Permissions".into(),
            PlusPage::Account => "Account".into(),
            PlusPage::Mention(drilled) => drilled
                .last()
                .map_or_else(|| "Mention".into(), |item| item.label.clone()),
        }
    }

    fn root_rows(&self, on_action: &OnPlus) -> Vec<TouchRow> {
        let mut rows = vec![
            TouchRow::new("chat-plus-mention", "Mention")
                .icon(IconName::ListTree)
                .more()
                .disabled(!self.enabled)
                .on_click(act(
                    on_action,
                    PlusAction::Open(PlusPage::Mention(Vec::new())),
                )),
        ];
        if let Some(settings) = self.settings {
            let setting = |id: &'static str, title: &'static str, icon, value: String, option| {
                valued(TouchRow::new(id, title).icon(icon), value)
                    .more()
                    .disabled(!self.enabled)
                    .on_click(act(on_action, PlusAction::Open(PlusPage::Setting(option))))
            };
            if !settings.models.is_empty() {
                rows.push(setting(
                    "chat-plus-model",
                    "Model",
                    IconName::Sparkle,
                    settings.model_label.clone(),
                    agent_settings::MODEL,
                ));
            }
            if !settings.efforts.is_empty() {
                rows.push(setting(
                    "chat-plus-effort",
                    "Effort",
                    IconName::Cpu,
                    settings.effort_name().to_owned(),
                    agent_settings::EFFORT,
                ));
            }
            if !settings.modes.is_empty() {
                let icon = self.provider.map_or(IconName::Hand, |provider| {
                    mode_icon(provider, &settings.mode)
                });
                rows.push(setting(
                    "chat-plus-mode",
                    "Permissions",
                    icon,
                    settings.mode_name().to_owned(),
                    agent_settings::MODE,
                ));
            }
        }
        if self.provider.is_some() {
            rows.push(
                valued(
                    TouchRow::new("chat-plus-account", "Account").icon(IconName::User),
                    self.account_name(),
                )
                .more()
                .disabled(!self.enabled)
                .on_click(act(on_action, PlusAction::Open(PlusPage::Account))),
            );
        }
        rows
    }

    fn setting_rows(&self, option: &'static str, on_action: &OnPlus) -> Vec<TouchRow> {
        let Some((choices, current)) = self.settings.and_then(|settings| settings.choices(option))
        else {
            return Vec::new();
        };
        choices
            .iter()
            .map(|choice| {
                let row = TouchRow::new(
                    SharedString::from(format!("chat-plus-{option}-{}", choice.value)),
                    choice.name.clone(),
                )
                .disabled(!self.enabled)
                .on_click(act(
                    on_action,
                    PlusAction::Set(option, choice.value.clone()),
                ));
                let row = match (option, self.provider) {
                    (agent_settings::MODE, Some(provider)) => {
                        row.icon(mode_icon(provider, &choice.value))
                    }
                    _ => row,
                };
                if choice.value == current {
                    row.trailing(TouchTrailing::Check)
                } else {
                    row
                }
            })
            .collect()
    }

    fn account_rows(&self, on_action: &OnPlus) -> Vec<TouchRow> {
        self.accounts()
            .into_iter()
            .enumerate()
            .map(|(index, account)| {
                let current = account.id.as_deref() == self.account;
                let row = TouchRow::new(
                    ("chat-plus-account-row", index),
                    agent_settings::account_name(&account),
                )
                .disabled(!self.enabled)
                .on_click(act(
                    on_action,
                    PlusAction::Account(AccountAction::Use(account.id.clone())),
                ));
                let row = match agent_settings::account_detail(&account) {
                    Some(detail) => row.detail(detail),
                    None => row,
                };
                if current {
                    row.trailing(TouchTrailing::Check)
                } else {
                    row
                }
            })
            .chain([TouchRow::new("chat-plus-account-add", "Add account")
                .icon(IconName::Plus)
                .disabled(!self.enabled)
                .on_click(act(on_action, PlusAction::Account(AccountAction::Add)))])
            .collect()
    }

    fn mention_rows(&self, drilled: &[TargetItem], on_action: &OnPlus) -> Vec<TouchRow> {
        let row = |index: usize, item: &TargetItem, inside: bool| {
            let row = TouchRow::new(("chat-plus-mention-row", index), item.label.clone())
                .icon(item.icon.clone());
            let row = if item.detail.is_empty() {
                row
            } else {
                row.detail(item.detail.clone())
            };
            if inside {
                row.trailing(TouchTrailing::Glyph(IconName::ChevronRight))
                    .on_click(act(on_action, PlusAction::Drill(item.clone())))
            } else {
                row.on_click(act(on_action, PlusAction::Mention(item.clone())))
            }
        };
        drilled
            .last()
            .map(|item| row(0, item, false))
            .into_iter()
            .chain(
                self.mention
                    .iter()
                    .enumerate()
                    .map(|(index, item)| row(index + 1, item, item.drills())),
            )
            .collect()
    }

    fn usage(&self, cx: &App) -> Option<AnyElement> {
        let provider = self.provider?;
        let quota = agent_settings::account_quota(self.quotas, provider, self.account, unix_now());
        let parts = usage_parts(self.context, quota);
        if parts.is_empty() {
            return None;
        }
        let ground = sheet_ground(cx);
        let theme = cx.theme();
        let muted = penna::muted_on(ground, cx);
        let warning = theme.paint_on(ground, Tone::Warning, Emphasis::Ghost).text;
        let count = parts.len();
        Some(
            h_flex()
                .debug_selector(|| "chat-plus-usage".to_owned())
                .mt(-Space::S8.rems())
                .px(Space::S16)
                .pb(Space::S4)
                .flex_wrap()
                .type_style(TextStyle::TouchSmall, cx)
                .text_color(muted)
                .children(parts.into_iter().enumerate().map(|(index, part)| {
                    let tail = if index + 1 < count { " · " } else { "" };
                    h_flex()
                        .whitespace_nowrap()
                        .child(
                            div()
                                .when(part.warns, |part| part.text_color(warning))
                                .child(format!("{} {}", part.label, part.value)),
                        )
                        .child(tail)
                }))
                .into_any_element(),
        )
    }

    pub fn sheet(
        &self,
        page: &PlusPage,
        bottom_inset: Pixels,
        on_action: impl Fn(PlusAction, &mut Window, &mut App) + 'static,
        on_dismiss: impl Fn(&mut Window, &mut App) + 'static,
        cx: &App,
    ) -> BottomSheet {
        let on_action: OnPlus = Rc::new(on_action);
        let rows = match page {
            PlusPage::Root => self.root_rows(&on_action),
            PlusPage::Setting(option) => self.setting_rows(option, &on_action),
            PlusPage::Account => self.account_rows(&on_action),
            PlusPage::Mention(drilled) => self.mention_rows(drilled, &on_action),
        };
        let root = *page == PlusPage::Root;
        let usage = root.then(|| self.usage(cx)).flatten();
        let title = self.title(page);
        let mut rows = rows.into_iter();
        let groups = if root {
            vec![
                TouchGroup::new().ruled().rows(rows.next()),
                TouchGroup::new().ruled().rows(rows),
            ]
        } else {
            vec![TouchGroup::new().ruled().rows(rows)]
        };
        let list = sheet_groups()
            .children(groups.into_iter().filter(|group| !group.is_empty()))
            .children(usage);
        let close = sheet_close("phone-chat-sheet-close", cx)
            .on_click(|_, _, cx| close_sheet(CHAT_SHEET, cx))
            .into_any_element();
        let sheet = bottom_sheet(CHAT_SHEET, title, [close], list, bottom_inset, on_dismiss);
        if root {
            return sheet;
        }
        let back = Rc::clone(&on_action);
        sheet.back(
            sheet_back("phone-chat-sheet-back", cx)
                .on_click(move |_, window, cx| back(PlusAction::Back, window, cx)),
        )
    }
}

pub fn held_sheet(
    held: &HeldMessage,
    on_fork: impl Fn(&ItemId, &mut Window, &mut App) + 'static,
    on_dismiss: impl Fn(&mut Window, &mut App) + 'static,
    cx: &App,
) -> BottomSheet {
    let text = held.text.clone();
    let copy = TouchRow::new("chat-hold-copy", "Copy")
        .icon(IconName::Copy)
        .on_click(move |_, window, cx| {
            cx.write_to_clipboard(ClipboardItem::new_string(text.to_string()));
            window.push_notification(
                Notification::new()
                    .message("Copied")
                    .autohide_after(std::time::Duration::from_millis(1600)),
                cx,
            );
            close_sheet(HOLD_SHEET, cx);
        });
    let fork = held.fork.clone().map(|item| {
        TouchRow::new("chat-hold-fork", "Fork from here")
            .icon(IconName::GitBranch)
            .on_click(move |_, window, cx| {
                on_fork(&item, window, cx);
                close_sheet(HOLD_SHEET, cx);
            })
    });
    hold_sheet(
        HOLD_SHEET,
        held.title.clone(),
        [copy].into_iter().chain(fork),
        on_dismiss,
        cx,
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PhoneAsk {
    Approval(ApprovalAction),
    Question(QuestionCardAction),
    Expand,
    Other(usize),
}

type OnAsk = Rc<dyn Fn(PhoneAsk, &mut Window, &mut App)>;

fn ask(
    on_ask: &OnAsk,
    action: PhoneAsk,
) -> impl Fn(&ala::ClickEvent, &mut Window, &mut App) + use<> {
    let on_ask = Rc::clone(on_ask);
    move |_, window, cx| {
        on_ask(action, window, cx);
        cx.stop_propagation();
    }
}

pub struct PhoneCard {
    pub id: ElementId,
    pub counter: Option<SharedString>,
    pub enabled: bool,
    pub expanded: bool,
    pub ground: Ground,
    pub max_height: Pixels,
}

impl PhoneCard {
    fn card(&self, title: impl Into<SharedString>) -> AskCard {
        AskCard::new(self.id.clone(), title, self.ground)
            .expanded(self.expanded)
            .max_height(self.max_height)
    }

    pub fn approval(
        &self,
        subject: AskSubject,
        choices: &[AskChoice],
        on_ask: impl Fn(PhoneAsk, &mut Window, &mut App) + 'static,
    ) -> AskCard {
        let on_ask: OnAsk = Rc::new(on_ask);
        let (card, what) = match subject {
            AskSubject::Tool {
                title, inset, note, ..
            } => {
                let card = self.card(title);
                match inset.or(note) {
                    Some(detail) => (card.detail(detail), PhoneAsk::Expand),
                    None => (card, PhoneAsk::Expand),
                }
            }
            AskSubject::Plan { markdown } => {
                let first = markdown
                    .lines()
                    .map(|line| line.trim().trim_start_matches('#').trim())
                    .find(|line| !line.is_empty())
                    .unwrap_or_default()
                    .to_owned();
                (
                    self.card("Plan").detail(first),
                    PhoneAsk::Approval(ApprovalAction::Open),
                )
            }
        };
        let card = match &self.counter {
            Some(counter) => card.count(counter.clone()),
            None => card,
        };
        let rows = choices.iter().enumerate().map(|(index, choice)| {
            TouchRow::new(
                SharedString::from(format!("{}-option-{index}", self.id)),
                choice.label.clone(),
            )
            .disabled(!self.enabled)
            .on_click(ask(
                &on_ask,
                PhoneAsk::Approval(ApprovalAction::Pick(index)),
            ))
        });
        card.on_what(ask(&on_ask, what)).rows(rows)
    }

    pub fn question(
        &self,
        state: &QuestionCardState,
        on_ask: impl Fn(PhoneAsk, &mut Window, &mut App) + 'static,
    ) -> Option<AskCard> {
        let on_ask: OnAsk = Rc::new(on_ask);
        let card = &state.card;
        let index = card.focused();
        let count = card.questions().len();
        let question = card.questions().get(index)?;
        if question.options.is_empty() {
            return None;
        }
        let shown = self.card(question.question.clone());
        let shown = if count > 1 {
            shown.count(format!("{} of {count}", index + 1))
        } else {
            match &self.counter {
                Some(counter) => shown.count(counter.clone()),
                None => shown,
            }
        };
        let rows = question
            .options
            .iter()
            .enumerate()
            .map(|(option, choice)| {
                let row = TouchRow::new(
                    SharedString::from(format!("{}-{index}-{option}", self.id)),
                    choice.label.clone(),
                )
                .disabled(!self.enabled)
                .on_click(ask(
                    &on_ask,
                    PhoneAsk::Question(QuestionCardAction::Pick {
                        question: index,
                        option,
                    }),
                ));
                let row = match &choice.description {
                    Some(detail) => row.detail(detail.clone()),
                    None => row,
                };
                if question.multi_select && card.is_picked(index, option) {
                    row.trailing(TouchTrailing::Check)
                } else {
                    row
                }
            })
            .collect::<Vec<_>>();
        let other = question.allow_other.then(|| {
            let row = TouchRow::new(
                SharedString::from(format!("{}-{index}-other", self.id)),
                if question.multi_select && card.other_selected(index) {
                    SharedString::from(card.other_text(index).to_owned())
                } else {
                    SharedString::new_static(SOMETHING_ELSE)
                },
            )
            .ink(TouchInk::Quiet)
            .disabled(!self.enabled)
            .on_click(ask(&on_ask, PhoneAsk::Other(index)));
            if question.multi_select && card.other_selected(index) {
                row.trailing(TouchTrailing::Check)
            } else {
                row
            }
        });
        let picked = card.picked(index);
        let send = (question.multi_select && picked > 0).then(|| {
            let label = if card.last_step() {
                if picked == 1 {
                    "Send 1 answer".to_owned()
                } else {
                    format!("Send {picked} answers")
                }
            } else {
                "Next".to_owned()
            };
            TouchRow::new(
                SharedString::from(format!("{}-{index}-send", self.id)),
                label,
            )
            .ink(TouchInk::Accent)
            .disabled(!self.enabled)
            .on_click(ask(&on_ask, PhoneAsk::Question(QuestionCardAction::Enter)))
        });
        Some(
            shown
                .on_what(ask(&on_ask, PhoneAsk::Expand))
                .rows(rows.into_iter().chain(other).chain(send)),
        )
    }
}

#[must_use]
pub fn other_label(state: &QuestionCardState) -> Option<SharedString> {
    let card = &state.card;
    card.questions()
        .get(card.focused())
        .map(|question| SharedString::from(question.question.clone()))
}

#[cfg(test)]
mod tests {
    use zz_protocol::QuotaWindow;

    use super::*;

    #[test]
    fn the_usage_line_meters_context_then_the_limits_and_warns_from_eighty_percent() {
        let quota = QuotaWindows {
            five_hour: Some(QuotaWindow {
                used: 0.8,
                resets_at: None,
            }),
            week: Some(QuotaWindow {
                used: 0.12,
                resets_at: None,
            }),
        };
        let parts = usage_parts(
            ContextUsage::Known {
                used: 34_000,
                size: 100_000,
            },
            Some(quota),
        );
        let line = parts
            .iter()
            .map(|part| format!("{} {}", part.label, part.value))
            .collect::<Vec<_>>()
            .join(" · ");
        assert_eq!(line, "Context 34% · 5-hour 80% · Week 12%");
        assert_eq!(
            parts.iter().map(|part| part.warns).collect::<Vec<_>>(),
            [false, true, false]
        );
        assert!(usage_parts(ContextUsage::Unknown, None).is_empty());
    }
}
