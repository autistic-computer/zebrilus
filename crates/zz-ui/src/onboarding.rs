use ala::{AnyElement, App, Div, SharedString, div, prelude::*, px};
use penna::{
    Label, TextStyle,
    blocks::touch::{PageDot, page_dots},
};

use crate::{
    h_flex,
    sky::{SkyZones, sky},
    v_flex,
};

pub const STEPS: [Step; 3] = [Step::Welcome, Step::Import, Step::Agents];

const SIDE: f32 = 40.0;
const BOTTOM: f32 = 28.0;
const BLOCK_GAP: f32 = 18.0;
const HEAD_GAP: f32 = 8.0;
const BAR_GAP: f32 = 10.0;
const ROWS_WIDTH: f32 = 320.0;
const ROW_HEIGHT: f32 = 34.0;
const ROW_GAP: f32 = 4.0;
const CONTROL_GAP: f32 = 12.0;
const DETAIL_GAP: f32 = 8.0;
const BUTTON_GAP: f32 = 8.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Welcome,
    Import,
    Agents,
}

impl Step {
    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::Welcome => "Zebrilus is simply tmux in a graphical interface.",
            Self::Import => "Bring your configs.",
            Self::Agents => "Not here to fight your harness.",
        }
    }

    #[must_use]
    pub const fn description(self) -> &'static str {
        match self {
            Self::Welcome => "Faster, modern, and not limited by a pty",
            Self::Import => "In fact, you might want to 'alias tmux=zz' too",
            Self::Agents => "Nothing to import, just install a hook.",
        }
    }

    #[must_use]
    pub const fn next(self) -> Option<Self> {
        match self {
            Self::Welcome => Some(Self::Import),
            Self::Import => Some(Self::Agents),
            Self::Agents => None,
        }
    }
}

#[must_use]
pub fn onboarding_row(
    name: impl Into<SharedString>,
    detail: Option<SharedString>,
    control: Option<AnyElement>,
) -> Div {
    h_flex()
        .flex_none()
        .w_full()
        .h(px(ROW_HEIGHT))
        .gap(px(CONTROL_GAP))
        .child(
            h_flex()
                .flex_1()
                .min_w_0()
                .items_baseline()
                .gap(px(DETAIL_GAP))
                .child(Label::new(TextStyle::Lead, name).flex_none())
                .children(
                    detail.map(|detail| Label::new(TextStyle::Small, detail).muted().truncate()),
                ),
        )
        .children(control)
}

#[must_use]
pub fn onboarding_page(step: Step, rows: Vec<Div>, buttons: Vec<AnyElement>, cx: &App) -> Div {
    let zones = SkyZones::default();
    let dots = page_dots(
        &[STEPS
            .map(|at| PageDot {
                active: at == step,
                attention: false,
            })
            .to_vec()],
        cx,
    );
    let head = v_flex()
        .flex_none()
        .items_start()
        .gap(px(HEAD_GAP))
        .child(Label::new(TextStyle::Display, step.title()).strong())
        .child(Label::new(TextStyle::Lead, step.description()).muted());
    let bar = h_flex()
        .flex_none()
        .w_full()
        .mt(px(BAR_GAP))
        .justify_between()
        .child(dots)
        .child(h_flex().flex_none().gap(px(BUTTON_GAP)).children(buttons));
    div()
        .relative()
        .size_full()
        .child(sky("onboarding-sky", &zones))
        .child(
            v_flex()
                .relative()
                .size_full()
                .justify_end()
                .gap(px(BLOCK_GAP))
                .px(px(SIDE))
                .pb(px(BOTTOM))
                .child(zones.mark(head))
                .when(!rows.is_empty(), |page| {
                    page.child(
                        zones.mark(
                            v_flex()
                                .flex_none()
                                .w(px(ROWS_WIDTH))
                                .max_w_full()
                                .gap(px(ROW_GAP))
                                .children(rows),
                        ),
                    )
                })
                .child(zones.mark(bar)),
        )
}
