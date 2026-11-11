use std::sync::Arc;

use ala::{
    AnyElement, AnyView, App, AppContext as _, Context, Entity, Image, IntoElement,
    ParentElement as _, Render, SharedString, Styled as _, Window, div, px,
};
use penna::{
    ActiveTheme as _, Disableable as _, IconName,
    blocks::chat::{
        COMPOSER_ATTACHMENT, COMPOSER_MAX_WIDTH, COMPOSER_OUTER_PADDING, ChatComposer, Composer,
        composer_add_button, composer_ground,
    },
    components::button::Button,
    components::input::InputState,
    h_flex,
};
use zz_client::agent_completion::{active_command_hint, pane_commands};
use zz_client::agent_settings::{Choice, LensSettings};
use zz_ui::agent::{
    agent_attachment_thumbnail,
    controls::{
        ComposerAction, agent_mode_chip, agent_model_picker, composer_action,
        composer_action_button, context_usage_meter, git_summary_footer,
    },
    presentation::{agent_resume_welcome, error_card},
    resume::{folder_button, resume_button, resume_menu, resume_rows},
};

use super::{
    cards::{slash_commands, slash_list},
    fixtures::{chart_screenshot, tab_strip_screenshot},
};
use crate::stories::console::palette::chat_backend;
use crate::story::{row, states};
use zz_protocol::PaneId;
use zz_ui::{command::target_picker, target::TargetPicker};

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

#[derive(Clone)]
pub enum Prefix {
    None,
    Queued(usize),
    Slash,
    Error(&'static str),
    Mention(Entity<TargetPicker>),
}

pub struct Draft {
    pub id: SharedString,
    pub input: Entity<InputState>,
    pub running: bool,
    pub writable: bool,
    pub images: Vec<Arc<Image>>,
    pub prefix: Vec<AnyElement>,
    pub settings: LensSettings,
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
                        composer_ground(cx.theme().ground(), cx),
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
    let ground = cx.theme().ground();
    let inside = composer_ground(ground, cx);
    let leading = composer_add_button(
        SharedString::from(format!("{id}-mention")),
        "Mention a chat, host, session or file",
        inside,
    )
    .into_any_element();
    let command_hint = active_command_hint(&value, &pane_commands(&[], true)).map(Into::into);
    ChatComposer {
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
                inside,
            )
            .into_any_element(),
            attachments,
            command_hint,
            ask: None,
            secret: false,
            ground,
        },
        footer: vec![git_summary_footer(
            SharedString::from(format!("{id}-git")),
            Some("fix/pinned-tab-width".into()),
            3,
            42,
            17,
            cx,
        )],
        prefix: draft.prefix,
        controls: footer_controls(&id, &draft.settings, draft.writable, cx),
    }
    .into_any_element()
}

pub fn claude_settings() -> LensSettings {
    let choices = |list: &[(&str, &str)]| {
        list.iter()
            .map(|(value, name)| Choice {
                value: (*value).to_owned(),
                name: (*name).to_owned(),
            })
            .collect::<Vec<_>>()
    };
    LensSettings {
        model: "opus".to_owned(),
        model_label: "Opus 5.5".to_owned(),
        models: choices(&[
            ("default", "Default"),
            ("opus", "Opus 5.5"),
            ("fable", "Fable 5.2"),
            ("sonnet", "Sonnet 5.5"),
            ("haiku", "Haiku 5.5"),
            ("fable-5-1", "Fable 5.1"),
            ("haiku-4-5", "Haiku 4.5"),
            ("sonnet-5", "Sonnet 5"),
            ("opus-5", "Opus 5"),
            ("fable-5", "Fable 5"),
            ("opus-4-8", "Opus 4.8"),
            ("opus-4-7", "Opus 4.7"),
            ("opus-4-6", "Opus 4.6"),
            ("sonnet-4-6", "Sonnet 4.6"),
        ]),
        effort: "high".to_owned(),
        efforts: choices(&[
            ("default", "Default"),
            ("low", "Low"),
            ("medium", "Medium"),
            ("high", "High"),
            ("xhigh", "Xhigh"),
            ("max", "Max"),
        ]),
        mode: "default".to_owned(),
        modes: choices(&[
            ("default", "Ask"),
            ("acceptEdits", "Accept edits"),
            ("plan", "Plan"),
            ("auto", "Auto"),
            ("bypassPermissions", "Bypass permissions"),
        ]),
    }
}

pub fn codex_settings() -> LensSettings {
    let choices = |list: &[(&str, &str)]| {
        list.iter()
            .map(|(value, name)| Choice {
                value: (*value).to_owned(),
                name: (*name).to_owned(),
            })
            .collect::<Vec<_>>()
    };
    LensSettings {
        model: "gpt-6.1-sol".to_owned(),
        model_label: "GPT-6.1-Sol".to_owned(),
        models: choices(&[
            ("gpt-6.1-sol", "GPT-6.1-Sol"),
            ("gpt-6-astra", "GPT-6-Astra"),
            ("gpt-6.1-terra", "GPT-6.1 Terra"),
            ("gpt-6-luna", "GPT-6-Luna"),
        ]),
        effort: "xhigh".to_owned(),
        efforts: choices(&[
            ("low", "Low"),
            ("medium", "Medium"),
            ("high", "High"),
            ("xhigh", "Extra high"),
            ("max", "Max"),
            ("ultra", "Ultra"),
        ]),
        mode: "auto".to_owned(),
        modes: choices(&[
            ("read-only", "Read only"),
            ("auto", "Auto"),
            ("full-access", "Full access"),
        ]),
    }
}

pub fn footer_controls(
    id: &str,
    settings: &LensSettings,
    enabled: bool,
    cx: &App,
) -> Vec<AnyElement> {
    let provider = if settings.modes.iter().any(|mode| mode.value == "read-only") {
        zz_protocol::AgentProvider::Codex
    } else {
        zz_protocol::AgentProvider::ClaudeCode
    };
    agent_mode_chip(
        SharedString::from(format!("{id}-mode")),
        provider,
        settings,
        enabled,
        |_, _, _| {},
        cx,
    )
    .into_iter()
    .chain([agent_model_picker(
        SharedString::from(format!("{id}-settings")),
        settings,
        enabled,
        |_, _, _, _| {},
        cx,
    )])
    .collect()
}

pub fn resume(_: &mut Window, cx: &mut App) -> AnyView {
    crate::story::stateless(
        |window, cx| {
            use penna::{
                Selectable as _,
                components::panel::{Panel, PanelLook},
            };
            use zz_client::agent_resume::{Menu, NOTHING_MATCHES, OTHER_FOLDERS, ResumeRow};
            let mut search = |id: &'static str, menu: Menu, text: &'static str, cx: &mut App| {
                window.use_keyed_state(id, cx, |window, cx| {
                    InputState::new(window, cx)
                        .placeholder(menu.placeholder())
                        .default_value(text)
                })
            };
            let searches = [
                search("resume-search-folders", Menu::Folders, "", cx),
                search("resume-search-chats", Menu::Chats, "", cx),
                search("resume-search-found", Menu::Chats, "clair", cx),
                search("resume-search-loading", Menu::Chats, "", cx),
                search("resume-search-none", Menu::Chats, "zzz", cx),
            ];
            let summary =
                |title: &str, place: &str| zz_protocol::agent_stream::AgentSessionSummary {
                    session_id: title.to_owned(),
                    cwd: format!("/Users/me/dev/{place}").into(),
                    additional_directories: Vec::new(),
                    title: Some(title.to_owned()),
                    updated_at: None,
                };
            let chat = |title: &str, place: Option<&str>, age: &str| ResumeRow::Chat {
                chat: summary(title, place.unwrap_or("zebrilus")),
                title: title.to_owned(),
                place: place.map(str::to_owned),
                age: Some(age.to_owned()),
            };
            let folder = |name: &str, path: &str, age: Option<&str>| ResumeRow::Folder {
                cwd: format!("/Users/me/dev/{name}").into(),
                name: name.to_owned(),
                path: path.to_owned(),
                age: age.map(str::to_owned),
                current: age.is_none(),
            };
            let buttons = |id: u64, open: Option<Menu>, cx: &App| {
                h_flex()
                    .flex_none()
                    .gap(px(6.0))
                    .child(
                        folder_button(("story-folder-button", id), "zebrilus", cx)
                            .selected(open == Some(Menu::Folders)),
                    )
                    .child(
                        resume_button(("story-resume-button", id), cx)
                            .selected(open == Some(Menu::Chats)),
                    )
            };
            let screen = |id: u64, open: Option<(Menu, &[ResumeRow], Option<usize>)>, cx: &App| {
                let menu = open.map(|(_, rows, selected)| {
                    let search = &searches[usize::try_from(id).unwrap_or_default() - 1];
                    let searching = !search.read(cx).value().is_empty();
                    Panel::new(("story-resume-menu", id), PanelLook::Popover)
                        .flex_none()
                        .mt(px(4.0))
                        .child(resume_menu(
                            search,
                            &ala::ScrollHandle::new(),
                            resume_rows(
                                id,
                                rows,
                                selected,
                                searching,
                                |_, _, _| {},
                                |_, _, _| {},
                                cx,
                            ),
                            cx,
                        ))
                });
                penna::v_flex()
                    .w(px(440.0))
                    .items_center()
                    .child(agent_resume_welcome(
                        zz_protocol::AgentProvider::ClaudeCode,
                        cx,
                    ))
                    .child(buttons(id, open.map(|(menu, ..)| menu), cx))
                    .children(menu)
            };
            let folders = [
                folder("zebrilus", "~/dev/autistic-computer", None),
                folder("clairvo", "~/dev", Some("now")),
                folder("autistic-computer", "~/dev", Some("now")),
                folder("ppweb", "~/dev/PsicoPilot", Some("15h")),
                folder("nidus", "~/dev/autistic-computer", Some("2d")),
                folder("landing-page", "~/dev/autistic-computer", Some("3d")),
                folder("penna", "~/dev/autistic-computer", Some("4d")),
                folder("ala-cef", "~/dev/autistic-computer", Some("1w")),
                folder("dev", "~", Some("2w")),
                folder("pele-rs", "~/dev/autistic-computer", Some("2w")),
                ResumeRow::Loading,
            ];
            let chats = [
                ResumeRow::Label("In zebrilus".to_owned()),
                chat("Fix pane stack hit testing", None, "2h"),
                chat("Gantt bars in pele", None, "1d"),
                chat("Flip keeps the model and effort", None, "3d"),
                ResumeRow::Label(OTHER_FOLDERS.to_owned()),
                chat(
                    "Clairvo invoice import fails on empty rows",
                    Some("clairvo"),
                    "now",
                ),
                chat("Segmented focus ring", Some("penna"), "3h"),
                chat("Queue retries for the mailer", Some("clairvo"), "1d"),
                chat("Pointer warp on Wayland", Some("nidus"), "2d"),
                chat("Clairvo landing copy", Some("landing-page"), "3d"),
                chat("Slider knob ring on yaru", Some("penna"), "6d"),
                chat("Seed data for the demo tenant", Some("clairvo"), "6d"),
                chat("Docs site figure fences", Some("penna"), "1w"),
            ];
            let found = [
                chat(
                    "Clairvo invoice import fails on empty rows",
                    Some("clairvo"),
                    "now",
                ),
                chat("Clairvo landing copy", Some("landing-page"), "3d"),
                chat("Queue retries for the mailer", Some("clairvo"), "1d"),
                chat("Seed data for the demo tenant", Some("clairvo"), "6d"),
            ];
            states()
                .state("at rest: the welcome over the folder and Resume buttons", screen(0, None, cx))
                .state(
                    "the folder menu: this folder checked, then the others newest first, eight rows that scroll, the spinner while more pages arrive",
                    screen(1, Some((Menu::Folders, &folders, Some(0))), cx),
                )
                .state(
                    "Resume: this folder's chats, then the newest from other folders",
                    screen(2, Some((Menu::Chats, &chats, Some(1))), cx),
                )
                .state(
                    "searching chats: one flat result, the first row picked",
                    screen(3, Some((Menu::Chats, &found, Some(0))), cx),
                )
                .state(
                    "the first listing on its way",
                    screen(4, Some((Menu::Chats, &[ResumeRow::Loading], None)), cx),
                )
                .state(
                    "a search that matches nothing",
                    screen(
                        5,
                        Some((Menu::Chats, &[ResumeRow::Note(NOTHING_MATCHES)], None)),
                        cx,
                    ),
                )
                .into_any_element()
        },
        cx,
    )
}

pub fn queue_chip(id: SharedString, queued: usize, enabled: bool, cx: &App) -> AnyElement {
    penna::blocks::chat::queue_chip(id, queued, enabled, |_, _| {}, cx).into_any_element()
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
            "@ lists the other chats first",
            "Compare with @",
            false,
            Prefix::None,
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
    for case in &mut cases {
        if case.input.read(cx).value().ends_with('@') {
            let picker = target_picker(&chat_backend(), Some(case.input.clone()), cx);
            picker.update(cx, |picker, cx| {
                picker.set_own_chat(Some(PaneId(10)));
                picker.set_body("", cx);
            });
            case.prefix = Prefix::Mention(picker);
        }
    }
    cx.new(|_| Composers { cases }).into()
}

impl Render for Composers {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut states = states();
        for (index, case) in self.cases.iter().enumerate() {
            let id = SharedString::from(format!("composer-{index}"));
            let prefix = match &case.prefix {
                Prefix::None => Vec::new(),
                Prefix::Queued(queued) => vec![queue_chip(
                    SharedString::from(format!("{id}-unqueue")),
                    *queued,
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
                Prefix::Mention(picker) => vec![picker.clone().into_any_element()],
            };
            let element = composer(
                Draft {
                    id,
                    input: case.input.clone(),
                    running: case.running,
                    writable: case.writable,
                    images: case.images.clone(),
                    prefix,
                    settings: claude_settings(),
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
        let inside = composer_ground(cx.theme().ground(), cx);
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
                        inside,
                    ))
                    .child(composer_action_button(
                        "action-send-off",
                        ComposerAction::Send,
                        false,
                        inside,
                    ))
                    .child(composer_action_button(
                        "action-queue",
                        ComposerAction::Queue,
                        true,
                        inside,
                    ))
                    .child(composer_action_button(
                        "action-stop",
                        ComposerAction::Stop,
                        true,
                        inside,
                    )),
            )
            .state(
                "+ starts a mention, enabled and disabled",
                row()
                    .child(composer_add_button(
                        "mention-on",
                        "Mention a host, session or file",
                        inside,
                    ))
                    .child(
                        composer_add_button(
                            "mention-off",
                            "Mention a host, session or file",
                            inside,
                        )
                        .disabled(true),
                    ),
            )
            .state("context usage meter", usage)
            .state("git summary", git)
            .state(
                "permissions, model and effort: Claude Code's own lists",
                row().children(footer_controls("claude", &claude_settings(), true, cx)),
            )
            .state(
                "the same controls on Codex, with the modes, models and levels it lists",
                row().children(footer_controls("codex", &codex_settings(), true, cx)),
            )
            .state(
                "a mode that acts without asking tints its chip",
                row().children(footer_controls(
                    "claude-auto",
                    &LensSettings {
                        mode: "acceptEdits".to_owned(),
                        ..claude_settings()
                    },
                    true,
                    cx,
                )),
            )
            .state(
                "Bypass permissions: nothing guards it, so the chip turns danger",
                row().children(footer_controls(
                    "claude-bypass",
                    &LensSettings {
                        mode: "bypassPermissions".to_owned(),
                        ..claude_settings()
                    },
                    true,
                    cx,
                )),
            )
            .state(
                "an older model picked: the list opens scrolled to it",
                row().children(footer_controls(
                    "claude-older",
                    &LensSettings {
                        model: "opus-4-8".to_owned(),
                        model_label: "Opus 4.8".to_owned(),
                        ..claude_settings()
                    },
                    true,
                    cx,
                )),
            )
    }
}
