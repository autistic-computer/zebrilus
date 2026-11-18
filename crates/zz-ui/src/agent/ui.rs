use std::{
    fmt,
    hash::{DefaultHasher, Hash, Hasher},
    rc::Rc,
    time::Duration,
};

use ala::{
    AnyElement, App, Context, Entity, Global, IntoElement, SharedString, Subscription, Task,
    WeakEntity, Window, div, prelude::*, px,
};
use penna::{
    Card, CardLook, TextStyle,
    blocks::a2ui::{
        A2uiEvent, A2uiSurface, Action, Document, Message, Mode, compile_express, parse_messages,
    },
};
use serde::de::{Deserialize, Deserializer, MapAccess, Visitor};
use serde_json::Value;

use super::visual::{DRAWING_HEIGHT, drawing, failure};
use crate::{
    ActiveTheme as _, Colorize as _, Disableable as _, StyledExt as _,
    button::Button,
    h_flex,
    text::{MarkdownNode, MarkdownParseContext, MarkdownPlugin, markdown_ast},
    v_flex,
};

pub(crate) const UI_NODE_NAME: &str = "zz-ui";
const UI_LANGUAGE: &str = "ui";
const SURFACE: &str = "main";
const MAX_SOURCE_BYTES: usize = 64 * 1024;
const ACTION_PREFIX: &str = "ui action ";
const PROBLEMS_PREFIX: &str = "ui problems ";
const FIX_LABEL: &str = "Ask for a fix";
const SENT_VALUE_CHARS: usize = 48;
const SENT_VALUE_INK: f32 = 0.72;
const PRESS_HOLD: Duration = Duration::from_secs(2);

pub struct AgentUiSender(pub Rc<dyn Fn(u64, String, &mut Window, &mut App) -> bool>);

impl Global for AgentUiSender {}

pub(crate) fn draws_ui(cx: &App) -> bool {
    cx.has_global::<AgentUiSender>()
}

fn send(owner: u64, text: String, window: &mut Window, cx: &mut App) -> bool {
    cx.try_global::<AgentUiSender>()
        .map(|sender| Rc::clone(&sender.0))
        .is_some_and(|sender| sender(owner, text, window, cx))
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UiLook {
    pub held: bool,
    pub pressed: Option<String>,
    pub open: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiSent {
    pub label: SharedString,
    pub values: Vec<SharedString>,
    pub event: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Sent {
    Action { name: String, values: Vec<String> },
    Problems(Vec<String>),
}

struct Fence<'text> {
    body: &'text str,
    closed: bool,
}

fn fence_run(line: &str) -> Option<(char, usize, &str)> {
    let line = line.trim_start();
    let marker = line
        .chars()
        .next()
        .filter(|marker| matches!(marker, '`' | '~'))?;
    let length = line
        .chars()
        .take_while(|character| *character == marker)
        .count();
    (length >= 3).then(|| (marker, length, line[length..].trim()))
}

fn fences(text: &str) -> Vec<Fence<'_>> {
    let mut found = Vec::new();
    let mut open: Option<(char, usize, bool, usize)> = None;
    let mut at = 0;
    for line in text.split_inclusive('\n') {
        let start = at;
        at += line.len();
        let run = fence_run(line);
        match (open, run) {
            (None, Some((marker, length, info))) => {
                let ui = info
                    .split_whitespace()
                    .next()
                    .is_some_and(|language| language.eq_ignore_ascii_case(UI_LANGUAGE));
                open = Some((marker, length, ui, at));
            }
            (Some((marker, length, ui, body)), Some((closing, run, "")))
                if closing == marker && run >= length =>
            {
                if ui {
                    found.push(Fence {
                        body: &text[body..start],
                        closed: true,
                    });
                }
                open = None;
            }
            _ => {}
        }
    }
    if let Some((_, _, true, body)) = open {
        found.push(Fence {
            body: &text[body.min(text.len())..],
            closed: false,
        });
    }
    found
}

pub(crate) fn has_block(text: &str) -> bool {
    !fences(text).is_empty()
}

pub(crate) fn tail_open(text: &str) -> bool {
    fences(text).last().is_some_and(|fence| !fence.closed)
}

fn compile(text: &str) -> Result<Vec<Message>, String> {
    let messages = if text.trim_start().starts_with(['{', '[']) {
        parse_messages(text)
    } else {
        compile_express(text).map(|message| vec![message])
    };
    messages.map_err(|error| error.to_string())
}

fn document(text: &str) -> Option<Document> {
    let mut document = Document::default();
    for message in compile(text).ok()? {
        document.apply(message);
    }
    document.problems().is_empty().then_some(document)
}

fn button_label(reply: &str, event: &str) -> Option<String> {
    fences(reply)
        .iter()
        .filter(|fence| fence.closed)
        .filter_map(|fence| document(fence.body))
        .find_map(|document| {
            document
                .buttons()
                .into_iter()
                .find(|(name, label)| name == event && !label.is_empty())
                .map(|(_, label)| label)
        })
}

fn action_line(action: &Action) -> String {
    let mut line = format!("{ACTION_PREFIX}{}", action.name);
    for (index, (name, value)) in action.context.iter().enumerate() {
        line.push_str(if index == 0 { " {" } else { "," });
        line.push_str(&Value::from(name.as_str()).to_string());
        line.push(':');
        line.push_str(&value.to_string());
    }
    if !action.context.is_empty() {
        line.push('}');
    }
    line
}

fn problems_line(problems: &[SharedString]) -> String {
    let problems: Vec<Value> = problems
        .iter()
        .map(|problem| Value::from(problem.as_ref()))
        .collect();
    format!("{PROBLEMS_PREFIX}{}", Value::Array(problems))
}

struct Pairs(Vec<(String, Value)>);

impl<'de> Deserialize<'de> for Pairs {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Listed;

        impl<'de> Visitor<'de> for Listed {
            type Value = Pairs;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a map")
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Pairs, A::Error> {
                let mut pairs = Vec::new();
                while let Some(pair) = map.next_entry()? {
                    pairs.push(pair);
                }
                Ok(Pairs(pairs))
            }
        }

        deserializer.deserialize_map(Listed)
    }
}

fn value_shown(value: &Value) -> Option<String> {
    match value {
        Value::Null => None,
        Value::String(text) => (!text.is_empty()).then(|| text.clone()),
        other => Some(other.to_string()),
    }
}

pub(crate) fn pressed(text: &str) -> Option<String> {
    match sent(text)? {
        Sent::Action { name, .. } => Some(name),
        Sent::Problems(_) => None,
    }
}

pub(crate) fn sent(text: &str) -> Option<Sent> {
    let text = text.trim();
    if let Some(problems) = text.strip_prefix(PROBLEMS_PREFIX) {
        return serde_json::from_str(problems).ok().map(Sent::Problems);
    }
    let action = text.strip_prefix(ACTION_PREFIX)?;
    let (name, context) = action.split_once(' ').unwrap_or((action, ""));
    if name.is_empty() || name.contains('\n') {
        return None;
    }
    let values = if context.trim().is_empty() {
        Vec::new()
    } else {
        serde_json::from_str::<Pairs>(context)
            .ok()?
            .0
            .iter()
            .filter_map(|(_, value)| value_shown(value))
            .collect()
    };
    Some(Sent::Action {
        name: name.to_owned(),
        values,
    })
}

fn cut(value: &str) -> SharedString {
    let value = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if value.chars().count() <= SENT_VALUE_CHARS {
        return value.into();
    }
    let kept: String = value.chars().take(SENT_VALUE_CHARS).collect();
    format!("{}…", kept.trim_end()).into()
}

impl UiSent {
    pub(crate) fn new(sent: &Sent, reply: Option<&str>) -> Self {
        match sent {
            Sent::Action { name, values } => Self {
                label: reply
                    .and_then(|reply| button_label(reply, name))
                    .unwrap_or_else(|| name.clone())
                    .into(),
                values: values.iter().map(|value| cut(value)).collect(),
                event: Some(name.clone()),
            },
            Sent::Problems(problems) => Self {
                label: FIX_LABEL.into(),
                values: problems.iter().map(|problem| cut(problem)).collect(),
                event: None,
            },
        }
    }
}

pub(crate) fn sent_row(sent: &UiSent, touch: bool, cx: &App) -> AnyElement {
    let style = if touch {
        TextStyle::TouchBody
    } else {
        TextStyle::ReadBody
    };
    h_flex()
        .flex_wrap()
        .items_baseline()
        .gap(px(6.0))
        .debug_selector(|| "agent-ui-sent".to_owned())
        .child(div().type_style_strong(style, cx).child(sent.label.clone()))
        .children(
            sent.values
                .iter()
                .map(|value| div().opacity(SENT_VALUE_INK).child(value.clone())),
        )
        .into_any_element()
}

struct UiBlock {
    key: Option<u64>,
    surface: Entity<A2uiSurface>,
    problems: Vec<SharedString>,
    buttons: bool,
    pressed: Option<String>,
    _release: Option<Task<()>>,
    _actions: Subscription,
}

impl UiBlock {
    fn new(owner: u64, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let surface = cx.new(|_| A2uiSurface::new(SURFACE));
        let actions = cx.subscribe_in(
            &surface,
            window,
            move |block, _, event: &A2uiEvent, window, cx| {
                let A2uiEvent::Action(action) = event;
                if block.pressed.is_some() || !send(owner, action_line(action), window, cx) {
                    return;
                }
                block.pressed = Some(action.name.clone());
                let hold = cx.background_executor().timer(PRESS_HOLD);
                block._release = Some(cx.spawn(async move |this: WeakEntity<Self>, cx| {
                    hold.await;
                    let _ = this.update(cx, |block, cx| {
                        block.pressed = None;
                        cx.notify();
                    });
                }));
                cx.notify();
            },
        );
        Self {
            key: None,
            surface,
            problems: Vec::new(),
            buttons: false,
            pressed: None,
            _release: None,
            _actions: actions,
        }
    }

    fn synchronize(&mut self, text: &str, cx: &mut Context<Self>) {
        let mut hasher = DefaultHasher::new();
        text.hash(&mut hasher);
        let key = hasher.finish();
        if self.key == Some(key) {
            return;
        }
        self.key = Some(key);
        match compile(text) {
            Ok(messages) => {
                self.surface.update(cx, |surface, cx| {
                    surface.apply(
                        Message::DeleteSurface {
                            surface_id: SURFACE.to_owned(),
                        },
                        cx,
                    );
                    for message in messages {
                        surface.apply(message, cx);
                    }
                });
                let document = self.surface.read(cx).document();
                self.problems = document
                    .problems()
                    .into_iter()
                    .map(SharedString::from)
                    .collect();
                self.buttons = !document.buttons().is_empty();
            }
            Err(error) => {
                self.problems = vec![error.into()];
                self.buttons = false;
            }
        }
        cx.notify();
    }
}

#[derive(Clone)]
struct UiSource {
    text: String,
    offset: usize,
    open: bool,
}

pub(crate) struct UiPlugin {
    pub(crate) owner: u64,
    pub(crate) look: UiLook,
}

impl MarkdownPlugin for UiPlugin {
    fn name(&self) -> &str {
        UI_NODE_NAME
    }

    fn parse<'a>(
        &self,
        node: &'a markdown_ast::AstNode<'a>,
        cx: &MarkdownParseContext<'_>,
    ) -> Option<MarkdownNode> {
        let code = cx.code_fence(node)?;
        if !code
            .lang
            .as_deref()
            .is_some_and(|language| language.eq_ignore_ascii_case(UI_LANGUAGE))
            || code.value.len() > MAX_SOURCE_BYTES
        {
            return None;
        }
        let open = self.look.open
            && cx
                .node_range(node)
                .is_some_and(|range| range.end >= cx.source().trim_end().len());
        let markdown = cx.node_source(node).unwrap_or(&code.value).to_owned();
        Some(
            MarkdownNode::new(
                UI_NODE_NAME,
                UiSource {
                    text: code.value.clone(),
                    offset: code.start,
                    open,
                },
            )
            .text(code.value)
            .markdown(markdown),
        )
    }

    fn render(&self, node: &MarkdownNode, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let source = node.data::<UiSource>().expect("ui node data");
        render_ui(source, self.owner, &self.look, window, cx)
    }
}

fn failed(
    problems: &[SharedString],
    owner: u64,
    held: bool,
    offset: usize,
    cx: &App,
) -> AnyElement {
    let line = problems_line(problems);
    let first = problems.first().cloned().unwrap_or_default();
    v_flex()
        .w_full()
        .gap_1()
        .debug_selector(|| "agent-ui-failed".to_owned())
        .child(
            failure(&SharedString::from("Couldn't draw this"), cx)
                .w_full()
                .child(div().flex_1())
                .child(
                    Button::new(("agent-ui-fix", offset))
                        .label(FIX_LABEL)
                        .disabled(held)
                        .on_click(move |_, window, cx| {
                            send(owner, line.clone(), window, cx);
                        }),
                ),
        )
        .child(
            div()
                .pl(px(22.0))
                .type_style_mono(TextStyle::Micro, cx)
                .text_color(cx.theme().foreground.muted())
                .child(first),
        )
        .into_any_element()
}

fn render_ui(
    source: &UiSource,
    owner: u64,
    look: &UiLook,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let offset = source.offset;
    if source.open {
        return drawing(("agent-ui-drawing", offset), px(DRAWING_HEIGHT), cx);
    }
    let state = window.use_keyed_state(
        SharedString::from(format!("zz-ui/{offset}")),
        cx,
        move |window, cx| UiBlock::new(owner, window, cx),
    );
    state.update(cx, |block, cx| block.synchronize(&source.text, cx));
    let (problems, buttons, pressed, surface) = {
        let state = state.read(cx);
        (
            state.problems.clone(),
            state.buttons,
            state.pressed.clone(),
            state.surface.clone(),
        )
    };
    let content = if problems.is_empty() {
        let mode = if look.held && buttons {
            Mode::Held {
                pressed: look.pressed.clone(),
            }
        } else if pressed.is_some() {
            Mode::Held { pressed }
        } else {
            Mode::Live
        };
        surface.update(cx, |surface, cx| surface.set_mode(mode, cx));
        surface.into_any_element()
    } else {
        failed(&problems, owner, look.held, offset, cx)
    };
    Card::new(("agent-ui", offset), CardLook::Figure)
        .p_0()
        .w_full()
        .min_w_0()
        .debug_selector(|| "agent-ui".to_owned())
        .on_mouse_down(ala::MouseButton::Left, |_, _, cx| {
            penna::components::text::suppress_text_selection(cx);
        })
        .child(content)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use ala::{Modifiers, Render, TestAppContext, VisualTestContext, point};

    use super::*;
    use crate::{
        agent::chat_markdown_extensions,
        text::{MarkdownExtensions, TextView},
    };

    struct Answer {
        text: &'static str,
        extensions: MarkdownExtensions,
    }

    impl Render for Answer {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().w(px(520.0)).h(px(400.0)).child(
                TextView::markdown("answer", self.text)
                    .selectable(true)
                    .markdown_extensions(self.extensions.clone()),
            )
        }
    }

    type Lines = Rc<RefCell<Vec<(u64, String)>>>;

    fn draw(cx: &mut VisualTestContext) {
        for _ in 0..2 {
            cx.update(|window, cx| {
                _ = window.draw(cx);
            });
        }
    }

    fn shown<'a>(
        text: &'static str,
        look: UiLook,
        cx: &'a mut TestAppContext,
    ) -> (Lines, &'a mut VisualTestContext) {
        cx.update(crate::init);
        let sent = Rc::new(RefCell::new(Vec::new()));
        let sink = Rc::clone(&sent);
        cx.update(|cx| {
            cx.set_global(AgentUiSender(Rc::new(move |owner, text, _, _| {
                sink.borrow_mut().push((owner, text));
                true
            })));
        });
        let (_, cx) = cx.add_window_view(|window, cx| {
            let view = cx.new(|_| Answer {
                text,
                extensions: chat_markdown_extensions(None, None, Some(UiPlugin { owner: 7, look })),
            });
            penna::Root::new(view, window, cx)
        });
        draw(cx);
        (sent, cx)
    }

    fn pressed(look: UiLook, cx: &mut TestAppContext) -> Vec<(u64, String)> {
        let (sent, cx) = shown(
            "```ui\nroot = Button(\"Go\", Event(\"go\", {n: 1}))\n```\n",
            look,
            cx,
        );
        let block = cx.debug_bounds("agent-ui").expect("the block is drawn");
        for _ in 0..2 {
            cx.simulate_click(block.origin + point(px(12.0), px(12.0)), Modifiers::none());
            cx.run_until_parked();
        }
        sent.take()
    }

    #[ala::test]
    fn a_field_takes_typing_and_the_press_carries_it(cx: &mut TestAppContext) {
        let (sent, cx) = shown(
            "```ui\nroot = Column([name, go])\nname = TextField($/name)\ngo = Button(\"Go\", Event(\"go\", {name: $/name}))\n```\n",
            UiLook::default(),
            cx,
        );
        let block = cx.debug_bounds("agent-ui").expect("the block is drawn");
        cx.simulate_click(block.origin + point(px(12.0), px(12.0)), Modifiers::none());
        draw(cx);
        cx.simulate_input("ab");
        draw(cx);
        cx.simulate_click(
            point(block.origin.x + px(12.0), block.bottom() - px(12.0)),
            Modifiers::none(),
        );
        cx.run_until_parked();
        assert_eq!(
            sent.take(),
            [(7, r#"ui action go {"name":"ab"}"#.to_owned())]
        );
    }

    #[ala::test]
    fn a_live_block_sends_its_press_once_as_one_line(cx: &mut TestAppContext) {
        assert_eq!(
            pressed(UiLook::default(), cx),
            [(7, r#"ui action go {"n":1}"#.to_owned())]
        );
    }

    #[ala::test]
    fn a_held_block_sends_nothing(cx: &mut TestAppContext) {
        let held = UiLook {
            held: true,
            pressed: Some("go".to_owned()),
            open: false,
        };
        assert_eq!(pressed(held, cx), []);
    }

    const REPLY: &str = "Name it.\n\n```ui\nroot = Column([name, Row([create, cancel])])\nname = TextField($/name, \"Branch name\")\ncreate = Button(\"Create\", Event(\"create\", {name: $/name, base: $/base}), \"primary\")\ncancel = Button(\"Cancel\", Event(\"cancel\"))\n$/name = \"fix-tab-strip-gap\"\n$/base = \"main\"\n```\n";

    #[test]
    fn a_press_travels_as_one_line_and_reads_back_in_the_listed_order() {
        let action = Action {
            name: "create".to_owned(),
            surface_id: SURFACE.to_owned(),
            source_component_id: "create".to_owned(),
            context: vec![
                ("name".to_owned(), Value::from("fix-tab-strip-gap")),
                ("base".to_owned(), Value::from("main")),
            ],
            user_message: None,
        };
        let line = action_line(&action);
        assert_eq!(
            line,
            r#"ui action create {"name":"fix-tab-strip-gap","base":"main"}"#
        );
        let read = sent(&line).expect("the line reads back");
        assert_eq!(
            UiSent::new(&read, Some(REPLY)),
            UiSent {
                label: "Create".into(),
                values: vec!["fix-tab-strip-gap".into(), "main".into()],
                event: Some("create".to_owned()),
            }
        );
        assert_eq!(
            sent("ui action cancel"),
            Some(Sent::Action {
                name: "cancel".to_owned(),
                values: Vec::new(),
            })
        );
        assert_eq!(sent("ui action create {broken"), None);
        assert_eq!(sent("build the ui action menu"), None);
    }

    #[test]
    fn a_fix_request_carries_the_problems() {
        let line = problems_line(&["`base` is used but never assigned".into()]);
        assert_eq!(line, r#"ui problems ["`base` is used but never assigned"]"#);
        assert_eq!(
            UiSent::new(&sent(&line).expect("the line reads back"), None),
            UiSent {
                label: "Ask for a fix".into(),
                values: vec!["`base` is used but never assigned".into()],
                event: None,
            }
        );
    }

    #[test]
    fn only_an_unclosed_ui_fence_is_still_open() {
        assert!(has_block(REPLY));
        assert!(!tail_open(REPLY));
        assert!(tail_open("Here:\n\n```ui\nroot = Text(\"a"));
        assert!(!has_block("```python\nui = 1\n```\n"));
        assert!(!has_block("````md\n```ui\nroot = Text(\"a\")\n```\n````\n"));
    }
}
