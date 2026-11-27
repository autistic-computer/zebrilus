use super::test_support::{allocator, geometry};
use super::*;

fn checksummed(body: &str) -> String {
    format!("{:04x},{body}", layout_checksum(body.as_bytes()))
}

fn nested_layout(depth: usize) -> (String, u16) {
    if depth == 0 {
        return ("1x1,0,0,0".to_owned(), 1);
    }
    let (child, child_width) = nested_layout(depth - 1);
    let width = child_width + 2;
    (format!("{width}x1,0,0{{{child},1x1,0,0,0}}"), width)
}

#[test]
fn dump_checksum_and_identity_mutations_work() {
    let mut layout = CellLayout::new(PaneId(0), 80, 24);
    assert_eq!(layout.dump(), "b25d,80x24,0,0,0");
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
    assert!(layout.swap(PaneId(0), PaneId(1)));
    assert_eq!(layout.panes_in_order(), [PaneId(1), PaneId(0)]);
    assert!(layout.replace(PaneId(1), PaneId(7)));
    assert_eq!(layout.panes_in_order(), [PaneId(7), PaneId(0)]);
    assert!(!layout.replace(PaneId(99), PaneId(8)));

    let mut maximum = CellLayout::new(PaneId(0), u16::MAX, u16::MAX);
    assert_eq!(maximum.extent(), (PANE_MAXIMUM, PANE_MAXIMUM));
    maximum.resize(u16::MAX, u16::MAX);
    assert_eq!(maximum.extent(), (PANE_MAXIMUM, PANE_MAXIMUM));
    assert_eq!(CellLayout::new(PaneId(0), 0, 0).extent(), (1, 1));
}

fn parse_error(input: &str) -> String {
    let mut parsed = match CellLayout::parse(input) {
        Ok(parsed) => parsed,
        Err(error) => return error.cause().to_owned(),
    };
    parsed
        .check_sizes()
        .map_or_else(|error| error.cause().to_owned(), |()| String::new())
}

#[test]
fn parsed_layout_corrects_the_root_and_assigns_dfs_panes() {
    let input = checksummed("999x999,9,9{40x24,8,8,111,39x24,9,9,222}");
    let mut parsed = CellLayout::parse(&input).unwrap();
    parsed.check_sizes().unwrap();
    assert_eq!(parsed.pane_count(), 2);
    let mut ids = allocator(20);
    let (layout, selection) = parsed.into_layout(&[PaneId(7), PaneId(8)], &mut ids);
    assert_eq!(selection, None);
    assert_eq!(
        layout.dump(),
        checksummed("80x24,0,0{40x24,0,0,7,39x24,41,0,8}")
    );
    let LayoutNode::Split { id, .. } = layout.project() else {
        panic!("expected parsed split");
    };
    assert_eq!(id, SplitId(20));
}

#[test]
fn parsed_layout_rejects_checksum_grammar_and_size_errors_like_the_pin() {
    assert_eq!(parse_error("0000,80x24,0,0,0"), "invalid layout checksum");
    let parsed = CellLayout::parse("B25D,80x24,0,0,0").unwrap();
    let mut ids = allocator(20);
    assert_eq!(
        parsed.into_layout(&[PaneId(7)], &mut ids).0.dump(),
        checksummed("80x24,0,0,7")
    );
    assert!(CellLayout::parse(" \t b25d,80x24,0,0,0").is_ok());
    for malformed in [
        "b25,80x24,0,0,0",
        "0b25d,80x24,0,0,0",
        "0xb25d,80x24,0,0,0",
        "",
    ] {
        assert_eq!(
            parse_error(malformed),
            "malformed layout header",
            "{malformed}"
        );
    }
    assert_eq!(
        parse_error(&checksummed("80x24,0,0,0garbage")),
        "invalid layout"
    );
    assert_eq!(parse_error(&checksummed("80x24,0,0,0,5")), "trailing data");
    assert_eq!(parse_error(&checksummed("80x24,0,0,0]")), "trailing data");
    assert_eq!(parse_error(&checksummed("0x24,0,0,0")), "invalid layout");
    assert_eq!(
        parse_error(&checksummed("80x10001,0,0,0")),
        "invalid layout"
    );
    assert_eq!(
        parse_error(&checksummed("80x24,10001,0,0")),
        "invalid layout"
    );
    assert_eq!(
        parse_error(&checksummed("80x24,0,0{80x24,0,0,0}")),
        "invalid layout"
    );
    assert_eq!(
        parse_error(&checksummed("80x24,0,0{40x23,0,0,0,39x24,41,0,1}")),
        "size mismatch after applying layout"
    );
}

#[test]
fn parsed_layout_leaf_pane_numbers_are_optional() {
    let with_ids = CellLayout::parse("8205,80x24,0,0{40x24,0,0,0,39x24,41,0,1}").unwrap();
    let without_ids = CellLayout::parse("347e,80x24,0,0{40x24,0,0,39x24,41,0}").unwrap();
    let panes = [PaneId(7), PaneId(8)];
    let mut with_ids_allocator = allocator(20);
    let mut without_ids_allocator = allocator(20);
    assert_eq!(
        with_ids.into_layout(&panes, &mut with_ids_allocator),
        without_ids.into_layout(&panes, &mut without_ids_allocator)
    );
}

#[test]
fn parsed_layout_trims_the_bottom_right_cell_with_tmux_gifting() {
    let input = checksummed("100x20,0,0[100x9,0,0{49x9,0,0,50,50x9,50,0,51},100x10,0,10,52]");
    let mut parsed = CellLayout::parse(&input).unwrap();
    assert_eq!(parsed.pane_count(), 3);
    parsed.trim_bottom_right();
    assert_eq!(parsed.pane_count(), 2);
    let mut ids = allocator(30);
    let layout = parsed.into_layout(&[PaneId(7), PaneId(8)], &mut ids).0;
    assert_eq!(
        layout.dump(),
        checksummed("100x20,0,0{49x20,0,0,7,50x20,50,0,8}")
    );
    let LayoutNode::Split { id, .. } = layout.project() else {
        panic!("expected trimmed split");
    };
    assert_eq!(id, SplitId(30));
}

#[test]
fn parsed_layout_enforces_the_protocol_depth_budget() {
    let accepted = nested_layout(MAX_LAYOUT_DEPTH - 1).0;
    assert!(CellLayout::parse(&checksummed(&accepted)).is_ok());
    let rejected = nested_layout(MAX_LAYOUT_DEPTH).0;
    assert_eq!(parse_error(&checksummed(&rejected)), "invalid layout");
}

#[test]
fn validate_rejects_corrupt_tree() {
    let layout = CellLayout {
        root: CellNode::Node {
            axis: Axis::Horizontal,
            geometry: CellGeometry {
                sx: 80,
                sy: 24,
                xoff: 0,
                yoff: 0,
            },
            children: vec![
                CellChild {
                    divider: None,
                    node: leaf(PaneId(0), 40, 24),
                },
                CellChild {
                    divider: None,
                    node: leaf(PaneId(1), 39, 24),
                },
            ],
        },
        extent: (80, 24),
    };
    assert_eq!(
        layout.validate(),
        Err("non-first child is missing a divider".to_owned())
    );
    let zero_leaf = CellLayout {
        root: leaf(PaneId(0), 0, 1),
        extent: (1, 1),
    };
    assert_eq!(
        zero_leaf.validate(),
        Err("leaf extent is below the pane minimum".to_owned())
    );
    assert_eq!(geometry(&CellLayout::new(PaneId(5), 1, 1), 5).sx, 1);
}
