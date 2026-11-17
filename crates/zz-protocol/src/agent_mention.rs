use std::{fmt, ops::Range};

use crate::{AgentProvider, PaneId};

pub const MENTION_NOTE: &str = "@+claude or @+codex[/MODEL] asks you to delegate: write a brief that stands on its own and run zz new-pane -d --kind agent claude|codex [--model MODEL] 'BRIEF'. @host:%N is a running chat: zz agent-send -t %N TEXT. Replies and permission asks come back to you as messages; do not wait or poll.";

const REPLY_TAIL: &str = "reply comes back to you as a message";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Mention {
    Agent {
        provider: AgentProvider,
        model: Option<String>,
    },
    Chat {
        host: String,
        pane: PaneId,
    },
}

impl Mention {
    #[must_use]
    pub fn parse(token: &str) -> Option<Self> {
        let body = token.strip_prefix('@')?;
        if let Some(agent) = body.strip_prefix('+') {
            let (name, model) = match agent.split_once('/') {
                Some((name, model)) => (
                    name,
                    Some(unescape(model)).filter(|model| !model.is_empty()),
                ),
                None => (agent, None),
            };
            let provider = match name {
                "claude" => AgentProvider::ClaudeCode,
                "codex" => AgentProvider::Codex,
                _ => return None,
            };
            return Some(Self::Agent { provider, model });
        }
        let (host, pane) = body.split_once(':')?;
        if host.is_empty() || host.starts_with(['/', '~']) || pane.contains(':') {
            return None;
        }
        Some(Self::Chat {
            host: host.to_owned(),
            pane: pane.parse().ok()?,
        })
    }
}

fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut escaped = false;
    for character in text.chars() {
        if escaped || character != '\\' {
            out.push(character);
            escaped = false;
        } else {
            escaped = true;
        }
    }
    out
}

#[must_use]
pub fn mention_spans(text: &str) -> Vec<Range<usize>> {
    let mut spans = Vec::new();
    let mut from = 0;
    while let Some(found) = text[from..].find('@') {
        let start = from + found;
        let opens = text[..start]
            .chars()
            .next_back()
            .is_none_or(|before| before.is_whitespace() || "([{'\"*_".contains(before));
        if opens {
            let end = mention_end(text, start);
            if end > start + 1 {
                spans.push(start..end);
                from = end;
                continue;
            }
        }
        from = start + 1;
    }
    spans
}

fn mention_end(text: &str, start: usize) -> usize {
    let mut end = text.len();
    let mut escaped = false;
    for (offset, character) in text[start + 1..].char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if character == '\\' {
            escaped = true;
            continue;
        }
        if character.is_whitespace() || "`)]}<>\"',".contains(character) {
            end = start + 1 + offset;
            break;
        }
    }
    let trimmed = text[..end].trim_end_matches(['.', ';', ':', '!', '?']);
    trimmed.len().max(start + 1)
}

#[must_use]
pub fn mentions(text: &str) -> Vec<Mention> {
    mention_spans(text)
        .into_iter()
        .filter_map(|span| Mention::parse(&text[span]))
        .collect()
}

#[must_use]
pub fn mention_note(text: &str) -> Option<&'static str> {
    (!mentions(text).is_empty()).then_some(MENTION_NOTE)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplyRoute {
    Back,
    Read,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReplyNotice {
    Opened {
        verb: String,
        pane: PaneId,
        provider: AgentProvider,
        route: Option<ReplyRoute>,
    },
    Sent {
        pane: PaneId,
        route: ReplyRoute,
    },
}

#[must_use]
pub fn wait_command(pane: PaneId) -> String {
    format!("zz agent-send -t {pane} --wait ''")
}

impl fmt::Display for ReplyNotice {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Opened {
                verb,
                pane,
                provider,
                route,
            } => {
                write!(
                    formatter,
                    "{verb}: opened {pane} ({})",
                    provider.command_name()
                )?;
                match route {
                    Some(ReplyRoute::Back) => write!(formatter, "; its {REPLY_TAIL}"),
                    Some(ReplyRoute::Read) => {
                        write!(formatter, "; read its reply with: {}", wait_command(*pane))
                    }
                    None => Ok(()),
                }
            }
            Self::Sent {
                pane,
                route: ReplyRoute::Back,
            } => write!(formatter, "agent-send: {pane}'s {REPLY_TAIL}"),
            Self::Sent {
                pane,
                route: ReplyRoute::Read,
            } => write!(
                formatter,
                "agent-send: read {pane}'s reply with: {}",
                wait_command(*pane)
            ),
        }
    }
}

impl ReplyNotice {
    #[must_use]
    pub fn find(text: &str) -> Option<Self> {
        text.lines().find_map(|line| Self::parse_line(line.trim()))
    }

    fn parse_line(line: &str) -> Option<Self> {
        if let Some(rest) = line.strip_prefix("agent-send: ") {
            if let Some(rest) = rest.strip_prefix("read ") {
                let (pane, tail) = rest.split_once("'s reply with: ")?;
                let pane = pane.parse().ok()?;
                return (tail == wait_command(pane)).then_some(Self::Sent {
                    pane,
                    route: ReplyRoute::Read,
                });
            }
            let pane = rest.strip_suffix(&format!("'s {REPLY_TAIL}"))?;
            return Some(Self::Sent {
                pane: pane.parse().ok()?,
                route: ReplyRoute::Back,
            });
        }
        let (verb, rest) = line.split_once(": opened ")?;
        let (pane, rest) = rest.split_once(" (")?;
        let pane = pane.parse().ok()?;
        let (name, tail) = rest.split_once(')')?;
        let route = match tail {
            "" => None,
            tail if tail == format!("; its {REPLY_TAIL}") => Some(ReplyRoute::Back),
            tail if tail == format!("; read its reply with: {}", wait_command(pane)) => {
                Some(ReplyRoute::Read)
            }
            _ => return None,
        };
        Some(Self::Opened {
            verb: verb.to_owned(),
            pane,
            provider: provider(name)?,
            route,
        })
    }
}

fn provider(name: &str) -> Option<AgentProvider> {
    match name {
        "claude" => Some(AgentProvider::ClaudeCode),
        "codex" => Some(AgentProvider::Codex),
        _ => None,
    }
}

fn header(line: &str) -> Option<(PaneId, AgentProvider, &str)> {
    let rest = line.trim().strip_prefix("[zz] ")?;
    let (pane, rest) = rest.split_once(" (")?;
    let (name, rest) = rest.split_once(") ")?;
    Some((pane.parse().ok()?, provider(name)?, rest))
}

fn write_header(
    formatter: &mut fmt::Formatter<'_>,
    pane: PaneId,
    provider: AgentProvider,
) -> fmt::Result {
    write!(formatter, "[zz] {pane} ({})", provider.command_name())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentReport {
    Finished {
        pane: PaneId,
        provider: AgentProvider,
        text: String,
    },
    Stopped {
        pane: PaneId,
        provider: AgentProvider,
        reason: String,
        text: String,
    },
    Failed {
        pane: PaneId,
        provider: AgentProvider,
        message: String,
    },
    Closed {
        pane: PaneId,
        provider: AgentProvider,
    },
}

impl fmt::Display for AgentReport {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let body = |formatter: &mut fmt::Formatter<'_>, text: &str| {
            if text.is_empty() {
                Ok(())
            } else {
                write!(formatter, "\n{text}")
            }
        };
        match self {
            Self::Finished {
                pane,
                provider,
                text,
            } => {
                write_header(formatter, *pane, *provider)?;
                formatter.write_str(" finished:")?;
                body(formatter, text)
            }
            Self::Stopped {
                pane,
                provider,
                reason,
                text,
            } => {
                write_header(formatter, *pane, *provider)?;
                write!(formatter, " stopped ({reason}):")?;
                body(formatter, text)
            }
            Self::Failed {
                pane,
                provider,
                message,
            } => {
                write_header(formatter, *pane, *provider)?;
                write!(formatter, " failed: {message}")
            }
            Self::Closed { pane, provider } => {
                write_header(formatter, *pane, *provider)?;
                formatter.write_str(" was closed.")
            }
        }
    }
}

impl AgentReport {
    #[must_use]
    pub const fn pane(&self) -> PaneId {
        match self {
            Self::Finished { pane, .. }
            | Self::Stopped { pane, .. }
            | Self::Failed { pane, .. }
            | Self::Closed { pane, .. } => *pane,
        }
    }

    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        let (line, rest) = text.split_once('\n').unwrap_or((text, ""));
        let (pane, provider, tail) = header(line)?;
        let rest = rest.trim_end().to_owned();
        if tail == "finished:" {
            return Some(Self::Finished {
                pane,
                provider,
                text: rest,
            });
        }
        if tail == "was closed." && rest.is_empty() {
            return Some(Self::Closed { pane, provider });
        }
        if let Some(message) = tail.strip_prefix("failed: ") {
            let message = if rest.is_empty() {
                message.to_owned()
            } else {
                format!("{message}\n{rest}")
            };
            return Some(Self::Failed {
                pane,
                provider,
                message,
            });
        }
        let reason = tail.strip_prefix("stopped (")?.strip_suffix("):")?;
        Some(Self::Stopped {
            pane,
            provider,
            reason: reason.to_owned(),
            text: rest,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AskOption {
    pub id: String,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AskAnswers {
    AllowDeny,
    Options(Vec<AskOption>),
    FreeText,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentAsk {
    pub pane: PaneId,
    pub provider: AgentProvider,
    pub what: String,
    pub answers: AskAnswers,
}

impl fmt::Display for AgentAsk {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let pane = self.pane;
        write_header(formatter, pane, self.provider)?;
        write!(formatter, " asks: {}", self.what)?;
        let deny = AgentRespond {
            pane,
            decision: AgentDecision::Deny,
            request: None,
        };
        match &self.answers {
            AskAnswers::AllowDeny => {
                let allow = AgentRespond {
                    pane,
                    decision: AgentDecision::Allow,
                    request: None,
                };
                write!(formatter, "\n{allow}\n{deny}")
            }
            AskAnswers::Options(options) => {
                for option in options {
                    let pick = AgentRespond {
                        pane,
                        decision: AgentDecision::Option(option.id.clone()),
                        request: None,
                    };
                    write!(formatter, "\n{pick}  {}", option.label)?;
                }
                write!(formatter, "\n{deny}")
            }
            AskAnswers::FreeText => {
                let answer = AgentRespond {
                    pane,
                    decision: AgentDecision::Answer("TEXT".to_owned()),
                    request: None,
                };
                write!(formatter, "\n{answer}\n{deny}")
            }
        }
    }
}

impl AgentAsk {
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let mut lines = text.trim().lines().map(str::trim);
        let (pane, provider, tail) = header(lines.next()?)?;
        let what = tail.strip_prefix("asks: ")?.to_owned();
        let rest = lines.collect::<Vec<_>>();
        let respond = |line: &str| AgentRespond::parse(line).filter(|respond| respond.pane == pane);
        let denies = |line: &str| respond(line).is_some_and(|r| r.decision == AgentDecision::Deny);
        let answers = match rest.as_slice() {
            [allow, deny]
                if respond(allow).is_some_and(|r| r.decision == AgentDecision::Allow)
                    && denies(deny) =>
            {
                AskAnswers::AllowDeny
            }
            [answer, deny]
                if respond(answer)
                    .is_some_and(|r| r.decision == AgentDecision::Answer("TEXT".to_owned()))
                    && denies(deny) =>
            {
                AskAnswers::FreeText
            }
            [options @ .., deny]
                if respond(deny).is_some_and(|r| r.decision == AgentDecision::Deny) =>
            {
                AskAnswers::Options(
                    options
                        .iter()
                        .map(|line| {
                            let (command, label) = line.split_once("  ").unwrap_or((line, ""));
                            match respond(command)?.decision {
                                AgentDecision::Option(id) => Some(AskOption {
                                    id,
                                    label: label.trim().to_owned(),
                                }),
                                _ => None,
                            }
                        })
                        .collect::<Option<Vec<_>>>()?,
                )
            }
            _ => return None,
        };
        Some(Self {
            pane,
            provider,
            what,
            answers,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentDecision {
    Allow,
    Deny,
    Option(String),
    Answer(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentRespond {
    pub pane: PaneId,
    pub decision: AgentDecision,
    pub request: Option<u64>,
}

impl fmt::Display for AgentRespond {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "zz agent-respond -t {}", self.pane)?;
        match &self.decision {
            AgentDecision::Allow => formatter.write_str(" --allow")?,
            AgentDecision::Deny => formatter.write_str(" --deny")?,
            AgentDecision::Option(id) => write!(formatter, " --option {id}")?,
            AgentDecision::Answer(text) => {
                write!(formatter, " --answer '{}'", text.replace('\'', "'\\''"))?;
            }
        }
        if let Some(request) = self.request {
            write!(formatter, " {request}")?;
        }
        Ok(())
    }
}

impl AgentRespond {
    #[must_use]
    pub fn parse(command: &str) -> Option<Self> {
        let words = shell_words(command);
        let Some(start) = words.iter().position(|word| word == "agent-respond") else {
            return words
                .iter()
                .filter(|word| word.contains("agent-respond") && word.contains(char::is_whitespace))
                .find_map(|word| Self::parse(word));
        };
        let mut pane = None;
        let mut decision = None;
        let mut request = None;
        let mut index = start + 1;
        while let Some(word) = words.get(index).map(String::as_str) {
            match word {
                ";" | "&&" | "||" | "|" | "&" => break,
                "-t" => {
                    pane = Some(words.get(index + 1)?.parse().ok()?);
                    index += 1;
                }
                "--allow" => decision = Some(AgentDecision::Allow),
                "--deny" => decision = Some(AgentDecision::Deny),
                "--option" | "--answer" => {
                    let value = words.get(index + 1)?.clone();
                    decision = Some(if word == "--option" {
                        AgentDecision::Option(value)
                    } else {
                        AgentDecision::Answer(value)
                    });
                    index += 1;
                }
                other => {
                    if let Some(target) = other.strip_prefix("-t") {
                        pane = Some(target.parse().ok()?);
                    } else if let Ok(id) = other.parse() {
                        request = Some(id);
                    } else {
                        break;
                    }
                }
            }
            index += 1;
        }
        Some(Self {
            pane: pane?,
            decision: decision?,
            request,
        })
    }
}

fn shell_words(text: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = None::<String>;
    let mut chars = text.chars().peekable();
    while let Some(character) = chars.next() {
        match character {
            '\'' | '"' => {
                let current = word.get_or_insert_default();
                while let Some(next) = chars.next() {
                    if next == character {
                        break;
                    }
                    if next == '\\'
                        && character == '"'
                        && let Some(escaped) = chars.next()
                    {
                        current.push(escaped);
                        continue;
                    }
                    current.push(next);
                }
            }
            '\\' => {
                if let Some(escaped) = chars.next() {
                    word.get_or_insert_default().push(escaped);
                }
            }
            ';' | '&' | '|' => {
                words.extend(word.take());
                let mut operator = character.to_string();
                if chars.next_if_eq(&character).is_some() {
                    operator.push(character);
                }
                words.push(operator);
            }
            character if character.is_whitespace() => words.extend(word.take()),
            character => word.get_or_insert_default().push(character),
        }
    }
    words.extend(word);
    words
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_ask_notice_and_respond_shapes_round_trip_exactly() {
        let pane = PaneId(4);
        let codex = AgentProvider::Codex;
        let reports = [
            (
                AgentReport::Finished {
                    pane,
                    provider: codex,
                    text: "all green\nno changes".to_owned(),
                },
                "[zz] %4 (codex) finished:\nall green\nno changes",
            ),
            (
                AgentReport::Stopped {
                    pane,
                    provider: AgentProvider::ClaudeCode,
                    reason: "max_tokens".to_owned(),
                    text: "half".to_owned(),
                },
                "[zz] %4 (claude) stopped (max_tokens):\nhalf",
            ),
            (
                AgentReport::Stopped {
                    pane,
                    provider: codex,
                    reason: "cancelled".to_owned(),
                    text: String::new(),
                },
                "[zz] %4 (codex) stopped (cancelled):",
            ),
            (
                AgentReport::Failed {
                    pane,
                    provider: codex,
                    message: "rate limited".to_owned(),
                },
                "[zz] %4 (codex) failed: rate limited",
            ),
            (
                AgentReport::Closed {
                    pane,
                    provider: codex,
                },
                "[zz] %4 (codex) was closed.",
            ),
        ];
        for (report, text) in reports {
            assert_eq!(report.to_string(), text);
            assert_eq!(AgentReport::parse(&format!("\n  {text}\n")), Some(report));
        }
        let asks = [
            (
                AskAnswers::AllowDeny,
                "[zz] %4 (codex) asks: execute cargo test\nzz agent-respond -t %4 --allow\nzz agent-respond -t %4 --deny",
            ),
            (
                AskAnswers::Options(vec![
                    AskOption {
                        id: "answer-0".to_owned(),
                        label: "Blue".to_owned(),
                    },
                    AskOption {
                        id: "answer-1".to_owned(),
                        label: "Dark red".to_owned(),
                    },
                ]),
                "[zz] %4 (codex) asks: execute cargo test\nzz agent-respond -t %4 --option answer-0  Blue\nzz agent-respond -t %4 --option answer-1  Dark red\nzz agent-respond -t %4 --deny",
            ),
            (
                AskAnswers::FreeText,
                "[zz] %4 (codex) asks: execute cargo test\nzz agent-respond -t %4 --answer 'TEXT'\nzz agent-respond -t %4 --deny",
            ),
        ];
        for (answers, text) in asks {
            let ask = AgentAsk {
                pane,
                provider: codex,
                what: "execute cargo test".to_owned(),
                answers,
            };
            assert_eq!(ask.to_string(), text);
            assert_eq!(AgentAsk::parse(text), Some(ask));
        }
        let opened = |route| ReplyNotice::Opened {
            verb: "new-pane".to_owned(),
            pane,
            provider: codex,
            route,
        };
        let notices = [
            (
                opened(Some(ReplyRoute::Back)),
                "new-pane: opened %4 (codex); its reply comes back to you as a message",
            ),
            (opened(None), "new-pane: opened %4 (codex)"),
            (
                opened(Some(ReplyRoute::Read)),
                "new-pane: opened %4 (codex); read its reply with: zz agent-send -t %4 --wait ''",
            ),
            (
                ReplyNotice::Sent {
                    pane,
                    route: ReplyRoute::Back,
                },
                "agent-send: %4's reply comes back to you as a message",
            ),
            (
                ReplyNotice::Sent {
                    pane,
                    route: ReplyRoute::Read,
                },
                "agent-send: read %4's reply with: zz agent-send -t %4 --wait ''",
            ),
        ];
        for (notice, line) in notices {
            assert_eq!(notice.to_string(), line);
            assert_eq!(ReplyNotice::find(&format!("%4\n{line}\n")), Some(notice));
        }
        let respond = AgentRespond {
            pane,
            decision: AgentDecision::Option("answer-1".to_owned()),
            request: Some(7),
        };
        assert_eq!(
            respond.to_string(),
            "zz agent-respond -t %4 --option answer-1 7"
        );
        assert_eq!(AgentRespond::parse(&respond.to_string()), Some(respond));
        let answer = AgentRespond {
            pane,
            decision: AgentDecision::Answer("it's blue".to_owned()),
            request: None,
        };
        assert_eq!(
            answer.to_string(),
            "zz agent-respond -t %4 --answer 'it'\\''s blue'"
        );
        assert_eq!(AgentRespond::parse(&answer.to_string()), Some(answer));
        assert_eq!(
            AgentRespond::parse("/bin/zsh -lc 'agent-respond -t %4 --deny' && echo done"),
            Some(AgentRespond {
                pane,
                decision: AgentDecision::Deny,
                request: None,
            })
        );
    }

    #[test]
    fn only_agent_and_chat_tokens_bring_the_note() {
        assert_eq!(
            mentions("@+codex/gpt-5\\]x fix it, ask @work:%12."),
            [
                Mention::Agent {
                    provider: AgentProvider::Codex,
                    model: Some("gpt-5]x".to_owned()),
                },
                Mention::Chat {
                    host: "work".to_owned(),
                    pane: PaneId(12),
                },
            ]
        );
        assert_eq!(mention_note("@+claude review"), Some(MENTION_NOTE));
        for plain in [
            "fix @work:main:1.%3",
            "read @/tmp/a.rs",
            "mail me@example.com",
            "@+gemini hi",
            "no tokens",
        ] {
            assert_eq!(mention_note(plain), None, "{plain}");
        }
        assert!(MENTION_NOTE.len() < 300);
    }
}
