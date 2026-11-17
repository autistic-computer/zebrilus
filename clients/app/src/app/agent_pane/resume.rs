use std::path::{Path, PathBuf};

use ala::{App, Context, Div, ScrollHandle, Window};
use zz_client::agent_resume::{Menu, Resume, ResumeRow, folder_name};
use zz_client::agent_timeline::unix_now;
use zz_protocol::{AgentProvider, AgentSessionOpKind, ProtocolMessage};
use zz_ui::agent::resume::EmptyChat;

use super::AgentPane;

impl AgentPane {
    pub(super) fn cwd(&self, cx: &App) -> PathBuf {
        PathBuf::from(self.chat_cwd(cx))
    }

    pub(super) fn reset_search(
        &mut self,
        text: &str,
        placeholder: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.search.update(cx, |input, cx| {
            input.set_placeholder(placeholder, cx);
            input.set_value(text.to_owned(), window, cx);
        });
    }

    fn send_session_op(&self, op: AgentSessionOpKind, cx: &mut Context<Self>) {
        let pane = self.pane;
        self.connection.update(cx, |connection, cx| {
            connection.send(ProtocolMessage::AgentSessionOp { pane, op }, cx);
        });
    }

    pub(super) fn send_wanted(&mut self, cx: &mut Context<Self>) {
        let cwd = self.cwd(cx);
        if let Some(op) = self.resume.wanted(&cwd) {
            self.send_session_op(op, cx);
        }
    }

    pub(super) fn request_resume(&mut self, cx: &mut Context<Self>) {
        let key = (self.session(cx), self.chat_cwd(cx));
        if self.resume_for.as_ref() == Some(&key) {
            return;
        }
        self.resume = Resume::default();
        for op in Resume::start(Path::new(&key.1)) {
            self.send_session_op(op, cx);
        }
        self.resume_for = Some(key);
    }

    pub(super) fn apply_sessions(&mut self, reply: &str, cx: &mut Context<Self>) {
        let cwd = self.cwd(cx);
        if !self.resume.apply(&cwd, reply) {
            return;
        }
        self.send_wanted(cx);
        cx.notify();
    }

    pub(super) fn resume_rows(&self, cx: &App) -> Vec<ResumeRow> {
        self.resume
            .rows(&self.cwd(cx), self.session(cx).as_deref(), unix_now())
    }

    pub(super) fn pick_row(&mut self, index: usize, cx: &mut Context<Self>) -> bool {
        let rows = self.resume_rows(cx);
        let Some(row) = rows.get(index).filter(|row| row.picks()) else {
            return false;
        };
        if let Some(op) = self.resume.pick(row, &self.cwd(cx)) {
            self.send_session_op(op, cx);
        }
        true
    }

    fn toggle_menu(&mut self, menu: Menu, open: bool, window: &mut Window, cx: &mut Context<Self>) {
        if open {
            self.request_resume(cx);
            self.resume.open(menu);
            self.menu_scroll = ScrollHandle::new();
            self.send_wanted(cx);
        } else if self.resume.menu() == Some(menu) {
            self.resume.close();
        } else {
            return;
        }
        self.reset_search("", menu.placeholder(), window, cx);
        cx.notify();
    }

    pub(super) fn pick_menu(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.pick_row(index, cx) {
            self.search
                .update(cx, |input, cx| input.set_value("", window, cx));
            cx.notify();
        }
    }

    pub(super) fn pick_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.phone.is_some() || self.resume.menu().is_none() {
            return;
        }
        let rows = self.resume_rows(cx);
        if let Some(index) = self.resume.selection(&rows) {
            self.pick_menu(index, window, cx);
        }
    }

    pub(super) fn step_menu(&mut self, down: bool, cx: &mut Context<Self>) -> bool {
        if self.phone.is_some() || self.resume.menu().is_none() {
            return false;
        }
        let rows = self.resume_rows(cx);
        if let Some(index) = self.resume.step(&rows, down) {
            self.menu_scroll.scroll_to_item(index);
            cx.notify();
        }
        true
    }

    fn hover_menu(&mut self, index: usize, cx: &mut Context<Self>) {
        let rows = self.resume_rows(cx);
        if rows.get(index).is_some_and(ResumeRow::picks) && self.resume.select(&rows, index) {
            cx.notify();
        }
    }

    pub(super) fn render_empty_chat(
        &mut self,
        provider: AgentProvider,
        cx: &mut Context<Self>,
    ) -> Div {
        self.request_resume(cx);
        let rows = self.resume_rows(cx);
        let toggle = cx.entity().downgrade();
        let pick = toggle.clone();
        let hover = toggle.clone();
        EmptyChat {
            pane: self.pane.0,
            provider,
            folder: &folder_name(&self.cwd(cx)),
            selected: self.resume.selection(&rows),
            rows: &rows,
            open: self.resume.menu(),
            search: &self.search,
            scroll: &self.menu_scroll,
            inset_top: self.header_inset(),
        }
        .render(
            move |menu, open, window, cx| {
                let _ = toggle.update(cx, |this, cx| this.toggle_menu(menu, open, window, cx));
            },
            move |index, window, cx| {
                let _ = pick.update(cx, |this, cx| this.pick_menu(index, window, cx));
            },
            move |index, _, cx| {
                let _ = hover.update(cx, |this, cx| this.hover_menu(index, cx));
            },
            cx,
        )
    }
}
