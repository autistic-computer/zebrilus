use std::sync::Arc;

use zz_client::agent_completion::{active_command_hint, pane_commands};
use zz_gpui::{
    AnyElement, AnyView, App, AppContext as _, Context, Entity, Image, IntoElement,
    ParentElement as _, Render, SharedString, Styled as _, Window, div, px,
};
use zz_ui::{
    ActiveTheme as _, Colorize as _, Disableable as _, IconName, Sizable as _,
    agent::{
        COMPOSER_ATTACHMENT, agent_attachment_thumbnail,
        composer::{AgentComposer, COMPOSER_MAX_WIDTH, COMPOSER_OUTER_PADDING, Composer},
        controls::{
            ComposerAction, composer_action, composer_action_button, composer_add_button,
            context_usage_meter, git_summary_footer,
        },
        presentation::error_card,
    },
    button::{Button, ButtonVariants as _},
    h_flex,
    input::InputState,
};

use super::{
    cards::{slash_commands, slash_list},
    fixtures::{chart_screenshot, tab_strip_screenshot},
};
use crate::story::{row, states};

pub const PLACEHOLDER: &str = "Message Claude Code…";

pub fn composer_input(value: &str, window: &mut Window, cx: &mut App) -> Entity<InputState> {
    let value = value.to_owned();
    cx.new(|cx| {
        InputState::new(window, cx)
            .placeholder(PLACEHOLDER)
            .auto_grow(1, 8)
            .submit_on_enter(true)
            .context_menu(true)
            .default_value(value)
    })
}

#[derive(Clone, Copy)]
pub enum Prefix {
    None,
    Queued(usize),
    Slash,
    Error(&'static str),
}

pub struct Draft {
    pub id: SharedString,
    pub input: Entity<InputState>,
    pub running: bool,
    pub writable: bool,
    pub images: Vec<Arc<Image>>,
    pub prefix: Vec<AnyElement>,
}

pub fn composer(draft: Draft, cx: &mut App) -> AnyElement {
    let id = draft.id;
    let value = draft.input.read(cx).value();
    let has_content = !value.trim().is_empty() || !draft.images.is_empty();
    let ready = draft.writable && !draft.running;
    let action = composer_action(draft.running, has_content);
    let enabled = match action {
        ComposerAction::Stop | ComposerAction::Queue => draft.writable,
        ComposerAction::Send => ready && has_content,
    };
    let attachments = (!draft.images.is_empty()).then(|| {
        h_flex()
            .w_full()
            .flex_wrap()
            .gap_2()
            .py_1()
            .children(draft.images.iter().enumerate().map(|(index, image)| {
                div()
                    .relative()
                    .child(agent_attachment_thumbnail(
                        (SharedString::from(format!("{id}-image")), index),
                        Arc::clone(image),
                        COMPOSER_ATTACHMENT,
                        cx,
                    ))
                    .child(
                        div().absolute().top(px(-6.0)).right(px(-6.0)).child(
                            Button::compact_icon(
                                (SharedString::from(format!("{id}-remove-image")), index),
                                IconName::Xmark,
                            )
                            .tooltip("Remove this image"),
                        ),
                    )
            }))
            .into_any_element()
    });
    let leading = composer_add_button(
        SharedString::from(format!("{id}-mention")),
        "Mention a host, session or file",
        cx,
    )
    .into_any_element();
    let command_hint = active_command_hint(&value, &pane_commands(&[], true)).map(Into::into);
    AgentComposer {
        composer: Composer {
            input: draft.input,
            leading: Some(leading),
            usage: Some(context_usage_meter(
                SharedString::from(format!("{id}-usage")),
                68_000,
                200_000,
                cx,
            )),
            action: composer_action_button(
                SharedString::from(format!("{id}-action")),
                action,
                enabled,
                cx,
            )
            .into_any_element(),
            attachments,
            command_hint,
            ground: None,
        },
        git: Some(git_summary_footer(
            SharedString::from(format!("{id}-git")),
            Some("fix/pinned-tab-width".into()),
            3,
            42,
            17,
            cx,
        )),
        prefix: draft.prefix,
    }
    .into_any_element()
}

pub fn queue_chip(id: SharedString, queued: usize, enabled: bool, cx: &App) -> AnyElement {
    h_flex()
        .w_full()
        .justify_end()
        .child(
            Button::new(id)
                .ghost()
                .small()
                .icon(IconName::Undo2)
                .label(format!("{queued} queued"))
                .tooltip("Return the queued prompts to the composer")
                .text_color(cx.theme().foreground.muted())
                .disabled(!enabled),
        )
        .into_any_element()
}

struct Case {
    label: &'static str,
    input: Entity<InputState>,
    running: bool,
    writable: bool,
    images: Vec<Arc<Image>>,
    prefix: Prefix,
}

pub struct Composers {
    cases: Vec<Case>,
}

pub fn composers(window: &mut Window, cx: &mut App) -> AnyView {
    let mut case = |label, value: &str, running, prefix, cx: &mut App| Case {
        label,
        input: composer_input(value, window, cx),
        running,
        writable: true,
        images: Vec::new(),
        prefix,
    };
    let mut cases = vec![
        case("idle, empty: Send disabled", "", false, Prefix::None, cx),
        case(
            "typing: Send",
            "Give pinned tabs a floor at their icon width.",
            false,
            Prefix::None,
            cx,
        ),
        case(
            "long draft grows to eight rows",
            "Give pinned tabs a floor at their icon width.\n\nThen:\n- keep the drop preview under the pointer\n- add a test with thirty tabs, three pinned\n- update the tab strip notes\n- run the browser suite\n- open a pull request against main\n- and ping me when CI is green",
            false,
            Prefix::None,
            cx,
        ),
        case("turn running, empty: Stop", "", true, Prefix::None, cx),
        case(
            "turn running, with text: Queue",
            "Also update the docs when you are done.",
            true,
            Prefix::None,
            cx,
        ),
        case(
            "prompts queued behind the turn",
            "",
            true,
            Prefix::Queued(2),
            cx,
        ),
        case("command argument hint", "/rewind ", false, Prefix::None, cx),
        case(
            "slash menu above the input",
            "/re",
            false,
            Prefix::Slash,
            cx,
        ),
        case(
            "error above the input",
            "Switch to plan mode",
            false,
            Prefix::Error("Timed out applying agent settings."),
            cx,
        ),
    ];
    let mut attached = case(
        "with attachments",
        "This is with thirty tabs open.",
        false,
        Prefix::None,
        cx,
    );
    attached.images = vec![tab_strip_screenshot(), chart_screenshot()];
    cases.insert(5, attached);
    let mut read_only = case("read-only attach", "", false, Prefix::None, cx);
    read_only.writable = false;
    cases.push(read_only);
    cx.new(|_| Composers { cases }).into()
}

impl Render for Composers {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut states = states();
        for (index, case) in self.cases.iter().enumerate() {
            let id = SharedString::from(format!("composer-{index}"));
            let prefix = match case.prefix {
                Prefix::None => Vec::new(),
                Prefix::Queued(queued) => vec![queue_chip(
                    SharedString::from(format!("{id}-unqueue")),
                    queued,
                    case.writable,
                    cx,
                )],
                Prefix::Slash => vec![slash_list(
                    &format!("{id}-slash"),
                    &slash_commands("re"),
                    Some(0),
                    cx,
                )],
                Prefix::Error(error) => vec![error_card(error, false, cx).into_any_element()],
            };
            let element = composer(
                Draft {
                    id,
                    input: case.input.clone(),
                    running: case.running,
                    writable: case.writable,
                    images: case.images.clone(),
                    prefix,
                },
                cx,
            );
            states = states.state(
                case.label,
                div()
                    .w_full()
                    .max_w(px(COMPOSER_MAX_WIDTH + 2.0 * COMPOSER_OUTER_PADDING))
                    .pt(px(COMPOSER_OUTER_PADDING))
                    .child(element),
            );
        }
        states
    }
}

pub struct Controls;

pub fn controls(_: &mut Window, cx: &mut App) -> AnyView {
    cx.new(|_| Controls).into()
}

impl Render for Controls {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let usage = [
            ("empty", 0, 200_000),
            ("a quarter", 50_000, 200_000),
            ("half", 100_000, 200_000),
            ("nearly full", 186_000, 200_000),
            ("full", 200_000, 200_000),
            ("size unknown", 12_000, 0),
        ]
        .into_iter()
        .enumerate()
        .fold(
            states().columns(6),
            |states, (index, (label, used, size))| {
                states.state(
                    label,
                    row().child(context_usage_meter(("usage", index), used, size, cx)),
                )
            },
        );
        let git = states()
            .columns(2)
            .state(
                "branch with changes",
                git_summary_footer(
                    "git-branch",
                    Some("fix/pinned-tab-width".into()),
                    3,
                    42,
                    17,
                    cx,
                ),
            )
            .state(
                "one file",
                git_summary_footer("git-one", Some("main".into()), 1, 2, 0, cx),
            )
            .state(
                "detached HEAD",
                git_summary_footer("git-detached", None, 12, 380, 96, cx),
            )
            .state(
                "long branch name",
                git_summary_footer(
                    "git-long",
                    Some("demfabris/browser-pinned-tabs-keep-their-icon-width-at-any-count".into()),
                    4,
                    1_204,
                    3_388,
                    cx,
                ),
            );
        states()
            .state(
                "action: Send, Send disabled, Queue, Stop",
                row()
                    .child(composer_action_button(
                        "action-send",
                        ComposerAction::Send,
                        true,
                        cx,
                    ))
                    .child(composer_action_button(
                        "action-send-off",
                        ComposerAction::Send,
                        false,
                        cx,
                    ))
                    .child(composer_action_button(
                        "action-queue",
                        ComposerAction::Queue,
                        true,
                        cx,
                    ))
                    .child(composer_action_button(
                        "action-stop",
                        ComposerAction::Stop,
                        true,
                        cx,
                    )),
            )
            .state(
                "+ starts a mention, enabled and disabled",
                row()
                    .child(composer_add_button(
                        "mention-on",
                        "Mention a host, session or file",
                        cx,
                    ))
                    .child(
                        composer_add_button("mention-off", "Mention a host, session or file", cx)
                            .disabled(true),
                    ),
            )
            .state("context usage meter", usage)
            .state("git summary", git)
    }
}
