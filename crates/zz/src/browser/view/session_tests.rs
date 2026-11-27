use super::*;
use ala::TestAppContext;
#[cfg(not(target_os = "macos"))]
use ala::VisualTestContext;
use ala_cef::BrowserError;
#[cfg(not(target_os = "macos"))]
use ala_cef::SessionId;
use penna::Root;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
use zz_daemon_client::DaemonError;

#[ala::test]
fn hidden_title_events_update_history_without_notifying_the_pane(cx: &mut TestAppContext) {
    let url = "https://example.com/";
    let profile = ala_cef::DEFAULT_BROWSER_PROFILE;
    cx.update(|cx| {
        zz_ui::init(cx);
        cx.set_global(recent_pages::RecentPages::default());
        recent_pages::record_visit(profile, url, cx);
    });
    let view_slot = Rc::new(RefCell::new(None));
    let captured_view = Rc::clone(&view_slot);
    let pane = PaneId(7);
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let controller =
            cx.new(|cx| BrowserController::new(Err(BrowserError::AlreadyShutdown), cx));
        let mux = cx.new(|cx| {
            MuxClient::new(
                Err(DaemonError::Thread("test client".to_owned())),
                zz_daemon_client::default_socket_path(),
                cx,
            )
        });
        let view = cx.new(|cx| {
            BrowserView::new(
                pane,
                &BrowserDescriptor::single(url.to_owned(), profile.to_owned()),
                controller,
                mux,
                window,
                cx,
            )
        });
        captured_view.replace(Some(view.clone()));
        Root::new(view, window, cx)
    });
    let view = view_slot.borrow().clone().expect("captured browser view");
    let notifications = Rc::new(Cell::new(0));
    let observed = Rc::clone(&notifications);
    cx.update(|_, cx| {
        cx.observe(&view, move |_, _| observed.set(observed.get() + 1))
            .detach();
    });
    for title in ["Hidden first", "Hidden second"] {
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                let controller = view.controller.clone();
                view.handle_controller_event(
                    &controller,
                    &ControllerEvent::Browser {
                        pane,
                        tab: view.active_tab,
                        event: BrowserEvent::TitleChanged {
                            session: SessionId(1),
                            title: Arc::from(title),
                        },
                    },
                    window,
                    cx,
                );
                assert_eq!(view.title, title);
                assert_eq!(
                    recent_pages::title(profile, url, cx).as_deref(),
                    Some(title)
                );
            });
        });
    }
    assert_eq!(notifications.get(), 0);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.set_visible(true, window, cx));
    });
    assert!(notifications.get() > 0);
    cx.update(|_, cx| assert_eq!(view.read(cx).title, "Hidden second"));
}

#[ala::test]
fn window_moves_carry_the_screen_origin_to_the_browser(cx: &mut TestAppContext) {
    cx.update(zz_ui::init);
    let view_slot = Rc::new(RefCell::new(None));
    let captured_view = Rc::clone(&view_slot);
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let controller =
            cx.new(|cx| BrowserController::new(Err(BrowserError::AlreadyShutdown), cx));
        let mux = cx.new(|cx| {
            MuxClient::new(
                Err(DaemonError::Thread("test client".to_owned())),
                zz_daemon_client::default_socket_path(),
                cx,
            )
        });
        let view = cx.new(|cx| {
            BrowserView::new(
                PaneId(7),
                &BrowserDescriptor::single(
                    "https://example.com/".to_owned(),
                    ala_cef::DEFAULT_BROWSER_PROFILE.to_owned(),
                ),
                controller,
                mux,
                window,
                cx,
            )
        });
        captured_view.replace(Some(view.clone()));
        Root::new(view, window, cx)
    });
    let view = view_slot.borrow().clone().expect("captured browser view");
    let screen_origin = |cx: &mut ala::VisualTestContext| {
        cx.update(|window, cx| {
            let view = view.read(cx);
            let content = view.content_bounds.expect("content bounds");
            let window_origin = window.bounds().origin;
            (
                (view.viewport.screen_x, view.viewport.screen_y),
                (
                    rounded_coordinate(window_origin.x + content.origin.x * window.zoom()),
                    rounded_coordinate(window_origin.y + content.origin.y * window.zoom()),
                ),
            )
        })
    };
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.update_content_bounds(
                Bounds::new(point(px(12.), px(40.)), ala::size(px(400.), px(300.))),
                window,
                cx,
            );
        });
    });
    let (viewport, expected) = screen_origin(cx);
    assert_eq!(viewport, expected);

    cx.simulate_move(point(px(300.), px(200.)));
    cx.run_until_parked();
    let (viewport, expected) = screen_origin(cx);
    assert_eq!(viewport, expected);
    assert_ne!(viewport, (12, 40));
}

#[ala::test]
fn held_resizes_cover_the_page_and_reach_the_browser_once(cx: &mut TestAppContext) {
    cx.update(zz_ui::init);
    let view_slot = Rc::new(RefCell::new(None));
    let captured_view = Rc::clone(&view_slot);
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let controller =
            cx.new(|cx| BrowserController::new(Err(BrowserError::AlreadyShutdown), cx));
        let mux = cx.new(|cx| {
            MuxClient::new(
                Err(DaemonError::Thread("test client".to_owned())),
                zz_daemon_client::default_socket_path(),
                cx,
            )
        });
        let view = cx.new(|cx| {
            BrowserView::new(
                PaneId(7),
                &BrowserDescriptor::single(
                    "https://example.com/".to_owned(),
                    ala_cef::DEFAULT_BROWSER_PROFILE.to_owned(),
                ),
                controller,
                mux,
                window,
                cx,
            )
        });
        captured_view.replace(Some(view.clone()));
        Root::new(view, window, cx)
    });
    let view = view_slot.borrow().clone().expect("captured browser view");
    let held = Rc::new(Cell::new(false));
    cx.update(|_, cx| {
        view.update(cx, |view, _| view.share_resize_hold(Rc::clone(&held)));
    });
    let resize = |width: f32, height: f32, cx: &mut ala::VisualTestContext| {
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.update_content_bounds(
                    Bounds::new(point(px(0.), px(40.)), ala::size(px(width), px(height))),
                    window,
                    cx,
                );
                (view.viewport.width, view.resize_covered())
            })
        })
    };

    assert_eq!(resize(400., 300., cx), (400, false));
    held.set(true);
    assert_eq!(resize(600., 300., cx), (400, true));
    assert_eq!(resize(700., 300., cx), (400, true));
    held.set(false);
    assert_eq!(resize(700., 300., cx), (700, true));
    cx.executor().advance_clock(RESIZE_COVER_TIMEOUT);
    cx.run_until_parked();
    assert!(!cx.update(|_, cx| view.read(cx).resize_covered()));

    let (width, height) = cx.update(|_, cx| {
        let viewport = view.read(cx).viewport;
        (viewport.width, viewport.height)
    });
    #[allow(clippy::cast_precision_loss, reason = "test pane sizes")]
    let (shown_width, shown_height) = (width as f32, height as f32);
    held.set(true);
    assert_eq!(resize(shown_width + 100., shown_height, cx), (width, true));
    held.set(false);
    assert_eq!(resize(shown_width, shown_height, cx), (width, false));
}

#[ala::test]
fn popup_tabs_render_blank_documents_and_follow_script_closure(cx: &mut TestAppContext) {
    cx.update(zz_ui::init);
    let view_slot = Rc::new(RefCell::new(None));
    let captured_view = Rc::clone(&view_slot);
    let pane = PaneId(7);
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let controller =
            cx.new(|cx| BrowserController::new(Err(BrowserError::AlreadyShutdown), cx));
        let mux = cx.new(|cx| {
            MuxClient::new(
                Err(DaemonError::Thread("test client".to_owned())),
                zz_daemon_client::default_socket_path(),
                cx,
            )
        });
        let view = cx.new(|cx| {
            BrowserView::new(
                pane,
                &BrowserDescriptor::single(
                    DEFAULT_URL.to_owned(),
                    ala_cef::DEFAULT_BROWSER_PROFILE.to_owned(),
                ),
                controller,
                mux,
                window,
                cx,
            )
        });
        captured_view.replace(Some(view.clone()));
        Root::new(view, window, cx)
    });
    let view = view_slot.borrow().clone().expect("captured browser view");
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.error = None;
            assert!(view.shows_empty_state());
            let popup = TabId(view.next_tab_id);
            view.next_tab_id += 1;
            view.tabs.push(BrowserTab {
                popup: true,
                ..BrowserTab::new(popup, DEFAULT_URL.to_owned())
            });
            view.activate_tab(popup, window, cx);
            assert!(!view.shows_empty_state());
            let controller = view.controller.clone();
            view.handle_controller_event(
                &controller,
                &ControllerEvent::Browser {
                    pane,
                    tab: popup,
                    event: BrowserEvent::Closed {
                        session: SessionId(1),
                    },
                },
                window,
                cx,
            );
            assert_eq!(view.tabs.len(), 2);
            view.handle_controller_event(
                &controller,
                &ControllerEvent::TabClosed { pane, tab: popup },
                window,
                cx,
            );
            assert_eq!(view.tabs.len(), 1);
            assert_eq!(view.active_tab, TAB_ID);
            assert!(view.shows_empty_state());
            view.handle_controller_event(
                &controller,
                &ControllerEvent::TabClosed { pane, tab: TAB_ID },
                window,
                cx,
            );
            assert_eq!(view.tabs.len(), 1);
            view.tabs[0].popup = true;
            view.handle_controller_event(
                &controller,
                &ControllerEvent::TabClosed { pane, tab: TAB_ID },
                window,
                cx,
            );
            assert_eq!(view.tabs.len(), 1);
            assert_ne!(view.active_tab, TAB_ID);
            assert!(!view.tabs[0].popup);
        });
    });
}

#[ala::test]
fn a_reordered_tab_list_reorders_tabs_without_navigating_them(cx: &mut TestAppContext) {
    cx.update(zz_ui::init);
    let view_slot = Rc::new(RefCell::new(None));
    let captured_view = Rc::clone(&view_slot);
    let urls = ["https://a.test/", "https://b.test/", "https://c.test/"].map(String::from);
    let tabs = urls.to_vec();
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let controller =
            cx.new(|cx| BrowserController::new(Err(BrowserError::AlreadyShutdown), cx));
        let mux = cx.new(|cx| {
            MuxClient::new(
                Err(DaemonError::Thread("test client".to_owned())),
                zz_daemon_client::default_socket_path(),
                cx,
            )
        });
        let view = cx.new(|cx| {
            let descriptor = BrowserDescriptor {
                tabs,
                active_tab: 1,
                profile: "default".to_owned(),
            };
            BrowserView::new(PaneId(7), &descriptor, controller, mux, window, cx)
        });
        captured_view.replace(Some(view.clone()));
        Root::new(view, window, cx)
    });
    let view = view_slot.borrow().clone().expect("captured browser view");
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let tabs = |view: &BrowserView| {
                view.tabs
                    .iter()
                    .map(|tab| tab.id)
                    .zip(view.tab_urls())
                    .collect::<Vec<_>>()
            };
            let before = tabs(view);
            let active = view.active_tab;
            let [a, b, c] = urls.clone();
            view.synchronize_tabs(&[c, a, b], 2, window, cx);
            assert_eq!(
                tabs(view),
                [before[2].clone(), before[0].clone(), before[1].clone()]
            );
            assert_eq!(view.active_tab, active);
        });
    });
}

#[ala::test]
fn first_blank_pane_edit_updates_omnibox_before_focus_event(cx: &mut TestAppContext) {
    cx.update(|cx| {
        zz_ui::init(cx);
        cx.set_global(recent_pages::RecentPages::default());
        recent_pages::record_visit(ala_cef::DEFAULT_BROWSER_PROFILE, "https://example.com", cx);
    });
    let view_slot = Rc::new(RefCell::new(None));
    let captured_view = Rc::clone(&view_slot);
    let pane = PaneId(7);
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let controller =
            cx.new(|cx| BrowserController::new(Err(BrowserError::AlreadyShutdown), cx));
        let mux = cx.new(|cx| {
            MuxClient::new(
                Err(DaemonError::Thread("test client".to_owned())),
                zz_daemon_client::default_socket_path(),
                cx,
            )
        });
        let view = cx.new(|cx| {
            BrowserView::new(
                pane,
                &BrowserDescriptor::single(
                    DEFAULT_URL.to_owned(),
                    ala_cef::DEFAULT_BROWSER_PROFILE.to_owned(),
                ),
                controller,
                mux,
                window,
                cx,
            )
        });
        captured_view.replace(Some(view.clone()));
        Root::new(view, window, cx)
    });
    let view = view_slot.borrow().clone().expect("captured browser view");
    let address = cx.update(|_, cx| view.read(cx).address.clone());

    cx.update(|window, cx| {
        assert!(!view.read(cx).address_editing);
        address.update(cx, |address, cx| {
            address.insert("exa", window, cx);
        });
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert!(view.address_editing);
        assert_eq!(view.omnibox.suggestions.len(), 1);
        assert_eq!(view.omnibox.suggestions[0].url, "https://example.com");
    });
}

#[cfg(not(target_os = "macos"))]
#[ala::test]
fn component_chrome_renders_and_navigation_preserves_active_address_edits(cx: &mut TestAppContext) {
    cx.update(zz_ui::init);
    let view_slot = Rc::new(RefCell::new(None));
    let controller_slot = Rc::new(RefCell::new(None));
    let captured_view = Rc::clone(&view_slot);
    let captured_controller = Rc::clone(&controller_slot);
    let pane = PaneId(7);
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let controller =
            cx.new(|cx| BrowserController::new(Err(BrowserError::AlreadyShutdown), cx));
        let mux = cx.new(|cx| {
            MuxClient::new(
                Err(DaemonError::Thread("test client".to_owned())),
                zz_daemon_client::default_socket_path(),
                cx,
            )
        });
        let view = cx.new(|cx| {
            let descriptor = zz_protocol::BrowserDescriptor {
                tabs: vec!["example.com".to_owned()],
                active_tab: 0,
                profile: "default".to_owned(),
            };
            BrowserView::new(pane, &descriptor, controller.clone(), mux, window, cx)
        });
        view.update(cx, |view, _| view.error = None);
        captured_view.replace(Some(view.clone()));
        captured_controller.replace(Some(controller));
        Root::new(view, window, cx)
    });
    let cx: &mut VisualTestContext = cx;
    let view = view_slot.borrow().clone().expect("captured browser view");
    let controller = controller_slot
        .borrow()
        .clone()
        .expect("captured browser controller");

    assert_eq!(
        cx.update(|_, cx| view.read(cx).address.read(cx).value().to_string()),
        "https://example.com/"
    );

    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.address_editing = true;
            view.address.update(cx, |address, cx| {
                address.set_value("typing.example/path", window, cx);
            });
            view.handle_controller_event(
                &controller,
                &ControllerEvent::Browser {
                    pane,
                    tab: TAB_ID,
                    event: BrowserEvent::AddressChanged {
                        session: SessionId(1),
                        url: Arc::from("https://redirect.example"),
                    },
                },
                window,
                cx,
            );
        });
    });
    assert_eq!(
        cx.update(|_, cx| view.read(cx).address.read(cx).value().to_string()),
        "typing.example/path"
    );

    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.address_editing = false;
            view.handle_controller_event(
                &controller,
                &ControllerEvent::Browser {
                    pane,
                    tab: TAB_ID,
                    event: BrowserEvent::AddressChanged {
                        session: SessionId(1),
                        url: Arc::from("https://settled.example"),
                    },
                },
                window,
                cx,
            );
        });
        let _ = window.draw(cx);
    });
    cx.run_until_parked();
    assert_eq!(
        cx.update(|_, cx| view.read(cx).address.read(cx).value().to_string()),
        "https://settled.example"
    );
}
