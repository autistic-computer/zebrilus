use super::*;

pub(super) fn collect_kitty_images(
    viewport: &TerminalViewport,
    cache: &dyn TerminalImageSource,
    local_scroll_target: Option<u32>,
    row_projection: RowProjection,
    origin: Point<Pixels>,
    grid_bounds: Bounds<Pixels>,
    cell_width: Pixels,
    line_height: Pixels,
    scale: f32,
    output: &mut PaintBuffers,
) {
    if scale <= 0.0 {
        return;
    }
    for placement in viewport.kitty_placements.iter() {
        let Some(image) = cache.image(placement.image_id, placement.image_generation) else {
            continue;
        };
        let Some(display_row) = kitty_display_row(placement, local_scroll_target, row_projection)
        else {
            continue;
        };
        if placement.pixel_width == 0
            || placement.pixel_height == 0
            || placement.grid_cols == 0
            || placement.grid_rows == 0
        {
            continue;
        }
        let destination = Bounds::new(
            point(
                origin.x
                    + px(f32::from(cell_width) * placement.viewport_col as f32)
                    + px(placement.cell_offset_x as f32 / scale),
                origin.y
                    + px(f32::from(line_height) * display_row as f32)
                    + px(placement.cell_offset_y as f32 / scale),
            ),
            size(
                px(f32::from(cell_width) * placement.grid_cols as f32),
                px(f32::from(line_height) * placement.grid_rows as f32),
            ),
        );
        let destination_aspect =
            f32::from(destination.size.width) / f32::from(destination.size.height);
        let shipped_aspect = placement.pixel_width as f32 / placement.pixel_height as f32;
        if !destination_aspect.is_finite()
            || destination_aspect <= 0.0
            || !shipped_aspect.is_finite()
            || shipped_aspect <= 0.0
        {
            continue;
        }
        let visible_bounds = destination.intersect(&grid_bounds);
        if visible_bounds.size.width <= px(0.0) || visible_bounds.size.height <= px(0.0) {
            continue;
        }
        let image_bounds = if let Some((source_x, source_y, source_width, source_height)) =
            placement.source_rect
        {
            let image_size = image.size(0);
            let Ok(image_width) = u32::try_from(image_size.width.0) else {
                continue;
            };
            let Ok(image_height) = u32::try_from(image_size.height.0) else {
                continue;
            };
            if source_width == 0
                || source_height == 0
                || source_x
                    .checked_add(source_width)
                    .is_none_or(|right| right > image_width)
                || source_y
                    .checked_add(source_height)
                    .is_none_or(|bottom| bottom > image_height)
            {
                continue;
            }
            let x_scale = f32::from(destination.size.width) / source_width as f32;
            let y_scale = f32::from(destination.size.height) / source_height as f32;
            Bounds::new(
                point(
                    destination.origin.x - px(source_x as f32 * x_scale),
                    destination.origin.y - px(source_y as f32 * y_scale),
                ),
                size(
                    px(image_width as f32 * x_scale),
                    px(image_height as f32 * y_scale),
                ),
            )
        } else {
            destination
        };
        let positioned = PositionedKittyImage {
            visible_bounds,
            image_bounds,
            image,
        };
        match placement.layer {
            KittyLayer::BelowBg => output.kitty_below_bg.push(positioned),
            KittyLayer::BelowText => output.kitty_below_text.push(positioned),
            KittyLayer::AboveText => output.kitty_above_text.push(positioned),
        }
    }
}

pub(super) fn paint_kitty_images(images: &mut Vec<PositionedKittyImage>, window: &mut Window) {
    for image in images.drain(..) {
        if let Err(error) = window.paint_image(
            image.visible_bounds,
            image.image_bounds,
            Corners::default(),
            image.image,
            0,
            false,
        ) {
            log::warn!("failed to paint Kitty terminal image: {error}");
        }
    }
}
