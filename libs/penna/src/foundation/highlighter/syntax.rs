use std::ops::Range;

use ala::{HighlightStyle, SharedString};
use ropey::Rope;
use web_time::Duration;

use super::{LanguageRegistry, theme::HighlightTheme};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Point {
    pub row: usize,
    pub column: usize,
}

impl Point {
    pub const fn new(row: usize, column: usize) -> Self {
        Self { row, column }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InputEdit {
    pub start_byte: usize,
    pub old_end_byte: usize,
    pub new_end_byte: usize,
    pub start_position: Point,
    pub old_end_position: Point,
    pub new_end_position: Point,
}

pub struct SyntaxHighlighter {
    language: SharedString,
    text: Rope,
}

impl SyntaxHighlighter {
    pub fn new(lang: &str) -> Self {
        let language = LanguageRegistry::singleton()
            .language(lang)
            .map_or_else(|| SharedString::new_static("text"), |config| config.name);
        Self {
            language,
            text: Rope::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.text.len() == 0
    }

    pub fn language(&self) -> &SharedString {
        &self.language
    }

    pub fn text(&self) -> &Rope {
        &self.text
    }

    pub fn update(
        &mut self,
        _edit: Option<InputEdit>,
        text: &Rope,
        _timeout: Option<Duration>,
    ) -> bool {
        self.text = text.clone();
        true
    }

    pub fn styles(
        &self,
        _range: &Range<usize>,
        _theme: &HighlightTheme,
    ) -> Vec<(Range<usize>, HighlightStyle)> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlighting_is_inert() {
        let mut highlighter = SyntaxHighlighter::new("rust");
        let text = Rope::from("fn main() { let x = 1; }");
        assert!(highlighter.update(None, &text, None));

        assert_eq!(highlighter.language().as_ref(), "text");
        assert_eq!(highlighter.text().to_string(), text.to_string());
        assert!(
            highlighter
                .styles(&(0..text.len()), &HighlightTheme::default_light())
                .is_empty()
        );
    }
}
