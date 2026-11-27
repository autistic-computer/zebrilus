use super::test_support::{allocator, three_horizontal};
use super::*;

fn projected_panes(node: &LayoutNode, panes: &mut Vec<PaneId>) {
    match node {
        LayoutNode::Pane(pane) => panes.push(*pane),
        LayoutNode::Empty => {}
        LayoutNode::Split { first, second, .. } => {
            projected_panes(first, panes);
            projected_panes(second, panes);
        }
    }
}

#[test]
fn projection_is_right_associative_and_keeps_dividers_stable() {
    let mut layout = three_horizontal();
    let projected = layout.project();
    let LayoutNode::Split {
        id, ratio, second, ..
    } = &projected
    else {
        panic!("expected projected split");
    };
    assert_eq!(*id, SplitId(1));
    assert!((*ratio - 40.0 / 79.0).abs() < f32::EPSILON);
    let LayoutNode::Split {
        id: nested_id,
        ratio: nested_ratio,
        ..
    } = second.as_ref()
    else {
        panic!("expected nested projected split");
    };
    assert_eq!(*nested_id, SplitId(2));
    assert_eq!(*nested_ratio, 0.5);
    let mut projected_order = Vec::new();
    projected_panes(&projected, &mut projected_order);
    assert_eq!(projected_order, layout.panes_in_order());

    layout.resize_pane(PaneId(0), Axis::Horizontal, 5).unwrap();
    let LayoutNode::Split { id: after, .. } = layout.project() else {
        panic!("expected projected split");
    };
    assert_eq!(after, SplitId(1));
}

#[test]
fn unrelated_split_keeps_the_existing_divider_id() {
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
        .split(
            PaneId(0),
            Axis::Horizontal,
            SplitSize::Default,
            false,
            false,
            PaneId(2),
            &mut ids,
        )
        .unwrap();
    let LayoutNode::Split { id, second, .. } = layout.project() else {
        panic!("expected projected split");
    };
    assert_eq!(id, SplitId(2));
    let LayoutNode::Split { id, .. } = second.as_ref() else {
        panic!("expected nested projected split");
    };
    assert_eq!(*id, SplitId(1));
}

#[test]
fn remap_is_simultaneous() {
    let mut layout = three_horizontal();
    let mapping = BTreeMap::from([
        (PaneId(0), PaneId(1)),
        (PaneId(1), PaneId(2)),
        (PaneId(2), PaneId(0)),
    ]);
    layout.remap(&mapping);
    assert_eq!(layout.panes_in_order(), [PaneId(1), PaneId(2), PaneId(0)]);
}

#[test]
fn pane_count_counts_every_leaf() {
    assert_eq!(CellLayout::new(PaneId(0), 80, 24).pane_count(), 1);
    assert_eq!(three_horizontal().pane_count(), 3);
}

#[test]
fn replace_panes_in_order_is_atomic_on_count_mismatch() {
    let mut layout = three_horizontal();
    assert!(!layout.replace_panes_in_order(&[PaneId(7), PaneId(8)]));
    assert_eq!(layout.panes_in_order(), [PaneId(0), PaneId(1), PaneId(2)]);
    assert!(layout.replace_panes_in_order(&[PaneId(7), PaneId(8), PaneId(9)]));
    assert_eq!(layout.panes_in_order(), [PaneId(7), PaneId(8), PaneId(9)]);
}

#[test]
fn refresh_divider_ids_replaces_every_projected_id() {
    let mut layout = three_horizontal();
    let mut ids = allocator(10);
    layout.refresh_divider_ids(&mut ids);
    let LayoutNode::Split { id, second, .. } = layout.project() else {
        panic!("expected projected split");
    };
    assert_eq!(id, SplitId(10));
    let LayoutNode::Split { id, .. } = second.as_ref() else {
        panic!("expected nested projected split");
    };
    assert_eq!(*id, SplitId(11));

    let mut nested = CellLayout::new(PaneId(0), 80, 24);
    let mut build = allocator(1);
    nested
        .split(
            PaneId(0),
            Axis::Horizontal,
            SplitSize::Default,
            false,
            false,
            PaneId(1),
            &mut build,
        )
        .unwrap();
    nested
        .split(
            PaneId(1),
            Axis::Vertical,
            SplitSize::Default,
            false,
            false,
            PaneId(2),
            &mut build,
        )
        .unwrap();
    let mut fresh = allocator(20);
    nested.refresh_divider_ids(&mut fresh);
    let LayoutNode::Split { id, second, .. } = nested.project() else {
        panic!("expected projected split");
    };
    assert_eq!(id, SplitId(20));
    let LayoutNode::Split { id, .. } = second.as_ref() else {
        panic!("expected nested projected split");
    };
    assert_eq!(*id, SplitId(21));
}
