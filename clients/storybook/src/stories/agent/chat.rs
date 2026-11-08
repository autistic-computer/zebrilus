use std::{rc::Rc, sync::Arc};

use ala::{
    AnyElement, AnyView, App, AppContext as _, Context, Div, Entity, Image, IntoElement,
    ParentElement as _, Render, SharedString, Styled as _, Window, div, prelude::*, px,
};
use penna::{
    ActiveTheme as _, IconName, WindowExt as _,
    blocks::chat::{CHAT_CONTENT_MAX_WIDTH, permission_card, permission_option},
    blocks::notification::Notification,
    components::button::Button,
    components::input::InputEvent,
    v_flex,
};
use zz_protocol::chat::{
    Answer, Ask, ChatView, ItemBody, ItemId, ItemKind, ItemStatus, OptionKind, QuestionAnswer,
    RequestClose, RequestId, RequestView, StepStatus, TurnId, TurnOutcome,
};
use zz_protocol::{MAX_AGENT_OPTION_BYTES, agent_stream::AgentQuestion};
use zz_ui::agent::{
    chat::{
        ChatClick, ChatClicks, ChatHooks, ChatHost, ChatLook, ChatOpen, ChatPane, chat_agent_pip,
        chat_column, chat_pip_frame, chat_surface, chat_thread, shows_chat,
    },
    question::{QuestionCardAction, QuestionCardState, QuestionCardStep},
    refs::ChatRefs,
};
use zz_ui::pane_look::pane_color;

use super::fixtures::{
    Script, agent, agents_script, approval, approval_of, branch_question, chart_screenshot,
    checks_question, completed, palettes, placement, question, rendering_question, story_hosts,
    tab_strip_screenshot,
};
use crate::story::states;

const PHASE: f32 = 0.25;
const PIP_FRAME_HEIGHT: f32 = 520.0;

#[derive(Clone, Copy)]
pub enum Frame {
    Grow(f32),
    Fixed(f32),
    Scroll(Option<f32>),
}

pub fn pane_fill(cx: &App) -> Div {
    let theme = cx.theme();
    div()
        .relative()
        .w_full()
        .min_w_0()
        .bg(pane_color(theme, theme.background))
        .text_color(theme.foreground)
}

pub fn empty_column(body: impl IntoElement) -> Div {
    div().size_full().px_3().child(
        div()
            .size_full()
            .max_w(px(CHAT_CONTENT_MAX_WIDTH))
            .mx_auto()
            .child(body),
    )
}

pub struct Thread {
    script: Script,
    remake: Option<fn() -> Script>,
    pane: ChatPane,
    cards: Vec<(RequestId, QuestionCardState)>,
    highlighted: usize,
    sent: Vec<RequestId>,
    refs: Option<ChatRefs>,
    vendor: IconName,
    frame: Frame,
    width: Option<f32>,
    leave: bool,
}

impl Thread {
    pub fn new(script: Script) -> Self {
        Self {
            script,
            remake: None,
            pane: ChatPane::default(),
            cards: Vec::new(),
            highlighted: 0,
            sent: Vec::new(),
            refs: None,
            vendor: zz_ui::program::CLAUDE_ICON,
            frame: Frame::Grow(0.0),
            width: None,
            leave: false,
        }
    }

    fn opened(mut self, open: ChatOpen) -> Self {
        self.pane.open = open;
        self
    }

    pub fn framed(mut self, frame: Frame) -> Self {
        self.frame = frame;
        self
    }

    fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    fn refs(mut self, refs: ChatRefs) -> Self {
        self.refs = Some(refs);
        self
    }

    pub fn vendor(mut self, vendor: IconName) -> Self {
        self.vendor = vendor;
        self
    }

    pub fn away(mut self) -> Self {
        self.leave = true;
        self
    }

    fn remade(mut self, remake: fn() -> Script) -> Self {
        self.remake = Some(remake);
        self
    }

    fn sent(mut self, request: u64) -> Self {
        self.sent.push(RequestId(request));
        self
    }

    pub fn view(self, window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| {
            let mut thread = self;
            thread.sync_cards(window, cx);
            thread
        })
    }

    pub fn shows_chat(&self) -> bool {
        shows_chat(&self.script.view)
    }

    fn card_mut(&mut self, request: RequestId) -> Option<&mut QuestionCardState> {
        self.cards
            .iter_mut()
            .find(|(id, _)| *id == request)
            .map(|(_, card)| card)
    }

    fn sync_cards(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let view = &self.script.view;
        self.cards
            .retain(|(id, _)| view.request(*id).is_some_and(RequestView::is_open));
        let Some(head) = view.open_requests().next() else {
            return;
        };
        let Ask::Questions { questions } = &head.ask else {
            return;
        };
        let id = head.id;
        if self.cards.iter().any(|(held, _)| *held == id) {
            return;
        }
        let questions = questions.clone();
        let card = QuestionCardState::new(
            id.0,
            questions,
            window,
            cx,
            move |thread: &mut Self, question, event, _, cx| {
                if matches!(event, InputEvent::Change)
                    && thread
                        .card_mut(id)
                        .is_some_and(|card| card.input_changed(question, cx))
                {
                    cx.notify();
                }
            },
        );
        self.cards.push((id, card));
    }

    fn reset(&mut self) {
        if let Some(remake) = self.remake {
            self.script = remake();
            self.pane = ChatPane::default();
            self.cards.clear();
            self.highlighted = 0;
        }
    }

    fn choose(&mut self, request: RequestId, option: String, deny: bool) {
        let item = self
            .script
            .view
            .request(request)
            .and_then(|request| request.item.clone());
        self.script.close(
            request,
            RequestClose::Answered {
                answer: Answer::Option { id: option },
            },
        );
        if let Some(item) = item {
            let status = if deny {
                ItemStatus::Declined
            } else {
                ItemStatus::Completed
            };
            self.script.end(item.as_str(), status);
        }
        self.highlighted = 0;
    }

    fn cancel(&mut self, request: RequestId) {
        let item = self
            .script
            .view
            .request(request)
            .and_then(|request| request.item.clone());
        self.script.close(
            request,
            RequestClose::Answered {
                answer: Answer::Cancel,
            },
        );
        if let Some(item) = item {
            self.script.end(item.as_str(), ItemStatus::Declined);
        }
        self.highlighted = 0;
    }

    fn question_action(
        &mut self,
        request: RequestId,
        action: QuestionCardAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(card) = self.card_mut(request) else {
            return;
        };
        let close = match card.action(action, window, cx) {
            QuestionCardStep::Stay => return,
            QuestionCardStep::Handled | QuestionCardStep::Other(_) => {
                cx.notify();
                return;
            }
            QuestionCardStep::Submit(answers) => RequestClose::Answered {
                answer: Answer::Questions {
                    answers: answers
                        .into_iter()
                        .map(|answer| QuestionAnswer {
                            question: answer.id,
                            answers: answer.answers,
                        })
                        .collect(),
                },
            },
            QuestionCardStep::Dismiss => RequestClose::Dismissed,
        };
        self.script.close(request, close);
        cx.notify();
    }

    fn ask(&self, me: &Entity<Self>, request: &RequestView, cx: &App) -> Option<AnyElement> {
        let open = self.script.view.open_requests().collect::<Vec<_>>();
        let current = open.first().is_some_and(|head| head.id == request.id);
        let counter =
            (current && open.len() > 1).then(|| SharedString::from(format!("1/{}", open.len())));
        let id = request.id;
        if let Some((_, card)) = self
            .cards
            .iter()
            .find(|(held, _)| *held == id)
            .filter(|_| current)
        {
            let me = me.clone();
            return Some(
                card.render(
                    &format!("story-question-{}", id.0),
                    counter,
                    true,
                    move |action, window, cx| {
                        me.update(cx, |thread, cx| {
                            thread.question_action(id, action, window, cx);
                        });
                    },
                    cx,
                )
                .into_any_element(),
            );
        }
        let (title, options) = match &request.ask {
            Ask::Questions { .. } => return None,
            Ask::Approval { title, options, .. }
            | Ask::Plan {
                markdown: title,
                options,
            } => (title, options),
        };
        if options.is_empty() {
            return None;
        }
        let enabled = !self.sent.contains(&id);
        let highlighted = current.then_some(self.highlighted);
        let tiles = options
            .iter()
            .enumerate()
            .map(|(index, option)| {
                let answer = me.clone();
                let hover = me.clone();
                let chosen = option.id.clone();
                let deny = option.kind == OptionKind::Deny;
                permission_option(
                    (SharedString::from(format!("story-ask-{}", id.0)), index),
                    index,
                    option.label.clone(),
                    deny,
                    highlighted == Some(index),
                    enabled,
                    cx,
                )
                .when(enabled, |tile| {
                    tile.on_click(move |_, _, cx| {
                        answer.update(cx, |thread, cx| {
                            thread.choose(id, chosen.clone(), deny);
                            cx.notify();
                        });
                        cx.stop_propagation();
                    })
                })
                .when(current, |tile| {
                    tile.on_hover(move |hovered, _, cx| {
                        if *hovered {
                            hover.update(cx, |thread, cx| {
                                if thread.highlighted != index {
                                    thread.highlighted = index;
                                    cx.notify();
                                }
                            });
                        }
                    })
                })
                .into_any_element()
            })
            .collect();
        let cancel = me.clone();
        Some(
            permission_card(
                title.clone(),
                counter,
                tiles,
                Button::new(SharedString::from(format!("story-ask-cancel-{}", id.0)))
                    .tooltip("Cancel request")
                    .on_click(move |_, _, cx| {
                        cancel.update(cx, |thread, cx| {
                            thread.cancel(id);
                            cx.notify();
                        });
                        cx.stop_propagation();
                    }),
                cx,
            )
            .into_any_element(),
        )
    }
}

impl ChatHost for Thread {
    fn chat_pane(&mut self) -> &mut ChatPane {
        &mut self.pane
    }

    fn chat_parts<'a>(&'a mut self, _: &'a App) -> (&'a mut ChatPane, Option<&'a ChatView>) {
        (&mut self.pane, Some(&self.script.view))
    }

    fn chat_read<'a>(&'a self, _: &'a App) -> (&'a ChatPane, Option<&'a ChatView>) {
        (&self.pane, Some(&self.script.view))
    }

    fn chat_ask(&self, request: &RequestView, host: &Entity<Self>, cx: &App) -> Option<AnyElement> {
        self.ask(host, request, cx)
    }

    fn chat_fork(&mut self, _: &ItemId, _: &mut Context<Self>) {}
}

impl Render for Thread {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_cards(window, cx);
        let me = cx.entity();
        let reset = self.remake.map(|_| {
            let me = me.clone();
            Button::new("story-chat-reset")
                .label("Reset")
                .on_click(move |_, _, cx| {
                    me.update(cx, |thread, cx| {
                        thread.reset();
                        cx.notify();
                    });
                })
        });
        let fill = pane_fill(cx).when_some(self.width, |fill, width| fill.w(px(width)));
        let frame = match self.frame {
            Frame::Scroll(height) => {
                self.pane.sync(&self.script.view);
                if std::mem::take(&mut self.leave) {
                    self.pane.scroll_to_top();
                }
                let look = ChatLook {
                    pane: 0,
                    fork: true,
                    refs: self.refs.clone(),
                    vendor: Some(self.vendor.clone()),
                };
                fill.map(|fill| match height {
                    Some(height) => fill.h(px(height)),
                    None => fill.size_full(),
                })
                .child(chat_surface(self, look, window, cx))
            }
            Frame::Grow(_) | Frame::Fixed(_) => {
                let click: ChatClicks = {
                    let me = me.clone();
                    Rc::new(move |click, window, cx| {
                        if let ChatClick::Fork(_) = click {
                            window.push_notification(
                                Notification::new()
                                    .message("Fork opens a new chat from this message"),
                                cx,
                            );
                        }
                        me.update(cx, |thread, cx| {
                            thread.pane.click(click);
                            cx.notify();
                        });
                    })
                };
                let app: &App = cx;
                let ask = |request: &RequestView, cx: &App| self.ask(&me, request, cx);
                let hooks = ChatHooks {
                    click: Some(click),
                    ask: Some(&ask),
                    fork: true,
                    refs: self.refs.as_ref(),
                    vendor: Some(self.vendor.clone()),
                    owner: None,
                };
                let view = &self.script.view;
                let now = self.script.at;
                let body = chat_column(chat_thread(view, now, &self.pane.open, PHASE, &hooks, app));
                let pip = chat_agent_pip(view, now, &self.pane.open, PHASE, &hooks, app)
                    .map(|pip| chat_pip_frame(pip, true));
                let row = ala::div()
                    .flex()
                    .flex_row()
                    .items_start()
                    .w_full()
                    .when(pip.is_some(), |row| row.h(px(PIP_FRAME_HEIGHT)))
                    .child(div().flex_1().min_w_0().child(body))
                    .children(pip);
                match self.frame {
                    Frame::Fixed(height) => fill.h(px(height)).overflow_hidden().child(row),
                    Frame::Grow(min) => fill.min_h(px(min)).child(row),
                    Frame::Scroll(_) => fill.child(row),
                }
            }
        };
        match reset {
            Some(reset) => v_flex()
                .w_full()
                .gap_3()
                .child(reset)
                .child(frame)
                .into_any_element(),
            None => frame.into_any_element(),
        }
    }
}

pub struct Views(pub Vec<(SharedString, AnyView)>);

impl Render for Views {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.0.iter().fold(states(), |states, (label, view)| {
            states.state(label.clone(), view.clone())
        })
    }
}

fn threads(cases: Vec<(&str, Thread)>, window: &mut Window, cx: &mut App) -> AnyView {
    let views = cases
        .into_iter()
        .map(|(label, thread)| {
            (
                SharedString::from(label.to_owned()),
                thread.view(window, cx).into(),
            )
        })
        .collect();
    cx.new(|_| Views(views)).into()
}

fn working(script: &mut Script) -> &mut Script {
    script
        .turn("The chat keeps spinning after Codex finishes a turn. Find out why and fix it.")
        .ran("r1", ItemKind::Read, "crates/zz-daemon/src/agent/fanout.rs")
        .ran("r2", ItemKind::Read, "crates/zz-daemon/src/agent/turn.rs")
        .ran("s1", ItemKind::Search, "turn_id")
        .think(
            "t1",
            "**Checking the turn machine.** Codex reports the thread idle a moment before turn/completed.",
        )
        .text(
            "a1",
            "The spinner comes from the fanout lane: it keeps `turn_id` after a lag replay.",
        )
        .ran("e1", ItemKind::Edit, "crates/zz-daemon/src/agent/fanout.rs")
}

pub fn live(window: &mut Window, cx: &mut App) -> AnyView {
    let described = || {
        let mut script = Script::new();
        working(&mut script).described(
            "x1",
            ItemKind::Execute,
            "cargo test -p zz-daemon agent::fanout",
            "Run the fanout lane tests",
        );
        script
    };
    let mut reported = described();
    reported
        .pause(4_000)
        .activity("Checking why the lane keeps its turn after a replay");
    let mut thinking = Script::new();
    working(&mut thinking).think(
        "t2",
        "**Reading the replay path.** The lane keeps its cursor.",
    );
    let mut fresh = Script::new();
    fresh
        .turn("The chat keeps spinning after Codex finishes a turn. Find out why and fix it.")
        .pause(3_000);
    threads(
        vec![
            (
                "a running tool: the line says what it does, in the tool's own words",
                Thread::new(described()),
            ),
            (
                "opened, the line lists the steps of the run still going",
                Thread::new(described()).opened(ChatOpen {
                    runs: vec![ItemId::new("e1")],
                    ..ChatOpen::default()
                }),
            ),
            (
                "the CLI's own activity line, newer than the tool",
                Thread::new(reported),
            ),
            (
                "between tools, the reasoning heading",
                Thread::new(thinking),
            ),
            ("nothing more specific yet: Thinking", Thread::new(fresh)),
        ],
        window,
        cx,
    )
}

fn repos() -> Script {
    let mut script = Script::new();
    script
        .turn("Check every repo's sync state and pull whatever is behind.")
        .ran(
            "x1",
            ItemKind::Execute,
            "git -C ala status --short --branch",
        )
        .ran(
            "x2",
            ItemKind::Execute,
            "git -C penna status --short --branch",
        )
        .ran("x3", ItemKind::Execute, "git -C pele-rs fetch --all")
        .summarize(
            &["x1", "x2", "x3"],
            "Fetched and checked each repo's sync state and dirty files",
        )
        .reply(
            "a1",
            "penna is two commits behind its main; the others are clean.",
        )
        .ran("x4", ItemKind::Execute, "just libs pull penna");
    script
}

fn repos_live() -> Script {
    let mut script = repos();
    script.described(
        "x5",
        ItemKind::Execute,
        "just libs check penna",
        "Check penna after the pull",
    );
    script
}

fn repos_done() -> Script {
    let mut script = repos();
    script
        .ran("r1", ItemKind::Read, "libs/penna/Cargo.lock")
        .summarize(&["x4", "r1"], "Pulled penna and checked its lockfile")
        .reply("a2", "penna is in step with its main again.")
        .finish(completed());
    script
}

fn codex_live() -> Script {
    let mut script = Script::new();
    script
        .turn("Run the daemon tests in two agents and tell me what fails.")
        .think(
            "t1",
            "**Splitting the suite**\n\nThe agent and mux tests run apart.",
        )
        .done("t1")
        .ran("x1", ItemKind::Execute, "cargo test -p zz-daemon --no-run")
        .ran("x2", ItemKind::Read, "crates/zz-daemon/Cargo.toml")
        .reply("a1", "Two agents run the agent and the mux tests.")
        .start(
            "w1",
            None,
            ItemKind::Tool,
            ItemBody {
                activity: Some("Waiting for agents".to_owned()),
                ..ItemBody::titled("Waited for agents")
            },
        );
    script
}

fn codex_done() -> Script {
    let mut script = codex_live();
    script
        .done("w1")
        .ran(
            "x3",
            ItemKind::Execute,
            "cargo test -p zz-daemon agent::fanout",
        )
        .reply("a2", "Both halves pass; nothing fails.")
        .finish(completed());
    script
}

pub fn batches(window: &mut Window, cx: &mut App) -> AnyView {
    let opened = |turns: Vec<TurnId>, runs: &[&str]| ChatOpen {
        turns,
        runs: runs.iter().map(|id| ItemId::new(*id)).collect(),
        ..ChatOpen::default()
    };
    threads(
        vec![
            (
                "a finished batch reads as the sentence the CLI sent for it",
                Thread::new(repos_live()),
            ),
            (
                "opened, the batch lists its calls",
                Thread::new(repos_live()).opened(opened(Vec::new(), &["x1"])),
            ),
            (
                "a finished turn folds its batches behind Worked for",
                Thread::new(repos_done()).opened(opened(vec![TurnId(1)], &[])),
            ),
            (
                "Codex sends no sentences: its runs keep zz's own count, and the live line comes from the active item",
                Thread::new(codex_live()).vendor(zz_ui::program::OPENAI_ICON),
            ),
            (
                "a finished Codex turn",
                Thread::new(codex_done())
                    .vendor(zz_ui::program::OPENAI_ICON)
                    .opened(opened(vec![TurnId(1)], &[])),
            ),
        ],
        window,
        cx,
    )
}

fn fixed() -> Script {
    let mut script = Script::new();
    working(&mut script)
        .ran("x1", ItemKind::Execute, "cargo test -p zz-daemon agent::fanout")
        .done("a1")
        .done("t1")
        .pause(48_000)
        .reply("a2", "Fixed. The fanout lane clears `turn_id` when its turn settles, and a new test covers the replay.\n\n`cargo test -p zz-daemon` passes, 281 tests.")
        .finish(completed());
    script
}

fn failed_step() -> Script {
    let mut script = Script::new();
    script
        .turn("Run the browser suite and fix whatever breaks.")
        .tool("x1", None, ItemKind::Execute, "cargo test -p zz-ui browser")
        .end(
            "x1",
            ItemStatus::Failed {
                message: Some("2 tests failed".to_owned()),
            },
        )
        .ran("r1", ItemKind::Read, "crates/zz-ui/src/browser/drag.rs")
        .ran("e1", ItemKind::Edit, "crates/zz-ui/src/browser/drag.rs")
        .ran("x2", ItemKind::Execute, "cargo test -p zz-ui browser")
        .reply(
            "a1",
            "Both drag tests pass now: the drop preview reads the widths the layout pass stored.",
        )
        .finish(completed());
    script
}

fn stopped() -> Script {
    let mut script = Script::new();
    script
        .turn("Rebuild the release bundle and notarize it.")
        .ran("r1", ItemKind::Read, "scripts/release-mac.sh")
        .reply(
            "a1",
            "The bundle builds first, then notarization waits on Apple.",
        )
        .tool(
            "x1",
            None,
            ItemKind::Execute,
            "just release mac build 0.24.0",
        )
        .end("x1", ItemStatus::Interrupted)
        .finish(TurnOutcome::Cancelled);
    script
}

fn failed_turn() -> Script {
    let mut script = Script::new();
    script
        .turn("Bump every crate to 0.24 and run the workspace tests.")
        .ran("e1", ItemKind::Edit, "Cargo.toml")
        .ran("e2", ItemKind::Edit, "Cargo.lock")
        .reply("a1", "Versions are bumped; the tests run next.")
        .notice("n1", ItemKind::Error, "API Error · 529 overloaded")
        .finish(TurnOutcome::Failed {
            message: "overloaded".to_owned(),
        });
    script
}

fn agents_done() -> Script {
    let mut script = Script::new();
    script
        .turn("Find who still decides agent state outside the chat machine. Use subagents.")
        .text("a1", "Splitting it two ways.")
        .start(
            "ag1",
            None,
            ItemKind::Subagent,
            agent("Explore", "Find every place outside the chat machine that decides whether an agent is busy."),
        )
        .tool("c1", Some("ag1"), ItemKind::Search, "AgentConnectionState")
        .done("c1")
        .start(
            "c2",
            Some("ag1"),
            ItemKind::Agent,
            ItemBody::text("The host phase and the fanout turn id."),
        )
        .done("c2")
        .pause(12_000)
        .done("ag1")
        .start(
            "ag2",
            None,
            ItemKind::Subagent,
            agent("Clients", "List what the desktop and thin clients keep on their own."),
        )
        .tool("c3", Some("ag2"), ItemKind::Read, "clients/app/src/app/agent_lens.rs")
        .done("c3")
        .pause(8_000)
        .done("ag2")
        .done("a1")
        .reply(
            "a2",
            "Four places: the host phase, the fanout turn id, the desktop submitted flag and the thin client's reducer.",
        )
        .finish(completed());
    script
}

fn long_thought() -> Script {
    let thought = [
        "**Important**: the cache is stale after a session switch, so the first thing to check is who clears it.",
        "#42 is the culprit here: the reset drops the open rows but keeps the decoded images.",
        "The thread keys images by user item id and the PiP prompt by agent id, so the two never collide.",
        "Dropping the Arc alone leaves the decoded pixels in the asset cache and the atlas for the life of the app.",
        "A pasted Retina screenshot is about 20 MB of RGBA, which matters on a phone.",
        "So eviction should queue the images and release them where the window is at hand.",
        "The same image can come back under the same content hash after a fork, so skip any that is still shown.",
        "Then a test: paste, draw, switch sessions, and check the cache and the atlas are empty.",
    ]
    .join("\n\n");
    let mut script = Script::new();
    script
        .turn("Why does memory grow after switching chats?")
        .think("t1", &thought)
        .done("t1")
        .ran("r1", ItemKind::Read, "crates/zz-ui/src/agent/chat.rs")
        .reply(
            "a1",
            "Evicted images kept their decoded pixels. The chat now releases them when an item leaves the view.",
        )
        .finish(completed());
    script
}

pub fn finished(window: &mut Window, cx: &mut App) -> AnyView {
    let first_run = |turn: u64, run: &str| ChatOpen {
        turns: vec![TurnId(turn)],
        runs: vec![ItemId::new(run)],
        ..ChatOpen::default()
    };
    threads(
        vec![
            ("folded behind Worked for", Thread::new(fixed())),
            (
                "opened, first run expanded",
                Thread::new(fixed()).opened(first_run(1, "r1")),
            ),
            (
                "a long thought scrolls inside its cap; inline bold and #42 keep their words",
                Thread::new(long_thought()).opened(first_run(1, "t1")),
            ),
            (
                "a step failed: the run counts it and the step turns red",
                Thread::new(failed_step()).opened(first_run(1, "x1")),
            ),
            ("stopped by you", Thread::new(stopped())),
            ("the turn failed", Thread::new(failed_turn())),
            (
                "subagents fold with the rest of the work",
                Thread::new(agents_done()).opened(ChatOpen {
                    turns: vec![TurnId(1)],
                    ..ChatOpen::default()
                }),
            ),
        ],
        window,
        cx,
    )
}

const STEP_KINDS: [(ItemKind, &str); 8] = [
    (ItemKind::Read, "crates/zz-ui/src/agent/chat.rs"),
    (ItemKind::Edit, "crates/zz-ui/src/agent/chat.rs"),
    (ItemKind::Search, "chat_column"),
    (
        ItemKind::Execute,
        "cargo test -p zz-ui --all-features agent::chat",
    ),
    (ItemKind::Fetch, "https://docs.rs/taffy/latest/taffy/"),
    (ItemKind::Web, "flexbox min-height auto"),
    (ItemKind::Mcp, "github · create_pull_request"),
    (ItemKind::Tool, "TodoWrite"),
];

fn steps_in(status: Option<&ItemStatus>) -> Script {
    let mut script = Script::new();
    script
        .turn("Wire chat_column into both apps and test it.")
        .think(
            "t0",
            "**Planning the change**\n\nBoth apps build the column by hand, so one helper in zz-ui keeps their padding and width in step.",
        )
        .done("t0");
    for (index, (kind, target)) in STEP_KINDS.iter().enumerate() {
        let id = format!("s{index}");
        let body = if matches!(kind, ItemKind::Mcp | ItemKind::Tool) {
            ItemBody::titled(*target)
        } else {
            ItemBody {
                target: Some((*target).to_owned()),
                ..ItemBody::default()
            }
        };
        script.start(&id, None, *kind, body);
        if let Some(status) = status {
            script.end(&id, status.clone());
        }
    }
    if status.is_some() {
        script.finish(completed());
    }
    script
}

pub fn steps(window: &mut Window, cx: &mut App) -> AnyView {
    let open = || ChatOpen {
        runs: vec![ItemId::new("t0")],
        ..ChatOpen::default()
    };
    let case = |label, status: Option<ItemStatus>| {
        (label, Thread::new(steps_in(status.as_ref())).opened(open()))
    };
    threads(
        vec![
            case("running", None),
            case("completed", Some(ItemStatus::Completed)),
            case(
                "failed",
                Some(ItemStatus::Failed {
                    message: Some("exit 101".to_owned()),
                }),
            ),
            case("declined", Some(ItemStatus::Declined)),
            case("interrupted", Some(ItemStatus::Interrupted)),
        ],
        window,
        cx,
    )
}

fn shown(prompt: &str, images: &[Arc<Image>]) -> Script {
    let mut script = Script::new();
    script
        .turn_with_images(prompt, images)
        .reply(
            "a1",
            "Pinned tabs share the strip with the rest, so past thirty tabs their share drops below the icon.",
        )
        .finish(completed());
    script
}

fn said(prompt: &str, reply: &str) -> Script {
    let mut script = Script::new();
    script.turn(prompt).reply("a1", reply).finish(completed());
    script
}

pub fn messages(window: &mut Window, cx: &mut App) -> AnyView {
    let lines = (1..=600)
        .map(|line| format!("test browser::tab_strip::case_{line:03} ... ok"))
        .collect::<Vec<_>>()
        .join("\n");
    let log = format!("Here is the whole run:\n\n```\n{lines}\n```");
    let pasted = (1..=40)
        .map(|line| format!("{line}. a pasted line of the brief, long enough to wrap once or twice in the bubble"))
        .collect::<Vec<_>>()
        .join("\n");
    let mut long = Script::new();
    long.turn(&pasted)
        .reply(
            "a1",
            "Got the brief: forty steps, starting with the tab strip.",
        )
        .finish(completed());
    let mut answered = Script::new();
    answered
        .turn("Did the browser tests pass?")
        .ran("x1", ItemKind::Execute, "cargo test -p zz-ui browser")
        .reply(
            "a1",
            "Yes. All 14 browser tests pass, including the new pinned tab test.",
        )
        .finish(completed());
    let mut two = Script::new();
    two.turn("Fix the tab strip.")
        .reply("a1", "The tab strip is fixed.")
        .reply("a2", "I also tightened the drop preview while I was there.")
        .finish(completed());
    threads(
        vec![
            (
                "short prompt and reply",
                Thread::new(said(
                    "Why do pinned tabs shrink when I open thirty tabs?",
                    "Every tab gets the same share of the strip, pinned or not, so with thirty tabs the share drops below the icon.",
                )),
            ),
            (
                "markdown prompt",
                Thread::new(said(
                    "Two things:\n\n1. Pinned tabs should keep their icon width\n2. The drop preview lags behind the pointer\n\nThe layout lives in `crates/zz-ui/src/browser/tab_strip.rs`.",
                    "Both come from the same split of the width; one fix covers them.",
                )),
            ),
            ("long pasted prompt, capped bubble", Thread::new(long)),
            (
                "unbroken URL",
                Thread::new(said(
                    "CI failed overnight: https://github.com/autistic-computer/zebrilus/actions/runs/18234567890/job/51987654321?pr=1234#step:7:212",
                    "Step 7 is the wasm build; it ran out of disk.",
                )),
            ),
            (
                "narrow pane, 320px",
                Thread::new(said(
                    "Pinned tabs shrink to nothing when I open thirty tabs, see `TabStrip::layout_pinned_and_unpinned_widths`.",
                    "They need a floor at the icon width; the rest of the strip goes to the unpinned tabs.",
                ))
                .width(320.0),
            ),
            (
                "a finished reply: hover it for its time, Copy and Fork",
                Thread::new(answered),
            ),
            ("two replies: Copy takes both", Thread::new(two)),
            (
                "a prompt with two screenshots: tiles come first and open full size",
                Thread::new(shown(
                    "Pinned tabs shrink to nothing past thirty tabs, and the chart shows when it started.",
                    &[tab_strip_screenshot(), chart_screenshot()],
                )),
            ),
            (
                "a screenshot with no text",
                Thread::new(shown("", &[tab_strip_screenshot()])),
            ),
            (
                "a 600-line log: the first 512 lines scroll in a 420px box, Copy takes all of it",
                Thread::new(said("Paste the whole test run.", &log))
                    .framed(Frame::Scroll(Some(600.0))),
            ),
        ],
        window,
        cx,
    )
}

const RICH_ANSWER: &str = "## What changed\n\n\
Pinned tabs now keep a **28px floor**, and only unpinned tabs share what is left.\n\n\
1. `TabStrip::layout` splits the width in two passes\n\
2. The drag code reads the same widths, so the drop preview stays under the pointer\n\
3. A new test covers thirty tabs with three pinned\n\n\
| Tabs | Pinned width | Unpinned width |\n\
| ---: | ---: | ---: |\n\
| 10 | 28px | 96px |\n\
| 30 | 28px | 31px |\n\n\
> The old behavior is still reachable with `browser.pinned-floor = 0`.\n\n\
See the [tab strip notes](https://zzmux.sh/docs/browser) for the full layout rules.";

const CODE_ANSWER: &str = "The floor goes in the first pass:\n\n\
```rust\n\
let pinned = tabs.iter().filter(|tab| tab.pinned).count() as f32;\n\
let floor = px(28.0) * pinned;\n\
let share = (width - floor).max(px(0.0)) / (tabs.len() as f32 - pinned).max(1.0);\n\
```\n\n\
Run it with:\n\n\
```sh\n\
cargo test -p zz-ui browser::tab_strip\n\
```\n\n\
A fence without a language falls back to text:\n\n\
```\n\
test result: ok. 14 passed; 0 failed\n\
```";

const MARKDOWN_FENCE: &str = "Here is the changelog entry, ready to paste:\n\n\
```markdown\n\
### Browser\n\n\
- Pinned tabs keep their icon width however many tabs are open\n\
- The drop preview follows the pointer across the pinned boundary\n\n\
Thanks to **everyone** who sent screenshots.\n\
```";

const CLASSES: &str = "The chat keeps three things apart:\n\n\
```mermaid\n\
classDiagram\n\
    class ChatView {\n\
        +turns\n\
        +items\n\
        +requests\n\
        +apply()\n\
    }\n\
    class ChatPane {\n\
        +open\n\
        +scroll\n\
        +click()\n\
    }\n\
    class ChatRows {\n\
        +turn_rows()\n\
        +agent_rows()\n\
    }\n\
    ChatRows --> ChatView : reads\n\
    ChatPane --> ChatView : syncs\n\
```";

const SEQUENCE: &str = "```mermaid\n\
sequenceDiagram\n\
    participant CLI\n\
    participant Daemon\n\
    participant Lens\n\
    CLI->>Daemon: transcript line\n\
    Daemon->>Daemon: chat machine\n\
    Daemon-->>Lens: chat event\n\
    Lens->>Lens: fold into ChatView\n\
    create participant Sub as subagent\n\
    Lens->>Sub: open its chat\n\
    destroy Sub\n\
    Sub-->>Lens: answer\n\
    Lens-->>CLI: prompt over the live channel\n\
```";

const FLOWCHART: &str = "```mermaid\n\
flowchart LR\n\
    Transcript[transcript line] --> Translator\n\
    Translator --> Machine{chat machine}\n\
    Machine -->|item| View[ChatView]\n\
    Machine -->|ask| Card[question card]\n\
    View --> Thread[chat thread]\n\
```";

const WIDE_FLOWCHART: &str = "```mermaid\n\
flowchart LR\n\
    Prompt --> Lens[lens pastes it] --> CLI[CLI runs a turn] --> Transcript[transcript line] --> Translator --> Machine[chat machine] --> View[ChatView] --> Thread[chat thread]\n\
```";

const STATES: &str = "```mermaid\n\
stateDiagram-v2\n\
    [*] --> Idle\n\
    Idle --> Working : prompt\n\
    Working --> Asking : question\n\
    Asking --> Working : answer\n\
    Working --> Idle : turn ends\n\
    Working --> Failed : error\n\
    Failed --> [*]\n\
```";

const ENTITIES: &str = "```mermaid\n\
erDiagram\n\
    SESSION ||--o{ WINDOW : holds\n\
    WINDOW ||--|{ PANE : splits\n\
    PANE ||--o| AGENT : runs\n\
    SESSION {\n\
        string id PK\n\
        string name\n\
    }\n\
    PANE {\n\
        string id PK\n\
        string window_id FK\n\
        string cwd\n\
    }\n\
```";

const GANTT: &str = "```mermaid\n\
gantt\n\
    title Lens rebuild\n\
    dateFormat YYYY-MM-DD\n\
    section Claude\n\
        Channel      :done, claude, 2026-10-27, 3d\n\
        Dialogs      :active, after claude, 3d\n\
    section Codex\n\
        Channel      :codex, 2026-10-28, 4d\n\
        Questions    :crit, after codex, 2d\n\
```";

const PIE: &str = "```mermaid\n\
pie showData\n\
    title Tool calls this week\n\
    \"Read\" : 42\n\
    \"Edit\" : 28\n\
    \"Bash\" : 18\n\
    \"Search\" : 12\n\
```";

const QUADRANT: &str = "```mermaid\n\
quadrantChart\n\
    title Bugs by cost and reach\n\
    x-axis Cheap to fix --> Costly to fix\n\
    y-axis Few users --> Many users\n\
    quadrant-1 Plan it\n\
    quadrant-2 Fix now\n\
    quadrant-3 Batch it\n\
    quadrant-4 Reconsider\n\
    Stale frame: [0.3, 0.8]\n\
    Tab flicker: [0.2, 0.35]\n\
    Wire bump: [0.75, 0.7]\n\
    Font cache: [0.65, 0.2]\n\
```";

const UNSUPPORTED_MERMAID: &str = "```mermaid
mindmap
  root((zz))
    Panes
      Terminal
      Browser
    Agents
      Lens
```";

const BROKEN_MERMAID: &str = "```mermaid\n\
flowchart LR\n\
    Prompt[ask the agent --> Answer\n\
```";

const SVG: &str = "The p99 per build:\n\n\
```svg\n\
<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"360\" height=\"140\" viewBox=\"0 0 360 140\">\n\
<rect x=\"20\" y=\"118\" width=\"320\" height=\"2\" fill=\"#8a8f98\"/>\n\
<rect x=\"40\" y=\"70\" width=\"36\" height=\"48\" rx=\"4\" fill=\"#7aa2f7\"/>\n\
<rect x=\"100\" y=\"52\" width=\"36\" height=\"66\" rx=\"4\" fill=\"#7aa2f7\"/>\n\
<rect x=\"160\" y=\"78\" width=\"36\" height=\"40\" rx=\"4\" fill=\"#7aa2f7\"/>\n\
<rect x=\"220\" y=\"20\" width=\"36\" height=\"98\" rx=\"4\" fill=\"#f7768e\"/>\n\
<rect x=\"280\" y=\"60\" width=\"36\" height=\"58\" rx=\"4\" fill=\"#7aa2f7\"/>\n\
</svg>\n\
```";

pub fn markdown(window: &mut Window, cx: &mut App) -> AnyView {
    threads(
        vec![
            (
                "rich answer",
                Thread::new(said("What changed in the tab strip?", RICH_ANSWER)),
            ),
            (
                "code blocks",
                Thread::new(said("Show me the fix.", CODE_ANSWER)),
            ),
            (
                "a markdown fence renders rich",
                Thread::new(said("Write the changelog entry.", MARKDOWN_FENCE)),
            ),
            (
                "mermaid flowchart",
                Thread::new(said("Where does a chat row come from?", FLOWCHART)),
            ),
            (
                "a flowchart too wide for the transcript turns to run down",
                Thread::new(said("Trace a prompt end to end.", WIDE_FLOWCHART)),
            ),
            (
                "mermaid sequence diagram, with a participant created and destroyed",
                Thread::new(said("How does a prompt get to the lens?", SEQUENCE)),
            ),
            (
                "mermaid class diagram",
                Thread::new(said("How is the chat put together?", CLASSES)),
            ),
            (
                "mermaid state diagram",
                Thread::new(said("What states does a turn go through?", STATES)),
            ),
            (
                "mermaid entity relationship diagram",
                Thread::new(said("How do sessions, windows and panes relate?", ENTITIES)),
            ),
            (
                "mermaid gantt chart",
                Thread::new(said("Where is the lens rebuild at?", GANTT)),
            ),
            (
                "mermaid pie chart",
                Thread::new(said("Which tools ran most?", PIE)),
            ),
            (
                "mermaid quadrant chart",
                Thread::new(said("Sort the open bugs.", QUADRANT)),
            ),
            (
                "a mermaid type zz does not draw stays a code block",
                Thread::new(said("Map out zz as a mind map.", UNSUPPORTED_MERMAID)),
            ),
            (
                "a mermaid source that does not parse shows the error",
                Thread::new(said("Draw the prompt flow.", BROKEN_MERMAID)),
            ),
            ("an SVG", Thread::new(said("Chart the p99s.", SVG))),
        ],
        window,
        cx,
    )
}

pub fn references(window: &mut Window, cx: &mut App) -> AnyView {
    let mut script = Script::new();
    script
        .turn("@macbook:zebrilus:1.%0 what is this, and is @macbook:%1 still on it?")
        .reply(
            "a1",
            "@macbook:zebrilus:1.%0 is an idle bash in @zebrilus, window @zebrilus:1. The chat in `%1` is still working on `crates/zz-daemon/src/agent/fanout.rs:1576`, and its notes are in @~/dev/notes/.\n\n```sh\nzz send-keys -t @macbook:zebrilus:1.%0 ls\n```",
        )
        .finish(completed());
    let refs = ChatRefs {
        hosts: story_hosts(),
        cwd: Some("/Users/me/dev/zebrilus".to_owned()),
        open: Arc::new(|target, window, cx| {
            window.push_notification(
                Notification::new().message(format!("Open {}", target.label)),
                cx,
            );
        }),
    };
    threads(
        vec![(
            "links in a prompt and a reply",
            Thread::new(script).refs(refs),
        )],
        window,
        cx,
    )
}

fn edit_card(
    thread: &Entity<Thread>,
    window: &mut Window,
    cx: &mut App,
    edit: impl FnOnce(&mut QuestionCardState, &mut Window, &mut App),
) {
    thread.update(cx, |thread, cx| {
        if let Some((_, card)) = thread.cards.first_mut() {
            edit(card, window, cx);
        }
    });
}

fn type_other(
    state: &mut QuestionCardState,
    question: usize,
    text: &str,
    window: &mut Window,
    cx: &mut App,
) {
    state.card.select_other(question);
    if let Some(input) = state.other_input(question) {
        input.update(cx, |input, cx| input.set_value(text.to_owned(), window, cx));
    }
    state.input_changed(question, cx);
}

fn asked(prompt: &str, request: u64, questions: Vec<AgentQuestion>) -> Script {
    let mut script = Script::new();
    script
        .turn(prompt)
        .ask(request, None, Ask::Questions { questions });
    script
}

pub fn asks(window: &mut Window, cx: &mut App) -> AnyView {
    let mut approve = Script::new();
    approve
        .turn("Clear the build cache and rerun the terminal benchmarks.")
        .ran("r1", ItemKind::Read, "Justfile")
        .reply(
            "a1",
            "The benchmarks read `target/release`. I'll clear it first.",
        )
        .tool("x1", None, ItemKind::Execute, "rm -rf target/release")
        .ask(
            1,
            Some("x1"),
            approval(
                "Delete target/release and rebuild?",
                "Allow rm in this chat",
            ),
        );
    let mut queued = Script::new();
    queued
        .turn("Commit the storybook lock and open a pull request.")
        .tool(
            "x1",
            None,
            ItemKind::Execute,
            "git commit -am 'Bump the storybook lock'",
        )
        .ask(
            1,
            Some("x1"),
            approval_of("git commit -am 'Bump the storybook lock'"),
        )
        .ask(
            2,
            None,
            Ask::Questions {
                questions: vec![branch_question()],
            },
        );
    let mut ask = Script::new();
    ask.turn("Add a setting for how long finished chats stay in the sidebar.")
        .ran("r1", ItemKind::Read, "crates/zz-ui/src/settings.rs")
        .reply("a1", "Two choices decide where this lives.")
        .ask(2, None, placement());
    let mut long = Script::new();
    long.turn("Give pinned tabs a width floor everywhere they are drawn.")
        .tool(
            "e1",
            None,
            ItemKind::Edit,
            "crates/zz-ui/src/browser/tab_strip.rs",
        )
        .ask(
            1,
            Some("e1"),
            approval_of(
                "Edit crates/zz-ui/src/browser/tab_strip.rs, crates/zz-ui/src/browser/drag.rs and four more files to give pinned tabs a width floor",
            ),
        );
    let mut many = Script::new();
    many.turn("Pick the palette the storybook screenshots use.")
        .ask(1, None, palettes());
    let mut sent = Script::new();
    sent.turn("Run the browser suite.")
        .tool("x1", None, ItemKind::Execute, "cargo test -p zz-ui browser")
        .ask(
            1,
            Some("x1"),
            approval("Run cargo test -p zz-ui browser?", "Allow cargo test"),
        );
    let mut answered = Script::new();
    answered
        .turn("Add a setting for how long finished chats stay in the sidebar.")
        .reply("a1", "Two choices decide where this lives.")
        .ask(2, None, placement())
        .answer(2, &["Settings › Interface", "7 days"])
        .tool("e1", None, ItemKind::Edit, "crates/zz-ui/src/settings.rs");
    let mut waiting = Script::new();
    waiting
        .turn("Run steps 1 to 9 of the render test, and ask me which element to stress at step 7.")
        .ran("x1", ItemKind::Execute, "cargo test -p pele render")
        .reply("a1", "Steps 1 to 6 passed. Step 7 needs your pick.")
        .ask_async(3, "call_q", rendering_question());
    let mut ended = Script::new();
    ended
        .turn("Run steps 1 to 9 of the render test, and ask me which element to stress at step 7.")
        .ran("x1", ItemKind::Execute, "cargo test -p pele render")
        .ask_async(3, "call_q", rendering_question())
        .reply(
            "a1",
            "Waiting for your choice: Tables, Diagrams, or Code blocks.",
        )
        .finish(completed());
    let mut views: Vec<(SharedString, AnyView)> = Vec::new();
    let mut add = |label: &'static str, thread: &Entity<Thread>| {
        views.push((label.into(), thread.clone().into()));
    };
    add(
        "Codex asks without stopping: the turn waits for your answer",
        &Thread::new(waiting)
            .vendor(zz_ui::program::OPENAI_ICON)
            .view(window, cx),
    );
    add(
        "Codex asked and ended its turn: the card stays open",
        &Thread::new(ended)
            .vendor(zz_ui::program::OPENAI_ICON)
            .view(window, cx),
    );
    add(
        "an approval on the tool that needs it, first option highlighted",
        &Thread::new(approve).view(window, cx),
    );
    add(
        "a long title wraps over the tiles",
        &Thread::new(long).view(window, cx),
    );
    add(
        "ten choices: digits stop at nine",
        &Thread::new(many).view(window, cx),
    );
    add(
        "answered: the tiles stay greyed until the CLI moves on",
        &Thread::new(sent).sent(1).view(window, cx),
    );
    add(
        "two asks: the first is live, the question behind it waits greyed out",
        &Thread::new(queued).view(window, cx),
    );
    add(
        "a question in the thread",
        &Thread::new(ask).view(window, cx),
    );
    let picks = Thread::new(asked(
        "Get the branch ready to push.",
        4,
        vec![checks_question()],
    ))
    .view(window, cx);
    edit_card(&picks, window, cx, |state, _, _| {
        state.card.pick(0, 0);
        state.card.pick(0, 1);
    });
    add("pick any, two picked", &picks);
    let typed =
        Thread::new(asked("Open the pull request.", 5, vec![branch_question()])).view(window, cx);
    edit_card(&typed, window, cx, |state, window, cx| {
        type_other(state, 0, "release/0.15", window, cx);
    });
    add("a typed answer replaces the choice", &typed);
    let secret = Thread::new(asked(
        "Deploy the branch to staging.",
        6,
        vec![AgentQuestion {
            secret: true,
            ..question("q0", "Paste the deploy token for staging", Vec::new())
        }],
    ))
    .view(window, cx);
    edit_card(&secret, window, cx, |state, window, cx| {
        type_other(state, 0, "not-a-real-token-1234", window, cx);
    });
    add("a secret", &secret);
    add(
        "a question whose id is too long for zz to send back",
        &Thread::new(asked(
            "Open the pull request.",
            8,
            vec![AgentQuestion {
                id: "q".repeat(MAX_AGENT_OPTION_BYTES + 1),
                ..branch_question()
            }],
        ))
        .view(window, cx),
    );
    add(
        "answered: a record, the turn goes on",
        &Thread::new(answered).view(window, cx),
    );
    cx.new(|_| Views(views)).into()
}

pub fn subagents(window: &mut Window, cx: &mut App) -> AnyView {
    threads(
        vec![
            ("working, done, needs you", Thread::new(agents_script())),
            (
                "Explore opened in its PiP",
                Thread::new(agents_script())
                    .opened(ChatOpen {
                        agent: Some(ItemId::new("ag1")),
                        ..ChatOpen::default()
                    })
                    .framed(Frame::Fixed(560.0)),
            ),
        ],
        window,
        cx,
    )
}

pub fn background(window: &mut Window, cx: &mut App) -> AnyView {
    let mut script = Script::new();
    script
        .turn("Watch tonight's benchmark run in the background and tell me if p99 regresses.")
        .start("w", None, ItemKind::Subagent, ItemBody::titled("Watcher"))
        .background("w")
        .reply(
            "a1",
            "Started a background agent, Watcher, on the bench window.",
        )
        .finish(completed())
        .tool(
            "w1",
            Some("w"),
            ItemKind::Execute,
            "zz send-keys -t bench:1 'just bench terminal' Enter",
        )
        .ask(
            4,
            Some("w1"),
            approval(
                "Watcher wants to restart the run that stalled",
                "Allow zz send-keys",
            ),
        );
    threads(
        vec![("the turn ended, its agent still asks", Thread::new(script))],
        window,
        cx,
    )
}

pub fn rows(window: &mut Window, cx: &mut App) -> AnyView {
    let mut script = Script::new();
    script
        .turn("Run the release checklist for v0.24.")
        .plan(
            "p1",
            &[
                ("Read the changelog since v0.23", StepStatus::Done),
                ("Bump versions in the workspace", StepStatus::Done),
                ("Skip the Windows build", StepStatus::Cancelled),
                ("Run the full test suite", StepStatus::Running),
                ("Build the signed macOS bundle", StepStatus::Pending),
            ],
        )
        .notice(
            "n1",
            ItemKind::Notice,
            "Context compacted · 182k → 41k tokens",
        )
        .notice(
            "n2",
            ItemKind::Notice,
            "Forked this conversation into a new chat.",
        )
        .notice(
            "n3",
            ItemKind::Notice,
            "Rewound to before your last 2 prompts. Files stay as they are.",
        )
        .notice(
            "n4",
            ItemKind::Error,
            "The model is overloaded · retry 2 of 5 in 4s",
        )
        .tool("x1", None, ItemKind::Execute, "cargo ci-test");
    threads(
        vec![("a plan, notices and an error", Thread::new(script))],
        window,
        cx,
    )
}

fn playground() -> Script {
    let mut script = agents_script();
    script.ask(5, None, placement());
    script
}

pub fn try_it(window: &mut Window, cx: &mut App) -> AnyView {
    Thread::new(playground())
        .remade(playground)
        .framed(Frame::Grow(560.0))
        .view(window, cx)
        .into()
}
