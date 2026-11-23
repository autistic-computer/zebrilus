use std::rc::Rc;

use ala::{
    AnyElement, App, ClipboardItem, IntoElement, ParentElement as _, SharedString, Styled as _,
    Window, prelude::*, px,
};
use zz_protocol::{AgentAccount, AgentProvider, ProviderStatus, ProvidersReport};

use super::{SettingEntry, SettingsStack, settings_card_ground};
use crate::{
    ActiveTheme as _, Disableable as _, Icon, IconName, Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    pane::agent_provider_icon,
    spinner::Spinner,
};
use penna::{Label, TextStyle, blocks::notification::Notification};

use zz_client::agent_settings::{self, SIGNED_IN, SIGNED_OUT};
pub use zz_client::providers::PROVIDERS;
use zz_client::providers::{
    NoteTone, ProviderAction as Action, provider_action, provider_description, provider_notes,
    version_current, version_text as version_label,
};

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

#[must_use]
pub fn providers_stack(
    id: impl Into<SharedString>,
    title: impl Into<SharedString>,
    host: &ProvidersHost,
    on_update: impl Fn(AgentProvider, &mut Window, &mut App) + 'static,
    cx: &App,
) -> SettingsStack {
    providers_stack_with_auth(id, title, host, None, on_update, cx)
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
