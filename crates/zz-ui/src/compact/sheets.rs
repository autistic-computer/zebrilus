use ala::{App, ElementId, Entity, IntoElement as _, ParentElement as _, SharedString, Window, px};
use penna::{
    Disableable as _, Sizable as _, Size,
    blocks::sheet::{
        BottomSheet, ModalAction, ModalTone, bottom_sheet, close_sheet, modal_body, modal_buttons,
        modal_groups, sheet_close, sheet_ground,
    },
    blocks::touch::{TouchGroup, TouchRow},
    components::field::Field,
    components::input::{Input, InputState},
};

const MODAL_FIELD: f32 = 48.0;
use zz_protocol::PaneKindSnapshot;
use zz_terminal::SearchStatus;

use crate::IconName;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Confirm {
    pub id: ElementId,
    pub question: SharedString,
    pub detail: Option<SharedString>,
    pub verb: SharedString,
}

impl Confirm {
    #[must_use]
    pub fn sheet(
        self,
        on_confirm: impl Fn(&mut Window, &mut App) + 'static,
        on_dismiss: impl Fn(&mut Window, &mut App) + 'static,
        window: &Window,
        cx: &App,
    ) -> BottomSheet {
        let buttons = modal_buttons(
            self.id.clone(),
            ModalAction::new(
                "confirm-sheet-act",
                self.verb,
                ModalTone::Danger,
                on_confirm,
            ),
            window,
            cx,
        );
        let sheet = bottom_sheet(
            self.id,
            self.question,
            Vec::new(),
            modal_body().child(buttons),
            px(0.0),
            on_dismiss,
        )
        .modal();
        match self.detail {
            Some(detail) => sheet.detail(detail),
            None => sheet,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rename {
    pub id: ElementId,
    pub title: SharedString,
    pub current: SharedString,
}

impl Rename {
    #[must_use]
    pub fn savable(&self, typed: &str) -> bool {
        let typed = typed.trim();
        !typed.is_empty() && typed != self.current.trim()
    }

    #[must_use]
    pub fn sheet(
        self,
        input: &Entity<InputState>,
        on_save: impl Fn(SharedString, &mut Window, &mut App) + 'static,
        on_dismiss: impl Fn(&mut Window, &mut App) + 'static,
        window: &Window,
        cx: &App,
    ) -> BottomSheet {
        let ground = sheet_ground(cx);
        let savable = self.savable(&input.read(cx).value());
        let typed = input.clone();
        let size = Size::Size(px(MODAL_FIELD));
        let buttons = modal_buttons(
            self.id.clone(),
            ModalAction::new(
                "rename-sheet-save",
                "Rename",
                ModalTone::Accent,
                move |window, cx| {
                    let name = typed.read(cx).value().trim().to_owned();
                    on_save(name.into(), window, cx);
                },
            )
            .disabled(!savable),
            window,
            cx,
        );
        let body = modal_body()
            .child(
                Field::new().with_size(size).child(
                    Input::new(input)
                        .id("rename-sheet-field")
                        .with_size(size)
                        .on(ground),
                ),
            )
            .child(buttons);
        bottom_sheet(self.id, self.title, Vec::new(), body, px(0.0), on_dismiss).modal()
    }
}

#[must_use]
pub fn hold_sheet(
    id: impl Into<ElementId>,
    title: impl Into<SharedString>,
    rows: impl IntoIterator<Item = TouchRow>,
    on_dismiss: impl Fn(&mut Window, &mut App) + 'static,
    cx: &App,
) -> BottomSheet {
    let id = id.into();
    let close = id.clone();
    let (danger, rest): (Vec<_>, Vec<_>) = rows.into_iter().partition(TouchRow::is_danger);
    let groups = [rest, danger]
        .into_iter()
        .filter(|rows| !rows.is_empty())
        .map(|rows| TouchGroup::new().ruled().rows(rows));
    bottom_sheet(
        id,
        title,
        [sheet_close("hold-sheet-close", cx)
            .on_click(move |_, _, cx| close_sheet(close.clone(), cx))
            .into_any_element()],
        modal_groups().children(groups),
        px(0.0),
        on_dismiss,
    )
    .modal()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkAction {
    Browser,
    Phone,
    Copy,
}

#[must_use]
pub fn link_sheet(
    id: impl Into<ElementId>,
    uri: impl Into<SharedString>,
    actions: &[LinkAction],
    on_pick: impl Fn(LinkAction, &mut Window, &mut App) + 'static,
    on_dismiss: impl Fn(&mut Window, &mut App) + 'static,
    cx: &App,
) -> BottomSheet {
    let on_pick = std::rc::Rc::new(on_pick);
    let rows = actions.iter().map(|action| {
        let (id, label, icon) = match action {
            LinkAction::Browser => ("link-browser", "Open in a browser pane", IconName::Globe),
            LinkAction::Phone => ("link-phone", "Open on this phone", IconName::ExternalLink),
            LinkAction::Copy => ("link-copy", "Copy", IconName::Copy),
        };
        let on_pick = std::rc::Rc::clone(&on_pick);
        let action = *action;
        TouchRow::new(id, label)
            .icon(icon)
            .on_click(move |_, window, cx| on_pick(action, window, cx))
    });
    hold_sheet(id, uri, rows, on_dismiss, cx).cut_middle()
}

#[must_use]
pub fn find_count(query: &str, status: Option<SearchStatus>) -> (String, bool) {
    if query.is_empty() {
        return (String::new(), false);
    }
    match status {
        Some(status) if status.invalid_pattern() => ("Invalid pattern".to_owned(), false),
        Some(status) if status.total > 0 => (
            format!("{} of {}", status.current().max(1), status.total),
            true,
        ),
        Some(status) if status.pending() => (String::new(), false),
        _ => ("No matches".to_owned(), false),
    }
}

#[must_use]
pub fn close_pane_detail(kind: &PaneKindSnapshot, title: &str) -> Option<String> {
    match kind {
        PaneKindSnapshot::Terminal => Some(format!("{title} stops. Its scrollback goes.")),
        PaneKindSnapshot::Agent(_) => Some("The chat stops. Its transcript stays.".to_owned()),
        PaneKindSnapshot::Browser(_) => Some("The page closes.".to_owned()),
        _ => None,
    }
}

#[must_use]
pub fn close_many_detail(count: usize, noun: &str) -> String {
    format!("Its {} close.", super::overview_count(count, noun))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_find_count_says_where_the_current_match_sits() {
        assert_eq!(
            find_count("", Some(SearchStatus::new(1, 3))),
            (String::new(), false)
        );
        assert_eq!(
            find_count("pill", Some(SearchStatus::new(3, 12))),
            ("3 of 12".to_owned(), true)
        );
        assert_eq!(
            find_count("pill", Some(SearchStatus::new(0, 0))),
            ("No matches".to_owned(), false)
        );
        assert_eq!(
            find_count("pill", Some(SearchStatus::new(0, 0).with_pending(true))),
            (String::new(), false)
        );
        assert_eq!(
            find_count(
                "(",
                Some(SearchStatus::new(0, 0).with_invalid_pattern(true))
            ),
            ("Invalid pattern".to_owned(), false)
        );
    }
}
