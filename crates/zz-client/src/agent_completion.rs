use std::ops::Range;

use agent_client_protocol_schema::v1::{AvailableCommand, AvailableCommandInput};

const MAX_COMPLETION_RESULTS: usize = 64;

#[derive(Clone, Debug, Default, Eq, PartialEq, serde::Serialize)]
pub struct AgentCommand {
    pub name: String,
    pub description: String,
    pub input_hint: Option<String>,
    pub terminal: bool,
    pub skill: bool,
}

pub fn agent_command_model(command: AvailableCommand) -> AgentCommand {
    let input_hint = command.input.and_then(|input| match input {
        AvailableCommandInput::Unstructured(input) => Some(input.hint),
        _ => None,
    });
    let flag = |key: &str| {
        command
            .meta
            .as_ref()
            .and_then(|meta| meta.get("zz"))
            .and_then(|zz| zz.get(key))
            .and_then(serde_json::Value::as_bool)
            .unwrap_or_default()
    };
    AgentCommand {
        terminal: flag("terminal"),
        skill: flag("skill"),
        name: command.name,
        description: command.description,
        input_hint,
    }
}

pub fn terminal_command<'a>(value: &str, commands: &'a [AgentCommand]) -> Option<&'a AgentCommand> {
    let name = value.trim().strip_prefix('/')?.split_whitespace().next()?;
    commands
        .iter()
        .find(|command| command.terminal && bare_command_name(&command.name) == name)
}

pub const MAX_RENDERED_ERROR_BYTES: usize = 1024;

pub fn rendered_error(error: &str) -> String {
    let mut rendered = String::with_capacity(MAX_RENDERED_ERROR_BYTES);
    let mut truncated = false;
    for character in error.trim().chars() {
        let character = if character.is_control() && !matches!(character, '\n' | '\t') {
            '�'
        } else {
            character
        };
        if rendered.len().saturating_add(character.len_utf8()) > MAX_RENDERED_ERROR_BYTES - 3 {
            truncated = true;
            break;
        }
        rendered.push(character);
    }
    if truncated {
        rendered.push('…');
    }
    rendered
}

const ZZ_COMMAND_DESCRIPTIONS: [(&str, &str, Option<&str>); 5] = [
    (
        "btw",
        "Ask on the side without adding to the conversation",
        Some("question"),
    ),
    ("side", "Same as /btw", Some("question")),
    ("steer", "Redirect the running turn", Some("text")),
    ("fork", "Continue in a copy of this conversation", None),
    (
        "rewind",
        "Continue from before your last n prompts; files stay as they are",
        Some("n or prompt id"),
    ),
];

pub fn pane_commands(vendor: &[AgentCommand], verbs: bool) -> Vec<AgentCommand> {
    if !verbs {
        return vendor.to_vec();
    }
    let own = ZZ_COMMAND_DESCRIPTIONS
        .iter()
        .map(|(name, description, input_hint)| AgentCommand {
            name: (*name).to_owned(),
            description: (*description).to_owned(),
            input_hint: input_hint.map(str::to_owned),
            ..AgentCommand::default()
        });
    vendor
        .iter()
        .filter(|command| {
            !zz_protocol::agent_stream::AGENT_VERBS.contains(&bare_command_name(&command.name))
        })
        .cloned()
        .chain(own)
        .collect()
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandCompletion {
    pub command: AgentCommand,
    pub replacement: Range<usize>,
}

impl CommandCompletion {
    pub fn insertion(&self) -> String {
        format!("{} ", command_label(&self.command.name))
    }
}

pub fn command_label(name: &str) -> String {
    let name = bare_command_name(name);
    if name.starts_with('$') {
        name.to_owned()
    } else {
        format!("/{name}")
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompletionQuery {
    pub needle: String,
    pub replacement: Range<usize>,
}

pub fn completion_query(value: &str, cursor: usize) -> Option<CompletionQuery> {
    if cursor > value.len() || !value.is_char_boundary(cursor) {
        return None;
    }
    let before_cursor = &value[..cursor];
    let line_start = before_cursor.rfind('\n').map_or(0, |index| index + 1);
    let sigil_index = before_cursor[line_start..].rfind('/')? + line_start;
    if sigil_index > line_start
        && !value[..sigil_index]
            .chars()
            .next_back()
            .is_some_and(char::is_whitespace)
    {
        return None;
    }
    let tail = &value[sigil_index + 1..cursor];
    if tail.chars().any(char::is_whitespace) {
        return None;
    }
    Some(CompletionQuery {
        needle: tail.to_owned(),
        replacement: sigil_index..cursor,
    })
}

pub fn bare_command_name(name: &str) -> &str {
    name.trim_start_matches('/')
}

pub fn completion_score(candidate: &str, needle: &str) -> Option<u8> {
    if needle.is_empty() {
        return Some(3);
    }
    if candidate == needle {
        return Some(0);
    }
    if candidate.starts_with(needle) {
        return Some(1);
    }
    if candidate.contains(needle) {
        return Some(2);
    }
    let mut characters = candidate.chars();
    needle
        .chars()
        .all(|needle| characters.by_ref().any(|candidate| candidate == needle))
        .then_some(3)
}

pub fn ranked_completions(
    commands: &[AgentCommand],
    query: &CompletionQuery,
) -> Vec<CommandCompletion> {
    let needle = query.needle.to_ascii_lowercase();
    let mut ranked = commands
        .iter()
        .filter_map(|command| {
            let searchable = bare_command_name(&command.name).to_ascii_lowercase();
            completion_score(&searchable, &needle).map(|score| {
                (
                    score,
                    searchable,
                    CommandCompletion {
                        command: command.clone(),
                        replacement: query.replacement.clone(),
                    },
                )
            })
        })
        .collect::<Vec<_>>();
    ranked.sort_by(|left, right| (left.0, &left.1).cmp(&(right.0, &right.1)));
    ranked
        .into_iter()
        .take(MAX_COMPLETION_RESULTS)
        .map(|(_, _, completion)| completion)
        .collect()
}

pub fn meaningful_command_description(description: &str) -> Option<&str> {
    let description = description.trim();
    (!description.is_empty()
        && description
            .chars()
            .any(|character| character != '.' && character != '…'))
    .then_some(description)
}

pub fn active_command_hint(value: &str, commands: &[AgentCommand]) -> Option<String> {
    let command = value.trim_start().strip_prefix('/')?;
    let (name, arguments) = command
        .split_once(char::is_whitespace)
        .map_or((command, ""), |(name, arguments)| (name, arguments));
    if !arguments.trim().is_empty() {
        return None;
    }
    commands
        .iter()
        .find(|command| bare_command_name(&command.name).eq_ignore_ascii_case(name))
        .and_then(|command| command.input_hint.as_deref())
        .map(|hint| format!("Argument · {hint}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completion_replacement_keeps_unicode_prefixes_and_the_remaining_text() {
        let value = "héllo /rev!";
        let cursor = value.find('!').unwrap();
        let query = completion_query(value, cursor).unwrap();
        let completion = ranked_completions(&[command("/review")], &query).remove(0);
        let mut edited = value.to_owned();
        edited.replace_range(completion.replacement.clone(), &completion.insertion());
        assert_eq!(edited, "héllo /review !");
        assert_eq!(completion_query(value, 2), None);
        assert_eq!(completion_query(value, value.len() + 1), None);
    }

    fn command(name: &str) -> AgentCommand {
        AgentCommand {
            name: name.to_owned(),
            description: format!("Run {name}"),
            ..AgentCommand::default()
        }
    }

    #[test]
    fn available_commands_use_standard_slash_semantics() {
        assert_eq!(bare_command_name("/review"), "review");
        assert_eq!(bare_command_name("$brainstorm"), "$brainstorm");
        assert_eq!(
            completion_query("/rev", 4),
            Some(CompletionQuery {
                needle: "rev".to_owned(),
                replacement: 0..4,
            })
        );
        assert_eq!(
            completion_query("please /rev", 11),
            Some(CompletionQuery {
                needle: "rev".to_owned(),
                replacement: 7..11,
            })
        );
        assert!(completion_query("$rev", 4).is_none());
        assert!(completion_query("https://zed.dev", 15).is_none());
        assert!(completion_query("/review branch", 14).is_none());
    }

    #[test]
    fn zz_commands_replace_the_vendor_commands_they_intercept() {
        let vendor = [command("review"), command("btw")];
        let names = |commands: Vec<AgentCommand>| {
            commands
                .into_iter()
                .map(|command| (command.name, command.description))
                .collect::<Vec<_>>()
        };
        assert_eq!(names(pane_commands(&vendor, false)).len(), 2);
        let merged = names(pane_commands(&vendor, true));
        assert_eq!(merged.len(), 6);
        assert!(merged.iter().any(|(name, description)| {
            name == "btw" && description.starts_with("Ask on the side")
        }));
        assert_eq!(
            active_command_hint("/rewind ", &pane_commands(&[], true)),
            Some("Argument · n or prompt id".to_owned())
        );
    }

    #[test]
    fn completion_matching_supports_bare_command_names() {
        assert_eq!(completion_score("brainstorm", "brain"), Some(1));
        assert_eq!(completion_score("gh-address-comments", "gac"), Some(3));
        assert_eq!(completion_score("review", "xyz"), None);
    }

    #[test]
    fn completion_results_keep_every_available_command() {
        let commands = (0..16)
            .map(|index| command(&format!("command-{index:02}")))
            .collect::<Vec<_>>();
        let query = completion_query("/", 1).expect("command completion query");

        let completions = ranked_completions(&commands, &query);

        assert_eq!(completions.len(), commands.len());
    }

    #[test]
    fn command_hints_follow_standard_slash_semantics() {
        let command = AgentCommand {
            input_hint: Some("optional context".to_owned()),
            ..command("review")
        };

        assert_eq!(
            active_command_hint("/review ", std::slice::from_ref(&command)),
            Some("Argument · optional context".to_owned())
        );
        assert_eq!(active_command_hint("$review ", &[command]), None);
    }

    #[test]
    fn lens_commands_carry_their_terminal_and_skill_flags() {
        let parse = |value: serde_json::Value| {
            agent_command_model(serde_json::from_value::<AvailableCommand>(value).unwrap())
        };
        let status = parse(serde_json::json!({
            "name": "status",
            "description": "Status in the terminal",
            "_meta": { "zz": { "terminal": true, "skill": false } },
        }));
        let pet = parse(serde_json::json!({
            "name": "$hatch-pet",
            "description": "Make a pet",
            "_meta": { "zz": { "skill": true } },
        }));
        let plain = parse(serde_json::json!({ "name": "compact", "description": "Compact" }));
        assert!(status.terminal && !status.skill);
        assert!(pet.skill && !pet.terminal);
        assert!(!plain.terminal && !plain.skill);
        let commands = [status, pet, plain];
        assert_eq!(
            terminal_command(" /status ", &commands).map(|command| command.name.as_str()),
            Some("status")
        );
        assert!(terminal_command("/compact", &commands).is_none());
        assert!(terminal_command("why /status", &commands).is_none());
        assert_eq!(command_label("$hatch-pet"), "$hatch-pet");
        assert_eq!(command_label("/compact"), "/compact");
        let query = completion_query("/hat", 4).unwrap();
        assert_eq!(
            ranked_completions(&commands, &query)[0].insertion(),
            "$hatch-pet "
        );
    }

    #[test]
    fn placeholder_only_command_descriptions_are_hidden() {
        assert_eq!(meaningful_command_description("..."), None);
        assert_eq!(meaningful_command_description(" … "), None);
        assert_eq!(
            meaningful_command_description(" Review the current diff "),
            Some("Review the current diff")
        );
    }
}
