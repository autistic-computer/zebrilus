use crate::agent_stream::AgentImage;
use crate::chat::{
    ChatEvent, ChatLog, ChatView, ItemBody, ItemId, ItemKind, ItemStatus, TurnBy, TurnId,
    TurnOutcome,
};

fn shot(index: u8) -> ItemBody {
    ItemBody {
        text: format!("look {index}"),
        images: vec![AgentImage {
            format: "image/png".to_owned(),
            data: vec![index],
        }],
        ..ItemBody::default()
    }
}

fn live(events: &[ChatEvent]) -> (ChatView, ChatLog) {
    let mut view = ChatView::default();
    let mut log = ChatLog::default();
    for event in [
        ChatEvent::SessionReady {
            session: "s1".to_owned(),
        },
        ChatEvent::TurnStarted {
            turn: TurnId(1),
            by: TurnBy::Agent,
            at: 1,
        },
    ]
    .iter()
    .chain(events)
    {
        view.apply(event).expect("live event");
        log.push(event);
    }
    (view, log)
}

fn replay(log: &ChatLog) -> ChatView {
    let mut view = ChatView::default();
    view.apply(&ChatEvent::Restart).expect("restart");
    for event in log.events() {
        view.apply(event).expect("replayed event");
    }
    view
}

fn prompt(index: u8) -> ChatEvent {
    ChatEvent::ItemStarted {
        item: ItemId::new(format!("u:{index}")),
        turn: Some(TurnId(1)),
        parent: None,
        kind: ItemKind::User,
        body: shot(index),
        at: 2,
    }
}

fn pictured(view: &ChatView) -> Vec<u8> {
    let mut held = view
        .items
        .values()
        .filter(|item| !item.body.images.is_empty())
        .map(|item| item.body.images[0].data[0])
        .collect::<Vec<_>>();
    held.sort_unstable();
    held
}

#[test]
fn a_live_view_keeps_images_on_its_latest_eight_items_like_the_log() {
    let (view, log) = live(&(0..10).map(prompt).collect::<Vec<_>>());
    assert_eq!(pictured(&view), (2..10).collect::<Vec<_>>());
    assert_eq!(view.items[&ItemId::new("u:0")].body.text, "look 0");
    assert_eq!(replay(&log), view);
}

#[test]
fn an_item_that_loses_its_images_gives_up_its_place() {
    let mut events = (0..8).map(prompt).collect::<Vec<_>>();
    events.push(ChatEvent::ItemChanged {
        item: ItemId::new("u:5"),
        body: ItemBody::text("look 5"),
    });
    events.push(prompt(8));
    let (view, log) = live(&events);
    assert_eq!(pictured(&view), [0, 1, 2, 3, 4, 6, 7, 8]);
    assert_eq!(replay(&log), view);
}

#[test]
fn a_new_session_forgets_which_items_had_images() {
    let (mut view, mut log) = live(&[prompt(0)]);
    for event in [
        ChatEvent::ItemEnded {
            item: ItemId::new("u:0"),
            status: ItemStatus::Completed,
            at: 3,
        },
        ChatEvent::TurnEnded {
            turn: TurnId(1),
            outcome: TurnOutcome::Cancelled,
            at: 3,
        },
        ChatEvent::SessionReady {
            session: "s2".to_owned(),
        },
    ] {
        view.apply(&event).expect("live event");
        log.push(&event);
    }
    assert_eq!(replay(&log), view);
}
