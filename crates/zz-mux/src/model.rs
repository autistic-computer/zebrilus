use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

#[cfg(test)]
use zz_protocol::LayoutNode;
pub use zz_protocol::layout::{joined_layout, swapped_layout};
use zz_protocol::{
    AgentDescriptor, Axis, BrowserDescriptor, EditorDescriptor, FloatingPaneSnapshot,
    MAX_SIMULATOR_DEVICE_BYTES, ModalPaneSnapshot, MuxSnapshot, PaneBorderIndicators,
    PaneBorderLines, PaneBorderStatus, PaneId, PaneKindSnapshot, PaneSnapshot, PaneStatus,
    PaneSummary, SIMULATOR_PANE_KIND, ServerError, SessionId, SessionSnapshot, SimulatorDescriptor,
    SplitId, ViewDescriptor, WindowId, WindowSnapshot, normalize_browser_profile_name,
};

use crate::{
    PresetOptions,
    journal::{ChangeJournal, Tracked},
    layout::{
        CellGeometry, CellLayout, LayoutError, LayoutFormat, LeafState, SplitSize, carve_border_row,
    },
};

mod floats;
mod panes;
mod targets;
mod windows;

use panes::*;
pub(crate) use targets::{GlobPattern, NO_MARKED_TARGET, fnmatch, is_marked_target};

pub(crate) const DEFAULT_WINDOW_EXTENT: (u16, u16) = (80, 24);
const SYNCHRONIZE_PANES: u8 = 1 << 0;
const AUTOMATIC_RENAME: u8 = 1 << 1;
const AGGRESSIVE_RESIZE: u8 = 1 << 2;
const LAYOUT_COORDINATE_MAX: u32 = 1_000_000;
const MAX_WINDOW_INDEX: u32 = i32::MAX.cast_unsigned();

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaneDirection {
    Left,
    Right,
    Up,
    Down,
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayoutPreset {
    EvenHorizontal,
    EvenVertical,
    MainHorizontal,
    MainHorizontalMirrored,
    MainVertical,
    MainVerticalMirrored,
    Tiled,
}

impl LayoutPreset {
    pub const ALL: [Self; 7] = [
        Self::EvenHorizontal,
        Self::EvenVertical,
        Self::MainHorizontal,
        Self::MainHorizontalMirrored,
        Self::MainVertical,
        Self::MainVerticalMirrored,
        Self::Tiled,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::EvenHorizontal => "even-horizontal",
            Self::EvenVertical => "even-vertical",
            Self::MainHorizontal => "main-horizontal",
            Self::MainHorizontalMirrored => "main-horizontal-mirrored",
            Self::MainVertical => "main-vertical",
            Self::MainVerticalMirrored => "main-vertical-mirrored",
            Self::Tiled => "tiled",
        }
    }

    #[must_use]
    const fn at_offset(self, offset: isize) -> Self {
        let current = self as isize;
        let count = 7_isize;
        Self::ALL[(current + offset).rem_euclid(count) as usize]
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct PaneRect {
    left: u32,
    top: u32,
    right: u32,
    bottom: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct InputOptions {
    values: u8,
    overrides: u8,
}

impl InputOptions {
    const fn synchronize_panes(self) -> Option<bool> {
        if self.overrides & SYNCHRONIZE_PANES == 0 {
            None
        } else {
            Some(self.values & SYNCHRONIZE_PANES != 0)
        }
    }

    fn set_synchronize_panes(&mut self, value: Option<bool>) {
        if let Some(value) = value {
            self.overrides |= SYNCHRONIZE_PANES;
            if value {
                self.values |= SYNCHRONIZE_PANES;
            } else {
                self.values &= !SYNCHRONIZE_PANES;
            }
        } else {
            self.values &= !SYNCHRONIZE_PANES;
            self.overrides &= !SYNCHRONIZE_PANES;
        }
    }

    const fn automatic_rename(self) -> Option<bool> {
        if self.overrides & AUTOMATIC_RENAME == 0 {
            None
        } else {
            Some(self.values & AUTOMATIC_RENAME != 0)
        }
    }

    fn set_automatic_rename(&mut self, value: Option<bool>) {
        if let Some(value) = value {
            self.overrides |= AUTOMATIC_RENAME;
            if value {
                self.values |= AUTOMATIC_RENAME;
            } else {
                self.values &= !AUTOMATIC_RENAME;
            }
        } else {
            self.values &= !AUTOMATIC_RENAME;
            self.overrides &= !AUTOMATIC_RENAME;
        }
    }

    const fn aggressive_resize(self) -> Option<bool> {
        if self.overrides & AGGRESSIVE_RESIZE == 0 {
            None
        } else {
            Some(self.values & AGGRESSIVE_RESIZE != 0)
        }
    }

    fn set_aggressive_resize(&mut self, value: Option<bool>) {
        if let Some(value) = value {
            self.overrides |= AGGRESSIVE_RESIZE;
            if value {
                self.values |= AGGRESSIVE_RESIZE;
            } else {
                self.values &= !AGGRESSIVE_RESIZE;
            }
        } else {
            self.values &= !AGGRESSIVE_RESIZE;
            self.overrides &= !AGGRESSIVE_RESIZE;
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PaneKind {
    Picker { inherit_cwd_from: Option<PaneId> },
    Terminal,
    Browser(BrowserDescriptor),
    Editor(EditorDescriptor),
    Agent(AgentDescriptor),
    View(ViewDescriptor),
    Simulator(SimulatorDescriptor),
}

impl PaneKind {
    fn snapshot(&self) -> PaneKindSnapshot {
        match self {
            Self::Picker { .. } => PaneKindSnapshot::Picker,
            Self::Terminal => PaneKindSnapshot::Terminal,
            Self::Browser(browser) => PaneKindSnapshot::Browser(browser.clone()),
            Self::Editor(editor) => PaneKindSnapshot::Editor(editor.clone()),
            Self::Agent(agent) => PaneKindSnapshot::Agent(agent.clone()),
            Self::View(view) => PaneKindSnapshot::View(view.clone()),
            Self::Simulator(_) => PaneKindSnapshot::Other {
                kind: SIMULATOR_PANE_KIND.to_owned(),
            },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SplitPlacement {
    pub size: SplitSize,
    pub before: bool,
    pub full_size: bool,
    pub detached: bool,
}

impl Default for SplitPlacement {
    fn default() -> Self {
        Self {
            size: SplitSize::Default,
            before: false,
            full_size: false,
            detached: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pane {
    pub id: PaneId,
    pub title: String,
    pub kind: PaneKind,
    pub active_point: u64,
    pub bell: bool,
    pub status: Option<PaneStatus>,
    pub summary: PaneSummary,
    pub dead: bool,
    pub dead_status: Option<u32>,
    pub dead_time: Option<u64>,
    pub(crate) input_off: bool,
    pub(crate) empty: bool,
    pub(crate) screen_extent: Option<(u16, u16)>,
    title_pinned: bool,
    pub over_zoom: bool,
    input_options: InputOptions,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Window {
    pub id: WindowId,
    pub session: SessionId,
    pub index: u32,
    pub name: String,
    pub created: u64,
    pub activity: u64,
    pub activity_time: Option<i64>,
    pub activity_flag: bool,
    pub silence_flag: bool,
    pub active_pane: PaneId,
    pub zoomed_pane: Option<PaneId>,
    pub layout: CellLayout,
    pub panes: BTreeMap<PaneId, Pane>,
    pane_order: Vec<PaneId>,
    z_order: Vec<PaneId>,
    last_panes: Vec<PaneId>,
    last_layout: Option<LayoutPreset>,
    previous_layout: Option<Box<SavedLayout>>,
    pub(crate) last_extent_probe: Option<(PaneId, u16, u16)>,
    pub(crate) manual_extent: (u16, u16),
    pub floats: WindowFloats,
    input_options: InputOptions,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FloatZ {
    Front,
    Back,
    Forward,
    Backward,
    ForwardLoop,
    BackwardLoop,
    Index(u32),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Modal {
    pub pane: PaneId,
    pub last: Option<PaneId>,
    pub capture_keys: bool,
    pub close_on_click: bool,
    pub close_on_cancel: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WindowFloats {
    pub modal: Option<Modal>,
    pub last_new_pane: (i32, i32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PushedZoom {
    No,
    Was(Option<PaneId>),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct FloatSpawn {
    pub geometry: CellGeometry,
    pub over_zoom: bool,
    pub modal: Option<Modal>,
    pub detached: bool,
    pub before: bool,
    pub full_size: bool,
}

#[derive(Clone, Debug, PartialEq)]
struct SavedLayout {
    layout: CellLayout,
    selection: Option<SavedSelection>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SavedSelection {
    pane_order: Vec<PaneId>,
    active: PaneId,
    last_panes: Vec<PaneId>,
    z_order: Vec<PaneId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Session {
    pub id: SessionId,
    pub name: String,
    working_directory: Option<PathBuf>,
    pub created: Option<i64>,
    pub activity: Option<i64>,
    pub sort_created: u64,
    pub sort_activity: u64,
    pub windows: Vec<WindowId>,
    pub active_window: WindowId,
    last_window: Option<WindowId>,
}

impl Session {
    #[must_use]
    pub fn last_window(&self) -> Option<WindowId> {
        self.last_window
    }

    fn activate_window(&mut self, window: WindowId) -> bool {
        if self.active_window == window {
            false
        } else {
            self.last_window = Some(self.active_window);
            self.active_window = window;
            true
        }
    }

    fn forget_window(&mut self, window: WindowId) -> Option<WindowId> {
        self.windows.retain(|candidate| *candidate != window);
        if self.last_window == Some(window) {
            self.last_window = None;
        }
        if self.active_window == window && !self.windows.is_empty() {
            self.active_window = self
                .last_window
                .take()
                .filter(|candidate| self.windows.contains(candidate))
                .unwrap_or(self.windows[0]);
            Some(self.active_window)
        } else {
            None
        }
    }
}

impl Window {
    #[must_use]
    pub fn displayed_pane_geometry(&self, pane: PaneId) -> Option<(u16, u16)> {
        self.displayed_pane_cell(pane, PaneBorderStatus::Off)
            .map(|geometry| (geometry.sx, geometry.sy))
    }

    #[must_use]
    pub(crate) fn displayed_pane_cell(
        &self,
        pane: PaneId,
        status: PaneBorderStatus,
    ) -> Option<CellGeometry> {
        if self.zoomed_pane == Some(pane) {
            let (sx, sy) = self.layout.extent();
            let full = CellGeometry {
                sx,
                sy,
                xoff: 0,
                yoff: 0,
            };
            return Some(carve_border_row(full, full, status));
        }
        if self.zoomed_pane.is_some()
            && let Some((sx, sy)) = self.panes.get(&pane).and_then(|pane| pane.screen_extent)
        {
            return Some(CellGeometry {
                sx,
                sy,
                xoff: 0,
                yoff: 0,
            });
        }
        self.layout.pane_geometry_with_border(pane, status)
    }

    #[must_use]
    pub fn displayed_pane_geometry_with_border(
        &self,
        pane: PaneId,
        status: PaneBorderStatus,
    ) -> Option<(u16, u16)> {
        self.displayed_pane_cell(pane, status)
            .map(|geometry| (geometry.sx, geometry.sy))
    }

    fn clear_pane_screen_extents(&mut self) {
        for pane in self.panes.values_mut() {
            pane.screen_extent = None;
        }
    }

    #[must_use]
    pub fn pane_order(&self) -> &[PaneId] {
        &self.pane_order
    }

    #[must_use]
    pub fn z_order(&self) -> &[PaneId] {
        &self.z_order
    }

    #[must_use]
    pub fn last_panes(&self) -> &[PaneId] {
        &self.last_panes
    }

    pub(crate) fn last_pane(&self) -> Option<PaneId> {
        self.last_panes.first().copied()
    }

    #[must_use]
    pub fn layout_string(&self, format: LayoutFormat, pane_base_index: u32) -> String {
        self.layout
            .dump_as(format, &|pane| self.leaf_state(pane, pane_base_index))
    }

    #[must_use]
    pub fn visible_layout_string(&self, format: LayoutFormat, pane_base_index: u32) -> String {
        let Some(zoomed) = self.zoomed_pane else {
            return self.layout_string(format, pane_base_index);
        };
        let (width, height) = self.layout.extent();
        let mut visible = CellLayout::new(zoomed, width, height);
        let mut next_id = 0;
        let mut ids = || {
            next_id += 1;
            SplitId(next_id)
        };
        for pane in &self.pane_order {
            if *pane == zoomed || !self.shows_floating(*pane) {
                continue;
            }
            if let Some(geometry) = self.layout.pane_geometry(*pane) {
                let _ = visible.float(zoomed, *pane, geometry, &mut ids);
            }
        }
        visible.dump_as(format, &|pane| {
            let mut state = self.leaf_state(pane, pane_base_index);
            state.z = self.shows_floating(pane).then(|| {
                let before = self
                    .z_order
                    .iter()
                    .take_while(|candidate| **candidate != pane)
                    .filter(|candidate| self.shows_floating(**candidate))
                    .count();
                u32::try_from(before).unwrap_or(u32::MAX)
            });
            state
        })
    }

    fn saved_layout(&self, legacy: bool) -> SavedLayout {
        let selection = (!legacy).then(|| SavedSelection {
            pane_order: self.pane_order.clone(),
            active: self.active_pane,
            last_panes: self
                .last_panes
                .iter()
                .copied()
                .filter(|pane| *pane != self.active_pane)
                .collect(),
            z_order: self.z_order.clone(),
        });
        SavedLayout {
            layout: self.layout.clone(),
            selection,
        }
    }

    fn leaf_state(&self, pane: PaneId, pane_base_index: u32) -> LeafState {
        let position = |panes: &[PaneId]| {
            panes
                .iter()
                .position(|candidate| *candidate == pane)
                .and_then(|position| u32::try_from(position).ok())
        };
        LeafState {
            active: pane == self.active_pane,
            last: position(&self.last_panes),
            index: pane_base_index.saturating_add(position(&self.pane_order).unwrap_or_default()),
            z: self.layout.is_floating(pane).then(|| {
                let zoomed_float = self.zoomed_pane == Some(self.active_pane)
                    && self.layout.is_floating(self.active_pane);
                match (zoomed_float, pane == self.active_pane) {
                    (true, true) => 0,
                    (true, false) => self.floats_before(pane).saturating_add(1),
                    (false, _) => self.floats_before(pane),
                }
            }),
        }
    }

    fn floats_before(&self, pane: PaneId) -> u32 {
        let count = self
            .z_order
            .iter()
            .take_while(|candidate| **candidate != pane)
            .filter(|candidate| self.layout.is_floating(**candidate))
            .count();
        u32::try_from(count).unwrap_or(u32::MAX)
    }

    #[must_use]
    pub fn is_floating(&self, pane: PaneId) -> bool {
        self.layout.is_floating(pane)
    }

    #[must_use]
    pub fn pane_z(&self, pane: PaneId) -> Option<u32> {
        if !self.z_order.contains(&pane) {
            return None;
        }
        let before = self
            .z_order
            .iter()
            .take_while(|candidate| **candidate != pane)
            .filter(|candidate| self.shows_floating(**candidate))
            .count();
        let before = u32::try_from(before).unwrap_or(u32::MAX);
        Some(if self.shows_floating(pane) {
            before
        } else {
            before.saturating_add(1)
        })
    }

    #[must_use]
    pub fn shows_floating(&self, pane: PaneId) -> bool {
        if !self.layout.is_floating(pane) {
            return false;
        }
        match self.zoomed_pane {
            None => true,
            Some(zoomed) => zoomed != pane && self.is_over_zoom(pane),
        }
    }

    #[must_use]
    pub fn is_over_zoom(&self, pane: PaneId) -> bool {
        self.panes.get(&pane).is_some_and(|pane| pane.over_zoom)
    }

    #[must_use]
    pub fn pane_is_visible(&self, pane: PaneId) -> bool {
        match self.zoomed_pane {
            None => true,
            Some(zoomed) => zoomed == pane || self.shows_floating(pane),
        }
    }

    #[must_use]
    pub fn active_is_over_zoom(&self) -> bool {
        self.zoomed_pane.is_some() && self.shows_floating(self.active_pane)
    }

    fn active_floats_over_zoom(&self) -> bool {
        self.is_over_zoom(self.active_pane) && self.layout.is_floating(self.active_pane)
    }

    pub(crate) fn zoom(&mut self, pane: PaneId) -> bool {
        if self.zoomed_pane.is_some() || self.panes.len() <= 1 || !self.panes.contains_key(&pane) {
            return false;
        }
        if self.active_pane != pane && !self.active_floats_over_zoom() {
            activate_window_pane(self, pane, false);
        }
        self.zoomed_pane = Some(pane);
        if self.layout.is_floating(pane) {
            self.lower_to_tail(pane);
        }
        self.clear_pane_screen_extents();
        true
    }

    pub(crate) fn unzoom(&mut self) -> bool {
        let Some(zoomed) = self.zoomed_pane.take() else {
            return false;
        };
        if self.layout.is_floating(zoomed) {
            self.z_order.retain(|candidate| *candidate != zoomed);
            let index = if zoomed == self.active_pane {
                0
            } else {
                self.z_order
                    .iter()
                    .position(|candidate| !self.layout.is_floating(*candidate))
                    .unwrap_or(self.z_order.len())
            };
            self.z_order.insert(index, zoomed);
        }
        self.clear_pane_screen_extents();
        true
    }

    pub(crate) fn push_zoom(&mut self, always: bool, flag: bool) -> PushedZoom {
        let pushed = if flag && (always || self.zoomed_pane.is_some()) {
            PushedZoom::Was(self.zoomed_pane)
        } else {
            PushedZoom::No
        };
        self.unzoom();
        pushed
    }

    pub(crate) fn pop_zoom(&mut self, pushed: PushedZoom) -> bool {
        let PushedZoom::Was(was_zoomed) = pushed else {
            return false;
        };
        let target = if self.active_floats_over_zoom() {
            was_zoomed
                .filter(|pane| self.panes.contains_key(pane))
                .unwrap_or(self.active_pane)
        } else {
            self.active_pane
        };
        self.zoom(target)
    }

    #[must_use]
    pub fn has_floating_panes(&self) -> bool {
        self.panes.keys().any(|pane| self.is_floating(*pane))
    }

    #[must_use]
    pub fn modal_pane(&self) -> Option<PaneId> {
        self.floats.modal.map(|modal| modal.pane)
    }

    fn raise(&mut self, pane: PaneId) {
        self.z_order.retain(|candidate| *candidate != pane);
        self.z_order.insert(0, pane);
    }

    fn lower_to_tail(&mut self, pane: PaneId) {
        self.z_order.retain(|candidate| *candidate != pane);
        self.z_order.push(pane);
    }

    fn insert_float_z(&mut self, pane: PaneId) {
        self.z_order.retain(|candidate| *candidate != pane);
        let index = self
            .floats
            .modal
            .and_then(|modal| {
                self.z_order
                    .iter()
                    .position(|candidate| *candidate == modal.pane)
            })
            .map_or(0, |index| index + 1);
        self.z_order.insert(index, pane);
    }

    fn hidden_by_zoom(&self, pane: PaneId) -> bool {
        self.zoomed_pane
            .is_some_and(|zoomed| zoomed != pane && !self.is_over_zoom(pane))
    }

    pub(crate) fn move_float_z(&mut self, pane: PaneId, position: FloatZ) {
        let hidden = |window: &Self, candidate: PaneId| window.is_floating(candidate);
        let current = self.z_order.iter().position(|candidate| *candidate == pane);
        let Some(current) = current else {
            return;
        };
        match position {
            FloatZ::Front => self.raise(pane),
            FloatZ::Back => {
                self.z_order.remove(current);
                let index = self
                    .z_order
                    .iter()
                    .position(|candidate| !hidden(self, *candidate))
                    .unwrap_or(self.z_order.len());
                self.z_order.insert(index, pane);
            }
            FloatZ::Forward | FloatZ::ForwardLoop => {
                let previous = self.z_order[..current]
                    .iter()
                    .rev()
                    .copied()
                    .find(|candidate| !self.hidden_by_zoom(*candidate));
                if let Some(previous) = previous {
                    self.z_order.remove(current);
                    let index = self
                        .z_order
                        .iter()
                        .position(|candidate| *candidate == previous)
                        .unwrap_or(0);
                    self.z_order.insert(index, pane);
                } else if position == FloatZ::ForwardLoop {
                    self.z_order.remove(current);
                    let index = self
                        .z_order
                        .iter()
                        .position(|candidate| !hidden(self, *candidate))
                        .unwrap_or(self.z_order.len());
                    self.z_order.insert(index, pane);
                }
            }
            FloatZ::Backward | FloatZ::BackwardLoop => {
                let next = self.z_order[current + 1..]
                    .iter()
                    .copied()
                    .find(|candidate| !self.hidden_by_zoom(*candidate));
                if let Some(next) = next.filter(|next| self.shows_floating(*next)) {
                    self.z_order.remove(current);
                    let index = self
                        .z_order
                        .iter()
                        .position(|candidate| *candidate == next)
                        .map_or(self.z_order.len(), |index| index + 1);
                    self.z_order.insert(index, pane);
                } else if position == FloatZ::BackwardLoop {
                    self.raise(pane);
                }
            }
            FloatZ::Index(z) => {
                self.z_order.remove(current);
                let floats = self
                    .z_order
                    .iter()
                    .take_while(|candidate| hidden(self, **candidate))
                    .count();
                let index = floats.min(usize::try_from(z).unwrap_or(usize::MAX));
                self.z_order.insert(index, pane);
            }
        }
    }
}

#[derive(Debug, Default)]
pub struct MuxState {
    generation: u64,
    next_session_id: u64,
    next_window_id: u64,
    next_pane_id: u64,
    next_split_id: u64,
    next_sort_point: u64,
    format_now: u64,
    default_pane_title: String,
    last_active_session: Option<SessionId>,
    input_options: InputOptions,
    marked_pane: Option<(SessionId, WindowId, PaneId)>,
    pub(crate) global_pane_base_index: u32,
    pub(crate) window_pane_base_indices: BTreeMap<WindowId, u32>,
    pub(crate) legacy_layout_saves: bool,
    pub sessions: Tracked<SessionId, Session>,
    pub windows: Tracked<WindowId, Window>,
    pub(crate) journal: ChangeJournal,
}

impl MuxState {
    #[must_use]
    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub(crate) fn set_default_pane_title(&mut self, title: impl Into<String>) {
        self.default_pane_title = title.into();
    }

    pub(crate) const fn set_format_now(&mut self, now: u64) {
        self.format_now = now;
    }

    pub(crate) const fn format_now(&self) -> u64 {
        self.format_now
    }

    fn format_time(&self) -> Option<i64> {
        i64::try_from(self.format_now)
            .ok()
            .filter(|time| *time != 0)
    }

    pub(crate) const fn next_session_id(&self) -> SessionId {
        SessionId(self.next_session_id)
    }

    #[must_use]
    pub fn session_working_directory(&self, session: SessionId) -> Option<&Path> {
        self.sessions
            .get(&session)
            .and_then(|session| session.working_directory.as_deref())
    }

    pub fn set_session_working_directory(
        &mut self,
        session: SessionId,
        working_directory: PathBuf,
    ) -> Result<bool, ServerError> {
        let state = self
            .sessions
            .get_mut(&mut self.journal, &session)
            .ok_or_else(|| ServerError::MissingTarget(session.to_string()))?;
        if state.working_directory.as_ref() == Some(&working_directory) {
            return Ok(false);
        }
        state.working_directory = Some(working_directory);
        self.bump_generation();
        Ok(true)
    }

    pub fn create_session(
        &mut self,
        name: impl Into<String>,
    ) -> Result<(SessionId, WindowId, PaneId), ServerError> {
        self.create_session_with_extent(name, DEFAULT_WINDOW_EXTENT)
    }

    pub(crate) fn create_session_with_extent(
        &mut self,
        name: impl Into<String>,
        extent: (u16, u16),
    ) -> Result<(SessionId, WindowId, PaneId), ServerError> {
        self.create_session_with_extent_at(name, extent, 0)
    }

    pub(crate) fn create_session_with_extent_at(
        &mut self,
        name: impl Into<String>,
        extent: (u16, u16),
        index: u32,
    ) -> Result<(SessionId, WindowId, PaneId), ServerError> {
        let name = name.into();
        if self.sessions.values().any(|session| session.name == name) {
            return Err(ServerError::InvalidCommand(format!(
                "duplicate session: {name}"
            )));
        }
        let session_id = self.allocate_session_id();
        let window_id = self.allocate_window_id();
        let pane_id = self.allocate_pane_id();
        let created = self.allocate_sort_point();
        let activity_time = self.format_time();
        let active_point = self.allocate_sort_point();
        let pane = Pane {
            id: pane_id,
            title: pane_title(&PaneKind::Terminal, &self.default_pane_title),
            kind: PaneKind::Terminal,
            active_point,
            bell: false,
            status: None,
            summary: PaneSummary::default(),
            dead: false,
            dead_status: None,
            dead_time: None,
            input_off: false,
            screen_extent: None,
            empty: false,
            title_pinned: false,
            over_zoom: false,
            input_options: InputOptions::default(),
        };
        let window = Window {
            id: window_id,
            session: session_id,
            index,
            name: index.to_string(),
            created,
            activity: created,
            activity_time,
            activity_flag: false,
            silence_flag: false,
            active_pane: pane_id,
            zoomed_pane: None,
            layout: CellLayout::new(pane_id, extent.0, extent.1),
            panes: BTreeMap::from([(pane_id, pane)]),
            pane_order: vec![pane_id],
            z_order: vec![pane_id],
            last_panes: Vec::new(),
            last_layout: None,
            previous_layout: None,
            last_extent_probe: None,
            manual_extent: extent,
            floats: WindowFloats::default(),
            input_options: InputOptions::default(),
        };
        self.windows.insert(&mut self.journal, window_id, window);
        self.sessions.insert(
            &mut self.journal,
            session_id,
            Session {
                id: session_id,
                name,
                working_directory: None,
                created: None,
                activity: None,
                sort_created: created,
                sort_activity: created,
                windows: vec![window_id],
                active_window: window_id,
                last_window: None,
            },
        );
        self.last_active_session = Some(session_id);
        self.bump_generation();
        Ok((session_id, window_id, pane_id))
    }

    pub fn rename_session(
        &mut self,
        session: SessionId,
        name: impl Into<String>,
    ) -> Result<(), ServerError> {
        let name = name.into();
        let current = self
            .sessions
            .get(&session)
            .ok_or_else(|| ServerError::MissingTarget(session.to_string()))?;
        if current.name == name {
            return Ok(());
        }
        if self
            .sessions
            .values()
            .any(|candidate| candidate.name == name)
        {
            return Err(ServerError::InvalidCommand(format!(
                "duplicate session: {name}"
            )));
        }
        self.sessions
            .get_mut(&mut self.journal, &session)
            .expect("session target was resolved")
            .name = name;
        self.bump_generation();
        Ok(())
    }

    pub fn kill_session(&mut self, session: SessionId) -> Result<Vec<PaneId>, ServerError> {
        let session = self
            .sessions
            .remove(&mut self.journal, &session)
            .ok_or_else(|| ServerError::MissingTarget(session.to_string()))?;
        let mut panes = Vec::new();
        for window in session.windows {
            if let Some(window) = self.windows.remove(&mut self.journal, &window) {
                panes.extend(window.pane_order);
            }
        }
        self.bump_generation();
        Ok(panes)
    }

    pub fn resize_pane(
        &mut self,
        pane: PaneId,
        axis: Axis,
        delta_cells: i32,
    ) -> Result<(), ServerError> {
        let window_id = self
            .window_for_pane(pane)
            .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
        let window = self
            .windows
            .get_mut(&mut self.journal, &window_id)
            .expect("window exists");
        window
            .layout
            .resize_pane(pane, axis, delta_cells)
            .map_err(|error| pane_layout_error(error, pane))?;
        window.zoomed_pane = None;
        self.bump_generation();
        Ok(())
    }

    pub fn resize_pane_to(
        &mut self,
        pane: PaneId,
        axis: Axis,
        cells: u16,
    ) -> Result<(), ServerError> {
        let window_id = self
            .window_for_pane(pane)
            .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
        let window = self
            .windows
            .get_mut(&mut self.journal, &window_id)
            .expect("window exists");
        window
            .layout
            .resize_pane_to(pane, axis, cells)
            .map_err(|error| pane_layout_error(error, pane))?;
        window.zoomed_pane = None;
        self.bump_generation();
        Ok(())
    }

    pub(crate) fn resize_window(
        &mut self,
        window: WindowId,
        columns: u16,
        rows: u16,
    ) -> Result<(), ServerError> {
        let window = self
            .windows
            .get_mut(&mut self.journal, &window)
            .ok_or_else(|| ServerError::MissingTarget(window.to_string()))?;
        let before = window.layout.extent();
        window.layout.resize(columns, rows);
        window.last_extent_probe = None;
        window.clear_pane_screen_extents();
        if window.layout.extent() != before {
            self.bump_generation();
        }
        Ok(())
    }

    pub fn resize_split(
        &mut self,
        window: WindowId,
        split: SplitId,
        ratio: f32,
    ) -> Result<bool, ServerError> {
        if !ratio.is_finite() {
            return Err(ServerError::InvalidCommand(
                "split ratio must be finite".to_owned(),
            ));
        }
        let window = self
            .windows
            .get_mut(&mut self.journal, &window)
            .ok_or_else(|| ServerError::MissingTarget(window.to_string()))?;
        let changed = window
            .layout
            .set_divider_ratio(split, ratio)
            .map_err(|error| divider_layout_error(error, split))?;
        if changed {
            window.zoomed_pane = None;
            self.bump_generation();
        }
        Ok(changed)
    }

    pub fn select_layout(
        &mut self,
        window: WindowId,
        preset: LayoutPreset,
        options: &PresetOptions,
    ) -> Result<(), ServerError> {
        let panes = self
            .windows
            .get(&window)
            .ok_or_else(|| ServerError::MissingTarget(window.to_string()))?
            .pane_order
            .clone();
        debug_assert!(!panes.is_empty(), "validated windows are never empty");
        let next_split_id = &mut self.next_split_id;
        let mut ids = || {
            let id = SplitId(*next_split_id);
            *next_split_id = (*next_split_id).saturating_add(1);
            id
        };

        let window = self
            .windows
            .get_mut(&mut self.journal, &window)
            .expect("window was resolved");
        let previous = window.saved_layout(self.legacy_layout_saves);
        window
            .layout
            .apply_preset(preset, &panes, options, &mut ids);
        window.previous_layout = Some(Box::new(previous));
        window.last_layout = Some(preset);
        self.bump_generation();
        Ok(())
    }

    pub fn select_layout_string(
        &mut self,
        window: WindowId,
        layout: &str,
    ) -> Result<(), ServerError> {
        let window_state = self
            .windows
            .get(&window)
            .ok_or_else(|| ServerError::MissingTarget(window.to_string()))?;
        let all_panes = window_state.pane_order.clone();
        let floats = all_panes
            .iter()
            .copied()
            .filter(|pane| window_state.is_floating(*pane))
            .filter_map(|pane| Some((pane, window_state.layout.pane_geometry(pane)?)))
            .collect::<Vec<_>>();
        let old_extent = window_state.layout.extent();
        let invalid = |cause: &str| ServerError::InvalidCommand(format!("{cause}: {layout}"));
        let mut parsed = CellLayout::parse(layout).map_err(|error| invalid(error.cause()))?;
        let legacy = parsed.version() == 1;
        let panes = if legacy {
            all_panes
                .iter()
                .copied()
                .filter(|pane| !floats.iter().any(|(float, _)| float == pane))
                .collect::<Vec<_>>()
        } else {
            all_panes.clone()
        };
        if panes.is_empty() {
            return Err(invalid(&format!("window {window} has no panes")));
        }
        let cells = parsed.pane_count();
        if panes.len() > cells {
            return Err(invalid(&format!(
                "have {} panes but need {cells}",
                panes.len()
            )));
        }
        while parsed.pane_count() > panes.len() {
            parsed.trim_bottom_right();
        }
        parsed
            .check_sizes()
            .map_err(|error| invalid(error.cause()))?;
        let split_ids = (0..all_panes.len().saturating_sub(1))
            .map(|_| self.allocate_split_id())
            .collect::<Vec<_>>();
        let mut split_ids = split_ids.into_iter();
        let mut ids = || {
            split_ids
                .next()
                .expect("parsed layout has one split ID per edge")
        };
        let (mut next, selection) = parsed.into_layout(&panes, &mut ids);
        if legacy {
            next.link_floats(&floats, &mut ids);
        }
        if !next.has_tiled() {
            next.keep_extent(old_extent);
        }
        let split_ids_exhausted = split_ids.next().is_none();
        debug_assert!(
            split_ids_exhausted,
            "parsed layout consumes one fresh ID per split"
        );

        let active_point = selection
            .as_ref()
            .and_then(|selection| selection.active)
            .map(|_| self.allocate_sort_point());
        let window = self
            .windows
            .get_mut(&mut self.journal, &window)
            .expect("window was resolved");
        let previous = window.saved_layout(self.legacy_layout_saves);
        let old_z = window.z_order.clone();
        window.layout = next;
        if let Some(selection) = &selection {
            let mut z_order = selection.floats.clone();
            z_order.extend(
                old_z
                    .iter()
                    .copied()
                    .filter(|pane| !window.layout.is_floating(*pane)),
            );
            window.z_order = z_order;
        }
        window.previous_layout = Some(Box::new(previous));
        window.last_extent_probe = None;
        if let Some(selection) = selection {
            apply_window_selection(window, selection.active, selection.last_panes, active_point);
        }
        self.bump_generation();
        Ok(())
    }

    pub fn cycle_layout(
        &mut self,
        window: WindowId,
        offset: isize,
        options: &PresetOptions,
    ) -> Result<LayoutPreset, ServerError> {
        let last = self
            .windows
            .get(&window)
            .ok_or_else(|| ServerError::MissingTarget(window.to_string()))?
            .last_layout;
        let preset = match last {
            Some(last) => last.at_offset(offset),
            None if offset < 0 => LayoutPreset::Tiled,
            None => LayoutPreset::EvenHorizontal,
        };
        self.select_layout(window, preset, options)?;
        Ok(preset)
    }

    pub fn restore_previous_layout(&mut self, window: WindowId) -> Result<(), ServerError> {
        let has_previous = self
            .windows
            .get(&window)
            .ok_or_else(|| ServerError::MissingTarget(window.to_string()))?
            .previous_layout
            .is_some();
        if !has_previous {
            let window = self
                .windows
                .get_mut(&mut self.journal, &window)
                .expect("window was resolved");
            window.previous_layout = Some(Box::new(window.saved_layout(self.legacy_layout_saves)));
            return Ok(());
        }
        let (pane_order, split_count) = {
            let window_state = self
                .windows
                .get(&window)
                .ok_or_else(|| ServerError::MissingTarget(window.to_string()))?;
            let previous = window_state
                .previous_layout
                .as_deref()
                .expect("previous layout was checked");
            if previous.layout.pane_count() != window_state.pane_order.len() {
                return Err(ServerError::InvalidCommand(format!(
                    "window {window} previous layout no longer matches its panes"
                )));
            }
            (
                window_state.pane_order.clone(),
                window_state.pane_order.len().saturating_sub(1),
            )
        };
        let split_ids = (0..split_count)
            .map(|_| self.allocate_split_id())
            .collect::<Vec<_>>();
        let active_point = self.allocate_sort_point();

        let window = self
            .windows
            .get_mut(&mut self.journal, &window)
            .expect("window was resolved");
        let current = window.saved_layout(self.legacy_layout_saves);
        let SavedLayout {
            layout: mut restored,
            selection,
        } = *window
            .previous_layout
            .take()
            .expect("previous layout was validated");
        let moved = |pane: PaneId, saved: &SavedSelection| {
            saved
                .pane_order
                .iter()
                .position(|candidate| *candidate == pane)
                .and_then(|position| pane_order.get(position))
                .copied()
        };
        let order = selection.as_ref().map_or_else(
            || Some(pane_order.clone()),
            |saved| {
                restored
                    .panes_in_order()
                    .into_iter()
                    .map(|pane| moved(pane, saved))
                    .collect::<Option<Vec<_>>>()
            },
        );
        let replaced = restored.replace_panes_in_order(order.as_deref().unwrap_or(&pane_order));
        debug_assert!(replaced);
        let mut split_ids = split_ids.into_iter();
        let mut ids = || {
            split_ids
                .next()
                .expect("restored layout has one ID per edge")
        };
        restored.refresh_divider_ids(&mut ids);
        let split_ids_exhausted = split_ids.next().is_none();
        debug_assert!(
            split_ids_exhausted,
            "restored layout consumes one fresh ID per split"
        );
        window.layout = restored;
        window.previous_layout = Some(Box::new(current));
        window.zoomed_pane = None;
        if let Some(saved) = selection.filter(|_| order.is_some()) {
            let active = moved(saved.active, &saved);
            let last_panes = saved
                .last_panes
                .iter()
                .filter_map(|pane| moved(*pane, &saved))
                .collect();
            apply_window_selection(window, active, last_panes, Some(active_point));
            let mut z_order = window.modal_pane().into_iter().collect::<Vec<_>>();
            for pane in saved.z_order.iter().filter_map(|pane| moved(*pane, &saved)) {
                if window.layout.is_floating(pane) && !z_order.contains(&pane) {
                    z_order.push(pane);
                }
            }
            for pane in window.z_order.clone() {
                if !z_order.contains(&pane) {
                    z_order.push(pane);
                }
            }
            window.z_order = z_order;
        }
        self.bump_generation();
        Ok(())
    }

    pub fn spread_layout(&mut self, pane: PaneId) -> Result<(), ServerError> {
        let window_id = self
            .window_for_pane(pane)
            .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
        let window = self
            .windows
            .get_mut(&mut self.journal, &window_id)
            .expect("pane window exists");
        let previous = window.saved_layout(self.legacy_layout_saves);
        window
            .layout
            .spread(pane)
            .map_err(|error| pane_layout_error(error, pane))?;
        window.previous_layout = Some(Box::new(previous));
        self.bump_generation();
        Ok(())
    }

    pub fn last_layout(&self, window: WindowId) -> Result<Option<LayoutPreset>, ServerError> {
        self.windows
            .get(&window)
            .map(|window| window.last_layout)
            .ok_or_else(|| ServerError::MissingTarget(window.to_string()))
    }

    pub fn update_pane_title(
        &mut self,
        pane: PaneId,
        title: impl Into<String>,
    ) -> Result<bool, ServerError> {
        let title = title.into();
        let pane_state = self
            .pane_mut(pane)
            .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
        if pane_state.title == title {
            return Ok(false);
        }
        pane_state.title = title;
        self.bump_generation();
        Ok(true)
    }

    pub(crate) fn pin_pane_title(
        &mut self,
        pane: PaneId,
        title: impl Into<String>,
    ) -> Result<bool, ServerError> {
        let changed = self.update_pane_title(pane, title)?;
        if let Some(pane_state) = self.pane_mut(pane) {
            pane_state.title_pinned = true;
        }
        Ok(changed)
    }

    pub fn update_pane_title_from_terminal(
        &mut self,
        pane: PaneId,
        title: impl Into<String>,
        program_wrote_title: bool,
    ) -> Result<bool, ServerError> {
        let pane_state = self
            .pane_mut(pane)
            .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
        if !program_wrote_title && pane_state.title_pinned {
            return Ok(false);
        }
        pane_state.title_pinned = false;
        self.update_pane_title(pane, title)
    }

    pub fn update_agent_title(
        &mut self,
        pane: PaneId,
        title: Option<&str>,
    ) -> Result<bool, ServerError> {
        let kind = &self
            .pane(pane)
            .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?
            .kind;
        if !matches!(kind, PaneKind::Agent(_)) {
            return Ok(false);
        }
        let fallback = pane_title(kind, "");
        match title {
            Some(title) => self.update_pane_title_from_terminal(pane, title, true),
            None => self.update_pane_title_from_terminal(pane, fallback, false),
        }
    }

    pub fn set_pane_bell(&mut self, pane: PaneId, bell: bool) -> bool {
        let Some(pane_state) = self.pane_mut(pane) else {
            return false;
        };
        if pane_state.bell == bell {
            return false;
        }
        pane_state.bell = bell;
        self.bump_generation();
        true
    }

    pub fn set_pane_status(&mut self, pane: PaneId, status: Option<PaneStatus>) -> bool {
        let Some(pane_state) = self.pane_mut(pane) else {
            return false;
        };
        if pane_state.status == status {
            return false;
        }
        pane_state.status = status;
        self.bump_generation();
        true
    }

    pub fn update_pane_summary(
        &mut self,
        pane: PaneId,
        edit: impl FnOnce(&mut PaneSummary),
    ) -> bool {
        let Some(current) = self.pane(pane) else {
            return false;
        };
        let mut summary = current.summary.clone();
        edit(&mut summary);
        if current.summary == summary {
            return false;
        }
        let Some(pane_state) = self.pane_mut(pane) else {
            return false;
        };
        pane_state.summary = summary;
        self.bump_generation();
        true
    }

    pub fn clear_pane_alerts(&mut self, pane: PaneId) -> bool {
        let bell = self.set_pane_bell(pane, false);
        let unseen = self.update_pane_summary(pane, |summary| summary.unseen = false);
        bell || unseen
    }

    pub fn materialize_pane(
        &mut self,
        pane: PaneId,
        kind: PaneKind,
    ) -> Result<Option<PaneId>, ServerError> {
        if matches!(kind, PaneKind::Picker { .. }) {
            return Err(ServerError::InvalidCommand(
                "a pane picker cannot materialize another picker".to_owned(),
            ));
        }
        let inherit_cwd_from = match &self
            .pane(pane)
            .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?
            .kind
        {
            PaneKind::Picker { inherit_cwd_from } => *inherit_cwd_from,
            _ => {
                return Err(ServerError::InvalidCommand(format!(
                    "pane {pane} is not awaiting a type selection"
                )));
            }
        };
        let inherit_cwd_from = inherit_cwd_from
            .filter(|donor| self.cwd_donor(*donor) == Some(*donor))
            .or_else(|| self.cwd_donor(pane));
        let title = pane_title(&kind, &self.default_pane_title);
        let pane_state = self
            .pane_mut(pane)
            .expect("the validated picker pane still exists");
        pane_state.title = title;
        pane_state.title_pinned = false;
        pane_state.kind = kind;
        self.bump_generation();
        Ok(inherit_cwd_from)
    }

    pub fn convert_pane(&mut self, pane: PaneId, kind: PaneKind) -> Result<(), ServerError> {
        let default_title = self.default_pane_title.clone();
        let pane_state = self
            .pane_mut(pane)
            .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
        if !matches!(
            (&pane_state.kind, &kind),
            (PaneKind::Terminal, PaneKind::Agent(_)) | (PaneKind::Agent(_), PaneKind::Terminal)
        ) {
            let into = if matches!(kind, PaneKind::Agent(_)) {
                "an agent"
            } else {
                "a terminal"
            };
            return Err(ServerError::InvalidCommand(format!(
                "pane {pane} cannot turn into {into}"
            )));
        }
        if !pane_state.title_pinned {
            pane_state.title = pane_title(&kind, &default_title);
        }
        pane_state.kind = kind;
        pane_state.status = None;
        pane_state.summary.process.clear();
        pane_state.summary.flip_armed = false;
        self.revive_pane(pane)?;
        self.bump_generation();
        Ok(())
    }

    pub fn update_agent(
        &mut self,
        pane: PaneId,
        update: impl FnOnce(&mut AgentDescriptor),
    ) -> Result<bool, ServerError> {
        let pane_state = self
            .pane_mut(pane)
            .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
        let PaneKind::Agent(agent) = &mut pane_state.kind else {
            return Err(ServerError::InvalidCommand(format!(
                "pane {pane} is not an agent"
            )));
        };
        let before = agent.clone();
        update(agent);
        if *agent == before {
            return Ok(false);
        }
        self.bump_generation();
        Ok(true)
    }

    pub fn update_browser_url(&mut self, pane: PaneId, url: String) -> Result<(), ServerError> {
        let pane_state = self
            .pane_mut(pane)
            .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
        let PaneKind::Browser(browser) = &mut pane_state.kind else {
            return Err(ServerError::InvalidCommand(format!(
                "pane {pane} is not a browser"
            )));
        };
        let index = browser.active_tab.min(browser.tabs.len().saturating_sub(1));
        match browser.tabs.get_mut(index) {
            Some(active) if *active == url => return Ok(()),
            Some(active) => *active = url,
            None => browser.tabs.push(url),
        }
        self.bump_generation();
        Ok(())
    }

    pub fn update_browser_tabs(
        &mut self,
        pane: PaneId,
        tabs: Vec<String>,
        active_tab: usize,
    ) -> Result<(), ServerError> {
        if tabs.is_empty() {
            return Err(ServerError::InvalidCommand(
                "a browser pane needs at least one tab".to_owned(),
            ));
        }
        if active_tab >= tabs.len() {
            return Err(ServerError::InvalidCommand(format!(
                "active tab {active_tab} is out of range for {} tabs",
                tabs.len()
            )));
        }
        let pane_state = self
            .pane_mut(pane)
            .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
        let PaneKind::Browser(browser) = &mut pane_state.kind else {
            return Err(ServerError::InvalidCommand(format!(
                "pane {pane} is not a browser"
            )));
        };
        if browser.tabs == tabs && browser.active_tab == active_tab {
            return Ok(());
        }
        browser.tabs = tabs;
        browser.active_tab = active_tab;
        self.bump_generation();
        Ok(())
    }

    pub fn update_editor_cwd(&mut self, pane: PaneId, cwd: String) -> Result<(), ServerError> {
        let pane_state = self
            .pane_mut(pane)
            .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
        let PaneKind::Editor(editor) = &mut pane_state.kind else {
            return Err(ServerError::InvalidCommand(format!(
                "pane {pane} is not an editor"
            )));
        };
        let next = EditorDescriptor {
            path: editor.path.clone(),
            cwd,
        };
        next.validate()
            .map_err(|error| ServerError::InvalidCommand(error.to_string()))?;
        if *editor == next {
            return Ok(());
        }
        *editor = next;
        self.bump_generation();
        Ok(())
    }

    pub fn update_editor_path(
        &mut self,
        pane: PaneId,
        path: Option<String>,
    ) -> Result<(), ServerError> {
        let pane_state = self
            .pane_mut(pane)
            .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
        let PaneKind::Editor(editor) = &mut pane_state.kind else {
            return Err(ServerError::InvalidCommand(format!(
                "pane {pane} is not an editor"
            )));
        };
        let next = EditorDescriptor {
            path,
            cwd: editor.cwd.clone(),
        };
        next.validate()
            .map_err(|error| ServerError::InvalidCommand(error.to_string()))?;
        if *editor == next {
            return Ok(());
        }
        *editor = next;
        self.bump_generation();
        Ok(())
    }

    pub fn update_view(
        &mut self,
        pane: PaneId,
        descriptor: ViewDescriptor,
    ) -> Result<(), ServerError> {
        let pane_state = self
            .pane_mut(pane)
            .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
        let PaneKind::View(view) = &mut pane_state.kind else {
            return Err(ServerError::InvalidCommand(format!(
                "pane {pane} is not a view"
            )));
        };
        if *view == descriptor {
            return Ok(());
        }
        if !pane_state.title_pinned {
            pane_state.title.clone_from(&descriptor.title);
        }
        *view = descriptor;
        self.bump_generation();
        Ok(())
    }

    pub fn update_simulator_device(
        &mut self,
        pane: PaneId,
        device: &str,
    ) -> Result<(), ServerError> {
        let next = simulator_descriptor(device)?;
        let pane_state = self
            .pane_mut(pane)
            .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
        let PaneKind::Simulator(simulator) = &mut pane_state.kind else {
            return Err(ServerError::InvalidCommand(format!(
                "pane {pane} is not a simulator"
            )));
        };
        if *simulator == next {
            return Ok(());
        }
        *simulator = next;
        self.bump_generation();
        Ok(())
    }

    pub fn update_browser_profile(
        &mut self,
        pane: PaneId,
        profile: &str,
    ) -> Result<(), ServerError> {
        let profile = normalize_browser_profile_name(profile)
            .map_err(|error| ServerError::InvalidCommand(error.to_string()))?;
        let pane_state = self
            .pane_mut(pane)
            .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
        let PaneKind::Browser(browser) = &mut pane_state.kind else {
            return Err(ServerError::InvalidCommand(format!(
                "pane {pane} is not a browser"
            )));
        };
        if browser.profile == profile {
            return Ok(());
        }
        browser.profile = profile;
        browser.tabs = vec![browser.url().to_owned()];
        browser.active_tab = 0;
        self.bump_generation();
        Ok(())
    }

    #[must_use]
    pub fn global_synchronize_panes(&self) -> bool {
        self.input_options.synchronize_panes().unwrap_or(false)
    }

    #[must_use]
    pub fn global_automatic_rename(&self) -> bool {
        self.input_options.automatic_rename().unwrap_or(true)
    }

    pub fn set_global_automatic_rename(&mut self, value: Option<bool>) {
        if self.input_options.automatic_rename() != value {
            self.input_options.set_automatic_rename(value);
            self.bump_generation();
        }
    }

    #[must_use]
    pub fn global_aggressive_resize(&self) -> bool {
        self.input_options.aggressive_resize().unwrap_or(false)
    }

    pub fn set_global_aggressive_resize(&mut self, value: Option<bool>) {
        if self.input_options.aggressive_resize() != value {
            self.input_options.set_aggressive_resize(value);
            self.bump_generation();
        }
    }

    pub fn window_aggressive_resize(&self, window: WindowId) -> Result<bool, ServerError> {
        let window = self
            .windows
            .get(&window)
            .ok_or_else(|| ServerError::MissingTarget(window.to_string()))?;
        Ok(window
            .input_options
            .aggressive_resize()
            .unwrap_or_else(|| self.global_aggressive_resize()))
    }

    pub fn window_aggressive_resize_override(
        &self,
        window: WindowId,
    ) -> Result<Option<bool>, ServerError> {
        self.windows
            .get(&window)
            .map(|window| window.input_options.aggressive_resize())
            .ok_or_else(|| ServerError::MissingTarget(window.to_string()))
    }

    pub fn set_window_aggressive_resize(
        &mut self,
        window: WindowId,
        value: Option<bool>,
    ) -> Result<(), ServerError> {
        let window = self
            .windows
            .get_mut(&mut self.journal, &window)
            .ok_or_else(|| ServerError::MissingTarget(window.to_string()))?;
        if window.input_options.aggressive_resize() != value {
            window.input_options.set_aggressive_resize(value);
            self.bump_generation();
        }
        Ok(())
    }

    pub fn window_automatic_rename(&self, window: WindowId) -> Result<bool, ServerError> {
        let window = self
            .windows
            .get(&window)
            .ok_or_else(|| ServerError::MissingTarget(window.to_string()))?;
        Ok(window
            .input_options
            .automatic_rename()
            .unwrap_or_else(|| self.global_automatic_rename()))
    }

    pub fn window_automatic_rename_override(
        &self,
        window: WindowId,
    ) -> Result<Option<bool>, ServerError> {
        self.windows
            .get(&window)
            .map(|window| window.input_options.automatic_rename())
            .ok_or_else(|| ServerError::MissingTarget(window.to_string()))
    }

    pub fn set_window_automatic_rename(
        &mut self,
        window: WindowId,
        value: Option<bool>,
    ) -> Result<(), ServerError> {
        let window = self
            .windows
            .get_mut(&mut self.journal, &window)
            .ok_or_else(|| ServerError::MissingTarget(window.to_string()))?;
        if window.input_options.automatic_rename() != value {
            window.input_options.set_automatic_rename(value);
            self.bump_generation();
        }
        Ok(())
    }

    pub fn set_global_synchronize_panes(&mut self, value: bool) {
        if self.global_synchronize_panes() != value {
            self.input_options.set_synchronize_panes(Some(value));
            self.bump_generation();
        }
    }

    pub fn window_synchronize_panes(&self, window: WindowId) -> Result<bool, ServerError> {
        let window = self
            .windows
            .get(&window)
            .ok_or_else(|| ServerError::MissingTarget(window.to_string()))?;
        Ok(window
            .input_options
            .synchronize_panes()
            .unwrap_or_else(|| self.global_synchronize_panes()))
    }

    pub fn window_synchronize_override(
        &self,
        window: WindowId,
    ) -> Result<Option<bool>, ServerError> {
        self.windows
            .get(&window)
            .map(|window| window.input_options.synchronize_panes())
            .ok_or_else(|| ServerError::MissingTarget(window.to_string()))
    }

    pub fn set_window_synchronize_panes(
        &mut self,
        window: WindowId,
        value: Option<bool>,
    ) -> Result<(), ServerError> {
        let window = self
            .windows
            .get_mut(&mut self.journal, &window)
            .ok_or_else(|| ServerError::MissingTarget(window.to_string()))?;
        if window.input_options.synchronize_panes() != value {
            window.input_options.set_synchronize_panes(value);
            self.bump_generation();
        }
        Ok(())
    }

    pub fn pane_synchronize_panes(&self, pane: PaneId) -> Result<bool, ServerError> {
        let window_id = self
            .window_for_pane(pane)
            .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
        self.pane_synchronize_panes_in(window_id, pane)
    }

    pub(crate) fn pane_synchronize_panes_in(
        &self,
        window_id: WindowId,
        pane: PaneId,
    ) -> Result<bool, ServerError> {
        let window = &self.windows[&window_id];
        let pane = &window.panes[&pane];
        Ok(pane
            .input_options
            .synchronize_panes()
            .unwrap_or(self.window_synchronize_panes(window_id)?))
    }

    pub fn pane_synchronize_override(&self, pane: PaneId) -> Result<Option<bool>, ServerError> {
        self.pane(pane)
            .map(|pane| pane.input_options.synchronize_panes())
            .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))
    }

    pub fn set_pane_synchronize_panes(
        &mut self,
        pane: PaneId,
        value: Option<bool>,
    ) -> Result<(), ServerError> {
        let pane_state = self
            .pane_mut(pane)
            .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
        if pane_state.input_options.synchronize_panes() != value {
            pane_state.input_options.set_synchronize_panes(value);
            self.bump_generation();
        }
        Ok(())
    }

    pub fn clear_pane_synchronize_overrides(
        &mut self,
        window: WindowId,
    ) -> Result<(), ServerError> {
        let window = self
            .windows
            .get_mut(&mut self.journal, &window)
            .ok_or_else(|| ServerError::MissingTarget(window.to_string()))?;
        let mut changed = false;
        for pane in window.panes.values_mut() {
            if pane.input_options.synchronize_panes().is_some() {
                pane.input_options.set_synchronize_panes(None);
                changed = true;
            }
        }
        if changed {
            self.bump_generation();
        }
        Ok(())
    }

    pub fn synchronized_input_targets(&self, source: PaneId) -> Result<Vec<PaneId>, ServerError> {
        let window_id = self
            .window_for_pane(source)
            .ok_or_else(|| ServerError::MissingTarget(source.to_string()))?;
        let window = &self.windows[&window_id];
        let synchronized = window
            .input_options
            .synchronize_panes()
            .unwrap_or_else(|| self.global_synchronize_panes());
        let source_synchronized = window.panes[&source]
            .input_options
            .synchronize_panes()
            .unwrap_or(synchronized);
        if !source_synchronized {
            return Ok(vec![source]);
        }
        Ok(window
            .panes
            .iter()
            .filter_map(|(id, pane)| {
                pane.input_options
                    .synchronize_panes()
                    .unwrap_or(synchronized)
                    .then_some(*id)
            })
            .collect())
    }

    pub(crate) fn mark_session_active_at(&mut self, session: SessionId, now: u64) {
        if self.sessions.contains_key(&session) {
            let activity = self.allocate_sort_point();
            let state = self
                .sessions
                .get_mut(&mut self.journal, &session)
                .expect("active session exists");
            state.activity = i64::try_from(now).ok();
            state.sort_activity = activity;
            self.last_active_session = Some(session);
        }
    }

    #[must_use]
    pub fn snapshot(&self) -> MuxSnapshot {
        let sessions = self
            .sessions
            .values()
            .map(|session| SessionSnapshot {
                id: session.id,
                name: session.name.clone(),
                active_window: session.active_window,
                windows: session
                    .windows
                    .iter()
                    .filter_map(|id| self.windows.get(id))
                    .map(|window| self.window_snapshot(window))
                    .collect(),
                viewers: Vec::new(),
            })
            .collect();
        MuxSnapshot {
            generation: self.generation,
            sessions,
            focused_window: None,
        }
    }

    fn window_snapshot(&self, window: &Window) -> WindowSnapshot {
        let synchronized = window
            .input_options
            .synchronize_panes()
            .unwrap_or_else(|| self.global_synchronize_panes());
        let pane_base_index = self.pane_base_index(window.id);
        let layout_dump = window.layout_string(LayoutFormat::V2, pane_base_index);
        let visible_layout_dump = window.visible_layout_string(LayoutFormat::V2, pane_base_index);
        let (sx, sy) = window.layout.extent();
        WindowSnapshot {
            id: window.id,
            index: window.index,
            name: window.name.clone(),
            automatic_rename: window
                .input_options
                .automatic_rename()
                .unwrap_or_else(|| self.global_automatic_rename()),
            active_pane: window.active_pane,
            zoomed_pane: window.zoomed_pane,
            layout: window.layout.project(),
            panes: window
                .panes
                .iter()
                .map(|(id, pane)| {
                    (
                        *id,
                        PaneSnapshot {
                            id: pane.id,
                            title: pane.title.clone(),
                            kind: pane.kind.snapshot(),
                            synchronized_input: pane
                                .input_options
                                .synchronize_panes()
                                .unwrap_or(synchronized),
                            bell: pane.bell,
                            dead: pane.dead,
                            dead_status: pane.dead_status,
                            border_colour: None,
                            active_border_colour: None,
                            border_status_text: String::new(),
                            mode: None,
                            status: pane.status.clone(),
                            summary: pane.summary.clone(),
                        },
                    )
                })
                .collect(),
            layout_dump,
            visible_layout_dump,
            status_label: String::new(),
            activity: window.activity_flag,
            silence: window.silence_flag,
            pane_border_status: PaneBorderStatus::Off,
            pane_border_lines: PaneBorderLines::Single,
            pane_border_indicators: PaneBorderIndicators::Colour,
            pane_order: window.pane_order.clone(),
            pane_z_order: window.z_order.clone(),
            floating: window
                .z_order
                .iter()
                .filter(|pane| window.layout.is_floating(**pane))
                .filter_map(|pane| {
                    let geometry = window.layout.pane_geometry(*pane)?;
                    Some(FloatingPaneSnapshot {
                        pane: *pane,
                        xoff: geometry.xoff,
                        yoff: geometry.yoff,
                        sx: geometry.sx,
                        sy: geometry.sy,
                        visible: window.shows_floating(*pane),
                        border_lines: PaneBorderLines::Single,
                        border_status: PaneBorderStatus::Off,
                    })
                })
                .collect(),
            modal: window.floats.modal.map(|modal| ModalPaneSnapshot {
                pane: modal.pane,
                capture_keys: modal.capture_keys,
                close_on_click: modal.close_on_click,
                close_on_cancel: modal.close_on_cancel,
            }),
            sx,
            sy,
            pane_base_index,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        let mut all_panes = BTreeSet::new();
        let mut all_splits = BTreeSet::new();
        for (window_id, window) in &self.windows {
            let session = self
                .sessions
                .get(&window.session)
                .ok_or_else(|| format!("window {window_id} has missing session"))?;
            if !session.windows.contains(window_id) {
                return Err(format!("session does not contain window {window_id}"));
            }
            if let Err(error) = window.layout.validate() {
                return Err(format!("window {window_id} has an invalid layout: {error}"));
            }
            let layout_panes = window.layout.panes_in_order();
            let layout_set = layout_panes.iter().copied().collect::<BTreeSet<_>>();
            let pane_set = window.panes.keys().copied().collect::<BTreeSet<_>>();
            if layout_set != pane_set || layout_panes.len() != layout_set.len() {
                return Err(format!("window {window_id} layout does not match panes"));
            }
            let z_order = window.z_order.iter().copied().collect::<BTreeSet<_>>();
            if z_order != pane_set || window.z_order.len() != z_order.len() {
                return Err(format!("window {window_id} z order does not match panes"));
            }
            if window
                .floats
                .modal
                .is_some_and(|modal| !window.is_floating(modal.pane))
            {
                return Err(format!("window {window_id} modal pane is not floating"));
            }
            let pane_order = window.pane_order.iter().copied().collect::<BTreeSet<_>>();
            if pane_order != pane_set || window.pane_order.len() != pane_order.len() {
                return Err(format!(
                    "window {window_id} pane order does not match panes"
                ));
            }
            let mut layout_splits = Vec::new();
            window.layout.project().splits(&mut layout_splits);
            for split in layout_splits {
                if !all_splits.insert(split) {
                    return Err(format!("split {split} occurs more than once"));
                }
            }
            if !pane_set.contains(&window.active_pane) {
                return Err(format!("window {window_id} active pane is missing"));
            }
            let history = window.last_panes.iter().copied().collect::<BTreeSet<_>>();
            if history.len() != window.last_panes.len() || !history.is_subset(&pane_set) {
                return Err(format!("window {window_id} pane history is invalid"));
            }
            if window.zoomed_pane.is_some_and(|pane| {
                !pane_set.contains(&pane)
                    || (pane != window.active_pane && !window.active_floats_over_zoom())
            }) {
                return Err(format!("window {window_id} zoomed pane is invalid"));
            }
            for pane in pane_set {
                if !all_panes.insert(pane) {
                    return Err(format!("pane {pane} occurs in multiple windows"));
                }
            }
        }
        for session in self.sessions.values() {
            if session.windows.is_empty() || !session.windows.contains(&session.active_window) {
                return Err(format!("session {} has invalid active window", session.id));
            }
        }
        Ok(())
    }

    fn allocate_session_id(&mut self) -> SessionId {
        let id = SessionId(self.next_session_id);
        self.next_session_id = self.next_session_id.saturating_add(1);
        id
    }

    fn allocate_window_id(&mut self) -> WindowId {
        let id = WindowId(self.next_window_id);
        self.next_window_id = self.next_window_id.saturating_add(1);
        id
    }

    fn allocate_pane_id(&mut self) -> PaneId {
        let id = PaneId(self.next_pane_id);
        self.next_pane_id = self.next_pane_id.saturating_add(1);
        id
    }

    fn allocate_split_id(&mut self) -> SplitId {
        let id = SplitId(self.next_split_id);
        self.next_split_id = self.next_split_id.saturating_add(1);
        id
    }

    fn allocate_sort_point(&mut self) -> u64 {
        let point = self.next_sort_point;
        self.next_sort_point = self.next_sort_point.saturating_add(1);
        point
    }

    pub fn bump_generation(&mut self) {
        self.generation = self.generation.saturating_add(1);
    }
}

pub(crate) fn simulator_descriptor(device: &str) -> Result<SimulatorDescriptor, ServerError> {
    if device.len() > MAX_SIMULATOR_DEVICE_BYTES {
        return Err(ServerError::InvalidCommand(format!(
            "simulator device cannot exceed {MAX_SIMULATOR_DEVICE_BYTES} bytes"
        )));
    }
    Ok(SimulatorDescriptor {
        device: device.to_owned(),
    })
}

fn pane_title(kind: &PaneKind, terminal: &str) -> String {
    match kind {
        PaneKind::Picker { .. } => "new pane".to_owned(),
        PaneKind::Terminal if terminal.is_empty() => "terminal".to_owned(),
        PaneKind::Terminal => terminal.to_owned(),
        PaneKind::Browser(browser) if browser.url() == "about:blank" => "browser".to_owned(),
        PaneKind::Browser(browser) => browser.url().to_owned(),
        PaneKind::Editor(_) => "editor".to_owned(),
        PaneKind::Agent(_) => "agent".to_owned(),
        PaneKind::View(view) => view.title.clone(),
        PaneKind::Simulator(_) => "Simulator".to_owned(),
    }
}

fn pane_layout_error(error: LayoutError, pane: PaneId) -> ServerError {
    match error {
        LayoutError::UnknownPane => ServerError::MissingTarget(pane.to_string()),
        LayoutError::NoSpace => ServerError::InvalidCommand("no space for a new pane".to_owned()),
        LayoutError::Floating => ServerError::InvalidCommand("pane is floating".to_owned()),
        LayoutError::LastPane | LayoutError::UnknownDivider => {
            ServerError::Internal(format!("unexpected pane layout error: {error:?}"))
        }
    }
}

fn divider_layout_error(error: LayoutError, split: SplitId) -> ServerError {
    match error {
        LayoutError::UnknownDivider => ServerError::MissingTarget(split.to_string()),
        LayoutError::LastPane
        | LayoutError::NoSpace
        | LayoutError::UnknownPane
        | LayoutError::Floating => {
            ServerError::Internal(format!("unexpected divider layout error: {error:?}"))
        }
    }
}

#[cfg(test)]
fn collect_pane_rects(node: &LayoutNode, bounds: PaneRect, output: &mut Vec<(PaneId, PaneRect)>) {
    match node {
        LayoutNode::Pane(pane) => output.push((*pane, bounds)),
        LayoutNode::Empty => {}
        LayoutNode::Split {
            axis,
            ratio,
            first,
            second,
            ..
        } => match axis {
            Axis::Horizontal => {
                let split = split_coordinate(bounds.left, bounds.right, *ratio);
                collect_pane_rects(
                    first,
                    PaneRect {
                        right: split,
                        ..bounds
                    },
                    output,
                );
                collect_pane_rects(
                    second,
                    PaneRect {
                        left: split,
                        ..bounds
                    },
                    output,
                );
            }
            Axis::Vertical => {
                let split = split_coordinate(bounds.top, bounds.bottom, *ratio);
                collect_pane_rects(
                    first,
                    PaneRect {
                        bottom: split,
                        ..bounds
                    },
                    output,
                );
                collect_pane_rects(
                    second,
                    PaneRect {
                        top: split,
                        ..bounds
                    },
                    output,
                );
            }
        },
    }
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "validated split ratios map a bounded one-million-unit logical extent to u32"
)]
#[cfg(test)]
fn split_coordinate(start: u32, end: u32, ratio: f32) -> u32 {
    let extent = end.saturating_sub(start);
    if extent <= 1 {
        return start.saturating_add(extent);
    }
    let offset = (f64::from(extent) * f64::from(ratio)).round() as u32;
    start + offset.clamp(1, extent - 1)
}

#[cfg(test)]
mod test_support;

#[cfg(test)]
mod sessions_tests;

#[cfg(test)]
mod windows_tests;

#[cfg(test)]
mod panes_tests;

#[cfg(test)]
mod targets_tests;

#[cfg(test)]
mod layout_tests;

#[cfg(test)]
mod options_tests;

#[cfg(test)]
mod pane_metadata_tests;
