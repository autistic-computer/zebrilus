use std::rc::Rc;

use crate::{
    blocks::sidebar::{sidebar_heading, sidebar_row},
    v_flex,
};
use ala::{
    App, ElementId, IntoElement, ParentElement as _, RenderOnce, SharedString, Styled as _, Window,
    prelude::*, px,
};

type Pick = Rc<dyn Fn(usize, &mut Window, &mut App)>;

#[derive(Clone, Debug)]
pub struct NavGroup {
    pub title: SharedString,
    pub entries: Vec<SharedString>,
}

impl NavGroup {
    #[must_use]
    pub fn new(
        title: impl Into<SharedString>,
        entries: impl IntoIterator<Item = impl Into<SharedString>>,
    ) -> Self {
        Self {
            title: title.into(),
            entries: entries.into_iter().map(Into::into).collect(),
        }
    }
}

#[derive(IntoElement)]
pub struct DocsNav {
    id: ElementId,
    groups: Vec<NavGroup>,
    selected: Option<usize>,
    on_pick: Option<Pick>,
}

impl DocsNav {
    #[must_use]
    pub fn new(id: impl Into<ElementId>, groups: impl IntoIterator<Item = NavGroup>) -> Self {
        Self {
            id: id.into(),
            groups: groups.into_iter().collect(),
            selected: None,
            on_pick: None,
        }
    }

    #[must_use]
    pub const fn selected(mut self, entry: Option<usize>) -> Self {
        self.selected = entry;
        self
    }

    #[must_use]
    pub fn on_pick(mut self, on_pick: impl Fn(usize, &mut Window, &mut App) + 'static) -> Self {
        self.on_pick = Some(Rc::new(on_pick));
        self
    }
}

impl RenderOnce for DocsNav {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let mut index = 0;
        let mut rows = Vec::new();
        for group in self.groups {
            rows.push(sidebar_heading(group.title, cx).into_any_element());
            for entry in group.entries {
                let on_pick = self.on_pick.clone();
                let entry_index = index;
                rows.push(
                    sidebar_row(
                        ("docs-nav-entry", index),
                        entry,
                        self.selected == Some(index),
                        cx,
                    )
                    .on_click(move |_, window, cx| {
                        if let Some(on_pick) = &on_pick {
                            on_pick(entry_index, window, cx);
                        }
                    })
                    .into_any_element(),
                );
                index += 1;
            }
        }
        v_flex()
            .id(self.id)
            .w_full()
            .gap(px(2.0))
            .px(px(8.0))
            .pt(px(4.0))
            .pb(px(24.0))
            .children(rows)
    }
}
