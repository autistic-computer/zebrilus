mod display_map;
mod fold_map;
#[cfg(feature = "tree-sitter")]
mod folding;
#[cfg(not(feature = "tree-sitter"))]
pub mod folding;
mod text_wrapper;
mod wrap_map;

pub use self::display_map::DisplayMap;
pub(crate) use self::text_wrapper::LineLayout;

#[cfg(not(feature = "tree-sitter"))]
#[allow(unused_imports)]
pub use folding::Tree;
#[allow(unused_imports)]
pub use folding::{FoldRange, extract_fold_ranges};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BufferPoint {
    pub line: usize,
    pub col: usize,
}

impl BufferPoint {
    pub fn new(line: usize, col: usize) -> Self {
        Self { line, col }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct WrapPoint {
    pub row: usize,
    pub col: usize,
}

impl WrapPoint {
    pub fn new(row: usize, col: usize) -> Self {
        Self { row, col }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DisplayPoint {
    pub row: usize,
    pub col: usize,
}

impl DisplayPoint {
    pub fn new(row: usize, col: usize) -> Self {
        Self { row, col }
    }
}
