use std::collections::BTreeMap;

use zz_protocol::{
    Axis, BrowserDescriptor, LayoutNode, PaneKindSnapshot, PaneSnapshot, PaneSummary,
    SessionSnapshot, SplitId, WindowId, WindowSnapshot,
};

use super::*;

pub(super) fn terminal_pane(id: u64, title: &str) -> PaneSnapshot {
    PaneSnapshot {
        id: PaneId(id),
        title: title.to_owned(),
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
        summary: PaneSummary::default(),
    }
}

pub(super) fn browser_pane(id: u64, title: &str, url: &str) -> PaneSnapshot {
    PaneSnapshot {
        id: PaneId(id),
        title: title.to_owned(),
        kind: PaneKindSnapshot::Browser(BrowserDescriptor::single(
            url.to_owned(),
            "default".to_owned(),
        )),
        synchronized_input: false,
        bell: false,
        dead: false,
        dead_status: None,
        border_colour: None,
        active_border_colour: None,
        border_status_text: String::new(),
        mode: None,
        status: None,
        summary: PaneSummary::default(),
    }
}

pub(super) fn mux_window(id: u64, index: u32, name: &str, pane: u64) -> WindowSnapshot {
    let pane = terminal_pane(pane, name);
    WindowSnapshot {
        id: WindowId(id),
        index,
        name: name.to_owned(),
        automatic_rename: true,
        active_pane: pane.id,
        zoomed_pane: None,
        layout: LayoutNode::Pane(pane.id),
        panes: BTreeMap::from([(pane.id, pane)]),
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
    }
}

pub(super) fn snapshot_with_two_panes() -> MuxSnapshot {
    let terminal = terminal_pane(101, "");
    let browser = browser_pane(202, "", "https://zed.dev");
    let window = WindowSnapshot {
        id: WindowId(11),
        index: 2,
        name: "work".to_owned(),
        automatic_rename: true,
        active_pane: browser.id,
        zoomed_pane: None,
        layout: LayoutNode::Split {
            id: SplitId(1),
            axis: Axis::Horizontal,
            ratio: 5000.0,
            first: Box::new(LayoutNode::Pane(browser.id)),
            second: Box::new(LayoutNode::Pane(terminal.id)),
        },
        panes: BTreeMap::from([(terminal.id, terminal), (browser.id, browser)]),
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
    };
    MuxSnapshot {
        generation: 7,
        focused_window: None,
        sessions: vec![SessionSnapshot {
            id: SessionId(1),
            name: "main workspace".to_owned(),
            active_window: window.id,
            windows: vec![window],
            viewers: Vec::new(),
        }],
    }
}

pub(super) fn local_model(snapshot: &MuxSnapshot, attached: Option<SessionId>) -> MuxTreeModel {
    let connected = HostState::Connected;
    MuxTreeModel::from_hosts(
        HostId::LOCAL,
        attached,
        [(HostId::LOCAL, "local", &connected, Some(snapshot))],
    )
}

pub(super) fn host_ids(names: &[&str]) -> Vec<HostId> {
    let configured = names
        .iter()
        .map(|name| crate::config::HostEntry {
            name: (*name).to_owned(),
            endpoint: zz_daemon_client::Endpoint::parse(&format!("ssh://{name}"))
                .expect("test endpoint"),
        })
        .collect::<Vec<_>>();
    let registry = crate::mux::hosts::HostRegistry::new(
        std::path::PathBuf::from("/tmp/zz-sidebar-local.sock"),
        &configured,
        crate::profile::LocalHostPolicy::Always,
    );
    names
        .iter()
        .map(|name| registry.get_by_name(name).unwrap().0)
        .collect()
}

pub(super) fn remote_snapshot() -> MuxSnapshot {
    MuxSnapshot {
        generation: 9,
        focused_window: None,
        sessions: vec![
            SessionSnapshot {
                id: SessionId(1),
                name: "same id elsewhere".to_owned(),
                active_window: WindowId(11),
                windows: vec![mux_window(11, 0, "remote window", 202)],
                viewers: Vec::new(),
            },
            SessionSnapshot {
                id: SessionId(9),
                name: "builds".to_owned(),
                active_window: WindowId(21),
                windows: vec![mux_window(21, 1, "compile", 303)],
                viewers: Vec::new(),
            },
        ],
    }
}
