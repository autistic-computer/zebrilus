use std::{rc::Rc, sync::Arc};

use zz_gpui::{
    AnyView, App, AppContext as _, Context, Entity, IntoElement, ListAlignment, ListState,
    ParentElement as _, Render, SharedString, Styled as _, Subscription, Window, div, prelude::*,
    px,
};
use zz_protocol::{AgentProvider, AgentTaskWire, PaneId};
use zz_ui::{
    ActiveTheme as _, CHROME_GAP, IconName,
    agent::{
        AgentPaneStatus, AgentTimeline, AgentTimelineStore, AgentToolKind as Kind,
        AgentToolStatus as Status, TimelineRow, agent_pane_header, agent_status_pill,
        composer::COMPOSER_OUTER_PADDING,
        controls::agent_pane_title,
        fold_timeline_rows,
        presentation::spinner_phase,
        tasks::{TaskTrayAction, TrayPanel, task_tray},
        title::agent_thread_title_editor,
    },
    h_flex,
    input::InputState,
    pane::{PaneDragSource, pane_header_icon_button},
    scroll::Scrollbar,
};

use super::{
    composer::{Draft, composer, composer_input},
    fixtures::{
        PLAN, TEST_FAILURE, assistant, done, plan, tab_strip_screenshot, tasks, thought, tool,
        user, user_with_images,
    },
    timeline::pane_frame,
};

const TITLE: &str = "Pinned tabs keep their icon width";

const FIRST_ANSWER: &str = "Pinned tabs now keep a **28px floor**, and only the unpinned tabs \
share what is left of the strip:\n\n\
```rust\n\
let floor = px(28.0) * pinned;\n\
let share = (width - floor).max(px(0.0)) / unpinned.max(1.0);\n\
```\n\n\
The new `thirty_tabs_three_pinned` test covers it.";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Look {
    Page,
    Tiled,
    Floating,
}

pub struct FullPane {
    store: Entity<AgentTimelineStore>,
    rows: Arc<Vec<TimelineRow>>,
    list: ListState,
    input: Entity<InputState>,
    tray: Option<TrayPanel>,
    look: Look,
    _observe: Subscription,
}

pub fn full_pane(window: &mut Window, cx: &mut App) -> AnyView {
    build(Look::Page, window, cx)
}

pub(crate) fn embedded_pane(window: &mut Window, cx: &mut App) -> AnyView {
    build(Look::Tiled, window, cx)
}

pub(crate) fn floating_pane(window: &mut Window, cx: &mut App) -> AnyView {
    build(Look::Floating, window, cx)
}

fn build(look: Look, window: &mut Window, cx: &mut App) -> AnyView {
    let input = composer_input("", window, cx);
    cx.new(|cx| {
        let rows = fold_timeline_rows(&[
            user_with_images(
                1,
                "Pinned tabs shrink to nothing when I open thirty tabs. Can you make them keep their icon width?",
                vec![tab_strip_screenshot()],
            ),
            thought(2, "The tab strip hands every tab the same share of the width, so pinned tabs shrink with the rest."),
            done(3, Kind::Search, "rg -n \"pinned\" crates/zz-ui/src/browser"),
            done(4, Kind::Read, "Read crates/zz-ui/src/browser/tab_strip.rs"),
            done(5, Kind::Edit, "Edit crates/zz-ui/src/browser/tab_strip.rs"),
            tool(6, Kind::Execute, Status::Completed, "cargo test -p zz-ui browser::tab_strip")
                .terminal("running 6 tests\ntest result: ok. 6 passed; 0 failed")
                .entry(),
            assistant(7, FIRST_ANSWER),
            user(8, "Nice. Run the whole browser suite and fix whatever breaks."),
            plan(14, PLAN),
            tool(9, Kind::Execute, Status::Failed, "cargo test -p zz-ui browser")
                .terminal(TEST_FAILURE)
                .exit(101)
                .entry(),
            thought(10, "Two failures, both in drag. The drop preview still measures each tab itself instead of reading the widths the layout pass stored, so it lands one slot off once pinned tabs stop shrinking."),
            done(11, Kind::Read, "Read crates/zz-ui/src/browser/drag.rs"),
            done(12, Kind::Edit, "Edit crates/zz-ui/src/browser/drag.rs"),
            tool(13, Kind::Execute, Status::Running, "cargo test -p zz-ui browser::drag").entry(),
        ])
        .rows;
        let store = cx.new(|_| AgentTimelineStore::default());
        let observe = cx.observe(&store, |_, _, cx| cx.notify());
        FullPane {
            list: ListState::new(rows.len(), ListAlignment::Bottom, px(1200.0)),
            rows,
            store,
            input,
            tray: None,
            look,
            _observe: observe,
        }
    })
    .into()
}

impl Render for FullPane {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity_id();
        let phase = spinner_phase(view, cx);
        let entity = cx.entity();
        let tasks = tasks().into_iter().take(1).collect::<Vec<_>>();
        let tray = task_tray(
            "full-pane-tray",
            Some(PLAN),
            &tasks,
            self.tray,
            true,
            phase,
            move |action, _, cx| {
                if let TaskTrayAction::Toggle(panel) = action {
                    entity.update(cx, |view, cx| {
                        view.tray = (view.tray != Some(panel)).then_some(panel);
                        cx.notify();
                    });
                }
            },
            cx,
        );
        let leading = agent_pane_title(
            AgentProvider::ClaudeCode,
            agent_thread_title_editor("full-pane-title", TITLE, true, |_, _| {}, window, cx),
            PaneDragSource::new(PaneId(1), TITLE, true, |_, _, _| {}),
            |_, _| {},
        );
        let trailing = h_flex()
            .gap(px(CHROME_GAP))
            .child(
                pane_header_icon_button("full-pane-split-bottom", IconName::PanelBottom, true, cx)
                    .tooltip("Split bottom"),
            )
            .child(
                pane_header_icon_button("full-pane-split-right", IconName::PanelRight, true, cx)
                    .tooltip("Split right"),
            )
            .when(self.look != Look::Floating, |actions| {
                actions.child(
                    pane_header_icon_button(
                        "full-pane-float",
                        IconName::PictureInPicture,
                        true,
                        cx,
                    )
                    .tooltip("Float"),
                )
            });
        let status = agent_status_pill(
            "full-pane-status",
            AgentPaneStatus::Running,
            phase,
            None::<Rc<dyn Fn(&mut Window, &mut App)>>,
            cx,
        )
        .into_any_element();
        let timeline = AgentTimeline::new(self.rows.clone(), self.list.clone(), self.store.clone())
            .active_turn(true)
            .bottom_padding(COMPOSER_OUTER_PADDING)
            .open_output(|_, _, _| {});
        let composer = composer(
            Draft {
                id: SharedString::from("full-pane-composer"),
                input: self.input.clone(),
                running: true,
                writable: true,
                images: Vec::new(),
                prefix: tray
                    .map(IntoElement::into_any_element)
                    .into_iter()
                    .collect(),
            },
            window,
            cx,
        );
        let frame = if self.look == Look::Page {
            pane_frame(cx).h(px(720.0))
        } else {
            div()
                .size_full()
                .bg(cx.theme().pane_background(cx.theme().background))
        };
        frame
            .flex()
            .flex_col()
            .child(agent_pane_header(
                true,
                leading,
                Some(status),
                trailing,
                true,
                cx,
            ))
            .child(
                div()
                    .id("full-pane-timeline")
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .child(timeline)
                    .child(
                        div()
                            .absolute()
                            .top_0()
                            .left_0()
                            .right_0()
                            .bottom(px(COMPOSER_OUTER_PADDING))
                            .child(Scrollbar::vertical(&self.list)),
                    ),
            )
            .child(composer)
    }
}

pub struct FeedPane {
    store: Entity<AgentTimelineStore>,
    rows: Arc<Vec<TimelineRow>>,
    list: ListState,
    tray: Option<TrayPanel>,
    _observe: Subscription,
}

const FEED_CHILDREN: [(&str, &str); 2] = [
    ("child-1", "Pauli · read mux a"),
    ("child-2", "Galileo · read mux b"),
];

pub(crate) fn feed_pane(cx: &mut App) -> AnyView {
    cx.new(|cx| {
        let rows = fold_timeline_rows(&[
            user(1, "explorer · map the pane headers"),
            done(2, Kind::Search, "rg -n \"pane_header_icon_button\" crates"),
            done(3, Kind::Read, "Read crates/zz-ui/src/pane.rs"),
            assistant(
                4,
                "Terminal, agent and browser headers all build on `pane_header_icon_button`.",
            ),
            user(5, FEED_CHILDREN[0].1),
            done(
                6,
                Kind::Execute,
                "sleep 2; cat crates/zz-mux/src/command/display.rs",
            ),
            done(
                7,
                Kind::Execute,
                "sleep 2; cat crates/zz-mux/src/command/format.rs",
            ),
            tool(
                8,
                Kind::Execute,
                Status::Running,
                "sleep 2; cat crates/zz-mux/src/command/listing.rs",
            )
            .entry(),
        ])
        .rows;
        let store = cx.new(|_| AgentTimelineStore::default());
        let observe = cx.observe(&store, |_, _, cx| cx.notify());
        FeedPane {
            list: ListState::new(rows.len(), ListAlignment::Bottom, px(1200.0)),
            rows,
            store,
            tray: Some(TrayPanel::Tasks),
            _observe: observe,
        }
    })
    .into()
}

impl Render for FeedPane {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let phase = spinner_phase(cx.entity_id(), cx);
        let entity = cx.entity();
        let tasks = FEED_CHILDREN
            .iter()
            .map(|(id, description)| AgentTaskWire {
                id: (*id).into(),
                kind: "agent".into(),
                description: (*description).into(),
                tool_call_id: None,
            })
            .collect::<Vec<_>>();
        let tray = task_tray(
            "feed-pane-tray",
            None,
            &tasks,
            self.tray,
            true,
            phase,
            move |action, _, cx| {
                if let TaskTrayAction::Toggle(panel) = action {
                    entity.update(cx, |view, cx| {
                        view.tray = (view.tray != Some(panel)).then_some(panel);
                        cx.notify();
                    });
                }
            },
            cx,
        );
        let leading = agent_pane_title(
            AgentProvider::Codex,
            agent_thread_title_editor("feed-pane-title", "Subagents", false, |_, _| {}, window, cx),
            PaneDragSource::new(PaneId(7), "Subagents", false, |_, _, _| {}),
            |_, _| {},
        );
        let trailing = h_flex()
            .gap(px(CHROME_GAP))
            .child(
                pane_header_icon_button("feed-pane-split-bottom", IconName::PanelBottom, true, cx)
                    .tooltip("Split bottom"),
            )
            .child(
                pane_header_icon_button("feed-pane-split-right", IconName::PanelRight, true, cx)
                    .tooltip("Split right"),
            );
        let status = agent_status_pill(
            "feed-pane-status",
            AgentPaneStatus::Running,
            phase,
            None::<Rc<dyn Fn(&mut Window, &mut App)>>,
            cx,
        )
        .into_any_element();
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(cx.theme().pane_background(cx.theme().background))
            .child(agent_pane_header(
                true,
                leading,
                Some(status),
                trailing,
                true,
                cx,
            ))
            .child(
                div()
                    .id("feed-pane-timeline")
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .child(
                        AgentTimeline::new(
                            self.rows.clone(),
                            self.list.clone(),
                            self.store.clone(),
                        )
                        .active_turn(true)
                        .bottom_padding(COMPOSER_OUTER_PADDING),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .w_full()
                    .flex_none()
                    .gap_2()
                    .px(px(COMPOSER_OUTER_PADDING))
                    .pb(px(COMPOSER_OUTER_PADDING))
                    .children(tray),
            )
    }
}
