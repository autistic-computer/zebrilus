use super::*;

fn wire_layout_panes(node: &LayoutNode) -> Vec<PaneId> {
    let mut panes = Vec::new();
    node.panes(&mut panes);
    panes
}

pub(super) fn layout_splits(layout: &CellLayout) -> Vec<SplitId> {
    let mut splits = Vec::new();
    layout.project().splits(&mut splits);
    splits
}

pub(super) fn layout_panes(layout: &CellLayout) -> Vec<PaneId> {
    layout.panes_in_order()
}

pub(super) fn pane_size(layout: &CellLayout, pane: PaneId) -> (u16, u16) {
    let geometry = layout.pane_geometry(pane).expect("layout contains pane");
    (geometry.sx, geometry.sy)
}

pub(super) fn same_projected_geometry(left: &CellLayout, right: &LayoutNode) -> bool {
    fn axes(node: &LayoutNode, output: &mut Vec<Axis>) {
        let LayoutNode::Split {
            axis,
            first,
            second,
            ..
        } = node
        else {
            return;
        };
        output.push(*axis);
        axes(first, output);
        axes(second, output);
    }

    let projected = left.project();
    let mut projected_axes = Vec::new();
    let mut predicted_axes = Vec::new();
    axes(&projected, &mut projected_axes);
    axes(right, &mut predicted_axes);
    let bounds = PaneRect {
        left: 0,
        top: 0,
        right: LAYOUT_COORDINATE_MAX,
        bottom: LAYOUT_COORDINATE_MAX,
    };
    let mut projected_rects = Vec::new();
    let mut predicted_rects = Vec::new();
    collect_pane_rects(&projected, bounds, &mut projected_rects);
    collect_pane_rects(right, bounds, &mut predicted_rects);
    let tolerance = LAYOUT_COORDINATE_MAX * 3 / 100;
    wire_layout_panes(&projected) == wire_layout_panes(right)
        && projected_axes == predicted_axes
        && projected_rects.len() == predicted_rects.len()
        && projected_rects.iter().all(|(pane, projected)| {
            predicted_rects.iter().any(|(candidate, predicted)| {
                pane == candidate
                    && projected.left.abs_diff(predicted.left) <= tolerance
                    && projected.top.abs_diff(predicted.top) <= tolerance
                    && projected.right.abs_diff(predicted.right) <= tolerance
                    && projected.bottom.abs_diff(predicted.bottom) <= tolerance
            })
        })
}
