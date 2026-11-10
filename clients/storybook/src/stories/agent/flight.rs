use std::sync::Arc;

use ala::{
    AnyElement, AnyView, App, AppContext as _, Context, Entity, IntoElement, ParentElement as _,
    Pixels, Render, Size, Styled as _, Window, div, prelude::*, px,
};
use penna::{ActiveTheme as _, WindowExt as _, blocks::notification::Notification};
use zz_protocol::{
    PaneId, ViewContent, WindowId,
    chat::{ChatView, ItemBody, ItemKind},
};
use zz_ui::{
    agent::{
        chat::ChatOpen,
        preview::ViewRequest,
        refs::ChatRefs,
        view::{ViewChat, view_body, view_buttons, view_chat_body},
    },
    pane::{
        PaneActionKeys, PaneDragSource, PaneMotion, PaneState, TerminalPaneAction, moving_pane,
        pane_header, pane_heading_icon,
    },
    pip::{PipSurface, departing_pip},
};

use super::chat::{Frame, Thread};
use super::fixtures::{Script, agent, story_hosts};

const STAGE_HEIGHT: f32 = 560.0;
const PIP_WIDTH: f32 = 380.0;
const PIP_MARGIN: f32 = 12.0;
const INSET: Size<Pixels> = Size {
    width: px(8.0),
    height: px(16.0),
};

const REPLY: &str = r"Every cell shared the first paragraph's key, so the whole table faded as one block. Each cell gets its own span now:

```rust src/agent/table.rs
fn cell_spans(row: &Row, source: &str) -> Vec<Span> {
    let mut spans = Vec::with_capacity(row.cells.len());
    let mut cursor = row.start;
    for cell in &row.cells {
        let start = cursor + leading_space(&source[cursor..]);
        let end = start + cell.text.len();
        spans.push(Span { start, end });
        cursor = end + 1;
    }
    spans
}
```

And the path a reply takes to the screen:

```mermaid
flowchart LR
    Driver --> Rows[Chat rows]
    Rows --> Markdown
    Rows --> Pane
    Markdown --> Pip
```
";

fn script() -> Script {
    let mut script = Script::new();
    script
        .turn("The table fades in as one block. Find out why, and have a subagent scan the drivers.")
        .text(
            "a1",
            "The empty block comes from the table. I'll have a subagent scan both drivers while I fix the fade here.",
        )
        .start(
            "ag1",
            None,
            ItemKind::Subagent,
            agent(
                "Scan the rendering drivers",
                "Read both drivers for labels that leak raw tool names.",
            ),
        )
        .tool(
            "s1",
            Some("ag1"),
            ItemKind::Read,
            "crates/zz-daemon/src/agent/claude/translate.rs",
        )
        .done("s1")
        .tool("s2", Some("ag1"), ItemKind::Search, "ItemStatus::Failed")
        .done("s2")
        .start(
            "s3",
            Some("ag1"),
            ItemKind::Agent,
            ItemBody::text(
                "Three labels are raw tool names, and a denied call reports as failed. Fixing the labels first.",
            ),
        )
        .done("s3")
        .done("ag1")
        .text("a2", REPLY);
    script
}

struct Shown {
    pane: PaneId,
    title: String,
    content: ViewContent,
}

pub struct FlightStage {
    thread: Entity<Thread>,
    view: ChatView,
    now: u64,
    motion: PaneMotion,
    shown: Option<Shown>,
    leaving: Vec<Shown>,
    next: u64,
}

impl FlightStage {
    fn open(&mut self, request: ViewRequest, window: &Window, cx: &mut Context<Self>) {
        if let Some(shown) = &mut self.shown {
            shown.title = request.title;
            shown.content = request.content;
            self.motion.relink(shown.pane, window, cx);
        } else {
            self.next += 1;
            self.shown = Some(Shown {
                pane: PaneId(100 + self.next),
                title: request.title,
                content: request.content,
            });
        }
        cx.notify();
    }

    fn close(&mut self, cx: &mut Context<Self>) {
        if let Some(shown) = self.shown.take()
            && self.motion.depart(shown.pane, cx)
        {
            self.leaving.push(shown);
        }
        cx.notify();
    }

    fn body(&self, shown: &Shown, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let pane = shown.pane;
        let body = match &shown.content {
            ViewContent::Subagent { .. } | ViewContent::Image { .. } => view_chat_body(
                &shown.content,
                &ViewChat {
                    view: Some(&self.view),
                    open: &ChatOpen::default(),
                    now: self.now,
                    phase: 0.0,
                    click: None,
                    vendor: zz_client::program::Program::by_id("claude"),
                    owner: 0,
                },
                cx,
            ),
            content => view_body(content, pane.0, false, window, cx),
        };
        div()
            .id(("story-flight-body", pane.0))
            .size_full()
            .overflow_y_scroll()
            .child(body)
            .into_any_element()
    }

    fn pip(&self, shown: &Shown, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let pane = shown.pane;
        let content = self.body(shown, window, cx);
        let stage = cx.entity().downgrade();
        let header = pane_header(
            pane_heading_icon(penna::IconName::Eye, None),
            true,
            shown.title.clone(),
            None,
            PaneDragSource::new(pane, shown.title.clone(), false, |_, _, _| {}),
            PaneState {
                floats: true,
                ..PaneState::default()
            },
            None,
            &view_buttons(&shown.content, false),
            &PaneActionKeys::default(),
            px(0.0),
            move |action, _, cx| {
                if action == TerminalPaneAction::Close {
                    stage.update(cx, FlightStage::close).ok();
                }
            },
            cx,
        );
        let surface = PipSurface::new(("story-flight-pip", pane.0), content)
            .header(header)
            .content_inset(INSET.width, INSET.height);
        slot(moving_pane(pane, &self.motion, surface).launches())
    }

    fn departing(&self, shown: &Shown, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let content = self.body(shown, window, cx);
        let card = departing_pip(
            shown.pane,
            pane_heading_icon(penna::IconName::Eye, None),
            shown.title.clone().into(),
            content,
            INSET,
            cx,
        );
        slot(moving_pane(shown.pane, &self.motion, card))
    }
}

fn slot(pip: impl IntoElement) -> AnyElement {
    div()
        .absolute()
        .top(px(PIP_MARGIN))
        .right(px(PIP_MARGIN))
        .bottom(px(PIP_MARGIN))
        .w(px(PIP_WIDTH))
        .child(pip)
        .into_any_element()
}

impl Render for FlightStage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let knobs = crate::knobs();
        self.motion
            .freeze(knobs.flag("pip-hold").then(|| knobs.value("pip-progress")));
        self.motion
            .begin(WindowId(1), window.viewport_size(), false);
        let motion = self.motion.clone();
        self.leaving.retain(|shown| motion.departing(shown.pane));
        let mut pips = self
            .leaving
            .iter()
            .map(|shown| self.departing(shown, window, cx))
            .collect::<Vec<_>>();
        pips.extend(self.shown.as_ref().map(|shown| self.pip(shown, window, cx)));
        div()
            .relative()
            .w_full()
            .h(px(STAGE_HEIGHT))
            .overflow_hidden()
            .bg(cx.theme().background)
            .child(self.thread.clone())
            .child(div().absolute().inset_0().children(pips))
    }
}

pub fn flight(window: &mut Window, cx: &mut App) -> AnyView {
    cx.new(|cx: &mut Context<FlightStage>| {
        let stage = cx.entity().downgrade();
        let refs = ChatRefs {
            hosts: story_hosts(),
            cwd: Some("/Users/me/dev/zebrilus".to_owned()),
            open: Arc::new(|target, window, cx| {
                window.push_notification(
                    Notification::new().message(format!("Open {}", target.label)),
                    cx,
                );
            }),
            view: Some(Arc::new(move |request, window, cx| {
                stage
                    .update(cx, |stage, cx| stage.open(request, window, cx))
                    .ok();
            })),
        };
        let script = script();
        let thread = Thread::new(script)
            .refs(refs)
            .framed(Frame::Scroll(Some(STAGE_HEIGHT)))
            .view(window, cx);
        let shown = self::script();
        FlightStage {
            thread,
            view: shown.view,
            now: shown.at,
            motion: PaneMotion::default(),
            shown: None,
            leaving: Vec::new(),
            next: 0,
        }
    })
    .into()
}
