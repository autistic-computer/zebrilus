#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AgentBlockPolicy {
    #[default]
    Wait,
    Fail,
    Allow,
    Deny,
}

pub const PARENT_CLAUDE_SESSION_ENVIRONMENT: &[&str] = &[
    "CLAUDECODE",
    "CLAUDE_CODE_CHILD_SESSION",
    "CLAUDE_CODE_SESSION_ID",
    "CLAUDE_CODE_SESSION_ATTENDED",
    "CLAUDE_CODE_ENTRYPOINT",
    "CLAUDE_CODE_EXECPATH",
    "CLAUDE_CODE_MESSAGING_SOCKET",
    "CLAUDE_CODE_MESSAGING_TOKEN",
    "CLAUDE_PID",
    "CLAUDE_EFFORT",
];
pub const TMUX_SHIM_EXECUTABLE_ENVIRONMENT_VARIABLE: &str = "ZZ_TMUX_EXECUTABLE";
pub const TMUX_SHIM_DIRECTORY_ENVIRONMENT_VARIABLE: &str = "ZZ_TMUX_SHIM_DIR";

pub const CODEX_RULES: &str = "# Written by zz tools --install-hooks; zz tools --remove-hooks deletes it.\nprefix_rule(pattern = [\"zz\"], decision = \"allow\")\n";

const WORKSPACE_CONTEXT: &str = r#"You're running in a zz pane. zz is a tmux-compatible multiplexer: run its commands as `zz`, which is on PATH and already targets this workspace (a login shell can find a real `tmux` first).

```text
ZZ_SESSION={session}
ZZ_PANE={pane}
```

`ZZ_PANE` is your own pane and `ZZ_SESSION` your session. Write these ids into commands as they are, not as `$ZZ_PANE` or `$ZZ_SESSION`: an approval rule can't see what a variable holds. The user may be looking at a different pane. Pick the target from the request:

- "Here", "beside you", or no place named: your own pane, `-t {pane}`.
- "The pane I'm looking at" or "the focused pane": the session's active pane, `zz display-message -p -t '{session}' '#{pane_id}'`.
- A pane or window by name: find it with `zz list-panes -s -t '{session}' -F '#{pane_id} #{window_name} #{pane_current_command}'`. An explicit target wins.

Target panes by id (`-t %N`), never `session:%N`. Keep the ids that creating commands print (`-P -F '#{pane_id}'`), and pass `-d` so you don't take focus from the user. If a creating command times out, list panes before retrying.

Put a pane where its work belongs. What serves your own task (a preview, a server, a log) splits your own window, never a new one: `zz split-window -d -h -t {pane} --kind browser URL` puts a page beside you, `-v -l 6` makes a small terminal, and a server can be a background task instead. Work that is not part of the window you would put it in gets a session of its own, named for the task, above all on another machine: `zz [--host NAME] new-session -d -P -F '#{pane_id}' -s TASK`.

Run a command in another pane with `zz run-pane -t %N -- CMD` (prints its output and exits with its status); read a pane with `zz capture-pane -p -J -t %N -S -`. If a `zz` command fails with "Operation not permitted", a sandbox kept it from the daemon: rerun it with escalated permissions.

A chat you open is a pane the user sees. Open one only for work they watch or steer: a task they asked you to hand off, a fork of a conversation, a task for another machine. `zz new-pane -d --kind agent claude|codex|agy|grok|cursor [--model M] [--effort E] 'BRIEF'` opens it and prints how its reply reaches you; `--model` and `--effort` go to that agent's CLI unchecked, so write them its way. Background work of your own (searching, reading, split-up edits) goes to your own subagents, never a zz chat.

Other machines: `zz list-hosts` names them and `zz --host NAME VERB -t %N` runs a verb there (pane IDs are per host), `zz --host NAME run-shell 'CMD'` a command with no pane. Run `zz tools --skill` for the full guide.
"#;

const AGENT_PANE_CONTEXT: &str = concat!(
    r"
This is an Agent pane, and it draws UI blocks and HTML you put in a reply.

To ask the user something, use your own question tool: it shows here as a card. Another pane is never the user, and a terminal runs whatever is typed into it.

- To show a comparison, stats, a chart, a form or a small control, open a fenced block with ```ui and write it as described under UI blocks below. It is drawn with the app's own components in the user's theme. A button press reaches you as the user's next message, on one line: `ui action NAME {...}` with the values that action listed. `ui problems [...]` means the block could not be drawn: answer with a corrected block.
- To show a page you are building (a site, a screen of the user's product), put the whole document, starting with `<!doctype html>`, in a plain ```html block. It keeps its own styles and is drawn in a frame at desktop size; a click opens it beside the chat.
- For a page with files of its own, or one you will keep changing, write it to disk and run `zz html PATH` once. It opens beside this chat and redraws when its files change.

Unless auto-approve is off, this pane approves `zz` commands without asking, alone or piped into `head`, `tail`, `grep`, `rg`, `wc`, `jq`, `cut` or `tr`, with no redirect but `2>&1` or `>/dev/null`.

UI blocks

",
    include_str!("../../../libs/penna/src/blocks/a2ui/express.md")
);

pub const WORKSPACE_CONTEXT_SENT_ENVIRONMENT_VARIABLE: &str = "ZZ_AGENT_CONTEXT";

#[must_use]
pub fn workspace_context(pane: &str, session: &str, agent_pane: bool) -> String {
    let context = WORKSPACE_CONTEXT
        .replace("{session}", session)
        .replace("{pane}", pane);
    if agent_pane {
        context + AGENT_PANE_CONTEXT
    } else {
        context
    }
}
