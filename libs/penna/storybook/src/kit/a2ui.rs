use ala::{
    AnyView, App, AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render,
    SharedString, Styled as _, Subscription, Window, px,
};
use penna::{
    Label, Space, TextStyle,
    blocks::a2ui::{A2uiEvent, A2uiSurface, Message, Mode, compile_express, parse_messages},
    v_flex,
};
use serde_json::Value;

use crate::story::{Section, Story};

pub const STORY: Story = Story {
    id: "ui-blocks",
    name: "A2UI surface",
    group: "Kit",
    summary: "A2uiSurface draws a declarative spec with penna components: a flat list of components by id, a data model they bind to, and named actions sent back as events.",
    sections: &[
        Section {
            id: "deploy",
            name: "Every component",
            summary: "Stats, badges, a segmented control, buttons, a field, a select, a slider, progress and a table from one spec. Inputs write to the data model; Deploy reports its context below.",
            build: |_, cx| json("deploy", include_str!("a2ui/deploy.json"), cx),
        },
        Section {
            id: "held",
            name: "Held",
            summary: "set_mode(Mode::Held { pressed }) once the block was answered: every control is off and keeps its value, and the pressed button carries a check.",
            build: |_, cx| {
                let messages = parse_messages(include_str!("a2ui/deploy.json"))
                    .expect("the story spec is valid JSON");
                let held = Mode::Held {
                    pressed: Some("deploy".to_owned()),
                };
                Demo::build("held", messages, held, Look::Desk, cx)
            },
        },
        Section {
            id: "chart",
            name: "Chart",
            summary: "Chart(rows, x, series, kind) reads a list from the data model: bars by default, lines with \"line\", up to four series.",
            build: |_, cx| {
                let spec = include_str!("a2ui/week.a2ui");
                let message = compile_express(spec).expect("the story spec compiles");
                Demo::view("chart", vec![message], cx)
            },
        },
        Section {
            id: "phone",
            name: "Phone",
            summary: "set_touch(true) draws the same spec for a thumb: 44 buttons sharing the row, 56 fields as wide as the block, a full-width segmented control, text on the touch ladder and at most two columns.",
            build: |_, cx| {
                let messages = parse_messages(include_str!("a2ui/deploy.json"))
                    .expect("the story spec is valid JSON");
                Demo::build("phone", messages, Mode::Live, Look::Phone, cx)
            },
        },
        Section {
            id: "list",
            name: "List template",
            summary: "One component repeated per item of a list in the data model, with paths read from the item.",
            build: |_, cx| json("list", include_str!("a2ui/list.json"), cx),
        },
        Section {
            id: "compact",
            name: "Compact form",
            summary: "The same kind of surface written as a few lines of calls, which compile to the same document as the JSON.",
            build: |_, cx| {
                let spec = include_str!("a2ui/settings.a2ui");
                let message = compile_express(spec).expect("the story spec compiles");
                Demo::view("compact", vec![message], cx)
            },
        },
        Section {
            id: "broken",
            name: "Problems",
            summary: "A spec with no root and a missing child lists what is wrong above whatever still draws.",
            build: |_, cx| json("broken", include_str!("a2ui/broken.json"), cx),
        },
    ],
};

fn json(id: &'static str, spec: &'static str, cx: &mut App) -> AnyView {
    let messages = parse_messages(spec).expect("the story spec is valid JSON");
    Demo::view(id, messages, cx)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Look {
    Desk,
    Phone,
}

struct Demo {
    surface: Entity<A2uiSurface>,
    look: Look,
    last: Option<SharedString>,
    _actions: Subscription,
}

impl Demo {
    fn view(id: &'static str, messages: Vec<Message>, cx: &mut App) -> AnyView {
        Self::build(id, messages, Mode::Live, Look::Desk, cx)
    }

    fn build(
        id: &'static str,
        messages: Vec<Message>,
        mode: Mode,
        look: Look,
        cx: &mut App,
    ) -> AnyView {
        cx.new(|cx| {
            let surface = cx.new(|cx| {
                let mut surface = A2uiSurface::new(id);
                for message in messages {
                    surface.apply(message, cx);
                }
                surface.set_mode(mode, cx);
                surface.set_touch(look == Look::Phone, cx);
                surface
            });
            let actions = cx.subscribe(&surface, |this: &mut Self, _, event, cx| {
                let A2uiEvent::Action(action) = event;
                let context = Value::Object(action.context.iter().cloned().collect());
                this.last = Some(format!("{} {context}", action.name).into());
                cx.notify();
            });
            Self {
                surface,
                look,
                last: None,
                _actions: actions,
            }
        })
        .into()
    }
}

impl Render for Demo {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let last = self
            .last
            .clone()
            .map(|last| Label::new(TextStyle::Mono, last).mono().muted());
        let width = match self.look {
            Look::Desk => 640.0,
            Look::Phone => 358.0,
        };
        v_flex()
            .w(px(width))
            .gap(Space::S12)
            .child(self.surface.clone())
            .children(last)
    }
}
