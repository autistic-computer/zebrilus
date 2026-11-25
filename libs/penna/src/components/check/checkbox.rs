use crate::StyledExt as _;
use std::rc::Rc;

use ala::{
    App, ClickEvent, ElementId, InteractiveElement as _, IntoElement, MouseButton,
    ParentElement as _, RenderOnce, Role, SharedString, StatefulInteractiveElement as _, Window,
    accesskit::Toggled, prelude::FluentBuilder as _,
};

use super::{Check, GROUP, hold, labeled, touch_slop};
use crate::{
    AuthoredElement as _, Disableable, Sizable, Size, Tone,
    blocks::touch::{CoarsePointer, hit_area},
};

type ClickHandler = Rc<dyn Fn(&bool, &mut Window, &mut App)>;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CheckState {
    #[default]
    Unchecked,
    Checked,
    Mixed,
}

impl CheckState {
    #[must_use]
    pub const fn from_counts(checked: usize, total: usize) -> Self {
        if total == 0 || checked == 0 {
            Self::Unchecked
        } else if checked >= total {
            Self::Checked
        } else {
            Self::Mixed
        }
    }

    #[must_use]
    pub const fn is_checked(self) -> bool {
        matches!(self, Self::Checked)
    }

    #[must_use]
    pub const fn is_mixed(self) -> bool {
        matches!(self, Self::Mixed)
    }
}

#[derive(IntoElement)]
pub struct Checkbox {
    id: ElementId,
    label: Option<SharedString>,
    checked: bool,
    mixed: bool,
    tone: Option<Tone>,
    disabled: bool,
    size: Size,
    on_click: Option<ClickHandler>,
}

impl Checkbox {
    #[must_use]
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            label: None,
            checked: false,
            mixed: false,
            tone: None,
            disabled: false,
            size: Size::default(),
            on_click: None,
        }
    }

    #[must_use]
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    #[must_use]
    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }

    #[must_use]
    pub fn indeterminate(mut self, mixed: bool) -> Self {
        self.mixed = mixed;
        self
    }

    #[must_use]
    pub const fn state(mut self, state: CheckState) -> Self {
        self.checked = state.is_checked();
        self.mixed = state.is_mixed();
        self
    }

    #[must_use]
    pub fn tone(mut self, tone: Tone) -> Self {
        self.tone = Some(tone);
        self
    }

    #[must_use]
    pub fn on_click(mut self, handler: impl Fn(&bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Rc::new(handler));
        self
    }
}

impl Disableable for Checkbox {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl Sizable for Checkbox {
    fn with_size(mut self, size: impl Into<Size>) -> Self {
        self.size = size.into();
        self
    }
}

impl RenderOnce for Checkbox {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let coarse = CoarsePointer::get(cx);
        let focus = window
            .use_keyed_state(self.id.clone(), cx, |_, cx| cx.focus_handle())
            .read(cx)
            .clone();
        let on_click = self.on_click.filter(|_| !self.disabled);
        let focused = on_click.is_some() && focus.is_focused(window);
        let toggled = if self.mixed {
            Toggled::Mixed
        } else if self.checked {
            Toggled::True
        } else {
            Toggled::False
        };
        let next = self.mixed || !self.checked;
        let mark = Check {
            mixed: self.mixed,
            tone: self.tone,
            disabled: self.disabled,
            size: self.size,
            live: on_click.is_some() && !coarse,
            focused,
            ..Check::checkbox(self.checked)
        };
        let (slop_x, slop_y) = touch_slop(self.size, window);
        labeled(mark, self.label.clone(), window, cx)
            .group(GROUP)
            .author_id(self.id)
            .role(Role::CheckBox)
            .aria_toggled(toggled)
            .aria_disabled(self.disabled)
            .when_some(self.label, ala::StatefulInteractiveElement::aria_label)
            .when_some(on_click, |this, on_click| {
                let click = Rc::new(move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
                    cx.stop_propagation();
                    on_click(&next, window, cx);
                });
                let tap = Rc::clone(&click);
                this.track_focus(&focus.tab_stop(true))
                    .click_cursor()
                    .on_mouse_down(MouseButton::Left, hold)
                    .on_click(move |event, window, cx| click(event, window, cx))
                    .when(coarse, |this| {
                        this.child(
                            hit_area("touch", slop_x, slop_y)
                                .on_mouse_down(MouseButton::Left, hold)
                                .on_click(move |event, window, cx| tap(event, window, cx)),
                        )
                    })
            })
    }
}

#[cfg(test)]
mod tests {
    use super::CheckState;

    #[test]
    fn check_state_derives_tri_state_from_selection_counts() {
        assert_eq!(CheckState::from_counts(0, 0), CheckState::Unchecked);
        assert_eq!(CheckState::from_counts(0, 5), CheckState::Unchecked);
        assert_eq!(CheckState::from_counts(2, 5), CheckState::Mixed);
        assert_eq!(CheckState::from_counts(5, 5), CheckState::Checked);
        assert_eq!(CheckState::from_counts(7, 5), CheckState::Checked);
        assert!(CheckState::Checked.is_checked());
        assert!(CheckState::Mixed.is_mixed());
        assert!(!CheckState::Mixed.is_checked());
    }
}
