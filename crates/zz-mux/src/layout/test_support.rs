use super::*;

pub(super) fn allocator(start: u64) -> impl FnMut() -> SplitId {
    let mut next = start;
    move || {
        let id = SplitId(next);
        next += 1;
        id
    }
}

pub(super) fn geometry(layout: &CellLayout, pane: u64) -> CellGeometry {
    layout.pane_geometry(PaneId(pane)).unwrap()
}

pub(super) fn sizes(layout: &CellLayout, panes: &[PaneId]) -> Vec<(u16, u16)> {
    panes
        .iter()
        .map(|pane| {
            let geometry = layout.pane_geometry(*pane).unwrap();
            (geometry.sx, geometry.sy)
        })
        .collect()
}

pub(super) fn three_horizontal() -> CellLayout {
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
            PaneId(1),
            Axis::Horizontal,
            SplitSize::Default,
            false,
            false,
            PaneId(2),
            &mut ids,
        )
        .unwrap();
    layout
}
