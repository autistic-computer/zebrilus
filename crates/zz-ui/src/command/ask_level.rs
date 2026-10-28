use zz_client::place::{self, LevelSet};
use zz_gpui::{AnyElement, App, IntoElement as _, Window};

use super::{
    ActionContext, ActionKey, CommandPaletteEvent, LevelEffect, PaletteActionLevel, PaletteHint,
    PaletteRow, command_palette_entry,
    palette_model::{DriverOffer, PaletteAction, PaletteEntry},
};
use crate::IconName;

pub(crate) fn ask_row(driver: Option<DriverOffer>, text: &str) -> Option<PaletteRow> {
    let (label, detail) = match driver? {
        DriverOffer::Ready => ("Ask the driver", text),
        DriverOffer::Asleep => ("Wake the driver", ""),
        DriverOffer::Off => ("Driver Agent is off", "Settings › Agents"),
    };
    Some(PaletteRow {
        label: label.into(),
        detail: detail.trim().to_owned().into(),
        icon: Some(IconName::Chat),
        ..Default::default()
    })
}

pub(crate) fn ask_entries(driver: Option<DriverOffer>, text: &str) -> Vec<PaletteEntry> {
    ask_row(driver, text).map_or_else(Vec::new, |row| {
        vec![
            PaletteEntry::header("Driver", "?"),
            PaletteEntry::row(row, PaletteAction::Ask),
        ]
    })
}

pub struct AskLevel {
    driver: DriverOffer,
    text: String,
    asked: bool,
}

impl AskLevel {
    #[must_use]
    pub const fn new(driver: DriverOffer) -> Self {
        Self {
            driver,
            text: String::new(),
            asked: false,
        }
    }

    fn ask(&self) -> LevelEffect {
        LevelEffect::Emit(CommandPaletteEvent::Driver {
            text: self.text.clone(),
        })
    }
}

impl PaletteActionLevel for AskLevel {
    fn update(&mut self, context: &ActionContext, _: &mut Window, _: &mut App) -> LevelEffect {
        let mention = (context.written != LevelSet::default())
            .then(|| place::written(&context.fleet, &context.place))
            .flatten();
        self.text = match mention {
            Some(mention) => format!("{mention} {}", context.rest.trim_start()),
            None => context.rest.trim_start().to_owned(),
        };
        if self.driver == DriverOffer::Ready && !self.asked {
            self.asked = true;
            return self.ask();
        }
        LevelEffect::Stay
    }

    fn key(&mut self, key: ActionKey, _: &mut Window, _: &mut App) -> Option<LevelEffect> {
        match key {
            ActionKey::Enter => Some(self.ask()),
            ActionKey::Up | ActionKey::Down | ActionKey::Tab => Some(LevelEffect::Stay),
            _ => None,
        }
    }

    fn rows(&mut self, _: &mut Window, cx: &mut App) -> AnyElement {
        ask_row(Some(self.driver), &self.text).map_or_else(
            || zz_gpui::Empty.into_any_element(),
            |row| command_palette_entry("ask-level-row", &row, true, cx).into_any_element(),
        )
    }

    fn hints(&self, _: &App) -> Vec<PaletteHint> {
        vec![
            PaletteHint {
                key: "enter",
                label: match self.driver {
                    DriverOffer::Ready => "ask",
                    DriverOffer::Asleep => "wake",
                    DriverOffer::Off => "settings",
                },
            },
            PaletteHint {
                key: "escape",
                label: "close",
            },
        ]
    }
}
