use std::{
    hash::{DefaultHasher, Hash as _, Hasher as _},
    rc::Rc,
};

use ala::{
    AnyElement, App, ElementId, Entity, IntoElement, Pixels, SharedString, Window, div, prelude::*,
    px,
};
use penna::blocks::chat::{
    ASK_GAP, AskHint, AskOption, Composer, ask_body, ask_count, ask_divider, ask_header, ask_hints,
    ask_inset, ask_key, ask_leading, ask_note, ask_options, ask_send, ask_sheet, ask_step,
    transcript_text,
};
use zz_client::agent_ask::{
    ApprovalTopic, Hint, KEYED_OPTIONS, approval_hints, approval_send, approval_topic,
};
use zz_client::agent_transcript::AgentPermissionKind;
use zz_protocol::chat::ChatView;

use super::{agent_text_view, chat::kind_icon};
use crate::{
    Disableable as _, Ground, IconName, h_flex, input::InputState, text::TextView, v_flex,
};

const PLAN_HEIGHT: f32 = 152.0;
const ASK_MIN_HEIGHT: f32 = 160.0;

pub struct AskParts {
    pub body: AnyElement,
    pub input: Option<Entity<InputState>>,
    pub leading: Option<AnyElement>,
    pub trailing: Vec<AnyElement>,
    pub hints: Vec<AskHint>,
    pub secret: bool,
}

pub(super) fn ask_hint(hint: Hint) -> AskHint {
    if hint.strong {
        AskHint::strong(hint.text)
    } else {
        AskHint::new(hint.text)
    }
}

pub struct PromptRow {
    pub input: Entity<InputState>,
    pub add: AnyElement,
    pub action: AnyElement,
    pub attachments: Option<AnyElement>,
    pub command_hint: Option<SharedString>,
}

impl AskParts {
    pub fn composer(
        self,
        id: impl Into<ElementId>,
        room: Pixels,
        prompt: PromptRow,
        ground: Ground,
    ) -> (Composer, AnyElement) {
        let typing = self.input.is_some();
        let body = div()
            .id(id)
            .w_full()
            .flex_none()
            .max_h((room * 0.5).max(px(ASK_MIN_HEIGHT)))
            .overflow_y_scroll()
            .child(self.body);
        let composer = Composer {
            input: self.input.unwrap_or(prompt.input),
            leading: if typing {
                self.leading
            } else {
                Some(prompt.add)
            },
            usage: Some(
                h_flex()
                    .flex_none()
                    .gap(px(ASK_GAP))
                    .children(self.trailing)
                    .into_any_element(),
            ),
            action: prompt.action,
            attachments: prompt.attachments.filter(|_| !typing),
            command_hint: prompt.command_hint.filter(|_| !typing),
            ask: Some(body.into_any_element()),
            secret: self.secret,
            ground,
        };
        (composer, ask_hints(self.hints, ground).into_any_element())
    }
}

pub enum AskSubject {
    Tool {
        icon: IconName,
        title: SharedString,
        inset: Option<SharedString>,
        note: Option<SharedString>,
    },
    Plan {
        markdown: SharedString,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AskChoice {
    pub label: SharedString,
    pub rejects: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApprovalAction {
    Pick(usize),
    Send,
    Open,
}

#[must_use]
pub const fn rejects(kind: AgentPermissionKind) -> bool {
    matches!(
        kind,
        AgentPermissionKind::RejectOnce | AgentPermissionKind::RejectAlways
    )
}

#[must_use]
pub fn approval_subject(
    chat: Option<&ChatView>,
    request_id: u64,
    title: &str,
    cwd: Option<&str>,
) -> AskSubject {
    match approval_topic(chat, request_id, title, cwd) {
        ApprovalTopic::Plan { markdown } => AskSubject::Plan {
            markdown: markdown.into(),
        },
        ApprovalTopic::Tool {
            kind,
            heading,
            inset,
            note,
        } => AskSubject::Tool {
            icon: kind.map_or(IconName::Info, kind_icon),
            title: heading.into(),
            inset: inset.map(Into::into),
            note: note.map(Into::into),
        },
    }
}

pub struct Approval<'a> {
    pub id: &'a str,
    pub choices: Vec<AskChoice>,
    pub subject: AskSubject,
    pub counter: Option<SharedString>,
    pub highlighted: usize,
    pub enabled: bool,
    pub reply: Option<Entity<InputState>>,
    pub replying: bool,
    pub owner: Option<u64>,
}

impl Approval<'_> {
    pub fn parts(
        self,
        on_action: impl Fn(ApprovalAction, &mut Window, &mut App) + 'static,
        on: Ground,
        cx: &App,
    ) -> AskParts {
        let on_action: Rc<dyn Fn(ApprovalAction, &mut Window, &mut App)> = Rc::new(on_action);
        let id = self.id;
        let enabled = self.enabled;
        let options = &self.choices;
        let reject = options.iter().position(|option| option.rejects);
        let plan = matches!(self.subject, AskSubject::Plan { .. });
        let counter = self.counter.map(|counter| ask_count(counter, on));
        let (header, detail) = match self.subject {
            AskSubject::Tool {
                icon,
                title,
                inset,
                note,
            } => (
                ask_header(icon, title, on, cx).children(counter),
                ask_body()
                    .children(inset.map(|inset| ask_inset(cx).child(inset)))
                    .children(note.map(|note| ask_note(note, on)))
                    .into_any_element(),
            ),
            AskSubject::Plan { markdown } => {
                let open = Rc::clone(&on_action);
                let scope = plan_scope(self.owner, id);
                let text = agent_text_view(
                    TextView::markdown(SharedString::from(format!("{id}-plan")), markdown),
                    scope,
                    self.owner,
                    cx,
                )
                .selectable(true)
                .scrollable(true)
                .h(px(PLAN_HEIGHT));
                (
                    ask_header(IconName::List, "Plan", on, cx)
                        .children(counter)
                        .child(
                            ask_step(
                                SharedString::from(format!("{id}-open")),
                                IconName::Expand,
                                "Open the plan in a view pane",
                                on,
                            )
                            .on_click(move |_, window, cx| {
                                open(ApprovalAction::Open, window, cx);
                                cx.stop_propagation();
                            }),
                        ),
                    ask_sheet(cx)
                        .child(transcript_text(div().w_full(), cx).child(text))
                        .into_any_element(),
                )
            }
        };
        let rows =
            ask_options().children(options.iter().enumerate().flat_map(|(index, option)| {
                let divider = (Some(index) == reject && index > 0)
                    .then(|| ask_divider(cx).into_any_element());
                let pick = Rc::clone(&on_action);
                let row = AskOption::new(
                    (SharedString::from(format!("{id}-option")), index),
                    option.label.clone(),
                    on,
                )
                .highlighted(index == self.highlighted)
                .enabled(enabled)
                .on_click(move |_, window, cx| {
                    pick(ApprovalAction::Pick(index), window, cx);
                    cx.stop_propagation();
                });
                let row = if index < KEYED_OPTIONS {
                    row.key((index + 1).to_string())
                } else {
                    row
                };
                divider.into_iter().chain([row.into_any_element()])
            }));
        let denying = options
            .get(self.highlighted)
            .is_some_and(|option| option.rejects);
        let label = approval_send(self.replying, denying, plan);
        let hints = approval_hints(options.len(), self.replying, plan, label)
            .into_iter()
            .map(ask_hint)
            .collect();
        let send = Rc::clone(&on_action);
        let leading = self
            .reply
            .as_ref()
            .filter(|_| options.len() < KEYED_OPTIONS)
            .map(|_| ask_leading(ask_key((options.len() + 1).to_string(), on)).into_any_element());
        AskParts {
            body: v_flex()
                .w_full()
                .child(header)
                .child(detail)
                .child(rows)
                .into_any_element(),
            input: self.reply,
            leading,
            trailing: vec![
                ask_send(SharedString::from(format!("{id}-send")), label, on)
                    .disabled(!enabled)
                    .on_click(move |_, window, cx| {
                        send(ApprovalAction::Send, window, cx);
                        cx.stop_propagation();
                    })
                    .into_any_element(),
            ],
            hints,
            secret: false,
        }
    }
}

fn plan_scope(owner: Option<u64>, id: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    owner.hash(&mut hasher);
    id.hash(&mut hasher);
    hasher.finish()
}
