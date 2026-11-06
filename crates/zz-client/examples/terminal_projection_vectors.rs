use std::sync::Arc;

use serde_json::{Value, json};
use zz_client::local_scroll::{
    LocalRowSource, RowProjection, grid_placement, kitty_display_row, last_visible_row_has_content,
    local_live_projection, local_row_source, peek_row_source,
};
use zz_terminal::{
    CellWidth, Color, ColourClass, GRAPHEME_TABLE_BIT, KittyLayer, KittyPlacement, LocalColors,
    PackedCell, PackedStyle, ScrollbarState, SessionStatus, TerminalDictionary, TerminalMode,
    TerminalPalette, TerminalViewport, UnderlineStyle,
};

fn uint(output: &mut Vec<u8>, mut value: u64) {
    while value >= 128 {
        output.push((value as u8 & 127) | 128);
        value >>= 7;
    }
    output.push(value as u8);
}
fn signed(output: &mut Vec<u8>, value: i64) {
    uint(output, ((value as u64) << 1) ^ ((value >> 63) as u64));
}
fn optional(output: &mut Vec<u8>, value: Option<u64>) {
    uint(output, u64::from(value.is_some()));
    if let Some(value) = value {
        uint(output, value);
    }
}
fn row(output: &mut Vec<u8>, value: RowProjection) {
    for value in [value.source_start, value.destination_start, value.count] {
        uint(output, u64::from(value));
    }
}
fn source(output: &mut Vec<u8>, value: LocalRowSource) {
    let (kind, index) = match value {
        LocalRowSource::History(v) => (0, v as u64),
        LocalRowSource::Live(v) => (1, u64::from(v)),
        LocalRowSource::Below(v) => (2, v as u64),
        LocalRowSource::Shimmer => (3, 0),
    };
    uint(output, kind);
    uint(output, index);
}
fn record(name: &str, request: &[u8], expected: &[u8]) -> Value {
    json!({"name":name,"request":request,"expected":expected})
}
fn style_fields(style: PackedStyle) -> [u64; 6] {
    let classes = style.class_word();
    [
        u64::from(style.foreground_raw() | (classes >> 8 & 255) << 24),
        u64::from(style.background_raw()),
        u64::from(style.underline_color_raw()),
        u64::from(style.attributes() | ((classes & 15) as u16) << 12),
        u64::from(style.underline_kind_raw()),
        u64::from(classes >> 16 & 255),
    ]
}

fn colors() -> ([Color; 18], [Color; 18], LocalColors) {
    let mut host = std::array::from_fn(|i| Color::from_packed(0x30_0000 + i as u32));
    host[0] = Color::from_packed(0x11_1111);
    host[1] = Color::from_packed(0x22_2222);
    host[2] = host[0];
    let local = std::array::from_fn(|i| Color::from_packed(0x90_0000 + i as u32));
    let palette = TerminalPalette::new(std::array::from_fn(|i| {
        if i < 16 {
            host[2 + i]
        } else {
            Color::default()
        }
    }));
    (
        host,
        local,
        LocalColors::new(host[0], host[1], &palette, local),
    )
}
fn colors_request(output: &mut Vec<u8>, host: [Color; 18], local: [Color; 18]) {
    for value in host.into_iter().chain(local) {
        uint(output, u64::from(value.packed()));
    }
}
fn class(kind: u8, index: u8) -> ColourClass {
    match kind {
        0 => ColourClass::Resolved,
        1 => ColourClass::Default,
        2 => ColourClass::Palette(index),
        _ => ColourClass::Rgb,
    }
}

fn projections() -> Vec<Value> {
    let mut output = Vec::new();
    for viewport in [0_u16, 1, 2, 24, 30, u16::MAX] {
        for grid in [0_u16, 1, 2, 24, 30, u16::MAX] {
            for bottom in [false, true] {
                let value = RowProjection::new(viewport, grid, bottom);
                let mut request = vec![];
                let mut expected = vec![];
                uint(&mut request, u64::from(viewport));
                uint(&mut request, u64::from(grid));
                uint(&mut request, u64::from(bottom));
                row(&mut expected, value);
                let offsets = [0, 1, value.count.saturating_sub(1), value.count, u16::MAX];
                uint(&mut request, offsets.len() as u64);
                for query in offsets {
                    uint(&mut request, u64::from(query));
                    optional(&mut expected, value.source_row(query).map(u64::from));
                }
                let sources = [
                    0,
                    1,
                    value.source_start.saturating_sub(1),
                    value.source_start,
                    value
                        .source_start
                        .saturating_add(value.count.saturating_sub(1)),
                    u16::MAX,
                ];
                uint(&mut request, sources.len() as u64);
                for query in sources {
                    uint(&mut request, u64::from(query));
                    optional(&mut expected, value.display_row(query).map(|v| v as u64));
                }
                output.push(record(
                    &format!("rows-{viewport}-{grid}-{bottom}"),
                    &request,
                    &expected,
                ));
            }
        }
    }
    output
}
fn local_projections() -> Vec<Value> {
    let mut output = Vec::new();
    for target in [0, 1, 96, 97, 100, 103, 104, u32::MAX - 1, u32::MAX] {
        for server in [0, 1, 96, 97, 100, 103, 104, u32::MAX - 1, u32::MAX] {
            for viewport in [0_u16, 1, 4, 24, u16::MAX] {
                for grid in [0_u16, 1, 4, 30, u16::MAX] {
                    let mut request = vec![];
                    for value in [
                        u64::from(target),
                        u64::from(server),
                        u64::from(viewport),
                        u64::from(grid),
                    ] {
                        uint(&mut request, value);
                    }
                    let mut expected = vec![];
                    row(
                        &mut expected,
                        local_live_projection(target, server, viewport, grid),
                    );
                    output.push(record(
                        &format!("local-{target}-{server}-{viewport}-{grid}"),
                        &request,
                        &expected,
                    ));
                }
            }
        }
    }
    output
}
fn sources() -> Vec<Value> {
    let mut output = Vec::new();
    for absolute in [0, 1, 97, 100, 103, 104, u32::MAX] {
        for server in [0, 1, 97, 100, 103, 104, u32::MAX] {
            for live in [0_u16, 1, 4, u16::MAX] {
                for history in [0, 3, usize::MAX] {
                    for below in [0, 2, usize::MAX] {
                        let mut request = vec![];
                        for value in [
                            u64::from(absolute),
                            u64::from(server),
                            u64::from(live),
                            history as u64,
                            below as u64,
                        ] {
                            uint(&mut request, value);
                        }
                        let mut expected = vec![];
                        source(
                            &mut expected,
                            local_row_source(absolute, server, live, history, below),
                        );
                        let distances = [0, 1, 3, u32::MAX];
                        uint(&mut request, distances.len() as u64);
                        for distance in distances {
                            uint(&mut request, u64::from(distance));
                            let value = peek_row_source(absolute, distance, server, live, history);
                            uint(&mut expected, u64::from(value.is_some()));
                            if let Some(value) = value {
                                source(&mut expected, value);
                            }
                        }
                        output.push(record(
                            &format!("source-{absolute}-{server}-{live}-{history}-{below}"),
                            &request,
                            &expected,
                        ));
                    }
                }
            }
        }
    }
    output
}
fn kitty() -> Vec<Value> {
    let mut output = Vec::new();
    for absolute in [0, 1, 100, i64::MAX as u64, i64::MAX as u64 + 1, u64::MAX] {
        for viewport in [i32::MIN, -24, -1, 0, 1, 24, i32::MAX] {
            for rows in [0, 1, 4, u32::MAX] {
                for target in [None, Some(0), Some(100), Some(u32::MAX)] {
                    for projection in [
                        RowProjection::new(4, 4, false),
                        RowProjection {
                            source_start: 2,
                            destination_start: 0,
                            count: 2,
                        },
                        RowProjection {
                            source_start: 0,
                            destination_start: 2,
                            count: 4,
                        },
                        RowProjection {
                            source_start: 65530,
                            destination_start: 65534,
                            count: 10,
                        },
                    ] {
                        let placement = KittyPlacement {
                            image_id: 0,
                            image_generation: 0,
                            layer: KittyLayer::BelowText,
                            viewport_col: 0,
                            viewport_row: viewport,
                            absolute_row: absolute,
                            cell_offset_x: 0,
                            cell_offset_y: 0,
                            grid_cols: 0,
                            grid_rows: rows,
                            pixel_width: 0,
                            pixel_height: 0,
                            source_rect: None,
                        };
                        let mut request = vec![];
                        row(&mut request, projection);
                        signed(&mut request, i64::from(viewport));
                        uint(&mut request, absolute);
                        uint(&mut request, u64::from(rows));
                        optional(&mut request, target.map(u64::from));
                        let value = kitty_display_row(&placement, target, projection);
                        let mut expected = vec![];
                        uint(&mut expected, u64::from(value.is_some()));
                        if let Some(value) = value {
                            signed(&mut expected, value);
                        }
                        output.push(record(
                            &format!(
                                "kitty-{absolute}-{viewport}-{rows}-{target:?}-{projection:?}"
                            ),
                            &request,
                            &expected,
                        ));
                    }
                }
            }
        }
    }
    output
}
fn recolors() -> Vec<Value> {
    let (host, local, colors) = colors();
    let mut output = Vec::new();
    for fg in 0..4 {
        for bg in 0..4 {
            for index in [0, 15, 16, 255] {
                for foreground in [host[0], host[1], Color::from_packed(0x12_3456)] {
                    for background in [host[1], host[0], Color::from_packed(0x65_4321)] {
                        let style = PackedStyle::new(
                            foreground,
                            background,
                            Some(Color::from_packed(0x11_2233)),
                            0x1ff,
                            UnderlineStyle::Curly,
                        )
                        .with_classes(class(fg, index), class(bg, index));
                        let mut request = vec![];
                        colors_request(&mut request, host, local);
                        for value in style_fields(style) {
                            uint(&mut request, value);
                        }
                        let mut expected = vec![];
                        for value in style_fields(colors.recolor(style)) {
                            uint(&mut expected, value);
                        }
                        output.push(record(
                            &format!("color-{fg}-{bg}-{index}-{foreground:?}-{background:?}"),
                            &request,
                            &expected,
                        ));
                    }
                }
            }
        }
    }
    output
}
fn view(scene: u8) -> TerminalViewport {
    let mut viewport = TerminalViewport::blank(4, 4, SessionStatus::Running);
    viewport.foreground = Color::from_packed(0x11_1111);
    viewport.background = Color::from_packed(0x22_2222);
    let mut styles = vec![
        PackedStyle::new(
            viewport.foreground,
            viewport.background,
            None,
            0,
            UnderlineStyle::None,
        )
        .with_classes(ColourClass::Default, ColourClass::Default),
    ];
    let mut offsets = vec![0, 3];
    let mut bytes = "e\u{301}".as_bytes().to_vec();
    let mut cells = vec![PackedCell::new(0, 0, CellWidth::Narrow); 16];
    match scene {
        0 => styles.clear(),
        1 => {}
        2 => cells[15] = PackedCell::new(u32::from('x'), 0, CellWidth::Narrow),
        3 => cells[15] = PackedCell::new(u32::from(' '), 0, CellWidth::Narrow),
        4 => cells[15] = PackedCell::from_raw(0xd800, 0, 0),
        5 => cells[15] = PackedCell::new(GRAPHEME_TABLE_BIT, 0, CellWidth::Narrow),
        6 => {
            offsets = vec![0, 0];
            bytes.clear();
            cells[15] = PackedCell::new(GRAPHEME_TABLE_BIT, 0, CellWidth::Narrow);
        }
        7 => {
            offsets = vec![0, 1];
            bytes = vec![255];
            cells[15] = PackedCell::new(GRAPHEME_TABLE_BIT, 0, CellWidth::Narrow);
        }
        8 => cells[15] = PackedCell::new(GRAPHEME_TABLE_BIT | 0x63, 0, CellWidth::Narrow),
        9 => {
            styles[0] = PackedStyle::new(
                viewport.foreground,
                Color::from_packed(0x33_3333),
                None,
                0,
                UnderlineStyle::None,
            )
            .with_classes(ColourClass::Rgb, ColourClass::Rgb);
        }
        10 => cells[0] = PackedCell::new(u32::from('x'), 0, CellWidth::Narrow),
        11 => cells[15] = PackedCell::new(u32::from('x'), 0, CellWidth::SpacerTail),
        12 => cells.truncate(15),
        13 => {
            viewport.columns = 0;
            cells.clear();
        }
        _ => {
            viewport.rows = 0;
            cells.clear();
        }
    }
    viewport.cells = cells.into();
    viewport.dictionary = Arc::new(TerminalDictionary::from_shared(
        styles.into(),
        offsets.into(),
        bytes.into(),
    ));
    viewport
}
fn viewport_request(output: &mut Vec<u8>, viewport: &TerminalViewport) {
    for value in [
        u64::from(viewport.columns),
        u64::from(viewport.rows),
        u64::from(viewport.background.packed()),
        match viewport.mode {
            TerminalMode::Live => 0,
            TerminalMode::Copy { .. } => 1,
            TerminalMode::View { .. } => 2,
        },
        u64::from(viewport.scrollbar.total),
        u64::from(viewport.scrollbar.offset),
        u64::from(viewport.scrollbar.len),
    ] {
        uint(output, value);
    }
    uint(output, viewport.cells.len() as u64);
    for cell in viewport.cells.iter() {
        for value in [
            u64::from(cell.glyph()),
            u64::from(cell.style_id()),
            u64::from(cell.flags()),
        ] {
            uint(output, value);
        }
    }
    uint(output, viewport.styles().len() as u64);
    for style in viewport.styles() {
        for value in style_fields(*style) {
            uint(output, value);
        }
    }
    uint(output, viewport.grapheme_offsets().len() as u64);
    for value in viewport.grapheme_offsets() {
        uint(output, u64::from(*value));
    }
    uint(output, viewport.grapheme_bytes().len() as u64);
    output.extend_from_slice(viewport.grapheme_bytes());
}
fn placements() -> Vec<Value> {
    let (host, local, colors) = colors();
    let mut output = Vec::new();
    for scene in 0..15 {
        for mode in 0..3 {
            for command in [false, true] {
                for scrolled in [false, true] {
                    for scroll in 0..3 {
                        for recolor in [false, true] {
                            let mut viewport = view(scene);
                            viewport.mode = match mode {
                                0 => TerminalMode::Live,
                                1 => TerminalMode::Copy {
                                    position: 1,
                                    total: 100,
                                    hide_position: false,
                                },
                                _ => TerminalMode::View {
                                    position: 1,
                                    total: 100,
                                },
                            };
                            viewport.scrollbar = match scroll {
                                0 => ScrollbarState {
                                    total: 40,
                                    offset: 36,
                                    len: 4,
                                },
                                1 => ScrollbarState {
                                    total: 40,
                                    offset: 20,
                                    len: 4,
                                },
                                _ => ScrollbarState {
                                    total: u32::MAX,
                                    offset: u32::MAX - 1,
                                    len: 4,
                                },
                            };
                            let target = (scrolled && scene % 2 == 1)
                                .then_some(viewport.scrollbar.offset.saturating_sub(3));
                            let grid = if scene % 2 == 0 { 2 } else { 6 };
                            let local_color = recolor.then_some(&colors);
                            let placement = grid_placement(
                                &viewport,
                                local_color,
                                command,
                                scrolled || target.is_some(),
                            );
                            let rows = RowProjection::new(
                                viewport.rows,
                                grid,
                                placement.project_from_bottom,
                            );
                            let live = target.map_or(rows, |target| {
                                local_live_projection(
                                    target,
                                    viewport.scrollbar.offset,
                                    viewport.rows,
                                    grid,
                                )
                            });
                            let mut request = vec![];
                            viewport_request(&mut request, &viewport);
                            uint(&mut request, u64::from(recolor));
                            if recolor {
                                colors_request(&mut request, host, local);
                            }
                            for value in [u64::from(grid), u64::from(command), u64::from(scrolled)]
                            {
                                uint(&mut request, value);
                            }
                            optional(&mut request, target.map(u64::from));
                            let mut expected = vec![];
                            for visible in [0, 1, viewport.rows, u16::MAX] {
                                uint(
                                    &mut expected,
                                    u64::from(last_visible_row_has_content(
                                        &viewport,
                                        local_color,
                                        visible,
                                    )),
                                );
                            }
                            uint(&mut expected, u64::from(placement.bottom_anchored));
                            uint(&mut expected, u64::from(placement.project_from_bottom));
                            row(&mut expected, rows);
                            row(&mut expected, live);
                            output.push(record(&format!("placement-{scene}-{mode}-{command}-{scrolled}-{scroll}-{recolor}"),&request,&expected));
                        }
                    }
                }
            }
        }
    }
    output
}

fn main() {
    println!(
        "{}",
        json!({"reference":"actual zz-client local_scroll projection/placement/Kitty and zz-terminal LocalColors","projections":projections(),"local":local_projections(),"sources":sources(),"kitty":kitty(),"recolors":recolors(),"placements":placements()})
    );
}
