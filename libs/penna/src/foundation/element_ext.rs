use ala::{
    App, Bounds, ClickEvent, ElementId, InteractiveElement, ParentElement, Pixels, SharedString,
    Stateful, StatefulInteractiveElement, Styled as _, Window, canvas,
};

pub trait ElementExt: ParentElement + Sized {
    fn on_prepaint<F>(self, f: F) -> Self
    where
        F: FnOnce(Bounds<Pixels>, &mut Window, &mut App) + 'static,
    {
        self.child(
            canvas(
                move |bounds, window, cx| f(bounds, window, cx),
                |_, _, _, _| {},
            )
            .absolute()
            .size_full(),
        )
    }
}

impl<T: ParentElement> ElementExt for T {}

pub trait InteractiveElementExt: InteractiveElement {
    fn on_double_click(
        mut self,
        listener: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self
    where
        Self: Sized,
    {
        self.interactivity().on_click(move |event, window, cx| {
            if event.click_count() == 2 {
                listener(event, window, cx);
            }
        });
        self
    }
}

impl<E: InteractiveElement> InteractiveElementExt for Stateful<E> {}

pub trait AuthoredElement: InteractiveElement + Sized {
    fn author_id(self, id: impl Into<ElementId>) -> Stateful<Self>
    where
        Stateful<Self>: StatefulInteractiveElement,
    {
        let id = id.into();
        let author = author_name(&id);
        self.id(id).accessibility_id(author)
    }
}

impl<E: InteractiveElement> AuthoredElement for E {}

fn author_name(id: &ElementId) -> SharedString {
    match id {
        ElementId::Name(name) => name.clone(),
        id => id.to_string().into(),
    }
}
