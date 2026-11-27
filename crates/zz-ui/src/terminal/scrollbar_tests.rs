use super::*;

#[test]
fn scrollbar_thumb_hides_at_rest_and_fades_after_scrolling() {
    let start = web_time::Instant::now();
    let at = |millis| start + Duration::from_millis(millis);
    let mut fade = ScrollbarFade::default();
    let bottom = ScrollbarState {
        total: 100,
        offset: 76,
        len: 24,
    };
    assert_eq!(fade.frame(bottom, at(0), false), None);
    let followed = ScrollbarState {
        total: 101,
        offset: 77,
        len: 24,
    };
    assert_eq!(fade.frame(followed, at(10), false), None);
    let scrolled = ScrollbarState {
        offset: 40,
        ..followed
    };
    assert_eq!(
        fade.frame(scrolled, at(20), false),
        Some((1.0, Duration::from_secs(2)))
    );
    assert_eq!(
        fade.frame(scrolled, at(1020), false),
        Some((1.0, Duration::from_secs(1)))
    );
    assert_eq!(
        fade.frame(scrolled, at(2520), false),
        Some((1.0 - 1.0 / 1024.0, Duration::from_millis(33)))
    );
    assert_eq!(fade.frame(scrolled, at(3020), false), None);
    let pinned = ScrollbarState {
        total: 150,
        ..scrolled
    };
    assert_eq!(fade.frame(pinned, at(4000), false), None);
    fade.show(at(5000));
    assert_eq!(
        fade.frame(pinned, at(7500), true),
        Some((1.0, Duration::from_millis(500)))
    );
    fade.show(at(8000));
    let empty = ScrollbarState {
        total: 24,
        offset: 0,
        len: 24,
    };
    assert_eq!(fade.frame(empty, at(8000), false), None);
}

#[test]
fn terminal_drag_geometry_clamps_scrollbar_and_accelerates_selection_outside_grid() {
    let surface = Bounds::new(point(px(10.0), px(20.0)), size(px(200.0), px(160.0)));
    let scrollbar = ScrollbarState {
        offset: 0,
        len: 10,
        total: 100,
    };
    assert!(scrollbar_strip_hit(
        surface,
        scrollbar,
        point(px(208.0), px(30.0)),
        px(16.0)
    ));
    assert!(!scrollbar_strip_hit(
        surface,
        scrollbar,
        point(px(100.0), px(30.0)),
        px(16.0)
    ));
    assert_eq!(scroll_fraction(surface, point(px(20.0), px(-50.0))), 0);
    assert_eq!(
        scroll_fraction(surface, point(px(20.0), px(500.0))),
        u32::MAX
    );
    assert_eq!(
        selection_autoscroll_lines(surface, px(16.0), point(px(20.0), px(40.0))),
        0
    );
    assert_eq!(
        selection_autoscroll_lines(surface, px(16.0), point(px(20.0), px(19.0))),
        -1
    );
    assert_eq!(
        selection_autoscroll_lines(surface, px(16.0), point(px(20.0), px(181.0))),
        1
    );
    assert_eq!(
        selection_autoscroll_lines(surface, px(16.0), point(px(20.0), px(1000.0))),
        8
    );
}
