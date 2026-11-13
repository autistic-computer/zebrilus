use super::*;
use crate::config::mux_bindings::{
    SplitDirection, SplitPaneKind, preferred_split_binding, split_binding_kind,
    update_split_binding,
};
use zz_protocol::KeyBindingSnapshot;

#[cfg(test)]
mod tests;

const DIRECTIONS: [SplitDirection; 2] = [SplitDirection::Vertical, SplitDirection::Horizontal];
const PANE_KINDS: [(SplitPaneKind, &str); 3] = [
    (SplitPaneKind::Picker, "Pane picker"),
    (SplitPaneKind::Terminal, "Terminal"),
    (SplitPaneKind::Browser, "Browser"),
];

struct SplitControl {
    binding: Option<KeyBindingSnapshot>,
    preferred_key: Option<String>,
    spelling: String,
    key: Entity<KeyFieldState>,
}

pub(super) struct SplitControls {
    rows: [SplitControl; 2],
    pending: Option<PendingSplit>,
    timeout: Option<ala::Task<()>>,
    _subscriptions: Vec<Subscription>,
}

struct PendingSplit {
    key: String,
    kind: SplitPaneKind,
    direction: SplitDirection,
    removed_key: Option<String>,
    timed_out: bool,
}

impl SettingsView {
    pub(super) fn synchronize_mux_splits(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.section != SettingsSection::Multiplexer {
            return;
        }
        if self.mux_split_controls.is_none() {
            let rows = DIRECTIONS.map(|direction| SplitControl {
                binding: None,
                preferred_key: None,
                spelling: default_key(direction).to_owned(),
                key: key_field_state(default_key(direction), window, cx),
            });
            let subscriptions = rows
                .iter()
                .enumerate()
                .map(|(index, row)| {
                    cx.subscribe_in(
                        &row.key,
                        window,
                        move |this, field, event: &KeyFieldEvent, window, cx| {
                            let KeyFieldEvent::Record(keystroke) = event;
                            let Some(spelling) = recorded_tmux_key(keystroke) else {
                                reject_recorded_key(cx);
                                this.reset_split_key(index, field, cx);
                                return;
                            };
                            let Some(controls) = this.mux_split_controls.as_mut() else {
                                return;
                            };
                            let row = &mut controls.rows[index];
                            let kind = row
                                .binding
                                .as_ref()
                                .map_or(Some(SplitPaneKind::Picker), |binding| {
                                    split_binding_kind(binding, DIRECTIONS[index])
                                });
                            let previous = std::mem::replace(&mut row.spelling, spelling);
                            if let Some(kind) = kind
                                && this.commit_mux_split(index, kind, window, cx)
                            {
                                return;
                            }
                            if let Some(controls) = this.mux_split_controls.as_mut() {
                                controls.rows[index].spelling = previous;
                            }
                            this.reset_split_key(index, field, cx);
                        },
                    )
                })
                .collect();
            self.mux_split_controls = Some(SplitControls {
                rows,
                pending: None,
                timeout: None,
                _subscriptions: subscriptions,
            });
        }

        let bindings = self.mux.read(cx).local_prefix_bindings().to_vec();
        let controls = self.mux_split_controls.as_mut().unwrap();
        if controls.pending.as_ref().is_some_and(|pending| {
            bindings.iter().any(|binding| {
                binding.key == pending.key
                    && split_binding_kind(binding, pending.direction) == Some(pending.kind)
            }) && pending
                .removed_key
                .as_ref()
                .is_none_or(|key| !bindings.iter().any(|binding| &binding.key == key))
        }) || !self.mux.read(cx).local_is_connected()
        {
            controls.pending = None;
            controls.timeout = None;
        }
        if controls
            .pending
            .as_ref()
            .is_some_and(|pending| !pending.timed_out)
        {
            return;
        }
        for (row, direction) in controls.rows.iter_mut().zip(DIRECTIONS) {
            let binding = row
                .preferred_key
                .as_ref()
                .and_then(|key| bindings.iter().find(|binding| &binding.key == key))
                .filter(|binding| {
                    preferred_split_binding(std::slice::from_ref(*binding), direction).is_some()
                })
                .or_else(|| preferred_split_binding(&bindings, direction))
                .or_else(|| {
                    bindings
                        .iter()
                        .find(|binding| binding.key == default_key(direction))
                })
                .cloned();
            if row.binding != binding {
                let key = binding
                    .as_ref()
                    .map_or(default_key(direction), |binding| &binding.key);
                key.clone_into(&mut row.spelling);
                row.key.update(cx, |field, cx| {
                    field.set_value(zz_ui::command::tmux_keystroke(key), cx);
                });
                row.binding = binding;
            }
        }
    }

    fn mux_split_disabled_reason(&self, cx: &App) -> Option<&'static str> {
        if !self.mux.read(cx).local_is_connected() {
            return Some("The local daemon is not connected.");
        }
        if self.pending_file_command.is_some() {
            return Some("Applying multiplexer configuration…");
        }
        if self
            .mux_split_controls
            .as_ref()
            .and_then(|controls| controls.pending.as_ref())
            .is_some_and(|pending| !pending.timed_out)
        {
            return Some("Applying split shortcut…");
        }
        if self.config_file_editor(ConfigFileKind::Mux).error.is_some() {
            return Some("Resolve the configuration error below to edit split shortcuts.");
        }
        None
    }

    pub(super) fn mux_splits_section(&self, cx: &Context<Self>) -> AnyElement {
        let Some(controls) = &self.mux_split_controls else {
            return div().into_any_element();
        };
        let disabled_reason = self.mux_split_disabled_reason(cx);
        let mut stack =
            SettingsStack::titled("Split panes").when_some(disabled_reason, SettingsStack::note);
        for (index, row) in controls.rows.iter().enumerate() {
            let kind = row
                .binding
                .as_ref()
                .and_then(|binding| split_binding_kind(binding, DIRECTIONS[index]));
            let custom = row.binding.is_some() && kind.is_none();
            let disabled = disabled_reason.is_some() || custom;
            let label = kind.map_or(if custom { "Custom" } else { "Unbound" }, |kind| {
                PANE_KINDS
                    .iter()
                    .find(|(value, _)| *value == kind)
                    .unwrap()
                    .1
            });
            let title = if index == 0 {
                "Split below"
            } else {
                "Split right"
            };
            let settings = cx.entity().downgrade();
            stack = stack.child(
                SettingEntry::new(title)
                    .when(custom, |entry| {
                        entry
                            .description("Bound to another command, edit it in the overrides below")
                    })
                    .control(
                        h_flex()
                            .gap(px(8.0))
                            .flex_none()
                            .child(
                                Label::new(TextStyle::Caption, "Prefix +")
                                    .muted()
                                    .on(settings_card_ground(cx)),
                            )
                            .child(key_field_control(&row.key, disabled, cx))
                            .child(
                                Button::new(("settings-split-kind", index))
                                    .label(label)
                                    .dropdown_caret(true)
                                    .disabled(disabled)
                                    .on(settings_card_ground(cx))
                                    .dropdown_menu_with_anchor(
                                        ala::Anchor::TopRight,
                                        move |menu, _, _| {
                                            PANE_KINDS.into_iter().fold(
                                                menu,
                                                |menu, (choice, label)| {
                                                    let settings = settings.clone();
                                                    menu.item(
                                                        PopupMenuItem::new(label)
                                                            .checked(kind == Some(choice))
                                                            .on_click(move |_, window, cx| {
                                                                let _ = settings.update(
                                                                    cx,
                                                                    |this, cx| {
                                                                        this.commit_mux_split(
                                                                            index, choice, window,
                                                                            cx,
                                                                        );
                                                                    },
                                                                );
                                                            }),
                                                    )
                                                },
                                            )
                                        },
                                    ),
                            ),
                    ),
            );
        }
        if controls
            .pending
            .as_ref()
            .is_some_and(|pending| pending.timed_out)
        {
            stack = stack.child(
                SettingEntry::new("Apply saved shortcuts").control(
                    Button::new("settings-retry-split-reload")
                        .label("Reload")
                        .on(settings_card_ground(cx))
                        .disabled(disabled_reason.is_some())
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.reload_mux_configuration(cx);
                        })),
                ),
            );
        }
        div().flex_none().child(stack).into_any_element()
    }

    fn reset_split_key(&self, index: usize, field: &Entity<KeyFieldState>, cx: &mut Context<Self>) {
        let Some(controls) = &self.mux_split_controls else {
            return;
        };
        let keystroke = zz_ui::command::tmux_keystroke(&controls.rows[index].spelling);
        field.update(cx, |field, cx| field.set_value(keystroke, cx));
    }

    fn commit_mux_split(
        &mut self,
        index: usize,
        kind: SplitPaneKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if let Some(reason) = self.mux_split_disabled_reason(cx) {
            toast::push(Notification::info(reason), cx);
            return false;
        }
        let controls = self.mux_split_controls.as_ref().unwrap();
        let row = &controls.rows[index];
        if row.binding.as_ref().is_some_and(|original| {
            self.mux
                .read(cx)
                .local_prefix_bindings()
                .iter()
                .find(|binding| binding.key == original.key)
                != Some(original)
        }) {
            self.synchronize_mux_splits(window, cx);
            toast::push(
                Notification::info("The shortcut changed. Review it and try again."),
                cx,
            );
            return false;
        }
        let key = row.spelling.clone();
        let previous_key = row.binding.as_ref().map(|binding| binding.key.clone());
        if row.binding.as_ref().is_some_and(|binding| {
            split_binding_kind(binding, DIRECTIONS[index]) == Some(kind)
                && zz_mux::parse_tmux_key(key.trim()).as_deref() == Some(binding.key.as_str())
        }) {
            return true;
        }
        let file = self.config_file_editor(ConfigFileKind::Mux);
        let source = match update_split_binding(
            &file.saved,
            self.mux.read(cx).local_prefix_bindings(),
            row.binding.as_ref(),
            DIRECTIONS[index],
            &key,
            kind,
        ) {
            Ok(source) => source,
            Err(error) => {
                toast::push(Notification::error(error), cx);
                return false;
            }
        };
        if source == file.saved {
            return true;
        }
        let path = file
            .path
            .clone()
            .map_or_else(|| config_file_path(ConfigFileKind::Mux), Ok);
        let current = path.and_then(|path| {
            config::read_config_editor_source(&path, ConfigFileKind::Mux.max_bytes())
                .map(|current| (path, current))
        });
        let path = match current {
            Ok((path, current)) if current == file.saved => path,
            Ok(_) => {
                self.reload_config_editor(ConfigFileKind::Mux, cx);
                toast::push(
                    Notification::info("Configuration changed on disk. Review it and try again."),
                    cx,
                );
                return false;
            }
            Err(error) => {
                toast::push(
                    Notification::error(format!(
                        "Could not read multiplexer configuration: {error}"
                    )),
                    cx,
                );
                return false;
            }
        };
        if let Err(error) =
            config::write_config_editor_source(&path, &source, ConfigFileKind::Mux.max_bytes())
        {
            report_write_error("save", ConfigFileKind::Mux.file_name(), &error, cx);
            return false;
        }
        let file = self.config_file_editor_mut(ConfigFileKind::Mux);
        file.path = Some(path);
        file.saved = source;
        file.error = None;
        self.reload_mux_configuration(cx);
        let key = zz_mux::parse_tmux_key(key.trim()).unwrap();
        let controls = self.mux_split_controls.as_mut().unwrap();
        controls.rows[index].preferred_key = Some(key.clone());
        controls.pending = Some(PendingSplit {
            removed_key: previous_key.filter(|previous| previous != &key),
            key,
            kind,
            direction: DIRECTIONS[index],
            timed_out: false,
        });
        self.wait_for_mux_split(cx);
        true
    }

    fn wait_for_mux_split(&mut self, cx: &mut Context<Self>) {
        let controls = self.mux_split_controls.as_mut().unwrap();
        if let Some(pending) = &mut controls.pending {
            pending.timed_out = false;
        }
        controls.timeout = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_secs(5))
                .await;
            let _ = this.update(cx, |this, cx| {
                if let Some(pending) = this
                    .mux_split_controls
                    .as_mut()
                    .and_then(|controls| controls.pending.as_mut())
                {
                    pending.timed_out = true;
                    cx.notify();
                }
            });
        }));
        cx.notify();
    }

    pub(super) fn cancel_pending_mux_split(&mut self) {
        if let Some(controls) = &mut self.mux_split_controls {
            controls.pending = None;
            controls.timeout = None;
        }
    }
}

fn default_key(direction: SplitDirection) -> &'static str {
    match direction {
        SplitDirection::Vertical => "\"",
        SplitDirection::Horizontal => "%",
    }
}
