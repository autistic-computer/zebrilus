use zz_protocol::chat::{ChatEvent, ItemBody, ItemId, ItemKind};

const MAX_CAPTURE_BYTES: usize = 64 * 1024;

#[derive(Default)]
struct Turn {
    prompt: Option<String>,
    lines: Vec<String>,
}

impl Turn {
    fn render(&self) -> String {
        let body = self.lines.join("\n");
        match (&self.prompt, body.is_empty()) {
            (Some(prompt), true) => prompt.clone(),
            (Some(prompt), false) => format!("{prompt}\n\n{body}"),
            (None, _) => body,
        }
    }
}

pub(crate) fn chat_text<'a>(events: impl Iterator<Item = &'a ChatEvent>, whole: bool) -> String {
    let mut turns = Vec::<Turn>::new();
    for event in events {
        let ChatEvent::ItemStarted {
            parent: None,
            kind,
            body,
            ..
        } = event
        else {
            continue;
        };
        if *kind == ItemKind::User {
            turns.push(Turn {
                prompt: Some(prompt(&body.text)),
                lines: Vec::new(),
            });
            continue;
        }
        let Some(line) = line(*kind, body) else {
            continue;
        };
        if turns.is_empty() {
            turns.push(Turn::default());
        }
        if let Some(turn) = turns.last_mut() {
            turn.lines.push(line);
        }
    }
    let shown = if whole {
        &turns[..]
    } else {
        &turns[turns.len().saturating_sub(1)..]
    };
    let text = shown
        .iter()
        .map(Turn::render)
        .filter(|turn| !turn.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    let mut start = text.len().saturating_sub(MAX_CAPTURE_BYTES);
    while !text.is_char_boundary(start) {
        start += 1;
    }
    text[start..].to_owned()
}

pub(crate) fn stream_events<'a>(
    updates: impl Iterator<Item = &'a serde_json::Value>,
) -> Vec<ChatEvent> {
    let mut events = Vec::<ChatEvent>::new();
    let mut last_user = None::<String>;
    for update in updates {
        let text = update["content"]["text"].as_str().unwrap_or_default();
        let (kind, title, joins) = match update["sessionUpdate"].as_str() {
            Some("user_message_chunk") => {
                let message = update["messageId"].as_str().unwrap_or_default().to_owned();
                let joins = last_user.as_deref() == Some(message.as_str());
                last_user = Some(message);
                (ItemKind::User, String::new(), joins)
            }
            Some("agent_message_chunk") if update["_meta"]["zz"]["notice"].is_null() => {
                last_user = None;
                (ItemKind::Agent, String::new(), true)
            }
            Some("tool_call") => {
                last_user = None;
                let kind = match update["kind"].as_str() {
                    Some("execute") => ItemKind::Execute,
                    Some("read") => ItemKind::Read,
                    Some("edit" | "delete" | "move") => ItemKind::Edit,
                    Some("search") => ItemKind::Search,
                    Some("fetch") => ItemKind::Fetch,
                    _ => ItemKind::Tool,
                };
                let title = update["title"].as_str().unwrap_or_default().to_owned();
                (kind, title, false)
            }
            _ => continue,
        };
        if joins
            && let Some(ChatEvent::ItemStarted {
                kind: last, body, ..
            }) = events.last_mut()
            && *last == kind
        {
            body.text.push_str(text);
            continue;
        }
        events.push(ChatEvent::ItemStarted {
            item: ItemId::new(format!("capture:{}", events.len())),
            turn: None,
            parent: None,
            kind,
            body: ItemBody {
                title,
                text: text.to_owned(),
                ..ItemBody::default()
            },
            at: 0,
        });
    }
    events
}

fn prompt(text: &str) -> String {
    text.trim_end()
        .lines()
        .enumerate()
        .map(|(index, line)| {
            if index == 0 {
                format!("> {line}")
            } else {
                format!("  {line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn line(kind: ItemKind, body: &ItemBody) -> Option<String> {
    let word = match kind {
        ItemKind::Agent => {
            let text = body.text.trim();
            return (!text.is_empty()).then(|| text.to_owned());
        }
        ItemKind::Read => "read",
        ItemKind::Edit => "edit",
        ItemKind::Search => "search",
        ItemKind::Execute => "execute",
        ItemKind::Fetch => "fetch",
        ItemKind::Web => "web",
        ItemKind::Mcp => "mcp",
        ItemKind::Tool => "tool",
        ItemKind::Subagent => "subagent",
        ItemKind::User
        | ItemKind::Reasoning
        | ItemKind::Plan
        | ItemKind::Notice
        | ItemKind::Error => return None,
    };
    let title = if body.title.trim().is_empty() {
        body.target.as_deref().unwrap_or_default()
    } else {
        &body.title
    };
    let title = title.split_whitespace().collect::<Vec<_>>().join(" ");
    Some(if title.is_empty() {
        word.to_owned()
    } else {
        format!("{word} {title}")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: &str, kind: ItemKind, title: &str, text: &str) -> ChatEvent {
        ChatEvent::ItemStarted {
            item: ItemId::new(id),
            turn: None,
            parent: None,
            kind,
            body: ItemBody {
                title: title.to_owned(),
                text: text.to_owned(),
                ..ItemBody::default()
            },
            at: 0,
        }
    }

    #[test]
    fn a_chat_prints_its_last_turn_or_all_of_it() {
        let events = [
            item("u1", ItemKind::User, "", "reply with ok"),
            item("a1", ItemKind::Agent, "", "ok"),
            item("u2", ItemKind::User, "", "run it\nplease"),
            item("r1", ItemKind::Reasoning, "", "thinking"),
            item("t1", ItemKind::Execute, "cargo  test -p\nzz-daemon", ""),
            item("a2", ItemKind::Agent, "", "all green\n"),
        ];
        assert_eq!(
            chat_text(events[..2].iter(), false),
            "> reply with ok\n\nok"
        );
        assert_eq!(
            chat_text(events.iter(), false),
            "> run it\n  please\n\nexecute cargo test -p zz-daemon\nall green"
        );
        assert_eq!(
            chat_text(events.iter(), true),
            "> reply with ok\n\nok\n\n> run it\n  please\n\nexecute cargo test -p zz-daemon\nall green"
        );
        assert_eq!(chat_text([].iter(), true), "");
    }
}
