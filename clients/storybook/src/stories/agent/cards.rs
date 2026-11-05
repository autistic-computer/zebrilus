use std::sync::Arc;

use ala::{
    AnyElement, AnyView, App, IntoElement, ParentElement as _,
    SharedString, Styled as _, Window, div, px, uniform_list,
};
use penna::{
    Disableable as _,
    blocks::chat::{
        COMPOSER_MAX_WIDTH, permission_card, permission_option, suggestion_list,
        suggestion_list_height, suggestion_row,
    },
    components::button::Button,
};
use zz_client::agent_completion::{
    AgentCommand, CommandCompletion, command_label, completion_query,
    meaningful_command_description, pane_commands, ranked_completions,
};

use crate::story::{stateless, states};

fn prefix_width(element: impl IntoElement) -> impl IntoElement {
    div().w_full().max_w(px(COMPOSER_MAX_WIDTH)).child(element)
}

pub(crate) fn permission(
    id: &str,
    title: &str,
    counter: Option<&str>,
    options: &[(&str, Option<bool>)],
    highlighted: usize,
    enabled: bool,
    cx: &App,
) -> AnyElement {
    let options = options
        .iter()
        .enumerate()
        .map(|(index, (name, allow))| {
            permission_option(
                (SharedString::from(format!("{id}-option")), index),
                index,
                *name,
                *allow == Some(false),
                index == highlighted,
                enabled,
                cx,
            )
            .into_any_element()
        })
        .collect();
    permission_card(
        title.to_owned(),
        counter.map(|counter| counter.to_owned().into()),
        options,
        Button::new(SharedString::from(format!("{id}-cancel")))
            .disabled(!enabled)
            .tooltip("Cancel request"),
        cx,
    )
    .into_any_element()
}

pub(crate) const COMMAND: [(&str, Option<bool>); 3] = [
    ("Allow", Some(true)),
    ("Always allow this session", Some(true)),
    ("Reject", Some(false)),
];

pub fn slash_commands(query: &str) -> Vec<CommandCompletion> {
    let command = |name: &str, description: &str, hint: Option<&str>| AgentCommand {
        name: name.to_owned(),
        description: description.to_owned(),
        input_hint: hint.map(str::to_owned),
        ..AgentCommand::default()
    };
    let vendor = vec![
        command(
            "compact",
            "Free up context by summarizing the conversation so far",
            Some("instructions"),
        ),
        command("clear", "Start a new session with empty context", None),
        command("stats", "Show usage and activity stats", None),
        command("model", "Switch the model", Some("model")),
        AgentCommand {
            skill: true,
            ..command(
                "security-review",
                "Complete a security review of the pending changes on the current branch",
                None,
            )
        },
        AgentCommand {
            skill: true,
            ..command(
                "codex:setup",
                "Check whether the local Codex CLI is installed and authenticated",
                None,
            )
        },
        AgentCommand {
            skill: true,
            ..command("$hatch-pet", "Create or repair a Codex pet", None)
        },
        command("frontend:lint", "Lint the frontend", Some("[path]")),
        command("statusline", "…", None),
    ];
    let commands = pane_commands(&vendor, true);
    let value = format!("/{query}");
    completion_query(&value, value.len())
        .map(|query| ranked_completions(&commands, &query))
        .unwrap_or_default()
}

pub fn slash_list(
    id: &str,
    completions: &[CommandCompletion],
    selected: Option<usize>,
    cx: &App,
) -> AnyElement {
    let completions: Arc<[CommandCompletion]> = completions.into();
    let count = completions.len();
    let prefix = id.to_owned();
    let rows = uniform_list(SharedString::from(format!("{id}-rows")), count, {
        move |range, _, cx| {
            range
                .filter_map(|index| {
                    let command = &completions.get(index)?.command;
                    Some(suggestion_row(
                        SharedString::from(format!("{prefix}-{index}")),
                        command_label(&command.name),
                        meaningful_command_description(&command.description)
                            .map(|description| description.to_owned().into()),
                        command.skill.then(|| "skill".into()),
                        selected == Some(index),
                        cx,
                    ))
                })
                .collect::<Vec<_>>()
        }
    })
    .w_full()
    .h(suggestion_list_height(count));
    suggestion_list("agent-slash-suggestions", rows, cx).into_any_element()
}

pub fn slash_menu(_: &mut Window, cx: &mut App) -> AnyView {
    stateless(render_slash_menu, cx)
}

fn render_slash_menu(_: &mut Window, cx: &mut App) -> AnyElement {
    states()
        .state(
            "every command, first selected, scrolls past six",
            prefix_width(slash_list("slash-all", &slash_commands(""), Some(0), cx)),
        )
        .state(
            "filtered by /re, second selected",
            prefix_width(slash_list("slash-re", &slash_commands("re"), Some(1), cx)),
        )
        .state(
            "nothing selected yet",
            prefix_width(slash_list("slash-none", &slash_commands("fo"), None, cx)),
        )
        .state(
            "skills, plugin skills and a Codex $skill",
            prefix_width(slash_list(
                "slash-skills",
                &slash_commands("se"),
                Some(0),
                cx,
            )),
        )
        .state(
            "filtered by /st",
            prefix_width(slash_list(
                "slash-terminal",
                &slash_commands("st"),
                Some(0),
                cx,
            )),
        )
        .state(
            "a command without a description",
            prefix_width(slash_list(
                "slash-bare",
                &slash_commands("statusl"),
                Some(0),
                cx,
            )),
        )
        .into_any_element()
}
