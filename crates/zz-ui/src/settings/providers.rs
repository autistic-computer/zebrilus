use std::rc::Rc;

use zz_gpui::{
    App, ClipboardItem, IntoElement, ParentElement as _, SharedString, Styled as _, Window, div,
    prelude::*, px,
};
use zz_protocol::{AgentProvider, ProviderStatus, ProvidersReport};

use super::{SettingEntry, SettingsStack, settings_card_ground};
use crate::{
    ActiveTheme as _, Colorize as _, Disableable as _, Icon, IconName, Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    pane::agent_provider_icon,
    spinner::Spinner,
};

pub use zz_client::providers::PROVIDERS;
use zz_client::providers::{
    NoteTone, ProviderAction as Action, provider_action, provider_description, provider_notes,
    version_current, version_text as version_label,
};

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
        let color = match tone {
            NoteTone::Muted => cx.theme().foreground.muted(),
            NoteTone::Foreground => cx.theme().foreground,
            NoteTone::Warning => cx.theme().warning,
        };
        entry = entry.child(task_line(note, color, cx));
    }
    entry
}

fn version_text(status: &ProviderStatus, cx: &App) -> impl IntoElement {
    let muted = cx.theme().foreground.muted();
    h_flex()
        .flex_none()
        .gap(px(5.0))
        .font_family(cx.theme().mono_font_family.clone())
        .text_size(crate::rems_from_px(11.0))
        .text_color(muted)
        .when(version_current(status), |this| {
            this.child(
                Icon::new(IconName::Check)
                    .small()
                    .text_color(cx.theme().success),
            )
        })
        .child(SharedString::from(version_label(status)))
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

fn task_line(text: impl Into<SharedString>, color: zz_gpui::Hsla, cx: &App) -> impl IntoElement {
    div()
        .min_w_0()
        .font_family(cx.theme().mono_font_family.clone())
        .text_size(crate::rems_from_px(11.0))
        .text_color(color)
        .child(text.into())
}
