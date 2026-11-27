use super::*;

pub(super) fn paned_model() -> (Model, zz_protocol::PaneId) {
    let core = ClientCore::new();
    let endpoint = Endpoint::parse("unix:///tmp/zz-app-mouse-test.sock").expect("endpoint");
    let mut model = Model::new(
        &core,
        crate::tty::TerminalSize {
            columns: 79,
            rows: 24,
            cell_width_px: 8,
            cell_height_px: 16,
        },
        "host".to_owned(),
        "host".to_owned(),
        endpoint.clone(),
        endpoint,
        Vec::new(),
    );
    let session = zz_protocol::SessionId(1);
    let window = zz_protocol::WindowId(1);
    let pane = zz_protocol::PaneId(7);
    model.attached_session = Some(session);
    model.update_snapshot(Arc::new(zz_protocol::MuxSnapshot {
        generation: 1,
        sessions: vec![zz_protocol::SessionSnapshot {
            id: session,
            name: "s".to_owned(),
            active_window: window,
            windows: vec![zz_protocol::WindowSnapshot {
                id: window,
                index: 0,
                name: "w".to_owned(),
                automatic_rename: true,
                active_pane: pane,
                zoomed_pane: None,
                layout: zz_protocol::LayoutNode::Pane(pane),
                panes: std::collections::BTreeMap::new(),
                layout_dump: String::new(),
                visible_layout_dump: String::new(),
                status_label: String::new(),
                activity: false,
                silence: false,
                pane_border_status: zz_protocol::PaneBorderStatus::Off,
                pane_border_lines: zz_protocol::PaneBorderLines::Single,
                pane_border_indicators: zz_protocol::PaneBorderIndicators::Colour,
                pane_order: Vec::new(),
                pane_z_order: Vec::new(),
                floating: Vec::new(),
                modal: None,
                sx: 0,
                sy: 0,
                pane_base_index: 0,
            }],
            viewers: Vec::new(),
        }],
        focused_window: Some(window),
    }));
    (model, pane)
}

pub(super) fn with_row_text(viewport: &TerminalViewport, row: u16, text: &str) -> TerminalViewport {
    let mut viewport = viewport.clone();
    let mut cells = viewport.cells.to_vec();
    let start = usize::from(row) * usize::from(viewport.columns);
    for (offset, glyph) in text.chars().enumerate() {
        cells[start + offset] =
            zz_terminal::PackedCell::new(u32::from(glyph), 0, zz_terminal::CellWidth::Narrow);
    }
    viewport.cells = Arc::from(cells);
    viewport
}
