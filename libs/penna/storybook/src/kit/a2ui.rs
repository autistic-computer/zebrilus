use ala::{
    AnyView, App, AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render,
    SharedString, Styled as _, Subscription, Window, px,
};
use penna::{
    Label, Space, TextStyle,
    blocks::a2ui::{A2uiEvent, A2uiSurface, Message, compile_express, parse_messages},
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

struct Demo {
    surface: Entity<A2uiSurface>,
    last: Option<SharedString>,
    _actions: Subscription,
}

impl Demo {
    fn view(id: &'static str, messages: Vec<Message>, cx: &mut App) -> AnyView {
        cx.new(|cx| {
            let surface = cx.new(|cx| {
                let mut surface = A2uiSurface::new(id);
                for message in messages {
                    surface.apply(message, cx);
                }
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
        v_flex()
            .w(px(640.0))
            .gap(Space::S12)
            .child(self.surface.clone())
            .children(last)
    }
}
