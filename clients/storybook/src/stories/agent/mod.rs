use crate::story::{Section, Story};

pub(super) mod cards;
mod chat;
mod chrome;
mod composer;
mod fixtures;
pub(super) mod lens;

pub const STORIES: &[Story] = &[STORY];

const STORY: Story = Story {
    id: "agent",
    name: "Agent",
    group: "Agent",
    summary: "A terminal running Claude Code or Codex shows a lens toggle in its header. The lens draws the CLI's session as a chat in place of the cells, with a composer that types into the CLI, and the toggle brings the terminal back. Every chat here is folded from fixed chat events and drawn with the calls the apps make: chat_column, chat_thread, chat_agent_pip and an ask hook that draws asks the way the desktop does, on the pane's own fill.",
    sections: &[
        Section {
            id: "lens-toggle",
            name: "Lens toggle",
            summary: "terminal_pane_header with a PaneLens: an eye before the pane actions, lit while the lens is open. Like the actions, it shows on the focused pane and waits for the pointer on the others.",
            build: lens::toggles,
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
            summary: "The pieces of the composer on their own: the action button, the + that starts a mention, the context meter, the git summary, and the footer's folder, model, effort and side chat controls.",
            build: composer::controls,
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
            id: "live",
            name: "Live turn",
            summary: "One line per run of tools; while a tool runs the line names it, between tools it shows the latest reasoning heading. The bottom line says Thinking or Working with a shimmer, a turning sparkle and the time since the prompt. A reply that lands while you watch types itself in over under a second.",
            build: chat::live,
        },
        Section {
            id: "finished",
            name: "Finished turn",
            summary: "When the reply lands, the work before it folds behind Worked for. Opening the fold lists each run; opening a run lists its steps. A run counts its failed and denied steps. A turn you stopped folds behind You stopped after, and a failed one behind Failed after with a red mark.",
            build: chat::finished,
        },
        Section {
            id: "steps",
            name: "Steps",
            summary: "Every kind of step in every status, as the steps of an opened run. A running step spins and the run's line names it; a failed step turns red, a declined one takes the warning color, and an interrupted one stays plain.",
            build: chat::steps,
        },
        Section {
            id: "messages",
            name: "Messages",
            summary: "Your messages sit right-aligned in a raised bubble that caps its height and scrolls; replies have no bubble. Hovering a message shows when it was sent, Copy and Fork; on a reply, Copy takes every text of the turn.",
            build: chat::messages,
        },
        Section {
            id: "markdown",
            name: "Markdown",
            summary: "Replies render as markdown with the agent's extensions: headings, lists, tables and quotes, code blocks, a markdown fence drawn rich, and Mermaid and SVG fences drawn as figures. A figure sits in a container with a header holding its label, Copy source and Open full diagram; a source that does not parse shows the error.",
            build: chat::markdown,
        },
        Section {
            id: "references",
            name: "References",
            summary: "zz references in a message, written as @host:session:window.%pane, a pane or session id in code, or a file path in code, show as named links: a pane by its program and window, a chat by its title, a file by its name and line. Clicking one switches to it, or opens the file or folder in a float.",
            build: chat::references,
        },
        Section {
            id: "asks",
            name: "Asks",
            summary: "Approvals and questions sit in the thread, drawn the way the desktop draws them: an approval on the tool row that needs it, with its options as tiles (digits pick, Enter confirms, Esc cancels) that grey out once you answer, until the CLI moves on; the first open question as the question card; a dialog the lens cannot answer as a card whose one action shows the terminal; anything behind the first ask greyed out until its turn; and an answered question as a record. Click an option to answer it.",
            build: chat::asks,
        },
        Section {
            id: "subagents",
            name: "Subagents",
            summary: "Each subagent is its own row: the CLI's mark, its name, a status glyph (spinning while it works, Needs you, done, failed) and how long it took. Clicking a row opens its chat in a PiP over the thread: its prompt, its own rows, its answer and a footer; the PiP's close button or a second click closes it. An approval waits right under its agent.",
            build: chat::subagents,
        },
        Section {
            id: "background",
            name: "After the turn",
            summary: "A background agent outlives its turn and keeps its approval: the finished turn stays open around it until it is answered.",
            build: chat::background,
        },
        Section {
            id: "rows",
            name: "Plans and notices",
            summary: "A plan is a card whose steps update in place. Notices from zz itself, like a compaction, a fork or a rewind, are muted rows with an info mark; errors are marked red.",
            build: chat::rows,
        },
        Section {
            id: "terminal-pip",
            name: "Terminal PiP",
            summary: "Commands that only the CLI's own screen can show (/config, /status, /usage, /mcp), and dialogs the lens cannot answer, open the pane's live terminal in a wide PiP over the chat, pinned to the bottom of the screen where the CLI draws its dialog. Keys go to the CLI. The PiP closes when the CLI's input box comes back; Esc (Codex) or Done close it too.",
            build: lens::sheets,
        },
        Section {
            id: "empty-states",
            name: "Empty states",
            summary: "What the lens shows before the first message: the welcome, which names the agent and says the composer types into its terminal, the waiting and failure messages, and the error card for a prompt the TUI did not take.",
            build: chrome::empty_states,
        },
        Section {
            id: "jump-to-bottom",
            name: "Jump to latest",
            summary: "The round button that appears over the chat once you scroll away from the newest message. Clicking it follows the bottom again.",
            build: chrome::jump,
        },
        Section {
            id: "try",
            name: "Try it",
            summary: "Click through it: open a fold or a run, open a subagent in its PiP and close it, answer the approval under Replay, then the question that was waiting behind it, and watch it turn into a record. Reset starts over.",
            build: chat::try_it,
        },
        Section {
            id: "lens",
            name: "Lens",
            summary: "The pieces together under the terminal header: the chat, the task tray and the composer. Click the toggle to switch between the lens and the terminal it reads; the terminal process stays the source of truth.",
            build: lens::panes,
        },
    ],
};
