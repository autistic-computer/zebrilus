use ala::{
    AnyElement, Context, Entity, Focusable as _, Subscription, TextInputMode, Window, div,
    prelude::*,
};
use penna::components::{
    input::{Input, InputEvent, InputState},
    select::{SelectEvent, SelectState},
    switch::Switch,
};
use zz_protocol::{CommandInvocation, MuxOptionKey};
use zz_ui::settings::{
    SettingEntry, SettingsScrollColumn, SettingsSection, SettingsSelectItem, SettingsStack,
    multiplexer::{MUX_SETTINGS, MuxControl, MuxSetting},
    settings_card_ground, settings_page_heading, settings_scroll_column,
};

use super::{choice_select, finish_field, number_control};
use crate::app::AppShell;

enum Field {
    Text(Entity<InputState>),
    Choice(Entity<SelectState<Vec<SettingsSelectItem>>>),
    Switch,
}

struct Row {
    setting: MuxSetting,
    field: Field,
    shown: Option<String>,
    sent: Option<(u64, String)>,
}

pub(super) struct MuxControls {
    rows: Vec<Row>,
    _subscriptions: Vec<Subscription>,
}

impl MuxControls {
    pub(super) fn new(window: &mut Window, cx: &mut Context<AppShell>) -> Self {
        let mut subscriptions = Vec::new();
        let mut rows = Vec::new();
        for (index, setting) in MUX_SETTINGS.into_iter().enumerate() {
            let field = match setting.control {
                MuxControl::Key | MuxControl::Number => {
                    let number = setting.control == MuxControl::Number;
                    let input = cx.new(|cx| {
                        let input = InputState::new(window, cx);
                        if number {
                            input.input_mode(TextInputMode::Numeric).min(0.0).step(1.0)
                        } else {
                            input
                        }
                    });
                    subscriptions.push(cx.subscribe_in(
                        &input,
                        window,
                        move |this, input, event, window, cx| {
                            if matches!(event, InputEvent::PressEnter { .. }) {
                                finish_field(window, cx);
                            }
                            if matches!(event, InputEvent::PressEnter { .. } | InputEvent::Blur) {
                                let value = input.read(cx).value().trim().to_owned();
                                this.set_mux_option(index, value, cx);
                            }
                        },
                    ));
                    Field::Text(input)
                }
                MuxControl::Choice(choices) => {
                    let items = choices
                        .iter()
                        .map(|&(title, value)| SettingsSelectItem::new(title, value))
                        .collect::<Vec<_>>();
                    let select = cx.new(|cx| SelectState::new(items, None, window, cx));
                    subscriptions.push(cx.subscribe_in(
                        &select,
                        window,
                        move |this, _, event: &SelectEvent<Vec<SettingsSelectItem>>, _, cx| {
                            if let SelectEvent::Confirm(Some(value)) = event {
                                this.set_mux_option(index, value.clone(), cx);
                            }
                        },
                    ));
                    Field::Choice(select)
                }
                MuxControl::Switch => Field::Switch,
            };
            rows.push(Row {
                setting,
                field,
                shown: None,
                sent: None,
            });
        }
        Self {
            rows,
            _subscriptions: subscriptions,
        }
    }
}

impl AppShell {
    fn mux_editable(&self, cx: &Context<Self>) -> bool {
        let connection = self.connection.read(cx);
        connection.connected && !connection.core.attached_read_only()
    }

    fn set_mux_option(&mut self, index: usize, value: String, cx: &mut Context<Self>) {
        let editable = self.mux_editable(cx);
        let connection = self.connection.clone();
        let row = &mut self.settings_controls.multiplexer.rows[index];
        let valid = !value.is_empty()
            && (row.setting.control != MuxControl::Number || value.parse::<u64>().is_ok());
        if !editable || !valid {
            row.shown = None;
            cx.notify();
            return;
        }
        if row.shown.as_ref() == Some(&value)
            || row.sent.as_ref().is_some_and(|(request, sent)| {
                *sent == value && connection.read(cx).awaits(*request)
            })
        {
            return;
        }
        let mut args = match connection.read(cx).core.attached_session() {
            Some(session) if row.setting.key == MuxOptionKey::Mouse => {
                vec!["-t".to_owned(), session.to_string()]
            }
            _ => vec!["-g".to_owned()],
        };
        args.extend([row.setting.key.as_str().to_owned(), value.clone()]);
        let command = CommandInvocation::new("set-option", args);
        let request = connection.update(cx, |connection, cx| connection.invoke(command, cx));
        row.sent = Some((request, value));
    }

    pub(in crate::app) fn sync_mux_controls(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.settings != Some(SettingsSection::Multiplexer) {
            return;
        }
        let connection = self.connection.read(cx);
        let options = connection.core.mux_options();
        let current = self
            .settings_controls
            .multiplexer
            .rows
            .iter()
            .map(|row| {
                let value = options
                    .get(row.setting.key)
                    .map(|option| option.value.clone())
                    .unwrap_or_default();
                let settled = row
                    .sent
                    .as_ref()
                    .is_some_and(|(request, _)| !connection.awaits(*request));
                (value, settled)
            })
            .collect::<Vec<_>>();
        for (row, (value, settled)) in self
            .settings_controls
            .multiplexer
            .rows
            .iter_mut()
            .zip(current)
        {
            if row.shown.as_ref() == Some(&value) && !settled {
                continue;
            }
            match &row.field {
                Field::Text(input) => {
                    let state = input.read(cx);
                    if state.focus_handle(cx).is_focused(window) {
                        let text = state.value();
                        let text = text.trim();
                        let untouched = row.shown.as_deref() == Some(text)
                            || (settled && row.sent.as_ref().is_some_and(|(_, sent)| sent == text));
                        if !untouched {
                            continue;
                        }
                    }
                    input.update(cx, |input, cx| input.set_value(value.clone(), window, cx));
                }
                Field::Choice(select) => {
                    select.update(cx, |select, cx| {
                        select.set_selected_value(&value, window, cx);
                    });
                }
                Field::Switch => {}
            }
            row.shown = Some(value);
            if settled {
                row.sent = None;
            }
        }
    }

    pub(super) fn multiplexer_page(
        &self,
        narrow: bool,
        cx: &mut Context<Self>,
    ) -> SettingsScrollColumn {
        let disabled = !self.mux_editable(cx);
        let options = self.connection.read(cx).core.mux_options();
        let entries = self
            .settings_controls
            .multiplexer
            .rows
            .iter()
            .enumerate()
            .map(|(index, row)| {
                let setting = row.setting;
                let control: AnyElement = match &row.field {
                    Field::Text(input) if setting.control == MuxControl::Number => {
                        number_control(input, cx).into_any_element()
                    }
                    Field::Text(input) => text_control(input, cx),
                    Field::Choice(select) => {
                        choice_select(select, setting.title, cx).into_any_element()
                    }
                    Field::Switch => {
                        let on = options
                            .get(setting.key)
                            .is_some_and(|option| option.value == "on");
                        Switch::new(format!("settings-mux-{}", setting.key.as_str()))
                            .checked(on)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let value = if on { "off" } else { "on" };
                                this.set_mux_option(index, value.to_owned(), cx);
                            }))
                            .into_any_element()
                    }
                };
                SettingEntry::new(setting.title)
                    .when_some(setting.description, SettingEntry::description)
                    .disabled(disabled)
                    .control(control)
            })
            .collect::<Vec<_>>();
        settings_scroll_column("settings-multiplexer")
            .children(
                (!narrow).then(|| settings_page_heading(SettingsSection::Multiplexer.title(), cx)),
            )
            .child(SettingsStack::titled("Options").children(entries))
    }
}

fn text_control(input: &Entity<InputState>, cx: &ala::App) -> AnyElement {
    let width = if penna::blocks::touch::CoarsePointer::get(cx) {
        110.0
    } else {
        120.0
    };
    div()
        .w(penna::rems_from_px(width))
        .max_w_full()
        .flex_none()
        .child(Input::new(input).on(settings_card_ground(cx)))
        .into_any_element()
}
