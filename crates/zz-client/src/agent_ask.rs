use serde::Serialize;
use zz_protocol::chat::{Ask, ChatView, ItemKind, RequestId};

use crate::agent_question::QuestionCard;

pub const KEYED_OPTIONS: usize = 9;
pub const DENY_PLACEHOLDER: &str = "Deny and say what to do instead";
pub const REPLAN_PLACEHOLDER: &str = "Keep planning and say what to change";

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Hint {
    pub text: String,
    pub strong: bool,
}

impl Hint {
    #[must_use]
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            strong: false,
        }
    }

    #[must_use]
    pub fn strong(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            strong: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ApprovalTopic {
    Plan {
        markdown: String,
    },
    Tool {
        kind: Option<ItemKind>,
        heading: &'static str,
        inset: Option<String>,
        note: Option<String>,
    },
}

impl ApprovalTopic {
    #[must_use]
    pub const fn plan(&self) -> bool {
        matches!(self, Self::Plan { .. })
    }

    #[must_use]
    pub const fn placeholder(&self) -> &'static str {
        if self.plan() {
            REPLAN_PLACEHOLDER
        } else {
            DENY_PLACEHOLDER
        }
    }
}

#[must_use]
pub fn approval_topic(
    chat: Option<&ChatView>,
    request_id: u64,
    title: &str,
    cwd: Option<&str>,
) -> ApprovalTopic {
    let view = chat.and_then(|chat| chat.request(RequestId(request_id)));
    if let Some(Ask::Plan { markdown, .. }) = view.map(|view| &view.ask) {
        return ApprovalTopic::Plan {
            markdown: markdown.clone(),
        };
    }
    let item = view
        .and_then(|view| view.item.as_ref())
        .and_then(|id| chat.and_then(|chat| chat.item(id)));
    let kind = item.map(|item| item.kind);
    let heading = match kind {
        Some(ItemKind::Execute) => "Run this command?",
        Some(ItemKind::Edit) => "Edit this file?",
        Some(ItemKind::Read) => "Read this file?",
        Some(ItemKind::Search) => "Run this search?",
        Some(ItemKind::Fetch) => "Fetch this page?",
        Some(ItemKind::Web) => "Search the web?",
        _ => "Allow this?",
    };
    let inset = item
        .and_then(|item| item.body.target.clone())
        .filter(|target| !target.is_empty())
        .or_else(|| Some(title.to_owned()).filter(|title| !title.is_empty()));
    let note = cwd
        .filter(|cwd| !cwd.is_empty() && matches!(kind, Some(ItemKind::Execute | ItemKind::Search)))
        .map(|cwd| format!("in {cwd}"));
    ApprovalTopic::Tool {
        kind,
        heading,
        inset,
        note,
    }
}

#[must_use]
pub const fn approval_send(replying: bool, denying: bool, plan: bool) -> &'static str {
    match (replying, denying, plan) {
        (true, ..) => "Send",
        (false, true, true) => "Keep planning",
        (false, true, false) => "Deny",
        (false, false, true) => "Build",
        (false, false, false) => "Allow",
    }
}

#[must_use]
pub fn approval_hints(options: usize, replying: bool, plan: bool, send: &str) -> Vec<Hint> {
    let mut hints = Vec::new();
    if !replying && options > 0 {
        hints.push(Hint::new(pick_hint(options, "pick")));
    }
    hints.push(Hint::new(format!("↵ {}", send.to_lowercase())));
    hints.push(Hint::new(if plan {
        "esc keep planning"
    } else {
        "esc deny"
    }));
    hints
}

#[must_use]
pub fn pick_hint(count: usize, verb: &str) -> String {
    match count.min(KEYED_OPTIONS) {
        1 => format!("1 {verb}"),
        count => format!("1 to {count} {verb}"),
    }
}

#[must_use]
pub fn question_send(card: &QuestionCard) -> &'static str {
    if card.last_step() { "Send" } else { "Next" }
}

#[must_use]
pub fn question_hints(card: &QuestionCard) -> Vec<Hint> {
    let index = card.focused();
    let Some(question) = card.questions().get(index) else {
        return Vec::new();
    };
    let choices = question.options.len();
    let typed = question.allow_other && choices > 0;
    let mut hints = Vec::new();
    if question.multi_select && choices > 0 {
        hints.push(match card.picked(index) {
            0 => Hint::new("Pick any"),
            picked => Hint::strong(format!("{picked} picked")),
        });
    }
    if choices > 0 {
        let verb = if question.multi_select {
            "toggle"
        } else {
            "pick"
        };
        hints.push(Hint::new(pick_hint(choices + usize::from(typed), verb)));
    }
    if card.questions().len() > 1 {
        hints.push(Hint::new("←→ page"));
    } else if choices > 1 {
        hints.push(Hint::new("↑↓ move"));
    }
    hints.push(Hint::new(format!(
        "↵ {}",
        question_send(card).to_lowercase()
    )));
    hints.push(Hint::new("esc skip"));
    hints
}

#[must_use]
pub fn question_other_key(card: &QuestionCard) -> Option<usize> {
    let question = card.questions().get(card.focused())?;
    let choices = question.options.len();
    (question.allow_other && choices > 0 && choices < KEYED_OPTIONS).then_some(choices + 1)
}
