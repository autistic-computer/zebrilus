use std::rc::Rc;

use ala::{
    AnyElement, App, ElementId, InteractiveElement as _, IntoElement, ParentElement, RenderOnce,
    Role, SharedString, StatefulInteractiveElement as _, StyleRefinement, Styled, Window, div,
    prelude::FluentBuilder as _, px,
};

use crate::{
    ActiveTheme as _, AuthoredElement as _, Colorize as _, Space, StyledExt as _, TextStyle,
    components::icon::cap_center_drop, foundation::motion::Disclosure as DisclosureChevron, h_flex,
    v_flex,
};

type ToggleHandler = Rc<dyn Fn(bool, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct Collapsible {
    id: ElementId,
    title: SharedString,
    meta: Option<SharedString>,
    open: Option<bool>,
    default_open: bool,
    on_toggle: Option<ToggleHandler>,
    style: StyleRefinement,
    children: Vec<AnyElement>,
}

impl Collapsible {
    #[must_use]
    pub fn new(id: impl Into<ElementId>, title: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            meta: None,
            open: None,
            default_open: false,
            on_toggle: None,
            style: StyleRefinement::default(),
            children: Vec::new(),
        }
    }

    #[must_use]
    pub fn meta(mut self, meta: impl Into<SharedString>) -> Self {
        self.meta = Some(meta.into());
        self
    }

    #[must_use]
    pub const fn open(mut self, open: bool) -> Self {
        self.open = Some(open);
        self
    }

    #[must_use]
    pub const fn default_open(mut self, open: bool) -> Self {
        self.default_open = open;
        self
    }

    #[must_use]
    pub fn on_toggle(mut self, handler: impl Fn(bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_toggle = Some(Rc::new(handler));
        self
    }
}

impl ParentElement for Collapsible {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl Styled for Collapsible {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Collapsible {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let default_open = self.default_open;
        let state = window.use_keyed_state((self.id.clone(), "open"), cx, |_, _| default_open);
        let is_open = self.open.unwrap_or_else(|| *state.read(cx));
        let controlled = self.open.is_some();
        let on_toggle = self.on_toggle;

        let theme = cx.theme();
        let fg = theme.foreground;
        let muted = theme.foreground.muted();
        let font_px = TextStyle::Small.design_px();
        let icon_px = 12.0;
        let drop = cap_center_drop(px(font_px), px(icon_px), cx);

        let header = h_flex()
            .author_id((self.id.clone(), "trigger"))
            .role(Role::Button)
            .aria_label(self.title.clone())
            .aria_expanded(is_open)
            .w_full()
            .min_w_0()
            .h(px(28.0))
            .px(Space::S6)
            .gap(Space::S6)
            .items_center()
            .rounded(theme.radius)
            .cursor_pointer()
            .text_color(if is_open { fg } else { muted })
            .hover(move |style| style.text_color(fg))
            .on_click(move |_, window, cx| {
                let next = !is_open;
                if !controlled {
                    state.update(cx, |open, cx| {
                        *open = next;
                        cx.notify();
                    });
                }
                if let Some(handler) = &on_toggle {
                    handler(next, window, cx);
                }
            })
            .child(
                div()
                    .flex_none()
                    .relative()
                    .top(drop)
                    .child(DisclosureChevron::new(
                        (self.id.clone(), "chevron"),
                        is_open,
                        px(icon_px),
                    )),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .type_style_strong(TextStyle::Small, cx)
                    .child(self.title),
            )
            .when_some(self.meta, |row, meta| {
                row.child(
                    div()
                        .flex_none()
                        .font_family(theme.mono_font_family.clone())
                        .type_style(TextStyle::Caption, cx)
                        .text_color(muted)
                        .child(meta),
                )
            });

        v_flex()
            .w_full()
            .min_w_0()
            .gap(Space::S4)
            .refine_style(&self.style)
            .child(header)
            .when(is_open, |container| {
                container.child(
                    v_flex()
                        .w_full()
                        .min_w_0()
                        .pl(Space::S24)
                        .pr(Space::S6)
                        .pb(Space::S6)
                        .gap(Space::S6)
                        .children(self.children),
                )
            })
    }
}
