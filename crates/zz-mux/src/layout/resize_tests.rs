use super::test_support::{allocator, sizes, three_horizontal};
use super::*;

#[test]
fn window_resize_uses_round_robin_with_first_remainders() {
    let mut grown = three_horizontal();
    grown.resize(100, 30);
    assert_eq!(
        sizes(&grown, &[PaneId(0), PaneId(1), PaneId(2)]),
        [(47, 30), (26, 30), (25, 30)]
    );

    let mut shrunk = three_horizontal();
    shrunk.resize(50, 20);
    assert_eq!(
        sizes(&shrunk, &[PaneId(0), PaneId(1), PaneId(2)]),
        [(30, 20), (9, 20), (9, 20)]
    );
}

#[test]
fn pane_resize_walks_victims_steps_back_and_clamps() {
    let mut walked = three_horizontal();
    walked
        .resize_pane(PaneId(1), Axis::Horizontal, -18)
        .unwrap();
    walked.resize_pane(PaneId(0), Axis::Horizontal, 5).unwrap();
    assert_eq!(
        sizes(&walked, &[PaneId(0), PaneId(1), PaneId(2)]),
        [(45, 24), (1, 24), (32, 24)]
    );
    walked.resize_pane(PaneId(2), Axis::Horizontal, 5).unwrap();
    assert_eq!(
        sizes(&walked, &[PaneId(0), PaneId(1), PaneId(2)]),
        [(45, 24), (6, 24), (27, 24)]
    );

    let mut clamped = CellLayout::new(PaneId(0), 80, 24);
    let mut ids = allocator(1);
    clamped
        .split(
            PaneId(0),
            Axis::Horizontal,
            SplitSize::Default,
            false,
            false,
            PaneId(1),
            &mut ids,
        )
        .unwrap();
    clamped
        .resize_pane(PaneId(0), Axis::Horizontal, -100)
        .unwrap();
    assert_eq!(
        sizes(&clamped, &[PaneId(0), PaneId(1)]),
        [(1, 24), (78, 24)]
    );
    clamped
        .resize_pane(PaneId(0), Axis::Horizontal, 100)
        .unwrap();
    assert_eq!(
        sizes(&clamped, &[PaneId(0), PaneId(1)]),
        [(78, 24), (1, 24)]
    );
}

#[test]
fn pane_resize_without_a_matching_ancestor_is_a_silent_noop() {
    let mut layout = CellLayout::new(PaneId(0), 80, 24);
    let mut ids = allocator(1);
    layout
        .split(
            PaneId(0),
            Axis::Vertical,
            SplitSize::Default,
            false,
            false,
            PaneId(1),
            &mut ids,
        )
        .unwrap();
    let before = layout.dump();

    assert_eq!(layout.resize_pane(PaneId(0), Axis::Horizontal, 10), Ok(()));
    assert_eq!(layout.dump(), before);
}

#[test]
fn pane_resize_to_inverts_from_last_child() {
    let mut layout = CellLayout::new(PaneId(0), 80, 24);
    let mut ids = allocator(1);
    layout
        .split(
            PaneId(0),
            Axis::Horizontal,
            SplitSize::Default,
            false,
            false,
            PaneId(1),
            &mut ids,
        )
        .unwrap();
    layout
        .resize_pane_to(PaneId(1), Axis::Horizontal, 30)
        .unwrap();
    assert_eq!(
        sizes(&layout, &[PaneId(0), PaneId(1)]),
        [(49, 24), (30, 24)]
    );
}

#[test]
fn divider_ratio_resizes_the_matching_boundary() {
    let mut layout = three_horizontal();
    assert!(layout.set_divider_ratio(SplitId(2), 0.75).unwrap());
    assert_eq!(
        sizes(&layout, &[PaneId(0), PaneId(1), PaneId(2)]),
        [(40, 24), (29, 24), (9, 24)]
    );
    assert!(!layout.set_divider_ratio(SplitId(2), 0.75).unwrap());
    assert_eq!(
        layout.set_divider_ratio(SplitId(99), 0.5),
        Err(LayoutError::UnknownDivider)
    );

    let mut inner = three_horizontal();
    assert!(!inner.set_divider_ratio(SplitId(1), 0.5).unwrap());
    assert_eq!(
        sizes(&inner, &[PaneId(0), PaneId(1), PaneId(2)]),
        [(40, 24), (19, 24), (19, 24)]
    );
    assert!(inner.set_divider_ratio(SplitId(1), 0.25).unwrap());
    assert_eq!(
        sizes(&inner, &[PaneId(0), PaneId(1), PaneId(2)]),
        [(20, 24), (39, 24), (19, 24)]
    );

    let mut walked = three_horizontal();
    walked
        .resize_pane(PaneId(1), Axis::Horizontal, -18)
        .unwrap();
    assert!(walked.set_divider_ratio(SplitId(2), 0.0).unwrap());
    assert_eq!(
        sizes(&walked, &[PaneId(0), PaneId(1), PaneId(2)]),
        [(39, 24), (1, 24), (38, 24)]
    );
}
