---
type: Design Plan
title: English to command translator
description: A small local model that takes an English sentence and the state of the current session and turns it into zz commands, which run at once with no preview.
status: "Proposed 2026-11-26; nothing built, no model chosen, no measurements"
resource: ../../crates/zz-protocol/src/catalog.rs
tags:
- commands
- tmux
- model
- input
timestamp: 2026-11-26T00:00:00-03:00
---

# What it is

You type an English sentence. A small model that runs on your machine turns it into one or more
zz commands, and zz runs them immediately. There is no preview and no confirmation, including for
commands that destroy something. The model sees the session you are in, so "the left one" refers
to a real pane.

It is for a person typing. Agents can call it, but human phrasing sets the bar.

# Decisions

| Question | Decision |
| --- | --- |
| Who it serves | A person typing a sentence. Agent callers are a side effect. |
| Language | English only. |
| What it emits | Commands from the [command catalog](/tmux/commands.md), the same text a user could type after `:` in the [palette](/concepts/command-palette.md). |
| Preview | None. Translated commands run at once. |
| Destructive commands | Run at once like the rest. No extra confirmation. |
| What it sees | The sentence and the state of the current session. Nothing from other sessions or hosts. |
| Where it runs | Locally. No network call and no account. |

# Shape

The model is a fine-tuned translator: one small model trained for this one command surface. A
general shell translator of the same size is not the target; a single-tool specialist is.

Its output is constrained while it decodes, in two ways:

- **Commands that parse.** The grammar is derived from `COMMAND_SPECS` and `DAEMON_COMMAND_SPECS`
  in `crates/zz-protocol/src/catalog.rs`: command names and aliases, each command's options and
  their value kinds, positionals. The model cannot emit a command the catalog would reject.
- **Targets that exist.** Where a command takes a session, window or pane target, the allowed
  values are the ones present in the session state given with the sentence.

Running with no preview depends on both. The constraint guarantees a valid command, not the
intended one.

One sentence may produce several commands in order ("split this in three and run the tests in the
last one"). The output is a command sequence, joined the way the catalog already joins them.

# Input

Two parts:

1. The sentence.
2. A text description of the current session: its windows, each window's layout and panes, which
   window and pane are active, pane kinds, names and working directories. The source is the
   snapshot the daemon already sends clients (`PaneSnapshot`, `PaneSummary` and `LayoutNode` in
   `crates/zz-protocol/src/snapshot.rs`).

Positional words ("left", "top", "the big one") resolve against the layout. Named references
resolve against window and pane names.

# Training data

A large model writes the pairs. For each command in the catalog it produces English sentences a
person would type, each paired with a session state and the command sequence that satisfies it.
Every pair is checked by parsing the command against the catalog and resolving its targets
against the paired state; pairs that fail are dropped.

The catalog changes, so generating the data and training are a repeatable run keyed to a catalog
revision, not a one-off.

# Out of scope

- Languages other than English.
- State outside the current session.
- Shell commands typed into a pane. The translator emits multiplexer commands only; it can send
  text to a pane only through a catalog command that does so.
- A preview or edit-before-run mode.
- Other apps. The data pipeline is specific to this catalog.

# Open

- **How much pane state the model sees.** Layout and names cover "the left one". "The one running
  tests" needs the program or last command in each pane, which `PaneSummary` carries but which
  lengthens the input on every call.
- **Speed.** The idea only works if translation feels immediate. No size, runtime or latency
  target is set, and none has been measured.
- **Which base model and how small.** Undecided.
- **Where the sentence is typed.** The palette, a prompt of its own, or the CLI.
- **What happens when no command fits.** The grammar forces a valid command, so a sentence the
  model cannot satisfy needs a way to produce nothing.
- **Which commands are in.** The whole catalog, or the subset a person would say out loud.
