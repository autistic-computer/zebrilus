use std::sync::Arc;

use super::test_support::{
    CLIPPED, attach_one_pane, block_model, tiled_under_float, waiting_pane_model,
};
use super::*;
use zz_terminal::{CellWidth, SessionStatus};

#[test]
fn a_terminal_pane_waiting_for_its_first_frame_paints_nothing() {
    let model = waiting_pane_model();
    let mut renderer = Renderer::new();
    renderer.paint_workspace(&model, true, false);
    let output = String::from_utf8(renderer.output).unwrap();
    assert_eq!(output, "");
}

fn erases(output: &str) -> usize {
    output
        .split("\x1b[")
        .skip(1)
        .filter(|sequence| {
            let digits = sequence.bytes().take_while(u8::is_ascii_digit).count();
            digits > 0 && sequence.as_bytes().get(digits) == Some(&b'X')
        })
        .count()
}

#[test]
fn blank_pane_rows_are_never_written_over_a_default_clear() {
    let mut model = waiting_pane_model();
    let pane = PaneId(3);
    let (send, receive) = std::sync::mpsc::channel();
    let mut renderer = Renderer::with_sink(Box::new(move |bytes| {
        send.send(bytes.to_vec()).unwrap();
        Ok(())
    }));
    let wait = std::time::Duration::from_secs(5);
    let read = || String::from_utf8(receive.recv_timeout(wait).unwrap()).unwrap();
    renderer.paint(&model, true).unwrap();
    let clear = read();
    assert!(clear.contains("\x1b[0m\x1b[2J"), "{clear:?}");
    assert_eq!(erases(&clear), 0, "{clear:?}");

    let content = model.layout.panes[0].content();
    let mut viewport =
        TerminalViewport::blank(content.width, content.height, SessionStatus::Running);
    let mut cells = viewport.cells.to_vec();
    cells[0] = PackedCell::new('x' as u32, 0, CellWidth::Narrow);
    viewport.cells = Arc::from(cells);
    model.viewports.insert(pane, viewport);
    renderer.note_frame(pane, FrameDamage::All);
    renderer.paint_frames(&model).unwrap();
    let first_frame = read();
    assert!(first_frame.contains('x'), "{first_frame:?}");
    assert_eq!(erases(&first_frame), 0, "{first_frame:?}");
    assert!(!first_frame.contains("\x1b[0m\x1b[0m"), "{first_frame:?}");

    renderer.invalidate();
    renderer.note_frame(pane, FrameDamage::All);
    renderer.paint_frames(&model).unwrap();
    let unknown = read();
    assert_eq!(
        erases(&unknown),
        usize::from(content.height),
        "rows over a screen the renderer no longer knows are erased: {unknown:?}"
    );
    assert!(!unknown.contains("\x1b[0m\x1b[0m"), "{unknown:?}");
}

#[test]
fn cells_no_painter_covers_keep_the_theme_background() {
    let mut model = waiting_pane_model();
    let mut snapshot = (*model.snapshot).clone();
    let window = snapshot.sessions[0].windows.first_mut().unwrap();
    window.panes.get_mut(&PaneId(3)).unwrap().mode = Some(zz_protocol::PaneMode::Clock {
        time: "12:00".to_owned(),
        colour: "blue".to_owned(),
    });
    model.update_snapshot(Arc::new(snapshot));
    let mut output = Vec::new();
    fill_unpainted_cells(&mut output, &model);
    let output = String::from_utf8(output).unwrap();
    let content = model.layout.panes[0].content();
    assert_eq!(erases(&output), usize::from(content.height), "{output:?}");
    let background = model.appearance.background;
    assert!(output.starts_with(&format!(
        "\x1b[48;2;{};{};{}m",
        background.r, background.g, background.b
    )));
}

#[test]
fn browser_frame_switches_the_renderer_between_card_and_live_modes() {
    let pane = PaneId(7);
    let mut renderer = Renderer::new();
    assert!(!renderer.browser_frame_live(pane));
    renderer.enable_kitty_graphics();
    renderer.install_browser_frame(BrowserFrameUpdate {
        image: KittyImageData {
            pane,
            image_id: BROWSER_IMAGE_ID,
            generation: 4,
            width: 2,
            height: 1,
            bytes: vec![0, 0, 255, 255, 0, 255, 0, 255],
        },
        placement: KittyPlacement {
            image_id: BROWSER_IMAGE_ID,
            image_generation: 4,
            layer: zz_terminal::KittyLayer::AboveText,
            viewport_col: 0,
            viewport_row: 0,
            absolute_row: 0,
            cell_offset_x: 0,
            cell_offset_y: 0,
            grid_cols: 2,
            grid_rows: 1,
            pixel_width: 2,
            pixel_height: 1,
            source_rect: None,
        },
    });

    assert!(renderer.browser_frame_live(pane));
    assert!(String::from_utf8_lossy(&renderer.queued_control).contains("\x1b_Ga=t"));
    renderer.resize_browser_frame(pane, (4, 3));
    assert_eq!(renderer.browser_placements[&pane].grid_cols, 4);
    assert_eq!(renderer.browser_placements[&pane].grid_rows, 3);

    renderer.remove_browser_frame(pane);
    assert!(!renderer.browser_frame_live(pane));
}

fn browser_under_float(float: Option<zz_protocol::FloatingPaneSnapshot>) -> Model {
    let browser = PaneId(7);
    let mut model = block_model(40, 12);
    attach_one_pane(&mut model, browser);
    let mut snapshot = (*model.snapshot).clone();
    let window = &mut snapshot.sessions[0].windows[0];
    let pane = |id: PaneId, kind: PaneKindSnapshot| zz_protocol::PaneSnapshot {
        id,
        title: String::new(),
        kind,
        synchronized_input: false,
        bell: false,
        dead: false,
        dead_status: None,
        border_colour: None,
        active_border_colour: None,
        border_status_text: String::new(),
        mode: None,
        status: None,
        summary: zz_protocol::PaneSummary::default(),
    };
    window.panes.insert(
        browser,
        pane(
            browser,
            PaneKindSnapshot::Browser(zz_protocol::BrowserDescriptor::single(
                "about:blank".to_owned(),
                "default".to_owned(),
            )),
        ),
    );
    if let Some(float) = float {
        window
            .panes
            .insert(float.pane, pane(float.pane, PaneKindSnapshot::Terminal));
        window.floating = vec![float];
    }
    model.update_snapshot(std::sync::Arc::new(snapshot));
    model
}

fn browser_placement_output(model: &Model) -> String {
    let mut renderer = Renderer::new();
    renderer.enable_kitty_graphics();
    renderer.install_browser_frame(BrowserFrameUpdate {
        image: KittyImageData {
            pane: PaneId(7),
            image_id: BROWSER_IMAGE_ID,
            generation: 1,
            width: 2,
            height: 1,
            bytes: vec![0, 0, 255, 255, 0, 255, 0, 255],
        },
        placement: KittyPlacement {
            image_id: BROWSER_IMAGE_ID,
            image_generation: 1,
            layer: zz_terminal::KittyLayer::AboveText,
            viewport_col: 0,
            viewport_row: 0,
            absolute_row: 0,
            cell_offset_x: 0,
            cell_offset_y: 0,
            grid_cols: 40,
            grid_rows: 11,
            pixel_width: 2,
            pixel_height: 1,
            source_rect: None,
        },
    });
    renderer.output.clear();
    renderer.reconcile_kitty_images(model);
    String::from_utf8_lossy(&renderer.output).into_owned()
}

fn placement_at(column: i32, columns: u32) -> KittyPlacement {
    KittyPlacement {
        image_id: 9,
        image_generation: 1,
        layer: zz_terminal::KittyLayer::AboveText,
        viewport_col: column,
        viewport_row: 0,
        absolute_row: 0,
        cell_offset_x: 0,
        cell_offset_y: 0,
        grid_cols: columns,
        grid_rows: 1,
        pixel_width: 2,
        pixel_height: 1,
        source_rect: None,
    }
}

fn clipped_float_placements(column: i32) -> String {
    let mut model = tiled_under_float(CLIPPED, None);
    let mut viewport = TerminalViewport::blank(10, 4, SessionStatus::Running);
    viewport.kitty_placements = std::sync::Arc::from([placement_at(column, 2)]);
    model.viewports.insert(PaneId(8), viewport);
    let mut renderer = Renderer::new();
    renderer.enable_kitty_graphics();
    renderer.install_kitty_image(KittyImageData {
        pane: PaneId(8),
        image_id: 9,
        generation: 1,
        width: 2,
        height: 1,
        bytes: vec![0, 0, 255, 255, 0, 255, 0, 255],
    });
    renderer.output.clear();
    renderer.reconcile_kitty_images(&model);
    String::from_utf8_lossy(&renderer.output).into_owned()
}

#[test]
fn a_clipped_float_places_only_the_images_it_still_shows_where_it_shows_them() {
    assert!(
        !clipped_float_placements(0).contains("\x1b_Ga=p"),
        "an image in the clipped-away columns is not placed"
    );
    assert!(clipped_float_placements(5).contains("\x1b_Ga=p"));
    assert_eq!(
        source_placements(&[placement_at(5, 2)], (3, 0))[0].viewport_col,
        2
    );
}

#[test]
fn a_float_paints_its_default_cells_in_its_window_style() {
    let float = zz_protocol::FloatingPaneSnapshot {
        xoff: 4,
        yoff: 2,
        sx: 6,
        sy: 2,
        ..CLIPPED
    };
    let paint = |window_style: &str, window_active_style: &str| {
        let mut model = tiled_under_float(float, None);
        let mut status = (*model.status).clone();
        status.pane_borders = vec![zz_protocol::PaneBorderPresentation {
            pane: PaneId(8),
            style: String::new(),
            window_style: window_style.to_owned(),
            window_active_style: window_active_style.to_owned(),
        }];
        model.status = std::sync::Arc::new(status);
        model.viewports.insert(
            PaneId(8),
            TerminalViewport::blank(6, 2, SessionStatus::Running),
        );
        let entry = model.pane_rect(PaneId(8)).expect("the float's rect");
        let mut renderer = Renderer::new();
        renderer.paint_entry(&model, &entry, true, false);
        (
            model.pane_window_style(PaneId(8)),
            String::from_utf8_lossy(&renderer.output).into_owned(),
        )
    };

    let (style, output) = paint("bg=themedarkgrey,fg=themewhite", "");
    assert_eq!(style.fg, Some(TmuxColour::Rgb(0x00e5_e5e5)));
    assert_eq!(style.bg, Some(TmuxColour::Rgb(0x0026_2626)));
    assert!(
        output.contains("\x1b[38;2;229;229;229m\x1b[48;2;38;38;38m      "),
        "{output:?}"
    );

    let (_, plain) = paint("", "");
    assert!(!plain.contains("\x1b[48;2;38;38;38m"), "{plain:?}");

    let (style, _) = paint("fg=blue,bg=red", "bg=green,fg=default");
    assert_eq!(style.fg, Some(TmuxColour::Basic(4)));
    assert_eq!(style.bg, Some(TmuxColour::Basic(2)));
}

#[test]
fn a_float_clipped_at_the_right_or_bottom_crops_its_pane_mode_instead_of_reflowing_it() {
    let clock = |float: zz_protocol::FloatingPaneSnapshot| {
        let mut model = tiled_under_float(float, None);
        let mut snapshot = (*model.snapshot).clone();
        snapshot.sessions[0].windows[0]
            .panes
            .get_mut(&PaneId(8))
            .unwrap()
            .mode = Some(zz_protocol::PaneMode::Clock {
            time: "12:00".to_owned(),
            colour: "blue".to_owned(),
        });
        model.update_snapshot(std::sync::Arc::new(snapshot));
        let entry = model.pane_rect(PaneId(8)).expect("the float's rect");
        let mut renderer = Renderer::new();
        renderer.paint_entry(&model, &entry, true, false);
        (
            entry.content(),
            entry.mode_rect(),
            String::from_utf8_lossy(&renderer.output).into_owned(),
        )
    };

    let right = zz_protocol::FloatingPaneSnapshot {
        xoff: 34,
        yoff: 2,
        sx: 10,
        sy: 5,
        ..CLIPPED
    };
    let (content, whole, output) = clock(right);
    assert_eq!((content.width, content.height), (6, 5));
    assert_eq!((whole.width, whole.height), (10, 5));
    assert!(output.contains("12:"), "{output:?}");
    assert!(
        !output.contains("12:00"),
        "the 10-column clock centres on the whole pane and loses its last columns: {output:?}"
    );

    let rows = tiled_under_float(right, None)
        .pane_rect(PaneId(7))
        .unwrap()
        .rect
        .height;
    let bottom = zz_protocol::FloatingPaneSnapshot {
        xoff: 4,
        yoff: i32::from(rows) - 2,
        sx: 10,
        sy: 5,
        ..CLIPPED
    };
    let (content, whole, output) = clock(bottom);
    assert_eq!(content.height, 2, "{content:?}");
    assert_eq!((whole.width, whole.height), (10, 5));
    assert!(
        !output.contains("12:00"),
        "the clock's row is below the window's last row: {output:?}"
    );
}

#[test]
fn a_browser_pane_a_float_covers_has_no_kitty_placement_while_covered() {
    let float = zz_protocol::FloatingPaneSnapshot {
        pane: PaneId(8),
        xoff: 5,
        yoff: 2,
        sx: 10,
        sy: 4,
        visible: true,
        border_lines: PaneBorderLines::Single,
        border_status: zz_protocol::PaneBorderStatus::Off,
    };
    let bare = browser_under_float(None);
    assert!(browser_placement_output(&bare).contains("\x1b_Ga=p"));

    let covered = browser_under_float(Some(float));
    assert_eq!(
        covered.layout.float(PaneId(8)).map(|float| float.frame),
        Some(Rect {
            x: 4,
            y: 1,
            width: 12,
            height: 6,
        })
    );
    assert!(!browser_placement_output(&covered).contains("\x1b_Ga=p"));

    let hidden = browser_under_float(Some(zz_protocol::FloatingPaneSnapshot {
        visible: false,
        ..float
    }));
    assert!(browser_placement_output(&hidden).contains("\x1b_Ga=p"));
}
