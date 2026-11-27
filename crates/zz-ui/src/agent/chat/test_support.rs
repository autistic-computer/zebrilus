use super::ChatView;
use zz_protocol::chat::{ChatEvent, ItemBody, ItemId, ItemKind, TurnBy, TurnId};

pub(super) fn ready(session: &str) -> ChatView {
    let mut view = ChatView::default();
    view.apply(&ChatEvent::SessionReady {
        session: session.to_owned(),
    })
    .expect("ready");
    view.apply(&ChatEvent::TurnStarted {
        turn: TurnId(1),
        by: TurnBy::Agent,
        at: 1,
    })
    .expect("turn");
    view.apply(&ChatEvent::ItemStarted {
        item: ItemId::new("a"),
        turn: Some(TurnId(1)),
        parent: None,
        kind: ItemKind::Subagent,
        body: ItemBody::titled("Explore"),
        at: 2,
    })
    .expect("item");
    view
}
