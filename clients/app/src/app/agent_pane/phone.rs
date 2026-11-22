use std::rc::Rc;

use ala::{
    AnyElement, App, Context, Entity, IntoElement, Pixels, ScrollHandle, SharedString, Window,
    canvas, div, prelude::*, px,
};
use penna::{
    ActiveTheme as _, Disableable as _,
    blocks::chat::{
        ComposerAction as Action, PhoneComposer, composer_ground, phone_action_button,
        phone_close_button, phone_file_tile, phone_options_button, phone_queued_row,
        phone_thumbnail,
    },
    blocks::compact::edge_blur,
    blocks::sheet::{PageTurn, bottom_sheet, close_sheet, floating_sheet, sheet_close},
    blocks::touch::PhoneEmpty,
    components::input::InputState,
};

use zz_client::agent_lens::{AgentEmpty, PendingPermission, agent_empty};
use zz_client::agent_login::LoginAccount;
use zz_client::agent_resume::{Menu, folder_name};
use zz_client::agent_settings;
use zz_client::window_summary::{PaneTile, summarize_window};
use zz_protocol::{AgentPaneWire, ProtocolMessage, ViewDescriptor};
use zz_ui::agent::{
    ask::{AskChoice, approval_subject},
    chat::{HeldMessage, shows_chat},
    controls::{AccountAction, ContextUsage, composer_action},
    phone::{
        AttachFrom, CHAT_SHEET, PhoneAsk, PhoneCard, PlusAction, PlusPage, PlusSheet, held_sheet,
        resume_sheet, start_rows,
    },
    question::QuestionCardAction,
    tasks::phone_plan_tray,
};
use zz_ui::sky::{SkyZones, sky};
use zz_ui::target::{TargetItem, TargetPicker};

use super::{AgentPane, preview_image};
use crate::app::view_pane::ViewPane;
use crate::connection::Connection;
use zz_ui::agent::preview::ViewRequest;

pub(super) const SEARCH: &str = "Search";
const DOCK_INSET: f32 = 16.0;
const DOCK_GAP: f32 = 8.0;
const VIEW_SHEET_TOP: f32 = 8.0;
const VIEW_SHEET_BOTTOM: f32 = 16.0;
const BOTTOM_FADE: f32 = 32.0;
const START_GAP: f32 = 24.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(in super::super) struct PhoneDock {
    pub bottom: Pixels,
    pub top: Pixels,
    pub reserve: Pixels,
    pub keyboard: bool,
    pub veil: f32,
    pub covered: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum ChatSheet {
    Plus(PlusPage),
    Hold(HeldMessage),
    Resume(Menu),
    View(String),
}

pub(super) struct PlusTurn {
    left: PlusPage,
    items: Vec<TargetItem>,
    headers: Vec<(usize, SharedString)>,
    turn: PageTurn,
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

    pub(super) fn open_sheet(
        &mut self,
        sheet: ChatSheet,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.dismiss_virtual_keyboard();
        self.sheet = Some(sheet);
        self.turn = None;
        self.typed = false;
        cx.notify();
    }

    fn loop_sheet(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        static LOOPING: std::sync::Once = std::sync::Once::new();
        let Some(period) = std::env::var("ZZ_ALA_SHEET_LOOP")
            .ok()
            .and_then(|value| value.parse().ok())
            .map(std::time::Duration::from_millis)
        else {
            return;
        };
        LOOPING.call_once(|| {
            cx.spawn_in(window, async move |this, cx| {
                cx.background_executor()
                    .timer(std::time::Duration::from_secs(8))
                    .await;
                loop {
                    cx.background_executor().timer(period).await;
                    let at = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_or(0.0, |time| time.as_secs_f64() * 1000.0);
                    let toggled = this.update_in(cx, |this, window, cx| {
                        if this.sheet.is_none() {
                            eprintln!("[zz-sheet] open at={at:.1}");
                            this.open_sheet(ChatSheet::Plus(PlusPage::Root), window, cx);
                        } else {
                            eprintln!("[zz-sheet] close at={at:.1}");
                            close_sheet(CHAT_SHEET, cx);
                            cx.notify();
                        }
                    });
                    if toggled.is_err() {
                        break;
                    }
                }
            })
            .detach();
        });
    }

    fn mention_turned(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.mention_level = self
            .mention
            .update(cx, TargetPicker::level_and_filter)
            .map(|(level, _)| level)
            .unwrap_or_default();
        self.reset_search("", SEARCH, window, cx);
    }

    pub(super) fn follow_mention(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.phone.is_none() || self.sheet.is_some() || !self.mention.read(cx).is_open() {
            return;
        }
        let Some((level, filter)) = self.mention.update(cx, TargetPicker::level_and_filter) else {
            return;
        };
        self.mention_level = level;
        self.sheet = Some(ChatSheet::Plus(PlusPage::Mention(Vec::new())));
        self.turn = None;
        self.typed = true;
        self.reset_search(&filter, SEARCH, window, cx);
        self.search.update(cx, |input, cx| input.focus(window, cx));
        cx.notify();
    }

    pub(super) fn search_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.search.read(cx).value().to_string();
        match self.sheet {
            Some(ChatSheet::Plus(PlusPage::Mention(_))) => {
                let level = self.mention_level.clone();
                self.mention
                    .update(cx, |picker, cx| picker.search(&level, &text, window, cx));
            }
            _ if self.resume.menu().is_some() && self.resume.set_search(&text) => {
                self.menu_scroll = ScrollHandle::new();
                self.send_wanted(cx);
                cx.notify();
            }
            _ => {}
        }
    }

    fn open_resume(&mut self, menu: Menu, window: &mut Window, cx: &mut Context<Self>) {
        self.request_resume(cx);
        self.resume.open(menu);
        self.reset_search("", menu.placeholder(), window, cx);
        self.open_sheet(ChatSheet::Resume(menu), window, cx);
        self.send_wanted(cx);
    }

    fn pick_resume(&mut self, index: usize, cx: &mut Context<Self>) {
        if !self.pick_row(index, cx) {
            return;
        }
        cx.haptic(ala::Haptic::Selection);
        close_sheet(CHAT_SHEET, cx);
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
        let items = self.mention.read(cx).items().to_vec();
        let headers = self.mention.read(cx).headers().to_vec();
        let left = shown.clone();
        let next = match action {
            PlusAction::Open(PlusPage::Mention(drilled)) => {
                self.mention
                    .update(cx, |picker, cx| picker.open_here(window, cx));
                self.mention_turned(window, cx);
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
                    self.mention_turned(window, cx);
                    Some(PlusPage::Mention(drilled))
                }
                PlusPage::Mention(_) if self.typed => {
                    close_sheet(CHAT_SHEET, cx);
                    None
                }
                PlusPage::Mention(_) => {
                    self.mention
                        .update(cx, |picker, cx| picker.dismiss(window, cx));
                    Some(PlusPage::Root)
                }
                PlusPage::SignIn => Some(PlusPage::Account),
                _ => Some(PlusPage::Root),
            },
            PlusAction::Set(option, value) => {
                cx.haptic(ala::Haptic::Selection);
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
                let pane = self.pane;
                let signs_in = self.connection.read(cx).signs_in();
                if let Some(provider) = self.provider(cx) {
                    self.connection.update(cx, |connection, cx| {
                        connection.start_agent_login(provider, LoginAccount::New, pane, cx);
                    });
                }
                if signs_in {
                    Some(PlusPage::SignIn)
                } else {
                    close_sheet(CHAT_SHEET, cx);
                    None
                }
            }
            PlusAction::Account(AccountAction::SignIn(account)) => {
                let pane = self.pane;
                if let Some(provider) = self.provider(cx) {
                    self.connection.update(cx, |connection, cx| {
                        connection.start_agent_login(
                            provider,
                            LoginAccount::of(account.as_deref()),
                            pane,
                            cx,
                        );
                    });
                }
                Some(PlusPage::SignIn)
            }
            PlusAction::Login(event) => {
                self.login_event(event, window, cx);
                None
            }
            PlusAction::Account(AccountAction::Refresh) => {
                self.connection
                    .update(cx, Connection::request_agent_accounts);
                None
            }
            PlusAction::Mention(item) => {
                cx.haptic(ala::Haptic::Selection);
                self.mention
                    .update(cx, |picker, cx| picker.accept_item(Some(item), window, cx));
                close_sheet(CHAT_SHEET, cx);
                None
            }
            PlusAction::Attach(from) => {
                close_sheet(CHAT_SHEET, cx);
                self.attach_from(from, cx);
                None
            }
            PlusAction::Drill(item) => {
                self.mention
                    .update(cx, |picker, cx| picker.drill(&item, window, cx));
                self.mention_turned(window, cx);
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
            if next != left {
                self.turn = Some(PlusTurn {
                    turn: PageTurn::new(left.depth(), next.depth(), cx),
                    left,
                    items,
                    headers,
                });
            }
            self.sheet = Some(ChatSheet::Plus(next));
        }
        cx.notify();
    }

    pub(super) fn open_view_sheet(
        &mut self,
        request: ViewRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let descriptor = ViewDescriptor {
            origin: Some(self.pane),
            title: request.title.clone(),
            content: request.content,
        };
        let pane = self.pane;
        let connection = self.connection.clone();
        let origin = cx.entity();
        let vendor = self
            .provider(cx)
            .and_then(zz_client::window_summary::provider_program);
        let view = cx.new(|cx| {
            let mut view = ViewPane::new(pane, &descriptor, connection, cx)
                .closing_with(|cx| close_sheet(CHAT_SHEET, cx));
            view.synchronize(&descriptor, vendor, Some(origin), cx);
            view
        });
        self.view_sheet = Some(view);
        self.open_sheet(ChatSheet::View(request.title), window, cx);
    }

    fn sheet_closed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.turn = None;
        match self.sheet {
            Some(ChatSheet::Plus(PlusPage::Mention(_))) => {
                self.mention
                    .update(cx, |picker, cx| picker.dismiss(window, cx));
            }
            Some(ChatSheet::Resume(_)) => {
                self.resume.close();
            }
            Some(ChatSheet::View(_)) => {
                self.view_sheet = None;
            }
            _ => {}
        }
        self.sheet = None;
        if std::mem::take(&mut self.typed) {
            self.input.update(cx, |input, cx| input.focus(window, cx));
        }
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
            ChatSheet::View(title) => {
                let view = self.view_sheet.clone()?;
                let scroll = view.read(cx).scroll();
                let close = sheet_close("phone-view-close", cx)
                    .on_click(|_, _, cx| close_sheet(CHAT_SHEET, cx))
                    .into_any_element();
                let body = div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .pb(px(VIEW_SHEET_BOTTOM))
                    .overflow_hidden()
                    .child(view);
                bottom_sheet(CHAT_SHEET, title, [close], body, dock.bottom, on_dismiss)
                    .content_scroll(scroll)
                    .full(dock.top + px(VIEW_SHEET_TOP))
            }
            ChatSheet::Resume(menu) => {
                let rows = self.resume_rows(cx);
                let pick = entity.downgrade();
                resume_sheet(
                    menu,
                    &rows,
                    &self.search,
                    dock.bottom,
                    move |index, _, cx| {
                        let _ = pick.update(cx, |this, cx| this.pick_resume(index, cx));
                    },
                    on_dismiss,
                    cx,
                )
            }
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
                let headers = self.mention.read(cx).headers().to_vec();
                let connection = self.connection.read(cx);
                let panes: Vec<PaneTile> = connection
                    .core
                    .snapshot()
                    .sessions
                    .iter()
                    .flat_map(|session| &session.windows)
                    .flat_map(|window| summarize_window(window).tiles)
                    .collect();
                let writable = connection.connected && !connection.core.attached_read_only();
                let context = ContextUsage::reported(self.usage);
                let context = if shows_chat(&self.transcript.lens.chat) {
                    context
                } else {
                    context.in_empty_chat()
                };
                let turn = self.turn.take().filter(|turn| turn.turn.running(cx));
                let mut plus = PlusSheet {
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
                    headers: &headers,
                    search: &self.search,
                    typed: self.typed,
                    panes: &panes,
                    enabled: writable,
                    signs_in: connection.signs_in(),
                    login: self.login.as_ref(),
                    code: self.code.as_ref().map(|field| &field.input),
                    git: connection
                        .core
                        .agent_state(self.pane)
                        .and_then(|state| state.git.as_ref()),
                    attach: self.attach_sources(cx),
                };
                let act = entity.downgrade();
                let sheet = plus.sheet(
                    &page,
                    dock.bottom,
                    move |action, window, cx| {
                        let _ = act.update(cx, |this, cx| this.plus_action(action, window, cx));
                    },
                    on_dismiss,
                    cx,
                );
                let sheet = match &turn {
                    Some(turn) => {
                        plus.mention = &turn.items;
                        plus.headers = &turn.headers;
                        let left = plus.sheet(&turn.left, dock.bottom, |_, _, _| {}, |_, _| {}, cx);
                        sheet.turned_from(left, turn.turn)
                    }
                    None => sheet,
                };
                self.turn = turn;
                sheet
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
    pub(super) fn chat_insets(&self) -> (Pixels, Pixels) {
        self.phone.map_or((px(0.0), px(0.0)), |dock| {
            (
                dock.top,
                dock.reserve + self.dock_height.get() + px(DOCK_GAP),
            )
        })
    }

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
            top: px(0.0),
            reserve: px(0.0),
            keyboard: false,
            veil: 0.0,
            covered: false,
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
        if let Some(plan) = self.current_plan(running).filter(|_| card.is_none()) {
            let entity = cx.entity();
            prefix.extend(phone_plan_tray(
                &format!("phone-agent-{}", self.pane.0),
                &plan,
                self.plan_open,
                (window.visual_viewport_bounds().bottom() - dock.top - dock.reserve) * 0.5,
                move |_, cx| {
                    entity.update(cx, |this, cx| {
                        this.plan_open = !this.plan_open;
                        cx.notify();
                    });
                },
                cx,
            ));
        }
        prefix.extend(self.shown_error(state).map(|(error, failed)| {
            zz_ui::agent::presentation::phone_error_card(error, failed, self.ground, cx)
        }));
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
        let measured = Rc::clone(&self.dock_height);
        let dock_element = div()
            .absolute()
            .left_0()
            .right_0()
            .bottom(dock.reserve)
            .flex()
            .flex_col()
            .gap(px(DOCK_GAP))
            .px(px(DOCK_INSET))
            .pb(px(if dock.keyboard { DOCK_GAP } else { 0.0 }))
            .opacity(if self.sheet.is_some() && dock.keyboard {
                0.0
            } else {
                1.0 - dock.veil.clamp(0.0, 1.0)
            })
            .children(prefix)
            .children(bottom)
            .child(
                canvas(
                    move |bounds, window, _| {
                        if (measured.get() - bounds.size.height).abs() > px(0.5) {
                            measured.set(bounds.size.height);
                            window.refresh();
                        }
                    },
                    |_, (), _, _| {},
                )
                .absolute()
                .inset_0(),
            );
        let use_chat = shows_chat(&self.transcript.lens.chat);
        let loading = self.connection.read(cx).agent_loading(self.pane);
        let chat = if loading {
            Some(self.render_skeleton(cx))
        } else {
            use_chat.then(|| self.render_chat(window, cx))
        };
        let provider = self.provider(cx);
        let connected = self.connection.read(cx).connected;
        let empty = (!use_chat && !loading).then(|| {
            let kind = agent_empty(connected, &state.phase, provider, self.settling(cx));
            let start = matches!(kind, AgentEmpty::Welcome(_)) && writable && !watch_only;
            let empty = match kind {
                AgentEmpty::Welcome(provider) => PhoneEmpty::new(
                    zz_ui::pane::agent_provider_icon(provider),
                    zz_client::agent_lens::agent_welcome_title(provider),
                ),
                AgentEmpty::Message { text, busy: true } => PhoneEmpty::spinner(text),
                AgentEmpty::Message { text, busy: false } => {
                    PhoneEmpty::new(penna::IconName::Chat, text)
                }
            };
            (empty, start)
        });
        let start = empty.as_ref().is_some_and(|(_, start)| *start);
        if start {
            self.request_resume(cx);
            self.loop_sheet(window, cx);
        }
        let folder = folder_name(&self.cwd(cx));
        let open = cx.entity().downgrade();
        let zones = SkyZones::default();
        let covered = dock.covered || self.sheet.is_some() || !self.completions.is_empty();
        let sky = start.then(|| sky(("phone-agent-sky", self.pane.0), &zones).paused(covered));
        let empty = empty.map(|(empty, _)| {
            zones.mark(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .flex_col()
                    .justify_center()
                    .pt(dock.top)
                    .pb(dock.reserve + self.dock_height.get())
                    .child(empty)
                    .when(start, |column| {
                        column.child(div().w_full().px(px(DOCK_INSET)).pt(px(START_GAP)).child(
                            start_rows(&folder, writable, move |menu, window, cx| {
                                let _ =
                                    open.update(cx, |this, cx| this.open_resume(menu, window, cx));
                            }),
                        ))
                    }),
            )
        });
        let sheet = self.phone_sheet(dock, window, cx);
        let fade = use_chat.then(|| {
            edge_blur(
                penna::Edge::Bottom,
                dock.reserve + self.dock_height.get() + px(BOTTOM_FADE),
                cx.theme().background,
            )
        });
        div()
            .relative()
            .size_full()
            .overflow_hidden()
            .text_color(cx.theme().foreground)
            .child(
                div()
                    .id("web-agent-timeline")
                    .absolute()
                    .inset_0()
                    .overflow_hidden()
                    .children(sky)
                    .children(empty)
                    .children(chat),
            )
            .children(fade)
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
                phone_options_button(("phone-agent-options", pane), inside)
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
        let has_content = !self.input.read(cx).value().trim().is_empty()
            || !self.attachments.is_empty()
            || !self.files.is_empty();
        let kind = match composer_action(running, has_content) {
            zz_ui::agent::controls::ComposerAction::Send => Action::Send,
            zz_ui::agent::controls::ComposerAction::Queue => Action::Queue,
            zz_ui::agent::controls::ComposerAction::Stop => Action::Stop,
        };
        let enabled = match kind {
            Action::Stop | Action::Queue => writable,
            Action::Send => {
                ready && state.pending_permission.is_none() && has_content && self.upload.is_none()
            }
        };
        let action = phone_action_button(("phone-agent-action", pane), kind, enabled, inside);
        let action = if kind == Action::Stop {
            action.on_click(cx.listener(|this, _, _, cx| this.cancel(cx)))
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
            .chain(self.files.iter().enumerate().map(|(index, file)| {
                let view = cx.entity().downgrade();
                phone_file_tile(
                    ("phone-agent-file", index),
                    file.name.clone(),
                    crate::attachments::file_meta(&file.name, file.data.len() as u64),
                    move |_, cx| {
                        let _ = view.update(cx, |this, cx| {
                            if index < this.files.len() {
                                this.files.remove(index);
                            }
                            this.draft_error = None;
                            cx.notify();
                        });
                    },
                    cx,
                )
            }))
            .collect();
        let _ = window;
        PhoneComposer {
            input: self.input.clone(),
            leading: phone_options_button(("phone-agent-options", pane), inside)
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

    fn attach_sources(&self, cx: &App) -> Vec<AttachFrom> {
        let connection = self.connection.read(cx);
        let images = connection.agent_images_supported(self.pane);
        let files = connection.attaches_files();
        #[cfg(any(target_os = "ios", target_os = "android"))]
        {
            use ala_platform::mobile::{PickSource, can_pick};
            [
                (AttachFrom::Photos, PickSource::Photos, images || files),
                (AttachFrom::Camera, PickSource::Camera, images || files),
                (AttachFrom::Files, PickSource::Files, files),
            ]
            .into_iter()
            .filter(|(_, source, allowed)| *allowed && can_pick(*source))
            .map(|(from, ..)| from)
            .collect()
        }
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        {
            let _ = (images, files);
            if self.picks_images(cx) {
                vec![AttachFrom::Photos]
            } else {
                Vec::new()
            }
        }
    }

    fn attach_from(&mut self, from: AttachFrom, cx: &mut Context<Self>) {
        #[cfg(any(target_os = "ios", target_os = "android"))]
        {
            use ala_platform::mobile::{PickSource, pick};
            let source = match from {
                AttachFrom::Photos => PickSource::Photos,
                AttachFrom::Camera => PickSource::Camera,
                AttachFrom::Files => PickSource::Files,
            };
            let (sender, receiver) = async_channel::bounded(1);
            pick(
                source,
                zz_protocol::MAX_AGENT_PROMPT_BYTES as u64,
                move |files| {
                    let _ = sender.try_send(files);
                },
            );
            cx.spawn(async move |this, cx| {
                if let Ok(files) = receiver.recv().await {
                    this.update(cx, |this, cx| this.add_picked(files, cx)).ok();
                }
            })
            .detach();
        }
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        if from == AttachFrom::Photos {
            self.pick_images(cx);
        }
    }

    #[cfg(any(target_os = "ios", target_os = "android"))]
    fn add_picked(
        &mut self,
        picked: Vec<ala_platform::mobile::PickedFile>,
        cx: &mut Context<Self>,
    ) {
        use crate::attachments::{DraftFile, agent_images, takes_image, validate_draft};
        let images = self.connection.read(cx).agent_images_supported(self.pane);
        let files = self.connection.read(cx).attaches_files();
        let text = self.input.read(cx).value().to_string();
        let mut error = None;
        for file in picked {
            let Some(data) = file.data else {
                error = Some(format!("{} is larger than 6 MiB.", file.name));
                continue;
            };
            let format = file
                .mime
                .as_deref()
                .filter(|mime| images && takes_image(Some(mime)))
                .and_then(ala::ImageFormat::from_mime_type);
            if let Some(format) = format {
                let image = std::sync::Arc::new(ala::Image::from_bytes(format, data));
                match agent_images(&[image]) {
                    Ok(added) => {
                        let mut combined = self.attachments.clone();
                        combined.extend(added);
                        match validate_draft(&text, &combined, &self.files) {
                            Ok(()) => self.attachments = combined,
                            Err(refused) => error = Some(refused),
                        }
                    }
                    Err(refused) => error = Some(refused),
                }
                continue;
            }
            if !files {
                error = Some(super::FILES_REFUSED.to_owned());
                continue;
            }
            let mut combined = self.files.clone();
            combined.push(DraftFile {
                name: file.name,
                data,
            });
            match validate_draft(&text, &self.attachments, &combined) {
                Ok(()) => self.files = combined,
                Err(refused) => error = Some(refused),
            }
        }
        if error.is_none() {
            cx.haptic(ala::Haptic::Selection);
        }
        self.draft_error = error;
        cx.notify();
    }
}
