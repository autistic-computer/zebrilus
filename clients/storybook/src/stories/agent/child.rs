use ala::{
    AnyView, App, AppContext as _, Context, IntoElement, Render, Window, div, prelude::*, px,
};
use penna::blocks::chat::CHAT_CONTENT_MAX_WIDTH;
use std::sync::Arc;
use zz_client::child_card::{ChildAsk, ChildCard, ChildState, Decision, Verdict};
use zz_client::program::Program;
use zz_protocol::{
    AgentProvider, PaneId,
    agent_mention::{
        AgentAsk, AgentDecision, AgentReport, AgentRespond, AskAnswers, ReplyNotice, ReplyRoute,
    },
    chat::{ChatView, ItemBody, ItemId, ItemKind},
};

use zz_ui::agent::{
    chat::{ChatHooks, ChatMarkdown, ChatOpen, chat_child_card},
    refs::ChatRefs,
};

use super::{
    chat::{Thread, pane_fill, threads},
    fixtures::{Script, completed, story_hosts},
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
        anchor: ItemId::new("open"),
        pane: PaneId(4),
        agent: AgentProvider::Codex,
        latest: true,
        state,
        started_at: now - took,
        ended_at: None,
        decisions: allowed
            .then(|| Decision {
                verdict: Verdict::Allowed,
                kind: ItemKind::Execute,
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
        page: None,
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
            click: Some(std::rc::Rc::new(|_, _, _| {})),
            markdown: Some(&self.markdown),
            vendor: Program::by_id("claude"),
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

fn child_hosts() -> Vec<zz_ui::target::TargetHost> {
    let mut hosts = story_hosts();
    hosts[0].sessions[0].windows[0]
        .panes
        .push(zz_ui::target::TargetPane {
            id: PaneId(4),
            title: String::new(),
            program: Some("codex".into()),
            icon: zz_ui::OPENAI_ICON,
            chat: Some(zz_ui::target::TargetChat {
                title: "codex".into(),
                icon: zz_ui::OPENAI_ICON,
            }),
        });
    hosts
}

fn tool(script: &mut Script, id: &str, command: String, output: String) {
    script
        .start(
            id,
            None,
            ItemKind::Execute,
            ItemBody {
                target: Some(command),
                text: output,
                ..ItemBody::default()
            },
        )
        .done(id);
}

pub fn child_transcript(window: &mut Window, cx: &mut App) -> AnyView {
    let pane = PaneId(4);
    let codex = AgentProvider::Codex;
    let ask = |what: &str, request: u64| {
        AgentAsk {
            pane,
            provider: codex,
            what: what.to_owned(),
            answers: AskAnswers::AllowDeny,
            request: Some(request),
        }
        .to_string()
    };
    let mut script = Script::new();
    script
        .turn("@+codex can the retry loop in turn.rs send twice?")
        .reply("a1", "I'll have codex read turn.rs and answer that.");
    tool(
        &mut script,
        "open",
        format!("zz new-pane -d --kind agent codex '{BRIEF}'"),
        ReplyNotice::Opened {
            verb: "new-pane".to_owned(),
            pane,
            provider: codex,
            route: Some(ReplyRoute::Back),
        }
        .to_string(),
    );
    script.finish(completed());
    script.turn(&ask(&format!("execute {COMMAND}"), 1));
    tool(
        &mut script,
        "allow",
        AgentRespond {
            pane,
            decision: AgentDecision::Allow,
            request: Some(1),
        }
        .to_string(),
        String::new(),
    );
    script
        .reply("a2", "Allowed: it only runs the daemon's tests.")
        .finish(completed());
    script
        .turn(
            &AgentReport::Finished {
                pane,
                provider: codex,
                text: REPLY.to_owned(),
            }
            .to_string(),
        )
        .reply(
            "a3",
            "Codex confirms it. I'll guard the resend on the turn id in turn.rs.",
        )
        .finish(completed());
    script
        .turn(
            &AgentReport::Closed {
                pane: PaneId(9),
                provider: codex,
            }
            .to_string(),
        )
        .reply("a4", "That chat is not one I opened.")
        .finish(completed());
    script.turn("@+codex/gpt-6.1-sol does the guard cover resumed sessions too?");
    tool(
        &mut script,
        "send",
        "zz agent-send -t %4 'Does the guard cover resumed sessions too?'".to_owned(),
        ReplyNotice::Sent {
            pane,
            route: ReplyRoute::Back,
        }
        .to_string(),
    );
    script.reply("a5", "Asked codex again.").finish(completed());
    script
        .turn(&ask("execute cargo test -p zz-daemon resume", 2))
        .reply("a6", "Waiting on you for that one.")
        .finish(completed());
    let refs = ChatRefs {
        hosts: child_hosts(),
        cwd: None,
        open: Arc::new(|_, _, _| {}),
        view: None,
        page: None,
    };
    threads(
        vec![(
            "open, ask, allow, finish, a message for a pane it never opened, then a second send that asks",
            Thread::new(script).vendor("claude").refs(refs),
        )],
        window,
        cx,
    )
}

const PEER: PaneId = PaneId(12);
const PEER_TITLE: &str = "Model-invoking hook and zz prompt";
const FINDINGS: &str = "Four findings are still open, each with the file and line that decides it, then two small ones, then what should stay as is.";
const PEER_REPLY: &str = "%6's list arrived and matches what I relayed; its delivery also reproduced a new bug.\n\nThe mention scan runs on every prompt that reaches a chat, not only on what the user typed, so a quoted prompt got two notes attached. I will start on that once child-chats has its report in, and send you the diff before landing it.";

fn peer_hosts() -> Vec<zz_ui::target::TargetHost> {
    let mut hosts = story_hosts();
    hosts[0].sessions[0].windows[0]
        .panes
        .push(zz_ui::target::TargetPane {
            id: PEER,
            title: String::new(),
            program: Some("claude".into()),
            icon: zz_ui::CLAUDE_ICON,
            chat: Some(zz_ui::target::TargetChat {
                title: PEER_TITLE.into(),
                icon: zz_ui::CLAUDE_ICON,
            }),
        });
    hosts
}

fn send(script: &mut Script, id: &str, pane: PaneId, summary: &str) {
    tool(
        script,
        id,
        format!("zz agent-send -t {pane} '{FINDINGS}'"),
        ReplyNotice::Sent {
            pane,
            route: ReplyRoute::Back,
        }
        .to_string(),
    );
    script.summarize(&[id], summary);
}

fn peer_exchange(replied: bool) -> Script {
    let mut script = Script::new();
    script.turn("let %12 know about your findings. its fixing stuff");
    send(&mut script, "send", PEER, "Sent findings to %12");
    script
        .reply(
            "a1",
            "Sent to %12: the four open findings with code pointers, the two small ones, and what should stay as is.",
        )
        .finish(completed());
    if replied {
        script
            .turn(
                &AgentReport::Finished {
                    pane: PEER,
                    provider: AgentProvider::ClaudeCode,
                    text: PEER_REPLY.to_owned(),
                }
                .to_string(),
            )
            .reply(
                "a2",
                "It confirms the list and found the quoted-prompt bug.",
            )
            .finish(completed());
    }
    script
}

fn peer_reports() -> Script {
    let claude = AgentProvider::ClaudeCode;
    let gone = PaneId(13);
    let mut script = Script::new();
    script.turn("ask %12 and %13 whether the queue order matters");
    send(
        &mut script,
        "send-1",
        PEER,
        "Asked %12 about the queue order",
    );
    send(&mut script, "send-2", gone, "Asked about the queue order");
    script.reply("a1", "Asked both.").finish(completed());
    script
        .turn(
            &AgentAsk {
                pane: PEER,
                provider: claude,
                what: format!("execute {COMMAND}"),
                answers: AskAnswers::AllowDeny,
                request: Some(4),
            }
            .to_string(),
        )
        .reply("a2", "Waiting on you for that one.")
        .finish(completed());
    script
        .turn(
            &AgentReport::Stopped {
                pane: PEER,
                provider: claude,
                reason: "cancelled".to_owned(),
                text: String::new(),
            }
            .to_string(),
        )
        .reply("a3", "It stopped before it answered.")
        .finish(completed());
    script
        .turn(
            &AgentReport::Failed {
                pane: gone,
                provider: AgentProvider::Codex,
                message: ERROR.to_owned(),
            }
            .to_string(),
        )
        .reply("a4", "The other one failed.")
        .finish(completed());
    script
        .turn(
            &AgentReport::Closed {
                pane: gone,
                provider: AgentProvider::Codex,
            }
            .to_string(),
        )
        .reply("a5", "And it is closed now.")
        .finish(completed());
    script
}

pub fn peer_rows(window: &mut Window, cx: &mut App) -> AnyView {
    let refs = || ChatRefs {
        hosts: peer_hosts(),
        cwd: None,
        open: Arc::new(|_, _, _| {}),
        view: None,
        page: None,
    };
    let thread = |script| Thread::new(script).vendor("codex").refs(refs());
    threads(
        vec![
            (
                "Waiting: the chip's dot works",
                thread(peer_exchange(false)),
            ),
            ("Replied", thread(peer_exchange(true))),
            (
                "Expanded",
                thread(peer_exchange(true)).opened(ChatOpen {
                    runs: vec![ItemId::new("u:story-2")],
                    ..ChatOpen::default()
                }),
            ),
            (
                "An ask, stopped, failed and closed; %13 is gone",
                thread(peer_reports()),
            ),
        ],
        window,
        cx,
    )
}
