use std::sync::Arc;

use super::*;
use zz_protocol::MenuState;
use zz_terminal::{CellWidth, SessionStatus, TerminalDictionary};

pub(super) fn styled_viewport() -> TerminalViewport {
    let mut viewport = TerminalViewport::blank(3, 1, SessionStatus::Running);
    let first = PackedStyle::new(
        Color::rgb(255, 0, 0),
        viewport.background,
        None,
        zz_terminal::ATTR_BOLD,
        UnderlineStyle::None,
    );
    let second = PackedStyle::new(
        Color::rgb(0, 255, 0),
        viewport.background,
        None,
        0,
        UnderlineStyle::None,
    );
    viewport.dictionary = Arc::new(TerminalDictionary::from_shared(
        Arc::from([first, second]),
        Arc::from([0]),
        Arc::from([]),
    ));
    viewport.cells = Arc::from([
        PackedCell::new('a' as u32, 0, CellWidth::Narrow),
        PackedCell::new('b' as u32, 0, CellWidth::Narrow),
        PackedCell::new('c' as u32, 1, CellWidth::Narrow),
    ]);
    viewport
}

pub(super) fn waiting_pane_model() -> Model {
    let mut model = block_model(40, 10);
    let session = zz_protocol::SessionId(1);
    let window = zz_protocol::WindowId(1);
    let pane = PaneId(3);
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
                panes: std::collections::BTreeMap::from([(
                    pane,
                    zz_protocol::PaneSnapshot {
                        id: pane,
                        title: "shell".to_owned(),
                        kind: PaneKindSnapshot::Terminal,
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
                    },
                )]),
                layout_dump: String::new(),
                visible_layout_dump: String::new(),
                status_label: String::new(),
                activity: false,
                silence: false,
                pane_border_status: PaneBorderStatus::Off,
                pane_border_lines: PaneBorderLines::Single,
                pane_border_indicators: PaneBorderIndicators::Colour,
                pane_order: vec![pane],
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
    model
}

pub(super) fn block_model(columns: u16, rows: u16) -> Model {
    let core = zz_client::ClientCore::new();
    let endpoint = zz_daemon_client::Endpoint::parse("unix:///tmp/zz-render-test.sock")
        .expect("test endpoint");
    Model::new(
        &core,
        crate::tty::TerminalSize {
            columns,
            rows,
            cell_width_px: 8,
            cell_height_px: 16,
        },
        "host".to_owned(),
        "host".to_owned(),
        endpoint.clone(),
        endpoint,
        Vec::new(),
    )
}

pub(super) fn menu_state() -> MenuState {
    MenuState {
        left: 5,
        top: 2,
        width: 20,
        height: 5,
        client_columns: 40,
        client_rows: 12,
        cell_width_px: 8,
        cell_height_px: 16,
        title: "Actions".to_owned(),
        style: "fg=white,bg=blue".to_owned(),
        selected_style: "fg=black,bg=yellow,bold".to_owned(),
        border_style: "fg=red".to_owned(),
        border_lines: PopupBorderLines::Single,
        items: vec![
            Some(zz_protocol::MenuItem {
                name: "界 item".to_owned(),
                key: Some("q".to_owned()),
                annotation: Some("q".to_owned()),
                enabled: true,
            }),
            None,
            Some(zz_protocol::MenuItem {
                name: "Disabled".to_owned(),
                key: None,
                annotation: None,
                enabled: false,
            }),
        ],
        selected: Some(0),
        stay_open: false,
        mouse_keys: false,
    }
}

pub(super) fn block_status(rows: Vec<&str>, customized: bool) -> zz_protocol::StatusLine {
    zz_protocol::StatusLine {
        rows: rows.into_iter().map(str::to_owned).collect(),
        customized,
        ..zz_protocol::StatusLine::default()
    }
}

pub(super) fn attach_one_pane(model: &mut Model, pane: PaneId) {
    let session = zz_protocol::SessionId(1);
    let window = zz_protocol::WindowId(1);
    model.attached_session = Some(session);
    model.update_snapshot(std::sync::Arc::new(zz_protocol::MuxSnapshot {
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
}

pub(super) fn tiled_under_float(
    float: zz_protocol::FloatingPaneSnapshot,
    mode: Option<zz_protocol::PaneMode>,
) -> Model {
    let tiled = PaneId(7);
    let mut model = block_model(40, 12);
    attach_one_pane(&mut model, tiled);
    let mut snapshot = (*model.snapshot).clone();
    let window = &mut snapshot.sessions[0].windows[0];
    let pane = |id: PaneId, mode: Option<zz_protocol::PaneMode>| zz_protocol::PaneSnapshot {
        id,
        title: String::new(),
        kind: PaneKindSnapshot::Terminal,
        synchronized_input: false,
        bell: false,
        dead: false,
        dead_status: None,
        border_colour: None,
        active_border_colour: None,
        border_status_text: String::new(),
        mode,
        status: None,
        summary: zz_protocol::PaneSummary::default(),
    };
    window.panes.insert(tiled, pane(tiled, mode));
    window.panes.insert(float.pane, pane(float.pane, None));
    window.floating = vec![float];
    model.update_snapshot(std::sync::Arc::new(snapshot));
    model
}

pub(super) const CLIPPED: zz_protocol::FloatingPaneSnapshot = zz_protocol::FloatingPaneSnapshot {
    pane: PaneId(8),
    xoff: -3,
    yoff: 2,
    sx: 10,
    sy: 4,
    visible: true,
    border_lines: PaneBorderLines::Single,
    border_status: zz_protocol::PaneBorderStatus::Off,
};
