use std::ops::Range;

#[cfg(feature = "tree-sitter")]
use tree_sitter::Node;
#[cfg(feature = "tree-sitter")]
pub use tree_sitter::Tree;

#[cfg(not(feature = "tree-sitter"))]
pub struct Tree;

#[cfg(feature = "tree-sitter")]
const MIN_FOLD_LINES: usize = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FoldRange {
    pub start_line: usize,
    pub end_line: usize,
}

impl FoldRange {
    pub fn new(start_line: usize, end_line: usize) -> Self {
        assert!(
            start_line <= end_line,
            "fold start_line must be <= end_line"
        );
        Self {
            start_line,
            end_line,
        }
    }
}

#[cfg(feature = "tree-sitter")]
fn is_foldable_node(node: &Node) -> bool {
    let start = node.start_position().row;
    let end = node.end_position().row;
    end.saturating_sub(start) >= MIN_FOLD_LINES
}

#[cfg(feature = "tree-sitter")]
pub fn extract_fold_ranges(tree: &Tree) -> Vec<FoldRange> {
    let mut ranges = Vec::new();
    let root = tree.root_node();
    let mut cursor = root.walk();
    for child in root.named_children(&mut cursor) {
        collect_foldable_nodes(child, &mut ranges);
    }

    ranges.sort_by_key(|r| r.start_line);
    ranges.dedup_by_key(|r| r.start_line);
    ranges
}

#[cfg(feature = "tree-sitter")]
pub fn extract_fold_ranges_in_range(tree: &Tree, byte_range: Range<usize>) -> Vec<FoldRange> {
    let mut ranges = Vec::new();
    let root = tree.root_node();
    let mut cursor = root.walk();
    for child in root.named_children(&mut cursor) {
        collect_foldable_nodes_in_range(child, &byte_range, &mut ranges);
    }

    ranges.sort_by_key(|r| r.start_line);
    ranges.dedup_by_key(|r| r.start_line);
    ranges
}

#[cfg(feature = "tree-sitter")]
fn collect_foldable_nodes_in_range(
    node: Node,
    byte_range: &Range<usize>,
    ranges: &mut Vec<FoldRange>,
) {
    if node.end_byte() <= byte_range.start || node.start_byte() >= byte_range.end {
        return;
    }

    if !is_foldable_node(&node) {
        return;
    }

    ranges.push(FoldRange {
        start_line: node.start_position().row,
        end_line: node.end_position().row,
    });

    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect_foldable_nodes_in_range(child, byte_range, ranges);
    }
}

#[cfg(feature = "tree-sitter")]
fn collect_foldable_nodes(node: Node, ranges: &mut Vec<FoldRange>) {
    if !is_foldable_node(&node) {
        return;
    }

    ranges.push(FoldRange {
        start_line: node.start_position().row,
        end_line: node.end_position().row,
    });

    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect_foldable_nodes(child, ranges);
    }
}

#[cfg(not(feature = "tree-sitter"))]
pub fn extract_fold_ranges(_tree: &Tree) -> Vec<FoldRange> {
    Vec::new()
}

#[cfg(not(feature = "tree-sitter"))]
pub fn extract_fold_ranges_in_range(_tree: &Tree, _byte_range: Range<usize>) -> Vec<FoldRange> {
    Vec::new()
}
