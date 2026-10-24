use std::sync::Arc;

use zz_gpui::{
    AnyElement, AnyView, App, AppContext as _, Context, Entity, IntoElement, ListAlignment,
    ListState, ParentElement as _, Render, SharedString, Styled as _, Subscription, Window, div,
    px,
};
use zz_ui::{
    agent::{
        AgentTimeline, AgentTimelineStore, AgentToolKind as Kind, TimelineRow,
        agent_jump_to_bottom_button,
        composer::COMPOSER_OUTER_PADDING,
        fold_timeline_rows,
        presentation::{empty_state, error_card, welcome_state},
    },
    scroll::Scrollbar,
};

use super::{
    fixtures::{assistant, done, thought, user},
    timeline::pane_frame,
};
use crate::story::{row, states};

pub struct Jump {
    store: Entity<AgentTimelineStore>,
    rows: Arc<Vec<TimelineRow>>,
    list: ListState,
    _observe: Subscription,
}

pub fn jump(_: &mut Window, cx: &mut App) -> AnyView {
    cx.new(|cx| {
        let rows = fold_timeline_rows(&[
            user(1, "Why do pinned tabs shrink when I open thirty tabs?"),
            thought(2, "The tab strip divides the width evenly."),
            done(3, Kind::Read, "Read crates/zz-ui/src/browser/tab_strip.rs"),
            assistant(4, "Every tab gets the same share of the width, pinned or not. With thirty tabs that share drops below the icon, so pinned tabs collapse first because they have no title to keep them wide.\n\nThe fix is a floor at the icon width for pinned tabs, with the rest of the width divided among the unpinned ones."),
            user(5, "Do it, and keep the drop preview in sync."),
            done(6, Kind::Edit, "Edit crates/zz-ui/src/browser/tab_strip.rs"),
            done(7, Kind::Edit, "Edit crates/zz-ui/src/browser/drag.rs"),
            done(8, Kind::Execute, "cargo test -p zz-ui browser"),
            assistant(9, "Done. Pinned tabs keep a 28px floor and the drop preview reads the same widths."),
        ])
        .rows;
        let store = cx.new(|_| AgentTimelineStore::default());
        let observe = cx.observe(&store, |_, _, cx| cx.notify());
        Jump {
            list: ListState::new(rows.len(), ListAlignment::Top, px(1200.0)),
            rows,
            store,
            _observe: observe,
        }
    })
    .into()
}

impl Render for Jump {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let timeline = AgentTimeline::new(self.rows.clone(), self.list.clone(), self.store.clone())
            .bottom_padding(COMPOSER_OUTER_PADDING);
        states()
            .state(
                "button",
                row().child(agent_jump_to_bottom_button("jump-alone", cx)),
            )
            .state(
                "over a timeline scrolled up",
                pane_frame(cx)
                    .h(px(300.0))
                    .child(timeline)
                    .child(
                        div()
                            .absolute()
                            .top_0()
                            .left_0()
                            .right_0()
                            .bottom(px(COMPOSER_OUTER_PADDING))
                            .child(Scrollbar::vertical(&self.list)),
                    )
                    .child(
                        div()
                            .absolute()
                            .left_0()
                            .right_0()
                            .bottom(px(2.0 * COMPOSER_OUTER_PADDING))
                            .flex()
                            .justify_center()
                            .child(agent_jump_to_bottom_button("jump-over-timeline", cx)),
                    ),
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
            pane_frame(cx)
                .child(empty_state(
                    SharedString::from(message.to_owned()),
                    busy,
                    view,
                    cx,
                ))
                .into_any_element()
        };
        let messages = [
            ("starting", "Starting Codex…", true),
            ("restoring", "Restoring the previous session…", true),
            (
                "running, nothing streamed yet",
                "Waiting for Claude Code’s first update…",
                true,
            ),
            ("cancelling", "Cancelling the current turn…", true),
            (
                "failed to start",
                "The agent could not start this session.",
                false,
            ),
            ("offline", "The agent is offline.", false),
        ];
        let grid = messages
            .iter()
            .fold(states().columns(2), |states, (label, message, busy)| {
                states.state(*label, empty(message, *busy, cx))
            });
        states()
            .state(
                "welcome, ready with no transcript",
                pane_frame(cx).h(px(340.0)).child(welcome_state(cx)),
            )
            .state("waiting and failure messages", grid)
            .state(
                "error card, the agent stopped",
                error_card("Codex exited with status 1.", true, cx),
            )
            .state(
                "error card, a request failed",
                error_card("This agent does not accept images.", false, cx),
            )
            .state(
                "error card, long error",
                error_card(&"thread 'tokio-runtime-worker' panicked at crates/zz-daemon/src/agent/host.rs:2873:13: the adapter closed stdout before answering session/new\nstack backtrace:\n   0: rust_begin_unwind\n   1: core::panicking::panic_fmt\n".repeat(3), true, cx),
            )
    }
}
