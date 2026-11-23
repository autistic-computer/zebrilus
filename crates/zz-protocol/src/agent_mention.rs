use std::{fmt, ops::Range};

use crate::{AgentProvider, PaneId};

const REPLY_TAIL: &str = "reply comes back to you as a message; end your turn now";

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
            let provider = provider(name)?;
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

impl Mention {
    #[must_use]
    pub fn instruction(&self) -> String {
        match self {
            Self::Agent { provider, model } => {
                let name = provider.command_name();
                let model = model
                    .as_deref()
                    .map(|model| format!(" --model {model}"))
                    .unwrap_or_default();
                format!(
                    "The user tagged {name}: {name} does this task, not you. Write it a brief that stands on its own, run zz new-pane -d --kind agent {name}{model} 'BRIEF' and end your turn. Do not do the task yourself. Its reply and any permission asks come back to you as messages."
                )
            }
            Self::Chat { host, pane } => format!(
                "@{host}:{pane} is another chat, not you: run zz agent-send -t {pane} 'TEXT' and end your turn. Its reply comes back to you as a message."
            ),
        }
    }
}

#[must_use]
pub fn mention_note(text: &str) -> Option<String> {
    let mut lines = Vec::<String>::new();
    for mention in mentions(text) {
        let line = mention.instruction();
        if !lines.contains(&line) {
            lines.push(line);
        }
    }
    (!lines.is_empty()).then(|| lines.join("\n"))
}

#[must_use]
pub fn child_note(parent: PaneId, parent_provider: AgentProvider, child: AgentProvider) -> String {
    let name = parent_provider.command_name();
    let escalate = if child == AgentProvider::Codex {
        " If a command fails with \"Operation not permitted\", rerun it with escalated permissions even if your brief says to stop on failure; the approval is answered for you."
    } else {
        ""
    };
    format!(
        "{parent} ({name}) opened this chat. Your final message is sent to it as your reply, so finish with the answer. You cannot open other agents or message {parent}; if you need something, say so in your reply.{escalate}"
    )
}

#[must_use]
pub fn hidden_note(text: &str, extra: Option<&str>) -> Option<String> {
    let notes = extra
        .map(str::to_owned)
        .into_iter()
        .chain(mention_note(text))
        .collect::<Vec<_>>();
    (!notes.is_empty()).then(|| notes.join("\n"))
}

#[must_use]
pub fn is_hidden_note(text: &str) -> bool {
    let mut lines = text.lines().map(str::trim).filter(|line| !line.is_empty());
    lines.clone().next().is_some() && lines.all(note_line)
}

fn note_line(line: &str) -> bool {
    if let Some(rest) = line.strip_prefix("The user tagged ") {
        let Some(provider) = rest.split_once(':').and_then(|(name, _)| provider(name)) else {
            return false;
        };
        let marker = format!(" --kind agent {} --model ", provider.command_name());
        let model = line
            .split_once(&marker)
            .and_then(|(_, rest)| rest.split_once(" 'BRIEF'"))
            .map(|(model, _)| model.to_owned());
        return Mention::Agent { provider, model }.instruction() == line;
    }
    if let Some((token, _)) = line.split_once(" is another chat, not you") {
        return Mention::parse(token)
            .filter(|mention| matches!(mention, Mention::Chat { .. }))
            .is_some_and(|mention| mention.instruction() == line);
    }
    let Some((opener, _)) = line.split_once(" opened this chat.") else {
        return false;
    };
    let Some((pane, name)) = opener.split_once(" (") else {
        return false;
    };
    let (Ok(pane), Some(parent)) = (pane.parse(), name.strip_suffix(')').and_then(provider)) else {
        return false;
    };
    AgentProvider::ALL
        .into_iter()
        .any(|child| child_note(pane, parent, child) == line)
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
    AgentProvider::ALL
        .into_iter()
        .find(|provider| provider.command_name() == name || provider.as_str() == name)
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

const CLOSED_TAIL: &str = "was closed before it replied. Tell the user; do not open it again.";

fn follow_up(pane: PaneId) -> String {
    format!(
        "If this answers the user, answer the user now and send nothing to {pane}. Only if something is missing: zz agent-send -t {pane} 'TEXT'"
    )
}

fn try_again(pane: PaneId) -> String {
    format!("Tell the user, or try again with: zz agent-send -t {pane} 'TEXT'")
}

impl fmt::Display for AgentReport {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let body = |formatter: &mut fmt::Formatter<'_>, pane: PaneId, text: &str| {
            if !text.is_empty() {
                write!(formatter, "\n{text}")?;
            }
            write!(formatter, "\n\n{}", follow_up(pane))
        };
        match self {
            Self::Finished {
                pane,
                provider,
                text,
            } => {
                write_header(formatter, *pane, *provider)?;
                formatter.write_str(" finished:")?;
                body(formatter, *pane, text)
            }
            Self::Stopped {
                pane,
                provider,
                reason,
                text,
            } => {
                write_header(formatter, *pane, *provider)?;
                write!(formatter, " stopped ({reason}):")?;
                body(formatter, *pane, text)
            }
            Self::Failed {
                pane,
                provider,
                message,
            } => {
                write_header(formatter, *pane, *provider)?;
                write!(formatter, " failed: {message}\n{}", try_again(*pane))
            }
            Self::Closed { pane, provider } => {
                write_header(formatter, *pane, *provider)?;
                write!(formatter, " {CLOSED_TAIL}")
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
        let rest = rest.trim_end();
        let reply = || {
            let follow = follow_up(pane);
            rest.strip_suffix(follow.as_str())
                .unwrap_or(rest)
                .trim()
                .to_owned()
        };
        if tail == "finished:" {
            return Some(Self::Finished {
                pane,
                provider,
                text: reply(),
            });
        }
        if (tail == CLOSED_TAIL || tail == "was closed.") && rest.is_empty() {
            return Some(Self::Closed { pane, provider });
        }
        if let Some(message) = tail.strip_prefix("failed: ") {
            let again = try_again(pane);
            let rest = rest.strip_suffix(again.as_str()).unwrap_or(rest).trim_end();
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
            text: reply(),
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
    pub request: Option<u64>,
}

impl fmt::Display for AgentAsk {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let pane = self.pane;
        write_header(formatter, pane, self.provider)?;
        write!(formatter, " asks: {}", self.what)?;
        let respond = |decision| AgentRespond {
            pane,
            decision,
            request: self.request,
        };
        let deny = respond(AgentDecision::Deny);
        match &self.answers {
            AskAnswers::AllowDeny => {
                let allow = respond(AgentDecision::Allow);
                write!(formatter, "\n{allow}\n{deny}")
            }
            AskAnswers::Options(options) => {
                for option in options {
                    let pick = respond(AgentDecision::Option(option.id.clone()));
                    write!(formatter, "\n{pick}  # {}", option.label)?;
                }
                write!(formatter, "\n{deny}")
            }
            AskAnswers::FreeText => {
                let answer = respond(AgentDecision::Answer("TEXT".to_owned()));
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
        let request = AgentRespond::parse(rest.last()?)?.request;
        let respond = |line: &str| {
            AgentRespond::parse(line)
                .filter(|respond| respond.pane == pane && respond.request == request)
        };
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
                            let (command, label) = line.split_once("  # ").unwrap_or((line, ""));
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
            request,
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
            '#' if word.is_none() => {
                for skipped in chars.by_ref() {
                    if skipped == '\n' {
                        break;
                    }
                }
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
                "[zz] %4 (codex) finished:\nall green\nno changes\n\nIf this answers the user, answer the user now and send nothing to %4. Only if something is missing: zz agent-send -t %4 'TEXT'",
            ),
            (
                AgentReport::Stopped {
                    pane,
                    provider: AgentProvider::ClaudeCode,
                    reason: "max_tokens".to_owned(),
                    text: "half".to_owned(),
                },
                "[zz] %4 (claude) stopped (max_tokens):\nhalf\n\nIf this answers the user, answer the user now and send nothing to %4. Only if something is missing: zz agent-send -t %4 'TEXT'",
            ),
            (
                AgentReport::Stopped {
                    pane,
                    provider: codex,
                    reason: "cancelled".to_owned(),
                    text: String::new(),
                },
                "[zz] %4 (codex) stopped (cancelled):\n\nIf this answers the user, answer the user now and send nothing to %4. Only if something is missing: zz agent-send -t %4 'TEXT'",
            ),
            (
                AgentReport::Failed {
                    pane,
                    provider: codex,
                    message: "rate limited".to_owned(),
                },
                "[zz] %4 (codex) failed: rate limited\nTell the user, or try again with: zz agent-send -t %4 'TEXT'",
            ),
            (
                AgentReport::Closed {
                    pane,
                    provider: codex,
                },
                "[zz] %4 (codex) was closed before it replied. Tell the user; do not open it again.",
            ),
        ];
        for (report, text) in reports {
            assert_eq!(report.to_string(), text);
            assert_eq!(AgentReport::parse(&format!("\n  {text}\n")), Some(report));
        }
        let asks = [
            (
                AskAnswers::AllowDeny,
                "[zz] %4 (codex) asks: execute cargo test\nzz agent-respond -t %4 --allow 9\nzz agent-respond -t %4 --deny 9",
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
                "[zz] %4 (codex) asks: execute cargo test\nzz agent-respond -t %4 --option answer-0 9  # Blue\nzz agent-respond -t %4 --option answer-1 9  # Dark red\nzz agent-respond -t %4 --deny 9",
            ),
            (
                AskAnswers::FreeText,
                "[zz] %4 (codex) asks: execute cargo test\nzz agent-respond -t %4 --answer 'TEXT' 9\nzz agent-respond -t %4 --deny 9",
            ),
        ];
        for (answers, text) in asks {
            let ask = AgentAsk {
                pane,
                provider: codex,
                what: "execute cargo test".to_owned(),
                answers,
                request: Some(9),
            };
            assert_eq!(ask.to_string(), text);
            assert_eq!(AgentAsk::parse(text), Some(ask));
        }
        let earlier = "[zz] %4 (codex) asks: execute cargo test\nzz agent-respond -t %4 --allow\nzz agent-respond -t %4 --deny";
        assert_eq!(
            AgentAsk::parse(earlier).map(|ask| (ask.answers, ask.request)),
            Some((AskAnswers::AllowDeny, None))
        );
        assert_eq!(
            AgentAsk::parse(
                "[zz] %4 (codex) asks: execute cargo test\nzz agent-respond -t %4 --allow 8\nzz agent-respond -t %4 --deny 9"
            ),
            None
        );
        let opened = |route| ReplyNotice::Opened {
            verb: "new-pane".to_owned(),
            pane,
            provider: codex,
            route,
        };
        let notices = [
            (
                opened(Some(ReplyRoute::Back)),
                "new-pane: opened %4 (codex); its reply comes back to you as a message; end your turn now",
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
                "agent-send: %4's reply comes back to you as a message; end your turn now",
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
        let codex = "The user tagged codex: codex does this task, not you. Write it a brief that stands on its own, run zz new-pane -d --kind agent codex 'BRIEF' and end your turn. Do not do the task yourself. Its reply and any permission asks come back to you as messages.";
        let claude = "The user tagged claude: claude does this task, not you. Write it a brief that stands on its own, run zz new-pane -d --kind agent claude --model opus 'BRIEF' and end your turn. Do not do the task yourself. Its reply and any permission asks come back to you as messages.";
        let chat = "@macbook:%2 is another chat, not you: run zz agent-send -t %2 'TEXT' and end your turn. Its reply comes back to you as a message.";
        assert_eq!(mention_note("@+codex fix it").as_deref(), Some(codex));
        let both = mention_note("@+claude/opus and @+codex, ask @macbook:%2 and @+codex again")
            .expect("note");
        assert_eq!(both, format!("{claude}\n{codex}\n{chat}"));
        let child = child_note(PaneId(1), AgentProvider::ClaudeCode, AgentProvider::Codex);
        assert_eq!(
            child,
            "%1 (claude) opened this chat. Your final message is sent to it as your reply, so finish with the answer. You cannot open other agents or message %1; if you need something, say so in your reply. If a command fails with \"Operation not permitted\", rerun it with escalated permissions even if your brief says to stop on failure; the approval is answered for you."
        );
        assert_eq!(
            child_note(PaneId(1), AgentProvider::Codex, AgentProvider::ClaudeCode),
            "%1 (codex) opened this chat. Your final message is sent to it as your reply, so finish with the answer. You cannot open other agents or message %1; if you need something, say so in your reply."
        );
        let hidden = hidden_note("@+claude/opus and @macbook:%2", Some(&child)).expect("note");
        assert_eq!(hidden, format!("{child}\n{claude}\n{chat}"));
        assert!(is_hidden_note(&hidden));
        assert!(!is_hidden_note(
            "The user tagged codex: please do it yourself."
        ));
        assert!(!is_hidden_note(""));
        for plain in [
            "fix @work:main:1.%3",
            "read @/tmp/a.rs",
            "mail me@example.com",
            "@+gemini hi",
            "no tokens",
        ] {
            assert_eq!(mention_note(plain), None, "{plain}");
        }
    }
}
