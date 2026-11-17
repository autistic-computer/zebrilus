use ala::{
    AnyView, App, AppContext as _, Context, IntoElement, Render, Window, div, prelude::*, px,
};
use penna::blocks::chat::CHAT_CONTENT_MAX_WIDTH;
use std::sync::Arc;
use zz_client::child_card::{ChildAsk, ChildCard, ChildState, Decision, Verdict};
use zz_protocol::{
    AgentProvider, PaneId,
    chat::{ChatView, ItemKind},
};

use zz_ui::agent::{
    chat::{ChatHooks, ChatMarkdown, chat_child_card},
    refs::ChatRefs,
};

use super::{
    chat::pane_fill,
    fixtures::{Script, story_hosts},
};
use crate::story::states;

const BRIEF: &str = "In crates/zz-daemon/src/agent/turn.rs, can the retry loop send the same prompt twice? Answer yes or no, with the line that decides it.";
const COMMAND: &str = "cargo test -p zz-daemon";
const REPLY: &str =
    "Yes. A retry scheduled before the ack lands resends the same prompt. Guard on turn id.";
const ERROR: &str = "stream disconnected before completion";

struct Case {
    label: &'static str,
    card: ChildCard,
    chat: ChatView,
    in_view: bool,
}

pub struct ChildCards {
    cases: Vec<Case>,
    now: u64,
    markdown: ChatMarkdown,
}

fn steps(script: &mut Script) -> &mut Script {
    script
        .turn(BRIEF)
        .ran("r1", ItemKind::Read, "crates/zz-daemon/src/agent/turn.rs")
        .summarize(&["r1"], "Read turn.rs")
        .ran("s1", ItemKind::Search, "crates/zz-daemon")
        .summarize(&["s1"], "Searched for ack_turn")
}

fn working() -> ChatView {
    let mut script = Script::new();
    steps(&mut script).activity("Reading the retry loop");
    script.view
}

fn running() -> ChatView {
    let mut script = Script::new();
    steps(&mut script).tool("c1", None, ItemKind::Execute, COMMAND);
    script.view
}

fn ran() -> ChatView {
    let mut script = Script::new();
    steps(&mut script).ran("c1", ItemKind::Execute, COMMAND);
    script.view
}

pub fn child_cards(_: &mut Window, cx: &mut App) -> AnyView {
    let now = working().turns.last().map_or(0, |turn| turn.started_at) + 42_000;
    let card = |state, took: u64, allowed: bool| ChildCard {
        pane: PaneId(4),
        agent: AgentProvider::Codex,
        host: AgentProvider::ClaudeCode,
        state,
        started_at: now - took,
        ended_at: None,
        decisions: allowed
            .then(|| Decision {
                verdict: Verdict::Allowed,
                title: COMMAND.to_owned(),
            })
            .into_iter()
            .collect(),
    };
    let asking = ChildState::Asking(ChildAsk {
        kind: ItemKind::Execute,
        title: COMMAND.to_owned(),
    });
    let case = |label, card, chat, in_view| Case {
        label,
        card,
        chat,
        in_view,
    };
    let cases = vec![
        case(
            "Working: the last three lines, the oldest fading",
            card(ChildState::Working, 42_000, false),
            working(),
            false,
        ),
        case("Asking", card(asking, 65_000, false), running(), false),
        case(
            "Allowed",
            card(ChildState::Working, 78_000, true),
            running(),
            false,
        ),
        case(
            "Done",
            card(
                ChildState::Done {
                    reply: REPLY.to_owned(),
                },
                151_000,
                true,
            ),
            ran(),
            false,
        ),
        case(
            "Failed",
            card(
                ChildState::Failed {
                    error: ERROR.to_owned(),
                },
                72_000,
                false,
            ),
            ran(),
            false,
        ),
        case(
            "Closed",
            card(ChildState::Closed, 58_000, false),
            ran(),
            false,
        ),
        case(
            "PiP in view: header line only",
            card(ChildState::Working, 78_000, true),
            running(),
            true,
        ),
        case(
            "Child chat not open here: header only",
            card(ChildState::Working, 42_000, false),
            ChatView::default(),
            false,
        ),
    ];
    let markdown = ChatMarkdown::new(ChatRefs {
        hosts: story_hosts(),
        cwd: None,
        open: Arc::new(|_, _, _| {}),
        view: None,
    });
    cx.new(|_| ChildCards {
        cases,
        now,
        markdown,
    })
    .into()
}

impl Render for ChildCards {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let hooks = ChatHooks {
            markdown: Some(&self.markdown),
            ..ChatHooks::default()
        };
        let mut shown = states().columns(2);
        for case in &self.cases {
            let chat = (!case.chat.turns.is_empty()).then_some(&case.chat);
            shown = shown.state(
                case.label,
                pane_fill(cx).p(px(16.0)).child(
                    div()
                        .w_full()
                        .max_w(px(CHAT_CONTENT_MAX_WIDTH))
                        .child(chat_child_card(
                            &case.card,
                            chat,
                            case.in_view,
                            self.now,
                            0.0,
                            &hooks,
                            cx,
                        )),
                ),
            );
        }
        shown
    }
}
