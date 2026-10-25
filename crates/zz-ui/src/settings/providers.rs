use std::rc::Rc;

use zz_gpui::{
    App, ClipboardItem, IntoElement, ParentElement as _, SharedString, Styled as _, Window, div,
    prelude::*, px,
};
use zz_protocol::{AgentProvider, ProviderStatus, ProviderTask, ProvidersReport};

use super::{SettingEntry, SettingsStack, settings_card_ground};
use crate::{
    ActiveTheme as _, Colorize as _, Disableable as _, Icon, IconName, Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    pane::agent_provider_icon,
    spinner::Spinner,
};

pub const PROVIDERS: [AgentProvider; 2] = [AgentProvider::ClaudeCode, AgentProvider::Codex];

type ProviderAction = Rc<dyn Fn(AgentProvider, &mut Window, &mut App)>;

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
            provider_entry(&id, &status, report.checking, &on_update, cx)
        })),
    }
}

fn provider_entry(
    id: &SharedString,
    status: &ProviderStatus,
    checking: bool,
    on_update: &ProviderAction,
    cx: &App,
) -> SettingEntry {
    let provider = status.provider;
    let mut entry = SettingEntry::new(provider.label())
        .description(
            status
                .path
                .clone()
                .unwrap_or_else(|| "Not installed".to_owned()),
        )
        .title_icon(Icon::new(agent_provider_icon(provider)))
        .control(
            h_flex()
                .flex_none()
                .gap(px(10.0))
                .child(version_text(status, cx))
                .children(action(id, status, checking, on_update, cx)),
        );
    if let Some(manual) = status.manual.as_ref().filter(|_| stale(status)) {
        let muted = cx.theme().foreground.muted();
        entry = match &manual.command {
            Some(command) => entry
                .child(task_line(
                    format!("Installed by {}. Update it with:", manual.owner),
                    muted,
                    cx,
                ))
                .child(task_line(command.clone(), cx.theme().foreground, cx)),
            None => entry.child(task_line(
                format!("Installed by {}. Update it there.", manual.owner),
                muted,
                cx,
            )),
        };
    }
    match &status.task {
        ProviderTask::Running { output } if !output.is_empty() => {
            entry = entry.child(task_line(output.clone(), cx.theme().foreground.muted(), cx));
        }
        ProviderTask::Failed { message } => {
            entry = entry.child(task_line(message.clone(), cx.theme().warning, cx));
        }
        ProviderTask::Running { .. } | ProviderTask::Idle => {}
    }
    entry
}

fn version_text(status: &ProviderStatus, cx: &App) -> impl IntoElement {
    let muted = cx.theme().foreground.muted();
    let text: SharedString = match (&status.version, &status.latest) {
        (Some(version), Some(latest)) if status.outdated() => {
            format!("{version} → {latest}").into()
        }
        (Some(version), _) => version.clone().into(),
        (None, Some(latest)) if !status.installed() => format!("Latest {latest}").into(),
        (None, _) if status.installed() => "Unknown version".into(),
        (None, _) => SharedString::default(),
    };
    h_flex()
        .flex_none()
        .gap(px(5.0))
        .font_family(cx.theme().mono_font_family.clone())
        .text_size(crate::rems_from_px(11.0))
        .text_color(muted)
        .when(
            status.installed() && !status.outdated() && status.latest.is_some(),
            |this| {
                this.child(
                    Icon::new(IconName::Check)
                        .small()
                        .text_color(cx.theme().success),
                )
            },
        )
        .child(text)
}

fn action(
    id: &SharedString,
    status: &ProviderStatus,
    checking: bool,
    on_update: &ProviderAction,
    cx: &App,
) -> Option<Button> {
    let provider = status.provider;
    let installed = status.installed();
    let id = SharedString::from(format!("providers-{id}-{}", provider.as_str()));
    if matches!(status.task, ProviderTask::Running { .. }) {
        return Some(
            Button::new(id)
                .loading(true)
                .disabled(true)
                .label(if installed { "Updating" } else { "Installing" })
                .on(settings_card_ground(cx)),
        );
    }
    if installed && !stale(status) {
        return None;
    }
    if let Some(manual) = &status.manual {
        let command = manual.command.clone()?;
        return Some(
            Button::new(id)
                .icon(IconName::Copy)
                .label("Copy command")
                .on(settings_card_ground(cx))
                .on_click(move |_, _, cx| {
                    cx.write_to_clipboard(ClipboardItem::new_string(command.clone()));
                }),
        );
    }
    let label = if installed { "Update" } else { "Install" };
    let on_update = Rc::clone(on_update);
    let button = Button::new(id)
        .label(label)
        .disabled(checking && status.latest.is_none() && installed)
        .on(settings_card_ground(cx))
        .on_click(move |_, window, cx| on_update(provider, window, cx));
    Some(if !installed || status.outdated() {
        button.primary()
    } else {
        button
    })
}

fn stale(status: &ProviderStatus) -> bool {
    status.outdated() || status.latest.is_none()
}

fn task_line(text: impl Into<SharedString>, color: zz_gpui::Hsla, cx: &App) -> impl IntoElement {
    div()
        .min_w_0()
        .font_family(cx.theme().mono_font_family.clone())
        .text_size(crate::rems_from_px(11.0))
        .text_color(color)
        .child(text.into())
}
