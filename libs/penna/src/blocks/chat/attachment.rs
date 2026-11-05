use std::sync::Arc;

use ala::{
    App, Div, ElementId, Image, ImageSource, IntoElement, ObjectFit, Pixels, Window, div, img,
    prelude::*, px,
};

use crate::{Card, CardLook, Ground, IconName, components::button::Button};

pub const TRANSCRIPT_ATTACHMENT: Pixels = px(140.0);
pub const COMPOSER_ATTACHMENT: Pixels = px(56.0);

pub fn attachment_tile(
    id: impl Into<ElementId>,
    image: Arc<Image>,
    side: Pixels,
    ground: Ground,
    on_open: impl Fn(Arc<Image>, &mut Window, &mut App) + 'static,
) -> Card {
    let preview = Arc::clone(&image);
    Card::new(id, CardLook::Image)
        .on(ground)
        .p_0()
        .size(side)
        .flex_none()
        .overflow_hidden()
        .cursor_pointer()
        .child(
            img(ImageSource::Image(image))
                .size_full()
                .object_fit(ObjectFit::ScaleDown),
        )
        .on_click(move |_, window, cx| on_open(Arc::clone(&preview), window, cx))
}

pub fn composer_attachment(
    tile: impl IntoElement,
    remove: impl Into<ElementId>,
    on_remove: impl Fn(&mut Window, &mut App) + 'static,
) -> Div {
    div().relative().child(tile).child(
        div().absolute().top(px(-6.0)).right(px(-6.0)).child(
            Button::compact_icon(remove, IconName::Xmark)
                .tooltip("Remove this image")
                .on_click(move |_, window, cx| {
                    on_remove(window, cx);
                    cx.stop_propagation();
                }),
        ),
    )
}
