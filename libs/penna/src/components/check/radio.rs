use std::rc::Rc;

use ala::{
    App, ClickEvent, ElementId, InteractiveElement as _, IntoElement, MouseButton,
    ParentElement as _, RenderOnce, Role, SharedString, StatefulInteractiveElement as _,
    Styled as _, Window, accesskit::Toggled, div, prelude::FluentBuilder as _,
};

use super::{Check, GROUP, hold, labeled, touch_slop};
use crate::{
    AuthoredElement as _, Disableable, Sizable, Size, Space, Tone,
    blocks::touch::{CoarsePointer, hit_area},
    foundation::author_name,
};

type SelectHandler = Rc<dyn Fn(usize, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct RadioGroup {
    id: ElementId,
    labels: Vec<SharedString>,
    selected: Option<usize>,
    on_select: Option<SelectHandler>,
    horizontal: bool,
    tone: Option<Tone>,
    disabled: bool,
    size: Size,
}

impl RadioGroup {
    #[must_use]
    pub fn new(
        id: impl Into<ElementId>,
        labels: impl IntoIterator<Item = impl Into<SharedString>>,
    ) -> Self {
        Self {
            id: id.into(),
            labels: labels.into_iter().map(Into::into).collect(),
            selected: None,
            on_select: None,
            horizontal: false,
            tone: None,
            disabled: false,
            size: Size::default(),
        }
    }

    #[must_use]
    pub fn selected(mut self, index: impl Into<Option<usize>>) -> Self {
        self.selected = index.into();
        self
    }

    #[must_use]
    pub fn on_select(mut self, handler: impl Fn(usize, &mut Window, &mut App) + 'static) -> Self {
        self.on_select = Some(Rc::new(handler));
        self
    }

    #[must_use]
    pub fn horizontal(mut self) -> Self {
        self.horizontal = true;
        self
    }

    #[must_use]
    pub fn tone(mut self, tone: Tone) -> Self {
        self.tone = Some(tone);
        self
    }
}

impl Disableable for RadioGroup {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl Sizable for RadioGroup {
    fn with_size(mut self, size: impl Into<Size>) -> Self {
        self.size = size.into();
        self
    }
}

impl RenderOnce for RadioGroup {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let coarse = CoarsePointer::get(cx);
        let focus = window
            .use_keyed_state(self.id.clone(), cx, |_, cx| cx.focus_handle())
            .read(cx)
            .clone();
        let Some(last) = self.labels.len().checked_sub(1) else {
            return div().author_id(self.id).role(Role::RadioGroup);
        };
        let on_select = self.on_select.filter(|_| !self.disabled);
        let focused = on_select.is_some() && focus.is_focused(window);
        let selected = self.selected.filter(|index| *index <= last);
        let ringed = selected.unwrap_or(0);
        let (slop_x, slop_y) = touch_slop(self.size, window);
        let author = author_name(&self.id);
        let radios: Vec<_> = self
            .labels
            .into_iter()
            .enumerate()
            .map(|(index, label)| {
                let picked = selected == Some(index);
                let mark = Check {
                    tone: self.tone,
                    disabled: self.disabled,
                    size: self.size,
                    live: on_select.is_some() && !coarse,
                    focused: focused && index == ringed,
                    ..Check::radio(picked)
                };
                let pick = on_select.clone().map(|on_select| {
                    Rc::new(move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
                        cx.stop_propagation();
                        if !picked {
                            on_select(index, window, cx);
                        }
                    })
                });
                labeled(mark, Some(label.clone()), window, cx)
                    .group(GROUP)
                    .author_id(ElementId::NamedInteger(author.clone(), index as u64))
                    .role(Role::RadioButton)
                    .aria_toggled(if picked {
                        Toggled::True
                    } else {
                        Toggled::False
                    })
                    .aria_label(label)
                    .aria_disabled(self.disabled)
                    .when_some(pick, |this, pick| {
                        let tap = Rc::clone(&pick);
                        this.on_click(move |event, window, cx| pick(event, window, cx))
                            .when(coarse, |this| {
                                this.child(
                                    hit_area("touch", slop_x, slop_y)
                                        .on_click(move |event, window, cx| tap(event, window, cx)),
                                )
                            })
                    })
            })
            .collect();
        div()
            .author_id(self.id)
            .flex()
            .map(|group| {
                if self.horizontal {
                    group.flex_row().flex_wrap().gap(Space::GAP_GROUP)
                } else {
                    group.flex_col().items_start().gap(Space::GAP_RELATED)
                }
            })
            .role(Role::RadioGroup)
            .aria_disabled(self.disabled)
            .when_some(on_select, |this, on_select| {
                this.track_focus(&focus.tab_stop(true))
                    .on_mouse_down(MouseButton::Left, hold)
                    .on_key_down(move |event, window, cx| {
                        if event.keystroke.modifiers.modified() {
                            return;
                        }
                        let next = match event.keystroke.key.as_str() {
                            "up" | "left" => ringed.saturating_sub(1),
                            "down" | "right" => (ringed + 1).min(last),
                            "home" => 0,
                            "end" => last,
                            "space" | "enter" if selected.is_none() => ringed,
                            _ => return,
                        };
                        cx.stop_propagation();
                        if selected != Some(next) {
                            on_select(next, window, cx);
                        }
                    })
            })
            .children(radios)
    }
}
