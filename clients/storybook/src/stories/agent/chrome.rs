use ala::{
    AnyElement, AnyView, App, AppContext as _, Context, IntoElement, ParentElement as _, Render,
    SharedString, Styled as _, Window, px,
};
use penna::blocks::chat::empty_state;
use zz_protocol::AgentProvider;
use zz_protocol::chat::ItemKind;
use zz_ui::agent::{
    agent_jump_to_bottom_button,
    presentation::{agent_welcome, error_card},
};

use super::{
    chat::{Frame, Thread, empty_column, pane_fill},
    fixtures::{Script, completed},
};
use crate::story::{row, states};

fn conversation() -> Script {
    let mut script = Script::new();
    script
        .turn("Why do pinned tabs shrink when I open thirty tabs?")
        .think("t1", "**Reading the tab strip.** It divides the width evenly.")
        .done("t1")
        .ran("r1", ItemKind::Read, "crates/zz-ui/src/browser/tab_strip.rs")
        .reply("a1", "Every tab gets the same share of the width, pinned or not. With thirty tabs that share drops below the icon, so pinned tabs collapse first because they have no title to keep them wide.\n\nThe fix is a floor at the icon width for pinned tabs, with the rest of the width divided among the unpinned ones.")
        .finish(completed())
        .turn("Do it, and keep the drop preview in sync.")
        .ran("e1", ItemKind::Edit, "crates/zz-ui/src/browser/tab_strip.rs")
        .ran("e2", ItemKind::Edit, "crates/zz-ui/src/browser/drag.rs")
        .ran("x1", ItemKind::Execute, "cargo test -p zz-ui browser")
        .reply("a2", "Done. Pinned tabs keep a 28px floor and the drop preview reads the same widths.")
        .finish(completed())
        .turn("Write it up for the changelog.")
        .reply("a3", "- Pinned tabs keep their icon width however many tabs are open\n- The drop preview follows the pointer across the pinned boundary")
        .finish(completed());
    script
}

pub struct Jump {
    thread: AnyView,
}

pub fn jump(window: &mut Window, cx: &mut App) -> AnyView {
    let thread = Thread::new(conversation())
        .framed(Frame::Scroll(Some(320.0)))
        .away()
        .view(window, cx)
        .into();
    cx.new(|_| Jump { thread }).into()
}

impl Render for Jump {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        states()
            .state(
                "button",
                row().child(agent_jump_to_bottom_button("jump-alone", cx)),
            )
            .state(
                "over a chat scrolled up: click it to follow the newest message again",
                self.thread.clone(),
            )
    }
}

pub struct EmptyStates;

pub fn empty_states(_: &mut Window, cx: &mut App) -> AnyView {
    cx.new(|_| EmptyStates).into()
}

impl Render for EmptyStates {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity_id();
        let empty = |message: &str, busy: bool, cx: &mut App| -> AnyElement {
            let state = empty_state(SharedString::from(message.to_owned()), busy, view, cx);
            pane_fill(cx).child(empty_column(state)).into_any_element()
        };
        let messages = [
            ("starting", "Starting Claude Code…", true),
            ("resuming a session", "Resuming Codex’s session…", true),
            (
                "running, nothing streamed yet",
                "Waiting for Codex’s first update…",
                true,
            ),
            ("cancelling", "Cancelling the current turn…", true),
            ("the CLI stopped", "Codex stopped.", false),
            ("offline", "The agent is offline.", false),
        ];
        let grid = messages
            .iter()
            .fold(states().columns(2), |states, (label, message, busy)| {
                states.state(*label, empty(message, *busy, cx))
            });
        let welcome = |provider, cx: &App| {
            pane_fill(cx)
                .h(px(340.0))
                .child(empty_column(agent_welcome(provider, cx)))
        };
        states()
            .state(
                "welcome: a new Claude Code chat, before its first prompt",
                welcome(AgentProvider::ClaudeCode, cx),
            )
            .state(
                "welcome: a new Codex chat",
                welcome(AgentProvider::Codex, cx),
            )
            .state("waiting and failure messages", grid)
            .state(
                "error card, a prompt with images",
                error_card(
                    "Codex does not take images with this model; pick another model or send text",
                    false,
                    cx,
                ),
            )
            .state(
                "error card, the CLI exited mid-turn",
                error_card("Claude Code exited before it took the prompt", true, cx),
            )
            .state(
                "error card, long error",
                error_card(
                    &"API Error: 529 overloaded; the turn stopped after 3 retries\n".repeat(12),
                    true,
                    cx,
                ),
            )
    }
}
