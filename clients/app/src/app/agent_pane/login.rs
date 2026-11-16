use ala::{AnyElement, Context, Window};
use zz_client::agent_flip::FlipMode;
use zz_client::agent_login::{
    self, CardAction, CardClose, cancel, card, in_terminal, restart, retry, settled,
};
use zz_protocol::{AgentLoginBrowser, AgentLoginStep};
use zz_ui::agent::login::{LoginEvent, login_card};
use zz_ui::agent::phone::{CHAT_SHEET, PlusPage};

use super::{AgentPane, phone::ChatSheet};

impl AgentPane {
    pub(super) fn observe_login(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let pane = self.pane;
        let login =
            agent_login::for_pane(self.connection.read(cx).core.agent_logins(), pane).cloned();
        if login == self.login {
            return;
        }
        let fresh = login
            .as_ref()
            .is_some_and(|login| self.login.as_ref().is_none_or(|known| known.id != login.id));
        let signing_in = self.sheet == Some(ChatSheet::Plus(PlusPage::SignIn));
        if let Some(finished) = login.as_ref().filter(|login| settled(login)) {
            let id = finished.id;
            let done = matches!(finished.step, AgentLoginStep::Done { .. });
            self.connection
                .update(cx, |connection, cx| connection.dismiss_agent_login(id, cx));
            if self.phone.is_some() && signing_in {
                if done {
                    self.sheet = Some(ChatSheet::Plus(PlusPage::Account));
                } else {
                    penna::blocks::sheet::close_sheet(CHAT_SHEET, cx);
                }
            } else if done {
                self.open_usage.set(true);
            }
        } else if fresh && self.phone.is_some() && self.sheet.is_none() {
            self.open_sheet(ChatSheet::Plus(PlusPage::SignIn), window, cx);
        }
        self.login = login.filter(|login| !settled(login));
        cx.notify();
    }

    pub(super) fn render_login(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let login = self.login.as_ref()?;
        let entity = cx.entity().downgrade();
        login_card(
            login,
            self.code.as_ref(),
            move |event, window, cx| {
                entity
                    .update(cx, |this, cx| this.login_event(event, window, cx))
                    .ok();
            },
            cx,
        )
    }

    pub(super) fn login_event(
        &mut self,
        event: LoginEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(login) = self.login.clone() else {
            if event == LoginEvent::Close {
                penna::blocks::sheet::close_sheet(CHAT_SHEET, cx);
            }
            return;
        };
        let Some(card) = card(&login) else {
            return;
        };
        let pane = self.pane;
        let accounts = self
            .connection
            .read(cx)
            .core
            .agent_accounts()
            .unwrap_or_default()
            .to_vec();
        let dismiss = |this: &mut Self, cx: &mut Context<Self>| {
            this.connection.update(cx, |connection, cx| {
                connection.dismiss_agent_login(login.id, cx);
            });
            this.login = None;
        };
        match event {
            LoginEvent::Close if card.close == CardClose::Cancel => {
                self.connection
                    .update(cx, |connection, cx| connection.send(cancel(&login), cx));
            }
            LoginEvent::Close => {
                dismiss(self, cx);
                if self.sheet == Some(ChatSheet::Plus(PlusPage::SignIn)) {
                    penna::blocks::sheet::close_sheet(CHAT_SHEET, cx);
                }
            }
            LoginEvent::Action(CardAction::UseCode) => {
                self.connection.update(cx, |connection, cx| {
                    connection.invoke(restart(&login, AgentLoginBrowser::Elsewhere), cx);
                });
            }
            LoginEvent::Action(CardAction::Retry) => {
                dismiss(self, cx);
                self.connection.update(cx, |connection, cx| {
                    connection.invoke(retry(&login, &accounts, login.browser), cx);
                });
            }
            LoginEvent::Action(CardAction::Terminal) => {
                dismiss(self, cx);
                self.connection.update(cx, |connection, cx| {
                    connection.invoke(in_terminal(&login, &accounts), cx);
                });
                if self.sheet.is_some() {
                    penna::blocks::sheet::close_sheet(CHAT_SHEET, cx);
                }
            }
            LoginEvent::Action(CardAction::Continue) => {
                if let Some(message) = self
                    .code
                    .as_ref()
                    .and_then(|field| field.take(&login, window, cx))
                {
                    self.connection
                        .update(cx, |connection, cx| connection.send(message, cx));
                }
            }
            LoginEvent::Action(action @ (CardAction::SwitchWhenIdle | CardAction::SwitchNow)) => {
                let mode = if action == CardAction::SwitchNow {
                    FlipMode::Now
                } else {
                    FlipMode::WhenIdle
                };
                dismiss(self, cx);
                self.connection.update(cx, |connection, cx| {
                    connection.switch_agent_account_when(pane, login.account.as_deref(), mode, cx);
                });
                if self.sheet.is_some() {
                    penna::blocks::sheet::close_sheet(CHAT_SHEET, cx);
                }
            }
        }
        cx.notify();
    }
}
