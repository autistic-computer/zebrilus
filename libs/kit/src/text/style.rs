#![allow(clippy::pedantic, clippy::style, clippy::complexity)]

use std::sync::Arc;

use zz_gpui::{Pixels, Rems, StyleRefinement, px, rems};

use crate::{SurfaceRole, highlighter::HighlightTheme};

#[derive(Clone)]
pub struct TextViewStyle {
    pub paragraph_gap: Rems,
    pub heading_base_font_size: Pixels,
    pub heading_font_size: Option<Arc<dyn Fn(u8, Pixels) -> Pixels + Send + Sync + 'static>>,
    pub highlight_theme: Arc<HighlightTheme>,
    pub code_block: StyleRefinement,
    pub table: StyleRefinement,
    pub table_cell: StyleRefinement,
    pub is_dark: bool,
    pub code_role: Option<SurfaceRole>,
    pub figure_role: Option<SurfaceRole>,
}

impl PartialEq for TextViewStyle {
    fn eq(&self, other: &Self) -> bool {
        self.paragraph_gap == other.paragraph_gap
            && self.heading_base_font_size == other.heading_base_font_size
            && self.highlight_theme == other.highlight_theme
            && self.code_role == other.code_role
            && self.figure_role == other.figure_role
    }
}

impl Default for TextViewStyle {
    fn default() -> Self {
        Self {
            paragraph_gap: rems(1.),
            heading_base_font_size: px(14.),
            heading_font_size: None,
            highlight_theme: HighlightTheme::default_light().clone(),
            code_block: StyleRefinement::default(),
            table: StyleRefinement::default(),
            table_cell: StyleRefinement::default(),
            is_dark: false,
            code_role: None,
            figure_role: None,
        }
    }
}

impl TextViewStyle {
    pub fn paragraph_gap(mut self, gap: Rems) -> Self {
        self.paragraph_gap = gap;
        self
    }

    pub fn heading_font_size<F>(mut self, f: F) -> Self
    where
        F: Fn(u8, Pixels) -> Pixels + Send + Sync + 'static,
    {
        self.heading_font_size = Some(Arc::new(f));
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

    pub fn code_role(mut self, role: SurfaceRole) -> Self {
        self.code_role = Some(role);
        self
    }

    pub fn figure_role(mut self, role: SurfaceRole) -> Self {
        self.figure_role = Some(role);
        self
    }
}
