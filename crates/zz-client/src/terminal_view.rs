use zz_protocol::PaneId;
use zz_terminal::{
    Cursor, CursorBlinkPolicy, IMAGE_PLACEHOLDER_SCHEME, KeyCode, KeyInput, Modifiers,
    PointerCellEvent, ScrollbarState, SearchCase, SearchMode, SearchQuery, SearchStatus,
    SessionStatus, TerminalMode, TerminalMouseButton, TerminalMouseInput, TerminalMousePhase,
    TerminalViewAction,
};

use crate::{ChromeAction, ChromeKeymap, TERMINAL_TABLE};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PointerGrid {
    pub left: f32,
    pub top: f32,
    pub width: f32,
    pub height: f32,
    pub cell_width: f32,
    pub line_height: f32,
    pub content_offset: f32,
    pub scale: f32,
    pub columns: u16,
    pub rows: u16,
}

impl PointerGrid {
    #[must_use]
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.left
            && x <= self.left + self.width
            && y >= self.top
            && y <= self.top + self.height
    }

    #[must_use]
    pub fn input(
        &self,
        x: f32,
        y: f32,
        phase: TerminalMousePhase,
        button: Option<TerminalMouseButton>,
        click_count: usize,
        modifiers: Modifiers,
        force_selection: bool,
    ) -> TerminalMouseInput {
        let x = (x - self.left).clamp(0., self.width);
        let y = (y - self.top).clamp(0., self.height);
        let column = (x / self.cell_width) as u16;
        let row = ((y - self.content_offset).max(0.) / self.line_height) as u16;
        TerminalMouseInput::new(
            phase,
            button,
            PointerCellEvent {
                column: column.min(self.columns.saturating_sub(1)),
                row: row.min(self.rows.saturating_sub(1)),
                click_count: click_count.min(3) as u8,
                rectangle: modifiers.alt(),
            },
            (x * self.scale).round() as u32,
            (y * self.scale).round() as u32,
            (self.width * self.scale).round() as u32,
            (self.height * self.scale).round() as u32,
            (self.cell_width * self.scale).round().max(1.) as u32,
            (self.line_height * self.scale).round().max(1.) as u32,
            modifiers,
            force_selection,
        )
    }
}

#[must_use]
pub fn selection_autoscroll_lines(top: f32, bottom: f32, line_height: f32, y: f32) -> i32 {
    let distance = if y < top {
        -(top - y)
    } else if y > bottom {
        y - bottom
    } else {
        return 0;
    };
    let rows = (distance.abs() / line_height.max(1.0))
        .ceil()
        .clamp(1.0, 8.0) as i32;
    if distance.is_sign_negative() {
        -rows
    } else {
        rows
    }
}

#[must_use]
pub fn scrollbar_strip_hit(
    right: f32,
    top: f32,
    bottom: f32,
    gutter: f32,
    scrollbar: ScrollbarState,
    x: f32,
    y: f32,
) -> bool {
    scrollbar.total > scrollbar.len && x >= right - gutter && x <= right && y >= top && y <= bottom
}

#[must_use]
pub fn scroll_fraction(top: f32, height: f32, y: f32) -> u32 {
    let fraction = ((y - top) / height.max(1.0)).clamp(0.0, 1.0);
    (fraction * u32::MAX as f32).round() as u32
}

pub fn accumulate_scroll(remainder: &mut f32, delta: f32) -> i32 {
    *remainder += delta;
    let lines = *remainder as i32;
    *remainder -= lines as f32;
    lines
}

#[must_use]
pub fn cursor_should_blink(
    cursor: Option<Cursor>,
    policy: CursorBlinkPolicy,
    focused: bool,
) -> bool {
    focused
        && cursor.is_some_and(|cursor| {
            cursor.visible()
                && match policy {
                    CursorBlinkPolicy::Off => false,
                    CursorBlinkPolicy::On => true,
                    CursorBlinkPolicy::Terminal => cursor.blinking(),
                }
        })
}

#[must_use]
pub fn terminal_shortcut(
    keymaps: [&ChromeKeymap; 2],
    claims_prefix: bool,
    modal_capture: Option<PaneId>,
    input: &KeyInput,
) -> Option<ChromeAction> {
    if claims_prefix || modal_capture.is_some() {
        return None;
    }
    keymaps
        .into_iter()
        .find_map(|keymap| keymap.resolve(TERMINAL_TABLE, input))
}

pub fn search_key_action(query: &mut SearchQuery, input: &KeyInput) -> Option<TerminalViewAction> {
    match input.key {
        KeyCode::Character('r' | 'R') if input.modifiers.alt() => {
            query.mode = match query.mode {
                SearchMode::Literal => SearchMode::Regex,
                SearchMode::Regex => SearchMode::Literal,
            };
            Some(TerminalViewAction::SearchUpdate(query.clone()))
        }
        KeyCode::Character('c' | 'C') if input.modifiers.alt() => {
            query.case = match query.case {
                SearchCase::Smart => SearchCase::Sensitive,
                SearchCase::Sensitive => SearchCase::Insensitive,
                SearchCase::Insensitive => SearchCase::Smart,
            };
            Some(TerminalViewAction::SearchUpdate(query.clone()))
        }
        _ => None,
    }
}

#[must_use]
pub fn search_result_text(status: Option<SearchStatus>) -> String {
    match status {
        Some(status) if status.invalid_pattern() => "Invalid pattern".to_owned(),
        Some(status) if status.pending() => "Searching…".to_owned(),
        Some(status) if status.total > 0 => {
            format!("{} / {}", status.current(), status.total)
        }
        _ => "0 matches".to_owned(),
    }
}

#[must_use]
pub fn search_case_label(case: SearchCase) -> &'static str {
    match case {
        SearchCase::Smart => "Smart case",
        SearchCase::Sensitive => "Match case",
        SearchCase::Insensitive => "Ignore case",
    }
}

#[must_use]
pub fn presented_uri(uri: &str) -> String {
    let mut characters = uri.chars();
    let mut presented = characters.by_ref().take(240).collect::<String>();
    if characters.next().is_some() {
        presented.push('…');
    }
    presented
}

#[must_use]
pub fn pasted_image_number(uri: &str) -> Option<u32> {
    uri.strip_prefix(IMAGE_PLACEHOLDER_SCHEME)?
        .strip_prefix("://")?
        .parse()
        .ok()
}

#[must_use]
pub fn openable_uri(uri: &str) -> bool {
    uri.starts_with("https://") || uri.starts_with("http://") || uri.starts_with("mailto:")
}

#[must_use]
pub fn mode_text(mode: TerminalMode, unseen_output: u32) -> Option<(Option<&'static str>, String)> {
    match mode {
        TerminalMode::Live if unseen_output > 0 => Some((None, format!("+{unseen_output} output"))),
        TerminalMode::Live => None,
        TerminalMode::Copy {
            position,
            total,
            hide_position,
        } => Some((
            Some("Copy mode"),
            match (hide_position, unseen_output) {
                (true, 0) => String::new(),
                (true, unseen) => format!("+{unseen} output"),
                (false, 0) => format!("{position} / {total}"),
                (false, unseen) => format!("{position} / {total} · +{unseen} output"),
            },
        )),
        TerminalMode::View { position, total } => {
            Some((Some("View mode"), format!("{position} / {total}")))
        }
    }
}

#[must_use]
pub fn dead_label(dead: bool, status: Option<u32>) -> Option<String> {
    dead.then(|| status.map_or_else(|| "Dead".to_owned(), |status| format!("Dead · {status}")))
}

#[must_use]
pub fn status_text(status: &SessionStatus) -> Option<String> {
    match status {
        SessionStatus::Starting | SessionStatus::Running => None,
        SessionStatus::Exited(exit) => Some(exit.signal.as_ref().map_or_else(
            || format!("shell exited ({})", exit.code),
            |signal| format!("shell exited: {signal}"),
        )),
        SessionStatus::Failed(error) => Some(error.as_ref().clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pointer_grid_clamps_to_the_grid_and_carries_physical_pixels() {
        let grid = PointerGrid {
            left: 10.,
            top: 20.,
            width: 80.,
            height: 32.,
            cell_width: 8.,
            line_height: 16.,
            content_offset: 0.,
            scale: 2.,
            columns: 10,
            rows: 2,
        };
        let input = grid.input(
            27.,
            45.,
            TerminalMousePhase::Press,
            Some(TerminalMouseButton::Left),
            5,
            Modifiers::new(true, false, true, false),
            true,
        );
        assert_eq!(input.cell.column, 2);
        assert_eq!(input.cell.row, 1);
        assert_eq!(input.cell.click_count, 3);
        assert!(input.cell.rectangle);
        assert_eq!((input.x, input.y), (34, 50));
        assert_eq!((input.cell_width, input.cell_height), (16, 32));
        let outside = grid.input(
            500.,
            500.,
            TerminalMousePhase::Motion,
            None,
            1,
            Modifiers::default(),
            false,
        );
        assert_eq!((outside.cell.column, outside.cell.row), (9, 1));
    }

    #[test]
    fn selection_autoscroll_speeds_up_with_distance() {
        assert_eq!(selection_autoscroll_lines(10., 100., 16., 50.), 0);
        assert_eq!(selection_autoscroll_lines(10., 100., 16., 5.), -1);
        assert_eq!(selection_autoscroll_lines(10., 100., 16., 140.), 3);
        assert_eq!(selection_autoscroll_lines(10., 100., 16., 1000.), 8);
    }

    #[test]
    fn openable_uris_are_web_and_mail_only() {
        assert!(openable_uri("https://zzmux.sh"));
        assert!(openable_uri("mailto:a@b.c"));
        assert!(!openable_uri("file:///etc/passwd"));
        assert_eq!(pasted_image_number("zz-image://7"), Some(7));
        assert_eq!(pasted_image_number("https://7"), None);
    }
}
