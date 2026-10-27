use std::{ops::Range, rc::Rc};

use zz_client::{
    place::{ActionKind, Fleet, LevelSet, Place},
    recents::{RecentKey, Recents, now_secs},
};
use zz_gpui::{AnyElement, App, Global, WeakEntity, Window, actions};
use zz_protocol::PaneId;

use super::{CommandPaletteEvent, CommandPaletteView, PaletteHint};

actions!(zz_palette, [CopyAddress]);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActionContext {
    pub kind: ActionKind,
    pub fleet: Rc<Fleet>,
    pub place: Place,
    pub written: LevelSet,
    pub host_auto: bool,
    pub rest: String,
    pub rest_range: Range<usize>,
    pub insert_pane: Option<PaneId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionKey {
    Enter,
    Tab,
    ShiftTab,
    Left,
    Right,
    Up,
    Down,
    First,
    Last,
    Backspace,
    Copy,
}

pub enum LevelEffect {
    Stay,
    Close,
    SetRest(String),
    Copied(String),
    Emit(CommandPaletteEvent),
}

pub trait PaletteActionLevel {
    fn attach(&mut self, _palette: WeakEntity<CommandPaletteView>, _cx: &mut App) {}
    fn update(&mut self, context: &ActionContext, window: &mut Window, cx: &mut App)
    -> LevelEffect;
    fn key(&mut self, key: ActionKey, window: &mut Window, cx: &mut App) -> Option<LevelEffect>;
    fn rows(&mut self, window: &mut Window, cx: &mut App) -> AnyElement;
    fn hints(&self, cx: &App) -> Vec<PaletteHint>;
    fn copy_text(&self, _cx: &App) -> Option<String> {
        None
    }
    fn closing(&mut self, _window: &mut Window, _cx: &mut App) {}
}

#[derive(Default)]
struct PaletteRecents {
    recents: Recents,
    save: Option<Rc<dyn Fn(&Recents, &mut App)>>,
}

impl Global for PaletteRecents {}

pub fn install_recents(
    recents: Recents,
    save: impl Fn(&Recents, &mut App) + 'static,
    cx: &mut App,
) {
    cx.set_global(PaletteRecents {
        recents,
        save: Some(Rc::new(save)),
    });
}

#[must_use]
pub fn palette_recents(cx: &App) -> Recents {
    cx.try_global::<PaletteRecents>()
        .map(|store| store.recents.clone())
        .unwrap_or_default()
}

pub fn record_recent(key: RecentKey, label: impl Into<String>, cx: &mut App) {
    if !cx.has_global::<PaletteRecents>() {
        cx.set_global(PaletteRecents::default());
    }
    let label = label.into();
    let store = cx.global_mut::<PaletteRecents>();
    store.recents.record(key, label, now_secs());
    let save = store.save.clone().map(|save| (save, store.recents.clone()));
    if let Some((save, recents)) = save {
        save(&recents, cx);
    }
}
