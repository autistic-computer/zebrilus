use std::rc::Rc;

use zz_client::{
    completion::apply_completion,
    place::{self, ActionKind, Level, LevelSet, Place, Written, parse, read_written},
    recents::RecentKey,
    verbs::{self, Filled, Run, Slot, Verb, VerbKind},
};
use zz_gpui::{App, Context, Window};
use zz_protocol::{CommandInvocation, MAX_COMMAND_PROMPT_BYTES, catalog_command_spec};

use super::{CommandPaletteView, has_command_arguments};
use crate::command::{
    ActionContext, ActionKey, PaletteHint, PaletteHostId, PaletteTarget,
    palette_model::{PaletteAction, UnifiedPalette, needs_arguments},
    palette_places::{Chip, Draft, chip_for, palette_fleet, place_label},
    record_recent,
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
            chip_for(fleet, at)
        })
        .collect()
}

fn insert_path_verb() -> Verb {
    Verb {
        name: "insert path".to_owned(),
        keywords: String::new(),
        description: String::new(),
        kind: VerbKind::Zz,
        run: Run::InsertPath,
        slots: vec![Slot::Path],
        raw: false,
    }
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
        let verbs = self.backend.verbs(cx);
        if let Some(unified) = &mut self.unified {
            unified.mode = None;
            unified.command = None;
            unified.host = None;
            unified.draft = None;
            unified.verbs = Rc::from(verbs);
            unified.chips = chip_for(&fleet, Place::host(tree.attached.0))
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

    fn draft(&self) -> Option<&Draft> {
        self.unified.as_ref()?.draft.as_ref()
    }

    fn next_slot(&self) -> Option<Slot> {
        self.draft()?.next().cloned()
    }

    fn set_command_input(&mut self, input: String, window: &mut Window, cx: &mut Context<Self>) {
        if self.places() {
            self.set_query(format!("{}{input}", place::COMMAND_SIGIL), window, cx);
        } else {
            self.set_query(input, window, cx);
        }
    }

    pub(super) fn set_files_body(
        &mut self,
        body: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.next_slot() != Some(Slot::Path)
            && let Some(unified) = &mut self.unified
        {
            let verb = unified
                .verbs
                .iter()
                .find(|verb| verb.run == Run::InsertPath)
                .cloned()
                .unwrap_or_else(insert_path_verb);
            unified.draft = Some(Draft::new(verb));
        }
        self.set_query(body.to_owned(), window, cx);
    }

    pub(super) fn files_body(&self) -> Option<String> {
        (self.places() && self.next_slot() == Some(Slot::Path)).then(|| self.last_input.clone())
    }

    pub(super) fn commit_typed(
        &mut self,
        value: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.places() || !self.last_input.is_empty() || value.chars().count() < 2 {
            return false;
        }
        let Some(unified) = &mut self.unified else {
            return false;
        };
        if unified.draft.is_some() {
            return false;
        }
        let fleet = Rc::clone(&unified.fleet);
        let Some(Written::Place(place)) = read_written(value, &fleet) else {
            return false;
        };
        unified.chips = chips_to(&fleet, place);
        self.set_query(String::new(), window, cx);
        true
    }

    pub(super) fn level_context(&self, cx: &App) -> Option<ActionContext> {
        if !self.places() {
            return None;
        }
        let unified = self.unified.as_ref()?;
        if parse(&self.last_input).command.is_some() {
            return None;
        }
        let (kind, focus) = match unified.draft.as_ref()?.next()? {
            Slot::Path => (ActionKind::Files, None),
            Slot::Value(id) => (ActionKind::Gui, Some(*id)),
            Slot::Target(_) | Slot::Text(_) => return None,
        };
        let written = unified
            .chips
            .iter()
            .filter(|chip| !chip.auto)
            .map(|chip| chip.level)
            .fold(LevelSet::default(), LevelSet::with);
        let host_auto = unified
            .chips
            .iter()
            .any(|chip| chip.level == Level::Host && chip.auto);
        Some(ActionContext {
            kind,
            fleet: Rc::clone(&unified.fleet),
            place: unified.scope(),
            written,
            host_auto,
            rest: self.last_input.clone(),
            rest_range: 0..self.last_input.len(),
            insert_pane: self.insert_pane.or_else(|| self.backend.active_pane(cx)),
            focus,
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
        if key == ActionKey::Enter
            && let Some(pick) = self.pick_folder.clone()
            && let Some(picked) = self
                .level
                .as_ref()
                .and_then(|(_, level)| level.picked_folder(cx))
        {
            self.close(cx);
            pick(picked.0, picked.1, window, cx);
            return true;
        }
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
        let Some(chip) = chip_for(&unified.fleet, place) else {
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
        unified.chips.retain(|existing| existing.level < chip.level);
        unified.chips.push(chip);
        self.set_query(String::new(), window, cx);
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
        let place = unified.scope();
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

    fn start_verb(&mut self, verb: Verb, window: &mut Window, cx: &mut Context<Self>) {
        if verb.raw
            && let Some(command) = verb.command()
        {
            self.set_command_input(format!("{command} "), window, cx);
            return;
        }
        let Some(unified) = &mut self.unified else {
            return;
        };
        let scope = unified.scope();
        let mut draft = Draft::new(verb);
        if matches!(draft.next(), Some(Slot::Target(_)))
            && scope.level().is_some_and(|level| level > Level::Host)
        {
            draft.filled.push((Filled::Place(scope), String::new()));
        }
        unified.draft = Some(draft);
        self.advance(window, cx);
    }

    fn advance(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.draft().is_some_and(|draft| draft.next().is_none()) {
            self.run_draft(window, cx);
        } else {
            self.set_query(String::new(), window, cx);
        }
    }

    fn fill(&mut self, value: Filled, label: String, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(draft) = self
            .unified
            .as_mut()
            .and_then(|unified| unified.draft.as_mut())
        {
            draft.filled.push((value, label));
        }
        self.advance(window, cx);
    }

    fn run_draft(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(unified) = &self.unified else {
            return;
        };
        let Some(draft) = unified.draft.clone() else {
            return;
        };
        let Some(compiled) = verbs::compile(&draft.verb, &draft.values(), &unified.fleet) else {
            return;
        };
        let host = compiled
            .host
            .or(unified.scope().host)
            .map_or(unified.tree.attached, PaletteHostId);
        if let Some(unified) = &mut self.unified {
            unified.host = Some(host);
        }
        record_recent(
            RecentKey::Command(compiled.line.clone()),
            compiled.display,
            cx,
        );
        self.run_command(compiled.line, compiled.invocation, window, cx);
    }

    pub(super) fn draft_usage(&self) -> Option<String> {
        let unified = self.unified.as_ref()?;
        let draft = unified.draft.as_ref()?;
        match draft.verb.kind {
            VerbKind::Gui => return Some("gui · no tmux command".to_owned()),
            VerbKind::Zz => return Some(format!("zz · {}", draft.verb.description)),
            VerbKind::Tmux | VerbKind::Alias => {}
        }
        let mut values = draft.values();
        match (draft.next(), self.selected_action()) {
            (Some(Slot::Target(_)), Some(PaletteAction::Fill(place))) => {
                values.push(Filled::Place(place));
            }
            (Some(Slot::Text(_)), _) if !self.last_input.trim().is_empty() => {
                values.push(Filled::Text(self.last_input.trim().to_owned()));
            }
            _ => {}
        }
        Some(format!(
            "runs  {}",
            verbs::compile(&draft.verb, &values, &unified.fleet)
                .map_or_else(|| draft.verb.spelled(), |compiled| compiled.display)
        ))
    }

    pub(super) fn row_usage(&self) -> Option<String> {
        let unified = self.unified.as_ref()?;
        match self.selected_action()? {
            PaletteAction::Verb(index) => {
                let verb = unified.verbs.get(index)?;
                Some(match verb.kind {
                    VerbKind::Gui => "gui · no tmux command".to_owned(),
                    VerbKind::Zz => format!("zz · {}", verb.description),
                    VerbKind::Tmux | VerbKind::Alias => {
                        let scope = unified.scope();
                        let filled = if matches!(verb.slots.first(), Some(Slot::Target(_)))
                            && scope.level().is_some_and(|level| level > Level::Host)
                        {
                            vec![Filled::Place(scope)]
                        } else {
                            Vec::new()
                        };
                        let line = verbs::compile(verb, &filled, &unified.fleet)
                            .map_or_else(|| verb.spelled(), |compiled| compiled.display);
                        format!("runs  {line}")
                    }
                })
            }
            PaletteAction::Place(place) => {
                verbs::go_to_line(&unified.fleet, &place).map(|line| format!("runs  {line}"))
            }
            _ => None,
        }
    }

    pub(super) fn activate_places(
        &mut self,
        complete: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(command) = parse(&self.last_input).command {
            self.activate_command(command, complete, window, cx);
            return;
        }
        if let Some(slot) = self.next_slot() {
            match slot {
                Slot::Path | Slot::Value(_) => {
                    let key = if complete {
                        ActionKey::Tab
                    } else {
                        ActionKey::Enter
                    };
                    self.level_key(key, window, cx);
                }
                Slot::Target(_) => {
                    if let Some(PaletteAction::Fill(place)) = self.selected_action() {
                        let label = self
                            .unified
                            .as_ref()
                            .and_then(|unified| place_label(&unified.fleet, &place))
                            .unwrap_or_default();
                        self.fill(Filled::Place(place), label, window, cx);
                    }
                }
                Slot::Text(_) => {
                    let text = self.last_input.trim().to_owned();
                    if !text.is_empty() {
                        self.fill(Filled::Text(text.clone()), text, window, cx);
                    }
                }
            }
            return;
        }
        match self.selected_action() {
            Some(PaletteAction::Place(place)) => {
                if complete {
                    self.enter_place(place, window, cx);
                } else if !complete {
                    self.go_to(place, window, cx);
                }
            }
            Some(PaletteAction::Verb(index)) => {
                let verb = self
                    .unified
                    .as_ref()
                    .and_then(|unified| unified.verbs.get(index).cloned());
                if let Some(verb) = verb {
                    self.start_verb(verb, window, cx);
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
                let place = self.unified.as_ref().map(UnifiedPalette::scope);
                if let Some(place) = place
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
        match key {
            RecentKey::Command(command) => {
                self.run_place_command(command.clone(), None, window, cx);
            }
            RecentKey::Place { .. } => {
                if let Some(place) = self
                    .unified
                    .as_ref()
                    .and_then(|unified| place::recent_place(&unified.fleet, key))
                {
                    self.go_to(place, window, cx);
                }
            }
            RecentKey::Gui(_) => {
                let Some(mut level) = self.backend.action_level(ActionKind::Gui, cx) else {
                    return;
                };
                level.attach(cx.entity().downgrade(), cx);
                let effect = level.open_recent(key, window, cx);
                self.apply_level_effect(effect, window, cx);
            }
            RecentKey::File { host, path, .. } => {
                if let Some(unified) = &mut self.unified
                    && let Some(host) = unified.fleet.host_named(host)
                    && let Some(chip) = chip_for(&unified.fleet, Place::host(host.id))
                {
                    unified.chips = vec![chip];
                }
                self.set_files_body(path, window, cx);
                let effect = match self.level.as_mut() {
                    Some((ActionKind::Files, level)) => level.open_recent(key, window, cx),
                    _ => return,
                };
                self.apply_level_effect(effect, window, cx);
            }
        }
    }

    pub(super) fn step_back(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(unified) = &mut self.unified else {
            return false;
        };
        let Some(draft) = &mut unified.draft else {
            let Some(chip) = unified.chips.pop() else {
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
            return true;
        };
        if draft.filled.pop().is_none_or(|(_, label)| label.is_empty()) {
            unified.draft = None;
        }
        self.set_query(String::new(), window, cx);
        true
    }

    pub(super) fn shift_tab_places(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.level_key(ActionKey::ShiftTab, window, cx) {
            return true;
        }
        self.step_back(window, cx)
    }

    pub(super) fn backspace_places(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.caret_at_end(cx) && self.level_key(ActionKey::Backspace, window, cx) {
            return true;
        }
        if !self.input.read(cx).value().is_empty() {
            return false;
        }
        if self.draft().is_some() {
            return self.step_back(window, cx);
        }
        let Some(unified) = &mut self.unified else {
            return false;
        };
        if unified.chips.pop().is_none() {
            return false;
        }
        self.set_query(String::new(), window, cx);
        true
    }

    pub(super) fn place_address(&self) -> Option<String> {
        let unified = self.unified.as_ref()?;
        match self.selected_action()? {
            PaletteAction::Place(place) | PaletteAction::Fill(place) => {
                place::written(&unified.fleet, &place)
            }
            _ => None,
        }
    }

    pub(super) fn place_hints(&self, enter: &'static str, cx: &App) -> Vec<PaletteHint> {
        let mut hints = Vec::new();
        if matches!(self.selected_action(), Some(PaletteAction::Place(_))) {
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
                .is_some_and(|unified| !unified.chips.is_empty() || unified.draft.is_some())
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
