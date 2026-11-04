#![allow(clippy::pedantic, clippy::style, clippy::complexity)]

use std::sync::Arc;

use ala::{Rems, StyleRefinement};

use crate::{Space, TextStyle, foundation::highlighter::HighlightTheme};

pub const READING_MEASURE: f32 = 640.0;

#[derive(Clone)]
pub struct TextViewStyle {
    pub text: Option<TextStyle>,
    pub paragraph_gap: Rems,
    pub highlight_theme: Arc<HighlightTheme>,
    pub code_block: StyleRefinement,
    pub table: StyleRefinement,
    pub table_cell: StyleRefinement,
    pub is_dark: bool,
}

impl PartialEq for TextViewStyle {
    fn eq(&self, other: &Self) -> bool {
        self.paragraph_gap == other.paragraph_gap && self.highlight_theme == other.highlight_theme
    }
}

impl Default for TextViewStyle {
    fn default() -> Self {
        Self {
            text: Some(TextStyle::ReadBody),
            paragraph_gap: Space::S12.rems(),
            highlight_theme: HighlightTheme::default_light().clone(),
            code_block: StyleRefinement::default(),
            table: StyleRefinement::default(),
            table_cell: StyleRefinement::default(),
            is_dark: false,
        }
    }
}

impl TextViewStyle {
    pub fn text(mut self, text: Option<TextStyle>) -> Self {
        self.text = text;
        self
    }

    pub fn paragraph_gap(mut self, gap: Rems) -> Self {
        self.paragraph_gap = gap;
        self
    }

    pub fn code_block(mut self, style: StyleRefinement) -> Self {
        self.code_block = style;
        self
    }

    pub fn table(mut self, style: StyleRefinement) -> Self {
        self.table = style;
        self
    }

    pub fn table_cell(mut self, style: StyleRefinement) -> Self {
        self.table_cell = style;
        self
    }
}
