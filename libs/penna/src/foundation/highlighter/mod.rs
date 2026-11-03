#![allow(clippy::pedantic, clippy::style, clippy::complexity)]

#[cfg(feature = "tree-sitter")]
mod highlighter;
#[cfg(feature = "tree-sitter")]
mod languages;
mod palette;
mod registry;
#[cfg(not(feature = "tree-sitter"))]
mod syntax;
mod theme;

#[cfg(feature = "tree-sitter")]
pub use highlighter::SyntaxHighlighter;
#[cfg(feature = "tree-sitter")]
pub use languages::Language;
pub use registry::{LanguageConfig, LanguageRegistry};
pub use ropey::Rope;
#[cfg(not(feature = "tree-sitter"))]
pub use syntax::{InputEdit, Point, SyntaxHighlighter};
pub use theme::{
    FontStyle, FontWeightContent, HighlightTheme, HighlightThemeStyle, SyntaxColors, ThemeStyle,
};
#[cfg(feature = "tree-sitter")]
pub use tree_sitter::{InputEdit, Point};
