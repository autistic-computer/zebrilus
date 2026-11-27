use super::*;

#[test]
fn copy_cursor_paints_a_full_row_beneath_existing_overlays() {
    let mut viewport = TerminalViewport::blank(80, 24, zz_terminal::SessionStatus::Running);
    viewport.overlays = Arc::from([
        zz_terminal::OverlaySpan::new(5, 2, 6, OverlayKind::Selection),
        zz_terminal::OverlaySpan::new(5, 4, 5, OverlayKind::CopyCursor),
    ]);
    let mut output = Vec::new();

    let _ = collect_overlays(
        &viewport,
        &TerminalAppearance::default(),
        RowProjection::new(24, 24, false),
        80,
        point(px(10.0), px(20.0)),
        px(8.0),
        px(19.0),
        1.0,
        &mut output,
        &mut Vec::new(),
    );

    assert_eq!(output.len(), 3);
    assert_eq!(
        output[0].bounds,
        Bounds::new(point(px(10.0), px(115.0)), size(px(640.0), px(19.0)))
    );
    assert_eq!(
        output[1].bounds,
        Bounds::new(point(px(26.0), px(115.0)), size(px(32.0), px(19.0)))
    );
    assert_eq!(
        output[2].bounds,
        Bounds::new(point(px(42.0), px(115.0)), size(px(8.0), px(19.0)))
    );
    assert!(
        copy_cursor_line_color(&TerminalAppearance::default()).a
            < overlay_color(OverlayKind::CopyCursor, &TerminalAppearance::default()).a
    );
}

#[test]
fn link_hover_reports_its_window_space_bounds() {
    let mut viewport = TerminalViewport::blank(20, 4, zz_terminal::SessionStatus::Running);
    viewport.overlays = Arc::from([zz_terminal::OverlaySpan::new(
        2,
        3,
        7,
        OverlayKind::LinkHover,
    )]);
    let mut output = Vec::new();

    let bounds = collect_overlays(
        &viewport,
        &TerminalAppearance::default(),
        RowProjection::new(4, 4, false),
        20,
        point(px(10.0), px(20.0)),
        px(8.0),
        px(19.0),
        1.0,
        &mut output,
        &mut Vec::new(),
    );

    assert_eq!(
        bounds,
        Some(Bounds::new(
            point(px(34.0), px(58.0)),
            size(px(32.0), px(19.0)),
        ))
    );
    assert_eq!(output.len(), 2, "hover fill and underline are painted");
}

#[test]
fn selected_ime_cells_are_masked_relative_to_the_selected_span() {
    let hidden = (3, 7..11);
    assert_eq!(
        selection_hidden_columns(3, 5..10, Some(&hidden)),
        Some(2..5)
    );
    assert_eq!(selection_hidden_columns(2, 5..10, Some(&hidden)), None);
    assert_eq!(selection_hidden_columns(3, 0..7, Some(&hidden)), None);
}

#[test]
fn linear_selection_rounds_only_exposed_row_corners() {
    let mut viewport = TerminalViewport::blank(10, 3, zz_terminal::SessionStatus::Running);
    viewport.overlays = Arc::from([
        zz_terminal::OverlaySpan::new(0, 2, 5, OverlayKind::Selection),
        zz_terminal::OverlaySpan::new(1, 2, 8, OverlayKind::Selection),
    ]);
    let radius = px(3.0);
    let first = selection_corners(&viewport, 0, radius);
    assert_eq!(first.top_left, radius);
    assert_eq!(first.top_right, radius);
    assert_eq!(first.bottom_left, px(0.0));
    assert_eq!(first.bottom_right, px(0.0));

    let second = selection_corners(&viewport, 1, radius);
    assert_eq!(second.top_left, px(0.0));
    assert_eq!(second.top_right, radius);
    assert_eq!(second.bottom_left, radius);
    assert_eq!(second.bottom_right, radius);
}
