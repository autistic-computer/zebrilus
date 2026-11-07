use std::rc::Rc;

use penna::blocks::palette::{PalettePill, PaletteRow};
use zz_client::{
    completion::PaneKindAvailability,
    place::{Level, Place, parse},
    recents::Recents,
    verbs::{self, Filled, Slot, VerbKind},
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
    pub usage: Vec<Option<String>>,
}

fn preview_usage(state: &UnifiedPalette, query: &str, selected: Option<usize>) -> Option<String> {
    let action = selected.and_then(|index| state.rows.get(index)?.action.as_ref());
    if state.draft.is_none()
        && let Some(command) = parse(query).command
    {
        return match action {
            Some(PaletteAction::Command(spec)) => Some(spec.usage.to_owned()),
            _ => command
                .split_whitespace()
                .next()
                .and_then(zz_protocol::catalog_command_spec)
                .map(|spec| spec.usage.to_owned()),
        };
    }
    if let Some(draft) = &state.draft {
        match draft.verb.kind {
            VerbKind::Gui => return Some("gui · no tmux command".to_owned()),
            VerbKind::Zz => return Some(format!("zz · {}", draft.verb.description)),
            VerbKind::Tmux | VerbKind::Alias => {}
        }
        let mut values = draft.values();
        match (draft.next(), action) {
            (Some(Slot::Target(_)), Some(PaletteAction::Fill(place))) => {
                values.push(Filled::Place(*place));
            }
            (Some(Slot::Text(_)), _) if !query.trim().is_empty() => {
                values.push(Filled::Text(query.trim().to_owned()));
            }
            _ => {}
        }
        return Some(format!(
            "runs  {}",
            verbs::compile(&draft.verb, &values, &state.fleet)
                .map_or_else(|| draft.verb.spelled(), |compiled| compiled.display)
        ));
    }
    match action? {
        PaletteAction::Verb(index) => {
            let verb = state.verbs.get(*index)?;
            Some(match verb.kind {
                VerbKind::Gui => "gui · no tmux command".to_owned(),
                VerbKind::Zz => format!("zz · {}", verb.description),
                VerbKind::Tmux | VerbKind::Alias => {
                    let scope = state.scope();
                    let values = if matches!(verb.slots.first(), Some(Slot::Target(_)))
                        && scope.level().is_some_and(|level| level > Level::Host)
                    {
                        vec![Filled::Place(scope)]
                    } else {
                        Vec::new()
                    };
                    format!(
                        "runs  {}",
                        verbs::compile(verb, &values, &state.fleet)
                            .map_or_else(|| verb.spelled(), |compiled| compiled.display)
                    )
                }
            })
        }
        PaletteAction::Place(place) => {
            verbs::go_to_line(&state.fleet, place).map(|line| format!("runs  {line}"))
        }
        _ => None,
    }
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
    let usage = std::iter::once(None)
        .chain((0..state.rows.len()).map(Some))
        .map(|selected| preview_usage(&state, &input.query, selected))
        .collect();
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
        usage,
    }
}
