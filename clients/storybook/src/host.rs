use ala::{
    AnyView, Context, IntoElement, ParentElement as _, Render, Styled as _, Window, canvas, div,
    px, size,
};
use penna::{ActiveTheme as _, Root};

const PADDING: f32 = 24.0;

pub struct SectionHost {
    section: &'static str,
    content: AnyView,
    floats: bool,
}

impl SectionHost {
    pub fn new(section: &'static str, content: AnyView, floats: bool) -> Self {
        Self {
            section,
            content,
            floats,
        }
    }
}

impl Render for SectionHost {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        penna::note_drawn_roles();
        let section = self.section;
        let dialog_layer = Root::render_dialog_layer(window, cx);
        let notification_layer = Root::render_notification_layer(window, cx);
        let backdrop = self
            .floats
            .then(|| crate::backdrop::backdrop(crate::knobs().backdrop, cx))
            .flatten();
        let theme = cx.theme();
        div()
            .relative()
            .size_full()
            .bg(theme.background)
            .text_color(theme.foreground)
            .font_family(theme.font_family.clone())
            .children(backdrop)
            .child(
                div()
                    .relative()
                    .w_full()
                    .flex_none()
                    .p(px(PADDING))
                    .child(self.content.clone())
                    .child(
                        canvas(
                            |bounds, window, _| {
                                let height = (bounds.size.height * window.zoom()).ceil();
                                let current = window.bounds().size;
                                if (current.height - height).abs() > px(0.5) {
                                    window.resize(size(current.width, height));
                                }
                            },
                            move |bounds, (), window, _| {
                                let drawn = penna::drawn_roles();
                                let size = window.viewport_size();
                                if bounds.size.width > px(0.0)
                                    && size.width > px(0.0)
                                    && size.height > px(0.0)
                                {
                                    crate::note_drawn(section, drawn);
                                }
                            },
                        )
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full(),
                    ),
            )
            .children(dialog_layer)
            .children(notification_layer)
    }
}
