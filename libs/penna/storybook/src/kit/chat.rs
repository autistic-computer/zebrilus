use ala::{
    AnyElement, AnyView, App, AppContext as _, Context, Entity, IntoElement, ParentElement as _,
    Render, SharedString, Styled as _, Window, div, px,
};
use penna::{
    ActiveTheme as _, Icon, IconName, Tone,
    blocks::chat::{
        ActivityCount, ActivityEnd, ActivityMark, COMPOSER_MAX_WIDTH, ChatComposer, Composer,
        ComposerAction, PlanState, TaskItem, TrayPanel, TrayPlan, UserMessage, activity_row,
        chrome_button, composer_action_button, composer_add_button, context_meter, empty_state,
        message_action, notice_row, permission_card, permission_option, plan_list, queue_chip,
        reply_bubble, suggestion_list, suggestion_row, task_tray, welcome_state,
    },
    components::button::Button,
    components::input::InputState,
    v_flex,
};

use crate::story::{Framed as _, Section, Story, stateless, states};

pub const STORY: Story = Story {
    id: "chat",
    name: "Chat",
    group: "Kit",
    summary: "The pieces of an agent chat: activity rows for tools and thinking, messages, the composer, slash suggestions, an action card and the task tray. Each takes plain data, so any app can feed it.",
    sections: &[
        Section {
            id: "activity",
            name: "Activity rows",
            summary: "activity_row with an ActivityMark: an icon, a running spinner, a tint and tag for a failed or waiting tool, detail text, per-kind counts, a shimmer while thinking, and a chevron that opens or a link that leaves.",
            build: |_, cx| stateless(activity, cx),
        },
        Section {
            id: "messages",
            name: "Messages",
            summary: "UserMessage in a bubble, with an action that shows on hover; reply_bubble with and without a note; notice_row; plan_list with pending, current and done steps.",
            build: |_, cx| stateless(messages, cx),
        },
        Section {
            id: "composer",
            name: "Composer",
            summary: "Composer: the leading +, the field, the context meter and the action button, which sends, queues behind a running turn or stops it. ChatComposer adds a prefix above and a footer below.",
            build: |window, cx| Composers::view(window, cx),
        },
        Section {
            id: "suggestions",
            name: "Suggestions",
            summary: "suggestion_list of suggestion_row: a label, a description, an optional badge, the picked row selected.",
            build: |_, cx| stateless(suggestions, cx),
        },
        Section {
            id: "card",
            name: "Action card",
            summary: "permission_card: a title, a counter, a close button and permission_option tiles with their number keys; the highlighted one shows return.",
            build: |_, cx| stateless(card, cx),
        },
        Section {
            id: "tasks",
            name: "Task tray",
            summary: "task_tray: the plan chip and the background tasks chip, each opening its panel above them.",
            build: |_, cx| stateless(tasks, cx),
        },
        Section {
            id: "empty",
            name: "Empty states",
            summary: "welcome_state before the first message and empty_state for a chat with nothing in it.",
            build: |_, cx| cx.new(|_| Empty).into(),
        },
    ],
};

const WIDTH: f32 = 560.0;

fn column(element: impl IntoElement) -> AnyElement {
    div().w(px(WIDTH)).child(element).into_any_element()
}

fn count(icon: IconName, count: usize, tooltip: &'static str, failed: bool) -> ActivityCount {
    ActivityCount {
        icon,
        count,
        tooltip: tooltip.into(),
        failed,
    }
}

fn activity(_: &mut Window, cx: &mut App) -> AnyElement {
    let cx: &App = cx;
    let row = |id: &'static str, mark: ActivityMark, label: &'static str, end: ActivityEnd| {
        activity_row(id, mark, label.into(), end, cx)
    };
    let mut detail = ActivityMark::plain(IconName::Search);
    detail.detail = Some("crates/zz-client".into());
    let mut running = ActivityMark::bare();
    running.spin = Some(0.0);
    running.label_color = Some(cx.theme().foreground);
    let mut failed = ActivityMark::plain(IconName::SquareTerminal);
    failed.tint = Some(cx.theme().danger);
    failed.tag = Some(("exit 1".into(), Tone::Danger));
    let mut waiting = ActivityMark::plain(IconName::SquareTerminal);
    waiting.tint = Some(cx.theme().warning);
    waiting.tag = Some(("needs approval".into(), Tone::Neutral));
    let mut counted = ActivityMark::bare();
    counted.counts = vec![
        count(IconName::File, 2, "2 reads", false),
        count(IconName::Pencil, 1, "1 edit", false),
        count(IconName::TriangleAlert, 1, "1 failed", true),
    ];
    let mut opened = counted.clone();
    opened.counts.truncate(2);
    let mut thinking = ActivityMark::bare();
    thinking.shimmer = Some(0);
    states()
        .state(
            "plain, detail, running",
            column(
                v_flex()
                    .child(row(
                        "activity-read",
                        ActivityMark::plain(IconName::File),
                        "Read tab_strip.rs",
                        ActivityEnd::None,
                    ))
                    .child(row(
                        "activity-search",
                        detail,
                        "Searched for turn_id",
                        ActivityEnd::None,
                    ))
                    .child(row(
                        "activity-running",
                        running,
                        "cargo test -p zz-ui",
                        ActivityEnd::None,
                    )),
            ),
        )
        .state(
            "failed and waiting tools",
            column(
                v_flex()
                    .child(row(
                        "activity-failed",
                        failed,
                        "cargo clippy",
                        ActivityEnd::None,
                    ))
                    .child(row(
                        "activity-waiting",
                        waiting,
                        "rm -rf target",
                        ActivityEnd::None,
                    )),
            ),
        )
        .state(
            "counts, closed and open; a row that opens its output",
            column(
                v_flex()
                    .child(row(
                        "activity-closed",
                        counted,
                        "Worked for 2m 14s",
                        ActivityEnd::Disclosure(false),
                    ))
                    .child(row(
                        "activity-open",
                        opened,
                        "Worked for 48s",
                        ActivityEnd::Disclosure(true),
                    ))
                    .child(row(
                        "activity-output",
                        ActivityMark::plain(IconName::SquareTerminal),
                        "just test",
                        ActivityEnd::Open,
                    )),
            ),
        )
        .state(
            "thinking",
            column(row(
                "activity-thinking",
                thinking,
                "Thinking",
                ActivityEnd::None,
            )),
        )
        .into_any_element()
}

fn text(body: &'static str) -> AnyElement {
    div().child(body).into_any_element()
}

fn messages(_: &mut Window, cx: &mut App) -> AnyElement {
    let cx: &App = cx;
    states()
        .state(
            "user message",
            column(UserMessage::new("message-user").body(text(
                "The chat keeps spinning after a turn ends. Find out why.",
            ))),
        )
        .state(
            "with an action on hover",
            column(
                UserMessage::new("message-action")
                    .body(text("Try the narrower width."))
                    .action(
                        message_action(
                            Button::compact_icon("message-rewind", IconName::History)
                                .tooltip("Rewind to here"),
                            cx,
                        ),
                        true,
                    ),
            ),
        )
        .state(
            "reply, with and without a note",
            column(
                v_flex()
                    .items_end()
                    .gap_2()
                    .child(reply_bubble(
                        "message-reply-note",
                        Some("Side answer · not in the conversation".into()),
                        text("It reads the width once, before the pinned tabs."),
                        cx,
                    ))
                    .child(reply_bubble("message-reply", None, text("Done."), cx)),
            ),
        )
        .state(
            "notice",
            column(notice_row(
                "message-notice",
                text("The agent compacted the conversation."),
                cx,
            )),
        )
        .state(
            "plan",
            column(plan_list(
                &[
                    (
                        PlanState::Done,
                        "Find where the tab strip divides its width".into(),
                    ),
                    (PlanState::Current, "Give pinned tabs a 28px floor".into()),
                    (PlanState::Pending, "Run the tab tests".into()),
                ],
                cx,
            )),
        )
        .into_any_element()
}

struct Composers {
    readonly: Entity<InputState>,
    empty: Entity<InputState>,
    typed: Entity<InputState>,
    busy: Entity<InputState>,
    queued: Entity<InputState>,
}

impl Composers {
    fn view(window: &mut Window, cx: &mut App) -> AnyView {
        let field = |value: &'static str, window: &mut Window, cx: &mut App| {
            cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("Message the agent…")
                    .default_value(value)
            })
        };
        let readonly = field("", window, cx);
        let empty = field("", window, cx);
        let typed = field("Give pinned tabs a 28px floor", window, cx);
        let busy = field("", window, cx);
        let queued = field("Then run the tests", window, cx);
        cx.new(|_| Self {
            readonly,
            empty,
            typed,
            busy,
            queued,
        })
        .into()
    }
}

fn composer(
    id: &'static str,
    input: &Entity<InputState>,
    action: ComposerAction,
    enabled: bool,
    cx: &App,
) -> Composer {
    Composer {
        input: input.clone(),
        leading: Some(
            composer_add_button(
                SharedString::from(format!("{id}-add")),
                "Mention a file",
                cx,
            )
            .into_any_element(),
        ),
        usage: Some(context_meter(
            SharedString::from(format!("{id}-usage")),
            0.34,
            "68000 of 200000 context tokens used (34%)",
            cx,
        )),
        action: composer_action_button(
            SharedString::from(format!("{id}-action")),
            action,
            enabled,
            cx,
        )
        .into_any_element(),
        attachments: None,
        command_hint: None,
        ground: None,
    }
}

impl Render for Composers {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let cx: &App = cx;
        let width = |element: AnyElement| div().w(px(COMPOSER_MAX_WIDTH)).child(element);
        states()
            .state(
                "empty, send",
                width(
                    composer(
                        "composer-empty",
                        &self.empty,
                        ComposerAction::Send,
                        true,
                        cx,
                    )
                    .into_any_element(),
                ),
            )
            .state(
                "typed, send",
                width(
                    composer(
                        "composer-typed",
                        &self.typed,
                        ComposerAction::Send,
                        true,
                        cx,
                    )
                    .into_any_element(),
                ),
            )
            .state(
                "read-only: send disabled",
                width(
                    composer(
                        "composer-readonly",
                        &self.readonly,
                        ComposerAction::Send,
                        false,
                        cx,
                    )
                    .into_any_element(),
                ),
            )
            .state(
                "a turn running: stop",
                width(
                    composer("composer-busy", &self.busy, ComposerAction::Stop, true, cx)
                        .into_any_element(),
                ),
            )
            .state(
                "ChatComposer: queue behind the turn, with a footer",
                div()
                    .w(px(COMPOSER_MAX_WIDTH + 24.0))
                    .pt(px(12.0))
                    .framed(cx)
                    .child(ChatComposer {
                        composer: composer(
                            "composer-queued",
                            &self.queued,
                            ComposerAction::Queue,
                            true,
                            cx,
                        ),
                        prefix: vec![
                            queue_chip("composer-queue", 2, true, |_, _| {}, cx).into_any_element(),
                        ],
                        footer: vec![
                            chrome_button("composer-folder")
                                .icon(IconName::Folder)
                                .label("~/src/app")
                                .into_any_element(),
                        ],
                        controls: vec![
                            chrome_button("composer-model")
                                .label("Model")
                                .into_any_element(),
                        ],
                    }),
            )
    }
}

fn suggestions(_: &mut Window, cx: &mut App) -> AnyElement {
    let cx: &App = cx;
    let rows = v_flex()
        .child(suggestion_row(
            "suggestion-compact",
            "/compact",
            Some("Summarize the conversation to free context".into()),
            None,
            false,
            cx,
        ))
        .child(suggestion_row(
            "suggestion-review",
            "/review",
            Some("Review the current changes".into()),
            Some("skill".into()),
            true,
            cx,
        ))
        .child(suggestion_row(
            "suggestion-model",
            "/model",
            None,
            None,
            false,
            cx,
        ));
    column(suggestion_list("suggestions", rows, cx))
}

fn card(_: &mut Window, cx: &mut App) -> AnyElement {
    let cx: &App = cx;
    let options = [
        ("Allow once", false),
        ("Always allow cargo test", false),
        ("Deny", true),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, (label, danger))| {
        permission_option(
            ("card-option", index),
            index,
            label,
            danger,
            index == 0,
            true,
            cx,
        )
        .into_any_element()
    })
    .collect();
    column(permission_card(
        "Run cargo test -p zz-ui?",
        Some("1 of 2".into()),
        options,
        Button::new("card-close").tooltip("Dismiss"),
        cx,
    ))
}

fn tasks(_: &mut Window, cx: &mut App) -> AnyElement {
    let cx: &App = cx;
    let plan = || TrayPlan {
        label: "Plan 1/3".into(),
        current: Some("Give pinned tabs a 28px floor".into()),
        items: vec![
            (
                PlanState::Done,
                "Find where the tab strip divides its width".into(),
            ),
            (PlanState::Current, "Give pinned tabs a 28px floor".into()),
            (PlanState::Pending, "Run the tab tests".into()),
        ],
    };
    let items = [
        TaskItem {
            id: "task-dev".into(),
            icon: IconName::Terminal,
            description: "npm run dev".into(),
            reveal: Some("tool-1".into()),
        },
        TaskItem {
            id: "task-agent".into(),
            icon: IconName::Bot,
            description: "Survey the tab tests".into(),
            reveal: None,
        },
    ];
    let tray = |id: &str, plan: Option<TrayPlan>, tasks: &[TaskItem], open| {
        column(div().children(task_tray(
            id,
            plan,
            tasks,
            format!("{} running", tasks.len()).into(),
            open,
            true,
            0.0,
            |_, _, _| {},
            cx,
        )))
    };
    states()
        .state("plan, closed", tray("tray-plan", Some(plan()), &[], None))
        .state(
            "plan, open",
            tray("tray-plan-open", Some(plan()), &[], Some(TrayPanel::Plan)),
        )
        .state(
            "tasks, open",
            tray("tray-tasks", None, &items, Some(TrayPanel::Tasks)),
        )
        .state(
            "plan and tasks",
            tray("tray-both", Some(plan()), &items, None),
        )
        .into_any_element()
}

struct Empty;

impl Render for Empty {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity_id();
        let welcome = welcome_state(
            Icon::new(IconName::Bot).size(px(40.0)),
            "Ask the agent",
            "It reads, edits and runs commands in this folder.",
            cx,
        );
        states()
            .state(
                "welcome_state",
                div().w(px(WIDTH)).h(px(260.0)).child(welcome),
            )
            .state(
                "empty_state",
                column(empty_state("No messages yet", false, view, cx)),
            )
    }
}
