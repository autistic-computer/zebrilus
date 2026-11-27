use super::*;

#[test]
fn light_box_connectors_follow_only_connected_cell_edges() {
    assert_eq!(
        light_box_edges('─'),
        Some(BoxDrawingEdges::from_bits(
            BoxDrawingEdges::RIGHT | BoxDrawingEdges::LEFT
        ))
    );
    assert_eq!(
        light_box_edges('│'),
        Some(BoxDrawingEdges::from_bits(
            BoxDrawingEdges::UP | BoxDrawingEdges::DOWN
        ))
    );
    assert_eq!(
        light_box_edges('╮'),
        Some(BoxDrawingEdges::from_bits(
            BoxDrawingEdges::DOWN | BoxDrawingEdges::LEFT
        ))
    );
    assert_eq!(
        light_box_edges('╰'),
        Some(BoxDrawingEdges::from_bits(
            BoxDrawingEdges::UP | BoxDrawingEdges::RIGHT
        ))
    );
    assert_eq!(
        light_box_edges('┼'),
        Some(BoxDrawingEdges::from_bits(
            BoxDrawingEdges::UP
                | BoxDrawingEdges::RIGHT
                | BoxDrawingEdges::DOWN
                | BoxDrawingEdges::LEFT
        ))
    );
    for character in ['A', '┄', '┃', '═', '╱'] {
        assert_eq!(light_box_edges(character), None, "{character:?}");
    }
    assert!(matches!(
        light_box_drawing('─'),
        Some(LightBoxDrawing::Segments(_))
    ));
    assert!(matches!(
        light_box_drawing('╮'),
        Some(LightBoxDrawing::Rounded(_))
    ));
}

#[test]
fn light_box_diagonals_cover_the_three_diagonal_forms() {
    assert_eq!(
        light_box_diagonals('╲'),
        Some(BoxDiagonals::from_bits(BoxDiagonals::FALLING))
    );
    assert_eq!(
        light_box_diagonals('╱'),
        Some(BoxDiagonals::from_bits(BoxDiagonals::RISING))
    );
    assert_eq!(
        light_box_diagonals('╳'),
        Some(BoxDiagonals::from_bits(
            BoxDiagonals::RISING | BoxDiagonals::FALLING
        ))
    );
    for character in ['A', '─', '╮', '╌'] {
        assert_eq!(light_box_diagonals(character), None, "{character:?}");
    }
}

#[test]
fn box_connector_quads_reach_exact_cell_boundaries() {
    let connector = [CachedBoxConnector {
        column: 2,
        drawing: LightBoxDrawing::Segments(BoxDrawingEdges::from_bits(
            BoxDrawingEdges::UP
                | BoxDrawingEdges::RIGHT
                | BoxDrawingEdges::DOWN
                | BoxDrawingEdges::LEFT,
        )),
        color: Color::rgb(0xaa, 0xbb, 0xcc),
        alpha: 1.0,
    }];
    let mut output = Vec::new();
    push_box_drawing_connectors(
        &connector,
        px(10.0),
        px(20.0),
        px(8.0),
        px(20.0),
        px(1.0),
        2.0,
        &mut output,
    );

    assert_eq!(output.len(), 2);
    let TerminalGraphicPaint::Quad(vertical) = &output[0] else {
        panic!("straight vertical connector must be a quad");
    };
    let TerminalGraphicPaint::Quad(horizontal) = &output[1] else {
        panic!("straight horizontal connector must be a quad");
    };
    let stroke = px(1.0);
    assert_eq!(vertical.bounds.origin.y, px(20.0));
    assert_eq!(vertical.bounds.size.width, stroke);
    assert_eq!(vertical.bounds.bottom(), px(40.0));
    assert_eq!(horizontal.bounds.origin.x, px(26.0));
    assert_eq!(horizontal.bounds.right(), px(34.0));
    assert_eq!(horizontal.bounds.size.height, stroke);

    output.clear();
    push_box_drawing_connectors(
        &connector,
        px(10.0),
        px(20.0),
        px(16.0),
        px(40.0),
        px(1.5),
        2.0,
        &mut output,
    );
    let TerminalGraphicPaint::Quad(vertical) = &output[0] else {
        panic!("straight vertical connector must be a quad");
    };
    let TerminalGraphicPaint::Quad(horizontal) = &output[1] else {
        panic!("straight horizontal connector must be a quad");
    };
    assert_eq!(vertical.bounds.size.width, px(1.5));
    assert_eq!(vertical.bounds.bottom(), px(60.0));
    assert_eq!(horizontal.bounds.size.height, px(1.5));
    assert_eq!(horizontal.bounds.right(), px(58.0));
}

#[test]
fn rounded_box_connectors_are_single_paths_that_reach_cell_edges() {
    for (character, horizontal_edge, vertical_edge) in [
        ('╭', px(34.0), px(40.0)),
        ('╮', px(26.0), px(40.0)),
        ('╯', px(26.0), px(20.0)),
        ('╰', px(34.0), px(20.0)),
    ] {
        let connector = [CachedBoxConnector {
            column: 2,
            drawing: light_box_drawing(character).expect("rounded box drawing"),
            color: Color::rgb(0xaa, 0xbb, 0xcc),
            alpha: 1.0,
        }];
        let mut output = Vec::new();
        push_box_drawing_connectors(
            &connector,
            px(10.0),
            px(20.0),
            px(8.0),
            px(20.0),
            px(1.0),
            2.0,
            &mut output,
        );

        assert_eq!(output.len(), 1, "{character:?}");
        let TerminalGraphicPaint::Path { path, .. } = &output[0] else {
            panic!("rounded connector {character:?} must be a single path");
        };
        let near_edge = |actual: Pixels, expected: Pixels| {
            (f32::from(actual) - f32::from(expected)).abs() < 0.1
        };
        if matches!(character, '╭' | '╰') {
            assert!(
                near_edge(path.bounds.right(), horizontal_edge),
                "{character:?}: {:?}",
                path.bounds
            );
        } else {
            assert!(
                near_edge(path.bounds.origin.x, horizontal_edge),
                "{character:?}: {:?}",
                path.bounds
            );
        }
        if matches!(character, '╭' | '╮') {
            assert!(
                near_edge(path.bounds.bottom(), vertical_edge),
                "{character:?}: {:?}",
                path.bounds
            );
        } else {
            assert!(
                near_edge(path.bounds.origin.y, vertical_edge),
                "{character:?}: {:?}",
                path.bounds
            );
        }
    }
}

#[test]
fn box_stroke_rounds_the_face_underline_up_to_whole_device_pixels() {
    assert_eq!(box_stroke_width(px(1.3), 2.0), px(1.5));
    assert_eq!(box_stroke_width(px(1.3), 1.0), px(2.0));
    assert_eq!(box_stroke_width(px(0.0), 2.0), px(0.5));
    assert_eq!(box_stroke_width(px(0.1), 3.0), px(1.0 / 3.0));
    assert_eq!(box_stroke_width(px(1.0), 2.0), px(1.0));
}

#[test]
fn diagonal_box_connectors_are_centered_and_overlap_across_rows() {
    let paint_row = |row_y: Pixels| {
        let connector = [CachedBoxConnector {
            column: 2,
            drawing: light_box_drawing('╲').expect("diagonal box drawing"),
            color: Color::rgb(0xaa, 0xbb, 0xcc),
            alpha: 1.0,
        }];
        let mut output = Vec::new();
        push_box_drawing_connectors(
            &connector,
            px(10.0),
            row_y,
            px(8.0),
            px(20.0),
            px(1.0),
            2.0,
            &mut output,
        );
        assert_eq!(output.len(), 1);
        let TerminalGraphicPaint::Path { path, .. } = output.remove(0) else {
            panic!("diagonal connector must be a single path");
        };
        path.bounds
    };

    let first = paint_row(px(20.0));
    let cell_center = point(px(26.0 + 4.0), px(20.0 + 10.0));
    assert!((f32::from(first.center().x) - f32::from(cell_center.x)).abs() < 0.05);
    assert!((f32::from(first.center().y) - f32::from(cell_center.y)).abs() < 0.05);
    assert!(first.origin.x < px(26.0) && first.right() > px(34.0));

    let second = paint_row(px(40.0));
    assert!(first.bottom() > second.origin.y);
    assert_eq!(second.center().x, first.center().x);
}

#[test]
fn diagonal_cross_draws_both_strokes_in_one_path() {
    let connector = [CachedBoxConnector {
        column: 0,
        drawing: light_box_drawing('╳').expect("diagonal cross"),
        color: Color::rgb(0xaa, 0xbb, 0xcc),
        alpha: 1.0,
    }];
    let mut output = Vec::new();
    push_box_drawing_connectors(
        &connector,
        px(0.0),
        px(0.0),
        px(8.0),
        px(20.0),
        px(1.0),
        2.0,
        &mut output,
    );
    assert_eq!(output.len(), 1);
    let TerminalGraphicPaint::Path { path, .. } = &output[0] else {
        panic!("diagonal cross must be a single path");
    };
    assert!(path.bounds.origin.x < px(0.0) && path.bounds.right() > px(8.0));
    assert!(path.bounds.origin.y < px(0.0) && path.bounds.bottom() > px(20.0));
}

#[test]
fn solid_block_elements_fill_snapped_cell_bounds_without_gaps() {
    assert_eq!(
        solid_block_element('█'),
        Some(SolidBlockElement::Rect {
            left_eighths: 0,
            top_eighths: 0,
            right_eighths: 8,
            bottom_eighths: 8,
        })
    );
    assert_eq!(
        solid_block_element('▄'),
        Some(SolidBlockElement::Rect {
            left_eighths: 0,
            top_eighths: 4,
            right_eighths: 8,
            bottom_eighths: 8,
        })
    );
    assert_eq!(solid_block_element('▒'), None);

    let blocks = [2, 3].map(|column| CachedSolidBlock {
        column,
        element: solid_block_element('█').expect("full block"),
        color: Color::rgb(0xaa, 0xbb, 0xcc),
        alpha: 1.0,
    });
    let mut output = Vec::new();
    push_solid_blocks(
        &blocks,
        px(10.0),
        px(20.0),
        px(8.0),
        px(20.0),
        2.0,
        &mut output,
    );

    assert_eq!(output.len(), 2);
    let TerminalGraphicPaint::Quad(left) = &output[0] else {
        panic!("solid block must be a quad");
    };
    let TerminalGraphicPaint::Quad(right) = &output[1] else {
        panic!("solid block must be a quad");
    };
    assert_eq!(
        left.bounds,
        Bounds::new(point(px(26.0), px(20.0)), size(px(8.0), px(20.0)))
    );
    assert_eq!(left.bounds.right(), right.bounds.origin.x);
}
