use super::*;
use ala::{AppContext as _, TestAppContext};
use std::{cell::RefCell, rc::Rc};

#[ala::test(iterations = 20)]
fn cef_work_runs_after_the_app_update_in_submission_order(cx: &mut TestAppContext) {
    let calls = Rc::new(RefCell::new(Vec::new()));
    let app = Rc::clone(&cx.app);
    let controller =
        cx.update(|cx| cx.new(|cx| BrowserController::new(Err(BrowserError::AlreadyShutdown), cx)));

    cx.update(|cx| {
        assert!(app.try_borrow_mut().is_err());
        let held = calls.borrow_mut();
        for call in [1, 2] {
            let calls = Rc::clone(&calls);
            let app = Rc::clone(&app);
            controller.update(cx, |controller, cx| {
                controller.queue_cef_work(
                    move || {
                        assert!(app.try_borrow_mut().is_ok());
                        calls.borrow_mut().push(call);
                    },
                    cx,
                );
            });
        }
        assert!(held.is_empty());
    });

    cx.run_until_parked();
    assert_eq!(*calls.borrow(), vec![1, 2]);
}

#[ala::test]
fn pump_timer_replacement_preserves_the_new_deadline(cx: &mut TestAppContext) {
    let controller =
        cx.update(|cx| cx.new(|cx| BrowserController::new(Err(BrowserError::AlreadyShutdown), cx)));
    cx.update(|cx| {
        controller.update(cx, |controller, cx| {
            controller.arm_pump_timer(Duration::from_millis(10), cx);
        });
    });
    while cx.executor().tick() {}
    cx.update(|cx| {
        controller.update(cx, |controller, cx| {
            controller.arm_pump_timer(Duration::from_millis(50), cx);
        });
    });
    while cx.executor().tick() {}
    cx.executor().advance_clock(Duration::from_millis(10));
    while cx.executor().tick() {}
    cx.update(|cx| {
        assert!(controller.read(cx).pump_task.is_some());
        assert!(controller.read(cx).pump_deadline.is_some());
    });
    cx.executor().advance_clock(Duration::from_millis(40));
    while cx.executor().tick() {}
    cx.update(|cx| {
        assert!(controller.read(cx).pump_task.is_none());
        assert!(controller.read(cx).pump_deadline.is_none());
        controller.update(cx, |controller, cx| {
            controller.arm_pump_timer(Duration::from_secs(1), cx);
            controller.arm_pump_timer(Duration::ZERO, cx);
        });
    });
    while cx.executor().tick() {}
    cx.update(|cx| assert!(controller.read(cx).pump_task.is_none()));
}

#[ala::test]
fn wheel_decay_replacement_extends_the_boost_and_focus_cancels_it(cx: &mut TestAppContext) {
    let key = (PaneId(7), TabId(3));
    let controller =
        cx.update(|cx| cx.new(|cx| BrowserController::new(Err(BrowserError::AlreadyShutdown), cx)));
    cx.update(|cx| {
        controller.update(cx, |controller, cx| {
            controller.active_tabs.insert(key.0, key.1);
            controller.schedule_wheel_decay(key, cx);
        });
    });
    while cx.executor().tick() {}
    cx.executor()
        .advance_clock(UNFOCUSED_SCROLL_FRAME_RATE_DECAY / 2);
    cx.update(|cx| {
        controller.update(cx, |controller, cx| {
            controller.schedule_wheel_decay(key, cx);
        });
    });
    while cx.executor().tick() {}
    cx.executor()
        .advance_clock(UNFOCUSED_SCROLL_FRAME_RATE_DECAY / 2);
    while cx.executor().tick() {}
    cx.update(|cx| assert!(controller.read(cx).wheel_decay_tasks.contains_key(&key)));
    cx.executor()
        .advance_clock(UNFOCUSED_SCROLL_FRAME_RATE_DECAY / 2);
    while cx.executor().tick() {}
    cx.update(|cx| {
        controller.update(cx, |controller, cx| {
            assert!(controller.wheel_decay_tasks.is_empty());
            controller.schedule_wheel_decay(key, cx);
            controller.set_focus(key.0, true);
            assert!(controller.wheel_decay_tasks.is_empty());
            controller.schedule_wheel_decay(key, cx);
            controller.close_pane(key.0);
            assert!(controller.wheel_decay_tasks.is_empty());
        });
    });
}

#[ala::test]
fn shutdown_cancels_scheduled_browser_timers(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let controller =
            cx.new(|cx| BrowserController::new(Err(BrowserError::AlreadyShutdown), cx));
        controller.update(cx, |controller, cx| {
            controller.arm_pump_timer(Duration::from_mins(1), cx);
            controller.schedule_wheel_decay((PaneId(7), TabId(3)), cx);
            let generation = controller.pump_generation;
            drop(controller.shutdown(cx));
            assert!(controller.pump_task.is_none());
            assert!(controller.pump_deadline.is_none());
            assert_ne!(controller.pump_generation, generation);
            assert!(controller.wheel_decay_tasks.is_empty());
        });
    });
}
