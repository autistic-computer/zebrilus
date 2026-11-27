use super::*;

pub(super) fn push_box_drawing_connectors(
    connectors: &[CachedBoxConnector],
    row_x: Pixels,
    row_y: Pixels,
    cell_width: Pixels,
    line_height: Pixels,
    stroke: Pixels,
    scale: f32,
    output: &mut Vec<TerminalGraphicPaint>,
) {
    for connector in connectors {
        let left = snap(row_x + cell_width * connector.column, scale);
        let right = snap(row_x + cell_width * (connector.column + 1), scale);
        let top = snap(row_y, scale);
        let bottom = snap(row_y + line_height, scale);
        let width = right - left;
        let height = bottom - top;
        let horizontal_span = snap_length(width * 0.5, scale);
        let vertical_span = snap_length(height * 0.5, scale);
        let vertical_x = snap(left + (width - stroke) * 0.5, scale);
        let horizontal_y = snap(top + (height - stroke) * 0.5, scale);
        let paint = |bounds| fill(bounds, color(connector.color, connector.alpha));
        let cell = Bounds::new(point(left, top), size(width, height));

        let edges = match connector.drawing {
            LightBoxDrawing::Rounded(edges) => {
                if let Some(path) = rounded_box_drawing_path(edges, cell, stroke, scale) {
                    output.push(TerminalGraphicPaint::Path {
                        path,
                        color: color(connector.color, connector.alpha),
                    });
                }
                continue;
            }
            LightBoxDrawing::Diagonals(diagonals) => {
                if let Some(path) = diagonal_box_drawing_path(diagonals, cell, stroke, scale) {
                    output.push(TerminalGraphicPaint::Path {
                        path,
                        color: color(connector.color, connector.alpha),
                    });
                }
                continue;
            }
            LightBoxDrawing::Segments(edges) => edges,
        };

        if edges.connects(BoxDrawingEdges::UP) && edges.connects(BoxDrawingEdges::DOWN) {
            output.push(TerminalGraphicPaint::Quad(paint(Bounds::new(
                point(vertical_x, top),
                size(stroke, height),
            ))));
        } else if edges.connects(BoxDrawingEdges::UP) {
            output.push(TerminalGraphicPaint::Quad(paint(Bounds::new(
                point(vertical_x, top),
                size(stroke, vertical_span),
            ))));
        } else if edges.connects(BoxDrawingEdges::DOWN) {
            output.push(TerminalGraphicPaint::Quad(paint(Bounds::new(
                point(vertical_x, bottom - vertical_span),
                size(stroke, vertical_span),
            ))));
        }

        if edges.connects(BoxDrawingEdges::LEFT) && edges.connects(BoxDrawingEdges::RIGHT) {
            output.push(TerminalGraphicPaint::Quad(paint(Bounds::new(
                point(left, horizontal_y),
                size(width, stroke),
            ))));
        } else if edges.connects(BoxDrawingEdges::LEFT) {
            output.push(TerminalGraphicPaint::Quad(paint(Bounds::new(
                point(left, horizontal_y),
                size(horizontal_span, stroke),
            ))));
        } else if edges.connects(BoxDrawingEdges::RIGHT) {
            output.push(TerminalGraphicPaint::Quad(paint(Bounds::new(
                point(right - horizontal_span, horizontal_y),
                size(horizontal_span, stroke),
            ))));
        }
    }
}

fn rounded_box_drawing_path(
    edges: BoxDrawingEdges,
    bounds: Bounds<Pixels>,
    stroke: Pixels,
    scale: f32,
) -> Option<Path<Pixels>> {
    const CURVE_CONTROL_FRACTION: f32 = 0.25;

    let connects_up = edges.connects(BoxDrawingEdges::UP);
    let connects_right = edges.connects(BoxDrawingEdges::RIGHT);
    let connects_down = edges.connects(BoxDrawingEdges::DOWN);
    let connects_left = edges.connects(BoxDrawingEdges::LEFT);
    if connects_up == connects_down || connects_left == connects_right {
        return None;
    }

    let vertical_direction = if connects_down { 1.0 } else { -1.0 };
    let horizontal_direction = if connects_right { 1.0 } else { -1.0 };
    let top = snap(bounds.origin.y, scale);
    let bottom = snap(bounds.bottom(), scale);
    let left = snap(bounds.origin.x, scale);
    let right = snap(bounds.right(), scale);
    let width = right - left;
    let height = bottom - top;
    let center = point(
        snap(left + (width - stroke) * 0.5, scale) + stroke * 0.5,
        snap(top + (height - stroke) * 0.5, scale) + stroke * 0.5,
    );
    let radius = width.min(height) * 0.5;
    let vertical_edge = if connects_down { bottom } else { top };
    let horizontal_edge = if connects_right { right } else { left };

    let mut path = PathBuilder::stroke(stroke);
    path.move_to(point(center.x, vertical_edge));
    path.line_to(point(center.x, center.y + radius * vertical_direction));
    path.cubic_bezier_to(
        point(center.x + radius * horizontal_direction, center.y),
        point(
            center.x,
            center.y + radius * CURVE_CONTROL_FRACTION * vertical_direction,
        ),
        point(
            center.x + radius * CURVE_CONTROL_FRACTION * horizontal_direction,
            center.y,
        ),
    );
    path.line_to(point(horizontal_edge, center.y));
    match path.build() {
        Ok(path) => Some(path),
        Err(error) => {
            log::error!("failed to build rounded terminal box path: {error}");
            None
        }
    }
}

pub(super) fn box_stroke_width(underline_thickness: Pixels, scale: f32) -> Pixels {
    px((f32::from(underline_thickness) * scale).ceil().max(1.0) / scale)
}

fn diagonal_box_drawing_path(
    diagonals: BoxDiagonals,
    bounds: Bounds<Pixels>,
    stroke: Pixels,
    scale: f32,
) -> Option<Path<Pixels>> {
    let width = f32::from(bounds.size.width);
    let height = f32::from(bounds.size.height);
    if width <= 0.0 || height <= 0.0 {
        return None;
    }

    let device = 1.0 / scale;
    let overshoot_x = px(0.5 * device * (width / height).min(1.0));
    let overshoot_y = px(0.5 * device * (height / width).min(1.0));
    let left = bounds.origin.x - overshoot_x;
    let right = bounds.right() + overshoot_x;
    let top = bounds.origin.y - overshoot_y;
    let bottom = bounds.bottom() + overshoot_y;

    let mut path = PathBuilder::stroke(stroke);
    if diagonals.contains(BoxDiagonals::FALLING) {
        path.move_to(point(left, top));
        path.line_to(point(right, bottom));
    }
    if diagonals.contains(BoxDiagonals::RISING) {
        path.move_to(point(right, top));
        path.line_to(point(left, bottom));
    }
    match path.build() {
        Ok(path) => Some(path),
        Err(error) => {
            log::error!("failed to build diagonal terminal box path: {error}");
            None
        }
    }
}

pub(super) fn push_solid_blocks(
    blocks: &[CachedSolidBlock],
    row_x: Pixels,
    row_y: Pixels,
    cell_width: Pixels,
    line_height: Pixels,
    scale: f32,
    output: &mut Vec<TerminalGraphicPaint>,
) {
    for block in blocks {
        let left = snap(row_x + cell_width * block.column, scale);
        let right = snap(row_x + cell_width * (block.column + 1), scale);
        let top = snap(row_y, scale);
        let bottom = snap(row_y + line_height, scale);
        let width = right - left;
        let height = bottom - top;
        let paint_rect =
            |left_eighths: u8, top_eighths: u8, right_eighths: u8, bottom_eighths: u8| {
                let x0 = snap(left + width * (f32::from(left_eighths) / 8.0), scale);
                let y0 = snap(top + height * (f32::from(top_eighths) / 8.0), scale);
                let x1 = snap(left + width * (f32::from(right_eighths) / 8.0), scale);
                let y1 = snap(top + height * (f32::from(bottom_eighths) / 8.0), scale);
                TerminalGraphicPaint::Quad(fill(
                    Bounds::new(point(x0, y0), size(x1 - x0, y1 - y0)),
                    color(block.color, block.alpha),
                ))
            };

        match block.element {
            SolidBlockElement::Rect {
                left_eighths,
                top_eighths,
                right_eighths,
                bottom_eighths,
            } => output.push(paint_rect(
                left_eighths,
                top_eighths,
                right_eighths,
                bottom_eighths,
            )),
            SolidBlockElement::Quadrants(quadrants) => {
                for (flag, rect) in [
                    (SolidBlockElement::TOP_LEFT, (0, 0, 4, 4)),
                    (SolidBlockElement::TOP_RIGHT, (4, 0, 8, 4)),
                    (SolidBlockElement::BOTTOM_LEFT, (0, 4, 4, 8)),
                    (SolidBlockElement::BOTTOM_RIGHT, (4, 4, 8, 8)),
                ] {
                    if quadrants & flag != 0 {
                        output.push(paint_rect(rect.0, rect.1, rect.2, rect.3));
                    }
                }
            }
        }
    }
}

pub(super) fn connected_light_box_drawing(glyph: Glyph<'_>) -> Option<LightBoxDrawing> {
    single_cell_character(glyph).and_then(light_box_drawing)
}

pub(super) fn connected_solid_block_element(glyph: Glyph<'_>) -> Option<SolidBlockElement> {
    single_cell_character(glyph).and_then(solid_block_element)
}

fn single_cell_character(glyph: Glyph<'_>) -> Option<char> {
    match glyph {
        Glyph::Scalar(character) => Some(character),
        Glyph::Grapheme(text) => {
            let mut characters = text.chars();
            let character = characters.next()?;
            characters.next().is_none().then_some(character)
        }
        Glyph::Empty => None,
    }
}

pub(super) fn light_box_drawing(character: char) -> Option<LightBoxDrawing> {
    if let Some(diagonals) = light_box_diagonals(character) {
        return Some(LightBoxDrawing::Diagonals(diagonals));
    }
    let edges = light_box_edges(character)?;
    Some(if matches!(character, '╭' | '╮' | '╯' | '╰') {
        LightBoxDrawing::Rounded(edges)
    } else {
        LightBoxDrawing::Segments(edges)
    })
}

pub(super) fn light_box_diagonals(character: char) -> Option<BoxDiagonals> {
    let bits = match character {
        '╱' => BoxDiagonals::RISING,
        '╲' => BoxDiagonals::FALLING,
        '╳' => BoxDiagonals::RISING | BoxDiagonals::FALLING,
        _ => return None,
    };
    Some(BoxDiagonals::from_bits(bits))
}

pub(super) fn light_box_edges(character: char) -> Option<BoxDrawingEdges> {
    let bits = match character {
        '─' => BoxDrawingEdges::RIGHT | BoxDrawingEdges::LEFT,
        '│' => BoxDrawingEdges::UP | BoxDrawingEdges::DOWN,
        '┌' | '╭' => BoxDrawingEdges::RIGHT | BoxDrawingEdges::DOWN,
        '┐' | '╮' => BoxDrawingEdges::DOWN | BoxDrawingEdges::LEFT,
        '└' | '╰' => BoxDrawingEdges::UP | BoxDrawingEdges::RIGHT,
        '┘' | '╯' => BoxDrawingEdges::UP | BoxDrawingEdges::LEFT,
        '├' => BoxDrawingEdges::UP | BoxDrawingEdges::RIGHT | BoxDrawingEdges::DOWN,
        '┤' => BoxDrawingEdges::UP | BoxDrawingEdges::DOWN | BoxDrawingEdges::LEFT,
        '┬' => BoxDrawingEdges::RIGHT | BoxDrawingEdges::DOWN | BoxDrawingEdges::LEFT,
        '┴' => BoxDrawingEdges::UP | BoxDrawingEdges::RIGHT | BoxDrawingEdges::LEFT,
        '┼' => {
            BoxDrawingEdges::UP
                | BoxDrawingEdges::RIGHT
                | BoxDrawingEdges::DOWN
                | BoxDrawingEdges::LEFT
        }
        '╴' => BoxDrawingEdges::LEFT,
        '╵' => BoxDrawingEdges::UP,
        '╶' => BoxDrawingEdges::RIGHT,
        '╷' => BoxDrawingEdges::DOWN,
        _ => return None,
    };
    Some(BoxDrawingEdges::from_bits(bits))
}

pub(super) fn solid_block_element(character: char) -> Option<SolidBlockElement> {
    let codepoint = character as u32;
    let rect = |left_eighths, top_eighths, right_eighths, bottom_eighths| SolidBlockElement::Rect {
        left_eighths,
        top_eighths,
        right_eighths,
        bottom_eighths,
    };

    match codepoint {
        0x2580 => Some(rect(0, 0, 8, 4)),
        0x2581..=0x2587 => {
            let height = u8::try_from(codepoint - 0x2580).ok()?;
            Some(rect(0, 8 - height, 8, 8))
        }
        0x2588 => Some(rect(0, 0, 8, 8)),
        0x2589..=0x258f => {
            let width = u8::try_from(0x2590 - codepoint).ok()?;
            Some(rect(0, 0, width, 8))
        }
        0x2590 => Some(rect(4, 0, 8, 8)),
        0x2594 => Some(rect(0, 0, 8, 1)),
        0x2595 => Some(rect(7, 0, 8, 8)),
        0x2596 => Some(SolidBlockElement::Quadrants(SolidBlockElement::BOTTOM_LEFT)),
        0x2597 => Some(SolidBlockElement::Quadrants(
            SolidBlockElement::BOTTOM_RIGHT,
        )),
        0x2598 => Some(SolidBlockElement::Quadrants(SolidBlockElement::TOP_LEFT)),
        0x2599 => Some(SolidBlockElement::Quadrants(
            SolidBlockElement::TOP_LEFT
                | SolidBlockElement::BOTTOM_LEFT
                | SolidBlockElement::BOTTOM_RIGHT,
        )),
        0x259a => Some(SolidBlockElement::Quadrants(
            SolidBlockElement::TOP_LEFT | SolidBlockElement::BOTTOM_RIGHT,
        )),
        0x259b => Some(SolidBlockElement::Quadrants(
            SolidBlockElement::TOP_LEFT
                | SolidBlockElement::TOP_RIGHT
                | SolidBlockElement::BOTTOM_LEFT,
        )),
        0x259c => Some(SolidBlockElement::Quadrants(
            SolidBlockElement::TOP_LEFT
                | SolidBlockElement::TOP_RIGHT
                | SolidBlockElement::BOTTOM_RIGHT,
        )),
        0x259d => Some(SolidBlockElement::Quadrants(SolidBlockElement::TOP_RIGHT)),
        0x259e => Some(SolidBlockElement::Quadrants(
            SolidBlockElement::TOP_RIGHT | SolidBlockElement::BOTTOM_LEFT,
        )),
        0x259f => Some(SolidBlockElement::Quadrants(
            SolidBlockElement::TOP_RIGHT
                | SolidBlockElement::BOTTOM_LEFT
                | SolidBlockElement::BOTTOM_RIGHT,
        )),
        _ => None,
    }
}
