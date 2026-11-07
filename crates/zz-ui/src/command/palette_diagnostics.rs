use std::rc::Rc;

use penna::blocks::palette::{PalettePill, PaletteRow};
use zz_client::{
    completion::PaneKindAvailability,
    place::Place,
    recents::Recents,
    verbs::{self, Filled},
};

use super::{
    PaletteTree,
    palette_model::{PaletteAction, UnifiedPalette},
    palette_places::{Chip, Draft, chip_for, palette_fleet},
};

pub struct PalettePlacesFixture {
    pub tree: PaletteTree,
    pub chips: Vec<(Place, bool)>,
    pub draft: Option<(usize, Vec<(Filled, String)>)>,
    pub query: String,
    pub history: Vec<String>,
    pub aliases: String,
    pub availability: PaneKindAvailability,
    pub insert_path: bool,
}

pub enum PaletteFixtureAction {
    None,
    Place(Place),
    Verb(usize),
    Command(String),
    History(String),
    Fill(Place),
    Submit,
    Other,
}

pub struct PalettePlacesResult {
    pub rows: Vec<(PaletteRow, PaletteFixtureAction)>,
    pub pills: Vec<PalettePill>,
    pub placeholder: String,
    pub initial: Option<usize>,
}

#[must_use]
pub fn palette_places_fixture(input: PalettePlacesFixture) -> PalettePlacesResult {
    let mut state = UnifiedPalette::new(None);
    let fleet = palette_fleet(&input.tree);
    state.chips = input
        .chips
        .into_iter()
        .filter_map(|(place, auto)| chip_for(&fleet, place).map(|chip| Chip { auto, ..chip }))
        .collect();
    state.verbs = Rc::from(verbs::verbs(
        input.availability,
        &[],
        input.insert_path,
        &input.aliases,
    ));
    state.draft = input.draft.and_then(|(index, filled)| {
        state
            .verbs
            .get(index)
            .cloned()
            .map(|verb| Draft { verb, filled })
    });
    state.rebuild_places(
        &input.query,
        &input.history,
        &Recents::default(),
        0,
        input.tree,
        input.availability,
        &|_| None,
    );
    let initial = state.initial_selection();
    let pills = state.place_pills();
    let placeholder = state.place_placeholder();
    let rows = state
        .rows
        .into_iter()
        .map(|entry| {
            let action = match entry.action {
                None => PaletteFixtureAction::None,
                Some(PaletteAction::Place(place)) => PaletteFixtureAction::Place(place),
                Some(PaletteAction::Verb(index)) => PaletteFixtureAction::Verb(index),
                Some(PaletteAction::Command(command)) => {
                    PaletteFixtureAction::Command(command.name.to_owned())
                }
                Some(PaletteAction::History(command)) => PaletteFixtureAction::History(command),
                Some(PaletteAction::Fill(place)) => PaletteFixtureAction::Fill(place),
                Some(PaletteAction::Submit) => PaletteFixtureAction::Submit,
                Some(_) => PaletteFixtureAction::Other,
            };
            (entry.row, action)
        })
        .collect();
    PalettePlacesResult {
        rows,
        pills,
        placeholder,
        initial,
    }
}
