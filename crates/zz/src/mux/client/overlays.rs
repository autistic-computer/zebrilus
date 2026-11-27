use super::*;

impl MuxClient {
    #[must_use]
    pub(crate) fn command_prompt(&self) -> Option<&CommandPromptState> {
        self.core.command_prompt()
    }

    #[must_use]
    pub(crate) fn command_prompt_revision(&self) -> u64 {
        self.command_prompt_revision
    }

    #[must_use]
    pub(crate) fn choose_tree(&self) -> Option<&ChooseTreeState> {
        self.core.choose_tree()
    }

    #[must_use]
    pub(crate) fn choose_tree_revision(&self) -> u64 {
        self.choose_tree_revision
    }

    #[must_use]
    pub(crate) fn choose_tree_closed_revision(&self) -> u64 {
        self.choose_tree_closed_revision
    }

    #[must_use]
    pub(crate) fn choose_buffer(&self) -> Option<&ChooseBufferState> {
        self.core.choose_buffer()
    }

    #[must_use]
    pub(crate) fn choose_buffer_revision(&self) -> u64 {
        self.choose_buffer_revision
    }

    #[must_use]
    pub(crate) fn display_panes(&self) -> Option<&DisplayPanesState> {
        self.core.display_panes()
    }

    #[must_use]
    pub(crate) fn display_panes_revision(&self) -> u64 {
        self.display_panes_revision
    }

    #[must_use]
    pub(crate) fn menu(&self) -> Option<&MenuState> {
        self.core.menu()
    }

    #[must_use]
    pub(crate) fn menu_revision(&self) -> u64 {
        self.menu_revision
    }

    #[must_use]
    pub(crate) fn confirm(&self) -> Option<&ConfirmState> {
        self.core.confirm()
    }

    #[must_use]
    pub(crate) fn confirm_revision(&self) -> u64 {
        self.confirm_revision
    }
}
