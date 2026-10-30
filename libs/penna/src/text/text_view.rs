#![allow(clippy::pedantic, clippy::style, clippy::complexity)]

use std::sync::Arc;

use ala::prelude::FluentBuilder as _;
use ala::{
    AnyElement, App, Bounds, Element, ElementId, Entity, GlobalElementId, Hitbox, HitboxBehavior,
    InspectorElementId, InteractiveElement, IntoElement, LayoutId, ParentElement, Pixels,
    SharedString, StyleRefinement, Styled, Window, div,
};

use crate::{StyledExt, scroll::ScrollableElement};

use super::{
    CONTEXT, Copy, clipboard_selection_text,
    global::TextGlobal,
    links::{SharedLinks, TextLinks},
    markdown_ext::MarkdownExtensions,
    node::CodeBlock,
    state::TextViewState,
    style::TextViewStyle,
    window_selection,
};

pub(crate) type CodeBlockActionsFn =
    dyn Fn(&CodeBlock, &mut Window, &mut App) -> AnyElement + Send + Sync;
pub(crate) type CodeBlockBodyFn =
    dyn Fn(&CodeBlock, &mut Window, &mut App) -> Option<AnyElement> + Send + Sync;

#[derive(Clone)]
pub struct TextView {
    id: ElementId,
    text: Option<SharedString>,
    pub(crate) state: Option<Entity<TextViewState>>,
    text_view_style: TextViewStyle,
    style: StyleRefinement,
    selectable: bool,
    scrollable: bool,
    streaming: bool,
    code_block_actions: Option<Arc<CodeBlockActionsFn>>,
    code_block_body: Option<Arc<CodeBlockBodyFn>>,
    markdown_extensions: Arc<MarkdownExtensions>,
    links: Option<SharedLinks>,
}

impl Styled for TextView {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl TextView {
    pub fn new(state: &Entity<TextViewState>) -> Self {
        Self {
            id: ElementId::Name(state.entity_id().to_string().into()),
            state: Some(state.clone()),
            text: None,
            text_view_style: TextViewStyle::default(),
            style: StyleRefinement::default(),
            selectable: false,
            scrollable: false,
            streaming: false,
            code_block_actions: None,
            code_block_body: None,
            markdown_extensions: Arc::default(),
            links: None,
        }
    }

    pub fn markdown(id: impl Into<ElementId>, markdown: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            text: Some(markdown.into()),
            text_view_style: TextViewStyle::default(),
            style: StyleRefinement::default(),
            state: None,
            selectable: false,
            scrollable: false,
            streaming: false,
            code_block_actions: None,
            code_block_body: None,
            markdown_extensions: Arc::default(),
            links: None,
        }
    }

    pub fn style(mut self, style: TextViewStyle) -> Self {
        self.text_view_style = style;
        self
    }

    pub fn selectable(mut self, selectable: bool) -> Self {
        self.selectable = selectable;
        self
    }

    pub fn scrollable(mut self, scrollable: bool) -> Self {
        self.scrollable = scrollable;
        self
    }

    pub fn streaming(mut self, streaming: bool) -> Self {
        self.streaming = streaming;
        self
    }

    pub fn code_block_actions<F, E>(mut self, f: F) -> Self
    where
        F: Fn(&CodeBlock, &mut Window, &mut App) -> E + Send + Sync + 'static,
        E: IntoElement,
    {
        self.code_block_actions = Some(Arc::new(move |code_block, window, cx| {
            f(&code_block, window, cx).into_any_element()
        }));
        self
    }

    pub fn code_block_body<F>(mut self, f: F) -> Self
    where
        F: Fn(&CodeBlock, &mut Window, &mut App) -> Option<AnyElement> + Send + Sync + 'static,
    {
        self.code_block_body = Some(Arc::new(f));
        self
    }

    pub fn links(mut self, links: impl TextLinks) -> Self {
        self.links = Some(Arc::new(links));
        self
    }

    pub fn markdown_extensions(mut self, extensions: MarkdownExtensions) -> Self {
        self.markdown_extensions = Arc::new(extensions);
        self
    }
}

impl IntoElement for TextView {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

pub struct TextViewLayoutState {
    state: Entity<TextViewState>,
    element: AnyElement,
}

impl Element for TextView {
    type RequestLayoutState = TextViewLayoutState;
    type PrepaintState = Hitbox;

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let state = if let Some(state) = self.state.clone() {
            state
        } else {
            let default_text = self.text.clone().unwrap_or_default();

            let state = window.use_keyed_state(
                SharedString::from(format!("{}/state", self.id)),
                cx,
                move |_, cx| TextViewState::markdown(default_text.as_str(), cx),
            );
            self.state = Some(state.clone());
            state
        };

        state.update(cx, |state, cx| {
            state.code_block_actions = self.code_block_actions.clone();
            state.code_block_body = self.code_block_body.clone();
            state.links = self.links.clone();
            state.set_markdown_extensions(self.markdown_extensions.clone(), cx);
            state.selectable = self.selectable;
            state.scrollable = self.scrollable;
            state.set_streaming(self.streaming, cx);
            state.text_view_style = self.text_view_style.clone();

            if let Some(text) = self.text.clone() {
                state.set_text(text.as_str(), cx);
            }
        });

        let focus_handle = state.read(cx).focus_handle.clone();
        let list_state = state.read(cx).list_state.clone();

        let mut el = div()
            .key_context(CONTEXT)
            .track_focus(&focus_handle)
            .when(self.scrollable, |this| {
                this.size_full().vertical_scrollbar(&list_state)
            })
            .relative()
            .on_action(move |_: &Copy, window, cx| {
                use crate::WindowExt as _;
                let selected_text = window.selected_text(cx);
                let text = clipboard_selection_text(&selected_text);
                if text.is_empty() {
                    cx.propagate();
                    return;
                }
                cx.write_to_clipboard(ala::ClipboardItem::new_string(text.to_string()));
            })
            .on_action(window.listener_for(&state, TextViewState::on_action_select_all))
            .child(state.clone())
            .refine_style(&self.style)
            .into_any_element();
        let layout_id = el.request_layout(window, cx);
        (layout_id, TextViewLayoutState { state, element: el })
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        request_layout.element.prepaint(window, cx);
        window.insert_hitbox(bounds, HitboxBehavior::Normal)
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        request_layout: &mut Self::RequestLayoutState,
        hitbox: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let state = &request_layout.state;
        if self.selectable {
            window_selection::register_selectable_text_view(state, hitbox, window, cx);
        }

        TextGlobal::push_view(cx, state.clone());
        request_layout.element.paint(window, cx);
        TextGlobal::pop_view(cx);
    }
}

#[cfg(test)]
mod tests {
    use super::{TextView, clipboard_selection_text};
    use crate::text::TextViewState;
    use ala::{
        AppContext as _, Context, Entity, InteractiveElement as _, IntoElement, Modifiers,
        MouseButton, MouseDownEvent, MouseUpEvent, ParentElement as _, Render, Styled as _,
        TestAppContext, VisualTestContext, Window, div, point, px,
    };

    struct TextViewTestRoot {
        text_view: Entity<TextViewState>,
    }

    #[test]
    fn clipboard_selection_preserves_code_whitespace() {
        let selected = "\t  fn main() {\n\t      body();\n\t  }  \n";

        assert_eq!(
            clipboard_selection_text(selected),
            "\t  fn main() {\n\t      body();\n\t  }  "
        );
    }

    #[test]
    fn clipboard_selection_removes_only_one_synthetic_line_feed() {
        assert_eq!(
            clipboard_selection_text("    code  \n\n\n"),
            "    code  \n\n"
        );
        assert_eq!(
            clipboard_selection_text("  no line feed  "),
            "  no line feed  "
        );
        assert_eq!(clipboard_selection_text("\n"), "");
    }

    impl TextViewTestRoot {
        fn new(text: &str, cx: &mut Context<Self>) -> Self {
            let text = text.to_string();
            let text_view = cx.new(|cx| TextViewState::markdown(&text, cx));
            Self { text_view }
        }
    }

    impl Render for TextViewTestRoot {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            div()
                .w(px(160.))
                .child(
                    div()
                        .h(px(24.))
                        .overflow_hidden()
                        .child(TextView::new(&self.text_view).selectable(true)),
                )
                .child(div().h(px(40.)).child("footer"))
        }
    }

    struct InlineImageTextViewTestRoot {
        text_view: Entity<TextViewState>,
    }

    impl InlineImageTextViewTestRoot {
        fn new(text: &str, cx: &mut Context<Self>) -> Self {
            let text_view = cx.new(|cx| TextViewState::markdown(text, cx));
            Self { text_view }
        }
    }

    impl Render for InlineImageTextViewTestRoot {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            div()
                .w(px(420.))
                .child(TextView::new(&self.text_view).selectable(true))
        }
    }

    #[ala::test]
    fn inline_image_keeps_surrounding_text_on_same_line(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let (_, cx) = cx.add_window_view(|window, cx| {
            let content = cx.new(|cx| {
                InlineImageTextViewTestRoot::new(
                    "Build Status ![inline image](https://example.com/image.svg) after",
                    cx,
                )
            });
            crate::Root::new(content, window, cx)
        });
        let cx: &mut VisualTestContext = cx;

        cx.run_until_parked();
        cx.update(|window, cx| {
            let _ = window.draw(cx);
        });

        let inline_bounds = cx.update(|window, cx| {
            crate::Root::read(window, cx)
                .expect("the test window's root is a Root")
                .text_selection
                .inline_bounds()
                .next()
                .cloned()
                .unwrap_or_default()
        });

        assert_eq!(inline_bounds.len(), 2);
        assert_eq!(
            inline_bounds[0].top(),
            inline_bounds[1].top(),
            "text before and after an inline image should share a rendered line"
        );
        assert!(
            inline_bounds[1].left() - inline_bounds[0].right() > px(8.),
            "inline image should reserve horizontal space in the text layout"
        );
        assert!(
            inline_bounds[1].left() - inline_bounds[0].right() < px(40.),
            "unloaded inline image fallback should stay generic and compact"
        );
    }

    #[ala::test]
    fn hovering_plain_glyphs_does_not_redraw(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let (root, cx) = cx.add_window_view(|window, cx| {
            let content = cx.new(|cx| {
                InlineImageTextViewTestRoot::new(
                    "plain words then [a link](https://example.com) end",
                    cx,
                )
            });
            crate::Root::new(content, window, cx)
        });
        let state = root.read_with(cx, |root, cx| {
            root.view()
                .clone()
                .downcast::<InlineImageTextViewTestRoot>()
                .unwrap()
                .read(cx)
                .text_view
                .clone()
        });
        let cx: &mut VisualTestContext = cx;
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));

        let notified = std::rc::Rc::new(std::cell::Cell::new(0));
        let _subscription = cx.update(|_, cx| {
            let notified = notified.clone();
            cx.observe(&state, move |_, _| notified.set(notified.get() + 1))
        });
        for step in 0..12 {
            cx.simulate_mouse_move(
                point(px(2. + step as f32 * 8.), px(8.)),
                None,
                Modifiers::default(),
            );
        }
        assert_eq!(
            notified.get(),
            0,
            "moving over plain glyphs redrew the view"
        );

        cx.simulate_mouse_move(point(px(190.), px(8.)), None, Modifiers::default());
        assert_eq!(notified.get(), 1, "entering a link redraws for its cursor");
    }

    #[ala::test]
    fn inline_image_after_hard_break_starts_the_next_line(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let (_, cx) = cx.add_window_view(|window, cx| {
            let content = cx.new(|cx| {
                InlineImageTextViewTestRoot::new(
                    "Hi  \n![inline image](https://example.com/image.svg) after",
                    cx,
                )
            });
            crate::Root::new(content, window, cx)
        });

        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
        let inline_bounds = cx.update(|window, cx| {
            crate::Root::read(window, cx)
                .expect("the test window's root is a Root")
                .text_selection
                .inline_bounds()
                .next()
                .cloned()
                .unwrap_or_default()
        });

        assert_eq!(inline_bounds.len(), 2);
        assert!(inline_bounds[1].top() >= inline_bounds[0].bottom());
        assert!(inline_bounds[1].left() > inline_bounds[0].left());
    }

    #[ala::test]
    fn list_item_renders_a_nested_code_block_at_full_width(cx: &mut TestAppContext) {
        struct ListItemBlockRoot;

        impl Render for ListItemBlockRoot {
            fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
                div().w(px(400.)).h(px(400.)).child(
                    TextView::markdown(
                        "list-with-code",
                        "1. List item\n   ```rust\n   nested\n   ```\n\n```rust\ntop-level\n```",
                    )
                    .code_block_actions(|code_block, _, _| {
                        let selector = if code_block.code().contains("nested") {
                            "nested-code-action"
                        } else {
                            "top-level-code-action"
                        };
                        div().debug_selector(move || selector.into()).child("Copy")
                    }),
                )
            }
        }

        cx.update(crate::init);
        let (_, cx) = cx.add_window_view(|_, _| ListItemBlockRoot);
        let cx: &mut VisualTestContext = cx;

        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });

        let nested = cx
            .debug_bounds("nested-code-action")
            .expect("nested code block was not rendered at all");
        let top_level = cx.debug_bounds("top-level-code-action").unwrap();
        assert!(
            top_level.right() - nested.right() < px(32.),
            "nested code block should fill the list item's available width, \
             but ended {} short of the top-level one",
            top_level.right() - nested.right()
        );
    }

    #[ala::test]
    fn clipped_markdown_link_does_not_open(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let (_, cx) = cx.add_window_view(|_, cx| {
            TextViewTestRoot::new("visible\n\n[hidden](https://example.com)", cx)
        });
        let cx: &mut VisualTestContext = cx;

        cx.simulate_click(point(px(10.), px(34.)), Modifiers::default());

        assert_eq!(cx.opened_url(), None);
    }

    #[ala::test]
    fn clipped_markdown_cannot_start_selection(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let (view, cx) = cx
            .add_window_view(|_, cx| TextViewTestRoot::new("visible\n\nhidden selection text", cx));
        let cx: &mut VisualTestContext = cx;

        cx.simulate_mouse_down(
            point(px(10.), px(34.)),
            MouseButton::Left,
            Modifiers::default(),
        );
        cx.simulate_mouse_move(
            point(px(90.), px(34.)),
            Some(MouseButton::Left),
            Modifiers::default(),
        );
        cx.simulate_mouse_up(
            point(px(90.), px(34.)),
            MouseButton::Left,
            Modifiers::default(),
        );

        let selected_text = view.read_with(cx, |root, cx| root.text_view.read(cx).selected_text());
        assert!(
            selected_text.is_empty(),
            "unexpected selection: {selected_text:?}"
        );
    }

    struct ClippedTallTextViewTestRoot {
        text_view: Entity<TextViewState>,
    }

    impl Render for ClippedTallTextViewTestRoot {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
                .w(px(200.))
                .child(
                    div()
                        .h(px(40.))
                        .overflow_hidden()
                        .child(TextView::new(&self.text_view).selectable(true)),
                )
                .child(div().h(px(160.)))
        }
    }

    #[ala::test]
    fn selection_band_beyond_clip_copies_offscreen_text(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let (root, cx) = cx.add_window_view(|window, cx| {
            let content = cx.new(|cx| ClippedTallTextViewTestRoot {
                text_view: cx
                    .new(|cx| TextViewState::markdown("alpha\n\nbravo\n\ncharlie\n\ndelta", cx)),
            });
            crate::Root::new(content, window, cx)
        });
        let content = root.read_with(cx, |root, _| {
            root.view()
                .clone()
                .downcast::<ClippedTallTextViewTestRoot>()
                .unwrap()
        });
        let cx: &mut VisualTestContext = cx;
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));

        let end = point(px(180.), px(150.));
        cx.simulate_mouse_down(
            point(px(2.), px(8.)),
            MouseButton::Left,
            Modifiers::default(),
        );
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.simulate_mouse_move(end, Some(MouseButton::Left), Modifiers::default());
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::default());
        cx.update(|window, cx| window.draw(cx).clear(cx));

        let selected_text =
            content.read_with(cx, |root, cx| root.text_view.read(cx).selected_text());
        assert!(
            selected_text.contains("charlie") && selected_text.contains("delta"),
            "clipped-out text was not copied: {selected_text:?}"
        );
    }

    #[ala::test]
    fn double_click_selects_word(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let (view, cx) =
            cx.add_window_view(|_, cx| TextViewTestRoot::new("quick select value", cx));

        let cx: &mut VisualTestContext = cx;
        cx.run_until_parked();
        cx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        let position = point(px(10.), px(16.));
        cx.simulate_event(MouseDownEvent {
            position,
            modifiers: Modifiers::default(),
            button: MouseButton::Left,
            click_count: 2,
            first_mouse: false,
        });
        cx.simulate_event(MouseUpEvent {
            position,
            modifiers: Modifiers::default(),
            button: MouseButton::Left,
            click_count: 2,
        });
        cx.update(|window, cx| {
            let _ = window.draw(cx);
        });

        let selected_text = view.read_with(cx, |root, cx| root.text_view.read(cx).selected_text());
        assert_eq!(selected_text.trim(), "quick");
    }

    #[ala::test]
    fn triple_click_selects_paragraph(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let (view, cx) =
            cx.add_window_view(|_, cx| TextViewTestRoot::new("quick select value", cx));

        let cx: &mut VisualTestContext = cx;
        cx.run_until_parked();
        cx.update(|window, cx| {
            let _ = window.draw(cx);
        });

        let position = point(px(10.), px(10.));
        cx.simulate_event(MouseDownEvent {
            position,
            modifiers: Modifiers::default(),
            button: MouseButton::Left,
            click_count: 3,
            first_mouse: false,
        });
        cx.simulate_event(MouseUpEvent {
            position,
            modifiers: Modifiers::default(),
            button: MouseButton::Left,
            click_count: 3,
        });
        cx.update(|window, cx| {
            let _ = window.draw(cx);
        });

        let selected_text = view.read_with(cx, |root, cx| root.text_view.read(cx).selected_text());
        assert_eq!(selected_text.trim(), "quick select value");
    }

    #[ala::test]
    fn outer_list_content_total_stable_while_scrolling(cx: &mut TestAppContext) {
        use ala::{ListAlignment, ListState, list};

        const ITEMS: &[&str] = &[
            "# Heading\n\nA paragraph long enough to wrap across several lines and produce a non-trivial height.",
            "Short.",
            "Paragraph A\n\nParagraph B\n\nParagraph C with more words to increase the height.",
            "## Subheading\n\n- One\n- Two\n- Three\n\nClosing paragraph.",
            "Only one line.",
            "**Bold**: medium length text with `code` mixed with regular words.",
            "1. First\n2. Second\n3. Third\n\nA short closing paragraph.",
            "A long message with enough words to wrap across multiple lines, create a taller item, and verify that off-screen measurement matches visible measurement.",
        ];
        let n = 40usize;

        struct ListRoot {
            state: ListState,
        }
        impl Render for ListRoot {
            fn render(&mut self, _w: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
                div().w(px(360.)).h(px(500.)).child(
                    list(self.state.clone(), |ix, _w, _cx| {
                        div()
                            .w_full()
                            .child(TextView::markdown(
                                ("md", ix as u64),
                                ITEMS[ix % ITEMS.len()],
                            ))
                            .into_any_element()
                    })
                    .size_full(),
                )
            }
        }

        cx.update(crate::init);
        let state = ListState::new(n, ListAlignment::Top, px(2048.)).measure_all();
        let probe = state.clone();
        let (_view, cx) = cx.add_window_view(|_w, _cx| ListRoot { state });
        let cx: &mut VisualTestContext = cx;

        cx.run_until_parked();
        cx.update(|w, cx| {
            let _ = w.draw(cx);
        });
        cx.run_until_parked();
        cx.update(|w, cx| {
            let _ = w.draw(cx);
        });

        let total = |p: &ListState| {
            f32::from(p.max_offset_for_scrollbar().y + p.viewport_bounds().size.height)
        };
        let mut totals = vec![total(&probe)];
        for _ in 0..20 {
            probe.scroll_by(px(150.));
            cx.update(|w, cx| {
                let _ = w.draw(cx);
            });
            cx.run_until_parked();
            totals.push(total(&probe));
        }
        let min = totals.iter().cloned().fold(f32::INFINITY, f32::min);
        let max = totals.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        assert!(
            (max - min) < 2.0,
            "list content total jittered while scrolling: min={min} max={max} totals={totals:?}"
        );
    }
}
