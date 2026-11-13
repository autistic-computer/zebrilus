use ala::{
    AnyElement, App, Context, Entity, IntoElement, Pixels, SharedString, Window, div, prelude::*,
    px,
};
use penna::{
    ActiveTheme as _, Disableable as _,
    blocks::chat::{
        ComposerAction as Action, PhoneComposer, composer_ground, phone_action_button,
        phone_close_button, phone_plus_button, phone_queued_row, phone_thumbnail,
    },
    blocks::sheet::{close_sheet, floating_sheet},
    blocks::touch::PhoneEmpty,
    components::input::InputState,
};
use zz_client::agent_lens::{AgentEmpty, PendingPermission, agent_empty};
use zz_client::agent_settings;
use zz_protocol::{AgentPaneWire, ProtocolMessage};
use zz_ui::agent::{
    ask::{AskChoice, approval_subject},
    chat::{HeldMessage, shows_chat},
    controls::{AccountAction, ContextUsage, composer_action},
    phone::{CHAT_SHEET, PhoneAsk, PhoneCard, PlusAction, PlusPage, PlusSheet, held_sheet},
    question::QuestionCardAction,
};

use super::{AgentPane, preview_image};
use crate::connection::Connection;

const DOCK_INSET: f32 = 16.0;
const DOCK_GAP: f32 = 8.0;
const EMPTY_TOP: f32 = 96.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(in super::super) struct PhoneDock {
    pub bottom: Pixels,
    pub keyboard: bool,
    pub veil: f32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum ChatSheet {
    Plus(PlusPage),
    Hold(HeldMessage),
}

struct Other {
    input: Entity<InputState>,
    question: SharedString,
    request: u64,
    pinned: bool,
}

impl AgentPane {
    pub(in super::super) fn set_phone(&mut self, dock: Option<PhoneDock>, cx: &mut Context<Self>) {
        if self.phone == dock {
            return;
        }
        if self.phone.is_some() != dock.is_some() {
            let rows = if dock.is_some() {
                penna::blocks::chat::PHONE_COMPOSER_ROWS
            } else {
                8
            };
            self.input
                .update(cx, |input, cx| input.set_max_rows(rows, cx));
            if dock.is_none() {
                self.sheet = None;
                self.other = None;
            }
        }
        self.phone = dock;
        cx.notify();
    }

    fn phone_other(&self, head: Option<&PendingPermission>, cx: &App) -> Option<Other> {
        let head = head?;
        let card = self
            .question
            .as_ref()
            .filter(|card| card.request_id() == head.request_id)?;
        let index = card.card.focused();
        let question = card.card.questions().get(index)?;
        let pinned = self.other == Some((head.request_id, index));
        if !pinned && !question.options.is_empty() {
            return None;
        }
        let input = card.other_input(index)?.clone();
        let _ = cx;
        Some(Other {
            input,
            question: question.question.clone().into(),
            request: head.request_id,
            pinned,
        })
    }

    pub(super) fn phone_row_input(&self, cx: &App) -> Entity<InputState> {
        let head = self.current_permission(cx);
        self.phone_other(head.as_ref(), cx)
            .map_or_else(|| self.input.clone(), |other| other.input)
    }

    fn phone_ask(
        &mut self,
        request_id: u64,
        action: PhoneAsk,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match action {
            PhoneAsk::Approval(action) => self.approval_action(request_id, action, window, cx),
            PhoneAsk::Question(action) => self.question_action(action, window, cx),
            PhoneAsk::Expand => {
                self.ask_open = if self.ask_open == Some(request_id) {
                    None
                } else {
                    Some(request_id)
                };
                cx.notify();
            }
            PhoneAsk::Other(question) => {
                self.other = Some((request_id, question));
                self.question_action(QuestionCardAction::Other { question }, window, cx);
                cx.notify();
            }
        }
    }

    fn leave_other(
        &mut self,
        input: &Entity<InputState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.other = None;
        input.update(cx, |input, cx| input.set_value("", window, cx));
        window.dismiss_virtual_keyboard();
        cx.notify();
    }

    fn open_sheet(&mut self, sheet: ChatSheet, window: &mut Window, cx: &mut Context<Self>) {
        window.dismiss_virtual_keyboard();
        self.sheet = Some(sheet);
        cx.notify();
    }

    pub(super) fn phone_hold(&mut self, held: &HeldMessage, cx: &mut Context<Self>) {
        if self.phone.is_some() {
            self.sheet = Some(ChatSheet::Hold(held.clone()));
            cx.notify();
        }
    }

    fn plus_action(&mut self, action: PlusAction, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ChatSheet::Plus(shown)) = self.sheet.clone() else {
            return;
        };
        let next = match action {
            PlusAction::Open(PlusPage::Mention(drilled)) => {
                self.mention
                    .update(cx, |picker, cx| picker.open_here(window, cx));
                Some(PlusPage::Mention(drilled))
            }
            PlusAction::Open(PlusPage::Account) => {
                self.connection
                    .update(cx, Connection::request_agent_accounts);
                Some(PlusPage::Account)
            }
            PlusAction::Open(page) => Some(page),
            PlusAction::Back => match shown {
                PlusPage::Mention(mut drilled) if !drilled.is_empty() => {
                    drilled.pop();
                    self.mention
                        .update(cx, |picker, cx| picker.climb(window, cx));
                    Some(PlusPage::Mention(drilled))
                }
                PlusPage::Mention(_) => {
                    self.mention
                        .update(cx, |picker, cx| picker.dismiss(window, cx));
                    Some(PlusPage::Root)
                }
                _ => Some(PlusPage::Root),
            },
            PlusAction::Set(option, value) => {
                let connection = self.connection.read(cx);
                if connection.connected && !connection.core.attached_read_only() {
                    let pane = self.pane;
                    self.connection.update(cx, |connection, cx| {
                        connection.send(
                            ProtocolMessage::AgentSetConfigOption {
                                pane,
                                option_id: option.to_owned(),
                                value,
                            },
                            cx,
                        );
                    });
                }
                Some(PlusPage::Root)
            }
            PlusAction::Account(AccountAction::Use(account)) => {
                let pane = self.pane;
                self.connection.update(cx, |connection, cx| {
                    connection.switch_agent_account(pane, account.as_deref(), cx);
                });
                Some(PlusPage::Root)
            }
            PlusAction::Account(AccountAction::Add) => {
                if let Some(provider) = self.provider(cx) {
                    self.connection.update(cx, |connection, cx| {
                        connection.add_agent_account(provider, cx);
                    });
                }
                close_sheet(CHAT_SHEET, cx);
                None
            }
            PlusAction::Account(AccountAction::Refresh) => {
                self.connection
                    .update(cx, Connection::request_agent_accounts);
                None
            }
            PlusAction::Mention(item) => {
                self.mention
                    .update(cx, |picker, cx| picker.accept_item(Some(item), window, cx));
                close_sheet(CHAT_SHEET, cx);
                None
            }
            PlusAction::Drill(item) => {
                self.mention
                    .update(cx, |picker, cx| picker.drill(&item, window, cx));
                match shown {
                    PlusPage::Mention(mut drilled) => {
                        drilled.push(item);
                        Some(PlusPage::Mention(drilled))
                    }
                    _ => None,
                }
            }
        };
        if let Some(next) = next {
            self.sheet = Some(ChatSheet::Plus(next));
        }
        cx.notify();
    }

    fn sheet_closed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if matches!(self.sheet, Some(ChatSheet::Plus(PlusPage::Mention(_)))) {
            self.mention
                .update(cx, |picker, cx| picker.dismiss(window, cx));
        }
        self.sheet = None;
        cx.notify();
    }

    fn phone_sheet(
        &mut self,
        dock: PhoneDock,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let sheet = self.sheet.clone()?;
        let entity = cx.entity();
        let dismiss = entity.downgrade();
        let on_dismiss = move |window: &mut Window, cx: &mut App| {
            let _ = dismiss.update(cx, |this, cx| this.sheet_closed(window, cx));
        };
        let sheet = match sheet {
            ChatSheet::Hold(held) => {
                let fork = entity.downgrade();
                held_sheet(
                    &held,
                    move |item, _, cx| {
                        let _ = fork.update(cx, |this, cx| this.fork_from(item.as_str(), cx));
                    },
                    on_dismiss,
                    cx,
                )
            }
            ChatSheet::Plus(page) => {
                let state = self
                    .connection
                    .read(cx)
                    .core
                    .agent_state(self.pane)
                    .cloned()
                    .unwrap_or_default();
                let settings = self.lens_settings(&state);
                let provider = self.provider(cx);
                let items = self.mention.read(cx).items().to_vec();
                let connection = self.connection.read(cx);
                let writable = connection.connected && !connection.core.attached_read_only();
                let context = ContextUsage::reported(self.usage);
                let context = if shows_chat(&self.transcript.lens.chat) {
                    context
                } else {
                    context.in_empty_chat()
                };
                let plus = PlusSheet {
                    agent: provider
                        .map_or("Chat", zz_protocol::AgentProvider::label)
                        .into(),
                    provider,
                    settings: settings.as_ref(),
                    account: agent_settings::pane_account(connection.core.snapshot(), self.pane),
                    accounts: connection.core.agent_accounts(),
                    quotas: connection.core.agent_quotas(),
                    context,
                    mention: &items,
                    enabled: writable,
                };
                let act = entity.downgrade();
                plus.sheet(
                    &page,
                    dock.bottom,
                    move |action, window, cx| {
                        let _ = act.update(cx, |this, cx| this.plus_action(action, window, cx));
                    },
                    on_dismiss,
                    cx,
                )
            }
        };
        Some(floating_sheet(sheet, window))
    }

    fn phone_card(
        &mut self,
        head: &PendingPermission,
        count: usize,
        writable: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let request_id = head.request_id;
        let card = PhoneCard {
            id: format!("phone-ask-{}-{request_id}", self.pane.0).into(),
            counter: (count > 1).then(|| SharedString::from(format!("1 of {count}"))),
            enabled: writable,
            expanded: self.ask_open == Some(request_id),
            ground: self.ground,
            max_height: window.viewport_size().height * 0.5,
        };
        let entity = cx.entity().downgrade();
        let on_ask = move |action, window: &mut Window, cx: &mut App| {
            let _ = entity.update(cx, |this, cx| {
                this.phone_ask(request_id, action, window, cx);
            });
        };
        if let Some(state) = self
            .question
            .as_ref()
            .filter(|state| state.request_id() == request_id)
        {
            return card
                .question(state, on_ask)
                .map(IntoElement::into_any_element);
        }
        if !head.questions.is_empty() || head.options.is_empty() {
            return None;
        }
        let cwd = zz_client::providers::home_relative(&self.chat_cwd(cx));
        let subject = approval_subject(
            Some(&self.transcript.lens.chat),
            request_id,
            &head.title,
            Some(&cwd),
        );
        let choices = head
            .options
            .iter()
            .map(|option| AskChoice {
                label: option.name.clone().into(),
                rejects: option.allow == Some(false),
            })
            .collect::<Vec<_>>();
        Some(card.approval(subject, &choices, on_ask).into_any_element())
    }

    #[allow(clippy::too_many_lines)]
    pub(super) fn render_phone(
        &mut self,
        state: &AgentPaneWire,
        permissions: &[PendingPermission],
        writable: bool,
        ready: bool,
        running: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let dock = self.phone.unwrap_or(PhoneDock {
            bottom: px(0.0),
            keyboard: false,
            veil: 0.0,
        });
        let head = permissions.first();
        if self.other.is_some_and(|(request, question)| {
            head.map(|head| head.request_id) != Some(request)
                || self
                    .question
                    .as_ref()
                    .is_none_or(|card| card.card.focused() != question)
        }) {
            self.other = None;
        }
        let inside = composer_ground(self.ground, cx);
        let other = self.phone_other(head, cx);
        let card = match (&other, head) {
            (None, Some(head)) => self.phone_card(head, permissions.len(), writable, window, cx),
            _ => None,
        };
        let shown = card.as_ref().and(head.map(|head| head.request_id));
        if shown.is_some() && self.carded != shown {
            window.dismiss_virtual_keyboard();
        }
        self.carded = shown;
        let mut prefix = Vec::new();
        if state.queued_prompts > 0 && card.is_none() {
            let connection = self.connection.clone();
            let pane = self.pane;
            prefix.push(phone_queued_row(
                ("phone-agent-unqueue", self.pane.0),
                format!("{} queued", state.queued_prompts),
                writable,
                move |_, cx| {
                    connection.update(cx, |connection, cx| {
                        connection.send(ProtocolMessage::AgentUnqueue { pane }, cx);
                    });
                },
                cx,
            ));
        }
        prefix.extend(self.render_error(state, cx));
        let prompting = other.is_none() && card.is_none();
        if prompting {
            prefix.extend(self.render_completions(cx));
        }
        let watch_only = self.connection.read(cx).agent_watch_only(self.pane);
        let bottom = if watch_only {
            None
        } else if let Some(card) = card {
            Some(card)
        } else {
            Some(self.phone_composer(other, state, ready, running, writable, inside, window, cx))
        };
        let over = zz_ui::target::mention_layer(&self.mention, cx)
            .filter(|_| prompting && self.sheet.is_none());
        let dock_element = div()
            .relative()
            .flex()
            .flex_col()
            .flex_none()
            .w_full()
            .gap(px(DOCK_GAP))
            .px(px(DOCK_INSET))
            .pb(px(if dock.keyboard { DOCK_GAP } else { 0.0 }))
            .opacity(1.0 - dock.veil.clamp(0.0, 1.0))
            .children(prefix)
            .children(bottom)
            .children(over.map(|over| {
                ala::deferred(
                    div()
                        .absolute()
                        .left(px(DOCK_INSET))
                        .right(px(DOCK_INSET))
                        .bottom(ala::relative(1.0))
                        .pb(px(DOCK_GAP))
                        .child(over),
                )
                .with_priority(1)
            }));
        let use_chat = shows_chat(&self.transcript.lens.chat);
        let chat = use_chat.then(|| self.render_chat(window, cx));
        let provider = self.provider(cx);
        let connected = self.connection.read(cx).connected;
        let empty = (!use_chat).then(|| {
            let empty = match agent_empty(connected, &state.phase, provider) {
                AgentEmpty::Welcome(provider) => PhoneEmpty::new(
                    zz_ui::pane::agent_provider_icon(provider),
                    zz_client::agent_lens::agent_welcome_title(provider),
                ),
                AgentEmpty::Message { text, busy: true } => PhoneEmpty::spinner(text),
                AgentEmpty::Message { text, busy: false } => {
                    PhoneEmpty::new(penna::IconName::Chat, text)
                }
            };
            div().w_full().pt(px(EMPTY_TOP)).child(empty)
        });
        let sheet = self.phone_sheet(dock, window, cx);
        div()
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .overflow_hidden()
            .text_color(cx.theme().foreground)
            .child(
                div()
                    .id("web-agent-timeline")
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .children(empty)
                    .children(chat),
            )
            .child(dock_element)
            .children(sheet)
            .capture_action(cx.listener(Self::complete))
            .capture_action(cx.listener(Self::move_completion_up))
            .capture_action(cx.listener(Self::move_completion_down))
            .map(|root| zz_ui::target::mention_keys(root, &self.mention))
            .into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn phone_composer(
        &mut self,
        other: Option<Other>,
        state: &AgentPaneWire,
        ready: bool,
        running: bool,
        writable: bool,
        inside: penna::Ground,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pane = self.pane.0;
        if let Some(other) = other {
            let typed = !other.input.read(cx).value().trim().is_empty();
            let input = other.input.clone();
            let leading = if other.pinned {
                phone_close_button(("phone-agent-other-close", pane), inside)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.leave_other(&input, window, cx);
                    }))
                    .into_any_element()
            } else {
                phone_plus_button(("phone-agent-plus", pane), inside)
                    .disabled(!writable)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.open_sheet(ChatSheet::Plus(PlusPage::Root), window, cx);
                    }))
                    .into_any_element()
            };
            let request = other.request;
            let action = phone_action_button(
                ("phone-agent-action", pane),
                Action::Send,
                writable && typed && self.can_answer(request, cx),
                inside,
            )
            .on_click(cx.listener(|this, _, window, cx| {
                this.question_action(QuestionCardAction::Enter, window, cx);
            }))
            .into_any_element();
            return PhoneComposer {
                input: other.input,
                leading,
                action,
                thumbnails: Vec::new(),
                above: Some(other.question),
                ground: self.ground,
            }
            .into_any_element();
        }
        let has_content =
            !self.input.read(cx).value().trim().is_empty() || !self.attachments.is_empty();
        let kind = match composer_action(running, has_content) {
            zz_ui::agent::controls::ComposerAction::Send => Action::Send,
            zz_ui::agent::controls::ComposerAction::Queue => Action::Queue,
            zz_ui::agent::controls::ComposerAction::Stop => Action::Stop,
        };
        let enabled = match kind {
            Action::Stop | Action::Queue => writable,
            Action::Send => ready && state.pending_permission.is_none() && has_content,
        };
        let action = phone_action_button(("phone-agent-action", pane), kind, enabled, inside);
        let action = if kind == Action::Stop {
            let connection = self.connection.clone();
            let pane = self.pane;
            action.on_click(move |_, _, cx| {
                connection.update(cx, |connection, cx| {
                    connection.send(ProtocolMessage::AgentCancel { pane }, cx);
                });
            })
        } else {
            action.on_click(cx.listener(|this, _, window, cx| this.submit(window, cx)))
        };
        let thumbnails = self
            .attachments
            .iter()
            .enumerate()
            .filter_map(|(index, image)| {
                let preview = preview_image(image)?;
                let view = cx.entity().downgrade();
                Some(phone_thumbnail(
                    ("phone-agent-image", index),
                    preview,
                    move |_, cx| {
                        let _ = view.update(cx, |this, cx| {
                            if index < this.attachments.len() {
                                this.attachments.remove(index);
                            }
                            this.draft_error = None;
                            cx.notify();
                        });
                    },
                    cx,
                ))
            })
            .collect();
        let _ = window;
        PhoneComposer {
            input: self.input.clone(),
            leading: phone_plus_button(("phone-agent-plus", pane), inside)
                .disabled(!writable)
                .on_click(cx.listener(|this, _, window, cx| {
                    this.open_sheet(ChatSheet::Plus(PlusPage::Root), window, cx);
                }))
                .into_any_element(),
            action: action.into_any_element(),
            thumbnails,
            above: None,
            ground: self.ground,
        }
        .into_any_element()
    }
}
