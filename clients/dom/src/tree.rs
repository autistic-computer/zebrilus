use std::collections::BTreeSet;

use serde_json::{Value, json};
use wasm_bindgen::prelude::*;
use zz_client::{
    ChromeKeymap, ChromeProfile, DropZone, MAX_VISIBLE_WINDOWS, PaneRect, SIDEBAR_TABLE,
    StatusBarModel, StatusBarSettings, UI_TABLE, coerced_drop_zone, drop_preview_bounds,
    drop_zone_at,
    floating::{
        FloatCells, PipGeometry, float_pane_commands, float_placement, float_title,
        tile_pane_command, zoom_pane_command,
    },
    navigation::{RenameTarget, ordered_panes, pane_label, rename_prompt_command, session_label},
    pane_drop_command,
    pane_picker::{PanePickerKey, pane_choices, pane_kind_command, pane_picker_key},
    pane_separator::{SeparatorSide, pane_separator},
    picker_split_command, predicted_drop_layout,
    sidebar::{
        SidebarBadge, SidebarNode, SidebarTarget, TreeNavigation, TreeNavigationResult,
        navigation_rows, sidebar_activation, sidebar_active, sidebar_active_path,
        sidebar_add_command, sidebar_close_command, sidebar_rename_command, sidebar_rows,
        tree_navigation,
    },
    split_ratio_basis, visible_window_range,
    window_summary::{Attention, DaemonClock, PaneTag, Tone, age_label, summarize_host},
};
use zz_protocol::{
    Axis, CommandInvocation, InputMessage, LayoutNode, MuxOptionKey, PaneBorderLines, PaneId,
    PaneKindSnapshot, SplitId, WindowId, WindowSnapshot,
};
use zz_terminal::{KeyAction, KeyCode, KeyInput, Modifiers};

use crate::{Client, terminal::key_code};

#[derive(Default)]
pub struct Sidebar {
    collapsed: BTreeSet<SidebarTarget>,
    selection: Option<SidebarTarget>,
    pointer_selection: bool,
    active: Option<SidebarTarget>,
    unseen: BTreeSet<PaneId>,
    focused: bool,
    clock: DaemonClock,
}

#[wasm_bindgen]
impl Client {
    pub fn tree(&self) -> String {
        let snapshot = self.core.snapshot();
        let sessions = snapshot
            .sessions
            .iter()
            .map(|session| {
                let focused = snapshot.focused_window_for(session);
                let windows = session
                    .windows
                    .iter()
                    .map(|window| self.window_json(window, window.id == focused))
                    .collect::<Vec<_>>();
                let label = session_label(session);
                json!({
                    "id": session.id.0,
                    "name": session.name,
                    "label": label.text,
                    "index": label.index,
                    "windows": windows,
                })
            })
            .collect::<Vec<_>>();
        json!({
            "attached": self.core.attached_session().map(|session| session.0),
            "generation": snapshot.generation,
            "readOnly": self.core.attached_read_only(),
            "prefix": self.core.prefix_armed(),
            "focusFollowsMouse": self
                .core
                .mux_options()
                .get(MuxOptionKey::FocusFollowsMouse)
                .is_some_and(|option| option.value == "on"),
            "sessions": sessions,
        })
        .to_string()
    }

    pub fn workspace_key(
        &self,
        table: &str,
        key: &str,
        shift: bool,
        control: bool,
        alt: bool,
        meta: bool,
    ) -> Option<String> {
        let input = key_input(key, shift, control, alt, meta);
        if self.core.claims_prefix_input(&input) {
            return Some("prefix".to_owned());
        }
        let table = if table == "sidebar" {
            SIDEBAR_TABLE
        } else {
            UI_TABLE
        };
        ChromeKeymap::for_profile(ChromeProfile::Desktop)
            .resolve(table, &input)
            .or_else(|| {
                (table == UI_TABLE)
                    .then(|| {
                        ChromeKeymap::for_profile(ChromeProfile::DesktopApple)
                            .resolve(table, &input)
                    })
                    .flatten()
            })
            .map(|action| action.name().to_owned())
    }

    pub fn pane_action(&mut self, pane: f64, action: &str, float_rows: u16) {
        let pane = pane_id(pane);
        let commands = match action {
            "split-bottom" => vec![picker_split_command(pane, Axis::Vertical)],
            "split-right" => vec![picker_split_command(pane, Axis::Horizontal)],
            "float" => float_pane_commands(pane, (float_rows > 0).then_some(float_rows)).to_vec(),
            "tile" => vec![tile_pane_command(pane)],
            "zoom" => vec![zoom_pane_command(pane)],
            "select" => vec![CommandInvocation::new(
                "select-pane",
                ["-t".to_owned(), pane.to_string()],
            )],
            "close" => vec![CommandInvocation::new(
                "kill-pane",
                ["-t".to_owned(), pane.to_string()],
            )],
            _ => Vec::new(),
        };
        if action != "select" && self.core.attached_read_only() {
            return;
        }
        for command in commands {
            self.invoke(command);
        }
    }

    pub fn pane_drop_zone(
        &self,
        window: f64,
        source: f64,
        target: f64,
        slot: Vec<f32>,
        x: f32,
        y: f32,
    ) -> Option<String> {
        let window = self.window(window)?;
        let zone = coerced_drop_zone(
            &window.layout,
            pane_id(source),
            pane_id(target),
            drop_zone_at(rect(&slot), (x, y)),
        );
        Some(zone_name(zone).to_owned())
    }

    pub fn pane_drop_preview(&self, slot: Vec<f32>, zone: &str, divider: f32) -> Vec<f32> {
        let Some(zone) = parse_zone(zone) else {
            return Vec::new();
        };
        let preview = drop_preview_bounds(rect(&slot), zone, divider);
        vec![preview.x, preview.y, preview.width, preview.height]
    }

    pub fn pane_drop(
        &mut self,
        window: f64,
        source: f64,
        target: f64,
        zone: &str,
    ) -> Option<String> {
        let zone = parse_zone(zone)?;
        let (source, target) = (pane_id(source), pane_id(target));
        if self.core.attached_read_only() {
            return None;
        }
        let command = pane_drop_command(source, target, zone)?;
        let predicted = self.window(window).and_then(|window| {
            predicted_drop_layout(&window.layout, source, target, zone)
                .map(|layout| layout_json(&layout, window.active_pane))
        });
        self.invoke(command);
        Some(predicted.unwrap_or(Value::Null).to_string())
    }

    pub fn split_commit(&mut self, window: f64, split: f64, start: f32, ratio: f32) -> f32 {
        let ratio_basis_points = split_ratio_basis(ratio);
        if ratio_basis_points == split_ratio_basis(start) || self.core.attached_read_only() {
            return -1.0;
        }
        self.input(InputMessage::ResizeSplit {
            window: WindowId(window as u64),
            split: SplitId(split as u64),
            ratio_basis_points,
        });
        f32::from(ratio_basis_points) / f32::from(zz_protocol::SPLIT_RATIO_BASIS)
    }

    pub fn floats(&self, window: f64, cell: Vec<f32>, canvas: Vec<f32>, header: f32) -> String {
        let Some(window) = self.window(window) else {
            return "[]".into();
        };
        let cell = (cell[0], cell[1]);
        let canvas = (canvas[0], canvas[1]);
        let floats = window
            .floating
            .iter()
            .rev()
            .filter(|float| float.visible)
            .filter_map(|float| {
                let modal = window.modal.filter(|modal| modal.pane == float.pane);
                let (placed, content) = float_placement(float, cell, canvas)?;
                let frame = if modal.is_some() {
                    placed
                } else {
                    pip_geometry(window, cell, canvas, float.border_lines, header)
                        .frame(FloatCells::from(float))
                };
                let title = window
                    .panes
                    .get(&float.pane)
                    .filter(|_| float.border_status.is_on())
                    .map(|pane| float_title(&pane.border_status_text).left)
                    .unwrap_or_default();
                Some(json!({
                    "pane": float.pane.0,
                    "modal": modal.is_some(),
                    "closeOnClick": modal.is_some_and(|modal| modal.close_on_click),
                    "bordered": float.border_lines != PaneBorderLines::None,
                    "frame": [frame.x, frame.y, frame.width, frame.height],
                    "inset": [content.x - placed.x, content.y - placed.y],
                    "title": title,
                    "cells": [float.xoff, float.yoff, float.sx, float.sy],
                }))
            })
            .collect::<Vec<_>>();
        Value::Array(floats).to_string()
    }

    pub fn float_rows(&self, window: f64, cell: Vec<f32>, canvas: Vec<f32>, header: f32) -> u16 {
        self.window(window).map_or(0, |window| {
            pip_geometry(
                window,
                (cell[0], cell[1]),
                (canvas[0], canvas[1]),
                window.pane_border_lines,
                header,
            )
            .fill_rows()
        })
    }

    pub fn float_land(
        &mut self,
        window: f64,
        pane: f64,
        cell: Vec<f32>,
        canvas: Vec<f32>,
        header: f32,
        centre: Vec<f32>,
    ) -> bool {
        let pane = pane_id(pane);
        let Some(place) = self.window(window).and_then(|window| {
            let float = window.floating.iter().find(|float| float.pane == pane)?;
            Some(
                pip_geometry(
                    window,
                    (cell[0], cell[1]),
                    (canvas[0], canvas[1]),
                    float.border_lines,
                    header,
                )
                .place_at((centre[0], centre[1])),
            )
        }) else {
            return false;
        };
        if self.core.attached_read_only() {
            return false;
        }
        self.invoke(place.command(pane));
        true
    }

    pub fn picker_choices(&self) -> String {
        Value::Array(
            pane_choices(false)
                .iter()
                .map(|choice| {
                    json!({ "title": choice.title, "kind": choice.kind, "shortcut": choice.shortcut })
                })
                .collect(),
        )
        .to_string()
    }

    pub fn picker_key(
        &mut self,
        pane: f64,
        selected: usize,
        key: &str,
        shift: bool,
        control: bool,
        alt: bool,
        meta: bool,
    ) -> i32 {
        if self
            .core
            .claims_prefix_input(&key_input(key, shift, control, alt, meta))
        {
            return -2;
        }
        let choices = pane_choices(false);
        match pane_picker_key(
            choices,
            selected,
            &browser_key_name(key),
            control || alt || meta,
        ) {
            PanePickerKey::Select(index) => i32::try_from(index).unwrap_or_default(),
            PanePickerKey::Choose(index) => {
                self.picker_choose(pane, index);
                -1
            }
            PanePickerKey::Close => {
                self.pane_action(pane, "close", 0);
                -1
            }
            PanePickerKey::Ignored => -2,
        }
    }

    pub fn picker_choose(&mut self, pane: f64, index: usize) {
        if let Some(choice) = pane_choices(false).get(index)
            && !self.core.attached_read_only()
        {
            self.invoke(pane_kind_command(pane_id(pane), choice));
        }
    }

    pub fn status_bar(&self) -> String {
        let model = StatusBarModel::from_snapshot(
            self.core.snapshot(),
            self.core.attached_session(),
            None,
            StatusBarSettings {
                badges: true,
                show_host: false,
                show_update: false,
            },
        );
        let active = model
            .windows
            .iter()
            .position(|window| window.active)
            .unwrap_or(0);
        let visible = visible_window_range(model.windows.len(), active, MAX_VISIBLE_WINDOWS);
        let window = |window: &zz_client::StatusBarWindow| {
            let (rename, _) = rename_prompt_command(RenameTarget::Window(window.id), &window.name);
            let mut panes = window
                .panes
                .iter()
                .map(|pane| json!({ "id": pane.id.0, "label": pane.label, "tag": tag(&pane.kind), "active": pane.active }))
                .collect::<Vec<_>>();
            if let Some(index) = window.panes.iter().position(|pane| pane.active) {
                let focused = panes.remove(index);
                panes.insert(0, focused);
            }
            json!({
                "id": window.id.0,
                "index": window.index,
                "label": window.label,
                "active": window.active,
                "bell": window.bell,
                "activity": window.activity,
                "rename": rename,
                "panes": panes,
            })
        };
        json!({
            "windows": model.windows[visible].iter().map(window).collect::<Vec<_>>(),
            "overflow": (model.windows.len() > MAX_VISIBLE_WINDOWS).then(|| {
                model
                    .windows
                    .iter()
                    .map(|window| json!({ "id": window.id.0, "label": format!("{} {}", window.index, window.label), "active": window.active }))
                    .collect::<Vec<_>>()
            }),
        })
        .to_string()
    }

    pub fn window_command(&mut self, window: f64, action: &str) {
        let id = WindowId(window as u64);
        let command = match action {
            "select" => CommandInvocation::new("select-window", ["-t".to_owned(), id.to_string()]),
            "close" => CommandInvocation::new("kill-window", ["-t".to_owned(), id.to_string()]),
            "rename" => {
                let Some((_, command)) =
                    sidebar_rename_command(self.core.snapshot(), SidebarTarget::Window(id))
                else {
                    return;
                };
                command
            }
            _ => return,
        };
        if action != "select" && self.core.attached_read_only() {
            return;
        }
        self.invoke(command);
    }

    pub fn select_pane(&mut self, pane: f64, zoom: bool) {
        let target = pane_id(pane).to_string();
        self.invoke(CommandInvocation::new(
            "select-window",
            ["-t".to_owned(), target.clone()],
        ));
        let mut args = vec!["-t".to_owned(), target];
        if zoom {
            args.insert(0, "-Z".to_owned());
        }
        self.invoke(CommandInvocation::new("select-pane", args));
    }

    pub fn focus_pane(&mut self, pane: f64) {
        self.invoke(CommandInvocation::new(
            "select-pane",
            ["-Z".to_owned(), "-t".to_owned(), pane_id(pane).to_string()],
        ));
    }

    pub fn dismiss_prefix(&mut self, pane: f64) {
        if !self.core.prefix_armed() {
            return;
        }
        for action in [KeyAction::Press, KeyAction::Release] {
            self.input(InputMessage::Key {
                pane: pane_id(pane),
                input: KeyInput {
                    key: KeyCode::Escape,
                    action,
                    modifiers: Modifiers::default(),
                    text: None,
                    unshifted_codepoint: None,
                },
                text_follows: false,
            });
        }
    }

    pub fn sidebar_hello(&mut self, now: f64) {
        self.sidebar.clock = DaemonClock::from_hello(self.core.server_clock(), now as u64);
    }

    pub fn sidebar_focus(&mut self, focused: bool) {
        self.sidebar.focused = focused;
        if focused {
            self.sidebar.pointer_selection = false;
            self.sidebar.selection = self
                .active_window()
                .map(|window| SidebarTarget::Pane(window.active_pane));
            self.sidebar.active = None;
        }
    }

    pub fn sidebar_agent_done(&mut self, pane: f64, window_active: bool) {
        let pane = pane_id(pane);
        if !window_active
            || self
                .active_window()
                .is_none_or(|window| window.active_pane != pane)
        {
            self.sidebar.unseen.insert(pane);
        }
    }

    pub fn sidebar_rows(&mut self, window_active: bool) -> String {
        let (rows, scroll) = self.reconcile_sidebar(window_active);
        let snapshot = self.core.snapshot();
        let active_row = rows.iter().rposition(|row| row.on_active_path);
        let selected = (!self.sidebar.pointer_selection)
            .then_some(self.sidebar.selection)
            .flatten();
        let rows = rows
            .iter()
            .enumerate()
            .map(|(index, row)| {
                json!({
                    "key": target_key(row.target),
                    "depth": row.depth,
                    "node": node_name(row.node),
                    "label": row.label,
                    "index": row.index,
                    "onActivePath": row.on_active_path,
                    "active": active_row == Some(index),
                    "selected": self.sidebar.focused && selected == Some(row.target),
                    "expanded": row.expanded,
                    "expandable": row.expandable,
                    "bell": row.bell,
                    "badge": row.badge.map(badge_name),
                    "program": row.program.map(|program| program.id),
                    "add": sidebar_add_command(row.target).map(|(label, _)| label),
                    "close": sidebar_close_command(row.target).map(|(label, _)| label),
                    "rename": sidebar_rename_command(snapshot, row.target).map(|(label, _)| label),
                })
            })
            .collect::<Vec<_>>();
        json!({ "rows": rows, "scroll": scroll, "focused": self.sidebar.focused }).to_string()
    }

    pub fn sidebar_toggle(&mut self, key: &str, pointer: bool) {
        let Some(target) = parse_target(key) else {
            return;
        };
        self.sidebar.selection = Some(target);
        self.sidebar.pointer_selection = pointer;
        if !self.sidebar.collapsed.remove(&target) {
            self.sidebar.collapsed.insert(target);
        }
    }

    pub fn sidebar_select(&mut self, key: &str, pointer: bool) {
        if let Some(target) = parse_target(key) {
            self.sidebar.selection = Some(target);
            self.sidebar.pointer_selection = pointer;
        }
    }

    pub fn sidebar_activate(&mut self, key: &str) -> bool {
        let Some(target) = parse_target(key) else {
            return false;
        };
        let activation =
            sidebar_activation(self.core.snapshot(), self.core.attached_session(), target);
        if let Some(session) = activation.attach {
            self.attach(&session.to_string());
        }
        for command in activation.commands {
            self.invoke(command);
        }
        true
    }

    pub fn sidebar_command(&mut self, key: &str, action: &str) {
        let Some(target) = parse_target(key) else {
            return;
        };
        if self.core.attached_read_only() {
            return;
        }
        let command = match action {
            "add" => sidebar_add_command(target),
            "close" => sidebar_close_command(target),
            "rename" => sidebar_rename_command(self.core.snapshot(), target),
            _ => None,
        };
        if let Some((_, command)) = command {
            self.invoke(command);
        }
    }

    pub fn sidebar_key(&mut self, action: &str, window_active: bool) -> Option<String> {
        let direction = match action {
            "sidebar-select-up" => TreeNavigation::Up,
            "sidebar-select-down" => TreeNavigation::Down,
            "sidebar-select-first" => TreeNavigation::First,
            "sidebar-select-last" => TreeNavigation::Last,
            "sidebar-select-left" => TreeNavigation::Left,
            "sidebar-select-right" => TreeNavigation::Right,
            _ => return self.sidebar.selection.map(target_key),
        };
        let (rows, _) = self.reconcile_sidebar(window_active);
        let selected = rows
            .iter()
            .position(|row| Some(row.target) == self.sidebar.selection);
        self.sidebar.pointer_selection = false;
        match tree_navigation(&navigation_rows(&rows), selected, direction) {
            TreeNavigationResult::Select(index) => {
                self.sidebar.selection = Some(rows[index].target);
            }
            TreeNavigationResult::Toggle(index) => {
                let target = rows[index].target;
                self.sidebar.selection = Some(target);
                if !self.sidebar.collapsed.remove(&target) {
                    self.sidebar.collapsed.insert(target);
                }
            }
            TreeNavigationResult::None => {}
        }
        self.sidebar.selection.map(target_key)
    }

    pub fn flat_sidebar(&self, now: f64) -> String {
        let snapshot = self.core.snapshot();
        let now = self.sidebar.clock.now(now as u64);
        let (sessions, rollup) = summarize_host(snapshot);
        let current = self
            .core
            .attached_session()
            .and_then(|attached| {
                snapshot
                    .sessions
                    .iter()
                    .find(|session| session.id == attached)
            })
            .map(|session| snapshot.focused_window_for(session).0);
        let sessions = sessions
            .into_iter()
            .map(|session| {
                let windows = session
                    .windows
                    .iter()
                    .map(|window| {
                        let age = age_label(window.changed_at, now);
                        let detail = window.detail.iter().map(|part| part.text.as_str()).collect::<String>();
                        let description = [Some(detail.as_str()), attention_word(window.attention), age.as_deref()]
                            .into_iter()
                            .flatten()
                            .filter(|part| !part.is_empty())
                            .collect::<Vec<_>>()
                            .join(", ");
                        json!({
                            "id": window.window.0,
                            "title": window.title,
                            "detail": window.detail.iter().map(|part| json!({ "text": part.text, "tone": tone_name(part.tone) })).collect::<Vec<_>>(),
                            "age": age,
                            "description": description,
                            "working": window.attention == Attention::Working,
                            "current": current == Some(window.window.0),
                            "tiles": window.tiles.iter().map(|tile| json!({
                                "pane": tile.pane.0,
                                "tag": tag_name(tile.tag),
                                "label": tile.label,
                                "program": tile.program.map(|program| program.id),
                            })).collect::<Vec<_>>(),
                        })
                    })
                    .collect::<Vec<_>>();
                json!({
                    "id": session.session.0,
                    "label": session.label.text,
                    "index": session.label.index,
                    "rollup": rollup_json(&session.rollup),
                    "windows": windows,
                })
            })
            .collect::<Vec<_>>();
        json!({
            "name": "zz daemon",
            "rollup": rollup_json(&rollup),
            "sessions": sessions,
        })
        .to_string()
    }
}

impl Client {
    fn window(&self, window: f64) -> Option<&WindowSnapshot> {
        let id = WindowId(window as u64);
        self.core
            .snapshot()
            .sessions
            .iter()
            .flat_map(|session| &session.windows)
            .find(|candidate| candidate.id == id)
    }

    fn active_window(&self) -> Option<&WindowSnapshot> {
        let snapshot = self.core.snapshot();
        let session = snapshot
            .sessions
            .iter()
            .find(|session| Some(session.id) == self.core.attached_session())?;
        session
            .windows
            .iter()
            .find(|window| window.id == snapshot.focused_window_for(session))
    }

    fn reconcile_sidebar(
        &mut self,
        window_active: bool,
    ) -> (Vec<zz_client::sidebar::SidebarRow>, Option<usize>) {
        let core = &self.core;
        let sidebar = &mut self.sidebar;
        sidebar
            .unseen
            .retain(|pane| core.agent_state(*pane).is_some());
        let snapshot = core.snapshot();
        let attached = core.attached_session();
        let active = sidebar_active(snapshot, attached);
        if window_active && let SidebarTarget::Pane(pane) = active {
            sidebar.unseen.remove(&pane);
        }
        let changed = sidebar.active != Some(active);
        if changed {
            sidebar.active = Some(active);
            for target in sidebar_active_path(snapshot, attached) {
                sidebar.collapsed.remove(&target);
            }
            if !sidebar.focused || sidebar.pointer_selection {
                sidebar.selection = Some(active);
            }
        }
        let rows = sidebar_rows(core, &sidebar.collapsed, &sidebar.unseen);
        let scroll = changed
            .then(|| rows.iter().position(|row| row.target == active))
            .flatten();
        if !rows.iter().any(|row| Some(row.target) == sidebar.selection) {
            sidebar.selection = rows
                .iter()
                .rfind(|row| row.on_active_path)
                .map(|row| row.target)
                .or(Some(SidebarTarget::Host));
        }
        (rows, scroll)
    }

    fn window_json(&self, window: &WindowSnapshot, focused: bool) -> Value {
        let panes = ordered_panes(window)
            .into_iter()
            .map(|pane| {
                json!({
                    "id": pane.id.0,
                    "label": pane_label(pane),
                    "kind": kind(&pane.kind),
                    "dead": pane.dead,
                    "deadStatus": pane.dead_status,
                    "deadLabel": pane.dead.then(|| pane.dead_status.map_or_else(|| "Dead".to_owned(), |status| format!("Dead · {status}"))),
                    "waiting": matches!(pane.kind, PaneKindSnapshot::Terminal) && self.core.viewport(pane.id).is_none(),
                    "bell": pane.bell,
                    "synchronized": pane.synchronized_input,
                    "mode": pane.mode.as_ref().map(|mode| format!("{mode:?}")),
                    "status": serde_json::to_value(&pane.status).unwrap_or(Value::Null),
                    "summary": serde_json::to_value(&pane.summary).unwrap_or(Value::Null),
                    "browser": match &pane.kind {
                        PaneKindSnapshot::Browser(browser) => json!({
                            "tabs": browser.tabs,
                            "active": browser.active_tab,
                            "url": browser.tabs.get(browser.active_tab),
                        }),
                        _ => Value::Null,
                    },
                })
            })
            .collect::<Vec<_>>();
        json!({
            "id": window.id.0,
            "index": window.index,
            "name": window.name,
            "automaticRename": window.automatic_rename,
            "focused": focused,
            "activePane": window.active_pane.0,
            "zoomedPane": window.zoomed_pane.map(|pane| pane.0),
            "activity": window.activity,
            "silence": window.silence,
            "layout": layout_json(&window.layout, window.active_pane),
            "panes": panes,
            "floating": serde_json::to_value(&window.floating).unwrap_or(Value::Null),
            "modal": serde_json::to_value(window.modal).unwrap_or(Value::Null),
            "columns": window.sx,
            "rows": window.sy,
        })
    }
}

fn pip_geometry(
    window: &WindowSnapshot,
    cell: (f32, f32),
    canvas: (f32, f32),
    lines: PaneBorderLines,
    header: f32,
) -> PipGeometry {
    PipGeometry {
        window: (window.sx, window.sy),
        cell,
        canvas,
        lines,
        header,
    }
}

fn key_input(key: &str, shift: bool, control: bool, alt: bool, meta: bool) -> KeyInput {
    let code = key_code(key);
    let character = match code {
        KeyCode::Character(value) => Some(value),
        _ => None,
    };
    KeyInput {
        action: KeyAction::Press,
        key: code,
        modifiers: Modifiers::new(shift, control, alt, meta),
        text: character
            .filter(|_| !control && !alt && !meta)
            .map(|value| value.to_string().into_boxed_str()),
        unshifted_codepoint: character,
    }
}

fn browser_key_name(key: &str) -> String {
    match key {
        "ArrowDown" => "down".into(),
        "ArrowUp" => "up".into(),
        "Enter" => "enter".into(),
        "Escape" => "escape".into(),
        other => other.to_lowercase(),
    }
}

fn pane_id(pane: f64) -> PaneId {
    PaneId(pane as u64)
}

fn rect(slot: &[f32]) -> PaneRect {
    PaneRect {
        x: slot.first().copied().unwrap_or_default(),
        y: slot.get(1).copied().unwrap_or_default(),
        width: slot.get(2).copied().unwrap_or_default(),
        height: slot.get(3).copied().unwrap_or_default(),
    }
}

const fn zone_name(zone: DropZone) -> &'static str {
    match zone {
        DropZone::Left => "left",
        DropZone::Right => "right",
        DropZone::Top => "top",
        DropZone::Bottom => "bottom",
        DropZone::Center => "center",
    }
}

fn parse_zone(zone: &str) -> Option<DropZone> {
    Some(match zone {
        "left" => DropZone::Left,
        "right" => DropZone::Right,
        "top" => DropZone::Top,
        "bottom" => DropZone::Bottom,
        "center" => DropZone::Center,
        _ => return None,
    })
}

fn target_key(target: SidebarTarget) -> String {
    match target {
        SidebarTarget::Host => "host".into(),
        SidebarTarget::Session(id) => format!("session-{id}"),
        SidebarTarget::Window(id) => format!("window-{id}"),
        SidebarTarget::Pane(id) => format!("pane-{id}"),
    }
}

fn parse_target(key: &str) -> Option<SidebarTarget> {
    if key == "host" {
        return Some(SidebarTarget::Host);
    }
    let (kind, id) = key.split_once('-')?;
    Some(match kind {
        "session" => SidebarTarget::Session(id.parse().ok()?),
        "window" => SidebarTarget::Window(id.parse().ok()?),
        "pane" => SidebarTarget::Pane(id.parse().ok()?),
        _ => return None,
    })
}

const fn node_name(node: SidebarNode) -> &'static str {
    match node {
        SidebarNode::Host => "host",
        SidebarNode::Session => "session",
        SidebarNode::Window => "window",
        SidebarNode::Pane(tag) => tag_name(tag),
    }
}

const fn tag_name(tag: PaneTag) -> &'static str {
    match tag {
        PaneTag::Terminal => "terminal",
        PaneTag::Browser => "browser",
        PaneTag::Editor => "editor",
        PaneTag::Picker => "picker",
    }
}

fn tag(kind: &PaneKindSnapshot) -> &'static str {
    tag_name(zz_client::window_summary::pane_tag(kind))
}

const fn badge_name(badge: SidebarBadge) -> &'static str {
    match badge {
        SidebarBadge::NeedsInput => "warning",
        SidebarBadge::Failed => "danger",
        SidebarBadge::Working => "working",
        SidebarBadge::Finished => "success",
    }
}

const fn tone_name(tone: Tone) -> &'static str {
    match tone {
        Tone::Normal => "normal",
        Tone::Muted => "muted",
        Tone::Warning => "warning",
        Tone::Danger => "danger",
    }
}

const fn attention_word(attention: Attention) -> Option<&'static str> {
    match attention {
        Attention::NeedsYou => Some("needs you"),
        Attention::Failed => Some("failed"),
        Attention::Done => Some("unseen"),
        Attention::Bell => Some("bell"),
        Attention::Working => Some("working"),
        Attention::None => None,
    }
}

fn rollup_json(rollup: &zz_client::window_summary::AttentionRollup) -> Value {
    json!({
        "warning": rollup.needs_you.saturating_add(rollup.bell),
        "danger": rollup.failed,
        "success": rollup.done,
        "working": rollup.working,
    })
}

pub fn kind(kind: &PaneKindSnapshot) -> &'static str {
    match kind {
        PaneKindSnapshot::Picker => "picker",
        PaneKindSnapshot::Terminal => "terminal",
        PaneKindSnapshot::Browser(_) => "browser",
        PaneKindSnapshot::Editor(_) => "editor",
    }
}

pub fn layout_json(node: &LayoutNode, active: PaneId) -> Value {
    match node {
        LayoutNode::Pane(pane) => json!({ "pane": pane.0 }),
        LayoutNode::Split {
            id,
            axis,
            ratio,
            first,
            second,
        } => json!({
            "split": id.0,
            "axis": match axis {
                Axis::Horizontal => "h",
                Axis::Vertical => "v",
            },
            "ratio": ratio,
            "highlight": pane_separator(node, active, None).map(|separator| json!({
                "start": separator.span().start(),
                "length": separator.span().length(),
                "side": match separator.side() {
                    SeparatorSide::First => "first",
                    SeparatorSide::Second => "second",
                },
            })),
            "first": layout_json(first, active),
            "second": layout_json(second, active),
        }),
        LayoutNode::Empty => Value::Null,
    }
}
