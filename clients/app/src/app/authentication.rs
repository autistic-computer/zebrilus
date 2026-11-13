use ala::{
    AnyElement, AppContext as _, Context, Entity, Focusable as _, IntoElement as _,
    ParentElement as _, SharedString, Styled as _, Subscription, Window, div, prelude::*, px,
};
use penna::{
    ActiveTheme as _, Disableable as _, Root, Sizable as _, Size, WindowExt as _,
    blocks::sheet::{
        ModalAction, ModalTone, bottom_sheet, close_sheet, modal_body, modal_buttons_labelled,
        modal_detail, sheet_ground,
    },
    components::field::Field,
    components::input::{Input, InputContentType, InputEvent, InputState},
};
use zz_daemon_client::AskpassPromptKind;

use super::AppShell;
use crate::connection::{AuthenticationPrompt, Connection};

const FIELD: f32 = 48.0;
const ROOM_LEFT: f32 = 240.0;
const DETAIL_FLOOR: f32 = 88.0;

pub(super) struct PhoneAuth {
    key: (ala::EntityId, u64),
    connection: Entity<Connection>,
    prompt: AuthenticationPrompt,
    title: String,
    input: Option<Entity<InputState>>,
    _events: Option<Subscription>,
}

struct Words {
    title: String,
    action: &'static str,
    cancel: &'static str,
}

fn words(prompt: &AuthenticationPrompt, host: &str) -> Words {
    match prompt.kind {
        AskpassPromptKind::Secret => Words {
            title: format!("Sign in to {host}"),
            action: "Continue",
            cancel: "Cancel",
        },
        AskpassPromptKind::SaveKey => Words {
            title: "Sign in without a password?".to_owned(),
            action: "Add key",
            cancel: "Not now",
        },
        AskpassPromptKind::HostKey => Words {
            title: "Trust host key?".to_owned(),
            action: "Yes, connect",
            cancel: "No",
        },
        _ => Words {
            title: "Allow authentication?".to_owned(),
            action: "Yes, connect",
            cancel: "No",
        },
    }
}

fn fingerprint(word: &str) -> bool {
    word.starts_with("SHA256:") || word.starts_with("MD5:")
}

fn detail_lines(text: &str) -> Vec<(String, bool)> {
    let mut lines = Vec::new();
    for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
        let mut plain = Vec::new();
        for word in line.split_whitespace() {
            if fingerprint(word) {
                if !plain.is_empty() {
                    lines.push((plain.join(" "), false));
                    plain.clear();
                }
                lines.push((word.trim_end_matches('.').to_owned(), true));
            } else {
                plain.push(word);
            }
        }
        if !plain.is_empty() {
            lines.push((plain.join(" "), false));
        }
    }
    lines
}

impl AppShell {
    pub(super) fn sync_authentication(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((connection, prompt)) =
            self.host_connections().into_iter().find_map(|connection| {
                let prompt = connection.read(cx).pending_auth_prompt().cloned()?;
                Some((connection, prompt))
            })
        else {
            self.auth_prompt_id = None;
            self.phone_auth = None;
            return;
        };
        let key = (connection.entity_id(), prompt.id);
        if self.auth_prompt_id == Some(key)
            || window
                .root::<Root>()
                .flatten()
                .is_some_and(|root| root.read(cx).has_active_dialog())
        {
            return;
        }
        self.auth_prompt_id = Some(key);
        let host = connection
            .read(cx)
            .endpoint()
            .map_or_else(|| "host".to_owned(), super::hosts::host_title);
        if super::settings::phone() {
            self.open_phone_auth(key, connection, prompt, &host, window, cx);
            return;
        }
        let sign_in = format!("Sign in to {host}");
        let input = (prompt.kind == AskpassPromptKind::Secret)
            .then(|| cx.new(|cx| InputState::new(window, cx)));
        let focus = input.as_ref().map(|input| input.focus_handle(cx));
        window.open_dialog(cx, move |dialog, _, cx| {
            let confirm = connection.clone();
            let cancel = connection.clone();
            let dialog = if let Some(input) = &input {
                let answer = input.clone();
                zz_ui::dialogs::ssh_text_prompt_dialog(
                    dialog,
                    sign_in.clone(),
                    &prompt.text,
                    input,
                    prompt.echo,
                    cx,
                )
                .on_ok(move |_, _, cx| {
                    let value = answer.read(cx).value().to_string();
                    confirm.update(cx, |connection, cx| {
                        connection.answer_auth_prompt(prompt.id, Some(value), cx);
                    });
                    true
                })
            } else if prompt.kind == AskpassPromptKind::SaveKey {
                zz_ui::dialogs::ssh_offer_prompt_dialog(
                    dialog,
                    "Sign in without a password?",
                    &prompt.text,
                    "Add key",
                    cx,
                )
                .on_ok(move |_, _, cx| {
                    confirm.update(cx, |connection, cx| {
                        connection.answer_auth_prompt(prompt.id, Some("yes".into()), cx);
                    });
                    true
                })
            } else {
                let title = if prompt.kind == AskpassPromptKind::HostKey {
                    "Trust host key?"
                } else {
                    "Allow authentication?"
                };
                zz_ui::dialogs::ssh_confirm_prompt_dialog(dialog, title, &prompt.text, cx).on_ok(
                    move |_, _, cx| {
                        confirm.update(cx, |connection, cx| {
                            connection.answer_auth_prompt(prompt.id, Some("yes".into()), cx);
                        });
                        true
                    },
                )
            };
            dialog.on_cancel(move |_, _, cx| {
                cancel.update(cx, |connection, cx| {
                    connection.answer_auth_prompt(prompt.id, None, cx);
                });
                true
            })
        });
        if let Some(focus) = focus {
            focus.focus(window, cx);
        }
    }

    fn open_phone_auth(
        &mut self,
        key: (ala::EntityId, u64),
        connection: Entity<Connection>,
        prompt: AuthenticationPrompt,
        host: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let secret = prompt.kind == AskpassPromptKind::Secret;
        let input = secret.then(|| cx.new(|cx| InputState::new(window, cx)));
        let events = input.as_ref().map(|input| {
            cx.subscribe_in(
                input,
                window,
                |this, _, event: &InputEvent, _, cx| match event {
                    InputEvent::PressEnter { .. } => this.answer_phone_auth(true, cx),
                    InputEvent::Change => cx.notify(),
                    _ => {}
                },
            )
        });
        if let Some(input) = input.clone() {
            cx.on_next_frame(window, move |_, window, cx| {
                input.update(cx, |input, cx| input.focus(window, cx));
                window.request_virtual_keyboard();
            });
        }
        self.phone_auth = Some(PhoneAuth {
            key,
            connection,
            title: words(&prompt, host).title,
            prompt,
            input,
            _events: events,
        });
        cx.notify();
    }

    fn answer_phone_auth(&mut self, accept: bool, cx: &mut Context<Self>) {
        let Some(auth) = &self.phone_auth else {
            return;
        };
        let answer = if !accept {
            None
        } else if let Some(input) = &auth.input {
            let value = input.read(cx).value().to_string();
            if value.is_empty() {
                return;
            }
            Some(value)
        } else {
            Some("yes".to_owned())
        };
        let id = auth.prompt.id;
        let sheet = ("ssh-prompt", id);
        auth.connection.update(cx, |connection, cx| {
            connection.answer_auth_prompt(id, answer, cx);
        });
        close_sheet(sheet, cx);
    }

    pub(super) fn phone_auth_modal(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let auth = self.phone_auth.as_ref()?;
        let id = auth.prompt.id;
        let key = auth.key;
        let words = words(&auth.prompt, "");
        let ground = sheet_ground(cx);
        let ready = auth
            .input
            .as_ref()
            .is_none_or(|input| !input.read(cx).value().is_empty());
        let visible = window
            .fully_visible_bounds()
            .size
            .height
            .min(window.visual_viewport_bounds().size.height);
        let room = (visible - px(ROOM_LEFT)).max(px(DETAIL_FLOOR));
        let mono = cx.theme().mono_font_family.clone();
        let detail = modal_detail(cx).child(
            div()
                .id(("ssh-prompt-detail", id))
                .max_h(room)
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .children(
                    detail_lines(&auth.prompt.text)
                        .into_iter()
                        .map(|(line, token)| {
                            div()
                                .w_full()
                                .when(token, |line| line.font_family(mono.clone()))
                                .child(SharedString::from(line))
                        }),
                ),
        );
        let field = auth.input.as_ref().map(|input| {
            let size = Size::Size(px(FIELD));
            Field::new().with_size(size).child(
                Input::new(input)
                    .id("ssh-prompt-field")
                    .with_size(size)
                    .when(!auth.prompt.echo, |input| {
                        input.content_type(InputContentType::Password)
                    })
                    .on(ground),
            )
        });
        let view = cx.weak_entity();
        let action = ModalAction::new(
            "ssh-prompt-act",
            words.action,
            ModalTone::Accent,
            move |_, cx| {
                let _ = view.update(cx, |this, cx| this.answer_phone_auth(true, cx));
            },
        )
        .disabled(!ready);
        let buttons = modal_buttons_labelled(("ssh-prompt", id), words.cancel, action, window, cx);
        let dismissed = cx.weak_entity();
        Some(
            bottom_sheet(
                ("ssh-prompt", id),
                auth.title.clone(),
                Vec::new(),
                modal_body().child(detail).children(field).child(buttons),
                px(0.0),
                move |_, cx| {
                    let _ = dismissed.update(cx, |this, cx| {
                        if this.phone_auth.as_ref().is_some_and(|auth| auth.key == key) {
                            let auth = this.phone_auth.take();
                            if let Some(auth) = auth {
                                auth.connection.update(cx, |connection, cx| {
                                    connection.answer_auth_prompt(id, None, cx);
                                });
                            }
                        }
                        cx.notify();
                    });
                },
            )
            .modal()
            .cut_middle()
            .into_any_element(),
        )
    }
}
