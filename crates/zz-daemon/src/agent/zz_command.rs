use std::path::Path;

use serde_json::Value;

const SHELLS: [&str; 4] = ["sh", "bash", "zsh", "dash"];
const SHELL_FLAGS: [&str; 3] = ["-c", "-lc", "-cl"];
const ZZ_PROGRAMS: [&str; 2] = ["zz", "zz-dev"];
const HELPERS: [&str; 4] = ["cd", "echo", "printf", "sleep"];
const FILTERS: [&str; 8] = ["head", "tail", "grep", "rg", "wc", "jq", "cut", "tr"];

type Stage = Vec<String>;
type Pipeline = Vec<Stage>;

pub(crate) fn runs_only_zz(command: &Value) -> bool {
    match command {
        Value::String(text) => match chain(text).as_deref() {
            Some([pipeline]) if pipeline.len() == 1 => match shell_script(&pipeline[0]) {
                Some(script) => script_runs_only_zz(script),
                None => script_runs_only_zz(text),
            },
            _ => script_runs_only_zz(text),
        },
        Value::Array(parts) => {
            let Some(words) = parts
                .iter()
                .map(|part| part.as_str().map(str::to_owned))
                .collect::<Option<Vec<_>>>()
            else {
                return false;
            };
            match shell_script(&words) {
                Some(script) => script_runs_only_zz(script),
                None => is_zz(&words),
            }
        }
        _ => false,
    }
}

fn shell_script(words: &[String]) -> Option<&str> {
    match words {
        [shell, flag, script]
            if SHELLS.contains(&program(shell)) && SHELL_FLAGS.contains(&flag.as_str()) =>
        {
            Some(script)
        }
        _ => None,
    }
}

fn script_runs_only_zz(script: &str) -> bool {
    let Some(pipelines) = chain(script) else {
        return false;
    };
    let mut ran_zz = false;
    for pipeline in pipelines.iter().filter(|pipeline| !pipeline.is_empty()) {
        let [first, rest @ ..] = pipeline.as_slice() else {
            return false;
        };
        if is_zz(first) {
            ran_zz = true;
        } else if !is_program(first, &HELPERS) {
            return false;
        }
        if !rest.iter().all(|stage| is_filter(stage)) {
            return false;
        }
    }
    ran_zz
}

fn is_zz(words: &[String]) -> bool {
    is_program(words, &ZZ_PROGRAMS)
}

fn is_filter(words: &[String]) -> bool {
    is_program(words, &FILTERS)
        && !(program(&words[0]) == "rg" && words.iter().any(|word| word.starts_with("--pre")))
}

fn is_program(words: &[String], names: &[&str]) -> bool {
    words
        .first()
        .is_some_and(|first| !first.contains('=') && names.contains(&program(first)))
}

fn program(word: &str) -> &str {
    Path::new(word)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(word)
}

fn chain(text: &str) -> Option<Vec<Pipeline>> {
    let mut pipelines: Vec<Pipeline> = vec![vec![Vec::new()]];
    let mut word: Option<String> = None;
    let mut chars = text.chars().peekable();
    let end_word = |word: &mut Option<String>, pipelines: &mut Vec<Pipeline>| -> Option<()> {
        if let Some(word) = word.take() {
            pipelines.last_mut()?.last_mut()?.push(word);
        }
        Some(())
    };
    while let Some(ch) = chars.next() {
        match ch {
            '\'' => {
                let word = word.get_or_insert_default();
                loop {
                    match chars.next()? {
                        '\'' => break,
                        ch => word.push(ch),
                    }
                }
            }
            '"' => {
                let word = word.get_or_insert_default();
                loop {
                    match chars.next()? {
                        '"' => break,
                        '`' => return None,
                        '$' if chars.peek() == Some(&'(') => return None,
                        '\\' => {
                            let next = chars.next()?;
                            if !matches!(next, '"' | '\\' | '$' | '`') {
                                word.push('\\');
                            }
                            word.push(next);
                        }
                        ch => word.push(ch),
                    }
                }
            }
            '\\' => word.get_or_insert_default().push(chars.next()?),
            ' ' | '\t' => end_word(&mut word, &mut pipelines)?,
            '>' => {
                if !matches!(word.as_deref(), None | Some("1" | "2")) {
                    return None;
                }
                word = None;
                if chars.next_if_eq(&'&').is_some() {
                    chars.next_if(|ch| matches!(ch, '1' | '2'))?;
                } else {
                    while chars.next_if(|ch| matches!(ch, ' ' | '\t')).is_some() {}
                    let target = chars.by_ref().take_while(|ch| !matches!(ch, ' ' | '\t'));
                    if target.collect::<String>() != "/dev/null" {
                        return None;
                    }
                }
            }
            '&' if chars.next_if_eq(&'>').is_some() => {
                end_word(&mut word, &mut pipelines)?;
                while chars.next_if(|ch| matches!(ch, ' ' | '\t')).is_some() {}
                let target = chars.by_ref().take_while(|ch| !matches!(ch, ' ' | '\t'));
                if target.collect::<String>() != "/dev/null" {
                    return None;
                }
            }
            '|' if chars.next_if_eq(&'|').is_none() => {
                end_word(&mut word, &mut pipelines)?;
                pipelines.last_mut()?.push(Vec::new());
            }
            ';' | '&' | '|' => {
                if ch == '&' && chars.next_if_eq(&'&').is_none() {
                    return None;
                }
                end_word(&mut word, &mut pipelines)?;
                pipelines.push(vec![Vec::new()]);
            }
            '<' | '(' | ')' | '`' | '\n' | '\r' => return None,
            '$' if chars.peek() == Some(&'(') => return None,
            ch => word.get_or_insert_default().push(ch),
        }
    }
    end_word(&mut word, &mut pipelines)?;
    for pipeline in &mut pipelines {
        if pipeline.len() > 1 && pipeline.iter().any(Vec::is_empty) {
            return None;
        }
        if pipeline.len() == 1 && pipeline[0].is_empty() {
            pipeline.clear();
        }
    }
    Some(pipelines)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::runs_only_zz;

    fn only_zz(command: &str) -> bool {
        runs_only_zz(&json!(command))
    }

    #[test]
    fn codex_shell_wrappers_around_zz_commands_qualify() {
        assert!(only_zz("/opt/homebrew/bin/bash -lc 'zz list-panes -a'"));
        assert!(only_zz(
            r#"/opt/homebrew/bin/bash -lc 'zz list-panes -s -t "$ZZ_SESSION" -F '"'"'#{pane_id} #{window_name}'"'"''"#
        ));
        assert!(only_zz(r#"bash -c "zz send-keys -t %2 \"echo hi\" Enter""#));
        assert!(only_zz(
            "zsh -lc 'zz capture-pane -p -t %2 && zz send-keys -t %2 y Enter; zz list-panes'"
        ));
        assert!(only_zz("zz run-pane -t %1 -- ./test-log.sh"));
        assert!(only_zz("/usr/local/bin/zz-dev list-sessions"));
        assert!(runs_only_zz(&json!(["bash", "-lc", "zz list-panes -a"])));
        assert!(runs_only_zz(&json!(["zz", "list-panes"])));
    }

    #[test]
    fn zz_output_may_flow_through_text_filters_and_harmless_helpers() {
        for command in [
            r#"/opt/homebrew/bin/bash -lc "zz capture-pane -p -J -t %2 -S - | rg 'FAILED|fail(ed)?'""#,
            "zz tools --skill 2>&1 | head -150",
            r#"echo "PANE=$ZZ_PANE"; zz tools --skill"#,
            "sleep 5 && zz capture-pane -p -t %3 | tail -n 5 | grep DONE-",
            "cd /tmp && zz list-panes 2>/dev/null",
            "zz list-panes -a -F '#{pane_id}' | wc -l",
            "zz list-panes --json | jq -r .pane_id >/dev/null",
            "zz list-panes &> /dev/null || zz list-sessions",
        ] {
            assert!(only_zz(command), "{command}");
        }
    }

    #[test]
    fn anything_besides_zz_needs_the_user() {
        for command in [
            "bash -lc 'zz list-panes; rm -rf ~'",
            "bash -lc 'zz list-panes | sh'",
            "zz list-panes | xargs rm",
            "zz list-panes | sort -o /etc/hosts",
            "zz list-panes | rg --pre=sh x",
            "zz list-panes | rg --pre sh x",
            "rg x | zz list-panes",
            "zz list-panes |",
            "| zz list-panes",
            "bash -lc 'zz list-panes > /tmp/out'",
            "zz list-panes >> /tmp/out",
            "zz list-panes 2>/tmp/err",
            "zz list-panes &> /tmp/out",
            "zz list-panes x>/dev/null",
            "bash -lc 'zz list-panes $(rm -rf ~)'",
            r#"bash -lc 'zz list-panes "$(rm x)"'"#,
            "bash -lc 'zz list-panes `rm x`'",
            "bash -lc 'zz list-panes & rm x'",
            "bash -lc 'cat SKILL.md && zz list-panes'",
            "bash -lc 'ZZ_SOCKET=/x zz list-panes'",
            "bash -lc 'X=/bin/zz rm -rf ~'",
            "X=/bin/zz rm -rf ~",
            "bash -lc 'zz list-panes\nrm x'",
            "bash -lc '(zz list-panes)'",
            "bash -lc 'zz list-panes",
            "for p in %1 %2; do zz capture-pane -p -t $p; done",
            "zz list-panes < /etc/passwd",
            "echo hi; sleep 1",
            "bash -lc 'rm -rf ~'",
            "python3 -c 'import os'",
            "bash -lc ''",
            "",
        ] {
            assert!(!only_zz(command), "{command}");
        }
        assert!(!runs_only_zz(&json!(["rm", "-rf", "/"])));
        assert!(!runs_only_zz(&json!(["bash", "-lc", "zz a; rm b"])));
        assert!(!runs_only_zz(&json!(null)));
    }
}
