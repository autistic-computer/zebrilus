use super::*;

#[test]
fn hit_grid_bounds_share_the_bottom_anchored_paint_origin() {
    let origin = point(px(4.0), px(13.0));
    let bounds = terminal_grid_bounds(origin, 80, 24, px(8.0), px(19.0));
    assert_eq!(bounds.origin, origin);
    assert_eq!(bounds.size, size(px(640.0), px(456.0)));
}

#[test]
fn fractional_scale_grid_tolerates_only_subdevice_rounding_noise() {
    for scale in [1.0, 1.25, 1.3, 1.5, 2.0] {
        let cell = snap_length(px(19.0), scale);
        let physical = f32::from(cell) * scale;
        assert!((physical - physical.round()).abs() < 0.0001);
        let exact = cell * 80;
        assert_eq!(grid_dimension(exact, cell, scale), 80);
        assert_eq!(
            grid_dimension(exact - device_pixel(scale) * 0.005, cell, scale),
            80
        );
        assert_eq!(
            grid_dimension(exact - device_pixel(scale) * 0.02, cell, scale),
            79
        );
    }
}

#[test]
fn grid_dimension_rejects_non_finite_geometry() {
    assert_eq!(grid_dimension(px(f32::NAN), px(8.0), 1.0), 1);
    assert_eq!(grid_dimension(px(f32::INFINITY), px(8.0), 1.0), 1);
    assert_eq!(grid_dimension(px(80.0), px(0.0), 1.0), 1);
}

#[test]
fn a_pane_box_measures_the_cells_it_wholly_contains() {
    let exact = terminal_grid_size(ala::size(px(640.0), px(384.0)), px(8.0), px(16.0), 1.0);
    assert_eq!(exact.columns, 80);
    assert_eq!(exact.rows, 24);
    assert_eq!(exact.cell_width_px, 8);
    assert_eq!(exact.cell_height_px, 16);

    let partial = terminal_grid_size(ala::size(px(647.0), px(399.0)), px(8.0), px(16.0), 1.0);
    assert_eq!(partial.columns, 80);
    assert_eq!(partial.rows, 24);

    let short = terminal_grid_size(ala::size(px(639.0), px(383.0)), px(8.0), px(16.0), 1.0);
    assert_eq!(short.columns, 79);
    assert_eq!(short.rows, 23);
}

#[test]
fn the_measurement_tolerates_a_device_pixel_and_scales_its_cells() {
    let scaled = terminal_grid_size(ala::size(px(640.0), px(384.0)), px(8.0), px(16.0), 2.0);
    assert_eq!(scaled.columns, 80);
    assert_eq!(scaled.rows, 24);
    assert_eq!(scaled.cell_width_px, 16);
    assert_eq!(scaled.cell_height_px, 32);

    let slack = terminal_grid_size(
        ala::size(px(640.0 - 0.001), px(384.0)),
        px(8.0),
        px(16.0),
        1.0,
    );
    assert_eq!(slack.columns, 80);

    let degenerate = terminal_grid_size(ala::size(px(0.0), px(0.0)), px(8.0), px(16.0), 1.0);
    assert_eq!(degenerate.columns, 1);
    assert_eq!(degenerate.rows, 1);
}
