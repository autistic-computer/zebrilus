use crate::story::{Section, Story};

pub(super) mod cards;
mod chat;
mod chrome;
mod composer;
mod fixtures;
pub(super) mod pane;

pub const STORIES: &[Story] = &[STORY];

const STORY: Story = Story {
    id: "agent",
    name: "Agent",
    group: "Agent",
    summary: "An agent pane runs Claude Code or Codex for you and draws its session as a chat, with a composer that sends to it. The flip button in the header switches the same pane, on the same session, between the chat and the CLI's own screen in a terminal. Every chat here is folded from fixed chat events and drawn with the calls the apps make: chat_column, chat_thread, chat_agent_pip and an ask hook that draws asks the way the desktop does, on the pane's own fill.",
    sections: &[
        Section {
            id: "flip-button",
            name: "Flip button",
            summary: "pane_header with a PaneFlip: before the pane actions, a chat bubble on a terminal running Claude Code or Codex and a terminal on an agent pane. It sends flip-agent for the pane. It is lit while a flip waits for the agent to be idle and disabled while the daemon answers. Like the actions, it shows on the focused pane and waits for the pointer on the others, unless a flip is armed.",
            build: pane::flip_buttons,
        },
        Section {
            id: "flip-choice",
            name: "Busy choice",
            summary: "When the agent is still working, flip-agent answers busy: with what is running, and the pane shows pane_flip_busy in its corner instead of a dialog: Switch when idle arms the flip (--when-idle), Switch now stops the work and switches (--now). An armed flip shows pane_flip_armed with Cancel (--cancel). Click the flip buttons and the choices; they run zz-client's flip state against a fake daemon.",
            build: pane::choices,
        },
        Section {
            id: "composer",
            name: "Composer",
            summary: "The agent pane's Composer: a pill with + (which starts an @ mention), the field, the context ring and the round action button, all inset the same from its edge. A long draft grows the pill and keeps the buttons on its last line. The branch and its changes sit under it. The action button sends, queues behind a running turn, or stops it.",
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
            summary: "Commands offered while typing a slash: the ones the agent reports as working in a chat plus the zz verbs, ranked by the query. Commands that need the CLI's own screen are left out; flip the pane to the CLI to use them.",
            build: cards::slash_menu,
        },
        Section {
            id: "live",
            name: "Live turn",
            summary: "The last line of a running turn says what the agent is doing now, with a turning sparkle, a shimmer and the time since the prompt. It takes the newest of three things: the CLI's own activity line, the running tool's own description (a command's own text when it has none), and the latest reasoning heading. It says Thinking only when nothing more specific is known. While the newest run of tools is still going, the line opens to its steps. A reply that lands while you watch types itself in over under a second.",
            build: chat::live,
        },
        Section {
            id: "batches",
            name: "Batch summaries",
            summary: "Claude Code sends one plain sentence for each finished batch of tool calls and names the calls by id; zz puts it on that batch's line, which opens to the calls. A sentence that lands after the reply, or after a replay, still finds its calls. Codex and subagents send none, so their runs keep zz's own count of what ran, and Codex's live line comes from the active item: the command, the tool's title, Waiting for agents, or the reasoning heading.",
            build: chat::batches,
        },
        Section {
            id: "finished",
            name: "Finished turn",
            summary: "When the reply lands, the work before it folds behind Worked for. Opening the fold lists each run; opening a run lists its steps and its thinking, which scrolls inside 260 px. A run counts its failed and denied steps. A turn you stopped folds behind You stopped after, and a failed one behind Failed after with a red mark.",
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
            summary: "Your messages sit right-aligned in a raised bubble that caps its height and scrolls; replies have no bubble. Hovering a message shows when it was sent, Copy and Fork, muted until the pointer is on them; on a reply, Copy takes every text of the turn.",
            build: chat::messages,
        },
        Section {
            id: "markdown",
            name: "Markdown",
            summary: "Replies render as markdown with the agent's extensions: headings, lists, tables and quotes, code blocks, a markdown fence drawn rich, and Mermaid and SVG fences drawn as figures. Mermaid flowchart, sequence, class, state, ER, gantt, pie and quadrant diagrams draw in the theme's colors on a transparent canvas; any other Mermaid type stays a plain code block. A figure sits in a container with a header holding its label, Copy source and Open full diagram; a source that does not parse shows the error with its line.",
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
            summary: "Approvals and questions sit in the thread, drawn the way the desktop draws them: an approval on the tool row that needs it, with its options as tiles (digits pick, Enter confirms, Esc cancels) that grey out once you answer, until the CLI moves on; the first open question as the question card; anything behind the first ask greyed out until its turn; and an answered question as a record. Click an option to answer it.",
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
            id: "empty-states",
            name: "Empty states",
            summary: "What an agent pane shows before the first message: the welcome, which names the agent and points at the flip button for the CLI's own screen, the starting, waiting and failure messages, and the error card for a turn that failed.",
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
            id: "agent-pane",
            name: "Agent pane",
            summary: "The pieces together under the pane header: the chat, the task tray and the composer. Click the flip button to switch a pane between the chat and the CLI in a terminal; a pane that is busy asks first.",
            build: pane::panes,
        },
    ],
};
