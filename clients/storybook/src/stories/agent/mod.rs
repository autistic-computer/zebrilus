use crate::story::{Section, Story};

pub(super) mod cards;
mod chat;
mod chrome;
mod composer;
mod fixtures;
pub(super) mod lens;
mod timeline;
mod transcript;

pub const STORIES: &[Story] = &[STORY, CHAT];

const CHAT: Story = Story {
    id: "agent-chat",
    name: "Agent chat",
    group: "Agent",
    summary: "The driver-backed chat drawn from the chat machine's events: a ChatView folded from fixed event scripts, turned into rows by zz_client::chat_rows and drawn by zz_ui::agent::chat. Runs of tools read as one line, a finished turn folds behind Worked for, and approvals, questions and subagents sit in the thread.",
    sections: &[
        Section {
            id: "live",
            name: "Live turn",
            summary: "One line per run of tools; while a tool runs the line names it, between tools it shows the latest reasoning heading.",
            build: chat::live,
        },
        Section {
            id: "finished",
            name: "Finished turn",
            summary: "When the answer lands, the work before it folds behind Worked for. Opening the fold lists each run; opening a run lists its steps.",
            build: chat::finished,
        },
        Section {
            id: "asks",
            name: "Inline asks",
            summary: "An approval sits on the tool row that needs it, a question sits in the thread, and an answered question leaves a one-line record.",
            build: chat::asks,
        },
        Section {
            id: "subagents",
            name: "Subagents",
            summary: "Subagents started together share one card, a row each: status, name, a one-line preview and how long it took. A row opens in place to show the prompt, the agent's own rows and a footer; an approval waits right under its agent.",
            build: chat::subagents,
        },
        Section {
            id: "background",
            name: "After the turn",
            summary: "A background agent outlives its turn and keeps its approval: the finished turn stays open around it until it is answered.",
            build: chat::background,
        },
        Section {
            id: "try",
            name: "Try it",
            summary: "Click through it: open a fold or a run, open a subagent in place and close it, answer the approval under Replay or the question in the thread and watch it turn into a record.",
            build: chat::try_it,
        },
        Section {
            id: "rows",
            name: "Plans and notices",
            summary: "A plan is a card whose steps update in place; compaction and retries are notice rows.",
            build: chat::rows,
        },
    ],
};

const STORY: Story = Story {
    id: "agent",
    name: "Agent lens",
    group: "Agent",
    summary: "A terminal running Claude Code or Codex shows a lens toggle in its header; the lens draws the session's timeline and a composer in place of the cells, and the toggle brings the terminal back. Every piece of the lens in the states zz draws it in, built from the same zz-ui calls the app makes and fed with fixed transcripts.",
    sections: &[
        Section {
            id: "lens-toggle",
            name: "Lens toggle",
            summary: "terminal_pane_header with a PaneLens: an eye before the pane actions, lit while the lens is open. Like the actions, it shows on the focused pane and waits for the pointer on the others.",
            build: lens::toggles,
        },
        Section {
            id: "user-message",
            name: "User message",
            summary: "Prompts sit right-aligned in a raised bubble. Images come first as 140px tiles that open a full view.",
            build: transcript::user_message,
        },
        Section {
            id: "answer",
            name: "Answer",
            summary: "Assistant text after the last step of a turn. Only the final answer of a finished turn gets a copy button, shown under the pointer.",
            build: transcript::answer,
        },
        Section {
            id: "turn-live",
            name: "Live turn",
            summary: "A running turn folds into one line that says what the agent is doing now. Rows draw no fill; a row you can open brightens under the pointer.",
            build: transcript::turn_live,
        },
        Section {
            id: "turn-done",
            name: "Finished turn",
            summary: "A finished turn shows one badge per kind of step and how long it worked. A turn that ended on a failure shows the last two lines of output.",
            build: transcript::turn_done,
        },
        Section {
            id: "turn-expanded",
            name: "Expanded trace",
            summary: "Opening a turn lists its thoughts, narration and tool calls in order, with subagent steps folded under their agent.",
            build: transcript::turn_expanded,
        },
        Section {
            id: "tool-marks",
            name: "Tool marks",
            summary: "Every tool kind in every status, as rows of an expanded trace. Spinners only run while the turn is live.",
            build: transcript::tool_marks,
        },
        Section {
            id: "replies-and-notices",
            name: "Replies and notices",
            summary: "Answers to /btw and other zz verbs hang under their prompt. Notices from zz itself are muted lines with an info mark.",
            build: transcript::replies,
        },
        Section {
            id: "markdown",
            name: "Markdown",
            summary: "Code blocks get a language label and a copy button that shows under the pointer. Markdown fences render rich in answers, and a streaming answer closes its open markers.",
            build: transcript::markdown,
        },
        Section {
            id: "mermaid",
            name: "Mermaid",
            summary: "Mermaid fences render as diagrams off the main thread, with copy and full view under the pointer. The first frames read Rendering Mermaid, and a source that does not parse shows the error.",
            build: transcript::mermaid,
        },
        Section {
            id: "large-message",
            name: "Large message",
            summary: "Messages over 32 KiB or 512 lines render a preview in a 420px scroll box. Prompts also offer Copy full message.",
            build: transcript::large_message,
        },
        Section {
            id: "composer",
            name: "Composer",
            summary: "The lens's Composer: a pill with + (which starts an @ mention), the field, the context ring and the round action button, all inset the same from its edge. A long draft grows the pill and keeps the buttons on its last line. The branch and its changes sit under it. The action button sends, queues behind a running turn, or stops it.",
            build: composer::composers,
        },
        Section {
            id: "composer-controls",
            name: "Composer controls",
            summary: "The pieces of the composer on their own: action button, the + that starts a mention, context meter and git summary.",
            build: composer::controls,
        },
        Section {
            id: "permission",
            name: "Permission card",
            summary: "A tool call waiting for approval, above the composer. Digits pick an option, Enter confirms, Esc cancels. The lens only ever offers Allow and Deny; the longer lists are the card's other states.",
            build: cards::permissions,
        },
        Section {
            id: "question",
            name: "Question card",
            summary: "Questions the agent asks, one card per request. A single plain question submits on the first pick; anything else waits for Submit. The lens does not draw these yet: Claude's questions stay in the terminal.",
            build: cards::questions,
        },
        Section {
            id: "slash-menu",
            name: "Slash menu",
            summary: "Commands offered while typing a slash: the agent's own plus the zz verbs, ranked by the query.",
            build: cards::slash_menu,
        },
        Section {
            id: "task-tray",
            name: "Task tray",
            summary: "The plan chip and background tasks, above the composer. Each opens a panel; click the chips to toggle.",
            build: cards::trays,
        },
        Section {
            id: "empty-states",
            name: "Empty states",
            summary: "What the lens shows before the first message: the welcome, which names the agent and says the composer types into its terminal, while the agent has written no transcript, the waiting and failure messages, and the error card for a prompt the TUI did not take.",
            build: chrome::empty_states,
        },
        Section {
            id: "jump-to-bottom",
            name: "Jump to latest",
            summary: "The round button that appears over the timeline once you scroll away from the newest message.",
            build: chrome::jump,
        },
        Section {
            id: "lens",
            name: "Lens",
            summary: "The pieces together under the terminal header. Click the toggle to switch between the lens and the terminal it reads; the terminal process stays the source of truth.",
            build: lens::panes,
        },
        Section {
            id: "lens-approval",
            name: "Lens approval",
            summary: "A Claude Code permission prompt becomes a card above the composer whose answers are the keys the TUI takes. Its other dialogs (plan approval, questions) get a card whose one action, Show terminal, switches the pane back to the TUI. Codex records nothing while its dialog is up, so its lens has no card.",
            build: lens::approvals,
        },
    ],
};
