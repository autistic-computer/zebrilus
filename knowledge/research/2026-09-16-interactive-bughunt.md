---
type: Research Report
title: "Interactive bughunt: Agent transcripts, Markdown, and terminal resizing"
description: Three September fixes and the separate resize-corruption and same-session prompt reports that lacked a decisive live trace.
tags:
- bughunt
- agent
- markdown
- terminal
- resize
timestamp: 2026-09-16T22:03:45Z
git_commit: 9816d60a5d0aef02d547413a283374968a2bb6a9
resource:
- crates/zz/src/workspace/view.rs
- crates/zz/src/agent/controller.rs
- libs/penna/src/components/text/inline.rs
- crates/zz-terminal/src/session.rs
---

# Four reports from September 16

The baseline was `9816d60a5`. Three fixes landed. The duplicate terminal paragraphs and the same-session
window-switch report remained unexplained in this investigation. The original agent pane was later
removed. [Agent panes](/concepts/agent-panes.md) ·
[Original investigation and exact test invocations](https://github.com/autistic-computer/zebrilus/blob/56f5a2e1d/knowledge/research/2026-09-16-interactive-bughunt.md)

## BH-001. Missing user prompts

Both providers showed the symptom. On a session switch the desktop could drop its local transcript.
Agent replay then reconstructed user rows from adapter echoes, which were optional. A normal window
switch within a session retained the transcript. No replay or journal reset from the affected pane was
captured.

`8cf1fdf83` journaled submitted text and images as standard user-message chunks on dispatch, split long
text on UTF-8 boundaries, and suppressed active echoes while keeping loaded history. Canceled,
undispatched prompts produced no row. Tests covered both providers, queued prompts, images, repeated
text, session changes and duplicate replay. Existing incomplete journals could still lack the prompt.
Same-session disappearance had no live confirmation. Current daemon source retains the user-chunk path
in `crates/zz-daemon/src/agent/runtime.rs`.

## BH-002. Inline-code wrapping

The screenshot shows the span wrapping after `-p`, with its background extending past the last glyph.
Paragraph-width painting plus 3 px of unreserved horizontal padding caused the original wrap and spacing
defects.

![Wrapped inline code and the unused background after the text](2026-09-16-inline-code-wrap.png)

`a005dd5ff` measured each visual-row fragment, excluded trailing wrap whitespace and reduced paint
padding to 1 px at that revision. The 59 Markdown tests covered multibyte text, a wrap-boundary start,
table alignment and real shaping. A repeat of the supplied native screenshot was not recorded. At
`1ce632dce`, Penna's `libs/penna/src/components/text/inline.rs` owns `code_fill_bounds` and its
wrapped-fragment regression. Its horizontal paint constant is now 4 px.

## BH-003. Duplicate paragraphs after resize

The user reported the failure after pane and whole-window resizing in two Claude terminals, `clairvo` %4
and %8. Dragging a pane improved the display. The panes were observed at 170 × 103 and later 172 × 103.
Their saved captures matched on a second read but did not bracket corruption and recovery. Whether the
program was still emitting output was unknown.

A separate 185 × 65 pane (`wire`, %0) contained four copies of a response opener and a heading indented
by 109 spaces in daemon-side capture. This establishes stored grid or scrollback damage in that pane.
The installed commands reported zz 0.10.0 and Claude Code 2.1.273. The versions of the running processes
were unidentified.

At the baseline, whole-window geometry changes sent each distinct grid size. Split dragging suppressed
intermediate measurements until the daemon acknowledged the final layout. On macOS the actor could
resize before processing already-readable PTY output produced for the old width. That ordering remained
a hypothesis. Existing stale-row and input-saturation tests passed but did not reproduce stored
duplication. Decisive evidence would be a timestamped resize/PTY-byte trace with main-screen state
before/after each size, replayed against an isolated terminal. No such trace was obtained.
[PTY worker](/concepts/pty-worker.md) · [Terminal frames](/concepts/terminal-frame.md)

## BH-004. Intermediate synchronized-output frames

A private shell wrote `BEFORE`, slept 0.5 s, started synchronized output (`CSI ?2026 h`), returned to
column zero and wrote `AFTER!`. It slept 1 s before releasing (`CSI ?2026 l`). Around 0.76 s the
published frame already contained `AFTER!`. A PTY-free fixture confirmed the same bug. The parser had
accepted the bytes correctly. Publication exposed an intermediate redraw.

`b911eab47` held the previous published frame while parsing continued. Release, resize, EOF, exit or a
one-second deadline allowed publication. Live-PTY tests saw `AFTER!` in capture while the displayed
frame still held `BEFORE`. The timeout remains in `crates/zz-terminal/src/session.rs`
(`SYNCHRONIZED_OUTPUT_TIMEOUT`). A held frame does not account for extra paragraphs in stored history.
[Terminal engine](/crates/zz-terminal.md)

At the measured revisions the terminal, text, daemon-agent and desktop-controller suites recorded 550
passes and one ignored test. Scoped Clippy and formatting passed. The affected panes and daemon were
left running.
