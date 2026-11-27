use super::*;

pub(super) fn spread_node(node: &mut CellNode) -> SpreadCell {
    let CellNode::Node {
        axis,
        geometry,
        children,
    } = node
    else {
        return SpreadCell::Unchanged;
    };
    let leaves = children
        .iter()
        .filter(|child| matches!(child.node, CellNode::Leaf { .. }))
        .count();
    if leaves <= 1 {
        return SpreadCell::Unchanged;
    }
    if children
        .iter()
        .any(|child| matches!(child.node, CellNode::Node { .. }) && child.node.tiled())
    {
        return SpreadCell::Refused;
    }
    let count = leaves;
    let size = usize::from(geometry.extent(*axis));
    if size < count - 1 {
        return SpreadCell::Unchanged;
    }
    let each = (size - (count - 1)) / count;
    if each == 0 {
        return SpreadCell::Unchanged;
    }
    let mut remainder = size - (count - 1) - each * count;
    let mut changed = false;
    for child in children
        .iter_mut()
        .filter(|child| matches!(child.node, CellNode::Leaf { .. }))
    {
        let extra = usize::from(remainder > 0);
        remainder = remainder.saturating_sub(extra);
        let target = u16::try_from(each + extra).unwrap_or(u16::MAX);
        let change = i32::from(target) - i32::from(child.node.extent(*axis));
        resize_adjust(&mut child.node, *axis, change);
        changed |= change != 0;
    }
    if changed {
        SpreadCell::Changed
    } else {
        SpreadCell::Unchanged
    }
}

fn make_children(
    panes: &[PaneId],
    sx: u16,
    sy: u16,
    ids: &mut dyn FnMut() -> SplitId,
) -> Vec<CellChild> {
    let mut children = Vec::with_capacity(panes.len());
    for (index, pane) in panes.iter().enumerate() {
        children.push(CellChild {
            divider: if index == 0 { None } else { Some(ids()) },
            node: leaf(*pane, sx, sy),
        });
    }
    children
}

pub(super) fn even_layout(
    axis: Axis,
    sx: u16,
    sy: u16,
    panes: &[PaneId],
    ids: &mut dyn FnMut() -> SplitId,
) -> CellNode {
    let minimum =
        u16::try_from(panes.len().saturating_mul(2).saturating_sub(1)).unwrap_or(u16::MAX);
    let (layout_width, layout_height) = match axis {
        Axis::Horizontal => (sx.max(minimum), sy),
        Axis::Vertical => (sx, sy.max(minimum)),
    };
    let mut root = CellNode::Node {
        axis,
        geometry: CellGeometry {
            sx: layout_width,
            sy: layout_height,
            xoff: 0,
            yoff: 0,
        },
        children: make_children(panes, layout_width, layout_height, ids),
    };
    spread_node(&mut root);
    root
}

pub(super) fn main_horizontal(
    sx: u16,
    sy: u16,
    panes: &[PaneId],
    mirrored: bool,
    options: &PresetOptions,
    ids: &mut dyn FnMut() -> SplitId,
) -> CellNode {
    let others = panes.len() - 1;
    let available = sy.saturating_sub(1);
    let mut main = layout_option_cells(&options.main_pane_height, available).unwrap_or(24);
    let other;
    if main.saturating_add(PANE_MINIMUM) >= available {
        main = if available <= PANE_MINIMUM * 2 {
            PANE_MINIMUM
        } else {
            available - PANE_MINIMUM
        };
        other = PANE_MINIMUM;
    } else {
        let configured_other = layout_option_cells(&options.other_pane_height, available);
        if configured_other
            .is_none_or(|other| other == 0 || other > available || available - other < main)
        {
            other = available - main;
        } else {
            other = configured_other.expect("other pane height was checked");
            main = available - other;
        }
    }
    let minimum_width =
        u16::try_from(others.saturating_mul(2).saturating_sub(1)).unwrap_or(u16::MAX);
    let layout_width = sx.max(minimum_width);
    let layout_height = main + other + 1;
    let main_node = leaf(panes[0], layout_width, main);
    let other_node = if others == 1 {
        leaf(panes[1], layout_width, other)
    } else {
        let mut node = CellNode::Node {
            axis: Axis::Horizontal,
            geometry: CellGeometry {
                sx: layout_width,
                sy: other,
                xoff: 0,
                yoff: 0,
            },
            children: make_children(&panes[1..], PANE_MINIMUM, other, ids),
        };
        spread_node(&mut node);
        node
    };
    let divider = ids();
    let children = if mirrored {
        vec![
            CellChild {
                divider: None,
                node: other_node,
            },
            CellChild {
                divider: Some(divider),
                node: main_node,
            },
        ]
    } else {
        vec![
            CellChild {
                divider: None,
                node: main_node,
            },
            CellChild {
                divider: Some(divider),
                node: other_node,
            },
        ]
    };
    CellNode::Node {
        axis: Axis::Vertical,
        geometry: CellGeometry {
            sx: layout_width,
            sy: layout_height,
            xoff: 0,
            yoff: 0,
        },
        children,
    }
}

pub(super) fn main_vertical(
    sx: u16,
    sy: u16,
    panes: &[PaneId],
    mirrored: bool,
    options: &PresetOptions,
    ids: &mut dyn FnMut() -> SplitId,
) -> CellNode {
    let others = panes.len() - 1;
    let available = sx.saturating_sub(1);
    let mut main = layout_option_cells(&options.main_pane_width, available).unwrap_or(80);
    let other;
    if main.saturating_add(PANE_MINIMUM) >= available {
        main = if available <= PANE_MINIMUM * 2 {
            PANE_MINIMUM
        } else {
            available - PANE_MINIMUM
        };
        other = PANE_MINIMUM;
    } else {
        let configured_other = layout_option_cells(&options.other_pane_width, available);
        if configured_other
            .is_none_or(|other| other == 0 || other > available || available - other < main)
        {
            other = available - main;
        } else {
            other = configured_other.expect("other pane width was checked");
            main = available - other;
        }
    }
    let minimum_height =
        u16::try_from(others.saturating_mul(2).saturating_sub(1)).unwrap_or(u16::MAX);
    let layout_width = main + other + 1;
    let layout_height = sy.max(minimum_height);
    let main_node = leaf(panes[0], main, layout_height);
    let other_node = if others == 1 {
        leaf(panes[1], other, layout_height)
    } else {
        let mut node = CellNode::Node {
            axis: Axis::Vertical,
            geometry: CellGeometry {
                sx: other,
                sy: layout_height,
                xoff: 0,
                yoff: 0,
            },
            children: make_children(&panes[1..], other, PANE_MINIMUM, ids),
        };
        spread_node(&mut node);
        node
    };
    let divider = ids();
    let children = if mirrored {
        vec![
            CellChild {
                divider: None,
                node: other_node,
            },
            CellChild {
                divider: Some(divider),
                node: main_node,
            },
        ]
    } else {
        vec![
            CellChild {
                divider: None,
                node: main_node,
            },
            CellChild {
                divider: Some(divider),
                node: other_node,
            },
        ]
    };
    CellNode::Node {
        axis: Axis::Horizontal,
        geometry: CellGeometry {
            sx: layout_width,
            sy: layout_height,
            xoff: 0,
            yoff: 0,
        },
        children,
    }
}

pub(super) fn tiled_layout(
    sx: u16,
    sy: u16,
    panes: &[PaneId],
    max_columns: u16,
    ids: &mut dyn FnMut() -> SplitId,
) -> CellNode {
    let count = panes.len();
    let mut rows = 1_usize;
    let mut columns = 1_usize;
    while rows * columns < count {
        rows += 1;
        if rows * columns < count && (max_columns == 0 || columns < usize::from(max_columns)) {
            columns += 1;
        }
    }
    let width = usize::from(sx)
        .saturating_sub(columns - 1)
        .checked_div(columns)
        .unwrap_or(0)
        .max(usize::from(PANE_MINIMUM));
    let height = usize::from(sy)
        .saturating_sub(rows - 1)
        .checked_div(rows)
        .unwrap_or(0)
        .max(usize::from(PANE_MINIMUM));
    let layout_width = usize::from(sx).max((width + 1) * columns - 1);
    let layout_height = usize::from(sy).max((height + 1) * rows - 1);
    let layout_width = u16::try_from(layout_width).unwrap_or(u16::MAX);
    let layout_height = u16::try_from(layout_height).unwrap_or(u16::MAX);
    let width = u16::try_from(width).unwrap_or(u16::MAX);
    let height = u16::try_from(height).unwrap_or(u16::MAX);
    let mut row_children = Vec::new();
    let mut start = 0;
    while start < count {
        let end = (start + columns).min(count);
        let row_panes = &panes[start..end];
        let row_node = if row_panes.len() == 1 || columns == 1 {
            leaf(row_panes[0], layout_width, height)
        } else {
            let mut children = make_children(row_panes, width, height, ids);
            let used = row_panes.len() * (usize::from(width) + 1) - 1;
            if usize::from(layout_width) > used {
                let extra = u16::try_from(usize::from(layout_width) - used).unwrap_or(u16::MAX);
                if let Some(last) = children.last_mut() {
                    resize_adjust(&mut last.node, Axis::Horizontal, i32::from(extra));
                }
            }
            CellNode::Node {
                axis: Axis::Horizontal,
                geometry: CellGeometry {
                    sx: layout_width,
                    sy: height,
                    xoff: 0,
                    yoff: 0,
                },
                children,
            }
        };
        let divider = if row_children.is_empty() {
            None
        } else {
            Some(ids())
        };
        row_children.push(CellChild {
            divider,
            node: row_node,
        });
        start = end;
    }
    let used = row_children.len() * usize::from(height) + row_children.len() - 1;
    if usize::from(layout_height) > used {
        let extra = u16::try_from(usize::from(layout_height) - used).unwrap_or(u16::MAX);
        if let Some(last) = row_children.last_mut() {
            resize_adjust(&mut last.node, Axis::Vertical, i32::from(extra));
        }
    }
    CellNode::Node {
        axis: Axis::Vertical,
        geometry: CellGeometry {
            sx: layout_width,
            sy: layout_height,
            xoff: 0,
            yoff: 0,
        },
        children: row_children,
    }
}

fn layout_option_cells(value: &str, available: u16) -> Option<u16> {
    let cells = if let Some(percentage) = value.strip_suffix('%') {
        let percentage = percentage.parse::<u16>().ok()?;
        if percentage > 1000 {
            return None;
        }
        u32::from(available) * u32::from(percentage) / 100
    } else {
        value.parse::<u32>().ok()?
    };
    (cells <= u32::from(available)).then(|| u16::try_from(cells).expect("cells fit available"))
}
