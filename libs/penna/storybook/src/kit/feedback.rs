use ala::{AnyElement, App, IntoElement, ParentElement as _, Styled as _, Window, div, px};
use penna::{
    ActiveTheme as _, IconName, Tone,
    button::{Button, ButtonVariants as _},
    feedback::{alert_actions, alert_ground, callout, callout_ground, empty_state, inline_alert},
};

use crate::story::{Framed as _, Section, Story, stateless, states};

pub const STORY: Story = Story {
    id: "feedback",
    name: "Feedback",
    group: "Kit",
    summary: "Messages drawn in place of content: toned callouts, the failure alert a view shows instead of a dialog, and the empty state that fills a page with nothing to show yet.",
    sections: &[
        Section {
            id: "callouts",
            name: "Callouts",
            summary: "callout on the inline panel in a tone: the icon takes the tone's color and the panel takes the tone wherever the theme tints it. A title is optional; actions go in alert_actions, on callout_ground.",
            build: |_, cx| stateless(callouts, cx),
        },
        Section {
            id: "inline-alert",
            name: "Inline alert",
            summary: "inline_alert, the failure a view shows in place instead of a modal: a danger icon on the neutral inline panel, with the main action last on the right.",
            build: |_, cx| stateless(inline_alerts, cx),
        },
        Section {
            id: "empty-state",
            name: "Empty state",
            summary: "empty_state centers a muted message in the space it is given, under an optional title. An action goes after it as a child.",
            build: |_, cx| stateless(empty_states, cx),
        },
    ],
};

fn callouts(_: &mut Window, cx: &mut App) -> AnyElement {
    let cx: &App = cx;
    let toned = |tone: Tone, icon: IconName, title: &str, message: &str| {
        callout(
            tone,
            icon,
            Some(title.to_owned().into()),
            message.to_owned(),
            cx,
        )
    };
    states()
        .columns(2)
        .state(
            "primary",
            toned(
                Tone::Primary,
                IconName::Info,
                "Two computers found",
                "Pair one to share the keyboard and the pointer with it.",
            ),
        )
        .state(
            "success",
            toned(
                Tone::Success,
                IconName::CircleCheck,
                "Paired",
                "The laptop takes the pointer when it crosses the right edge.",
            ),
        )
        .state(
            "warning",
            toned(
                Tone::Warning,
                IconName::TriangleAlert,
                "Accessibility access is off",
                "Without it the keyboard reaches this computer but not the others.",
            ),
        )
        .state(
            "danger",
            toned(
                Tone::Danger,
                IconName::CircleX,
                "The service stopped",
                "It quit with exit code 1 after the last update.",
            ),
        )
        .state(
            "message only",
            callout(
                Tone::Primary,
                IconName::Info,
                None,
                "Changes apply to every window.",
                cx,
            ),
        )
        .state(
            "with an action",
            callout(
                Tone::Warning,
                IconName::TriangleAlert,
                Some("Update ready".into()),
                "Restart to finish installing version 2.4.",
                cx,
            )
            .child(
                alert_actions(cx).child(
                    Button::new("callout-restart")
                        .label("Restart")
                        .on(callout_ground(Tone::Warning, cx)),
                ),
            ),
        )
        .into_any_element()
}

fn inline_alerts(_: &mut Window, cx: &mut App) -> AnyElement {
    let cx: &App = cx;
    states()
        .columns(2)
        .state(
            "titled, with an action",
            inline_alert(
                Some("This page couldn't load".into()),
                "The server closed the connection before it sent a response.",
                cx,
            )
            .child(
                alert_actions(cx).child(
                    Button::new("alert-retry")
                        .label("Try again")
                        .on(alert_ground(cx)),
                ),
            ),
        )
        .state(
            "message only",
            inline_alert(
                None,
                "The file changed on disk and could not be read again.",
                cx,
            ),
        )
        .into_any_element()
}

fn empty_states(_: &mut Window, cx: &mut App) -> AnyElement {
    let cx: &App = cx;
    let page = |content: ala::Div| {
        div()
            .flex()
            .w_full()
            .h(px(220.0))
            .bg(cx.theme().background)
            .framed(cx)
            .rounded(cx.theme().radius)
            .child(content)
    };
    states()
        .columns(2)
        .state("message only", page(empty_state(None, "Connecting…", cx)))
        .state(
            "titled, with an action",
            page(
                empty_state(
                    Some("Could not start".into()),
                    "The background service did not answer.",
                    cx,
                )
                .child(Button::new("empty-retry").primary().label("Try again")),
            ),
        )
        .into_any_element()
}
