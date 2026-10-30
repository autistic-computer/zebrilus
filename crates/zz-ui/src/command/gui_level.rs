use std::{cell::Cell, rc::Rc};

use ala::{
    AnyElement, App, InteractiveElement as _, IntoElement, ParentElement as _, ScrollHandle,
    StatefulInteractiveElement as _, Styled as _, WeakEntity, Window, div, px,
};
use zz_client::{
    fuzzy::fuzzy_match,
    gui_actions::{GuiAction, GuiOffer, GuiOption, THEME},
    recents::RecentKey,
};

use super::{
    ActionContext, ActionKey, CommandPaletteEvent, CommandPaletteView, GuiEvent, LevelEffect,
    PaletteActionLevel, PaletteHint, PaletteRow, command_palette_empty, command_palette_entry,
    command_palette_list_height, command_palette_row_height, command_palette_section,
    record_recent,
};
use crate::IconName;
use penna::settings::appearance::{ThemePicks, theme_labels};

#[must_use]
pub fn theme_offer(picks: &ThemePicks) -> Option<GuiOffer> {
    let options = theme_labels(picks)
        .into_iter()
        .zip(&picks.themes)
        .map(|(label, (id, _))| GuiOption {
            value: id.to_string(),
            title: label.to_string(),
        })
        .collect();
    let current = picks.themes.get(picks.selected)?.0.to_string();
    GuiOffer::listed(THEME, &current, options)
}

#[derive(Clone)]
enum Pick {
    Action(&'static GuiAction),
    Value(&'static GuiAction, String),
}

struct Row {
    row: PaletteRow,
    pick: Pick,
    score: usize,
}

fn matching(mut row: Row, query: &str, metadata: &str) -> Option<Row> {
    if let Some((score, hits)) = fuzzy_match(query, &row.row.label) {
        row.score = score;
        row.row.matches = hits;
    } else if !query.is_empty() && !metadata.is_empty() {
        row.score = fuzzy_match(query, metadata)?.0.saturating_add(1000);
    } else {
        return None;
    }
    Some(row)
}

fn value_row(
    action: &'static GuiAction,
    option: &GuiOption,
    offer: &GuiOffer,
    label: String,
) -> Row {
    Row {
        row: PaletteRow {
            label: label.into(),
            right: if option.value == offer.current {
                "current"
            } else {
                ""
            }
            .into(),
            icon: Some(IconName::Settings),
            ..Default::default()
        },
        pick: Pick::Value(action, option.value.clone()),
        score: 0,
    }
}

pub struct GuiLevel {
    offers: Vec<GuiOffer>,
    action: Option<&'static GuiAction>,
    rest: String,
    rows: Vec<Row>,
    selected: Rc<Cell<usize>>,
    previewing: Option<String>,
    focused: bool,
    palette: Option<WeakEntity<CommandPaletteView>>,
    scroll: ScrollHandle,
}

impl GuiLevel {
    #[must_use]
    pub fn new(offers: Vec<GuiOffer>) -> Self {
        Self {
            offers,
            action: None,
            rest: String::new(),
            rows: Vec::new(),
            selected: Rc::default(),
            previewing: None,
            focused: false,
            palette: None,
            scroll: ScrollHandle::new(),
        }
    }

    fn offer(&self, action: &GuiAction) -> Option<&GuiOffer> {
        self.offers.iter().find(|offer| offer.action == action)
    }

    fn rebuild(&mut self) {
        let query = self.rest.trim();
        let mut rows = Vec::new();
        if let Some(offer) = self.action.and_then(|action| self.offer(action)) {
            rows.extend(offer.options.iter().filter_map(|option| {
                matching(
                    value_row(offer.action, option, offer, option.title.clone()),
                    query,
                    &option.value,
                )
            }));
        } else {
            for offer in &self.offers {
                let action = offer.action;
                rows.extend(matching(
                    Row {
                        row: PaletteRow {
                            label: action.title.into(),
                            right: offer.current_title().to_owned().into(),
                            icon: Some(IconName::Settings),
                            ..Default::default()
                        },
                        pick: Pick::Action(action),
                        score: 0,
                    },
                    query,
                    &action.aliases.join(" "),
                ));
                if query.is_empty() {
                    continue;
                }
                rows.extend(offer.options.iter().filter_map(|option| {
                    let label = format!("{} › {}", action.title, option.title);
                    matching(value_row(action, option, offer, label), query, "")
                }));
            }
        }
        if !query.is_empty() {
            rows.sort_by_key(|row| row.score);
        }
        let initial = if query.is_empty() && self.action.is_some() {
            rows.iter().position(|row| row.row.right == "current")
        } else {
            None
        };
        self.rows = rows;
        self.selected.set(initial.unwrap_or_default());
    }

    fn selected(&self) -> Option<&Row> {
        self.rows.get(self.selected.get())
    }

    fn preview(&mut self) -> LevelEffect {
        let Some(action) = self.action.filter(|action| action.preview) else {
            return LevelEffect::Stay;
        };
        let Some(Pick::Value(_, value)) = self.selected().map(|row| row.pick.clone()) else {
            return LevelEffect::Stay;
        };
        if self.previewing.as_deref() == Some(value.as_str()) {
            return LevelEffect::Stay;
        }
        if self
            .offer(action)
            .is_some_and(|offer| offer.current == value)
        {
            return self.revert();
        }
        self.previewing = Some(value.clone());
        LevelEffect::Emit(CommandPaletteEvent::Gui(GuiEvent::Preview {
            action,
            value,
        }))
    }

    fn revert(&mut self) -> LevelEffect {
        if self.previewing.take().is_some() {
            LevelEffect::Emit(CommandPaletteEvent::Gui(GuiEvent::Revert))
        } else {
            LevelEffect::Stay
        }
    }

    fn apply(&mut self, action: &'static GuiAction, value: String, cx: &mut App) -> LevelEffect {
        self.previewing = None;
        let title = self
            .offer(action)
            .and_then(|offer| offer.options.iter().find(|option| option.value == value))
            .map_or(value.as_str(), |option| option.title.as_str());
        record_recent(
            RecentKey::Gui(format!("{}={value}", action.id)),
            format!("{} › {title}", action.title),
            cx,
        );
        LevelEffect::Finish(CommandPaletteEvent::Gui(GuiEvent::Apply { action, value }))
    }

    fn open_values(&mut self, action: &'static GuiAction) -> LevelEffect {
        self.action = Some(action);
        let typed = !self.rest.is_empty();
        self.rest.clear();
        self.rebuild();
        if typed {
            LevelEffect::SetRest(String::new())
        } else {
            self.preview()
        }
    }

    fn leave_values(&mut self) -> LevelEffect {
        self.action = None;
        self.rebuild();
        self.revert()
    }

    fn activate(&mut self, complete: bool, cx: &mut App) -> LevelEffect {
        let Some(pick) = self.selected().map(|row| row.pick.clone()) else {
            return LevelEffect::Stay;
        };
        match pick {
            Pick::Action(action) => {
                let flipped = self
                    .offer(action)
                    .and_then(GuiOffer::flipped)
                    .map(str::to_owned);
                match flipped {
                    Some(value) if !complete => self.apply(action, value, cx),
                    _ => self.open_values(action),
                }
            }
            Pick::Value(action, value) if !complete => self.apply(action, value, cx),
            Pick::Value(..) => LevelEffect::Stay,
        }
    }

    fn step(&mut self, to: impl FnOnce(usize, usize) -> usize) -> LevelEffect {
        let count = self.rows.len();
        if count == 0 {
            return LevelEffect::Stay;
        }
        let next = to(self.selected.get().min(count - 1), count);
        self.selected.set(next);
        self.scroll.scroll_to_item(next);
        self.preview()
    }
}

impl PaletteActionLevel for GuiLevel {
    fn attach(&mut self, palette: WeakEntity<CommandPaletteView>, _: &mut App) {
        self.palette = Some(palette);
    }

    fn update(&mut self, context: &ActionContext, _: &mut Window, cx: &mut App) -> LevelEffect {
        self.rest.clone_from(&context.rest);
        if let Some(id) = context.focus
            && self.action.is_none_or(|action| action.id != id)
            && let Some(offer) = self.offers.iter().find(|offer| offer.action.id == id)
        {
            let action = offer.action;
            if let Some(value) = offer.flipped().map(str::to_owned) {
                return self.apply(action, value, cx);
            }
            self.action = Some(action);
            self.focused = true;
        }
        self.rebuild();
        self.preview()
    }

    fn key(&mut self, key: ActionKey, _: &mut Window, cx: &mut App) -> Option<LevelEffect> {
        Some(match key {
            ActionKey::Up => self.step(|at, count| at.checked_sub(1).unwrap_or(count - 1)),
            ActionKey::Down => self.step(|at, count| (at + 1) % count),
            ActionKey::First => self.step(|_, _| 0),
            ActionKey::Last => self.step(|_, count| count - 1),
            ActionKey::Enter => self.activate(false, cx),
            ActionKey::Tab => self.activate(true, cx),
            ActionKey::ShiftTab | ActionKey::Backspace if self.focused && self.rest.is_empty() => {
                LevelEffect::Leave(
                    self.previewing
                        .take()
                        .map(|_| CommandPaletteEvent::Gui(GuiEvent::Revert)),
                )
            }
            ActionKey::ShiftTab if self.action.is_some() => self.leave_values(),
            ActionKey::Backspace if self.action.is_some() && self.rest.is_empty() => {
                self.leave_values()
            }
            _ => return None,
        })
    }

    fn rows(&mut self, window: &mut Window, cx: &mut App) -> AnyElement {
        if self.rows.is_empty() {
            return command_palette_empty(cx).into_any_element();
        }
        let height = (self.rows.len() as f32 * command_palette_row_height()).min(
            command_palette_list_height(window.viewport_size().height, false),
        );
        let selected = self.selected.get();
        let header = self
            .action
            .map(|action| command_palette_section(action.title, ">", cx));
        div()
            .id("gui-level-rows")
            .flex()
            .flex_col()
            .max_h(px(height
                + header
                    .as_ref()
                    .map_or(0.0, |_| command_palette_row_height())))
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .children(header)
            .children(self.rows.iter().enumerate().map(|(index, row)| {
                let hover = Rc::clone(&self.selected);
                let click = Rc::clone(&self.selected);
                let hover_palette = self.palette.clone();
                let click_palette = self.palette.clone();
                command_palette_entry(("gui-level-row", index), &row.row, index == selected, cx)
                    .on_mouse_enter(move |_, _, cx| {
                        if hover.replace(index) != index
                            && let Some(palette) = hover_palette.as_ref()
                        {
                            palette.update(cx, |_, cx| cx.notify()).ok();
                        }
                    })
                    .on_click(move |_, window, cx| {
                        click.set(index);
                        if let Some(palette) = click_palette.as_ref() {
                            palette
                                .update(cx, |palette, cx| {
                                    palette.press_level(ActionKey::Enter, window, cx);
                                })
                                .ok();
                        }
                        cx.stop_propagation();
                    })
            }))
            .into_any_element()
    }

    fn hints(&self, _: &App) -> Vec<PaletteHint> {
        let enter = match self.selected().map(|row| &row.pick) {
            Some(Pick::Value(..)) => "apply",
            Some(Pick::Action(action))
                if self
                    .offer(action)
                    .is_some_and(|offer| offer.flipped().is_some()) =>
            {
                "switch"
            }
            Some(Pick::Action(_)) | None => "choose",
        };
        let mut hints = Vec::new();
        if self.action.is_none() {
            hints.push(PaletteHint {
                key: "tab",
                label: "values",
            });
        }
        hints.extend([
            PaletteHint {
                key: "enter",
                label: enter,
            },
            PaletteHint {
                key: "backspace",
                label: if self.action.is_some() {
                    "back"
                } else {
                    "leave"
                },
            },
            PaletteHint {
                key: "escape",
                label: "close",
            },
        ]);
        hints
    }

    fn open_recent(&mut self, key: &RecentKey, _: &mut Window, cx: &mut App) -> LevelEffect {
        let RecentKey::Gui(recent) = key else {
            return LevelEffect::Stay;
        };
        let Some((id, value)) = recent.split_once('=') else {
            return LevelEffect::Stay;
        };
        let Some(action) = self
            .offers
            .iter()
            .find(|offer| {
                offer.action.id == id && offer.options.iter().any(|option| option.value == value)
            })
            .map(|offer| offer.action)
        else {
            return LevelEffect::Stay;
        };
        self.apply(action, value.to_owned(), cx)
    }

    fn closing(&mut self, _: &mut App) {
        self.previewing = None;
    }
}
