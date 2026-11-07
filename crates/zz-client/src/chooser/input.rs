use zz_terminal::Modifiers;

use super::chooser_key_is_text;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ChooserSync {
    pub changed: bool,
    pub selection_changed: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ChooserInput {
    revision: u64,
    selected: Option<u32>,
    search_active: bool,
    marked_text: Option<String>,
}

impl ChooserInput {
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub const fn selected(&self) -> Option<u32> {
        self.selected
    }

    #[must_use]
    pub const fn search_active(&self) -> bool {
        self.search_active
    }

    #[must_use]
    pub fn marked_text(&self) -> Option<&str> {
        self.marked_text.as_deref()
    }

    pub fn synchronize(
        &mut self,
        revision: u64,
        selected: u32,
        search_active: bool,
    ) -> ChooserSync {
        if self.revision == revision {
            return ChooserSync::default();
        }
        self.revision = revision;
        self.search_active = search_active;
        let selection_changed = self.selected != Some(selected);
        self.selected = Some(selected);
        if !search_active {
            self.marked_text = None;
        }
        ChooserSync {
            changed: true,
            selection_changed,
        }
    }

    #[must_use]
    pub const fn key_is_text(&self, character: bool, modifiers: Modifiers) -> bool {
        chooser_key_is_text(self.search_active, character, modifiers)
    }

    pub fn unmark(&mut self) -> bool {
        self.marked_text.take().is_some()
    }

    pub fn replace_text<'a>(&mut self, text: &'a str) -> Option<&'a str> {
        self.marked_text = None;
        (!text.is_empty() && self.search_active).then_some(text)
    }

    pub fn mark_text(&mut self, text: &str) {
        self.marked_text = (!text.is_empty()).then(|| text.to_owned());
    }
}
