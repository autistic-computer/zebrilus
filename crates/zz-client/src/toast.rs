use crate::providers::home_relative;

#[must_use]
pub fn agent_setup(report: &str) -> String {
    let mut installed = Vec::new();
    let mut current = Vec::new();
    let mut removed = Vec::new();
    let mut skipped = Vec::new();
    let mut notes = Vec::new();
    for line in report.lines() {
        let Some((name, rest)) = line.split_once(": ") else {
            continue;
        };
        if rest.starts_with("could not") {
            notes.push(fold_home(line));
        } else if rest.starts_with("skipped") {
            skipped.push(name);
        } else if rest.starts_with("hook current") {
            current.push(name);
        } else if rest.starts_with("hook removed") {
            removed.push(name);
        } else if rest.starts_with("hook ") {
            installed.push(name);
        } else if rest.starts_with("approve") {
            notes.push(format!("Approve it in {name} with /hooks"));
        }
    }
    let mut first = Vec::new();
    if !installed.is_empty() {
        installed.extend(current);
        first.push(format!("Hook installed for {}", names(&installed)));
    } else if !current.is_empty() {
        first.push(format!("Hook already installed for {}", names(&current)));
    }
    if !removed.is_empty() {
        first.push(format!("Hook removed for {}", names(&removed)));
    }
    if !skipped.is_empty() {
        first.push(format!("{} skipped", names(&skipped)));
    }
    if first.is_empty() && notes.is_empty() {
        return report.to_owned();
    }
    let first = first.join(", ");
    (!first.is_empty())
        .then_some(first)
        .into_iter()
        .chain(notes)
        .collect::<Vec<_>>()
        .join("\n")
}

#[must_use]
pub fn tmux_import(output: &str) -> String {
    brief_import(output).unwrap_or_else(|| output.to_owned())
}

fn brief_import(output: &str) -> Option<String> {
    let (head, counts) = output.rsplit_once(": ")?;
    let (source, _) = head.strip_prefix("Imported ")?.split_once(" into ")?;
    let (copied, unsupported) = counts.split_once(", ")?;
    let copied = copied.strip_suffix(" copied")?;
    let unsupported = unsupported.replacen(" commented out as unsupported", " unsupported", 1);
    let source = home_relative(source);
    Some(if unsupported.starts_with("0 ") {
        format!("Imported {copied} from {source}")
    } else {
        format!("Imported {copied} from {source}, {unsupported}")
    })
}

fn fold_home(line: &str) -> String {
    line.split(' ')
        .map(home_relative)
        .collect::<Vec<_>>()
        .join(" ")
}

fn names(names: &[&str]) -> String {
    match names {
        [] => String::new(),
        [only] => (*only).to_owned(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

#[cfg(test)]
mod tests {
    use super::{agent_setup, tmux_import};

    #[test]
    fn a_hook_report_becomes_one_line_and_what_is_left_to_do() {
        let both = "Claude Code: hook added in /Users/me/.claude/settings.json\n\
            Claude Code: skill added at /Users/me/.claude/skills/zz-workspace/SKILL.md\n\
            Claude Code: rule added in /Users/me/.claude/settings.json: Bash(zz:*) runs zz commands without asking\n\
            Codex: hook added in /Users/me/.codex/hooks.json\n\
            Codex: skill current at /Users/me/.agents/skills/zz-workspace/SKILL.md\n\
            Codex: rule current at /Users/me/.codex/rules/zz.rules\n\
            Codex: approve the new hook with /hooks in Codex; until then Codex skips it";
        assert_eq!(
            agent_setup(both),
            "Hook installed for Claude Code and Codex\nApprove it in Codex with /hooks"
        );
        assert_eq!(
            agent_setup(
                "Claude Code: skipped, /Users/me/.claude does not exist\n\
                 Codex: hook current in /Users/me/.codex/hooks.json\n\
                 Codex: skill current at /Users/me/.agents/skills/zz-workspace/SKILL.md"
            ),
            "Hook already installed for Codex, Claude Code skipped"
        );
        assert_eq!(
            agent_setup(
                "Claude Code: hook current in /Users/me/.claude/settings.json\n\
                 Codex: could not write /Users/me/.codex/hooks.json: Permission denied"
            ),
            "Hook already installed for Claude Code\n\
             Codex: could not write ~/.codex/hooks.json: Permission denied"
        );
        assert_eq!(agent_setup("HOME is not set"), "HOME is not set");
    }

    #[test]
    fn a_tmux_import_names_the_file_and_the_counts() {
        assert_eq!(
            tmux_import(
                "Imported /Users/me/.tmux.conf into /Users/me/.config/zz/mux.conf: \
                 38 commands copied, 0 commented out as unsupported"
            ),
            "Imported 38 commands from ~/.tmux.conf"
        );
        assert_eq!(
            tmux_import(
                "Imported /Users/me/.tmux.conf into /Users/me/.config/zz/mux.conf: \
                 36 commands copied, 2 commented out as unsupported (run-shell, if-shell)"
            ),
            "Imported 36 commands from ~/.tmux.conf, 2 unsupported (run-shell, if-shell)"
        );
        assert_eq!(tmux_import("no such file"), "no such file");
    }
}
