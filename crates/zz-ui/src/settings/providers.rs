use std::rc::Rc;

use ala::{
    AnyElement, App, ClipboardItem, IntoElement, ParentElement as _, SharedString, Styled as _,
    Window, div, prelude::*, px,
};
use zz_protocol::{AgentAccount, AgentProvider, ProviderStatus, ProviderTask, ProvidersReport};

use super::{
    SettingEntry, SettingsSection, SettingsStack, settings_card_ground, settings_page_title,
};
use crate::{
    ActiveTheme as _, Disableable as _, Icon, IconName, Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex, muted_on,
    pane::agent_provider_icon,
    spinner::Spinner,
    tooltip::Tooltip,
    v_flex,
};
use penna::{
    Label, Size, Space, TextStyle, blocks::notification::Notification,
    components::segmented::SegmentedControl,
};

use zz_client::agent_settings::{self, SIGNED_IN, SIGNED_OUT};
pub use zz_client::providers::PROVIDERS;
use zz_client::providers::{
    NoteTone, ProviderAction as Action, accounts_text, has_update, home_relative, provider_action,
    provider_description, provider_notes, provider_rows, row_notes, version_current,
    version_text as version_label,
};

const AGENT_COLUMN: f32 = 150.0;
const VERSION_COLUMN: f32 = 120.0;
const ACTIONS_COLUMN: f32 = 132.0;
const COLUMN_GAP: f32 = 10.0;
const ROW_CONTROL: f32 = 24.0;
const NOTE_RISE: f32 = 2.0;

type ProviderAction = Rc<dyn Fn(AgentProvider, &mut Window, &mut App)>;
type OnProviderAuth = Rc<dyn Fn(AgentProvider, ProviderAuthAction, &mut Window, &mut App)>;
type ProviderLogin = Rc<dyn Fn(AgentProvider, &App) -> Option<AnyElement>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProviderAuthAction {
    SignIn,
    AddAccount,
}

pub struct ProvidersAuth<'a> {
    pub signs_in: bool,
    pub accounts: Option<&'a [AgentAccount]>,
    pub on_auth: OnProviderAuth,
    pub login: ProviderLogin,
}

pub fn agent_setup_notification(
    result: &Result<String, String>,
    host: Option<&str>,
) -> Notification {
    let (Ok(report) | Err(report)) = result;
    log::info!(target: "zz::agents", "agent setup host={host:?} report={report:?}");
    let brief = zz_client::toast::agent_setup(report);
    let lasting = result.is_err() || brief.contains('\n');
    let (notification, title) = match result {
        Ok(_) => (Notification::success(brief), host.map(ToOwned::to_owned)),
        Err(_) => (
            Notification::error(brief),
            Some(host.map_or_else(
                || "Agent setup failed".to_owned(),
                |host| format!("Agent setup failed on {host}"),
            )),
        ),
    };
    let notification = notification.autohide(!lasting);
    match title {
        Some(title) => notification.title(title),
        None => notification,
    }
}

pub enum ProvidersHost {
    Offline(SharedString),
    Waiting,
    Report(ProvidersReport),
}

pub struct AgentsMachine {
    pub title: SharedString,
    pub host: ProvidersHost,
}

#[must_use]
pub fn agents_heading(
    update_all: Option<impl Fn(&mut Window, &mut App) + 'static>,
    on_check: impl Fn(&mut Window, &mut App) + 'static,
    cx: &App,
) -> ala::Div {
    h_flex()
        .gap(px(COLUMN_GAP))
        .child(settings_page_title(SettingsSection::Agents, cx).flex_1())
        .child(
            Button::new("settings-providers-check")
                .ghost()
                .small()
                .label("Check again")
                .on_click(move |_, window, cx| on_check(window, cx)),
        )
        .children(update_all.map(|update_all| {
            Button::new("settings-providers-update-all")
                .primary()
                .small()
                .label("Update all")
                .on_click(move |_, window, cx| update_all(window, cx))
        }))
}

#[must_use]
pub fn machine_picker(
    machines: &[AgentsMachine],
    picked: usize,
    on_pick: impl Fn(usize, &mut Window, &mut App) + 'static,
) -> Option<impl IntoElement> {
    let marked = |test: fn(&ProvidersHost) -> bool| {
        machines
            .iter()
            .enumerate()
            .filter(move |(_, machine)| test(&machine.host))
            .map(|(index, _)| index)
    };
    let dots = marked(|host| matches!(host, ProvidersHost::Report(report) if has_update(report)));
    let dimmed = marked(|host| matches!(host, ProvidersHost::Offline(_)));
    (machines.len() > 1).then(|| {
        h_flex()
            .id("settings-agents-machines")
            .w_full()
            .overflow_x_scroll()
            .restrict_scroll_to_axis()
            .child(
                SegmentedControl::new(
                    "settings-agents-machine",
                    machines.iter().map(|machine| machine.title.clone()),
                )
                .selected(picked)
                .dots(dots)
                .dimmed(dimmed)
                .on_select(on_pick),
            )
    })
}

#[must_use]
pub fn machine_card(
    id: impl Into<SharedString>,
    host: &ProvidersHost,
    auth: Option<&ProvidersAuth<'_>>,
    on_update: impl Fn(AgentProvider, &mut Window, &mut App) + 'static,
    cx: &App,
) -> SettingsStack {
    let id = id.into();
    let on_update: ProviderAction = Rc::new(on_update);
    let stack = SettingsStack::new();
    let checking = || {
        card_row(
            row()
                .child(muted(TextStyle::Body, "Checking", cx).flex_1())
                .child(Spinner::new().small()),
        )
    };
    match host {
        ProvidersHost::Offline(state) => stack.child(card_row(row().child(muted(
            TextStyle::Body,
            state.clone(),
            cx,
        )))),
        ProvidersHost::Waiting => stack.child(checking()),
        ProvidersHost::Report(report) if report.checking && report.providers.is_empty() => {
            stack.child(checking())
        }
        ProvidersHost::Report(report) => {
            let (installed, missing): (Vec<_>, Vec<_>) = provider_rows(report)
                .into_iter()
                .partition(ProviderStatus::installed);
            stack
                .children(
                    installed.iter().map(|status| {
                        agent_row(&id, status, report.checking, auth, &on_update, cx)
                    }),
                )
                .children(
                    (!missing.is_empty())
                        .then(|| install_row(&id, &missing, report.checking, &on_update, cx)),
                )
        }
    }
}

#[must_use]
pub fn agents_hook(on_set_up: impl Fn(&mut Window, &mut App) + 'static, cx: &App) -> SettingsStack {
    SettingsStack::titled("Coding agents").child(
        SettingEntry::new("Claude Code and Codex")
            .description("A startup hook so the ones in terminal panes know they run in zz")
            .control(
                Button::new("settings-agent-setup")
                    .label("Set up")
                    .on(settings_card_ground(cx))
                    .on_click(move |_, window, cx| on_set_up(window, cx)),
            ),
    )
}

fn row() -> ala::Div {
    h_flex().w_full().min_h(px(ROW_CONTROL)).gap(px(COLUMN_GAP))
}

fn card_row(content: impl IntoElement) -> SettingEntry {
    SettingEntry::bare().child(v_flex().w_full().child(content))
}

fn muted(style: TextStyle, text: impl Into<SharedString>, cx: &App) -> Label {
    Label::new(style, text).muted().on(settings_card_ground(cx))
}

fn notes_under(notes: Vec<(String, NoteTone)>, cx: &App) -> Option<impl IntoElement> {
    (!notes.is_empty()).then(|| {
        v_flex()
            .mt(px(NOTE_RISE))
            .pl(px(AGENT_COLUMN + COLUMN_GAP))
            .children(
                notes
                    .into_iter()
                    .map(|(note, tone)| task_line(note, tone, cx)),
            )
    })
}

fn agent_row(
    id: &SharedString,
    status: &ProviderStatus,
    checking: bool,
    auth: Option<&ProvidersAuth<'_>>,
    on_update: &ProviderAction,
    cx: &App,
) -> SettingEntry {
    let provider = status.provider;
    let ground = settings_card_ground(cx);
    let auth = auth.filter(|auth| auth.signs_in);
    let login = auth.and_then(|auth| (auth.login)(provider, cx));
    let signed = auth.and_then(|auth| {
        auth.accounts
            .map(|accounts| accounts_text(accounts, provider))
    });
    let auth_button = |action: ProviderAuthAction| {
        auth.map(|auth| {
            let on_auth = Rc::clone(&auth.on_auth);
            move |_: &ala::ClickEvent, window: &mut Window, cx: &mut App| {
                on_auth(provider, action, window, cx);
            }
        })
    };
    let auth_id = SharedString::from(format!("providers-auth-{id}-{}", provider.as_str()));
    let signed_out = matches!(signed, Some(None));
    let accounts = match (&status.task, signed) {
        (ProviderTask::Running { output }, _) => (!output.is_empty()).then(|| {
            task_line(output.clone(), NoteTone::Muted, cx)
                .truncate()
                .into_any_element()
        }),
        (_, Some(Some(text))) => Some(
            h_flex()
                .min_w_0()
                .gap(Space::GAP_TIGHT)
                .child(Label::new(TextStyle::Small, text).min_w_0().truncate())
                .when(provider.accounts() && login.is_none(), |this| {
                    this.children(auth_button(ProviderAuthAction::AddAccount).map(|click| {
                        Button::compact_icon(
                            auth_id.clone(),
                            Icon::new(IconName::Plus).text_color(muted_on(ground, cx)),
                        )
                        .tooltip("Add account")
                        .on(ground)
                        .on_click(click)
                    }))
                })
                .into_any_element(),
        ),
        (_, Some(None)) => Some(muted(TextStyle::Small, SIGNED_OUT, cx).into_any_element()),
        (_, None) => None,
    };
    let sign_in = (signed_out && login.is_none())
        .then(|| auth_button(ProviderAuthAction::SignIn))
        .flatten()
        .map(|click| {
            Button::new(auth_id.clone())
                .small()
                .label("Sign in")
                .on(ground)
                .on_click(click)
        });
    let path = status.path.as_deref().map(home_relative);
    let name = h_flex()
        .id(SharedString::from(format!(
            "providers-name-{id}-{}",
            provider.as_str()
        )))
        .flex_none()
        .w(px(AGENT_COLUMN))
        .gap(Space::S8)
        .child(Icon::new(agent_provider_icon(provider)).with_size(Size::Medium))
        .child(
            Label::new(TextStyle::Body, provider.label())
                .min_w_0()
                .truncate(),
        )
        .when_some(path, |this, path| {
            let path = SharedString::from(path);
            this.tooltip(move |window, cx| Tooltip::new(path.clone()).build(window, cx))
        });
    let line = row()
        .child(name)
        .child(h_flex().flex_1().min_w_0().children(accounts))
        .child(
            div()
                .flex_none()
                .w(px(VERSION_COLUMN))
                .child(version_text(status, cx)),
        )
        .child(
            h_flex()
                .flex_none()
                .min_w(px(ACTIONS_COLUMN))
                .justify_end()
                .gap(px(COLUMN_GAP))
                .children(sign_in)
                .children(action(id, status, checking, on_update, cx).map(crate::Sizable::small)),
        );
    SettingEntry::bare().child(
        v_flex()
            .w_full()
            .child(line)
            .children(notes_under(row_notes(status), cx))
            .children(login.map(|card| div().mt(Space::INSET_CARD).child(card))),
    )
}

fn install_row(
    id: &SharedString,
    missing: &[ProviderStatus],
    checking: bool,
    on_update: &ProviderAction,
    cx: &App,
) -> SettingEntry {
    let ground = settings_card_ground(cx);
    let output = missing.iter().find_map(|status| match &status.task {
        ProviderTask::Running { output } if !output.is_empty() => Some(output.clone()),
        _ => None,
    });
    let buttons = missing.iter().filter_map(|status| {
        let provider = status.provider;
        let button_id = SharedString::from(format!("providers-{id}-{}", provider.as_str()));
        Some(match provider_action(status, checking)? {
            Action::Busy { label } => Button::new(button_id)
                .small()
                .loading(true)
                .disabled(true)
                .label(label)
                .on(ground),
            Action::Update { disabled, .. } => {
                let on_update = Rc::clone(on_update);
                Button::new(button_id)
                    .small()
                    .icon(Icon::new(agent_provider_icon(provider)))
                    .label(provider.label())
                    .disabled(disabled)
                    .on(ground)
                    .on_click(move |_, window, cx| on_update(provider, window, cx))
            }
            Action::Copy { .. } => return None,
        })
    });
    let notes = missing.iter().flat_map(row_notes).collect();
    SettingEntry::bare().child(
        v_flex()
            .w_full()
            .child(
                row()
                    .child(div().flex_none().w(px(AGENT_COLUMN)).child(muted(
                        TextStyle::Body,
                        "Install",
                        cx,
                    )))
                    .child(h_flex().flex_1().min_w_0().children(
                        output.map(|output| task_line(output, NoteTone::Muted, cx).truncate()),
                    ))
                    .child(
                        h_flex()
                            .flex_none()
                            .justify_end()
                            .gap(px(COLUMN_GAP))
                            .children(buttons),
                    ),
            )
            .children(notes_under(notes, cx)),
    )
}

#[must_use]
pub fn providers_stack_with_auth(
    id: impl Into<SharedString>,
    title: impl Into<SharedString>,
    host: &ProvidersHost,
    auth: Option<&ProvidersAuth<'_>>,
    on_update: impl Fn(AgentProvider, &mut Window, &mut App) + 'static,
    cx: &App,
) -> SettingsStack {
    let id = id.into();
    let on_update: ProviderAction = Rc::new(on_update);
    let stack = SettingsStack::titled(title);
    match host {
        ProvidersHost::Offline(state) => {
            stack.child(SettingEntry::new("Not connected").description(state.clone()))
        }
        ProvidersHost::Report(report) if report.checking && report.providers.is_empty() => {
            stack.child(SettingEntry::new("Checking").control(Spinner::new().small()))
        }
        ProvidersHost::Waiting => {
            stack.child(SettingEntry::new("Checking").control(Spinner::new().small()))
        }
        ProvidersHost::Report(report) => stack.children(PROVIDERS.into_iter().map(|provider| {
            let status = report
                .provider(provider)
                .cloned()
                .unwrap_or_else(|| ProviderStatus::new(provider));
            provider_entry(&id, &status, report.checking, auth, &on_update, cx)
        })),
    }
}

fn provider_entry(
    id: &SharedString,
    status: &ProviderStatus,
    checking: bool,
    auth: Option<&ProvidersAuth<'_>>,
    on_update: &ProviderAction,
    cx: &App,
) -> SettingEntry {
    let provider = status.provider;
    let mut entry = SettingEntry::new(provider.label())
        .description(provider_description(status))
        .title_icon(Icon::new(agent_provider_icon(provider)))
        .control(
            h_flex()
                .flex_none()
                .gap(px(10.0))
                .child(version_text(status, cx))
                .children(action(id, status, checking, on_update, cx)),
        );
    for (note, tone) in provider_notes(status) {
        entry = entry.child(task_line(note, tone, cx));
    }
    if let Some(auth) = auth.filter(|auth| auth.signs_in && status.installed()) {
        if let Some(card) = (auth.login)(provider, cx) {
            entry = entry.child(card);
        } else if let Some(accounts) = auth.accounts {
            let primary_update = matches!(
                provider_action(status, checking),
                Some(Action::Update { primary: true, .. })
            );
            entry = entry.child(account_row(
                id,
                provider,
                accounts,
                !primary_update,
                &auth.on_auth,
                cx,
            ));
        }
    }
    entry
}

fn account_row(
    id: &SharedString,
    provider: AgentProvider,
    accounts: &[AgentAccount],
    primary_sign_in: bool,
    on_auth: &OnProviderAuth,
    cx: &App,
) -> impl IntoElement {
    let signed_in = accounts
        .iter()
        .filter(|account| account.provider == provider && account.signed_in)
        .collect::<Vec<_>>();
    let (summary, is_signed_in) = match signed_in.as_slice() {
        [] => (SIGNED_OUT.to_owned(), false),
        [account] => {
            let name = agent_settings::account_name(account);
            let detail = agent_settings::account_detail(account);
            let label = match detail {
                Some(detail) if name == SIGNED_IN => detail,
                Some(detail) => format!("{name} · {detail}"),
                None => name,
            };
            (label, true)
        }
        many => (
            many.iter()
                .map(|account| agent_settings::account_name(account))
                .collect::<Vec<_>>()
                .join(", "),
            true,
        ),
    };
    let button = if !is_signed_in {
        let on_auth = Rc::clone(on_auth);
        let button = Button::new(SharedString::from(format!(
            "providers-auth-{id}-{}",
            provider.as_str()
        )))
        .compact()
        .label("Sign in")
        .on(settings_card_ground(cx))
        .on_click(move |_, window, cx| on_auth(provider, ProviderAuthAction::SignIn, window, cx));
        Some(if primary_sign_in {
            button.primary()
        } else {
            button
        })
    } else if provider.accounts() {
        let on_auth = Rc::clone(on_auth);
        Some(
            Button::new(SharedString::from(format!(
                "providers-auth-{id}-{}",
                provider.as_str()
            )))
            .compact()
            .label("Add account")
            .on(settings_card_ground(cx))
            .on_click(move |_, window, cx| {
                on_auth(provider, ProviderAuthAction::AddAccount, window, cx);
            }),
        )
    } else {
        None
    };
    let summary = Label::new(TextStyle::Caption, summary).min_w_0().truncate();
    let summary = if is_signed_in {
        summary.muted().on(settings_card_ground(cx))
    } else {
        summary.text_color(cx.theme().warning)
    };
    h_flex()
        .w_full()
        .items_center()
        .justify_between()
        .gap(px(10.0))
        .child(summary)
        .children(button)
}

fn version_text(status: &ProviderStatus, cx: &App) -> impl IntoElement {
    h_flex()
        .flex_none()
        .gap(px(5.0))
        .when(version_current(status), |this| {
            this.child(
                Icon::new(IconName::Check)
                    .small()
                    .text_color(cx.theme().success),
            )
        })
        .child(
            Label::new(TextStyle::Caption, version_label(status))
                .mono()
                .muted()
                .on(settings_card_ground(cx)),
        )
}

fn action(
    id: &SharedString,
    status: &ProviderStatus,
    checking: bool,
    on_update: &ProviderAction,
    cx: &App,
) -> Option<Button> {
    let provider = status.provider;
    let id = SharedString::from(format!("providers-{id}-{}", provider.as_str()));
    Some(match provider_action(status, checking)? {
        Action::Busy { label } => Button::new(id)
            .loading(true)
            .disabled(true)
            .label(label)
            .on(settings_card_ground(cx)),
        Action::Copy { command } => Button::new(id)
            .icon(IconName::Copy)
            .label("Copy command")
            .on(settings_card_ground(cx))
            .on_click(move |_, _, cx| {
                cx.write_to_clipboard(ClipboardItem::new_string(command.clone()));
            }),
        Action::Update {
            label,
            primary,
            disabled,
        } => {
            let on_update = Rc::clone(on_update);
            let button = Button::new(id)
                .label(label)
                .disabled(disabled)
                .on(settings_card_ground(cx))
                .on_click(move |_, window, cx| on_update(provider, window, cx));
            if primary { button.primary() } else { button }
        }
    })
}

fn task_line(text: impl Into<SharedString>, tone: NoteTone, cx: &App) -> Label {
    let line = Label::new(TextStyle::Caption, text).mono().min_w_0();
    match tone {
        NoteTone::Muted => line.muted().on(settings_card_ground(cx)),
        NoteTone::Foreground => line.text_color(cx.theme().foreground),
        NoteTone::Warning => line.text_color(cx.theme().warning),
    }
}
