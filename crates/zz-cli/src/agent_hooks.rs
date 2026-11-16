use std::{ffi::OsString, process::ExitCode};

use zz_daemon::{
    AgentHookChange, WORKSPACE_CONTEXT_SENT_ENVIRONMENT_VARIABLE, agent_hook_lines,
    workspace_context, workspace_tools,
};
use zz_protocol::{RawText, catalog_command_spec};

pub(crate) fn run(arguments: &[RawText]) -> ExitCode {
    let (flag, change) = match arguments {
        [flag] if flag == "--context" => {
            if let Some(text) = context(|name| std::env::var_os(name)) {
                print!("{text}");
            }
            return ExitCode::SUCCESS;
        }
        [flag, rest @ ..] if flag == "--fork-chat" => return crate::fork_chat::run(rest),
        [flag] if flag == "--install-hooks" => (flag, AgentHookChange::Install),
        [flag] if flag == "--remove-hooks" => (flag, AgentHookChange::Remove),
        _ => return guide(arguments),
    };
    let lines = match agent_hook_lines(change) {
        Ok(lines) => lines,
        Err(error) => {
            eprintln!("tools {flag}: {error}");
            return ExitCode::from(1);
        }
    };
    let mut failed = false;
    for line in lines {
        match line {
            Ok(line) => println!("{line}"),
            Err(line) => {
                failed = true;
                eprintln!("{line}");
            }
        }
    }
    if failed {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

fn guide(arguments: &[RawText]) -> ExitCode {
    match workspace_tools(arguments) {
        Ok(text) => {
            print!("{text}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            let error = match catalog_command_spec("tools") {
                Some(spec) => spec.classify_usage_error(error),
                None => error,
            };
            eprintln!("{}", error.tmux_message());
            ExitCode::from(error.exit_code())
        }
    }
}

fn context(var: impl Fn(&str) -> Option<OsString>) -> Option<String> {
    let set = |name: &str| var(name).filter(|value| !value.is_empty());
    if set(WORKSPACE_CONTEXT_SENT_ENVIRONMENT_VARIABLE).is_some() {
        return None;
    }
    let pane = set("ZZ_PANE")?;
    let session = set("ZZ_SESSION").unwrap_or_default();
    Some(workspace_context(
        &pane.to_string_lossy(),
        &session.to_string_lossy(),
        false,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<OsString> + use<> {
        let pairs = pairs
            .iter()
            .map(|(name, value)| ((*name).to_owned(), OsString::from(value)))
            .collect::<Vec<_>>();
        move |name| {
            pairs
                .iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.clone())
        }
    }

    #[test]
    fn context_needs_a_pane_and_stays_quiet_once_sent() {
        assert_eq!(context(env(&[])), None);
        assert_eq!(context(env(&[("ZZ_PANE", "")])), None);
        let text = context(env(&[("ZZ_PANE", "%7"), ("ZZ_SESSION", "work")])).expect("context");
        assert!(text.contains("ZZ_PANE=%7\n"));
        assert!(text.contains("ZZ_SESSION=work\n"));
        assert!(text.contains("'#{pane_id}'"));
        assert_eq!(
            context(env(&[
                ("ZZ_PANE", "%7"),
                (WORKSPACE_CONTEXT_SENT_ENVIRONMENT_VARIABLE, "1")
            ])),
            None
        );
    }
}
