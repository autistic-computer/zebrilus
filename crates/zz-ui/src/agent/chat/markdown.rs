use super::*;

fn markdown_extensions(
    refs: &Arc<ChatRefs>,
    html: Option<crate::agent::HtmlBlocks>,
    ui: Option<UiPlugin>,
) -> crate::text::MarkdownExtensions {
    let links = Arc::clone(refs);
    crate::agent::chat_markdown_extensions(refs.view.clone(), html, ui)
        .inline_links(move |text, source| ref_links(&links, text, source))
}

impl ChatMarkdown {
    #[must_use]
    pub fn new(refs: ChatRefs) -> Self {
        Self::showing(refs, None, None)
    }

    pub(super) fn showing(
        refs: ChatRefs,
        html: Option<crate::agent::HtmlBlocks>,
        ui: Option<UiBlocks>,
    ) -> Self {
        let refs = Arc::new(refs);
        Self {
            extensions: markdown_extensions(&refs, html, None),
            html,
            ui,
            refs,
            blocks: RefCell::default(),
        }
    }

    pub(super) fn reads(
        &self,
        refs: &ChatRefs,
        html: Option<crate::agent::HtmlBlocks>,
        ui: Option<UiBlocks>,
    ) -> bool {
        self.refs.hosts == refs.hosts
            && self.refs.cwd == refs.cwd
            && self.html == html
            && self.ui == ui
    }

    pub(super) fn show(&self, looks: &HashMap<ItemId, UiLook>) {
        let Some(drawn) = self.ui else {
            return;
        };
        let mut blocks = self.blocks.borrow_mut();
        blocks.retain(|id, _| looks.contains_key(id));
        for (id, look) in looks {
            if blocks.get(id).is_some_and(|(shown, _)| shown == look) {
                continue;
            }
            let plugin = UiPlugin {
                blocks: drawn,
                look: look.clone(),
            };
            blocks.insert(
                id.clone(),
                (
                    look.clone(),
                    markdown_extensions(&self.refs, self.html, Some(plugin)),
                ),
            );
        }
    }

    pub(super) fn extensions(&self, id: &ItemId) -> crate::text::MarkdownExtensions {
        self.blocks.borrow().get(id).map_or_else(
            || self.extensions.clone(),
            |(_, extensions)| extensions.clone(),
        )
    }
}
