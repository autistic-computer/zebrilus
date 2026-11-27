use std::sync::Arc;

use ala::{Image, RenderImage, TestAppContext, VisualTestContext, prelude::*};
use zz_protocol::agent_stream::AgentImage;
use zz_protocol::chat::{ChatEvent, ItemBody, ItemId, ItemKind, ItemStatus, TurnId};

use super::test_support::ready;
use super::tests::*;
use super::{ChatOpen, ChatPane, ChatView};

fn shot(tag: u8) -> Vec<u8> {
    let mut data = SHOT.to_vec();
    data.push(tag);
    data
}

fn pasted(view: &mut ChatView, id: &str, text: &str) {
    pasted_shot(view, id, text, SHOT.to_vec());
}

fn pasted_shot(view: &mut ChatView, id: &str, text: &str, data: Vec<u8>) {
    view.apply(&ChatEvent::ItemStarted {
        item: ItemId::new(id),
        turn: Some(TurnId(1)),
        parent: None,
        kind: ItemKind::User,
        body: ItemBody {
            text: text.to_owned(),
            images: vec![AgentImage {
                format: "image/png".to_owned(),
                data,
            }],
            ..ItemBody::default()
        },
        at: 2,
    })
    .expect("prompt");
    view.apply(&ChatEvent::ItemEnded {
        item: ItemId::new(id),
        status: ItemStatus::Completed,
        at: 2,
    })
    .expect("prompt ends");
}

#[test]
fn pasted_images_are_decoded_once_per_item() {
    let mut pane = ChatPane::default();
    let mut view = ready("s1");
    pasted(&mut view, "u1", "what is this?");
    pane.sync(&view);
    let first = pane.open.images.get(&ItemId::new("u1")).cloned();
    assert_eq!(first.as_ref().map(|images| images.len()), Some(1));
    pane.sync(&view);
    assert!(std::sync::Arc::ptr_eq(
        first.as_ref().expect("cached"),
        pane.open
            .images
            .get(&ItemId::new("u1"))
            .expect("still cached")
    ));
    pane.sync(&ready("s2"));
    assert!(pane.open.images.is_empty());
}

#[ala::test]
fn a_pasted_image_shows_as_a_tile_in_the_bubble(cx: &mut TestAppContext) {
    cx.update(crate::init);
    let mut view = ready("s1");
    pasted(&mut view, "u1", "");
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
    let tile = cx
        .debug_bounds("chat-user-attachment")
        .expect("the pasted image should be painted");
    assert_eq!(tile.size.width, super::TRANSCRIPT_ATTACHMENT);
    assert_eq!(tile.size.width, tile.size.height);
}

#[ala::test]
fn an_evicted_image_drops_its_decoded_pixels(cx: &mut TestAppContext) {
    cx.update(crate::init);
    let mut view = ready("s1");
    pasted(&mut view, "u1", "");
    let mut pane = ChatPane::default();
    pane.sync(&view);
    let image = pane.open.images[&ItemId::new("u1")][0].clone();
    let open = pane.open.clone();
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|_| ThreadTest { view, open });
        penna::Root::new(view, window, cx)
    });
    let cx: &mut VisualTestContext = cx;
    cx.run_until_parked();
    cx.update(|window, cx| {
        _ = window.draw(cx);
    });
    let render = cx
        .update(|window, cx| image.clone().get_render_image(window, cx))
        .expect("the pasted image should be decoded");
    cx.update(|window, cx| {
        assert!(image.is_asset_cached(cx));
        assert!(window.has_image_atlas_entry(&render));
        let mut again = ready("s2");
        pasted(&mut again, "u1", "");
        pane.sync(&again);
        pane.release(window, cx);
        assert!(
            image.is_asset_cached(cx),
            "the same image shown again stays"
        );
        assert!(window.has_image_atlas_entry(&render));
        pane.sync(&ready("s3"));
        pane.release(window, cx);
        assert!(!image.is_asset_cached(cx));
        assert!(!window.has_image_atlas_entry(&render));
    });
}

fn painted(cx: &mut VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, cx| {
        _ = window.draw(cx);
    });
    cx.run_until_parked();
}

fn decoded(cx: &mut VisualTestContext, image: &Arc<Image>) -> Arc<RenderImage> {
    cx.update(|window, cx| image.clone().get_render_image(window, cx))
        .expect("the pasted image should be decoded")
}

fn held(cx: &mut VisualTestContext, image: &Arc<Image>, render: &RenderImage) -> (bool, bool) {
    cx.update(|window, cx| {
        (
            image.is_asset_cached(cx),
            window.has_image_atlas_entry(render),
        )
    })
}

#[ala::test]
fn a_ninth_pasted_image_frees_the_oldest_ones_pixels(cx: &mut TestAppContext) {
    cx.update(crate::init);
    let mut view = ready("s1");
    pasted_shot(&mut view, "u0", "", shot(0));
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let captured = slot.clone();
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let owner = cx.new(|cx| Owner::new(view, window, cx));
        captured.replace(Some(owner.clone()));
        penna::Root::new(owner, window, cx)
    });
    let cx: &mut VisualTestContext = cx;
    let owner = slot.borrow_mut().take().expect("owner");
    painted(cx);
    let oldest = cx.update(|_, cx| owner.read(cx).image("u0"));
    let oldest_pixels = decoded(cx, &oldest);
    assert_eq!(held(cx, &oldest, &oldest_pixels), (true, true));
    for index in 1..9 {
        cx.update(|_, cx| {
            owner.update(cx, |owner, cx| {
                pasted_shot(&mut owner.view, &format!("u{index}"), "", shot(index));
                cx.notify();
            });
        });
        painted(cx);
    }
    let newest = cx.update(|_, cx| owner.read(cx).image("u8"));
    let newest_pixels = decoded(cx, &newest);
    let items = cx.update(|_, cx| {
        let mut items = owner
            .read(cx)
            .chat
            .open
            .images
            .keys()
            .map(|id| id.as_str().to_owned())
            .collect::<Vec<_>>();
        items.sort();
        items
    });
    assert_eq!(
        items,
        (1..9).map(|index| format!("u{index}")).collect::<Vec<_>>()
    );
    assert_eq!(
        held(cx, &oldest, &oldest_pixels),
        (false, false),
        "the ninth image frees the first one's pixels"
    );
    assert_eq!(held(cx, &newest, &newest_pixels), (true, true));
}

#[ala::test]
fn closing_a_chat_frees_its_images_but_not_one_another_chat_shows(cx: &mut TestAppContext) {
    cx.update(crate::init);
    let mut first = ready("s1");
    pasted_shot(&mut first, "both", "", shot(1));
    pasted_shot(&mut first, "own", "", shot(2));
    let mut second = ready("s2");
    pasted_shot(&mut second, "both", "", shot(1));
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let captured = slot.clone();
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let owners = cx.new(|cx| Owners {
            first: Some(cx.new(|cx| Owner::new(first, window, cx))),
            second: cx.new(|cx| Owner::new(second, window, cx)),
        });
        captured.replace(Some(owners.clone()));
        penna::Root::new(owners, window, cx)
    });
    let cx: &mut VisualTestContext = cx;
    let owners = slot.borrow_mut().take().expect("owners");
    painted(cx);
    let (shared, own) = cx.update(|_, cx| {
        let first = owners.read(cx).first.clone().expect("first chat");
        let first = first.read(cx);
        (first.image("both"), first.image("own"))
    });
    let shared_pixels = decoded(cx, &shared);
    let own_pixels = decoded(cx, &own);
    assert_eq!(held(cx, &shared, &shared_pixels), (true, true));
    assert_eq!(held(cx, &own, &own_pixels), (true, true));
    cx.update(|_, cx| {
        owners.update(cx, |owners, cx| {
            owners.first = None;
            cx.notify();
        });
    });
    painted(cx);
    assert_eq!(
        held(cx, &own, &own_pixels),
        (false, false),
        "the closed chat's own image is freed"
    );
    assert_eq!(
        held(cx, &shared, &shared_pixels),
        (true, true),
        "the other chat still shows the shared image"
    );
}

#[ala::test]
fn closing_the_window_frees_its_chats_images(cx: &mut TestAppContext) {
    cx.update(crate::init);
    let mut view = ready("s1");
    pasted_shot(&mut view, "own", "", shot(3));
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let captured = slot.clone();
    let (_, window_cx) = cx.add_window_view(move |window, cx| {
        let owner = cx.new(|cx| Owner::new(view, window, cx));
        captured.replace(Some(owner.downgrade()));
        penna::Root::new(owner, window, cx)
    });
    let window_cx: &mut VisualTestContext = window_cx;
    let owner = slot.borrow_mut().take().expect("owner");
    painted(window_cx);
    let image = window_cx.update(|_, cx| owner.upgrade().expect("owner").read(cx).image("own"));
    window_cx.update(|window, cx| {
        assert!(image.is_asset_cached(cx));
        window.remove_window();
    });
    cx.run_until_parked();
    assert!(owner.upgrade().is_none(), "the window took its chat along");
    cx.update(|cx| assert!(!image.is_asset_cached(cx)));
}
