use std::rc::Rc;

use zz_client::{
    completion::apply_completion,
    place::{self, ActionKind, Level, LevelSet, Place, Written, parse, read_written},
    recents::{RecentKey, now_secs},
};
use zz_gpui::{App, Context, Window};
use zz_protocol::{CommandInvocation, MAX_COMMAND_PROMPT_BYTES, catalog_command_spec};

use super::{CommandPaletteView, has_command_arguments};
use crate::command::{
    ActionContext, ActionKey, PaletteHint, PaletteHostId, PaletteTarget,
    palette_model::{PaletteAction, UnifiedPalette, needs_arguments},
    palette_places::{Chip, chip_for, palette_fleet, place_label},
    palette_recents, record_recent,
};

fn chips_to(fleet: &place::Fleet, place: Place) -> Vec<Chip> {
    Level::ALL
        .into_iter()
        .filter(|level| place.level().is_some_and(|deepest| *level <= deepest))
        .filter(|level| match level {
            Level::Host => place.host.is_some(),
            Level::Session => place.session.is_some(),
            Level::Window => place.window.is_some(),
            Level::Pane => place.pane.is_some(),
        })
        .filter_map(|level| {
            let at = level.deeper().map_or(place, |deeper| place.above(deeper));
            chip_for(fleet, at, None)
        })
        .collect()
}

impl CommandPaletteView {
    pub(super) fn start_places(
        &mut self,
        field: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let tree = self.backend.tree(cx);
        let fleet = palette_fleet(&tree);
        if let Some(unified) = &mut self.unified {
            unified.mode = None;
            unified.command = None;
            unified.host = None;
            unified.chips = chip_for(&fleet, Place::host(tree.attached.0), None)
                .map(|chip| Chip { auto: true, ..chip })
                .into_iter()
                .collect();
            unified.fleet = Rc::new(fleet);
        }
        self.set_query(field.to_owned(), window, cx);
    }

    pub(super) fn places(&self) -> bool {
        self.window_chooser.is_none()
            && self.unified.as_ref().is_some_and(UnifiedPalette::is_places)
    }

    fn boost(&self, cx: &App) -> impl Fn(&Place) -> i64 + use<> {
        let recents = palette_recents(cx);
        let now = now_secs();
        let fleet = self
            .unified
            .as_ref()
            .map(|unified| Rc::clone(&unified.fleet))
            .unwrap_or_default();
        move |place: &Place| {
            place::recent_key(&fleet, place).map_or(0, |key| recents.boost(&key, now))
        }
    }

    pub(super) fn line_place(&self) -> Option<Place> {
        let unified = self.unified.as_ref()?;
        let line = parse(&self.last_input);
        let resolved = place::resolve(&unified.fleet, &unified.scope(), &line.parts, &|_| 0);
        resolved.failed.is_none().then_some(resolved.place)
    }

    fn set_command_input(&mut self, input: String, window: &mut Window, cx: &mut Context<Self>) {
        if !self.places() {
            self.set_query(input, window, cx);
            return;
        }
        let prefix = parse(&self.last_input)
            .action
            .filter(|action| action.kind == ActionKind::Command)
            .map_or_else(
                || ":".to_owned(),
                |action| self.last_input[..action.rest_range().start].to_owned(),
            );
        self.set_query(format!("{prefix}{input}"), window, cx);
    }

    pub(super) fn set_files_body(
        &mut self,
        body: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let prefix = parse(&self.last_input)
            .action
            .filter(|action| action.kind == ActionKind::Files)
            .map_or_else(
                || "&".to_owned(),
                |action| self.last_input[..action.rest_range().start].to_owned(),
            );
        self.set_query(format!("{prefix}{body}"), window, cx);
    }

    pub(super) fn files_body(&self) -> Option<String> {
        parse(&self.last_input)
            .action
            .filter(|action| action.kind == ActionKind::Files)
            .map(|action| action.rest)
    }

    pub(super) fn commit_typed(
        &mut self,
        value: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.places() {
            return false;
        }
        let boost = self.boost(cx);
        let Some(unified) = &mut self.unified else {
            return false;
        };
        let fleet = Rc::clone(&unified.fleet);
        if self.last_input.is_empty()
            && value.chars().count() > 1
            && let Some(Written::Place(place)) = read_written(value, &fleet)
        {
            unified.chips = chips_to(&fleet, place);
            self.set_query(String::new(), window, cx);
            return true;
        }
        let line = parse(value);
        let count = if line.action.is_some() {
            line.parts.len()
        } else {
            line.parts.len().saturating_sub(1)
        };
        if count == 0 {
            if value.chars().count() > 1
                && let Some(chip) = unified.chips.last_mut()
            {
                chip.typed = None;
            }
            return false;
        }
        let first = line.parts[0].level;
        let mut chips: Vec<Chip> = unified
            .chips
            .iter()
            .filter(|chip| chip.level < first)
            .cloned()
            .collect();
        for part in &line.parts[..count] {
            let scope = chips.last().map_or_else(Place::default, |chip| chip.place);
            let Some(chip) = place::candidates(&fleet, &scope, part, &boost)
                .into_iter()
                .next()
                .and_then(|best| chip_for(&fleet, best.place, None))
            else {
                return false;
            };
            chips.push(chip);
        }
        let typed = &line.parts[count - 1].range;
        if let Some(chip) = chips.last_mut() {
            chip.typed = Some(value[typed.clone()].trim_end().to_owned());
        }
        let rest = line
            .parts
            .get(count)
            .map(|part| part.range.start)
            .or_else(|| line.action.as_ref().map(|action| action.start))
            .unwrap_or(value.len());
        unified.chips = chips;
        self.set_query(value[rest..].to_owned(), window, cx);
        true
    }

    pub(super) fn level_context(&self, cx: &App) -> Option<ActionContext> {
        if !self.places() {
            return None;
        }
        let unified = self.unified.as_ref()?;
        let line = parse(&self.last_input);
        let action = line.action.clone()?;
        if action.kind == ActionKind::Command {
            return None;
        }
        let written = unified
            .chips
            .iter()
            .filter(|chip| !chip.auto)
            .map(|chip| chip.level)
            .chain(line.parts.iter().map(|part| part.level))
            .fold(LevelSet::default(), LevelSet::with);
        let host_auto = unified
            .chips
            .iter()
            .any(|chip| chip.level == Level::Host && chip.auto)
            && !line.parts.iter().any(|part| part.level == Level::Host);
        Some(ActionContext {
            kind: action.kind,
            fleet: Rc::clone(&unified.fleet),
            place: self.line_place().unwrap_or_else(|| unified.scope()),
            written,
            host_auto,
            rest_range: action.rest_range(),
            rest: action.rest,
            insert_pane: self.insert_pane.or_else(|| self.backend.active_pane(cx)),
        })
    }

    pub(super) fn sync_level(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(context) = self.level_context(cx) else {
            if let Some((_, mut level)) = self.level.take() {
                level.closing(cx);
            }
            return;
        };
        if self
            .level
            .as_ref()
            .is_none_or(|(kind, _)| *kind != context.kind)
        {
            if let Some((_, mut level)) = self.level.take() {
                level.closing(cx);
            }
            if let Some(mut level) = self.backend.action_level(context.kind, cx) {
                level.attach(cx.entity().downgrade(), cx);
                self.level = Some((context.kind, level));
            }
        }
        let effect = match self.level.as_mut() {
            Some((_, level)) => level.update(&context, window, cx),
            None => return,
        };
        self.apply_level_effect(effect, window, cx);
    }

    pub(super) fn level_key(
        &mut self,
        key: ActionKey,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let effect = match self.level.as_mut() {
            Some((_, level)) => level.key(key, window, cx),
            None => return false,
        };
        effect.is_some_and(|effect| {
            self.apply_level_effect(effect, window, cx);
            true
        })
    }

    pub(super) fn caret_at_end(&self, cx: &App) -> bool {
        let input = self.input.read(cx);
        input.selected_range().is_empty() && input.cursor() == input.value().len()
    }

    fn selected_action(&self) -> Option<PaletteAction> {
        let unified = self.unified.as_ref()?;
        unified.rows.get(self.selected?)?.action.clone()
    }

    pub(super) fn enter_place(
        &mut self,
        place: Place,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(unified) = &mut self.unified else {
            return;
        };
        let Some(chip) = chip_for(&unified.fleet, place, None) else {
            return;
        };
        if chip.level == Level::Host
            && let Some(host) = place.host
            && !unified.fleet.host(host).is_some_and(|host| host.online)
        {
            self.backend.connect_host(PaletteHostId(host), cx);
        }
        let Some(unified) = &mut self.unified else {
            return;
        };
        let field = chip
            .level
            .deeper()
            .map_or_else(String::new, |level| level.sigil().to_string());
        unified.chips.retain(|existing| existing.level < chip.level);
        unified.chips.push(chip);
        self.set_query(field, window, cx);
    }

    pub(super) fn go_to(&mut self, place: Place, window: &mut Window, cx: &mut Context<Self>) {
        let Some(unified) = &self.unified else {
            return;
        };
        let Some(host) = place.host else {
            return;
        };
        let fleet = Rc::clone(&unified.fleet);
        let label = place_label(&fleet, &place).unwrap_or_default();
        let host_id = PaletteHostId(host);
        let target = place
            .pane
            .map(PaletteTarget::Pane)
            .or(place.window.map(PaletteTarget::Window))
            .or(place.session.map(PaletteTarget::Session));
        let Some(target) = target else {
            if !fleet.host(host).is_some_and(|host| host.online) {
                self.backend.connect_host(host_id, cx);
            }
            if self.backend.activate_host(host_id, cx) {
                self.close(cx);
                Self::notify_action(format!("Switched to {label}"), window, cx);
            } else {
                self.enter_place(place, window, cx);
            }
            return;
        };
        if self.backend.activate(host_id, target, cx) {
            if let Some(key) = place::recent_key(&fleet, &place) {
                record_recent(key, label.clone(), cx);
            }
            self.close(cx);
            Self::notify_action(format!("Switched to {label}"), window, cx);
        }
    }

    fn run_place_command(
        &mut self,
        input: String,
        invocation: Option<CommandInvocation>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(unified) = &self.unified else {
            return;
        };
        let place = self.line_place().unwrap_or_else(|| unified.scope());
        let host = place.host.map_or(unified.tree.attached, PaletteHostId);
        let name = input
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .to_owned();
        let target = catalog_command_spec(&name)
            .and_then(|spec| place::command_target(spec, &place))
            .filter(|_| !place::has_target(&input));
        let (line, invocation) = match target {
            Some(target) => (
                place::with_target(&input, &target),
                invocation
                    .filter(|invocation| invocation.args.is_empty())
                    .map(|_| CommandInvocation::new(&name, ["-t", target.as_str()])),
            ),
            None => (input.clone(), invocation),
        };
        if let Some(unified) = &mut self.unified {
            unified.host = Some(host);
        }
        record_recent(RecentKey::Command(input), name, cx);
        self.run_command(line, invocation, window, cx);
    }

    fn activate_command(
        &mut self,
        rest: String,
        complete: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if has_command_arguments(&rest) && !complete && !self.navigation_engaged {
            self.run_place_command(rest, None, window, cx);
            return;
        }
        match self.selected_action() {
            Some(PaletteAction::Command(spec)) => {
                if complete || needs_arguments(spec) {
                    self.set_command_input(format!("{} ", spec.name), window, cx);
                } else {
                    self.run_place_command(
                        spec.name.to_owned(),
                        Some(CommandInvocation::new(spec.name, [] as [&str; 0])),
                        window,
                        cx,
                    );
                }
            }
            Some(PaletteAction::Completion(suggestion)) => {
                let (completed, _) = apply_completion(&rest, &suggestion);
                if completed.len() <= MAX_COMMAND_PROMPT_BYTES {
                    self.set_command_input(completed, window, cx);
                }
            }
            Some(PaletteAction::History(command)) => {
                self.run_place_command(command, None, window, cx);
            }
            _ if !complete && !rest.trim().is_empty() => {
                self.run_place_command(rest, None, window, cx);
            }
            _ => {}
        }
    }

    pub(super) fn activate_places(
        &mut self,
        complete: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let line = parse(&self.last_input);
        if let Some(action) = line.action {
            if action.kind == ActionKind::Command {
                self.activate_command(action.rest, complete, window, cx);
            } else {
                let key = if complete {
                    ActionKey::Tab
                } else {
                    ActionKey::Enter
                };
                self.level_key(key, window, cx);
            }
            return;
        }
        match self.selected_action() {
            Some(PaletteAction::Place(place)) => {
                if complete && place.level() != Some(Level::Pane) {
                    self.enter_place(place, window, cx);
                } else if !complete {
                    self.go_to(place, window, cx);
                }
            }
            Some(PaletteAction::Command(spec)) => {
                if complete || needs_arguments(spec) {
                    self.set_command_input(format!("{} ", spec.name), window, cx);
                } else {
                    self.run_place_command(
                        spec.name.to_owned(),
                        Some(CommandInvocation::new(spec.name, [] as [&str; 0])),
                        window,
                        cx,
                    );
                }
            }
            Some(PaletteAction::History(command)) if !complete => {
                self.run_place_command(command, None, window, cx);
            }
            Some(PaletteAction::Recent(key)) if !complete => self.open_recent(&key, window, cx),
            None if !complete => {
                if let Some(place) = self.line_place()
                    && place != Place::default()
                    && self
                        .unified
                        .as_ref()
                        .is_some_and(|unified| !unified.chips.iter().all(|chip| chip.auto))
                {
                    self.go_to(place, window, cx);
                }
            }
            _ => {}
        }
    }

    fn open_recent(&mut self, key: &RecentKey, window: &mut Window, cx: &mut Context<Self>) {
        let kind = match key {
            RecentKey::Gui(_) => ActionKind::Gui,
            RecentKey::File { .. } => ActionKind::Files,
            RecentKey::Command(command) => {
                self.run_place_command(command.clone(), None, window, cx);
                return;
            }
            RecentKey::Place { .. } => {
                if let Some(place) = self
                    .unified
                    .as_ref()
                    .and_then(|unified| place::recent_place(&unified.fleet, key))
                {
                    self.go_to(place, window, cx);
                }
                return;
            }
        };
        let Some(mut level) = self.backend.action_level(kind, cx) else {
            return;
        };
        level.attach(cx.entity().downgrade(), cx);
        let effect = level.open_recent(key, window, cx);
        self.apply_level_effect(effect, window, cx);
    }

    pub(super) fn shift_tab_places(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.level_key(ActionKey::ShiftTab, window, cx) {
            return true;
        }
        let Some(chip) = self
            .unified
            .as_mut()
            .and_then(|unified| unified.chips.pop())
        else {
            return false;
        };
        self.set_query(String::new(), window, cx);
        if let Some(unified) = &self.unified
            && let Some(index) = unified.rows.iter().position(|entry| {
                matches!(entry.action, Some(PaletteAction::Place(place)) if place == chip.place)
            })
        {
            self.selected = Some(index);
            self.navigation_engaged = true;
            cx.notify();
        }
        true
    }

    pub(super) fn backspace_places(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.caret_at_end(cx) && self.level_key(ActionKey::Backspace, window, cx) {
            return true;
        }
        if !self.input.read(cx).value().is_empty() {
            return false;
        }
        let Some(chip) = self
            .unified
            .as_mut()
            .and_then(|unified| unified.chips.pop())
        else {
            return false;
        };
        self.set_query(chip.typed.unwrap_or_default(), window, cx);
        true
    }

    pub(super) fn place_address(&self) -> Option<String> {
        let unified = self.unified.as_ref()?;
        match self.selected_action()? {
            PaletteAction::Place(place) => place::written(&unified.fleet, &place),
            _ => None,
        }
    }

    pub(super) fn place_hints(&self, enter: &'static str, cx: &App) -> Vec<PaletteHint> {
        let mut hints = Vec::new();
        if matches!(
            self.selected_action(),
            Some(PaletteAction::Place(place)) if place.level() != Some(Level::Pane)
        ) {
            hints.push(PaletteHint {
                key: "tab",
                label: "into",
            });
        }
        hints.push(PaletteHint {
            key: "enter",
            label: enter,
        });
        if self.input.read(cx).value().is_empty()
            && self
                .unified
                .as_ref()
                .is_some_and(|unified| !unified.chips.is_empty())
        {
            hints.push(PaletteHint {
                key: "backspace",
                label: "back",
            });
        }
        hints.push(PaletteHint {
            key: "escape",
            label: "close",
        });
        hints
    }
}
