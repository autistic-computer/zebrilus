use super::*;
use ala::{AppContext as _, TestAppContext};
use std::{cell::RefCell, rc::Rc};

fn pending_browser(viewport: Viewport) -> PendingBrowser {
    PendingBrowser {
        url: "about:blank".to_owned(),
        profile: "default".to_owned(),
        egress: None,
        viewport,
        page_zoom_factor: 1.0,
        gpu_context: None,
        allow_shared_texture: true,
        background: None,
    }
}

fn test_egress_spec(socks_port: u16) -> EgressSpec {
    EgressSpec {
        composite_profile: "default@egress-0badc0de".to_owned(),
        egress_host: "build.internal".to_owned(),
        socks_port,
    }
}

#[ala::test]
fn browser_egress_records_the_route_until_its_tabs_close(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let controller =
            cx.new(|cx| BrowserController::new(Err(BrowserError::AlreadyShutdown), cx));
        let pane = PaneId(1);
        let other_pane = PaneId(2);
        let first = TabId(1);
        let second = TabId(2);
        let pending = TabId(3);

        controller.update(cx, |controller, _| {
            for key in [(pane, first), (pane, second), (other_pane, first)] {
                controller
                    .browser_egress
                    .insert(key, test_egress_spec(41_080));
            }
            controller.pending_browsers.insert(
                (pane, pending),
                PendingBrowser {
                    egress: Some(test_egress_spec(41_080)),
                    ..pending_browser(Viewport::default())
                },
            );

            controller.refresh_egress(pane, Some(test_egress_spec(41_081)));
            assert_eq!(
                controller.pending_browsers[&(pane, pending)]
                    .egress
                    .as_ref()
                    .expect("the pending request kept its route")
                    .socks_port,
                41_081,
            );
            assert_eq!(controller.browser_egress[&(pane, first)].socks_port, 41_081);
            assert_eq!(
                controller.browser_egress[&(pane, second)].socks_port,
                41_081
            );
            assert_eq!(
                controller.browser_egress[&(other_pane, first)].socks_port,
                41_080,
            );

            controller.refresh_egress(pane, None);
            controller.refresh_egress(
                pane,
                Some(EgressSpec {
                    composite_profile: "other@egress-0badc0de".to_owned(),
                    ..test_egress_spec(41_082)
                }),
            );
            assert_eq!(controller.browser_egress[&(pane, first)].socks_port, 41_081);

            controller.close_tab(pane, second);
            assert!(!controller.browser_egress.contains_key(&(pane, second)));
            assert!(controller.browser_egress.contains_key(&(pane, first)));

            controller.close_pane(pane);
            assert!(
                controller
                    .browser_egress
                    .range(browser_key_range(pane))
                    .next()
                    .is_none()
            );
            assert!(controller.browser_egress.contains_key(&(other_pane, first)));
        });
    });
}

#[ala::test]
fn active_tab_flips_pending_session_visibility(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let controller =
            cx.new(|cx| BrowserController::new(Err(BrowserError::AlreadyShutdown), cx));
        let pane = PaneId(7);
        let first = TabId(3);
        let second = TabId(9);
        let visible = Viewport {
            width: 800,
            height: 500,
            visible: true,
            ..Viewport::default()
        };

        controller.update(cx, |controller, cx| {
            controller.active_tabs.insert(pane, first);
            controller.pane_viewports.insert(pane, visible);
            controller
                .pending_browsers
                .insert((pane, first), pending_browser(visible));
            controller.pending_browsers.insert(
                (pane, second),
                pending_browser(Viewport {
                    visible: false,
                    ..visible
                }),
            );

            controller.set_active_tab(pane, second, cx);

            assert_eq!(controller.active_tab(pane), Some(second));
            assert!(!controller.pending_browsers[&(pane, first)].viewport.visible);
            assert!(
                controller.pending_browsers[&(pane, second)]
                    .viewport
                    .visible
            );
        });
    });
}

#[ala::test]
fn close_tab_and_close_pane_keep_other_tab_keys_isolated(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let controller =
            cx.new(|cx| BrowserController::new(Err(BrowserError::AlreadyShutdown), cx));
        let pane = PaneId(7);
        let other_pane = PaneId(8);
        let first = TabId(1);
        let active = TabId(2);
        let orphaned_bookkeeping = TabId(3);
        let other = TabId(1);
        let viewport = Viewport::default();

        controller.update(cx, |controller, _| {
            for key in [(pane, first), (pane, active), (other_pane, other)] {
                controller
                    .pending_browsers
                    .insert(key, pending_browser(viewport));
                controller.forced_readback.insert(key);
            }
            controller.next_external_begin_frame.insert(
                (pane, orphaned_bookkeeping),
                ExternalBeginFrameDeadline::anchored(
                    Instant::now(),
                    Duration::from_millis(10),
                    false,
                ),
            );
            controller.first_frame_watchdogs.insert(
                (pane, orphaned_bookkeeping),
                FirstFrameWatchdog::new(SessionId(41)),
            );
            controller.active_tabs.insert(pane, active);
            controller.active_tabs.insert(other_pane, other);
            controller.pane_viewports.insert(pane, viewport);
            controller.pane_viewports.insert(other_pane, viewport);

            assert_eq!(
                controller
                    .pending_browsers
                    .range(browser_key_range(pane))
                    .count(),
                2,
            );

            controller.close_tab(pane, first);
            assert!(!controller.pending_browsers.contains_key(&(pane, first)));
            assert!(!controller.forced_readback.contains(&(pane, first)));
            assert!(controller.pending_browsers.contains_key(&(pane, active)));
            assert!(
                controller
                    .pending_browsers
                    .contains_key(&(other_pane, other))
            );
            assert_eq!(controller.active_tab(pane), Some(active));

            controller.close_pane(pane);
            assert!(
                controller
                    .pending_browsers
                    .range(browser_key_range(pane))
                    .next()
                    .is_none()
            );
            assert!(
                controller
                    .forced_readback
                    .range(browser_key_range(pane))
                    .next()
                    .is_none()
            );
            assert!(
                controller
                    .next_external_begin_frame
                    .range(browser_key_range(pane))
                    .next()
                    .is_none()
            );
            assert!(
                controller
                    .first_frame_watchdogs
                    .range(browser_key_range(pane))
                    .next()
                    .is_none()
            );
            assert_eq!(controller.active_tab(pane), None);
            assert!(!controller.pane_viewports.contains_key(&pane));

            assert!(
                controller
                    .pending_browsers
                    .contains_key(&(other_pane, other))
            );
            assert!(controller.forced_readback.contains(&(other_pane, other)));
            assert_eq!(controller.active_tab(other_pane), Some(other));
            assert!(controller.pane_viewports.contains_key(&other_pane));
        });
    });
}

#[ala::test]
fn cookie_import_queues_while_session_is_initializing_and_clears_on_close(cx: &mut TestAppContext) {
    let pane = PaneId(11);
    let tab = TabId(1);
    let controller =
        cx.update(|cx| cx.new(|cx| BrowserController::new(Err(BrowserError::AlreadyShutdown), cx)));
    let failed = Rc::new(RefCell::new(Vec::new()));
    let observed = Rc::clone(&failed);
    cx.update(|cx| {
        cx.subscribe(&controller, move |_, event, _| {
            if let ControllerEvent::BrowserDataFailed { message, .. } = event {
                observed.borrow_mut().push(message.to_string());
            }
        })
        .detach();
    });
    let batch = ala_cef::parse_cookie_import(
        r#"[{"name":"sid","value":"abc","domain":"example.com","path":"/"}]"#,
    )
    .expect("valid cookie batch");
    controller.update(cx, |controller, cx| {
        controller.detached_runtime_phase = Some(RuntimePhase::Initializing);
        controller.import_cookies(pane, tab, batch, cx);
        assert_eq!(
            controller
                .pending_cookie_imports
                .get(&(pane, tab))
                .map(Vec::len),
            Some(1)
        );
        controller.close_tab(pane, tab);
        assert!(!controller.pending_cookie_imports.contains_key(&(pane, tab)));
    });
    cx.run_until_parked();
    assert!(failed.borrow().is_empty());
}
