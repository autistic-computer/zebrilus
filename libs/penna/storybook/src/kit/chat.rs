use ala::{
    AnyElement, AnyView, App, AppContext as _, Context, Div, Entity, IntoElement,
    ParentElement as _, Render, SharedString, Styled as _, Window, div, px,
};
use penna::{
    ActiveTheme as _, Disableable as _, Ground, Icon, IconName, Label, TextStyle, Tone,
    blocks::chat::{
        ASK_GAP, ActivityCount, ActivityEnd, ActivityMark, AskDetail, AskHint, AskOption,
        COMPOSER_MAX_WIDTH, ChatComposer, Composer, ComposerAction, PlanState, TrayPlan,
        UserMessage, activity_row, ask_body, ask_count, ask_divider, ask_header, ask_hints,
        ask_inset, ask_key, ask_leading, ask_note, ask_options, ask_picked_key, ask_question,
        ask_send, ask_sheet, ask_skip, ask_step, chrome_button, composer_action_button,
        composer_add_button, composer_ground, context_meter, empty_state, message_action,
        notice_row, plan_list, plan_tray, queue_chip, reply_bubble, suggestion_list,
        suggestion_row, welcome_state,
    },
    components::button::Button,
    components::input::InputState,
    h_flex, v_flex,
};

use crate::story::{Framed as _, Section, Story, stateless, states};

pub const STORY: Story = Story {
    id: "chat",
    name: "Chat",
    group: "Kit",
    summary: "The pieces of an agent chat: activity rows for tools and thinking, messages, the composer, slash suggestions, the composer asking a question or for approval, and the task tray. Each takes plain data, so any app can feed it.",
    sections: &[
        Section {
            id: "activity",
            name: "Activity rows",
            summary: "activity_row with an ActivityMark: an icon, a running spinner, a tint and tag for a failed or waiting tool, detail text, per-kind counts, a chevron that opens or a link that leaves, and the live line: a crest that sweeps the muted label and a glyph that breathes on the same 2.6s cycle, shown here frozen at four moments.",
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
            id: "ask",
            name: "Asking",
            summary: "The composer morphs to ask: Composer.ask holds a question as its own header (ask_question), or an approval's header and tool, then AskOption rows with their number keys above the input row, which takes the answer's own field, a keycap for typing, Skip and the send button, on the sheet's gutter. A picked answer of a pick-any question lights its key. ask_hints go in the footer, leading with the pick count.",
            build: |window, cx| Asks::view(window, cx),
        },
        Section {
            id: "tasks",
            name: "Plan tray",
            summary: "plan_tray: the plan chip over the composer, opening the plan's steps above it.",
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
const NARROW: f32 = 280.0;

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
    let mut merged = ActivityMark::bare();
    merged.counts = vec![
        count(IconName::Pencil, 1, "1 edit", false),
        count(IconName::File, 12, "12 reads", false),
    ];
    let mut long_step = ActivityMark::plain(IconName::SquareTerminal);
    long_step.detail = Some("Permission denied (os error 13) while opening the lock file".into());
    long_step.tag = Some(("exit 1".into(), Tone::Danger));
    let live = |at: u64| {
        let mut mark = ActivityMark::plain(IconName::Sparkle);
        mark.shimmer = Some(at);
        mark
    };
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
            "too long for a narrow pane: the detail gives way, then the label ends in an ellipsis",
            div()
                .w(px(NARROW))
                .child(
                    v_flex()
                        .child(row(
                            "activity-long-run",
                            merged,
                            "Edited iridescent-petting-dragonfly.md, read 12 files, ran 9 commands, used 4 tools",
                            ActivityEnd::Disclosure(false),
                        ))
                        .child(row(
                            "activity-long-step",
                            long_step,
                            "cargo test -p zz-ui --all-features agent::chat::steps",
                            ActivityEnd::Open,
                        )),
                )
                .into_any_element(),
        )
        .state(
            "live: the sweep and the breathing glyph at 0.3s, 0.9s, 1.5s and 2.3s of the cycle",
            column(
                v_flex()
                    .child(row(
                        "activity-live-early",
                        live(300),
                        "Fetching the release notes",
                        ActivityEnd::None,
                    ))
                    .child(row(
                        "activity-live-middle",
                        live(900),
                        "Fetching the release notes",
                        ActivityEnd::None,
                    ))
                    .child(row(
                        "activity-live-late",
                        live(1500),
                        "Fetching the release notes",
                        ActivityEnd::None,
                    ))
                    .child(row(
                        "activity-live-rest",
                        live(2300),
                        "Fetching the release notes",
                        ActivityEnd::None,
                    )),
            ),
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
    let inside = composer_ground(cx.theme().ground(), cx);
    Composer {
        input: input.clone(),
        leading: Some(
            composer_add_button(
                SharedString::from(format!("{id}-add")),
                "Mention a file",
                inside,
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
            inside,
        )
        .into_any_element(),
        attachments: None,
        command_hint: None,
        ask: None,
        secret: false,
        ground: cx.theme().ground(),
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
                        over: None,
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

const LONG_LABEL: &str = "Keep this answer's label long on purpose, so it has to wrap across several lines of the sheet without pushing the key cap out, breaking the highlight under it, or moving the answers below, which is exactly what this state is here to check";
const LONG_DETAIL: &str = "Its description wraps as well: it says what picking this answer does, which files and commands it touches, what it leaves alone and why the agent stops to ask before it goes ahead, so it runs over more than one line at the width the composer has in a pane.";
const LONG_QUESTION: &str = "The agent asks a question long enough to wrap: it says which part of the sheet it wants to change, why the change matters for every ask that grows out of the composer, and what it will leave alone";

struct Asks {
    question: Entity<InputState>,
    long: Entity<InputState>,
    multiple: Entity<InputState>,
    unpicked: Entity<InputState>,
    wrapped: Entity<InputState>,
    typed: Entity<InputState>,
    approval: Entity<InputState>,
    plan: Entity<InputState>,
    readonly: Entity<InputState>,
    answering: Entity<InputState>,
}

impl Asks {
    fn view(window: &mut Window, cx: &mut App) -> AnyView {
        let field =
            |placeholder: &'static str, value: &'static str, window: &mut Window, cx: &mut App| {
                cx.new(|cx| {
                    InputState::new(window, cx)
                        .placeholder(placeholder)
                        .default_value(value)
                })
            };
        let question = field("Something else", "", window, cx);
        let long = field("Something else", "", window, cx);
        let multiple = field("Something else", "Code blocks", window, cx);
        let unpicked = field("Something else", "", window, cx);
        let wrapped = field("Something else", "", window, cx);
        let typed = field("Type your answer", "", window, cx);
        let approval = field("Deny and say what to do instead", "", window, cx);
        let plan = field("Keep planning and say what to change", "", window, cx);
        let readonly = field("Deny and say what to do instead", "", window, cx);
        let answering = field("Something else", "", window, cx);
        cx.new(|_| Self {
            question,
            long,
            multiple,
            unpicked,
            wrapped,
            typed,
            approval,
            plan,
            readonly,
            answering,
        })
        .into()
    }
}

struct Asking {
    id: &'static str,
    body: AnyElement,
    leading: Option<AnyElement>,
    skip: bool,
    send: &'static str,
    enabled: bool,
    hints: Vec<AskHint>,
}

fn asking(input: &Entity<InputState>, ask: Asking, cx: &App) -> AnyElement {
    let id = ask.id;
    let ground = cx.theme().ground();
    let inside = composer_ground(ground, cx);
    let trailing = h_flex()
        .flex_none()
        .gap(px(ASK_GAP))
        .children(ask.skip.then(|| {
            ask_skip(SharedString::from(format!("{id}-skip")), inside)
                .disabled(!ask.enabled)
                .into_any_element()
        }))
        .child(
            ask_send(SharedString::from(format!("{id}-send")), ask.send, inside)
                .disabled(!ask.enabled),
        );
    div()
        .w(px(COMPOSER_MAX_WIDTH + 24.0))
        .pt(px(12.0))
        .framed(cx)
        .child(ChatComposer {
            composer: Composer {
                input: input.clone(),
                leading: ask.leading,
                usage: Some(trailing.into_any_element()),
                action: composer_action_button(
                    SharedString::from(format!("{id}-stop")),
                    ComposerAction::Stop,
                    true,
                    inside,
                )
                .into_any_element(),
                attachments: None,
                command_hint: None,
                ask: Some(ask.body),
                secret: false,
                ground,
            },
            prefix: Vec::new(),
            over: None,
            footer: vec![ask_hints(ask.hints, ground).into_any_element()],
            controls: Vec::new(),
        })
        .into_any_element()
}

struct Rows<'a> {
    id: &'static str,
    answers: &'a [(&'static str, Option<&'static str>)],
    place: AskDetail,
    highlighted: usize,
    reject: Option<usize>,
    picks: Option<&'a [bool]>,
    keyed: usize,
    enabled: bool,
}

impl Rows<'_> {
    fn render(self, cx: &App) -> AnyElement {
        let inside = composer_ground(cx.theme().ground(), cx);
        ask_options()
            .children(
                self.answers
                    .iter()
                    .enumerate()
                    .flat_map(|(index, (label, detail))| {
                        let divider = (Some(index) == self.reject)
                            .then(|| ask_divider(cx).into_any_element());
                        let row =
                            AskOption::new((SharedString::from(self.id), index), *label, inside)
                                .highlighted(index == self.highlighted)
                                .enabled(self.enabled);
                        let row = if index < self.keyed {
                            row.key((index + 1).to_string())
                        } else {
                            row
                        };
                        let row = match detail {
                            Some(detail) => row.detail(*detail, self.place),
                            None => row,
                        };
                        let row = match self.picks.and_then(|picks| picks.get(index)) {
                            Some(picked) => row.picked(*picked),
                            None => row,
                        };
                        divider.into_iter().chain([row.into_any_element()])
                    }),
            )
            .into_any_element()
    }
}

fn question_rows(
    id: &'static str,
    answers: &[(&'static str, Option<&'static str>)],
    highlighted: usize,
    cx: &App,
) -> AnyElement {
    Rows {
        id,
        answers,
        place: AskDetail::Under,
        highlighted,
        reject: None,
        picks: None,
        keyed: usize::MAX,
        enabled: true,
    }
    .render(cx)
}

fn pager(header: Div, id: &'static str, count: &'static str, inside: Ground) -> Div {
    header
        .child(ask_step(
            SharedString::from(format!("{id}-back")),
            IconName::ChevronLeft,
            "Previous question",
            inside,
        ))
        .child(ask_count(count, inside))
        .child(ask_step(
            SharedString::from(format!("{id}-forward")),
            IconName::ChevronRight,
            "Next question",
            inside,
        ))
}

const SCOPE: &[(&str, Option<&str>)] = &[
    ("Questions", Some("Single and multiple choice")),
    ("Tool approvals", Some("Commands, edits and fetches")),
    ("Plan approval", Some("The plan the agent wants to build")),
    ("Subagent prompts", Some("Asks that come from a subagent")),
];

impl Render for Asks {
    #[expect(clippy::too_many_lines, reason = "one state per ask kind")]
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let cx: &App = cx;
        let inside = composer_ground(cx.theme().ground(), cx);
        let approvals: &[(&str, Option<&str>)] = &[
            ("Allow once", None),
            ("Always allow cargo test", Some("this session")),
            ("Deny", None),
        ];
        let question = v_flex()
            .w_full()
            .child(ask_question(
                "Which accent should the composer use?",
                inside,
                cx,
            ))
            .child(question_rows(
                "ask-question",
                &[
                    ("Teal", Some("Calm, reads well on both modes")),
                    ("Violet", Some("Matches the current brand")),
                    ("Amber", Some("Warm, draws the eye")),
                    ("Graphite", Some("No accent at all")),
                ],
                0,
                cx,
            ))
            .into_any_element();
        let long = v_flex()
            .w_full()
            .child(ask_question(
                "Which answer should the long-label case use?",
                inside,
                cx,
            ))
            .child(question_rows(
                "ask-long",
                &[
                    (LONG_LABEL, Some(LONG_DETAIL)),
                    ("Short", Some("A one-line answer under it")),
                ],
                0,
                cx,
            ))
            .into_any_element();
        let multiple = v_flex()
            .w_full()
            .child(pager(
                ask_question("What should the morph cover?", inside, cx),
                "ask-multiple",
                "2 of 3",
                inside,
            ))
            .child(
                Rows {
                    id: "ask-multiple",
                    answers: &SCOPE[..3],
                    place: AskDetail::Under,
                    highlighted: 1,
                    reject: None,
                    picks: Some(&[true, true, false]),
                    keyed: usize::MAX,
                    enabled: true,
                }
                .render(cx),
            )
            .into_any_element();
        let unpicked = v_flex()
            .w_full()
            .child(ask_question("What should the morph cover?", inside, cx))
            .child(
                Rows {
                    id: "ask-unpicked",
                    answers: SCOPE,
                    place: AskDetail::Under,
                    highlighted: 2,
                    reject: None,
                    picks: Some(&[true, true, false, true]),
                    keyed: 3,
                    enabled: true,
                }
                .render(cx),
            )
            .into_any_element();
        let wrapped = v_flex()
            .w_full()
            .child(pager(
                ask_question(LONG_QUESTION, inside, cx),
                "ask-wrapped",
                "1 of 2",
                inside,
            ))
            .child(question_rows(
                "ask-wrapped",
                &[
                    ("The header", Some("The question takes its place")),
                    ("The picks", Some("A picked answer lights its key")),
                ],
                0,
                cx,
            ))
            .into_any_element();
        let typed = v_flex()
            .w_full()
            .child(ask_question(
                "How long should finished chats stay in the sidebar?",
                inside,
                cx,
            ))
            .into_any_element();
        let answering = v_flex()
            .w_full()
            .child(ask_question("What should the morph cover?", inside, cx))
            .child(
                Rows {
                    id: "ask-answering",
                    answers: &SCOPE[..3],
                    place: AskDetail::Under,
                    highlighted: 0,
                    reject: None,
                    picks: Some(&[true, false, true]),
                    keyed: usize::MAX,
                    enabled: false,
                }
                .render(cx),
            )
            .into_any_element();
        let approval = |id: &'static str, enabled: bool| {
            v_flex()
                .w_full()
                .child(
                    ask_header(IconName::SquareTerminal, "Run this command?", inside, cx)
                        .child(ask_count("1 of 2", inside)),
                )
                .child(
                    ask_body()
                        .child(ask_inset(cx).child("cargo test -p zz-mux view_pane"))
                        .child(ask_note("in ~/src/app", inside)),
                )
                .child(
                    Rows {
                        id,
                        answers: approvals,
                        place: AskDetail::Beside,
                        highlighted: 2,
                        reject: Some(2),
                        picks: None,
                        keyed: usize::MAX,
                        enabled,
                    }
                    .render(cx),
                )
                .into_any_element()
        };
        let plan = v_flex()
            .w_full()
            .child(
                ask_header(IconName::List, "Plan", inside, cx).child(ask_step(
                    "ask-plan-open",
                    IconName::Expand,
                    "Open the plan in a view pane",
                    inside,
                )),
            )
            .child(
                ask_sheet(cx).h(px(152.0)).child(
                    v_flex()
                        .gap(px(6.0))
                        .child(Label::new(TextStyle::Body, "Composer morph").strong())
                        .child(Label::new(
                            TextStyle::Body,
                            "1. Give the composer an ask body above its input row.",
                        ))
                        .child(Label::new(
                            TextStyle::Body,
                            "2. Show the answer's own field in the input row.",
                        ))
                        .child(Label::new(
                            TextStyle::Body,
                            "3. Move approvals and plan approval into the same body.",
                        )),
                ),
            )
            .child(
                Rows {
                    id: "ask-plan",
                    answers: &[
                        ("Build it", Some("accept edits as they come")),
                        ("Build it", Some("ask before each edit")),
                        ("Keep planning", None),
                    ],
                    place: AskDetail::Beside,
                    highlighted: 2,
                    reject: Some(2),
                    picks: None,
                    keyed: usize::MAX,
                    enabled: true,
                }
                .render(cx),
            )
            .into_any_element();
        let key = |key: &'static str| Some(ask_leading(ask_key(key, inside)).into_any_element());
        let lit = |key: &'static str| {
            Some(ask_leading(ask_picked_key(Some(key.into()), inside)).into_any_element())
        };
        let hints = |items: &[&'static str]| -> Vec<AskHint> {
            items.iter().map(|item| AskHint::from(*item)).collect()
        };
        let picked = |count: &'static str, items: &[&'static str]| -> Vec<AskHint> {
            std::iter::once(AskHint::strong(count))
                .chain(items.iter().map(|item| AskHint::from(*item)))
                .collect()
        };
        states()
            .state(
                "a question: pick one or type something else",
                asking(
                    &self.question,
                    Asking {
                        id: "ask-question",
                        body: question,
                        leading: key("5"),
                        skip: true,
                        send: "Send",
                        enabled: true,
                        hints: hints(&["1 to 5 pick", "↑↓ move", "↵ send", "esc skip"]),
                    },
                    cx,
                ),
            )
            .state(
                "several questions, pick any: two picked and something else typed, the highlight on a picked answer",
                asking(
                    &self.multiple,
                    Asking {
                        id: "ask-multiple",
                        body: multiple,
                        leading: lit("4"),
                        skip: true,
                        send: "Next",
                        enabled: true,
                        hints: picked("3 picked", &["1 to 4 toggle", "←→ page", "↵ next", "esc skip"]),
                    },
                    cx,
                ),
            )
            .state(
                "pick any: the highlight on an answer not picked; past the keyed answers a check marks a pick",
                asking(
                    &self.unpicked,
                    Asking {
                        id: "ask-unpicked",
                        body: unpicked,
                        leading: None,
                        skip: true,
                        send: "Send",
                        enabled: true,
                        hints: picked("3 picked", &["1 to 3 toggle", "↑↓ move", "↵ send", "esc skip"]),
                    },
                    cx,
                ),
            )
            .state(
                "a long question wraps; its glyph and the pager stay on the first line",
                asking(
                    &self.wrapped,
                    Asking {
                        id: "ask-wrapped",
                        body: wrapped,
                        leading: key("3"),
                        skip: true,
                        send: "Next",
                        enabled: true,
                        hints: hints(&["1 to 3 pick", "←→ page", "↵ next", "esc skip"]),
                    },
                    cx,
                ),
            )
            .state(
                "a question with no answers to pick: the header, then the field",
                asking(
                    &self.typed,
                    Asking {
                        id: "ask-typed",
                        body: typed,
                        leading: None,
                        skip: true,
                        send: "Send",
                        enabled: true,
                        hints: hints(&["↵ send", "esc skip"]),
                    },
                    cx,
                ),
            )
            .state(
                "a tool approval, the highlight on the last answer",
                asking(
                    &self.approval,
                    Asking {
                        id: "ask-approval",
                        body: approval("ask-approval", true),
                        leading: key("4"),
                        skip: false,
                        send: "Deny",
                        enabled: true,
                        hints: hints(&["1 to 3 pick", "↵ deny", "esc deny"]),
                    },
                    cx,
                ),
            )
            .state(
                "plan approval, the highlight on the last answer",
                asking(
                    &self.plan,
                    Asking {
                        id: "ask-plan",
                        body: plan,
                        leading: key("4"),
                        skip: false,
                        send: "Keep planning",
                        enabled: true,
                        hints: hints(&["1 to 3 pick", "↵ keep planning", "esc keep planning"]),
                    },
                    cx,
                ),
            )
            .state(
                "read-only: options and send disabled",
                asking(
                    &self.readonly,
                    Asking {
                        id: "ask-readonly",
                        body: approval("ask-readonly", false),
                        leading: key("4"),
                        skip: false,
                        send: "Allow",
                        enabled: false,
                        hints: hints(&["1 to 3 pick", "↵ allow", "esc deny"]),
                    },
                    cx,
                ),
            )
            .state(
                "read-only pick any: lit keys stay lit, dimmed",
                asking(
                    &self.answering,
                    Asking {
                        id: "ask-answering",
                        body: answering,
                        leading: key("4"),
                        skip: true,
                        send: "Send",
                        enabled: false,
                        hints: picked("2 picked", &["1 to 4 toggle", "↑↓ move", "↵ send", "esc skip"]),
                    },
                    cx,
                ),
            )
            .state(
                "a long answer wraps; its highlight keeps the corner of a short one",
                asking(
                    &self.long,
                    Asking {
                        id: "ask-long",
                        body: long,
                        leading: key("3"),
                        skip: true,
                        send: "Send",
                        enabled: true,
                        hints: hints(&["1 to 3 pick", "↑↓ move", "↵ send", "esc skip"]),
                    },
                    cx,
                ),
            )
    }
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
    let tray = |id: &str, open| column(plan_tray(id, plan(), open, |_, _| {}, cx));
    states()
        .state("plan, closed", tray("tray-plan", false))
        .state("plan, open", tray("tray-plan-open", true))
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
