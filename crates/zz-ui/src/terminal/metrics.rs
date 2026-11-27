use super::*;

pub(super) fn measure_cell(
    font: &Font,
    font_size: Pixels,
    scale: f32,
    probe_color: Hsla,
    window: &Window,
) -> (Pixels, Pixels, ala::FontId) {
    let probe_run = TextRun {
        len: 1,
        font: font.clone(),
        color: probe_color,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let probe = window
        .text_system()
        .shape_line("m".into(), font_size, &[probe_run], None);
    let width = snap_length(
        if f32::from(probe.width) > 1.0 {
            probe.width
        } else {
            px(8.0)
        },
        scale,
    );
    let font_id = probe.runs.first().map_or_else(
        || window.text_system().resolve_font(font),
        |run| run.font_id,
    );
    let line_height =
        (probe.ascent + probe.descent + window.text_system().line_gap(font_id, font_size))
            .max(px(1.0));
    (width, line_height, font_id)
}

pub fn terminal_cell_size(
    font: &Font,
    font_size: Pixels,
    appearance: &TerminalAppearance,
    window: &Window,
) -> ala::Size<Pixels> {
    let scale = window.scale_factor();
    let (width, natural_line_height, _) =
        measure_cell(font, font_size, scale, Hsla::default(), window);
    let line_height = snap_length(
        px(appearance
            .cell_height_adjustment
            .apply(f32::from(natural_line_height))
            .max(1.0)),
        scale,
    );
    size(width, line_height)
}

pub fn terminal_grid_size(
    available: ala::Size<Pixels>,
    cell_width: Pixels,
    line_height: Pixels,
    scale: f32,
) -> GridSize {
    GridSize {
        columns: grid_dimension(available.width, cell_width, scale),
        rows: grid_dimension(available.height, line_height, scale),
        cell_width_px: physical_pixels(cell_width, scale),
        cell_height_px: physical_pixels(line_height, scale),
    }
}

pub(super) fn grid_dimension(available: Pixels, cell: Pixels, scale: f32) -> u16 {
    let tolerance = device_pixel(scale) * 0.01;
    let cells = ((available + tolerance) / cell).floor();
    if !cells.is_finite() {
        return 1;
    }
    cells.clamp(1.0, f32::from(u16::MAX)) as u16
}

pub(super) fn terminal_grid_bounds(
    origin: Point<Pixels>,
    columns: u16,
    rows: u16,
    cell_width: Pixels,
    line_height: Pixels,
) -> Bounds<Pixels> {
    Bounds::new(
        origin,
        size(
            cell_width * usize::from(columns),
            line_height * usize::from(rows),
        ),
    )
}

fn physical_pixels(value: Pixels, scale: f32) -> u32 {
    (f32::from(value) * scale)
        .round()
        .clamp(1.0, f32::from(u16::MAX)) as u32
}

pub(super) fn snap(value: Pixels, scale: f32) -> Pixels {
    px((f32::from(value) * scale).round() / scale)
}

pub(super) fn snap_length(value: Pixels, scale: f32) -> Pixels {
    snap(value.max(device_pixel(scale)), scale).max(device_pixel(scale))
}

pub(super) fn device_pixel(scale: f32) -> Pixels {
    px(1.0 / scale)
}
