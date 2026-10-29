use super::{Selection, history::HistoryItem};

#[derive(Debug, PartialEq, Clone)]
pub struct Change {
    pub(crate) old_range: Selection,
    pub(crate) old_text: String,
    pub(crate) new_range: Selection,
    pub(crate) new_text: String,
    version: usize,
}

impl Change {
    pub fn new(
        old_range: impl Into<Selection>,
        old_text: &str,
        new_range: impl Into<Selection>,
        new_text: &str,
    ) -> Self {
        Self {
            old_range: old_range.into(),
            old_text: old_text.to_string(),
            new_range: new_range.into(),
            new_text: new_text.to_string(),
            version: 0,
        }
    }
}

impl HistoryItem for Change {
    fn version(&self) -> usize {
        self.version
    }

    fn set_version(&mut self, version: usize) {
        self.version = version;
    }

    fn merge(&mut self, next: &Self) -> bool {
        if !next.old_range.is_empty()
            || !next.old_text.is_empty()
            || next.old_range.start != self.new_range.end
        {
            return false;
        }
        self.new_text.push_str(&next.new_text);
        self.new_range.end = next.new_range.end;
        true
    }
}
