#![cfg(all(target_os = "linux", feature = "ladybird", not(feature = "chromium")))]

use std::time::{Duration, Instant};
use zz_browser_runtime::{
    BrowserBootstrap, BrowserEvent, BrowserProfilePaths, BrowserSession, Modifiers, PointerButton,
    PointerEvent, PointerPhase, RuntimeConfig, Viewport, WheelEvent, bootstrap_with_profile_paths,
};

fn wait_for_title(session: &BrowserSession, expected: &str) {
    let events = session.events();
    let until = Instant::now() + Duration::from_secs(15);
    while Instant::now() < until {
        while let Ok(event) = events.try_recv() {
            if let BrowserEvent::TitleChanged { title, .. } = event
                && &*title == expected
            {
                return;
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("Did not receive page title {expected}");
}

#[test]
#[ignore = "requires the pinned Linux Hub in ZZ_LADYBIRD_HUB"]
fn page_handles_mouse_wheel_and_history() {
    let temp = tempfile::tempdir().unwrap();
    let page = temp.path().join("page.html");
    std::fs::write(&page, r#"<style>*{margin:0}button{width:100px;height:80px}body{height:2000px}</style><button onclick="document.title='clicked'">Click</button><script>document.title='ready';onscroll=()=>document.title='scrolled'</script>"#).unwrap();
    let url = format!("file://{}", page.display());
    let BrowserBootstrap::Runtime(mut runtime) = bootstrap_with_profile_paths(
        &RuntimeConfig::default(),
        BrowserProfilePaths::under(temp.path(), "zz-default", "zz-"),
    )
    .unwrap() else {
        panic!("Expected a browser runtime")
    };
    runtime.start().unwrap();
    runtime.handle_context_initialized().unwrap();
    runtime
        .handle_request_context_initialized("default")
        .unwrap();
    let viewport = Viewport {
        width: 400,
        height: 300,
        scale_factor: 1.,
        visible: true,
        ..Viewport::default()
    };
    let session = runtime
        .create_session("default", &url, viewport, 1., None, None, false)
        .unwrap();
    wait_for_title(&session, "ready");
    session.set_focus(true);
    for phase in [PointerPhase::Move, PointerPhase::Down, PointerPhase::Up] {
        session.send_pointer(PointerEvent {
            x: 50,
            y: 40,
            phase,
            button: Some(PointerButton::Left),
            click_count: 1,
            modifiers: Modifiers::default(),
        });
    }
    wait_for_title(&session, "clicked");
    session.send_wheel(WheelEvent {
        x: 150,
        y: 100,
        delta_x: 0,
        delta_y: -90,
        precise: true,
        modifiers: Modifiers::default(),
    });
    wait_for_title(&session, "scrolled");
    let large = format!("data:text/html,<title>second</title>{}", "x".repeat(70000));
    session.navigate(&large);
    wait_for_title(&session, "second");
    session.go_back();
    wait_for_title(&session, "ready");
    session.go_forward();
    wait_for_title(&session, "second");
    session
        .navigate("data:text/html,<script>document.title=prompt('Pick a word','initial')</script>");
    let events = session.events();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        assert!(Instant::now() < deadline, "Expected a prompt");
        if let Ok(BrowserEvent::JsDialogRequested { request, .. }) = events.try_recv() {
            assert_eq!(request.message.as_ref(), "Pick a word");
            assert_eq!(request.default_prompt.as_deref(), Some("initial"));
            session.respond_js_dialog(request.id, true, Some("answered"));
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    wait_for_title(&session, "answered");
    drop(session);
    runtime.shutdown().unwrap();
}
