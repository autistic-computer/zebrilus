#[cfg(unix)]
use super::test_support::*;
use super::*;
#[cfg(unix)]
use crate::DEFAULT_HISTORY_LIMIT;

fn test_base64(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let bits = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        output.push(char::from(TABLE[((bits >> 18) & 0x3f) as usize]));
        output.push(char::from(TABLE[((bits >> 12) & 0x3f) as usize]));
        output.push(if chunk.len() > 1 {
            char::from(TABLE[((bits >> 6) & 0x3f) as usize])
        } else {
            '='
        });
        output.push(if chunk.len() > 2 {
            char::from(TABLE[(bits & 0x3f) as usize])
        } else {
            '='
        });
    }
    output
}

#[test]
fn kitty_pixel_formats_convert_to_premultiplied_bgra() {
    assert_eq!(
        kitty_to_premultiplied_bgra(&[0x80], ImageFormat::Gray, 4),
        [0x80, 0x80, 0x80, 0xff]
    );
    assert_eq!(
        kitty_to_premultiplied_bgra(&[200, 128], ImageFormat::GrayAlpha, 4),
        [100, 100, 100, 128]
    );
    assert_eq!(
        kitty_to_premultiplied_bgra(&[1, 2, 3], ImageFormat::Rgb, 4),
        [3, 2, 1, 0xff]
    );
    assert_eq!(
        kitty_to_premultiplied_bgra(&[100, 50, 200, 128], ImageFormat::Rgba, 4),
        [100, 25, 50, 128]
    );
}

#[test]
fn kitty_vt_write_exports_placement_metadata_and_owned_pixels() {
    let mut terminal = new_terminal(8, 2, 16).expect("terminal");
    terminal.resize(8, 2, 8, 18).expect("terminal pixels");
    terminal.vt_write(b"\x1b_Ga=T,f=24,s=1,v=1,i=42;/wAA\x1b\\");

    let mut render_state = RenderState::new().expect("render state");
    let mut rows = RowIterator::new().expect("rows");
    let mut cells = CellIterator::new().expect("cells");
    let mut generations = ViewportGenerations::new().expect("Kitty iterator");
    let mut dictionary = ViewportDictionary::default();
    let viewport = snapshot(
        &terminal,
        &mut render_state,
        &mut rows,
        &mut cells,
        &mut generations,
        SnapshotChange::Content,
        &mut dictionary,
        None,
        SessionStatus::Running,
        None,
    )
    .expect("Kitty snapshot");
    let placement = viewport
        .kitty_placements
        .first()
        .expect("displayed Kitty placement");
    assert_eq!(placement.image_id, 42);
    assert_eq!(placement.viewport_col, 0);
    assert_eq!(placement.viewport_row, 0);
    assert_eq!(placement.absolute_row, u64::from(viewport.scrollbar.offset));
    assert_eq!(placement.layer, KittyLayer::AboveText);

    let image = generations
        .kitty
        .as_mut()
        .expect("Kitty state")
        .image(&terminal, 42)
        .expect("image query")
        .expect("stored image");
    assert_eq!((image.width, image.height), (1, 1));
    assert_eq!(image.generation, placement.image_generation);
    assert_eq!(image.bgra, [0, 0, 255, 255]);
}

#[test]
fn kitty_per_cell_image_placements_cover_large_previews() {
    let columns = 32_u16;
    let row_count = 17_u16;
    let mut terminal = new_terminal(columns, row_count, 16).expect("terminal");
    terminal
        .resize(columns, row_count, 8, 18)
        .expect("terminal pixels");
    let encoded = "/wAA".repeat(usize::from(columns) * usize::from(row_count));
    terminal.vt_write(
        format!("\x1b_Gq=2,a=t,f=24,s={columns},v={row_count},i=42;{encoded}\x1b\\").as_bytes(),
    );
    for row in 0..row_count {
        for column in 0..columns {
            let placement_id = u32::from(row) * u32::from(columns) + u32::from(column) + 1;
            terminal.vt_write(
                format!(
                    "\x1b[{};{}H\x1b_Gq=2,a=p,i=42,p={placement_id},x={column},y={row},w=1,h=1,c=1,r=1,z=-1,C=1\x1b\\",
                    row + 1,
                    column + 1,
                )
                .as_bytes(),
            );
        }
    }

    let mut render_state = RenderState::new().expect("render state");
    let mut rows = RowIterator::new().expect("rows");
    let mut cells = CellIterator::new().expect("cells");
    let mut generations = ViewportGenerations::new().expect("Kitty iterator");
    let mut dictionary = ViewportDictionary::default();
    let viewport = snapshot(
        &terminal,
        &mut render_state,
        &mut rows,
        &mut cells,
        &mut generations,
        SnapshotChange::Content,
        &mut dictionary,
        None,
        SessionStatus::Running,
        None,
    )
    .expect("Kitty snapshot");
    let covered = viewport
        .kitty_placements
        .iter()
        .map(|placement| {
            assert_eq!(placement.image_id, 42);
            assert_eq!(placement.layer, KittyLayer::BelowText);
            assert_eq!((placement.grid_cols, placement.grid_rows), (1, 1));
            assert_eq!(
                placement.source_rect,
                Some((
                    u32::try_from(placement.viewport_col).expect("visible column"),
                    u32::try_from(placement.viewport_row).expect("visible row"),
                    1,
                    1,
                )),
            );
            (placement.viewport_col, placement.viewport_row)
        })
        .collect::<HashSet<_>>();
    assert_eq!(
        covered.len(),
        usize::from(columns) * usize::from(row_count),
        "every image cell must survive viewport extraction",
    );
    for row in 0..row_count {
        for column in 0..columns {
            assert!(covered.contains(&(i32::from(column), i32::from(row))));
        }
    }
}

#[test]
fn kitty_storage_quota_admits_wire_cap_sized_images() {
    let mut terminal = new_terminal(8, 2, 16).expect("terminal");
    terminal.resize(8, 2, 8, 18).expect("terminal pixels");
    configure_kitty_storage(&mut terminal).expect("raise Kitty storage quota");

    let encoded = "AAAA".repeat(2000 * 2000);
    let chunks = encoded.as_bytes().chunks(4096).collect::<Vec<_>>();
    for (index, chunk) in chunks.iter().enumerate() {
        let more = usize::from(index < chunks.len() - 1);
        let mut apc = if index == 0 {
            format!("\x1b_Ga=T,f=24,s=2000,v=2000,i=91,m={more};").into_bytes()
        } else {
            format!("\x1b_Gm={more};").into_bytes()
        };
        apc.extend_from_slice(chunk);
        apc.extend_from_slice(b"\x1b\\");
        terminal.vt_write(&apc);
    }

    let mut render_state = RenderState::new().expect("render state");
    let mut rows = RowIterator::new().expect("rows");
    let mut cells = CellIterator::new().expect("cells");
    let mut generations = ViewportGenerations::new().expect("Kitty iterator");
    let mut dictionary = ViewportDictionary::default();
    let viewport = snapshot(
        &terminal,
        &mut render_state,
        &mut rows,
        &mut cells,
        &mut generations,
        SnapshotChange::Content,
        &mut dictionary,
        None,
        SessionStatus::Running,
        None,
    )
    .expect("Kitty snapshot");
    assert!(
        viewport
            .kitty_placements
            .iter()
            .any(|placement| placement.image_id == 91),
        "a 12 MB image should fit the raised Kitty storage quota"
    );
}

#[test]
fn kitty_png_decoder_installs_through_the_safe_wrapper() {
    use image::ImageEncoder as _;

    install_kitty_png_decoder();
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(&[255, 0, 0, 128], 1, 1, image::ExtendedColorType::Rgba8)
        .expect("encode PNG fixture");
    let payload = test_base64(&png);
    let mut terminal = new_terminal(8, 2, 16).expect("terminal");
    terminal.vt_write(format!("\x1b_Ga=T,f=100,i=43;{payload}\x1b\\").as_bytes());

    let mut kitty = KittyGraphicsState::new().expect("Kitty iterator");
    let image = kitty
        .image(&terminal, 43)
        .expect("image query")
        .expect("decoded PNG image");
    assert_eq!((image.width, image.height), (1, 1));
    assert_eq!(image.bgra, [0, 0, 128, 128]);
}

#[cfg(unix)]
#[test]
fn live_session_decodes_and_places_a_png_transmission() {
    let session = TerminalSession::spawn(
        DEFAULT_HISTORY_LIMIT,
        Arc::new(TerminalAppearance::default()),
        TerminalSpawn::default(),
    );
    let view = TerminalViewId(11);
    attach_streaming(&session, view);
    session.resize(100, 30, 8, 18);
    session.send_text(
        "printf '\\033_Ga=T,f=100,i=78;iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAIAAAD91Jpz\
             AAAAEklEQVR4nGP4z8DAAMIM/4EAAB/uBfsL2WiLAAAAAElFTkSuQmCC\\033\\\\'\n",
    );

    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    loop {
        while session.events().try_recv().is_ok() {}
        let placed = session.latest_viewport_for(view).is_some_and(|viewport| {
            viewport
                .kitty_placements
                .iter()
                .any(|placement| placement.image_id == 78)
        });
        if placed {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "PNG transmission never produced a Kitty placement"
        );
        thread::sleep(Duration::from_millis(20));
    }
}

#[cfg(unix)]
#[test]
fn live_session_publishes_kitty_placements() {
    let session = TerminalSession::spawn(
        DEFAULT_HISTORY_LIMIT,
        Arc::new(TerminalAppearance::default()),
        TerminalSpawn::default(),
    );
    let view = TerminalViewId(9);
    attach_streaming(&session, view);
    session.resize(100, 30, 8, 18);
    session.send_text("printf '\\033_Ga=T,f=24,s=1,v=1,i=77;/wAA\\033\\\\'\n");

    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    loop {
        while session.events().try_recv().is_ok() {}
        let placed = session.latest_viewport_for(view).is_some_and(|viewport| {
            viewport
                .kitty_placements
                .iter()
                .any(|placement| placement.image_id == 77)
        });
        if placed {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "live session never published a Kitty placement"
        );
        thread::sleep(Duration::from_millis(20));
    }
}

#[cfg(unix)]
#[test]
fn a_session_that_is_never_resized_still_publishes_kitty_placements() {
    let session = TerminalSession::spawn(
        DEFAULT_HISTORY_LIMIT,
        Arc::new(TerminalAppearance::default()),
        TerminalSpawn {
            initial_size: Some(TerminalSize {
                columns: 38,
                rows: 12,
                cell_width_px: 8,
                cell_height_px: 18,
            }),
            ..TerminalSpawn::default()
        },
    );
    let view = TerminalViewId(21);
    attach_streaming(&session, view);
    session.send_text("printf '\\033_Ga=T,f=24,s=1,v=1,i=77;/wAA\\033\\\\'\n");

    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    loop {
        while session.events().try_recv().is_ok() {}
        let placed = session.latest_viewport_for(view).is_some_and(|viewport| {
            viewport
                .kitty_placements
                .iter()
                .any(|placement| placement.image_id == 77)
        });
        if placed {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "unresized session never published a Kitty placement"
        );
        thread::sleep(Duration::from_millis(20));
    }
}
