use super::test_support::{allocator, sizes, three_horizontal};
use super::*;

#[test]
fn spread_biases_remainders_to_first_children() {
    let mut layout = three_horizontal();
    layout.resize(81, 24);
    layout
        .resize_pane_to(PaneId(0), Axis::Horizontal, 60)
        .unwrap();
    assert!(layout.spread(PaneId(2)).unwrap());
    assert_eq!(
        sizes(&layout, &[PaneId(0), PaneId(1), PaneId(2)]),
        [(27, 24), (26, 24), (26, 24)]
    );
}

#[test]
fn spread_skips_parents_with_node_children_like_the_pin() {
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
            Axis::Vertical,
            SplitSize::Default,
            false,
            false,
            PaneId(2),
            &mut ids,
        )
        .unwrap();
    layout
        .resize_pane_to(PaneId(0), Axis::Horizontal, 60)
        .unwrap();
    let before = sizes(&layout, &[PaneId(0), PaneId(1), PaneId(2)]);
    assert!(!layout.spread(PaneId(2)).unwrap());
    assert_eq!(sizes(&layout, &[PaneId(0), PaneId(1), PaneId(2)]), before);
}

#[test]
fn spread_refuses_a_mixed_parent_the_pin_corrupts() {
    let mut layout = three_horizontal();
    let mut ids = allocator(3);
    layout
        .split(
            PaneId(1),
            Axis::Vertical,
            SplitSize::Default,
            false,
            false,
            PaneId(3),
            &mut ids,
        )
        .unwrap();
    layout
        .resize_pane_to(PaneId(0), Axis::Horizontal, 10)
        .unwrap();
    let panes = [PaneId(0), PaneId(1), PaneId(3), PaneId(2)];
    let before = sizes(&layout, &panes);
    assert!(!layout.spread(PaneId(0)).unwrap());
    assert_eq!(sizes(&layout, &panes), before);
    layout.validate().unwrap();
}

#[test]
fn presets_match_exact_three_and_five_pane_geometry() {
    let panes3 = [PaneId(0), PaneId(1), PaneId(2)];
    let panes5 = [PaneId(0), PaneId(1), PaneId(2), PaneId(3), PaneId(4)];
    let expected3 = [
        [(26, 24), (26, 24), (26, 24)],
        [(80, 8), (80, 7), (80, 7)],
        [(80, 22), (40, 1), (39, 1)],
        [(80, 22), (40, 1), (39, 1)],
        [(78, 24), (1, 12), (1, 11)],
        [(78, 24), (1, 12), (1, 11)],
        [(39, 11), (40, 11), (80, 12)],
    ];
    let expected5 = [
        [(16, 24), (15, 24), (15, 24), (15, 24), (15, 24)],
        [(80, 4), (80, 4), (80, 4), (80, 4), (80, 4)],
        [(80, 22), (20, 1), (19, 1), (19, 1), (19, 1)],
        [(80, 22), (20, 1), (19, 1), (19, 1), (19, 1)],
        [(78, 24), (1, 6), (1, 5), (1, 5), (1, 5)],
        [(78, 24), (1, 6), (1, 5), (1, 5), (1, 5)],
        [(39, 7), (40, 7), (39, 7), (40, 7), (80, 8)],
    ];

    for (index, preset) in LayoutPreset::ALL.into_iter().enumerate() {
        let mut three = CellLayout::new(PaneId(0), 80, 24);
        let mut ids = allocator(1);
        three.apply_preset(preset, &panes3, &PresetOptions::default(), &mut ids);
        assert_eq!(
            sizes(&three, &panes3),
            expected3[index],
            "{}",
            preset.name()
        );

        let mut five = CellLayout::new(PaneId(0), 80, 24);
        let mut ids = allocator(1);
        five.apply_preset(preset, &panes5, &PresetOptions::default(), &mut ids);
        assert_eq!(sizes(&five, &panes5), expected5[index], "{}", preset.name());
    }
}

#[test]
fn preset_options_resolve_percentages_other_sizes_and_tiled_caps_at_apply_time() {
    let panes = [PaneId(0), PaneId(1), PaneId(2)];
    let mut layout = CellLayout::new(PaneId(0), 80, 24);
    let mut ids = allocator(1);
    let options = PresetOptions {
        main_pane_height: "50%".to_owned(),
        ..PresetOptions::default()
    };
    layout.apply_preset(LayoutPreset::MainHorizontal, &panes, &options, &mut ids);
    assert_eq!(sizes(&layout, &panes), [(80, 11), (40, 12), (39, 12)]);

    let mut ids = allocator(10);
    let options = PresetOptions {
        main_pane_height: "50%".to_owned(),
        other_pane_height: "5".to_owned(),
        ..PresetOptions::default()
    };
    layout.apply_preset(LayoutPreset::MainHorizontal, &panes, &options, &mut ids);
    assert_eq!(sizes(&layout, &panes), [(80, 18), (40, 5), (39, 5)]);

    let panes = [PaneId(0), PaneId(1), PaneId(2), PaneId(3), PaneId(4)];
    let mut ids = allocator(20);
    let options = PresetOptions {
        tiled_layout_max_columns: 1,
        ..PresetOptions::default()
    };
    layout.apply_preset(LayoutPreset::Tiled, &panes, &options, &mut ids);
    assert_eq!(sizes(&layout, &panes), [(80, 4); 5]);
}

#[test]
fn two_pane_main_presets_size_the_other_pane() {
    let panes = [PaneId(0), PaneId(1)];
    let cases = [
        (
            LayoutPreset::MainHorizontal,
            [(80, 22), (80, 1)],
            [PaneId(0), PaneId(1)],
        ),
        (
            LayoutPreset::MainHorizontalMirrored,
            [(80, 22), (80, 1)],
            [PaneId(1), PaneId(0)],
        ),
        (
            LayoutPreset::MainVertical,
            [(78, 24), (1, 24)],
            [PaneId(0), PaneId(1)],
        ),
        (
            LayoutPreset::MainVerticalMirrored,
            [(78, 24), (1, 24)],
            [PaneId(1), PaneId(0)],
        ),
    ];
    for (preset, expected_sizes, expected_order) in cases {
        let mut layout = CellLayout::new(PaneId(0), 80, 24);
        let mut ids = allocator(1);
        layout.apply_preset(preset, &panes, &PresetOptions::default(), &mut ids);
        assert_eq!(sizes(&layout, &panes), expected_sizes, "{}", preset.name());
        assert_eq!(layout.panes_in_order(), expected_order, "{}", preset.name());
    }
}
