use super::test_support::{layout_panes, layout_splits, pane_size, same_projected_geometry};
use super::*;

#[test]
fn cwd_donors_fall_back_to_the_last_focused_terminal() {
    let mut state = MuxState::default();
    let (_, _, first) = state.create_session("main").unwrap();
    let second = state
        .split_pane(first, Axis::Horizontal, PaneKind::Terminal)
        .unwrap();
    let browser = state
        .split_pane(
            second,
            Axis::Vertical,
            PaneKind::Browser(BrowserDescriptor::single(
                "about:blank".to_owned(),
                "default".to_owned(),
            )),
        )
        .unwrap();

    assert_eq!(state.cwd_donor(first), Some(first));
    assert_eq!(state.cwd_donor(second), Some(second));
    assert_eq!(
        state.cwd_donor(browser),
        Some(second),
        "a pane without a working directory borrows the last focused terminal's"
    );

    state.select_pane(first).unwrap();
    state.select_pane(browser).unwrap();
    assert_eq!(state.cwd_donor(browser), Some(first));

    let picker = state
        .split_pane(
            browser,
            Axis::Horizontal,
            PaneKind::Picker {
                inherit_cwd_from: None,
            },
        )
        .unwrap();
    assert_eq!(
        state.materialize_pane(
            picker,
            PaneKind::Editor(EditorDescriptor {
                path: None,
                cwd: "/".to_owned()
            })
        ),
        Ok(Some(first)),
        "a picker split with no donor still opens on the last focused terminal"
    );

    state.kill_pane(first).unwrap();
    state.kill_pane(second).unwrap();
    assert_eq!(
        state.cwd_donor(browser),
        None,
        "a window with no terminal left has nothing to inherit"
    );

    let unplaced = state
        .split_pane(
            browser,
            Axis::Vertical,
            PaneKind::Agent(AgentDescriptor::default()),
        )
        .unwrap();
    assert_eq!(state.cwd_donor(unplaced), None);

    let agent = state
        .split_pane(
            unplaced,
            Axis::Vertical,
            PaneKind::Agent(AgentDescriptor {
                cwd: Some(PathBuf::from("/work/project")),
                ..AgentDescriptor::default()
            }),
        )
        .unwrap();
    assert_eq!(state.cwd_donor(agent), Some(agent));
    assert_eq!(
        state.cwd_donor(browser),
        Some(agent),
        "an agent pane in a folder lends it like a terminal"
    );
    assert_eq!(state.cwd_donor(unplaced), Some(agent));
}

#[test]
fn split_remove_and_ids_preserve_invariants() {
    let mut state = MuxState::default();
    let (_, window, first) = state.create_session("main").unwrap();
    let second = state
        .split_pane(first, Axis::Horizontal, PaneKind::Terminal)
        .unwrap();
    let third = state
        .split_pane(
            second,
            Axis::Vertical,
            PaneKind::Browser(BrowserDescriptor::single(
                "https://example.com".to_owned(),
                "default".to_owned(),
            )),
        )
        .unwrap();
    assert_eq!((first, second, third), (PaneId(0), PaneId(1), PaneId(2)));
    assert!(state.validate().is_ok());
    state.kill_pane(second).unwrap();
    assert!(state.validate().is_ok());
    assert!(state.windows[&window].panes.contains_key(&third));
    let fourth = state
        .split_pane(first, Axis::Vertical, PaneKind::Terminal)
        .unwrap();
    assert_eq!(fourth, PaneId(3));
}

#[test]
fn pane_removal_fallback_preserves_the_replacement_active_point() {
    let mut state = MuxState::default();
    let (_, window, first) = state.create_session("work").unwrap();
    let second = state
        .split_pane(first, Axis::Horizontal, PaneKind::Terminal)
        .unwrap();
    let third = state
        .split_pane(second, Axis::Vertical, PaneKind::Terminal)
        .unwrap();
    state.select_pane(first).unwrap();
    let replacement_point = state.windows[&window].panes[&third].active_point;

    state.kill_pane(first).unwrap();

    assert_eq!(state.windows[&window].active_pane, third);
    assert_eq!(
        state.windows[&window].panes[&third].active_point,
        replacement_point
    );
    assert!(state.validate().is_ok());
}

#[test]
fn failed_split_keeps_the_cell_tree_and_allocators_unchanged() {
    let mut state = MuxState::default();
    let (_, window, first) = state.create_session("main").unwrap();
    let narrow = state
        .split_pane_with(
            first,
            Axis::Vertical,
            PaneKind::Terminal,
            SplitPlacement {
                size: SplitSize::Cells(1),
                ..SplitPlacement::default()
            },
        )
        .unwrap();
    let layout = state.windows[&window].layout.clone();
    let next_pane_id = state.next_pane_id;
    let next_split_id = state.next_split_id;
    let generation = state.generation();

    assert_eq!(
        state.split_pane(narrow, Axis::Vertical, PaneKind::Terminal),
        Err(ServerError::InvalidCommand(
            "no space for a new pane".to_owned()
        ))
    );
    assert_eq!(state.windows[&window].layout, layout);
    assert_eq!(state.windows[&window].panes.len(), 2);
    assert_eq!(state.next_pane_id, next_pane_id);
    assert_eq!(state.next_split_id, next_split_id);
    assert_eq!(state.generation(), generation);
    let next = state
        .split_pane(first, Axis::Horizontal, PaneKind::Terminal)
        .unwrap();
    assert_eq!(next, PaneId(next_pane_id));
    assert!(state.validate().is_ok());
}

#[test]
fn remove_promotes_the_surviving_cell_subtree_without_reallocating_descendants() {
    for target_first in [true, false] {
        let mut state = MuxState::default();
        let (_, window, first) = state.create_session("work").unwrap();
        let second = state
            .split_pane(first, Axis::Horizontal, PaneKind::Terminal)
            .unwrap();
        let (target, survivor, nested) = if target_first {
            let nested = state
                .split_pane(second, Axis::Vertical, PaneKind::Terminal)
                .unwrap();
            (first, second, nested)
        } else {
            let nested = state
                .split_pane(first, Axis::Vertical, PaneKind::Terminal)
                .unwrap();
            (second, first, nested)
        };
        assert_eq!(
            layout_splits(&state.windows[&window].layout),
            [SplitId(0), SplitId(1)]
        );

        state.kill_pane(target).unwrap();

        let layout = &state.windows[&window].layout;
        assert_eq!(layout_splits(layout), [SplitId(1)]);
        assert_eq!(pane_size(layout, survivor), (80, 12));
        assert_eq!(pane_size(layout, nested), (80, 11));
        assert!(state.validate().is_ok());
    }
}

#[test]
fn directional_navigation_uses_geometry_wrapping_and_mru_tie_breaking() {
    let mut state = MuxState::default();
    let (_, window, left) = state.create_session("work").unwrap();
    let right_top = state
        .split_pane(left, Axis::Horizontal, PaneKind::Terminal)
        .unwrap();
    let right_bottom = state
        .split_pane(right_top, Axis::Vertical, PaneKind::Terminal)
        .unwrap();

    assert_eq!(
        state
            .pane_in_direction(right_top, PaneDirection::Down)
            .unwrap(),
        Some(right_bottom)
    );
    assert_eq!(
        state
            .pane_in_direction(right_bottom, PaneDirection::Up)
            .unwrap(),
        Some(right_top)
    );
    assert_eq!(
        state
            .pane_in_direction(right_bottom, PaneDirection::Right)
            .unwrap(),
        Some(left)
    );
    assert_eq!(
        state.pane_in_direction(left, PaneDirection::Up).unwrap(),
        None
    );

    state.select_pane(left).unwrap();
    assert_eq!(
        state.pane_in_direction(left, PaneDirection::Right).unwrap(),
        Some(right_bottom),
        "the most recently active adjacent pane wins"
    );
    state.select_pane(right_top).unwrap();
    state.select_pane(left).unwrap();
    assert_eq!(
        state.pane_in_direction(left, PaneDirection::Right).unwrap(),
        Some(right_top)
    );

    state.resize_split(window, SplitId(0), 0.72).unwrap();
    state.resize_split(window, SplitId(1), 0.35).unwrap();
    assert_eq!(
        state
            .pane_in_direction(right_top, PaneDirection::Left)
            .unwrap(),
        Some(left)
    );
    assert!(state.validate().is_ok());
}

#[test]
fn pane_swaps_preserve_layout_identity_active_slots_and_cross_window_state() {
    let mut state = MuxState::default();
    let (session, window, first) = state.create_session("work").unwrap();
    let second = state
        .split_pane(first, Axis::Horizontal, PaneKind::Terminal)
        .unwrap();
    let third = state
        .split_pane(second, Axis::Vertical, PaneKind::Terminal)
        .unwrap();
    let split_ids = layout_splits(&state.windows[&window].layout);

    assert_eq!(state.previous_pane(first).unwrap(), third);
    assert_eq!(state.next_pane(third).unwrap(), first);
    state.toggle_zoom(third).unwrap();
    state.swap_panes(second, third, false, true).unwrap();
    assert_eq!(
        layout_panes(&state.windows[&window].layout),
        [first, third, second]
    );
    assert_eq!(state.windows[&window].pane_order, [first, third, second]);
    assert_eq!(state.windows[&window].active_pane, third);
    assert_eq!(state.windows[&window].zoomed_pane, Some(third));
    let current_split_ids = layout_splits(&state.windows[&window].layout);
    assert_eq!(current_split_ids, split_ids, "split IDs remain stable");

    let second_point = state.windows[&window].panes[&second].active_point;
    let third_point = state.windows[&window].panes[&third].active_point;
    state.swap_panes(third, second, true, false).unwrap();
    assert_eq!(
        layout_panes(&state.windows[&window].layout),
        [first, second, third]
    );
    assert_eq!(state.windows[&window].pane_order, [first, second, third]);
    assert_eq!(
        state.windows[&window].active_pane, third,
        "tmux reselects the source after briefly selecting the target"
    );
    let second_point_after = state.windows[&window].panes[&second].active_point;
    let third_point_after = state.windows[&window].panes[&third].active_point;
    assert!(second_point_after > second_point);
    assert!(third_point_after > third_point);
    assert!(third_point_after > second_point_after);
    assert_eq!(state.windows[&window].zoomed_pane, None);

    let (other_window, other) = state
        .create_window(session, Some("other".to_owned()), PaneKind::Terminal)
        .unwrap();
    state.swap_panes(third, other, false, false).unwrap();
    assert!(state.windows[&window].panes.contains_key(&other));
    assert!(!state.windows[&window].panes.contains_key(&third));
    assert!(state.windows[&other_window].panes.contains_key(&third));
    assert_eq!(state.windows[&window].active_pane, other);
    assert_eq!(state.windows[&other_window].active_pane, third);
    assert_eq!(layout_panes(&state.windows[&window].layout)[2], other);
    assert_eq!(
        state.windows[&other_window].layout.project(),
        LayoutNode::Pane(third)
    );
    assert_eq!(state.windows[&window].pane_order, [first, second, other]);
    assert_eq!(state.windows[&other_window].pane_order, [third]);
    assert!(state.validate().is_ok());
}

#[test]
fn window_rotation_moves_surfaces_through_stable_layout_slots_and_preserves_zoom() {
    let mut state = MuxState::default();
    let (_, window, first) = state.create_session("work").unwrap();
    let second = state
        .split_pane(
            first,
            Axis::Horizontal,
            PaneKind::Browser(BrowserDescriptor::single(
                "https://rotate.example".to_owned(),
                "default".to_owned(),
            )),
        )
        .unwrap();
    let third = state
        .split_pane(second, Axis::Vertical, PaneKind::Terminal)
        .unwrap();
    let fourth = state
        .split_pane(first, Axis::Vertical, PaneKind::Terminal)
        .unwrap();
    state
        .select_layout(
            window,
            LayoutPreset::MainHorizontalMirrored,
            &PresetOptions::default(),
        )
        .unwrap();
    state.select_pane(third).unwrap();
    state.toggle_zoom(third).unwrap();

    let panes = state.windows[&window].panes.clone();
    let order = state.windows[&window].pane_order.clone();
    assert_eq!(order, [first, fourth, second, third]);
    let layout = state.windows[&window].layout.clone();
    let layout_order = layout_panes(&layout);
    let split_ids = layout_splits(&layout);

    let rotated = state.rotate_window(window, false, true).unwrap();
    assert_eq!(rotated, first);
    assert_eq!(state.windows[&window].active_pane, first);
    assert_eq!(state.windows[&window].zoomed_pane, Some(first));
    assert_eq!(
        state.windows[&window].pane_order,
        [fourth, second, third, first]
    );
    let replacements = BTreeMap::from([
        (first, fourth),
        (fourth, second),
        (second, third),
        (third, first),
    ]);
    assert_eq!(
        layout_panes(&state.windows[&window].layout),
        layout_order
            .iter()
            .map(|pane| replacements[pane])
            .collect::<Vec<_>>()
    );
    let rotated_split_ids = layout_splits(&state.windows[&window].layout);
    assert_eq!(rotated_split_ids, split_ids);
    assert!(state.windows[&window].panes[&first].active_point > panes[&first].active_point);
    for pane in [second, third, fourth] {
        assert_eq!(state.windows[&window].panes[&pane], panes[&pane]);
    }

    let restored = state.rotate_window(window, true, true).unwrap();
    assert_eq!(restored, third);
    assert_eq!(state.windows[&window].pane_order, order);
    assert_eq!(state.windows[&window].layout, layout);
    assert_eq!(state.windows[&window].active_pane, third);
    assert_eq!(state.windows[&window].zoomed_pane, Some(third));

    state.rotate_window(window, false, false).unwrap();
    assert_eq!(state.windows[&window].zoomed_pane, None);
    assert!(state.validate().is_ok());
}

#[test]
fn break_and_join_follow_tmux_window_activity_transitions() {
    let mut state = MuxState::default();
    let (session, original_window, first) = state.create_session("work").unwrap();
    let moving = state
        .split_pane(first, Axis::Horizontal, PaneKind::Terminal)
        .unwrap();

    let next_sort_point = state.next_sort_point;
    let broken_window = state
        .break_pane(moving, session, None, None, false)
        .unwrap();
    assert_eq!(state.next_sort_point, next_sort_point + 2);
    assert!(
        state.windows[&broken_window].activity > state.windows[&broken_window].created,
        "selecting the newly created window updates its activity"
    );
    assert_eq!(state.sessions[&session].active_window, broken_window);

    let target_activity = state.windows[&original_window].activity;
    let next_sort_point = state.next_sort_point;
    state
        .join_pane(
            moving,
            first,
            Axis::Horizontal,
            SplitSize::Default,
            false,
            false,
            false,
        )
        .unwrap();
    assert_eq!(state.next_sort_point, next_sort_point + 2);
    assert!(!state.windows.contains_key(&broken_window));
    assert_eq!(state.sessions[&session].active_window, original_window);
    assert!(state.windows[&original_window].activity > target_activity);

    let next_sort_point = state.next_sort_point;
    let detached_window = state.break_pane(moving, session, None, None, true).unwrap();
    assert_eq!(state.next_sort_point, next_sort_point + 1);
    assert_eq!(
        state.windows[&detached_window].activity,
        state.windows[&detached_window].created
    );
    state.select_window(session, detached_window).unwrap();
    let target_activity = state.windows[&original_window].activity;
    let moving_point = state.windows[&detached_window].panes[&moving].active_point;
    let next_sort_point = state.next_sort_point;

    state
        .join_pane(
            moving,
            first,
            Axis::Horizontal,
            SplitSize::Default,
            false,
            false,
            true,
        )
        .unwrap();

    assert_eq!(state.next_sort_point, next_sort_point + 1);
    assert!(!state.windows.contains_key(&detached_window));
    assert_eq!(state.sessions[&session].active_window, original_window);
    assert_eq!(state.windows[&original_window].active_pane, first);
    assert_eq!(
        state.windows[&original_window].panes[&moving].active_point,
        moving_point
    );
    assert!(state.windows[&original_window].activity > target_activity);
    assert!(state.validate().is_ok());
}

#[test]
fn break_and_join_reparent_existing_panes_without_reusing_layout_ids() {
    let mut state = MuxState::default();
    let (session, original_window, first) = state.create_session("work").unwrap();
    let browser = state
        .split_pane(
            first,
            Axis::Horizontal,
            PaneKind::Browser(BrowserDescriptor::single(
                "https://example.com".to_owned(),
                "default".to_owned(),
            )),
        )
        .unwrap();
    let third = state
        .split_pane(browser, Axis::Vertical, PaneKind::Terminal)
        .unwrap();
    let next_split = SplitId(state.next_split_id);

    let broken_window = state
        .break_pane(browser, session, None, Some("web".to_owned()), false)
        .unwrap();
    assert_eq!(
        state.windows[&broken_window].layout.project(),
        LayoutNode::Pane(browser)
    );
    assert_eq!(state.windows[&broken_window].name, "web");
    assert!(matches!(
        state.windows[&broken_window].panes[&browser].kind,
        PaneKind::Browser(_)
    ));
    assert_eq!(state.sessions[&session].active_window, broken_window);
    assert_eq!(
        layout_panes(&state.windows[&original_window].layout),
        [first, third]
    );
    assert_eq!(state.windows[&original_window].pane_order, [first, third]);
    assert_eq!(state.windows[&broken_window].pane_order, [browser]);

    state
        .join_pane(
            browser,
            first,
            Axis::Horizontal,
            SplitSize::Percent(30),
            true,
            false,
            false,
        )
        .unwrap();
    assert!(!state.windows.contains_key(&broken_window));
    assert_eq!(
        layout_panes(&state.windows[&original_window].layout),
        [browser, first, third]
    );
    assert_eq!(
        state.windows[&original_window].pane_order,
        [first, browser, third]
    );
    assert_eq!(
        [browser, first, third]
            .map(|pane| { pane_size(&state.windows[&original_window].layout, pane) }),
        [(12, 24), (27, 24), (39, 24)]
    );
    assert!(layout_splits(&state.windows[&original_window].layout).contains(&next_split));
    assert_eq!(state.windows[&original_window].active_pane, browser);

    let second_break = state
        .break_pane(browser, session, None, None, true)
        .unwrap();
    state
        .join_pane(
            browser,
            third,
            Axis::Vertical,
            SplitSize::Percent(25),
            false,
            true,
            true,
        )
        .unwrap();
    assert!(!state.windows.contains_key(&second_break));
    assert_eq!(
        [first, third, browser]
            .map(|pane| { pane_size(&state.windows[&original_window].layout, pane) }),
        [(40, 17), (39, 17), (80, 6)]
    );
    assert!(matches!(
        state.windows[&original_window].layout.project(),
        LayoutNode::Split {
            axis: Axis::Vertical,
            second,
            ..
        } if second.as_ref() == &LayoutNode::Pane(browser)
    ));
    assert_eq!(
        state.windows[&original_window].pane_order,
        [first, third, browser]
    );
    assert!(state.validate().is_ok());
}

#[test]
fn breaking_a_last_pane_moves_its_window_and_retires_the_source_session() {
    let mut state = MuxState::default();
    let (source_session, source_window, source) = state.create_session("source").unwrap();
    let (destination_session, destination_window, destination) =
        state.create_session("destination").unwrap();
    state
        .rename_window(source_window, "kept")
        .expect("rename source window");
    let source_layout = state.windows[&source_window].layout.clone();

    let broken = state
        .break_pane(source, destination_session, None, None, true)
        .unwrap();

    assert_eq!(broken, source_window);
    assert!(!state.sessions.contains_key(&source_session));
    assert_eq!(state.window_for_pane(source), Some(source_window));
    assert_eq!(state.window_for_pane(destination), Some(destination_window));
    assert_eq!(state.windows[&source_window].name, "kept");
    assert_eq!(state.windows[&source_window].layout, source_layout);
    assert_eq!(state.windows[&source_window].session, destination_session);
    assert_eq!(
        state.sessions[&destination_session].active_window,
        destination_window
    );
    assert!(
        state.sessions[&destination_session]
            .windows
            .contains(&source_window)
    );
    assert!(state.validate().is_ok());
}

#[test]
fn failed_cross_window_join_restores_the_source_and_target_layouts() {
    let mut state = MuxState::default();
    let (session, target_window, target) = state.create_session("work").unwrap();
    let (source_window, source) = state
        .create_window(session, Some("source".to_owned()), PaneKind::Terminal)
        .unwrap();
    state
        .windows
        .get_mut(&mut state.journal, &target_window)
        .unwrap()
        .layout
        .resize(80, 1);
    let target_layout = state.windows[&target_window].layout.clone();
    let source_layout = state.windows[&source_window].layout.clone();
    let next_split_id = state.next_split_id;
    let generation = state.generation();

    assert_eq!(
        state.join_pane(
            source,
            target,
            Axis::Vertical,
            SplitSize::Default,
            false,
            false,
            false,
        ),
        Err(ServerError::InvalidCommand(
            "no space for a new pane".to_owned()
        ))
    );
    assert_eq!(state.windows[&target_window].layout, target_layout);
    assert_eq!(state.windows[&source_window].layout, source_layout);
    assert!(state.windows[&source_window].panes.contains_key(&source));
    assert!(!state.windows[&target_window].panes.contains_key(&source));
    assert_eq!(state.next_split_id, next_split_id);
    assert_eq!(state.generation(), generation);
    assert!(state.validate().is_ok());
}

#[test]
fn cross_session_join_keeps_the_source_session_nonempty() {
    let mut state = MuxState::default();
    let (source_session, _, source_base) = state.create_session("source").unwrap();
    let moving = state
        .split_pane(source_base, Axis::Horizontal, PaneKind::Terminal)
        .unwrap();
    let moving_window = state
        .break_pane(moving, source_session, None, None, true)
        .unwrap();
    let (target_session, target_window, target) = state.create_session("target").unwrap();

    state
        .join_pane(
            moving,
            target,
            Axis::Vertical,
            SplitSize::Default,
            false,
            false,
            false,
        )
        .unwrap();
    assert!(!state.windows.contains_key(&moving_window));
    assert!(state.sessions.contains_key(&source_session));
    assert_eq!(state.window_for_pane(moving), Some(target_window));
    assert_eq!(state.windows[&target_window].session, target_session);
    assert_eq!(state.windows[&target_window].pane_order, [target, moving]);
    state
        .select_layout(
            target_window,
            LayoutPreset::MainHorizontal,
            &PresetOptions::default(),
        )
        .unwrap();
    assert!(matches!(
        state.windows[&target_window].layout.project(),
        LayoutNode::Split {
            first,
            ..
        } if matches!(first.as_ref(), LayoutNode::Pane(pane) if *pane == target)
    ));

    let (last_session, _, last_pane) = state.create_session("last").unwrap();
    assert!(matches!(
        state.join_pane(
            last_pane,
            target,
            Axis::Horizontal,
            SplitSize::Default,
            false,
            false,
            false,
        ),
        Err(ServerError::InvalidCommand(message)) if message.contains("last window")
    ));
    assert!(state.sessions.contains_key(&last_session));
    assert!(state.validate().is_ok());
}

#[test]
fn same_window_join_reparents_a_pane_beside_the_target() {
    for (axis, before, expected_layout, expected_order) in [
        (Axis::Horizontal, true, [3, 1, 2], [1, 3, 2]),
        (Axis::Horizontal, false, [1, 3, 2], [1, 3, 2]),
        (Axis::Vertical, true, [3, 1, 2], [1, 3, 2]),
        (Axis::Vertical, false, [1, 3, 2], [1, 3, 2]),
    ] {
        let mut state = MuxState::default();
        let (_, window, first) = state.create_session("work").unwrap();
        let second = state
            .split_pane(first, Axis::Horizontal, PaneKind::Terminal)
            .unwrap();
        let third = state
            .split_pane(second, Axis::Vertical, PaneKind::Terminal)
            .unwrap();
        let panes = [first, second, third];
        let expected_layout = expected_layout.map(|index| panes[index - 1]);
        let expected_order = expected_order.map(|index| panes[index - 1]);
        state.select_pane(second).unwrap();

        let predicted = joined_layout(
            &state.windows[&window].layout.project(),
            third,
            first,
            SplitId(u64::MAX),
            axis,
            0.5,
            before,
        )
        .expect("both leaves are in the window");
        state
            .join_pane(third, first, axis, SplitSize::Default, before, false, true)
            .unwrap();

        let layout = &state.windows[&window].layout;
        assert_eq!(layout_panes(layout), expected_layout);
        assert!(same_projected_geometry(layout, &predicted));
        assert_eq!(state.windows[&window].pane_order, expected_order);
        assert_eq!(state.windows[&window].active_pane, second);
        assert!(state.validate().is_ok());
    }
}

#[test]
fn same_window_join_matches_tmux_active_pane_transitions() {
    for detached in [true, false] {
        let mut state = MuxState::default();
        let (_, window, first) = state.create_session("work").unwrap();
        let second = state
            .split_pane(first, Axis::Horizontal, PaneKind::Terminal)
            .unwrap();
        let source = state
            .split_pane(second, Axis::Vertical, PaneKind::Terminal)
            .unwrap();
        state.select_pane(second).unwrap();
        state.select_pane(source).unwrap();
        let source_point = state.windows[&window].panes[&source].active_point;
        let fallback_point = state.windows[&window].panes[&second].active_point;
        let next_sort_point = state.next_sort_point;

        state
            .join_pane(
                source,
                first,
                Axis::Horizontal,
                SplitSize::Default,
                false,
                false,
                detached,
            )
            .unwrap();

        let expected_active = if detached { second } else { source };
        assert_eq!(state.windows[&window].active_pane, expected_active);
        assert_eq!(
            state.windows[&window].panes[&second].active_point, fallback_point,
            "losing the active source uses the fallback without selecting it"
        );
        if detached {
            assert_eq!(
                state.windows[&window].panes[&source].active_point,
                source_point
            );
            assert_eq!(state.next_sort_point, next_sort_point);
        } else {
            assert!(state.windows[&window].panes[&source].active_point > source_point);
            assert_eq!(state.next_sort_point, next_sort_point + 1);
        }
        assert!(state.validate().is_ok());
    }
}

#[test]
fn joining_the_last_sibling_splits_the_promoted_root() {
    let mut state = MuxState::default();
    let (_, window, first) = state.create_session("work").unwrap();
    let second = state
        .split_pane(first, Axis::Horizontal, PaneKind::Terminal)
        .unwrap();

    let predicted = joined_layout(
        &state.windows[&window].layout.project(),
        second,
        first,
        SplitId(u64::MAX),
        Axis::Vertical,
        0.5,
        true,
    )
    .expect("both leaves are in the window");
    state
        .join_pane(
            second,
            first,
            Axis::Vertical,
            SplitSize::Default,
            true,
            false,
            true,
        )
        .unwrap();

    let layout = &state.windows[&window].layout;
    assert!(matches!(
        layout.project(),
        LayoutNode::Split { axis: Axis::Vertical, first: top, second: bottom, .. }
            if top.as_ref() == &LayoutNode::Pane(second)
                && bottom.as_ref() == &LayoutNode::Pane(first)
    ));
    assert!(same_projected_geometry(layout, &predicted));
    assert_eq!(state.windows[&window].pane_order, [first, second]);
    assert!(state.validate().is_ok());
}

#[test]
fn swapped_layout_matches_the_swap_mutation() {
    let mut state = MuxState::default();
    let (_, window, first) = state.create_session("work").unwrap();
    let second = state
        .split_pane(first, Axis::Horizontal, PaneKind::Terminal)
        .unwrap();
    let third = state
        .split_pane(second, Axis::Vertical, PaneKind::Terminal)
        .unwrap();

    let predicted = swapped_layout(&state.windows[&window].layout.project(), first, third);
    state.swap_panes(first, third, true, false).unwrap();

    assert_eq!(state.windows[&window].layout.project(), predicted);
    assert_eq!(
        layout_panes(&state.windows[&window].layout),
        [third, second, first]
    );
    assert!(state.validate().is_ok());
}

#[test]
fn zoom_tracks_the_active_pane_and_layout_mutations_unzoom() {
    let mut state = MuxState::default();
    let (_, window, first) = state.create_session("work").unwrap();
    state.toggle_zoom(first).unwrap();
    assert_eq!(state.windows[&window].zoomed_pane, None);

    let second = state
        .split_pane(first, Axis::Horizontal, PaneKind::Terminal)
        .unwrap();
    state.toggle_zoom(first).unwrap();
    assert_eq!(state.windows[&window].active_pane, first);
    assert_eq!(state.windows[&window].zoomed_pane, Some(first));

    state.select_pane(second).unwrap();
    assert_eq!(state.windows[&window].zoomed_pane, None);
    state.toggle_zoom(second).unwrap();
    state.select_pane_with_zoom(first, true).unwrap();
    assert_eq!(state.windows[&window].zoomed_pane, Some(first));

    state.resize_pane(first, Axis::Horizontal, 1).unwrap();
    assert_eq!(state.windows[&window].zoomed_pane, None);
    state.toggle_zoom(first).unwrap();
    state
        .split_pane(first, Axis::Vertical, PaneKind::Terminal)
        .unwrap();
    assert_eq!(state.windows[&window].zoomed_pane, None);
    assert!(state.validate().is_ok());
}

#[test]
fn revive_clears_command_and_unseen() {
    let mut state = MuxState::default();
    let (_, _, pane) = state.create_session("work").unwrap();
    assert!(state.update_pane_summary(pane, |summary| {
        summary.changed_at = 9;
        summary.unseen = true;
        summary.command = Some(zz_protocol::ShellCommand::Finished(1));
        summary.name = "reviewer".to_owned();
    }));
    assert!(state.mark_pane_dead(pane, Some(1)).unwrap());

    assert!(state.revive_pane(pane).unwrap());
    let summary = &state.pane(pane).unwrap().summary;
    assert_eq!(summary.command, None);
    assert!(!summary.unseen);
    assert_eq!(summary.changed_at, 9);
    assert_eq!(summary.name, "reviewer");
    assert!(!state.revive_pane(pane).unwrap());

    assert!(state.update_pane_summary(pane, |summary| {
        summary.command = Some(zz_protocol::ShellCommand::Running);
    }));
    assert!(state.revive_pane(pane).unwrap());
    assert_eq!(state.pane(pane).unwrap().summary.command, None);
}
