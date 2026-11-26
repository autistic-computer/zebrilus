use super::test_support::{layout_panes, layout_splits, pane_size};
use super::*;

fn split_ratio(layout: &CellLayout, split: SplitId) -> Option<f32> {
    fn find(node: &LayoutNode, split: SplitId) -> Option<f32> {
        let LayoutNode::Split {
            id,
            ratio,
            first,
            second,
            ..
        } = node
        else {
            return None;
        };
        (*id == split)
            .then_some(*ratio)
            .or_else(|| find(first, split))
            .or_else(|| find(second, split))
    }

    find(&layout.project(), split)
}

fn same_layout_geometry(left: &CellLayout, right: &CellLayout) -> bool {
    left.dump() == right.dump()
}

#[test]
fn split_ids_are_stable_and_exact_resize_targets_nested_boundaries() {
    let mut state = MuxState::default();
    let (_, window, first) = state.create_session("work").unwrap();
    let second = state
        .split_pane(first, Axis::Horizontal, PaneKind::Terminal)
        .unwrap();
    let third = state
        .split_pane(second, Axis::Horizontal, PaneKind::Terminal)
        .unwrap();

    let layout = &state.windows[&window].layout;
    assert_eq!(
        [
            pane_size(layout, first),
            pane_size(layout, second),
            pane_size(layout, third)
        ],
        [(40, 24), (19, 24), (19, 24)]
    );
    assert!((split_ratio(layout, SplitId(0)).unwrap() - 40.0 / 79.0).abs() < 1e-6);
    assert_eq!(split_ratio(layout, SplitId(1)), Some(0.5));
    assert_eq!(
        state.snapshot().sessions[0].windows[0].layout,
        layout.project()
    );

    state.resize_pane(second, Axis::Horizontal, 1).unwrap();
    let layout = &state.windows[&window].layout;
    assert_eq!(
        [
            pane_size(layout, first),
            pane_size(layout, second),
            pane_size(layout, third)
        ],
        [(40, 24), (20, 24), (18, 24)]
    );
    assert!((split_ratio(layout, SplitId(0)).unwrap() - 40.0 / 79.0).abs() < 1e-6);
    assert!((split_ratio(layout, SplitId(1)).unwrap() - 20.0 / 38.0).abs() < 1e-6);

    assert!(state.resize_split(window, SplitId(0), 0.72).unwrap());
    assert!(!state.resize_split(window, SplitId(0), 0.72).unwrap());
    let layout = &state.windows[&window].layout;
    assert_eq!(
        [
            pane_size(layout, first),
            pane_size(layout, second),
            pane_size(layout, third)
        ],
        [(57, 24), (3, 24), (18, 24)]
    );
    assert!((split_ratio(layout, SplitId(0)).unwrap() - 57.0 / 79.0).abs() < 1e-6);
    assert!((split_ratio(layout, SplitId(1)).unwrap() - 3.0 / 21.0).abs() < 1e-6);
    assert!(matches!(
        state.resize_split(window, SplitId(0), f32::NAN),
        Err(ServerError::InvalidCommand(message)) if message.contains("finite")
    ));

    state.kill_pane(third).unwrap();
    state
        .split_pane(first, Axis::Vertical, PaneKind::Terminal)
        .unwrap();
    let splits = layout_splits(&state.windows[&window].layout);
    assert!(splits.contains(&SplitId(2)));
    assert!(!splits.contains(&SplitId(1)));
    assert!(state.validate().is_ok());
}

#[test]
fn named_layouts_rebuild_only_the_cells_with_tmux_geometry() {
    let mut state = MuxState::default();
    let (_, window, first) = state.create_session("work").unwrap();
    let mut target = first;
    for index in 1..5 {
        target = state
            .split_pane(
                target,
                Axis::Horizontal,
                if index == 2 {
                    PaneKind::Browser(BrowserDescriptor::single(
                        "https://layout.example".to_owned(),
                        "default".to_owned(),
                    ))
                } else {
                    PaneKind::Terminal
                },
            )
            .unwrap();
    }
    let panes = state.windows[&window].panes.clone();
    let pane_order = state.windows[&window].pane_order.clone();
    let mut retired_splits = layout_splits(&state.windows[&window].layout);
    for (preset, expected) in [
        (
            LayoutPreset::EvenHorizontal,
            [(16, 24), (15, 24), (15, 24), (15, 24), (15, 24)],
        ),
        (
            LayoutPreset::EvenVertical,
            [(80, 4), (80, 4), (80, 4), (80, 4), (80, 4)],
        ),
        (
            LayoutPreset::MainHorizontal,
            [(80, 22), (20, 1), (19, 1), (19, 1), (19, 1)],
        ),
        (
            LayoutPreset::MainHorizontalMirrored,
            [(80, 22), (20, 1), (19, 1), (19, 1), (19, 1)],
        ),
        (
            LayoutPreset::MainVertical,
            [(78, 24), (1, 6), (1, 5), (1, 5), (1, 5)],
        ),
        (
            LayoutPreset::MainVerticalMirrored,
            [(78, 24), (1, 6), (1, 5), (1, 5), (1, 5)],
        ),
        (
            LayoutPreset::Tiled,
            [(39, 7), (40, 7), (39, 7), (40, 7), (80, 8)],
        ),
    ] {
        state
            .select_layout(window, preset, &PresetOptions::default())
            .unwrap();
        let arranged = &state.windows[&window];
        assert_eq!(arranged.panes, panes);
        assert_eq!(arranged.zoomed_pane, None);
        assert_eq!(arranged.last_layout, Some(preset));
        assert_eq!(
            pane_order
                .iter()
                .map(|pane| pane_size(&arranged.layout, *pane))
                .collect::<Vec<_>>(),
            expected.to_vec(),
            "{}",
            preset.name()
        );
        let new_splits = layout_splits(&arranged.layout);
        assert_eq!(new_splits.len(), panes.len() - 1);
        assert!(
            new_splits
                .iter()
                .all(|split| !retired_splits.contains(split))
        );
        retired_splits.extend(new_splits);
    }

    state
        .select_layout(
            window,
            LayoutPreset::MainHorizontal,
            &PresetOptions::default(),
        )
        .unwrap();
    state
        .select_layout(
            window,
            LayoutPreset::MainVerticalMirrored,
            &PresetOptions::default(),
        )
        .unwrap();

    state.swap_panes(first, target, true, false).unwrap();
    let reordered = state.windows[&window].pane_order.clone();
    state.restore_previous_layout(window).unwrap();
    assert_eq!(layout_panes(&state.windows[&window].layout), reordered);
    let projected = state.windows[&window].layout.project();
    assert!(matches!(
        &projected,
        LayoutNode::Split {
            axis: Axis::Vertical,
            first: main,
            ..
        } if matches!(main.as_ref(), LayoutNode::Pane(pane) if *pane == target)
    ));

    let tiled_order = state.windows[&window].pane_order.clone();
    state
        .select_layout(window, LayoutPreset::Tiled, &PresetOptions::default())
        .unwrap();
    assert_eq!(
        tiled_order
            .iter()
            .map(|pane| pane_size(&state.windows[&window].layout, *pane))
            .collect::<Vec<_>>(),
        [(39, 7), (40, 7), (39, 7), (40, 7), (80, 8)].to_vec()
    );
    assert!(state.validate().is_ok());
}

#[test]
fn serialized_layouts_adopt_extent_assign_pane_order_and_trim_bottom_right() {
    let mut state = MuxState::default();
    let (_, window, first) = state.create_session("work").unwrap();
    let second = state
        .split_pane(first, Axis::Horizontal, PaneKind::Terminal)
        .unwrap();
    let third = state
        .split_pane(second, Axis::Vertical, PaneKind::Terminal)
        .unwrap();
    let original = state.windows[&window].layout.clone();
    let retired = layout_splits(&original);
    let input = "b78d,120x30,0,0{50x30,0,0,9,69x30,51,0[69x14,51,0,8,69x15,51,15,7]}";

    state.select_layout_string(window, input).unwrap();
    let applied = state.windows[&window].layout.clone();
    assert_eq!(applied.extent(), (120, 30));
    assert_eq!(applied.panes_in_order(), [first, second, third]);
    assert_eq!(pane_size(&applied, first), (50, 30));
    assert_eq!(pane_size(&applied, second), (69, 14));
    assert_eq!(pane_size(&applied, third), (69, 15));
    assert!(
        layout_splits(&applied)
            .iter()
            .all(|split| !retired.contains(split))
    );

    state.restore_previous_layout(window).unwrap();
    assert!(same_layout_geometry(
        &state.windows[&window].layout,
        &original
    ));
    state.restore_previous_layout(window).unwrap();
    assert!(same_layout_geometry(
        &state.windows[&window].layout,
        &applied
    ));

    state.kill_pane(second).unwrap();
    state
        .select_layout_string(
            window,
            "e7f0,100x20,0,0[100x9,0,0{49x9,0,0,50,50x9,50,0,51},100x10,0,10,52]",
        )
        .unwrap();
    let trimmed = &state.windows[&window].layout;
    assert_eq!(trimmed.extent(), (100, 20));
    assert_eq!(trimmed.panes_in_order(), [first, third]);
    assert_eq!(pane_size(trimmed, first), (49, 20));
    assert_eq!(pane_size(trimmed, third), (50, 20));
    assert!(state.validate().is_ok());
}

#[test]
fn serialized_layouts_keep_the_tiled_z_order() {
    let mut state = MuxState::default();
    let (_, window, first) = state.create_session("work").unwrap();
    let second = state
        .split_pane_with(
            first,
            Axis::Horizontal,
            PaneKind::Terminal,
            SplitPlacement {
                before: true,
                ..SplitPlacement::default()
            },
        )
        .unwrap();
    assert_eq!(
        state.windows[&window].layout.panes_in_order(),
        [second, first]
    );
    assert_eq!(state.windows[&window].z_order(), [first, second]);

    state
        .select_layout_string(window, "0209,80x24,0,0{40x24,0,0,1,39x24,41,0,0}")
        .unwrap();
    assert_eq!(
        state.windows[&window].layout.panes_in_order(),
        [second, first]
    );
    assert_eq!(state.windows[&window].z_order(), [first, second]);
}

#[test]
fn serialized_layout_have_need_error_is_atomic() {
    let mut state = MuxState::default();
    let (_, window, first) = state.create_session("work").unwrap();
    state
        .split_pane(first, Axis::Horizontal, PaneKind::Terminal)
        .unwrap();
    let before = state.windows[&window].layout.clone();
    let generation = state.generation();
    let next_split_id = state.next_split_id;
    let input = "b25d,80x24,0,0,0";

    assert_eq!(
        state.select_layout_string(window, input),
        Err(ServerError::InvalidCommand(format!(
            "have 2 panes but need 1: {input}"
        )))
    );
    assert_eq!(state.windows[&window].layout, before);
    assert_eq!(state.generation(), generation);
    assert_eq!(state.next_split_id, next_split_id);
}

#[test]
fn layout_cycle_restore_and_spread_are_reversible_and_atomic() {
    let mut state = MuxState::default();
    let (_, window, first) = state.create_session("work").unwrap();
    let second = state
        .split_pane(first, Axis::Horizontal, PaneKind::Terminal)
        .unwrap();
    let third = state
        .split_pane(second, Axis::Vertical, PaneKind::Terminal)
        .unwrap();
    let original = state.windows[&window].layout.clone();
    let mut retired_splits = layout_splits(&original);

    assert_eq!(
        state
            .cycle_layout(window, 1, &PresetOptions::default())
            .unwrap(),
        LayoutPreset::EvenHorizontal
    );
    let even = state.windows[&window].layout.clone();
    retired_splits.extend(layout_splits(&even));
    assert_ne!(even, original);
    state.restore_previous_layout(window).unwrap();
    assert!(same_layout_geometry(
        &state.windows[&window].layout,
        &original
    ));
    let restored_splits = layout_splits(&state.windows[&window].layout);
    assert!(
        restored_splits
            .iter()
            .all(|split| !retired_splits.contains(split))
    );
    retired_splits.extend(restored_splits);
    state.restore_previous_layout(window).unwrap();
    assert!(same_layout_geometry(&state.windows[&window].layout, &even));
    let restored_splits = layout_splits(&state.windows[&window].layout);
    assert!(
        restored_splits
            .iter()
            .all(|split| !retired_splits.contains(split))
    );

    state.resize_pane(first, Axis::Horizontal, -4).unwrap();
    state.spread_layout(first).unwrap();
    let spread = &state.windows[&window];
    assert_eq!(
        spread
            .pane_order
            .iter()
            .map(|pane| pane_size(&spread.layout, *pane))
            .collect::<Vec<_>>(),
        [(26, 24), (26, 24), (26, 24)]
    );

    state
        .select_layout(window, LayoutPreset::Tiled, &PresetOptions::default())
        .unwrap();
    assert_eq!(
        state
            .cycle_layout(window, 1, &PresetOptions::default())
            .unwrap(),
        LayoutPreset::EvenHorizontal
    );
    assert_eq!(
        state
            .cycle_layout(window, -1, &PresetOptions::default())
            .unwrap(),
        LayoutPreset::Tiled
    );
    let fourth = state
        .split_pane(third, Axis::Vertical, PaneKind::Terminal)
        .unwrap();
    let before_failed_restore = state.windows[&window].layout.clone();
    let generation = state.generation();
    assert!(matches!(
        state.restore_previous_layout(window),
        Err(ServerError::InvalidCommand(_))
    ));
    assert_eq!(state.windows[&window].layout, before_failed_restore);
    assert_eq!(state.generation(), generation);
    assert!(state.windows[&window].panes.contains_key(&fourth));
    assert!(state.validate().is_ok());
}
