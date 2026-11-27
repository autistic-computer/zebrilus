use super::test_support::ready;
use super::{ChatClick, ChatPane, ChatView};
use crate::agent::ui::{UiLook, UiSent};
use zz_protocol::chat::{ChatEvent, ItemBody, ItemId, ItemKind, ItemStatus, TurnId};

#[test]
fn the_same_item_in_two_panes_keeps_separate_block_scopes() {
    assert_ne!(
        super::block_scope(Some(7), "chat-text-a1"),
        super::block_scope(Some(8), "chat-text-a1")
    );
    assert_eq!(
        super::block_scope(Some(7), "chat-text-a1"),
        super::block_scope(Some(7), "chat-text-a1")
    );
}

#[test]
fn opened_rows_close_when_the_session_changes() {
    let mut pane = ChatPane::default();
    let first = ready("s1");
    pane.sync(&first);
    pane.click(&ChatClick::Agent(ItemId::new("a")));
    pane.click(&ChatClick::Turn(TurnId(1)));
    pane.sync(&first);
    assert_eq!(pane.open.agent, Some(ItemId::new("a")));
    assert!(!pane.away());
    assert!(
        pane.list.follow_hold().is_some(),
        "an expand holds the view in place"
    );
    pane.sync(&ready("s2"));
    assert_eq!(pane.open.agent, None);
    assert!(pane.open.turns.is_empty());
    assert!(!pane.away(), "a new session lands on its latest message");
    assert_eq!(pane.list.follow_hold(), None);
}

#[test]
fn an_agent_pip_closes_on_a_second_click_or_close() {
    let mut pane = ChatPane::default();
    let view = ready("s1");
    pane.sync(&view);
    pane.click(&ChatClick::Agent(ItemId::new("a")));
    pane.click(&ChatClick::Agent(ItemId::new("a")));
    assert_eq!(pane.open.agent, None);
    pane.click(&ChatClick::Agent(ItemId::new("a")));
    pane.click(&ChatClick::CloseAgent);
    assert_eq!(pane.open.agent, None);
    assert!(!pane.away());
}

#[test]
fn opened_rows_drop_items_the_view_no_longer_has() {
    let mut pane = ChatPane::default();
    let view = ready("s1");
    pane.sync(&view);
    pane.click(&ChatClick::Agent(ItemId::new("gone")));
    pane.sync(&view);
    assert_eq!(pane.open.agent, None);
}

fn finished(view: &mut ChatView, item: &str, parent: Option<&str>, kind: ItemKind, at: u64) {
    let body = if kind == ItemKind::Agent {
        ItemBody::text("done")
    } else {
        ItemBody::titled(item)
    };
    view.apply(&ChatEvent::ItemStarted {
        item: ItemId::new(item),
        turn: Some(TurnId(1)),
        parent: parent.map(ItemId::new),
        kind,
        body,
        at,
    })
    .expect("start");
    view.apply(&ChatEvent::ItemEnded {
        item: ItemId::new(item),
        status: ItemStatus::Completed,
        at,
    })
    .expect("end");
}

fn said(view: &mut ChatView, id: &str, kind: ItemKind, text: &str, ended: bool) {
    view.apply(&ChatEvent::ItemStarted {
        item: ItemId::new(id),
        turn: Some(TurnId(1)),
        parent: None,
        kind,
        body: ItemBody::text(text),
        at: 3,
    })
    .expect("start");
    if ended {
        view.apply(&ChatEvent::ItemEnded {
            item: ItemId::new(id),
            status: ItemStatus::Completed,
            at: 3,
        })
        .expect("end");
    }
}

#[test]
fn a_ui_block_is_held_by_what_the_user_sends_after_it() {
    let mut view = ready("s1");
    let block = "Name it.\n\n```ui\nroot = Button(\"Create\", Event(\"create\", {name: $/name}))\n$/name = \"fix\"\n```\n";
    said(&mut view, "reply", ItemKind::Agent, block, true);
    let mut pane = ChatPane::default();
    pane.sync(&view);
    assert_eq!(pane.open.ui[&ItemId::new("reply")], UiLook::default());
    assert!(pane.open.sent.is_empty());

    said(
        &mut view,
        "press",
        ItemKind::User,
        "ui action create {\"name\":\"fix\"}",
        true,
    );
    said(
        &mut view,
        "next",
        ItemKind::Agent,
        "Again:\n\n```ui\nroot = Te",
        false,
    );
    pane.sync(&view);
    assert_eq!(
        pane.open.ui[&ItemId::new("reply")],
        UiLook {
            held: true,
            pressed: Some("create".to_owned()),
            open: false,
        }
    );
    assert_eq!(
        pane.open.ui[&ItemId::new("next")],
        UiLook {
            held: false,
            pressed: None,
            open: true,
        }
    );
    assert_eq!(
        pane.open.sent[&ItemId::new("press")],
        UiSent {
            label: "Create".into(),
            values: vec!["fix".into()],
            event: Some("create".to_owned()),
        }
    );
}

#[test]
fn a_long_reply_previews_its_first_64_lines() {
    let mut view = ready("s1");
    let long = format!("```\n{}```\n", "line\n".repeat(600));
    view.apply(&ChatEvent::ItemStarted {
        item: ItemId::new("long"),
        turn: Some(TurnId(1)),
        parent: None,
        kind: ItemKind::Agent,
        body: ItemBody::text(long.clone()),
        at: 3,
    })
    .expect("reply");
    finished(&mut view, "short", None, ItemKind::Agent, 4);
    let mut pane = ChatPane::default();
    pane.sync(&view);
    assert_eq!(
        pane.open.previews[&ItemId::new("long")].as_ref(),
        format!("```\n{}", "line\n".repeat(63))
    );
    assert_eq!(pane.open.texts[&ItemId::new("long")].as_ref(), long);
    assert!(!pane.open.previews.contains_key(&ItemId::new("short")));
}

#[test]
fn revealing_a_folded_tool_opens_its_turn_and_run_or_its_agent() {
    let mut view = ready("s1");
    finished(&mut view, "step", Some("a"), ItemKind::Read, 3);
    view.apply(&ChatEvent::ItemEnded {
        item: ItemId::new("a"),
        status: ItemStatus::Completed,
        at: 3,
    })
    .expect("agent ends");
    finished(&mut view, "read", None, ItemKind::Read, 4);
    finished(&mut view, "reply", None, ItemKind::Agent, 5);
    view.apply(&ChatEvent::TurnEnded {
        turn: TurnId(1),
        outcome: zz_protocol::chat::TurnOutcome::Completed {
            stop_reason: "end_turn".to_owned(),
        },
        at: 6,
    })
    .expect("turn ends");
    let mut pane = ChatPane::default();
    pane.sync(&view);
    assert!(!pane.away());

    pane.reveal(&view, &ItemId::new("read"));
    assert_eq!(pane.open.turns, [TurnId(1)]);
    assert_eq!(pane.open.runs, [ItemId::new("read")]);
    assert_eq!(pane.open.agent, None);
    assert!(pane.away(), "a revealed row stays put");

    pane.reveal(&view, &ItemId::new("step"));
    assert_eq!(pane.open.turns, [TurnId(1)]);
    assert_eq!(pane.open.agent, Some(ItemId::new("a")));
}
