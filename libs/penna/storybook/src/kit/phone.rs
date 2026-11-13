use ala::{
    AnyElement, AnyView, App, AppContext as _, Context, Entity, IntoElement, ParentElement as _,
    Render, Styled as _, Window, div, px,
};
use penna::{
    ActiveTheme as _, Colorize as _, Disableable as _, Icon, IconName, Sizable as _, Size,
    blocks::compact::{ZoomCard, compact_bar_button, zoom_card_ground},
    blocks::touch::{
        ACTION_BAR_BOTTOM, PhoneEmpty, TouchGroup, TouchHeader, TouchRow, TouchTrailing, action_bar,
    },
    components::button::{Button, ButtonVariants as _},
    components::field::Field,
    components::input::{Input, InputContentType, InputState},
    v_flex,
};

use super::support::after_first_frame;
use crate::story::{Section, Story, row, stateless, states};

pub const STORY: Story = Story {
    id: "phone",
    name: "Phone",
    group: "Kit",
    summary: "The pieces a phone screen is built from: the 56 main action and field, the 44 icon button, touch rows, the row group and the section header.",
    sections: &[
        Section {
            id: "main-action",
            name: "Main action",
            summary: "Size::TouchLarge, full width: 56 tall, touch-body, inset 16, a 20 icon 8 from the label. Loading keeps the label.",
            build: |_, cx| stateless(main_action, cx),
        },
        Section {
            id: "icon-button",
            name: "Icon button",
            summary: "compact_bar_button: 44 square with a 20 glyph, ghost.",
            build: |_, cx| stateless(icon_buttons, cx),
        },
        Section {
            id: "fields",
            name: "Fields",
            summary: "Field and Input at Size::TouchLarge: strong touch-small label, touch-small help and error, a 20 leading icon, and 44 accessories 4 from the field's end.",
            build: |window, cx| cx.new(|cx| Fields::new(window, cx)).into(),
        },
        Section {
            id: "field-focused",
            name: "Focused field",
            summary: "Focus fills the field with the highlight a selected row takes, with no edge.",
            build: |window, cx| Focused::view(window, cx),
        },
        Section {
            id: "rows",
            name: "Touch rows",
            summary: "TouchRow: one line 52, two lines 64, a 20 muted glyph, and a trailing glyph, value or check. Disabled rows go faint and drop their trailing mark; danger rows take the danger ink.",
            build: |_, cx| stateless(rows, cx),
        },
        Section {
            id: "groups",
            name: "Row groups",
            summary: "TouchHeader over a TouchGroup: the group takes the phone container corner, its rows sit 4 in and 2 apart with their corners capped by the group's. A holdable header is 44 tall.",
            build: |_, cx| stateless(groups, cx),
        },
        Section {
            id: "action-bar",
            name: "Action bar",
            summary: "action_bar: a column 8 apart, 16 from the sides and 20 above the bottom edge, holding the one main action at 56 and at most one secondary above it.",
            build: |_, cx| stateless(action_bars, cx),
        },
        Section {
            id: "empty-state",
            name: "Empty state",
            summary: "PhoneEmpty: a 32 muted icon or spinner, the title in title, and a touch-small muted line only when there is a state to say.",
            build: |_, cx| stateless(empty_states, cx),
        },
        Section {
            id: "zoom-card",
            name: "Zoom card",
            summary: "ZoomCard on the phone.card look: a 44 top row with the close control at its end, the 52 large title, the body 16 in with groups 16 apart, then the action bar. The add card and the connecting card.",
            build: |window, cx| cx.new(|cx| Cards::new(window, cx)).into(),
        },
    ],
};

const WIDTH: f32 = 343.0;

fn column(child: impl IntoElement) -> impl IntoElement {
    div().w(px(WIDTH)).child(child)
}

fn main_action(_: &mut Window, _: &mut App) -> AnyElement {
    let action = |id: &'static str, label: &'static str| {
        Button::new(id)
            .label(label)
            .with_size(Size::TouchLarge)
            .w_full()
    };
    states()
        .columns(2)
        .state(
            "primary",
            column(action("main-primary", "Connect").primary()),
        )
        .state("secondary", column(action("main-secondary", "Scan QR")))
        .state(
            "ghost",
            column(action("main-ghost", "Type an address instead").ghost()),
        )
        .state(
            "danger",
            column(action("main-danger", "Forget xps").danger()),
        )
        .state(
            "icon",
            column(
                action("main-icon", "Add machine")
                    .primary()
                    .icon(Icon::new(IconName::Plus)),
            ),
        )
        .state(
            "disabled",
            column(action("main-disabled", "Connect").primary().disabled(true)),
        )
        .state(
            "loading",
            column(action("main-loading", "Connecting").primary().loading(true)),
        )
        .into_any_element()
}

fn icon_buttons(_: &mut Window, _: &mut App) -> AnyElement {
    row()
        .child(compact_bar_button("icon-close", IconName::Xmark).tooltip("Close"))
        .child(compact_bar_button("icon-settings", IconName::Settings).tooltip("Settings"))
        .child(compact_bar_button("icon-add", IconName::Plus).tooltip("Add"))
        .into_any_element()
}

fn field(
    window: &mut Window,
    cx: &mut App,
    build: impl FnOnce(InputState) -> InputState,
) -> Entity<InputState> {
    cx.new(|cx| build(InputState::new(window, cx)))
}

fn lead(cx: &App) -> Icon {
    Icon::new(IconName::Server)
        .with_size(Size::TouchLarge)
        .text_color(cx.theme().foreground.muted())
}

struct Fields {
    empty: Entity<InputState>,
    filled: Entity<InputState>,
    error: Entity<InputState>,
    disabled: Entity<InputState>,
    password: Entity<InputState>,
    loading: Entity<InputState>,
}

impl Fields {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            empty: field(window, cx, |s| s.placeholder("user@host")),
            filled: field(window, cx, |s| s.default_value("fabrico@xps")),
            error: field(window, cx, |s| s.default_value("fabrico@xps:99999")),
            disabled: field(window, cx, |s| s.placeholder("user@host")),
            password: field(window, cx, |s| s.default_value("hunter22")),
            loading: {
                let state = field(window, cx, |s| s.default_value("fabrico@xps"));
                state.update(cx, |state, cx| state.set_loading(true, window, cx));
                state
            },
        }
    }
}

impl Render for Fields {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let phone = || Field::new().with_size(Size::TouchLarge);
        let input = |state: &Entity<InputState>, id: &'static str| {
            Input::new(state).id(id).with_size(Size::TouchLarge)
        };
        states()
            .columns(2)
            .state(
                "label and help",
                column(
                    phone()
                        .label("Address")
                        .help("Your tailnet name works too.")
                        .child(input(&self.empty, "phone-empty").prefix(lead(cx))),
                ),
            )
            .state(
                "filled, with clear",
                column(
                    phone().label("Address").child(
                        input(&self.filled, "phone-filled")
                            .prefix(lead(cx))
                            .cleanable(true),
                    ),
                ),
            )
            .state(
                "error",
                column(
                    phone()
                        .label("Address")
                        .error("Ports go up to 65535.")
                        .child(
                            input(&self.error, "phone-error")
                                .prefix(lead(cx))
                                .tone(penna::Tone::Danger),
                        ),
                ),
            )
            .state(
                "disabled",
                column(
                    phone().label("Address").child(
                        input(&self.disabled, "phone-disabled")
                            .prefix(lead(cx))
                            .disabled(true),
                    ),
                ),
            )
            .state(
                "accessories",
                column(
                    phone().label("Password").child(
                        input(&self.password, "phone-password")
                            .content_type(InputContentType::Password)
                            .cleanable(true)
                            .suffix(
                                compact_bar_button("phone-password-reveal", IconName::Eye)
                                    .tooltip("Show password"),
                            ),
                    ),
                ),
            )
            .state(
                "loading",
                column(
                    phone()
                        .label("Address")
                        .child(input(&self.loading, "phone-loading").prefix(lead(cx))),
                ),
            )
    }
}

struct Focused {
    state: Entity<InputState>,
}

impl Focused {
    fn view(window: &mut Window, cx: &mut App) -> AnyView {
        let state = field(window, cx, |s| s.default_value("fabrico@xps"));
        let target = state.clone();
        after_first_frame(window, move |window, cx| {
            target.update(cx, |state, cx| state.focus(window, cx));
        });
        cx.new(|_| Self { state }).into()
    }
}

impl Render for Focused {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        states().state(
            "focused",
            column(
                Field::new()
                    .with_size(Size::TouchLarge)
                    .label("Name")
                    .child(
                        Input::new(&self.state)
                            .id("phone-focused")
                            .with_size(Size::TouchLarge),
                    ),
            ),
        )
    }
}

fn rows(_: &mut Window, _: &mut App) -> AnyElement {
    states()
        .columns(2)
        .state(
            "glyph",
            column(
                TouchRow::new("row-glyph", "Appearance")
                    .icon(IconName::Palette)
                    .trailing(TouchTrailing::Glyph(IconName::ChevronRight)),
            ),
        )
        .state(
            "value",
            column(
                TouchRow::new("row-value", "Machines")
                    .icon(IconName::HardDrive)
                    .trailing(TouchTrailing::Value("3".into())),
            ),
        )
        .state(
            "two lines",
            column(
                TouchRow::new("row-detail", "xps")
                    .icon(IconName::Laptop)
                    .detail("On your tailnet · runs zz")
                    .trailing(TouchTrailing::Glyph(IconName::Plus)),
            ),
        )
        .state(
            "check",
            column(
                TouchRow::new("row-check", "Dark")
                    .icon(IconName::Moon)
                    .trailing(TouchTrailing::Check),
            ),
        )
        .state(
            "disabled",
            column(
                TouchRow::new("row-disabled", "homelab")
                    .icon(IconName::HardDrive)
                    .detail("No zz on it yet")
                    .trailing(TouchTrailing::Glyph(IconName::Plus))
                    .disabled(true),
            ),
        )
        .state(
            "danger",
            column(
                TouchRow::new("row-danger", "Close session")
                    .icon(IconName::Xmark)
                    .danger(),
            ),
        )
        .into_any_element()
}

fn groups(_: &mut Window, _: &mut App) -> AnyElement {
    states()
        .columns(2)
        .state(
            "header with a count",
            column(
                v_flex()
                    .child(TouchHeader::new("Found on your tailnet").count(3))
                    .child(
                        TouchGroup::new()
                            .row(
                                TouchRow::new("found-macbook", "macbook")
                                    .icon(IconName::Laptop)
                                    .detail("On your tailnet · runs zz")
                                    .trailing(TouchTrailing::Glyph(IconName::Plus)),
                            )
                            .row(
                                TouchRow::new("found-alienware", "alienware")
                                    .icon(IconName::HardDrive)
                                    .detail("On your tailnet · runs zz")
                                    .trailing(TouchTrailing::Glyph(IconName::Plus)),
                            )
                            .row(
                                TouchRow::new("found-homelab", "homelab")
                                    .icon(IconName::HardDrive)
                                    .detail("No zz on it yet")
                                    .disabled(true),
                            ),
                    ),
            ),
        )
        .state(
            "holdable header",
            column(
                v_flex().child(TouchHeader::new("work").holdable()).child(
                    TouchGroup::new()
                        .row(TouchRow::new("work-rename", "Rename session").icon(IconName::Pencil))
                        .row(
                            TouchRow::new("work-close", "Close session")
                                .icon(IconName::Xmark)
                                .danger(),
                        ),
                ),
            ),
        )
        .into_any_element()
}

const BAR_WIDTH: f32 = WIDTH + 32.0;

fn main_button(id: &'static str, label: &'static str) -> Button {
    Button::new(id)
        .label(label)
        .with_size(Size::TouchLarge)
        .w_full()
}

fn action_bars(_: &mut Window, _: &mut App) -> AnyElement {
    let bar = |bar: ala::Div| div().w(px(BAR_WIDTH)).child(bar);
    states()
        .columns(2)
        .state(
            "main action",
            bar(action_bar(px(ACTION_BAR_BOTTOM))
                .child(main_button("bar-connect", "Connect").primary())),
        )
        .state(
            "secondary above",
            bar(action_bar(px(ACTION_BAR_BOTTOM))
                .child(main_button("bar-scan", "Scan QR instead").ghost())
                .child(main_button("bar-add", "Add machine").primary())),
        )
        .into_any_element()
}

fn empty_states(_: &mut Window, _: &mut App) -> AnyElement {
    states()
        .columns(2)
        .state(
            "icon",
            column(PhoneEmpty::new(IconName::Laptop, "No machines yet")),
        )
        .state(
            "spinner and a line",
            column(PhoneEmpty::spinner("Connecting").line("Waiting for ssh on fabrico@xps")),
        )
        .into_any_element()
}

const CARD_WIDTH: f32 = 370.0;
const CARD_HEIGHT: f32 = 640.0;

struct Cards {
    address: Entity<InputState>,
}

impl Cards {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            address: field(window, cx, |s| s.placeholder("user@host")),
        }
    }
}

fn card_frame(card: ZoomCard, cx: &App) -> impl IntoElement {
    div()
        .w(px(CARD_WIDTH))
        .h(px(CARD_HEIGHT))
        .child(card.top_inset(px(16.0)).rounded(cx.theme().phone_radius()))
}

impl Render for Cards {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ground = zoom_card_ground(cx);
        let found = |id: &'static str, name: &'static str, icon, detail: &'static str| {
            TouchRow::new(id, name)
                .icon(icon)
                .detail(detail)
                .trailing(TouchTrailing::Glyph(IconName::Plus))
        };
        let add = ZoomCard::new("card-add", "Add machine")
            .child(
                v_flex()
                    .child(TouchHeader::new("Found on your tailnet").count(2))
                    .child(
                        TouchGroup::new()
                            .row(found(
                                "card-found-macbook",
                                "macbook",
                                IconName::Laptop,
                                "macOS · runs zz",
                            ))
                            .row(
                                found(
                                    "card-found-homelab",
                                    "homelab",
                                    IconName::HardDrive,
                                    "No zz on it yet",
                                )
                                .disabled(true),
                            ),
                    ),
            )
            .child(
                Field::new()
                    .with_size(Size::TouchLarge)
                    .label("Or an address")
                    .child(
                        Input::new(&self.address)
                            .id("card-address")
                            .with_size(Size::TouchLarge)
                            .on(ground),
                    ),
            )
            .action(
                main_button("card-connect", "Connect")
                    .primary()
                    .on(ground)
                    .disabled(true),
            );
        let connecting = ZoomCard::new("card-connecting", "xps")
            .child(
                div().mt(px(96.0)).child(
                    PhoneEmpty::spinner("Connecting")
                        .line("Waiting for ssh on fabrico@xps")
                        .on(ground),
                ),
            )
            .action(main_button("card-cancel", "Cancel").on(ground));
        states()
            .columns(2)
            .state("add card", card_frame(add, cx))
            .state("connecting card", card_frame(connecting, cx))
    }
}
