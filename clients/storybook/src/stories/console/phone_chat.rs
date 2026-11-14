use std::sync::Arc;

use ala::{
    AnyElement, AnyView, App, AppContext as _, Context, Entity, Image, IntoElement,
    ParentElement as _, Render, SharedString, Styled as _, Window, div, prelude::*, px,
};
use penna::{
    ActiveTheme as _, Ground, IconName, Tone,
    blocks::chat::{
        ComposerAction, PhoneComposer, phone_action_button, phone_close_button, phone_plus_button,
        phone_queued_row, phone_thumbnail,
    },
    blocks::compact::{PillLabel, compact_floating_bar, compact_floating_pill},
    blocks::sheet::BottomSheet,
    blocks::touch::page_dots,
    components::input::InputState,
    v_flex,
};
use zz_protocol::{
    AgentProvider, AgentQuota, PaneId,
    chat::{ItemId, ItemKind},
};
use zz_ui::{
    agent::{
        ask::{AskChoice, AskSubject},
        chat::HeldMessage,
        phone::{PhoneCard, PlusPage, PlusSheet, held_sheet},
        question::QuestionCardState,
    },
    command::mention_picker,
    target::TargetPicker,
};

use super::palette::chat_backend;
use super::phone::{groups, phone, placement};
use crate::stories::agent::{
    chat::{Frame, Thread},
    composer::{Usage, claude_settings, composer_input},
    fixtures::{Script, checks_question, completed, option, question, tab_strip_screenshot},
};
use crate::story::states;

const HEIGHT: f32 = 720.0;
const PHONE_HEIGHT: f32 = 852.0;
const DOCK: f32 = 16.0;
const GAP: f32 = 8.0;
const TITLE: &str = "Make the pill menu offer New chat";
const WHERE: &str = "zebrilus · main";
const ASKED: &str = "Make the pill menu offer New chat, nearest the thumb.";
const PLAN: &str = "The order lives in `pill_menu.rs`. I'll put New chat last in the New group, where it sits nearest the thumb.";
const DONE: &str = "Done. New chat sits nearest the thumb, and the pill tests pass.";

fn rest_script() -> Script {
    let mut script = Script::new();
    script
        .turn(ASKED)
        .reply("p1", PLAN)
        .ran("r1", ItemKind::Read, "crates/zz-ui/src/compact/menus.rs")
        .ran("e1", ItemKind::Edit, "crates/zz-ui/src/compact/menus.rs")
        .ran("x1", ItemKind::Execute, "cargo test -p penna pill")
        .reply("d1", DONE)
        .finish(completed());
    script
}

fn working_script() -> Script {
    let mut script = Script::new();
    script
        .turn(ASKED)
        .reply("p1", PLAN)
        .ran("r1", ItemKind::Read, "crates/zz-ui/src/compact/menus.rs")
        .ran("e1", ItemKind::Edit, "crates/zz-ui/src/compact/menus.rs")
        .tool("x1", None, ItemKind::Execute, "cargo test -p penna pill");
    script
}

fn thread(script: Script, window: &mut Window, cx: &mut App) -> Entity<Thread> {
    Thread::new(script)
        .framed(Frame::Scroll(None))
        .touch()
        .view(window, cx)
}

fn ground(cx: &App) -> Ground {
    cx.theme().ground()
}

fn inside(cx: &App) -> Ground {
    penna::blocks::chat::composer_ground(ground(cx), cx)
}

fn pill(
    id: &'static str,
    word: &'static str,
    meta: &'static str,
    tone: Tone,
    cx: &App,
) -> AnyElement {
    let placement = placement();
    let pill = compact_floating_pill(
        id,
        PillLabel {
            icon: IconName::Chat,
            face: None,
            title: TITLE.into(),
            word: word.into(),
            meta: meta.into(),
            tone,
        },
        page_dots(&groups(), cx),
        &placement,
        cx,
    );
    compact_floating_bar(pill, &placement, cx).into_any_element()
}

fn rest_pill(id: &'static str, cx: &App) -> AnyElement {
    pill(id, "", WHERE, Tone::Neutral, cx)
}

struct Screen {
    id: &'static str,
    transcript: AnyElement,
    dock: Vec<AnyElement>,
    pill: Option<AnyElement>,
    sheet: Option<BottomSheet>,
}

impl Screen {
    fn new(id: &'static str, transcript: impl IntoElement) -> Self {
        Self {
            id,
            transcript: transcript.into_any_element(),
            dock: Vec::new(),
            pill: None,
            sheet: None,
        }
    }

    fn dock(mut self, element: impl IntoElement) -> Self {
        self.dock.push(element.into_any_element());
        self
    }

    fn pill(mut self, pill: AnyElement) -> Self {
        self.pill = Some(pill);
        self
    }

    fn sheet(mut self, sheet: BottomSheet) -> Self {
        self.sheet = Some(sheet);
        self
    }

    fn draw(self, cx: &App) -> ala::Div {
        let keyboard = self.pill.is_none();
        phone(
            div()
                .id(self.id)
                .relative()
                .h(px(HEIGHT))
                .overflow_hidden()
                .flex()
                .flex_col()
                .child(div().flex_1().min_h_0().child(self.transcript))
                .child(
                    v_flex()
                        .flex_none()
                        .gap(px(GAP))
                        .px(px(DOCK))
                        .when(keyboard, |dock| dock.pb(px(GAP)))
                        .children(self.dock),
                )
                .children(self.pill)
                .children(self.sheet),
            cx,
        )
    }
}

fn composer(
    id: &'static str,
    input: &Entity<InputState>,
    action: ComposerAction,
    enabled: bool,
    thumbnails: Vec<AnyElement>,
    cx: &App,
) -> PhoneComposer {
    let on = inside(cx);
    PhoneComposer {
        input: input.clone(),
        leading: phone_plus_button((id, 0usize), on).into_any_element(),
        action: phone_action_button((id, 1usize), action, enabled, on).into_any_element(),
        thumbnails,
        above: None,
        ground: ground(cx),
    }
}

pub(super) struct ChatStates {
    rest: Entity<Thread>,
    working: Entity<Thread>,
    empty: Entity<InputState>,
    typed: Entity<InputState>,
    queued: Entity<InputState>,
    image: Entity<InputState>,
    other: Entity<InputState>,
    shot: Arc<Image>,
}

pub(super) fn chat_states(window: &mut Window, cx: &mut App) -> AnyView {
    let view = ChatStates {
        rest: thread(rest_script(), window, cx),
        working: thread(working_script(), window, cx),
        empty: composer_input("", window, cx),
        typed: composer_input("Also open the menu story so I can look at it", window, cx),
        queued: composer_input("Then the swipe test", window, cx),
        image: composer_input("This is the menu now", window, cx),
        other: composer_input("Ask me first, then embed", window, cx),
        shot: tab_strip_screenshot(),
    };
    cx.new(|_| view).into()
}

impl Render for ChatStates {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let on = inside(cx);
        let rest = Screen::new("chat-rest", self.rest.clone())
            .dock(composer(
                "chat-rest-row",
                &self.empty,
                ComposerAction::Send,
                false,
                Vec::new(),
                cx,
            ))
            .pill(rest_pill("chat-rest-pill", cx));
        let working = Screen::new("chat-working", self.working.clone())
            .dock(phone_queued_row(
                "chat-working-queued",
                "2 queued",
                true,
                |_, _| {},
                cx,
            ))
            .dock(composer(
                "chat-working-row",
                &self.empty,
                ComposerAction::Stop,
                true,
                Vec::new(),
                cx,
            ))
            .pill(pill(
                "chat-working-pill",
                "",
                "Running cargo test -p penna pill",
                Tone::Neutral,
                cx,
            ));
        let queueing = Screen::new("chat-queue", self.working.clone())
            .dock(composer(
                "chat-queue-row",
                &self.queued,
                ComposerAction::Queue,
                true,
                Vec::new(),
                cx,
            ))
            .pill(pill(
                "chat-queue-pill",
                "",
                "Running cargo test -p penna pill",
                Tone::Neutral,
                cx,
            ));
        let typing = Screen::new("chat-typing", self.rest.clone()).dock(composer(
            "chat-typing-row",
            &self.typed,
            ComposerAction::Send,
            true,
            Vec::new(),
            cx,
        ));
        let shot = Arc::clone(&self.shot);
        let image = Screen::new("chat-image", self.rest.clone())
            .dock(composer(
                "chat-image-row",
                &self.image,
                ComposerAction::Send,
                true,
                vec![phone_thumbnail("chat-image-thumb", shot, |_, _| {}, cx)],
                cx,
            ))
            .pill(rest_pill("chat-image-pill", cx));
        let other = Screen::new("chat-other", self.rest.clone()).dock(PhoneComposer {
            input: self.other.clone(),
            leading: phone_close_button("chat-other-close", on).into_any_element(),
            action: phone_action_button("chat-other-send", ComposerAction::Send, true, on)
                .into_any_element(),
            thumbnails: Vec::new(),
            above: Some("Should the daemon embed the tailnet node, or talk to tailscaled?".into()),
            ground: ground(cx),
        });
        states()
            .columns(2)
            .state("resting: send off while the field is empty", rest.draw(cx))
            .state(
                "working: stop, the running step spins, 2 queued",
                working.draw(cx),
            )
            .state("typing while it works: queue", queueing.draw(cx))
            .state(
                "keyboard up: the composer takes the pill's place, 8 above the keyboard",
                typing.draw(cx),
            )
            .state("a pasted image", image.draw(cx))
            .state(
                "Something else: the question over the field, close goes back to the answers",
                other.draw(cx),
            )
    }
}

pub(super) struct AskStates {
    approval: Entity<Thread>,
    question: Entity<Thread>,
    one_of_two: QuestionCardState,
    checks: QuestionCardState,
}

fn ask_script(tool: bool) -> Script {
    let mut script = Script::new();
    if tool {
        script
            .turn(ASKED)
            .reply("p1", PLAN)
            .ran("r1", ItemKind::Read, "crates/zz-ui/src/compact/menus.rs")
            .ran("e1", ItemKind::Edit, "crates/zz-ui/src/compact/menus.rs");
    } else {
        script
            .turn("Pick a tailnet library for zz.")
            .reply("p1", "Two fit, and they differ in who runs the node.")
            .ran("r1", ItemKind::Read, "knowledge/designs/tailnet-node.md");
    }
    script
}

impl AskStates {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let tailnet = question(
            "q0",
            "Should the daemon embed the tailnet node, or talk to tailscaled?",
            vec![
                option("Embed it", Some("One process, no system service")),
                option(
                    "Talk to tailscaled",
                    Some("Uses the node the machine already runs"),
                ),
            ],
        );
        let port = question(
            "q1",
            "Which port should it listen on?",
            vec![option("41641", None), option("Any free port", None)],
        );
        let one_of_two =
            QuestionCardState::new(7, vec![tailnet, port], window, cx, |_, _, _, _, _| {});
        let mut checks =
            QuestionCardState::new(8, vec![checks_question()], window, cx, |_, _, _, _, _| {});
        checks.card.pick(0, 0);
        checks.card.pick(0, 1);
        Self {
            approval: thread(ask_script(true), window, cx),
            question: thread(ask_script(false), window, cx),
            one_of_two,
            checks,
        }
    }
}

pub(super) fn ask_states(window: &mut Window, cx: &mut App) -> AnyView {
    cx.new(|cx| AskStates::new(window, cx)).into()
}

fn card(id: &'static str, cx: &App) -> PhoneCard {
    PhoneCard {
        id: id.into(),
        counter: None,
        enabled: true,
        expanded: false,
        ground: ground(cx),
        max_height: px(PHONE_HEIGHT / 2.0),
    }
}

impl Render for AskStates {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let choices = ["Allow once", "Always allow cargo test", "Deny"]
            .into_iter()
            .map(|label| AskChoice {
                label: label.into(),
                rejects: label == "Deny",
            })
            .collect::<Vec<_>>();
        let subject = || AskSubject::Tool {
            icon: IconName::SquareTerminal,
            title: "Run this command?".into(),
            inset: Some("cargo test -p penna pill_menu --all-features".into()),
            note: None,
        };
        let approval = card("ask-approval-card", cx).approval(subject(), &choices, |_, _, _| {});
        let opened = PhoneCard {
            expanded: true,
            ..card("ask-opened-card", cx)
        }
        .approval(subject(), &choices, |_, _, _| {});
        let question = card("ask-question-card", cx)
            .question(&self.one_of_two, |_, _, _| {})
            .map_or_else(|| div().into_any_element(), IntoElement::into_any_element);
        let checks = card("ask-checks-card", cx)
            .question(&self.checks, |_, _, _| {})
            .map_or_else(|| div().into_any_element(), IntoElement::into_any_element);
        let waiting = |id, word| pill(id, word, "", Tone::Warning, cx);
        states()
            .columns(2)
            .state(
                "needs approval: the card takes the composer's place",
                Screen::new("ask-approval", self.approval.clone())
                    .dock(approval)
                    .pill(waiting("ask-approval-pill", "Needs approval"))
                    .draw(cx),
            )
            .state(
                "a tap on what it would run shows all of it",
                Screen::new("ask-opened", self.approval.clone())
                    .dock(opened)
                    .pill(waiting("ask-opened-pill", "Needs approval"))
                    .draw(cx),
            )
            .state(
                "has a question: answers with their detail, Something else last",
                Screen::new("ask-question", self.question.clone())
                    .dock(question)
                    .pill(waiting("ask-question-pill", "Has a question"))
                    .draw(cx),
            )
            .state(
                "several answers: checks, then Send 2 answers",
                Screen::new("ask-checks", self.question.clone())
                    .dock(checks)
                    .pill(waiting("ask-checks-pill", "Has a question"))
                    .draw(cx),
            )
    }
}

pub(super) struct PlusStates {
    thread: Entity<Thread>,
    input: Entity<InputState>,
    top: Entity<TargetPicker>,
    inside: Entity<TargetPicker>,
}

impl PlusStates {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let picker = |window: &mut Window, cx: &mut Context<Self>| {
            let input = composer_input("Compare with ", window, cx);
            let picker = mention_picker(&chat_backend(), input.clone(), window, cx);
            picker.update(cx, |picker, cx| {
                picker.set_own_chat(Some(PaneId(10)));
                picker.open_here(window, cx);
                picker.sync(cx);
            });
            picker
        };
        let top = picker(window, cx);
        let inside = picker(window, cx);
        let session = inside
            .read(cx)
            .items()
            .iter()
            .find(|item| item.kind == zz_ui::target::TargetKind::Session)
            .cloned();
        if let Some(session) = session {
            inside.update(cx, |picker, cx| {
                picker.drill(&session, window, cx);
                picker.sync(cx);
            });
        }
        Self {
            thread: thread(rest_script(), window, cx),
            input: composer_input("", window, cx),
            top,
            inside,
        }
    }
}

pub(super) fn plus_states(window: &mut Window, cx: &mut App) -> AnyView {
    cx.new(|cx| PlusStates::new(window, cx)).into()
}

impl Render for PlusStates {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let settings = claude_settings();
        let usage = Usage::near_a_limit();
        let quotas = usage
            .quota
            .map(|windows| AgentQuota {
                provider: AgentProvider::ClaudeCode,
                account: usage.current.map(str::to_owned),
                windows,
            })
            .into_iter()
            .collect::<Vec<_>>();
        let accounts = Usage::two_with_the_second_picked().accounts;
        let top = self.top.read(cx).items().to_vec();
        let inside = self.inside.read(cx).items().to_vec();
        let drilled = self
            .top
            .read(cx)
            .items()
            .iter()
            .find(|item| item.kind == zz_ui::target::TargetKind::Session)
            .cloned();
        let bottom = placement().bottom;
        let sheet = |page: PlusPage, mention: &[zz_ui::target::TargetItem], cx: &App| {
            PlusSheet {
                agent: AgentProvider::ClaudeCode.label().into(),
                provider: Some(AgentProvider::ClaudeCode),
                settings: Some(&settings),
                account: usage.current,
                accounts: accounts.as_deref(),
                quotas: &quotas,
                context: usage.context,
                mention,
                panes: &[],
                enabled: true,
            }
            .sheet(&page, bottom, |_, _, _| {}, |_, _| {}, cx)
        };
        let screen = |id: &'static str, sheet: BottomSheet, cx: &App| {
            Screen::new(id, self.thread.clone())
                .dock(composer(
                    "plus-row",
                    &self.input,
                    ComposerAction::Send,
                    false,
                    Vec::new(),
                    cx,
                ))
                .pill(rest_pill("plus-pill", cx))
                .sheet(sheet)
                .draw(cx)
        };
        let cx: &App = cx;
        states()
            .columns(2)
            .state(
                "the plus: Mention, the chat's settings, the use; a limit at 80% in warning ink",
                screen("plus-root", sheet(PlusPage::Root, &top, cx), cx),
            )
            .state(
                "Model",
                screen(
                    "plus-model",
                    sheet(
                        PlusPage::Setting(zz_client::agent_settings::MODEL),
                        &top,
                        cx,
                    ),
                    cx,
                ),
            )
            .state(
                "Effort",
                screen(
                    "plus-effort",
                    sheet(
                        PlusPage::Setting(zz_client::agent_settings::EFFORT),
                        &top,
                        cx,
                    ),
                    cx,
                ),
            )
            .state(
                "Permissions: each mode with its glyph",
                screen(
                    "plus-mode",
                    sheet(PlusPage::Setting(zz_client::agent_settings::MODE), &top, cx),
                    cx,
                ),
            )
            .state(
                "Account, then Add account",
                screen("plus-account", sheet(PlusPage::Account, &top, cx), cx),
            )
            .state(
                "Mention: chats, sessions, hosts, files",
                screen(
                    "plus-mention",
                    sheet(PlusPage::Mention(Vec::new()), &top, cx),
                    cx,
                ),
            )
            .state(
                "inside a session: the session itself first",
                screen(
                    "plus-mention-inside",
                    sheet(
                        PlusPage::Mention(drilled.into_iter().collect()),
                        &inside,
                        cx,
                    ),
                    cx,
                ),
            )
    }
}

pub(super) struct HoldState {
    thread: Entity<Thread>,
    input: Entity<InputState>,
}

pub(super) fn hold_state(window: &mut Window, cx: &mut App) -> AnyView {
    let view = HoldState {
        thread: thread(rest_script(), window, cx),
        input: composer_input("", window, cx),
    };
    cx.new(|_| view).into()
}

impl Render for HoldState {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let held = HeldMessage {
            title: SharedString::new_static(ASKED),
            text: SharedString::new_static(ASKED),
            at: None,
            fork: Some(ItemId::new("u1")),
        };
        let sheet = held_sheet(&held, |_, _, _| {}, |_, _| {}, cx);
        states().state(
            "hold a message: Copy and Fork from here",
            Screen::new("hold-message", self.thread.clone())
                .dock(composer(
                    "hold-row",
                    &self.input,
                    ComposerAction::Send,
                    false,
                    Vec::new(),
                    cx,
                ))
                .pill(rest_pill("hold-pill", cx))
                .sheet(sheet)
                .draw(cx),
        )
    }
}
