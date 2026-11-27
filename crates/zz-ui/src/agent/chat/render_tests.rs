use std::sync::Arc;

use ala::{TestAppContext, VisualTestContext, prelude::*, px};
use zz_protocol::agent_stream::AgentQuestion;
use zz_protocol::chat::{
    Answer, Ask, ChatEvent, ItemBody, ItemId, ItemKind, ItemStatus, QuestionAnswer, RequestClose,
    RequestId, TurnBy, TurnId, TurnOutcome,
};

use super::test_support::ready;
use super::tests::*;
use super::{ChatOpen, ChatPane, ChatRefs, ChatView};

#[ala::test]
fn an_opened_thought_scrolls_inside_a_capped_height(cx: &mut TestAppContext) {
    cx.update(crate::init);
    let mut view = ready("s1");
    let long = (0..200)
        .map(|line| format!("Line {line} of a long thought."))
        .collect::<Vec<_>>()
        .join("\n\n");
    view.apply(&ChatEvent::ItemStarted {
        item: ItemId::new("t1"),
        turn: Some(TurnId(1)),
        parent: None,
        kind: ItemKind::Reasoning,
        body: ItemBody::text(long),
        at: 3,
    })
    .expect("thought");
    let open = ChatOpen {
        runs: vec![ItemId::new("t1")],
        ..ChatOpen::default()
    };
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|_| ThreadTest { view, open });
        penna::Root::new(view, window, cx)
    });
    let cx: &mut VisualTestContext = cx;
    cx.run_until_parked();
    cx.update(|window, cx| {
        _ = window.draw(cx);
    });
    let thought = cx
        .debug_bounds("chat-thought")
        .expect("the opened thought should be painted");
    assert!(thought.size.height <= px(super::THOUGHT_MAX_HEIGHT));
    assert!(thought.size.height > px(40.0));
}

#[ala::test]
fn the_answer_to_a_long_question_stays_on_its_row(cx: &mut TestAppContext) {
    cx.update(crate::init);
    let mut view = ChatView::default();
    let events = [
        ChatEvent::SessionReady {
            session: "s1".to_owned(),
        },
        ChatEvent::TurnStarted {
            turn: TurnId(1),
            by: TurnBy::Agent,
            at: 1,
        },
        ChatEvent::RequestOpened {
            request: RequestId(4),
            turn: Some(TurnId(1)),
            item: None,
            ask: Ask::Questions {
                questions: vec![AgentQuestion {
                    id: "q0".to_owned(),
                    question: "Can you have Feb submit a bug report from the affected tab and share the ticket ID? There's no report from that session yet.".to_owned(),
                    ..AgentQuestion::default()
                }],
            },
            blocking: false,
            at: 2,
        },
        ChatEvent::RequestClosed {
            request: RequestId(4),
            close: RequestClose::Answered {
                answer: Answer::Questions {
                    answers: vec![QuestionAnswer {
                        question: "q0".to_owned(),
                        answers: vec!["Ticket 4821".to_owned()],
                    }],
                },
            },
            at: 3,
        },
    ];
    for event in &events {
        view.apply(event).expect("event");
    }
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|_| ThreadTest {
            view,
            open: ChatOpen::default(),
        });
        penna::Root::new(view, window, cx)
    });
    let cx: &mut VisualTestContext = cx;
    cx.run_until_parked();
    cx.update(|window, cx| {
        _ = window.draw(cx);
    });
    let answer = cx
        .debug_bounds("chat-activity-detail")
        .expect("the answer should be painted");
    assert!(answer.size.width > px(40.0), "{answer:?}");
}

#[ala::test]
fn the_first_chat_row_rests_below_the_floating_header(cx: &mut TestAppContext) {
    cx.update(crate::init);
    let (host, cx) = cx.add_window_view(|_, _| InsetHost {
        pane: ChatPane::default(),
        view: ready("s1"),
        refs: None,
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        _ = window.draw(cx);
    });
    let top = host.read_with(cx, |host, _| {
        host.pane.list.bounds_for_item(0).map(|bounds| bounds.top())
    });
    assert_eq!(top, Some(px(46.0)));
}
#[ala::test]
fn a_click_on_a_page_row_opens_the_file_under_the_chats_folder(cx: &mut TestAppContext) {
    use std::sync::Mutex;

    cx.update(crate::init);
    let mut view = ChatView::default();
    let item = |id: &str, kind, body, at| ChatEvent::ItemStarted {
        item: ItemId::new(id),
        turn: Some(TurnId(1)),
        parent: None,
        kind,
        body,
        at,
    };
    let done = |id: &str, at| ChatEvent::ItemEnded {
        item: ItemId::new(id),
        status: ItemStatus::Completed,
        at,
    };
    let page = ItemBody {
        target: Some("site/landing.html".to_owned()),
        ..ItemBody::default()
    };
    let answer = ItemBody {
        text: "Dark hero, green button.".to_owned(),
        ..ItemBody::default()
    };
    for event in [
        ChatEvent::SessionReady {
            session: "s1".to_owned(),
        },
        ChatEvent::TurnStarted {
            turn: TurnId(1),
            by: TurnBy::Agent,
            at: 1,
        },
        item("e1", ItemKind::Edit, page, 2),
        done("e1", 3),
        item("a1", ItemKind::Agent, answer, 4),
        done("a1", 5),
        ChatEvent::TurnEnded {
            turn: TurnId(1),
            outcome: TurnOutcome::Completed {
                stop_reason: "end_turn".to_owned(),
            },
            at: 6,
        },
    ] {
        view.apply(&event).expect("event");
    }
    let opened = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&opened);
    let refs = ChatRefs {
        hosts: Vec::new(),
        cwd: Some("/work".to_owned()),
        open: Arc::new(|_, _, _| {}),
        view: None,
        page: Some(Arc::new(move |path, _, _| {
            seen.lock().unwrap().push(path.to_owned());
        })),
    };
    let (_host, cx) = cx.add_window_view(|_, _| InsetHost {
        pane: ChatPane::default(),
        view,
        refs: Some(refs),
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        _ = window.draw(cx);
    });
    let row = cx.debug_bounds("chat-page").expect("the page row is drawn");
    cx.simulate_click(row.center(), ala::Modifiers::none());
    assert_eq!(*opened.lock().unwrap(), ["/work/site/landing.html"]);
}
