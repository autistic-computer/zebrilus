use std::{ffi::OsStr, fs, path::Path};

use serde_json::Value;

use crate::agent::stream::AgentStreamPayload;

const OPEN: char = '\u{e200}';
const SPLIT: char = '\u{e202}';
const CLOSE: char = '\u{e201}';
const MAX_REFERENCE_BYTES: usize = 8 * 1024;
const MAX_VISUAL_BYTES: u64 = 1024 * 1024;

#[derive(Default)]
pub(crate) struct Visuals {
    held: Option<(Value, String)>,
}

impl Visuals {
    pub(crate) fn pass(&mut self, payload: AgentStreamPayload) -> Vec<AgentStreamPayload> {
        let AgentStreamPayload::Update { update } = payload else {
            return self.flush_before(payload);
        };
        let Some(text) = message_text(&update) else {
            return self.flush_before(AgentStreamPayload::Update { update });
        };
        if self.held.is_none() && !text.contains(OPEN) {
            return vec![AgentStreamPayload::Update { update }];
        }
        let mut text = text.to_owned();
        let mut out = Vec::new();
        if let Some((held, prefix)) = self.held.take() {
            if held.get("messageId") == update.get("messageId") {
                text.insert_str(0, &prefix);
            } else {
                out.push(with_text(held, prefix));
            }
        }
        let (shown, rest) = rewrite(&text);
        if let Some(rest) = rest {
            self.held = Some((update.clone(), rest));
        }
        if !shown.is_empty() {
            out.push(with_text(update, shown));
        }
        out
    }

    fn flush_before(&mut self, payload: AgentStreamPayload) -> Vec<AgentStreamPayload> {
        self.held
            .take()
            .map(|(update, text)| with_text(update, text))
            .into_iter()
            .chain([payload])
            .collect()
    }
}

fn message_text(update: &Value) -> Option<&str> {
    if update.get("sessionUpdate").and_then(Value::as_str) != Some("agent_message_chunk") {
        return None;
    }
    let content = update.get("content")?;
    if content.get("type").and_then(Value::as_str) != Some("text") {
        return None;
    }
    content.get("text").and_then(Value::as_str)
}

fn with_text(mut update: Value, text: String) -> AgentStreamPayload {
    update["content"]["text"] = Value::String(text);
    AgentStreamPayload::Update { update }
}

fn rewrite(text: &str) -> (String, Option<String>) {
    let mut shown = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find(OPEN) {
        shown.push_str(&rest[..start]);
        let reference = &rest[start..];
        let Some(end) = reference.find(CLOSE) else {
            if reference.len() > MAX_REFERENCE_BYTES {
                shown.push_str(reference);
                return (shown, None);
            }
            return (shown, Some(reference.to_owned()));
        };
        let end = end + CLOSE.len_utf8();
        shown.push_str(&expand(&reference[..end]));
        rest = &reference[end..];
    }
    shown.push_str(rest);
    (shown, None)
}

fn expand(reference: &str) -> String {
    let inner = &reference[OPEN.len_utf8()..reference.len() - CLOSE.len_utf8()];
    let Some(("visualize", arguments)) = inner.split_once(SPLIT) else {
        return reference.to_owned();
    };
    let Ok(arguments) = serde_json::from_str::<Value>(arguments) else {
        return reference.to_owned();
    };
    let Some(path) = arguments.get("path").and_then(Value::as_str) else {
        return reference.to_owned();
    };
    match read_visual(Path::new(path)) {
        Ok(html) => html_block(&html),
        Err(error) => format!("\n\n> Could not show {path}: {error}\n\n"),
    }
}

fn read_visual(path: &Path) -> Result<String, String> {
    if !path.is_absolute() {
        return Err("the path is not absolute".to_owned());
    }
    let html = path
        .extension()
        .and_then(OsStr::to_str)
        .is_some_and(|extension| {
            extension.eq_ignore_ascii_case("html") || extension.eq_ignore_ascii_case("htm")
        });
    if !html {
        return Err("it is not an .html file".to_owned());
    }
    let metadata = fs::metadata(path).map_err(|error| error.to_string())?;
    if metadata.len() > MAX_VISUAL_BYTES {
        return Err("it is larger than 1 MiB".to_owned());
    }
    fs::read_to_string(path).map_err(|error| error.to_string())
}

fn html_block(html: &str) -> String {
    let longest = html
        .split(|character| character != '`')
        .map(str::len)
        .max()
        .unwrap_or(0);
    let fence = "`".repeat(longest.max(2) + 1);
    format!(
        "\n{fence}html preview\n{}\n{fence}\n",
        html.trim_end_matches('\n')
    )
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn chunk(id: &str, text: &str) -> AgentStreamPayload {
        AgentStreamPayload::Update {
            update: json!({
                "sessionUpdate": "agent_message_chunk",
                "messageId": id,
                "content": { "type": "text", "text": text },
            }),
        }
    }

    fn texts(payloads: &[AgentStreamPayload]) -> Vec<String> {
        payloads
            .iter()
            .filter_map(|payload| match payload {
                AgentStreamPayload::Update { update } => message_text(update).map(str::to_owned),
                _ => None,
            })
            .collect()
    }

    fn reference(path: &Path) -> String {
        format!(
            "{OPEN}visualize{SPLIT}{}{CLOSE}",
            json!({ "path": path.display().to_string() })
        )
    }

    #[test]
    fn a_visualize_reference_becomes_an_html_preview_block() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("blob.html");
        fs::write(&path, "<div id=\"blob\">```hi```</div>\n").expect("write fragment");
        let mut visuals = Visuals::default();
        let text = format!("Here it is.\n\n{}\n\nDone.", reference(&path));
        let out = visuals.pass(chunk("m1", &text));
        assert_eq!(
            texts(&out),
            ["Here it is.\n\n\n````html preview\n<div id=\"blob\">```hi```</div>\n````\n\n\nDone."]
        );
    }

    #[test]
    fn a_reference_split_across_chunks_is_held_until_it_closes() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("blob.html");
        fs::write(&path, "<p>hi</p>").expect("write fragment");
        let whole = reference(&path);
        let (head, tail) = whole.split_at(whole.len() / 2);
        let mut visuals = Visuals::default();
        assert_eq!(
            texts(&visuals.pass(chunk("m1", &format!("A {head}")))),
            ["A "]
        );
        assert_eq!(
            texts(&visuals.pass(chunk("m1", &format!("{tail} B")))),
            ["\n```html preview\n<p>hi</p>\n```\n B"]
        );
    }

    #[test]
    fn an_unclosed_reference_is_released_before_other_events() {
        let mut visuals = Visuals::default();
        assert!(texts(&visuals.pass(chunk("m1", "\u{e200}visualize"))).is_empty());
        let out = visuals.pass(AgentStreamPayload::Update {
            update: json!({ "sessionUpdate": "plan", "entries": [] }),
        });
        assert_eq!(out.len(), 2);
        assert_eq!(texts(&out), ["\u{e200}visualize"]);
    }

    #[test]
    fn a_missing_file_says_why_and_other_references_stay() {
        let mut visuals = Visuals::default();
        let missing = reference(Path::new("/nonexistent/zz-visual.html"));
        let other = format!("{OPEN}image{SPLIT}{{}}{CLOSE}");
        let out = visuals.pass(chunk("m1", &format!("{missing}{other}")));
        let text = &texts(&out)[0];
        assert!(text.starts_with("\n\n> Could not show /nonexistent/zz-visual.html: "));
        assert!(text.ends_with(&other));
    }

    #[test]
    fn plain_chunks_pass_untouched() {
        let mut visuals = Visuals::default();
        assert_eq!(texts(&visuals.pass(chunk("m1", "hello"))), ["hello"]);
    }
}
