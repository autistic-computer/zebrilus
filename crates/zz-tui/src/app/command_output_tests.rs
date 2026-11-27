use super::test_support::paned_model;
use super::*;
use zz_terminal::SearchDirection;

#[test]
fn command_output_begin_search_starts_the_matching_local_prompt() {
    let (mut model, pane) = paned_model();
    model.set_command_output(
        Some(1),
        Some((
            pane,
            TerminalViewport::blank(79, 22, zz_terminal::SessionStatus::Running),
        )),
    );
    model.command_output_swallowed_key = Some(zz_terminal::KeyCode::Escape);

    let action = command_output_ui_action(
        &mut model,
        pane,
        TerminalUiCommand::BeginSearch {
            direction: SearchDirection::Backward,
        },
    );
    assert!(matches!(
        action,
        Some(TerminalViewAction::SearchBegin(ref query))
            if query.direction == SearchDirection::Backward && query.text.is_empty()
    ));
    assert_eq!(
        model
            .command_output_search
            .as_ref()
            .map(|query| query.direction),
        Some(SearchDirection::Backward)
    );
    assert_eq!(model.command_output_swallowed_key, None);

    assert_eq!(
        command_output_ui_action(
            &mut model,
            PaneId(pane.0 + 1),
            TerminalUiCommand::BeginSearch {
                direction: SearchDirection::Forward,
            },
        ),
        None
    );
    assert_eq!(
        model
            .command_output_search
            .as_ref()
            .map(|query| query.direction),
        Some(SearchDirection::Backward)
    );
}

#[test]
fn command_output_resize_matches_rendered_content_and_tracks_geometry() {
    let (mut model, pane) = paned_model();
    let viewport = TerminalViewport::blank(79, 22, zz_terminal::SessionStatus::Running);
    assert_eq!(command_output_resize_message(&model), None);

    model.set_command_output(Some(1), Some((pane, viewport.clone())));
    let content = model.command_output_content_rect();
    let (geometry, message) = command_output_resize_message(&model).expect("open resize");
    assert_eq!(geometry.0, content.width);
    assert_eq!(geometry.1, content.height);
    assert!(matches!(
        message,
        InputMessage::ResizeCommandOutput {
            columns,
            rows,
            cell_width_px: 8,
            cell_height_px: 16,
        } if columns == content.width && rows == content.height
    ));

    model.last_sent_command_output_geometry = Some(geometry);
    assert_eq!(command_output_resize_message(&model), None);

    let mut size = model.size;
    size.columns = 91;
    size.rows = 30;
    model.set_size(size);
    let resized_content = model.command_output_content_rect();
    let (resized_geometry, resized_message) =
        command_output_resize_message(&model).expect("terminal resize");
    assert_eq!(resized_geometry.0, resized_content.width);
    assert_eq!(resized_geometry.1, resized_content.height);
    assert!(matches!(
        resized_message,
        InputMessage::ResizeCommandOutput { columns: 91, rows, .. }
            if rows == resized_content.height
    ));

    model.last_sent_command_output_geometry = Some(resized_geometry);
    assert!(model.set_status(zz_protocol::StatusLine {
        rows: vec!["one".to_owned(), "two".to_owned()],
        position: zz_protocol::StatusPosition::Bottom,
        ..zz_protocol::StatusLine::default()
    }));
    let status_content = model.command_output_content_rect();
    let (status_geometry, status_message) =
        command_output_resize_message(&model).expect("status resize");
    assert_eq!(status_geometry.1, status_content.height);
    assert!(matches!(
        status_message,
        InputMessage::ResizeCommandOutput { rows, .. } if rows == status_content.height
    ));

    model.last_sent_command_output_geometry = Some(status_geometry);
    model.set_command_output(None, None);
    assert_eq!(model.last_sent_command_output_geometry, None);
    assert_eq!(command_output_resize_message(&model), None);

    model.set_command_output(Some(2), Some((pane, viewport)));
    assert!(command_output_resize_message(&model).is_some());
}

#[test]
fn same_output_frames_dedupe_but_an_identical_replacement_resizes() {
    let (mut model, pane) = paned_model();
    let content = model.command_output_content_rect();
    let mut frame = TerminalViewport::blank(
        content.width,
        content.height,
        zz_terminal::SessionStatus::Running,
    );
    model.set_command_output(Some(1), Some((pane, frame.clone())));
    let (geometry, _) = command_output_resize_message(&model).expect("open resize");
    model.last_sent_command_output_geometry = Some(geometry);

    frame.generation = frame.generation.saturating_add(1);
    model.set_command_output(Some(1), Some((pane, frame.clone())));
    assert_eq!(command_output_resize_message(&model), None);

    model.set_command_output(Some(2), Some((pane, frame)));
    let (replacement_geometry, replacement) =
        command_output_resize_message(&model).expect("replacement resize");
    assert_eq!(replacement_geometry, geometry);
    assert!(matches!(
        replacement,
        InputMessage::ResizeCommandOutput { columns, rows, .. }
            if columns == geometry.0 && rows == geometry.1
    ));
}
