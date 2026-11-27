use super::test_support::{allocator, sizes};
use super::*;

#[test]
fn split_sizes_before_full_and_no_space_match_tmux() {
    let mut ids = allocator(1);
    let mut default = CellLayout::new(PaneId(0), 80, 24);
    default
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
    assert_eq!(
        sizes(&default, &[PaneId(0), PaneId(1)]),
        [(40, 24), (39, 24)]
    );

    let mut ids = allocator(1);
    let mut cells = CellLayout::new(PaneId(0), 80, 24);
    cells
        .split(
            PaneId(0),
            Axis::Horizontal,
            SplitSize::Cells(30),
            false,
            false,
            PaneId(1),
            &mut ids,
        )
        .unwrap();
    assert_eq!(sizes(&cells, &[PaneId(0), PaneId(1)]), [(49, 24), (30, 24)]);

    let mut ids = allocator(1);
    let mut percent = CellLayout::new(PaneId(0), 80, 24);
    percent
        .split(
            PaneId(0),
            Axis::Horizontal,
            SplitSize::Percent(25),
            false,
            false,
            PaneId(1),
            &mut ids,
        )
        .unwrap();
    assert_eq!(
        sizes(&percent, &[PaneId(0), PaneId(1)]),
        [(59, 24), (20, 24)]
    );

    let mut ids = allocator(1);
    let mut before = CellLayout::new(PaneId(0), 80, 24);
    before
        .split(
            PaneId(0),
            Axis::Horizontal,
            SplitSize::Default,
            true,
            false,
            PaneId(1),
            &mut ids,
        )
        .unwrap();
    assert_eq!(before.panes_in_order(), [PaneId(1), PaneId(0)]);
    assert_eq!(
        sizes(&before, &[PaneId(1), PaneId(0)]),
        [(40, 24), (39, 24)]
    );

    let mut ids = allocator(1);
    let mut full = CellLayout::new(PaneId(0), 80, 24);
    full.split(
        PaneId(0),
        Axis::Horizontal,
        SplitSize::Default,
        false,
        false,
        PaneId(1),
        &mut ids,
    )
    .unwrap();
    full.split(
        PaneId(0),
        Axis::Vertical,
        SplitSize::Default,
        false,
        true,
        PaneId(2),
        &mut ids,
    )
    .unwrap();
    assert_eq!(
        full.dump(),
        "eb1f,80x24,0,0[80x12,0,0{40x12,0,0,0,39x12,41,0,1},80x11,0,13,2]"
    );

    let mut ids = allocator(1);
    let mut same_axis_full = CellLayout::new(PaneId(0), 80, 24);
    same_axis_full
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
    same_axis_full
        .split(
            PaneId(0),
            Axis::Horizontal,
            SplitSize::Default,
            false,
            true,
            PaneId(2),
            &mut ids,
        )
        .unwrap();
    assert_eq!(
        sizes(&same_axis_full, &[PaneId(0), PaneId(1), PaneId(2)]),
        [(20, 24), (19, 24), (39, 24)]
    );

    let mut ids = allocator(1);
    let mut nested_full = CellLayout::new(PaneId(0), 80, 24);
    nested_full
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
    nested_full
        .resize_pane_to(PaneId(0), Axis::Horizontal, 60)
        .unwrap();
    nested_full
        .split(
            PaneId(0),
            Axis::Vertical,
            SplitSize::Default,
            false,
            true,
            PaneId(2),
            &mut ids,
        )
        .unwrap();
    nested_full
        .split(
            PaneId(2),
            Axis::Horizontal,
            SplitSize::Default,
            false,
            true,
            PaneId(3),
            &mut ids,
        )
        .unwrap();
    assert_eq!(
        sizes(&nested_full, &[PaneId(0), PaneId(1), PaneId(2), PaneId(3)]),
        [(30, 12), (9, 12), (40, 11), (39, 24)]
    );

    let mut ids = allocator(1);
    let mut tiny = CellLayout::new(PaneId(0), 2, 24);
    assert_eq!(
        tiny.split(
            PaneId(0),
            Axis::Horizontal,
            SplitSize::Default,
            false,
            false,
            PaneId(1),
            &mut ids,
        ),
        Err(LayoutError::NoSpace)
    );

    let mut ids = allocator(1);
    let mut over_percent = CellLayout::new(PaneId(0), 80, 24);
    over_percent
        .split(
            PaneId(0),
            Axis::Horizontal,
            SplitSize::Percent(200),
            false,
            false,
            PaneId(1),
            &mut ids,
        )
        .unwrap();
    assert_eq!(
        sizes(&over_percent, &[PaneId(0), PaneId(1)]),
        [(1, 24), (78, 24)]
    );

    let mut allocated = 0;
    let result = {
        let mut ids = || {
            allocated += 1;
            SplitId(allocated)
        };
        default.split(
            PaneId(99),
            Axis::Horizontal,
            SplitSize::Default,
            false,
            false,
            PaneId(2),
            &mut ids,
        )
    };
    assert_eq!(result, Err(LayoutError::UnknownPane));
    assert_eq!(allocated, 0);
}

#[test]
fn remove_gifts_after_and_preserves_promoted_same_axis_nodes() {
    let mut ids = allocator(1);
    let mut layout = CellLayout::new(PaneId(0), 80, 24);
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
    layout
        .split(
            PaneId(1),
            Axis::Vertical,
            SplitSize::Default,
            false,
            false,
            PaneId(2),
            &mut ids,
        )
        .unwrap();
    layout.remove(PaneId(0)).unwrap();
    assert_eq!(sizes(&layout, &[PaneId(1), PaneId(2)]), [(80, 18), (80, 5)]);

    let mut ids = allocator(1);
    let mut nested = CellLayout::new(PaneId(0), 80, 24);
    nested
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
    nested
        .split(
            PaneId(1),
            Axis::Vertical,
            SplitSize::Default,
            false,
            false,
            PaneId(2),
            &mut ids,
        )
        .unwrap();
    nested
        .split(
            PaneId(1),
            Axis::Horizontal,
            SplitSize::Default,
            false,
            false,
            PaneId(3),
            &mut ids,
        )
        .unwrap();
    nested.remove(PaneId(2)).unwrap();
    assert_eq!(nested.panes_in_order(), [PaneId(0), PaneId(1), PaneId(3)]);
    let CellNode::Node { axis, children, .. } = &nested.root else {
        panic!("expected root node");
    };
    assert_eq!(*axis, Axis::Horizontal);
    assert_eq!(children.len(), 2);
    assert_eq!(children[1].divider, Some(SplitId(1)));
    let CellNode::Node {
        axis: promoted_axis,
        children: promoted_children,
        ..
    } = &children[1].node
    else {
        panic!("expected promoted subtree");
    };
    assert_eq!(*promoted_axis, Axis::Horizontal);
    assert_eq!(promoted_children.len(), 2);
    assert!(nested.validate().is_ok());
}

#[test]
fn remove_reports_last_pane_and_guards_single_child_nodes() {
    let mut root_leaf = CellLayout::new(PaneId(0), 80, 24);
    assert_eq!(root_leaf.remove(PaneId(0)), Err(LayoutError::LastPane));

    let mut malformed = CellLayout {
        root: CellNode::Node {
            axis: Axis::Horizontal,
            geometry: CellGeometry {
                sx: 80,
                sy: 24,
                xoff: 0,
                yoff: 0,
            },
            children: vec![CellChild {
                divider: None,
                node: leaf(PaneId(0), 80, 24),
            }],
        },
        extent: (80, 24),
    };
    assert_eq!(malformed.remove(PaneId(0)), Ok(()));
    assert_eq!(
        malformed.resize_pane(PaneId(0), Axis::Horizontal, 1),
        Ok(())
    );
}
