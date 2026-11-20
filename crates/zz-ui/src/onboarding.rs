use ala::{AnyElement, App, Div, Pixels, SharedString, div, prelude::*, px};
use penna::{
    Label, TextStyle,
    blocks::touch::{PageDot, page_dots},
};

use crate::{
    ActiveTheme as _, h_flex,
    pane_look::pane_color,
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
const PHONE_SIDE: f32 = 20.0;
const PHONE_WIDTH: f32 = 440.0;
const PHONE_ABOVE: f32 = 8.0;
const PHONE_ROW: f32 = 52.0;
const PHONE_DOTS_GAP: f32 = 14.0;

pub const PHONE_STEPS: [PhoneStep; 3] =
    [PhoneStep::Welcome, PhoneStep::Machine, PhoneStep::Tailnet];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PhoneStep {
    Welcome,
    Machine,
    Tailnet,
}

impl PhoneStep {
    #[must_use]
    pub fn title(self, machine: &str) -> SharedString {
        match self {
            Self::Welcome => Step::Welcome.title().into(),
            Self::Machine => "Point it at your machine.".into(),
            Self::Tailnet => format!("{machine} brought friends.").into(),
        }
    }

    #[must_use]
    pub const fn description(self) -> &'static str {
        match self {
            Self::Welcome => Step::Welcome.description(),
            Self::Machine => "Type its address, or scan the code from zz on your computer.",
            Self::Tailnet => "Found on your tailnet.",
        }
    }
}

pub struct PhonePage {
    pub step: PhoneStep,
    pub shown: PhoneStep,
    pub title: SharedString,
    pub body: Option<Div>,
    pub fade: f32,
    pub shift: Pixels,
    pub button: AnyElement,
    pub skip: Option<AnyElement>,
    pub top: Pixels,
    pub bottom: Pixels,
}

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
    let theme = cx.theme();
    div()
        .relative()
        .size_full()
        .bg(pane_color(theme, theme.background))
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

#[must_use]
pub fn phone_onboarding_row(
    name: impl Into<SharedString>,
    detail: impl Into<SharedString>,
    control: Option<AnyElement>,
) -> Div {
    h_flex()
        .flex_none()
        .w_full()
        .h(px(PHONE_ROW))
        .gap(px(CONTROL_GAP))
        .child(
            h_flex()
                .flex_1()
                .min_w_0()
                .items_baseline()
                .gap(px(DETAIL_GAP))
                .child(Label::new(TextStyle::TouchBody, name).flex_none())
                .child(Label::new(TextStyle::TouchSmall, detail).muted().truncate()),
        )
        .children(control)
}

#[must_use]
pub fn phone_onboarding_page(page: PhonePage, cx: &App) -> Div {
    let zones = SkyZones::default();
    let dots = page_dots(
        &[PHONE_STEPS
            .map(|at| PageDot {
                active: at == page.step,
                attention: false,
            })
            .to_vec()],
        cx,
    );
    let head = v_flex()
        .flex_none()
        .items_start()
        .gap(px(HEAD_GAP))
        .child(Label::new(TextStyle::Display, page.title).strong())
        .child(Label::new(TextStyle::TouchBody, page.shown.description()).muted());
    let foot = v_flex()
        .flex_none()
        .w_full()
        .gap(px(PHONE_DOTS_GAP))
        .child(zones.mark(h_flex().flex_none().justify_center().child(dots)))
        .child(zones.mark(div().w_full().child(page.button)));
    let theme = cx.theme();
    div()
        .relative()
        .size_full()
        .bg(pane_color(theme, theme.background))
        .child(sky("phone-onboarding-sky", &zones))
        .child(
            v_flex()
                .absolute()
                .inset_0()
                .items_start()
                .justify_end()
                .px(px(PHONE_SIDE))
                .pb(page.bottom + px(PHONE_ABOVE))
                .child(
                    v_flex()
                        .w_full()
                        .max_w(px(PHONE_WIDTH))
                        .gap(px(BLOCK_GAP))
                        .child(
                            v_flex()
                                .relative()
                                .left(page.shift)
                                .w_full()
                                .gap(px(BLOCK_GAP))
                                .opacity(page.fade)
                                .child(zones.mark(head))
                                .children(page.body.map(|body| zones.mark(body.w_full()))),
                        )
                        .child(foot),
                ),
        )
        .children(page.skip.map(|skip| {
            div()
                .absolute()
                .top(page.top)
                .right(px(PHONE_SIDE - CONTROL_GAP))
                .child(skip)
        }))
}
