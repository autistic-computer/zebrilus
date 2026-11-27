use super::*;

impl ChatHooks<'_> {
    pub(super) fn refs(&self) -> Option<&ChatRefs> {
        self.markdown.map(|markdown| &*markdown.refs)
    }

    pub(super) fn clickable<E: StatefulInteractiveElement + Styled>(
        &self,
        element: E,
        target: ChatClick,
    ) -> E {
        match self.click.clone() {
            Some(click) => element
                .click_cursor()
                .on_click(move |_, window, cx| click(&target, window, cx)),
            None => element,
        }
    }

    pub(super) fn hold(&self, held: HeldMessage) -> Option<AnyElement> {
        let click = self.click.clone().filter(|_| self.touch)?;
        Some(
            penna::blocks::compact::hold_listener(Rc::new(move |window, cx| {
                click(&ChatClick::Hold(held.clone()), window, cx);
            }))
            .into_any_element(),
        )
    }

    pub(super) fn fork_from(&self, id: &ItemId) -> Option<(ChatClicks, ItemId)> {
        self.click
            .clone()
            .filter(|_| self.fork)
            .filter(|_| zz_client::agent_settings::source_message(id.as_str()).is_some())
            .map(|click| (click, id.clone()))
    }
}

impl ChatFrame {
    pub(super) fn hooks(&self) -> ChatHooks<'_> {
        ChatHooks {
            click: self.click.clone(),
            fork: self.fork,
            markdown: self.markdown.as_deref(),
            vendor: self.vendor,
            owner: self.owner,
            ground: self.ground,
            touch: self.touch,
            children: self.children.clone(),
        }
    }
}
