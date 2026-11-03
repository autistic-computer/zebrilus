use std::sync::Arc;

use crate::{
    ActiveTheme as _, IconName, Sizable as _,
    components::button::{Button, ButtonVariants as _},
    components::card::{Card, CardLook},
    components::segmented::SegmentedControl,
    components::text::{TextView, TextViewStyle},
    h_flex,
};
use ala::{
    AnyElement, App, ClipboardItem, ElementId, IntoElement, ParentElement as _, RenderOnce,
    SharedString, Styled as _, Window, div, prelude::*, px,
};

#[derive(Clone, Debug)]
pub struct ExampleCode {
    pub label: SharedString,
    pub language: SharedString,
    pub source: SharedString,
}

impl ExampleCode {
    #[must_use]
    pub fn new(
        label: impl Into<SharedString>,
        language: impl Into<SharedString>,
        source: impl Into<SharedString>,
    ) -> Self {
        Self {
            label: label.into(),
            language: language.into(),
            source: source.into(),
        }
    }
}

#[derive(IntoElement)]
pub struct Example {
    id: ElementId,
    preview: Vec<AnyElement>,
    code: Vec<ExampleCode>,
}

impl Example {
    #[must_use]
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            preview: Vec::new(),
            code: Vec::new(),
        }
    }

    #[must_use]
    pub fn code(mut self, code: ExampleCode) -> Self {
        self.code.push(code);
        self
    }
}

impl ParentElement for Example {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.preview.extend(elements);
    }
}

impl RenderOnce for Example {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let tab = window.use_keyed_state((self.id.clone(), "tab"), cx, |_, _| 0_usize);
        let shown = (*tab.read(cx)).min(self.code.len().saturating_sub(1));
        let code = self.code.get(shown).cloned();
        let tabs = (self.code.len() > 1).then(|| {
            let tab = tab.clone();
            SegmentedControl::new(
                (self.id.clone(), "tabs"),
                self.code.iter().map(|code| code.label.clone()),
            )
            .selected(shown)
            .on_select(move |index, _, cx| {
                tab.update(cx, |tab, cx| {
                    *tab = index;
                    cx.notify();
                });
            })
        });
        let style = TextViewStyle {
            highlight_theme: Arc::clone(&cx.theme().highlight_theme),
            is_dark: cx.theme().is_dark(),
            ..TextViewStyle::default()
        };
        Card::new(self.id.clone(), CardLook::Figure)
            .flex()
            .flex_col()
            .w_full()
            .overflow_hidden()
            .child(
                h_flex()
                    .flex_wrap()
                    .justify_center()
                    .gap(px(8.0))
                    .px(px(24.0))
                    .py(px(40.0))
                    .children(self.preview),
            )
            .when_some(tabs, |this, tabs| {
                this.child(h_flex().px(px(8.0)).pb(px(8.0)).child(tabs))
            })
            .when_some(code, |this, code| {
                let source = code.source.clone();
                this.child(
                    div().px(px(8.0)).pb(px(8.0)).child(
                        TextView::markdown(
                            ElementId::Name(format!("{}-code-{shown}", self.id).into()),
                            format!("```{}\n{}\n```", code.language, code.source),
                        )
                        .style(style)
                        .code_block_actions(move |_, _, _| {
                            let source = source.clone();
                            div().absolute().top_1().right_1().child(
                                Button::new("example-copy")
                                    .icon(IconName::Copy)
                                    .ghost()
                                    .small()
                                    .tooltip("Copy")
                                    .on_click(move |_, _, cx| {
                                        cx.write_to_clipboard(ClipboardItem::new_string(
                                            source.to_string(),
                                        ));
                                    }),
                            )
                        }),
                    ),
                )
            })
    }
}
