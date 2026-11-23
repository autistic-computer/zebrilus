# Chat

The pieces of an agent chat. Each takes plain data and callbacks, so an app keeps its own transcript model and feeds the block what to draw.

```figure uses=notice
<div class="penna-notice" data-penna="notice" data-tone="warning">
  <div class="penna-notice-tone" data-penna="chip" data-tone="warning">
    <span class="penna-notice-lead"><i data-icon="triangle-alert"></i></span>
    <div class="penna-notice-body">
      <span class="penna-text penna-notice-title" data-penna-text>Gap against the spec</span>
      <span class="penna-notice-detail">Desktop chat matches. On a phone, the queued-messages sheet, empty-chat Folder and Resume rows, transcript image zoom, and <code>@</code> mention sheet are not built yet; <code>phone_plan_tray</code> has no HTML twin.</span>
    </div>
  </div>
</div>
```

## Activity rows

One row per tool call, thought, or summary: an icon or running spinner, the label, a smaller detail in the same muted ink on its baseline, a tag, per-kind counts, and a chevron that opens it. While the agent works, the live line shows a muted label with no glyph; each new label comes into focus from left to right, a character at a time, from a soft blur to sharp.

```figure uses=chat,badge,tooltip
<div class="docs-show" data-gap="wide" style="width: 100%; flex-direction: column; align-items: stretch; gap: 20px">
  <div class="docs-cell" style="width: 100%">
    <div class="penna-chat-activity"><span class="penna-chat-label penna-chat-resolve" data-resolve-labels="Thinking|Reading agent/view.rs|Searching for popover">Thinking…</span><span class="penna-chat-detail" data-penna-text>14s</span></div>
    <div class="penna-chat-activity"><span class="penna-chat-glyph"><i data-icon="file"></i></span><span class="penna-chat-label" data-penna-text>Read tab_strip.rs</span></div>
    <div class="penna-chat-activity"><span class="penna-chat-glyph"><i data-icon="search"></i></span><span class="penna-chat-label" data-penna-text>Searched for turn_id</span><span class="penna-chat-detail" data-penna-text>crates/zz-client</span></div>
    <div class="penna-chat-activity"><span class="penna-chat-glyph"><i data-icon="loader"></i></span><span class="penna-chat-label" data-strong data-penna-text>cargo test -p zz-ui</span></div>
    <div class="penna-chat-activity"><span class="penna-chat-glyph" data-tint="danger"><i data-icon="square-terminal"></i></span><span class="penna-chat-label" data-penna-text>cargo clippy</span><span style="display: flex; flex: none"><span class="penna-badge" data-penna="badge" data-tone="danger"><span class="badge-text" data-penna-text>exit 1</span></span></span></div>
    <div class="penna-chat-activity"><span class="penna-chat-glyph" data-tint="warning"><i data-icon="square-terminal"></i></span><span class="penna-chat-label" data-penna-text>rm -rf target</span><span style="display: flex; flex: none"><span class="penna-badge" data-penna="badge"><span class="badge-text" data-penna-text>needs approval</span></span></span></div>
    <div class="penna-chat-activity" data-clickable><span class="penna-chat-label" data-penna-text>Worked for 2m 14s</span><span class="penna-chat-counts"><span class="penna-chat-count"><i data-icon="file"></i><span class="penna-badge" data-penna="badge" data-size="small" data-opaque><span class="badge-text" data-penna-text>2</span></span></span><span class="penna-chat-count"><i data-icon="pencil"></i><span class="penna-badge" data-penna="badge" data-size="small" data-opaque><span class="badge-text" data-penna-text>1</span></span></span><span class="penna-chat-count" data-failed><i data-icon="triangle-alert"></i><span class="penna-badge" data-penna="badge" data-tone="danger" data-size="small" data-opaque data-emphasis="solid" data-solid="danger"><span class="badge-text" data-penna-text>1</span></span></span></span><span class="penna-chat-glyph penna-chat-chevron"><i data-icon="chevron-right"></i></span></div>
    <div class="penna-chat-activity" data-clickable><span class="penna-chat-glyph"><i data-icon="square-terminal"></i></span><span class="penna-chat-label" data-penna-text>just test</span><span class="penna-chat-glyph penna-chat-chevron"><i data-icon="external-link"></i></span></div>
    <span class="docs-note">Live resolving line, tool steps, failed/waiting tags, summary counts, and open output</span>
  </div>
  <div class="docs-cell" style="width: min(100%, 280px)">
    <div class="penna-chat-activity" data-clickable><span class="penna-chat-glyph"><i data-icon="square-terminal"></i></span><span class="penna-chat-label" data-penna-text>cargo test -p zz-ui --all-features agent::chat::steps</span><span class="penna-chat-detail" data-penna-text>Permission denied (os error 13)</span><span style="display: flex; flex: none"><span class="penna-badge" data-penna="badge" data-tone="danger"><span class="badge-text" data-penna-text>exit 1</span></span></span><span class="penna-chat-glyph penna-chat-chevron"><i data-icon="external-link"></i></span></div>
    <span class="docs-note">Narrow width: detail gives way before the label truncates</span>
  </div>
</div>
```

```rust
let mut mark = ActivityMark::plain(IconName::SquareTerminal);
mark.tint = Some(cx.theme().danger);
mark.tag = Some(("exit 1".into(), Tone::Danger));
activity_row("tool-7", mark, "cargo clippy".into(), ActivityEnd::None, cx)
```

## Messages

The user's message in a bubble with an action that shows on hover, a reply bubble, a notice, and a plan list.

```figure uses=chat,card,button,panel,check,tooltip
<div class="docs-show" data-gap="wide" style="width: 100%; flex-direction: column; align-items: stretch; gap: 16px">
  <div class="penna-chat-message">
    <div class="penna-chat-message-row">
      <span class="penna-chat-message-action" data-enabled><button class="penna-button" data-penna="icon-button.row" data-shape="icon" type="button" aria-label="Rewind to here" data-tooltip="Rewind to here" style="color: var(--penna-fg-muted)"><i data-icon="history"></i></button></span>
      <div class="penna-card penna-chat-bubble" data-penna="card.bubble" data-ground role="group"><span data-penna-text>The chat keeps spinning after a turn ends. Find out why.</span></div>
    </div>
  </div>
  <div style="display: flex; flex-direction: column; align-items: flex-end; gap: 8px">
    <div class="penna-panel penna-chat-reply" data-penna="inline-panel" data-ground role="group"><span class="penna-chat-note" data-penna-text>Side answer · not in the conversation</span><span data-penna-text>It reads the width once, before the pinned tabs.</span></div>
  </div>
  <div class="penna-chat-notice"><span class="penna-chat-glyph"><i data-icon="info"></i></span><span data-penna-text>The agent compacted the conversation.</span></div>
  <div class="penna-chat-plan">
    <div class="penna-chat-plan-row" data-state="done"><span class="penna-chat-glyph"><span class="penna-check" aria-checked="true"><i data-icon="check"></i></span></span><span data-penna-text>Find where the tab strip divides its width</span></div>
    <div class="penna-chat-plan-row" data-state="current"><span class="penna-chat-glyph"><span class="penna-chat-plan-current"></span></span><span data-penna-text>Give pinned tabs a 28px floor</span></div>
    <div class="penna-chat-plan-row" data-state="pending"><span class="penna-chat-glyph"><span class="penna-check" aria-checked="false"></span></span><span data-penna-text>Run the tab tests</span></div>
  </div>
</div>
```

```rust
UserMessage::new(("message", id))
    .body(body)
    .action(rewind, true)
reply_bubble(("reply", id), Some("Side answer".into()), body, cx)
notice_row(("notice", id), body, cx)
plan_list(&[(PlanState::Current, "Give pinned tabs a 28px floor".into())], cx)
```

## Composer

The field with a leading button, the context meter, and the action button, which sends, queues behind a running turn, or stops it. `ChatComposer` adds a prefix above, a footer below, and an `over` layer that floats over the chat from the composer's top edge without moving it, where zz puts its `@` menu. It is glass when glass is on; otherwise it rests on the activity cards' wash, opaque and with no edge.

```figure uses=chat,button,tooltip
<div class="docs-show" data-gap="wide" style="width: 100%; flex-direction: column; align-items: stretch; gap: 20px">
  <div class="docs-cell" style="width: 100%">
    <div class="penna-chat-composer">
      <div class="penna-chat-field input-chrome" data-penna="field.composer">
        <button class="penna-button" data-penna="button.send" data-shape="icon" type="button" aria-label="Mention a file" data-tooltip="Mention a file"><i data-icon="plus"></i></button>
        <span class="penna-chat-input" role="textbox" aria-label="Message" data-penna-text>Give pinned tabs a 28px floor</span>
        <span class="penna-chat-meter" role="progressbar" aria-label="34% of context used" data-tooltip="68000 of 200000 context tokens used (34%)" style="--penna-chat-used: 0.34"><span></span></span>
        <button class="penna-button" data-penna="button.send" data-shape="icon" data-emphasis="solid" data-solid="primary" type="button" aria-label="Send message" data-tooltip="Send message"><i data-icon="arrow-up"></i></button>
      </div>
    </div>
    <span class="docs-note">Typed, send</span>
  </div>
  <div class="docs-cell" style="width: 100%">
    <div class="penna-chat-composer">
      <div class="penna-chat-field input-chrome" data-penna="field.composer">
        <button class="penna-button" data-penna="button.send" data-shape="icon" type="button" aria-label="Mention a file" data-tooltip="Mention a file"><i data-icon="plus"></i></button>
        <span class="penna-chat-input" role="textbox" aria-label="Message" data-placeholder="Message the agent…"></span>
        <span class="penna-chat-meter" role="progressbar" aria-label="34% of context used" data-tooltip="68000 of 200000 context tokens used (34%)" style="--penna-chat-used: 0.34"><span></span></span>
        <button class="penna-button" data-penna="button.send" data-shape="icon" data-emphasis="solid" data-solid="primary" type="button" aria-label="Stop the current turn" data-tooltip="Stop the current turn"><i data-icon="stop"></i></button>
      </div>
    </div>
    <span class="docs-note">Running turn, stop</span>
  </div>
  <div class="docs-cell" style="width: 100%">
    <div class="penna-chat-frame">
      <div class="penna-chat-frame-top" style="padding: 0">
        <div class="penna-chat-queue">
          <button class="penna-button penna-chat-chrome" data-penna="button.quiet" data-size="small" type="button" style="color: var(--penna-fg-muted)"><i data-icon="undo-2"></i><span class="penna-label" data-penna-text>2 queued</span></button>
        </div>
        <div class="penna-chat-composer">
          <span class="input-underlay" data-penna="field.composer"></span>
          <div class="penna-chat-field input-chrome" data-penna="field.composer">
            <button class="penna-button" data-penna="button.send" data-shape="icon" type="button" aria-label="Mention a file" data-tooltip="Mention a file"><i data-icon="plus"></i></button>
            <span class="penna-chat-input" role="textbox" aria-label="Message" data-penna-text>Then run the tests</span>
            <span class="penna-chat-meter" role="progressbar" aria-label="34% of context used" data-tooltip="68000 of 200000 context tokens used (34%)" style="--penna-chat-used: 0.34"><span></span></span>
            <button class="penna-button" data-penna="button.send" data-shape="icon" type="button" aria-label="Queue this as the next turn" data-tooltip="Queue this as the next turn"><i data-icon="plus"></i></button>
          </div>
        </div>
      </div>
      <div class="penna-chat-frame-foot" style="margin: 8px 0 0">
        <div><button class="penna-button penna-chat-chrome" data-penna="button.quiet" data-size="small" type="button"><i data-icon="folder"></i><span class="penna-label" data-penna-text>~/src/app</span></button></div>
        <button class="penna-button penna-chat-chrome" data-penna="button.quiet" data-size="small" type="button"><span class="penna-label" data-penna-text>Model</span></button>
      </div>
    </div>
    <span class="docs-note">ChatComposer with queued prefix, queue action, and footer controls</span>
  </div>
</div>
```

```rust
let on = composer_ground(ground, cx);
ChatComposer {
    composer: Composer {
        input,
        leading: Some(
            composer_add_button("add", "Mention", on).into_any_element(),
        ),
        usage: Some(context_meter("usage", 0.34, "34% used", cx)),
        action: composer_action_button("send", ComposerAction::Send, true, on)
            .into_any_element(),
        attachments: None,
        command_hint: None,
        ask: None,
        secret: false,
        ground,
    },
    prefix: vec![
        queue_chip("queued", 2, true, on_return, cx).into_any_element(),
    ],
    over: None,
    footer: vec![
        chrome_button("folder").label("~/src/app").into_any_element(),
    ],
    controls: vec![],
}
```

## Suggestions

Slash commands and mentions offered while typing.

```figure uses=chat,panel,row
<div class="penna-panel penna-chat-list" data-penna="inline-panel" data-ground role="listbox" aria-label="Suggestions">
  <div class="penna-row penna-chat-suggestion" data-penna="menu-item" role="option" aria-selected="false"><div><strong data-penna-text>/compact</strong><small data-penna-text>Summarize the conversation to free context</small></div></div>
  <div class="penna-row penna-chat-suggestion" data-penna="menu-item" role="option" data-state="selected" aria-selected="true"><div><strong data-penna-text>/review</strong><small data-penna-text>Review the current changes</small></div><em data-penna-text>skill</em></div>
  <div class="penna-row penna-chat-suggestion" data-penna="menu-item" role="option" aria-selected="false"><div><strong data-penna-text>/model</strong></div></div>
</div>
```

```rust
suggestion_list("suggestions", rows, cx)
suggestion_row(
    "review",
    "/review",
    Some("Review the current changes".into()),
    Some("skill".into()),
    true,
    cx,
)
```

## Ask

A question, a tool approval, or a plan approval grows out of the top of the composer, with numbered options and an input row that says what typing does. A question is its own header; an approval names itself and shows what it wants to run. When several answers can be picked, a picked answer lights its key: no checkbox, and the same key takes it back. Click options below to try picking and toggling.

```figure uses=chat,button,row,kbd,well,tooltip
<div class="docs-show" data-gap="wide" style="width: 100%; flex-direction: column; align-items: stretch; gap: 28px">
  <div class="docs-cell" style="width: 100%">
    <div class="penna-chat-frame">
      <div class="penna-chat-frame-top" style="padding: 0">
        <div class="penna-chat-composer">
          <span class="input-underlay" data-penna="field.composer"></span>
          <div class="penna-chat-field input-chrome" data-penna="field.composer" data-asking>
            <div class="penna-chat-ask" data-interactive>
              <div class="penna-chat-ask-header" data-question><span class="penna-chat-ask-glyph"><i data-icon="chat"></i></span><span class="penna-chat-ask-question" data-penna-text>Which accent should the composer use?</span></div>
              <div class="penna-chat-ask-options" role="listbox" aria-label="Accent choices">
                <div class="penna-row penna-chat-ask-option" data-penna="menu-item" role="option" data-under data-state="selected" aria-selected="true"><div class="penna-chat-ask-text"><span class="penna-chat-ask-label" data-penna-text>Teal</span><span class="penna-chat-ask-detail" data-penna-text>Calm, reads well on both modes</span></div><kbd class="penna-kbd" data-penna="keycap" data-size="small"><span data-penna-text>1</span></kbd></div>
                <div class="penna-row penna-chat-ask-option" data-penna="menu-item" role="option" data-under aria-selected="false"><div class="penna-chat-ask-text"><span class="penna-chat-ask-label" data-penna-text>Violet</span><span class="penna-chat-ask-detail" data-penna-text>Matches the current brand</span></div><kbd class="penna-kbd" data-penna="keycap" data-size="small"><span data-penna-text>2</span></kbd></div>
                <div class="penna-row penna-chat-ask-option" data-penna="menu-item" role="option" data-under aria-selected="false"><div class="penna-chat-ask-text"><span class="penna-chat-ask-label" data-penna-text>Amber</span><span class="penna-chat-ask-detail" data-penna-text>Warm, draws the eye</span></div><kbd class="penna-kbd" data-penna="keycap" data-size="small"><span data-penna-text>3</span></kbd></div>
                <div class="penna-row penna-chat-ask-option" data-penna="menu-item" role="option" data-under aria-selected="false"><div class="penna-chat-ask-text"><span class="penna-chat-ask-label" data-penna-text>Graphite</span><span class="penna-chat-ask-detail" data-penna-text>No accent at all</span></div><kbd class="penna-kbd" data-penna="keycap" data-size="small"><span data-penna-text>4</span></kbd></div>
              </div>
            </div>
            <span class="penna-chat-ask-rule" data-penna="rule.list"></span>
            <div class="penna-chat-input-row">
              <span class="penna-chat-ask-leading"><kbd class="penna-kbd" data-penna="keycap" data-size="small"><span data-penna-text>5</span></kbd></span>
              <span class="penna-chat-input" role="textbox" aria-label="Custom answer" data-placeholder="Something else"></span>
              <button class="penna-button penna-chat-ask-skip" data-penna="button.quiet" type="button"><span class="penna-label" data-penna-text>Skip</span></button>
              <button class="penna-button penna-chat-ask-send" data-penna="button" data-emphasis="solid" data-solid="primary" type="button"><span class="penna-label" data-penna-text>Send</span><i data-icon="key-return"></i></button>
              <button class="penna-button" data-penna="button.send" data-shape="icon" data-emphasis="solid" data-solid="primary" type="button" aria-label="Stop the current turn" data-tooltip="Stop the current turn"><i data-icon="stop"></i></button>
            </div>
          </div>
        </div>
      </div>
      <div class="penna-chat-frame-foot" style="margin: 8px 0 0"><div class="penna-chat-ask-hints"><span data-penna-text>1 to 5 pick</span><span data-penna-text>↑↓ move</span><span data-penna-text>↵ send</span><span data-penna-text>esc skip</span></div></div>
    </div>
    <span class="docs-note">Single-choice question with custom input row</span>
  </div>
  <div class="docs-cell" style="width: 100%">
    <div class="penna-chat-frame">
      <div class="penna-chat-frame-top" style="padding: 0">
        <div class="penna-chat-composer">
          <span class="input-underlay" data-penna="field.composer"></span>
          <div class="penna-chat-field input-chrome" data-penna="field.composer" data-asking>
            <div class="penna-chat-ask" data-interactive>
              <div class="penna-chat-ask-header" data-question><span class="penna-chat-ask-glyph"><i data-icon="chat"></i></span><span class="penna-chat-ask-question" data-penna-text>What should the morph cover?</span><button class="penna-button" data-penna="icon-button.row" data-shape="icon" type="button" aria-label="Previous question" data-tooltip="Previous question"><i data-icon="chevron-left"></i></button><span class="penna-chat-ask-count" data-penna-text>2 of 3</span><button class="penna-button" data-penna="icon-button.row" data-shape="icon" type="button" aria-label="Next question" data-tooltip="Next question"><i data-icon="chevron-right"></i></button></div>
              <div class="penna-chat-ask-options" role="group" aria-label="Morph targets">
                <div class="penna-row penna-chat-ask-option" data-penna="menu-item" role="checkbox" aria-checked="true" data-under><div class="penna-chat-ask-text"><span class="penna-chat-ask-label" data-penna-text>Questions</span><span class="penna-chat-ask-detail" data-penna-text>Single and multiple choice</span></div><kbd class="penna-kbd" data-penna="keycap" data-size="small" data-tone="accent" data-emphasis="solid" data-solid="accent"><span data-penna-text>1</span></kbd></div>
                <div class="penna-row penna-chat-ask-option" data-penna="menu-item" role="checkbox" aria-checked="true" data-under data-state="selected"><div class="penna-chat-ask-text"><span class="penna-chat-ask-label" data-penna-text>Tool approvals</span><span class="penna-chat-ask-detail" data-penna-text>Commands, edits and fetches</span></div><kbd class="penna-kbd" data-penna="keycap" data-size="small" data-tone="accent" data-emphasis="solid" data-solid="accent"><span data-penna-text>2</span></kbd></div>
                <div class="penna-row penna-chat-ask-option" data-penna="menu-item" role="checkbox" aria-checked="false" data-under><div class="penna-chat-ask-text"><span class="penna-chat-ask-label" data-penna-text>Plan approval</span><span class="penna-chat-ask-detail" data-penna-text>The plan the agent wants to build</span></div><kbd class="penna-kbd" data-penna="keycap" data-size="small"><span data-penna-text>3</span></kbd></div>
              </div>
            </div>
            <span class="penna-chat-ask-rule" data-penna="rule.list"></span>
            <div class="penna-chat-input-row">
              <span class="penna-chat-ask-leading"><kbd class="penna-kbd" data-penna="keycap" data-size="small" data-tone="accent" data-emphasis="solid" data-solid="accent"><span data-penna-text>4</span></kbd></span>
              <span class="penna-chat-input" role="textbox" aria-label="Custom answer" data-penna-text>Code blocks</span>
              <button class="penna-button penna-chat-ask-skip" data-penna="button.quiet" type="button"><span class="penna-label" data-penna-text>Skip</span></button>
              <button class="penna-button penna-chat-ask-send" data-penna="button" data-emphasis="solid" data-solid="primary" type="button"><span class="penna-label" data-penna-text>Next</span><i data-icon="key-return"></i></button>
              <button class="penna-button" data-penna="button.send" data-shape="icon" data-emphasis="solid" data-solid="primary" type="button" aria-label="Stop the current turn" data-tooltip="Stop the current turn"><i data-icon="stop"></i></button>
            </div>
          </div>
        </div>
      </div>
      <div class="penna-chat-frame-foot" style="margin: 8px 0 0"><div class="penna-chat-ask-hints"><b data-picked-count data-penna-text>2 picked</b><span data-penna-text>1 to 4 toggle</span><span data-penna-text>←→ page</span><span data-penna-text>↵ next</span><span data-penna-text>esc skip</span></div></div>
    </div>
    <span class="docs-note">Multi-select question with pager: picked options light their keycap</span>
  </div>
  <div class="docs-cell" style="width: 100%">
    <div class="penna-chat-frame">
      <div class="penna-chat-frame-top" style="padding: 0">
        <div class="penna-chat-composer">
          <span class="input-underlay" data-penna="field.composer"></span>
          <div class="penna-chat-field input-chrome" data-penna="field.composer" data-asking>
            <div class="penna-chat-ask" data-interactive>
              <div class="penna-chat-ask-header"><i data-icon="square-terminal"></i><span class="penna-chat-ask-title" data-penna-text>Run this command?</span><span class="penna-chat-ask-count" data-penna-text>1 of 2</span></div>
              <div class="penna-chat-ask-body">
                <div class="penna-well penna-chat-ask-inset" data-penna="well"><span data-penna-text>cargo test -p zz-mux view_pane</span></div>
                <span class="penna-chat-ask-note" data-penna-text>in ~/src/app</span>
              </div>
              <div class="penna-chat-ask-options" role="listbox" aria-label="Approval actions">
                <div class="penna-row penna-chat-ask-option" data-penna="menu-item" role="option" aria-selected="false"><div class="penna-chat-ask-text"><span class="penna-chat-ask-label" data-penna-text>Allow once</span></div><kbd class="penna-kbd" data-penna="keycap" data-size="small"><span data-penna-text>1</span></kbd></div>
                <div class="penna-row penna-chat-ask-option" data-penna="menu-item" role="option" aria-selected="false"><div class="penna-chat-ask-text"><span class="penna-chat-ask-label" data-penna-text>Always allow cargo test</span><span class="penna-chat-ask-detail" data-penna-text>this session</span></div><kbd class="penna-kbd" data-penna="keycap" data-size="small"><span data-penna-text>2</span></kbd></div>
                <span class="penna-chat-ask-divider" data-penna="rule.list"></span>
                <div class="penna-row penna-chat-ask-option" data-penna="menu-item" role="option" data-state="selected" aria-selected="true"><div class="penna-chat-ask-text"><span class="penna-chat-ask-label" data-penna-text>Deny</span></div><kbd class="penna-kbd" data-penna="keycap" data-size="small"><span data-penna-text>3</span></kbd></div>
              </div>
            </div>
            <span class="penna-chat-ask-rule" data-penna="rule.list"></span>
            <div class="penna-chat-input-row">
              <span class="penna-chat-ask-leading"><kbd class="penna-kbd" data-penna="keycap" data-size="small"><span data-penna-text>4</span></kbd></span>
              <span class="penna-chat-input" role="textbox" aria-label="Alternative instruction" data-placeholder="Deny and say what to do instead"></span>
              <button class="penna-button penna-chat-ask-send" data-penna="button" data-emphasis="solid" data-solid="primary" type="button"><span class="penna-label" data-penna-text>Deny</span><i data-icon="key-return"></i></button>
              <button class="penna-button" data-penna="button.send" data-shape="icon" data-emphasis="solid" data-solid="primary" type="button" aria-label="Stop the current turn" data-tooltip="Stop the current turn"><i data-icon="stop"></i></button>
            </div>
          </div>
        </div>
      </div>
      <div class="penna-chat-frame-foot" style="margin: 8px 0 0"><div class="penna-chat-ask-hints"><span data-penna-text>1 to 3 pick</span><span data-penna-text>↵ deny</span><span data-penna-text>esc deny</span></div></div>
    </div>
    <span class="docs-note">Tool approval with command well and divider before Deny</span>
  </div>
  <div class="docs-cell" style="width: 100%">
    <div class="penna-chat-frame">
      <div class="penna-chat-frame-top" style="padding: 0">
        <div class="penna-chat-composer">
          <span class="input-underlay" data-penna="field.composer"></span>
          <div class="penna-chat-field input-chrome" data-penna="field.composer" data-asking>
            <div class="penna-chat-ask" data-interactive>
              <div class="penna-chat-ask-header"><i data-icon="list"></i><span class="penna-chat-ask-title" data-penna-text>Plan</span><button class="penna-button" data-penna="icon-button.row" data-shape="icon" type="button" aria-label="Open the plan in a view pane" data-tooltip="Open the plan in a view pane"><i data-icon="expand"></i></button></div>
              <div class="penna-well penna-chat-ask-sheet" data-penna="well"><div style="display: flex; flex-direction: column; gap: 6px"><span style="font-weight: 500" data-penna-text>Composer morph</span><span data-penna-text>1. Give the composer an ask body above its input row.</span><span data-penna-text>2. Show the answer's own field in the input row.</span><span data-penna-text>3. Move approvals and plan approval into the same body.</span></div></div>
              <div class="penna-chat-ask-options" role="listbox" aria-label="Plan actions">
                <div class="penna-row penna-chat-ask-option" data-penna="menu-item" role="option" aria-selected="false"><div class="penna-chat-ask-text"><span class="penna-chat-ask-label" data-penna-text>Build it</span><span class="penna-chat-ask-detail" data-penna-text>accept edits as they come</span></div><kbd class="penna-kbd" data-penna="keycap" data-size="small"><span data-penna-text>1</span></kbd></div>
                <div class="penna-row penna-chat-ask-option" data-penna="menu-item" role="option" aria-selected="false"><div class="penna-chat-ask-text"><span class="penna-chat-ask-label" data-penna-text>Build it</span><span class="penna-chat-ask-detail" data-penna-text>ask before each edit</span></div><kbd class="penna-kbd" data-penna="keycap" data-size="small"><span data-penna-text>2</span></kbd></div>
                <span class="penna-chat-ask-divider" data-penna="rule.list"></span>
                <div class="penna-row penna-chat-ask-option" data-penna="menu-item" role="option" data-state="selected" aria-selected="true"><div class="penna-chat-ask-text"><span class="penna-chat-ask-label" data-penna-text>Keep planning</span></div><kbd class="penna-kbd" data-penna="keycap" data-size="small"><span data-penna-text>3</span></kbd></div>
              </div>
            </div>
            <span class="penna-chat-ask-rule" data-penna="rule.list"></span>
            <div class="penna-chat-input-row">
              <span class="penna-chat-ask-leading"><kbd class="penna-kbd" data-penna="keycap" data-size="small"><span data-penna-text>4</span></kbd></span>
              <span class="penna-chat-input" role="textbox" aria-label="Plan revision" data-placeholder="Keep planning and say what to change"></span>
              <button class="penna-button penna-chat-ask-send" data-penna="button" data-emphasis="solid" data-solid="primary" type="button"><span class="penna-label" data-penna-text>Keep planning</span><i data-icon="key-return"></i></button>
              <button class="penna-button" data-penna="button.send" data-shape="icon" data-emphasis="solid" data-solid="primary" type="button" aria-label="Stop the current turn" data-tooltip="Stop the current turn"><i data-icon="stop"></i></button>
            </div>
          </div>
        </div>
      </div>
      <div class="penna-chat-frame-foot" style="margin: 8px 0 0"><div class="penna-chat-ask-hints"><span data-penna-text>1 to 3 pick</span><span data-penna-text>↵ keep planning</span><span data-penna-text>esc keep planning</span></div></div>
    </div>
    <span class="docs-note">Plan approval with scrollable plan sheet and expand control</span>
  </div>
</div>
```

```rust
Composer {
    input,
    leading: Some(
        ask_leading(ask_picked_key(Some("4".into()), on)).into_any_element(),
    ),
    usage: Some(ask_send("send", "Next", on).into_any_element()),
    action: composer_action_button("stop", ComposerAction::Stop, true, on)
        .into_any_element(),
    attachments: None,
    command_hint: None,
    ask: Some(
        v_flex()
            .child(
                ask_question("What should the morph cover?", on, cx)
                    .child(ask_count("2 of 3", on)),
            )
            .child(
                ask_options()
                    .child(
                        AskOption::new(("opt", 0), "Questions", on)
                            .key("1")
                            .picked(true),
                    )
                    .child(
                        AskOption::new(("opt", 1), "Approvals", on)
                            .key("2")
                            .picked(false),
                    ),
            )
            .into_any_element(),
    ),
    secret: false,
    ground,
}
```

## Plan tray

The plan's chip above the composer; click the chip below to toggle its steps.

```figure uses=chat,button,panel,check
<div class="penna-chat-tray" data-interactive>
  <div class="penna-panel penna-chat-tray-panel" data-penna="inline-panel" data-ground role="group">
    <div class="penna-chat-tray-plan">
      <div class="penna-chat-plan">
        <div class="penna-chat-plan-row" data-state="done"><span class="penna-chat-glyph"><span class="penna-check" aria-checked="true"><i data-icon="check"></i></span></span><span data-penna-text>Find where the tab strip divides its width</span></div>
        <div class="penna-chat-plan-row" data-state="current"><span class="penna-chat-glyph"><span class="penna-chat-plan-current"></span></span><span data-penna-text>Give pinned tabs a 28px floor</span></div>
        <div class="penna-chat-plan-row" data-state="pending"><span class="penna-chat-glyph"><span class="penna-check" aria-checked="false"></span></span><span data-penna-text>Run the tab tests</span></div>
      </div>
    </div>
  </div>
  <div class="penna-chat-tray-bar">
    <div style="min-width: 0"><button class="penna-button penna-chat-plan-chip" data-penna="tab.tray" type="button" aria-expanded="true" data-state="current"><span class="penna-chat-glyph"><i data-icon="circle-check"></i></span><span data-penna-text>Plan 1/3</span><span class="penna-chat-current" data-penna-text>Give pinned tabs a 28px floor</span><span class="penna-chat-glyph" style="color: var(--penna-muted)"><i data-icon="chevron-down"></i></span></button></div>
  </div>
</div>
```

```rust
plan_tray("tray", plan, open, on_toggle, cx)
```

## Empty states

```figure uses=chat,card
<div class="docs-pr-split">
  <div class="docs-pr-col">
    <div class="docs-pr-col-head"><span class="docs-pr-col-title">Welcome state</span></div>
    <div data-penna="card" data-ground style="height: 220px; display: flex">
      <div class="penna-chat-welcome"><i data-icon="bot"></i><strong data-penna-text>Ask the agent</strong><span data-penna-text>It reads, edits and runs commands here.</span></div>
    </div>
  </div>
  <div class="docs-pr-col">
    <div class="docs-pr-col-head"><span class="docs-pr-col-title">Empty transcript</span></div>
    <div data-penna="card" data-ground style="height: 220px; display: flex; align-items: center; justify-content: center">
      <div class="penna-chat-empty"><span data-penna-text>No messages yet</span></div>
    </div>
  </div>
</div>
```

```rust
welcome_state(
    Icon::new(IconName::Bot).size(px(40.0)),
    "Ask the agent",
    detail,
    cx,
)
empty_state("No messages yet", false, view, cx)
```

## On a phone

The same chat drawn for a thumb. The composer is one row: a 44 options button (`sliders`), the field at `touch-body` growing to five lines, and a 44 action that sends, stops, or queues. Send is one look whatever the permission mode. The options button opens a [sheet](sheet.html) led by three 84 attach tiles (`Camera`, `Photos`, `File`), then `Mention`, then the chat's settings (`Model`, `Effort`, `Permissions`, `Account`) and a muted context line. Attached images and files sit over the field as 88 tiles, each with a 24 remove badge and a 44 target. Tool steps are 44 rows, and a held message opens a sheet with Copy and Fork from here.

A permission request or question docks in the composer's place as an ask card with no scrim: what is asked in one or two lines, then the answers as touch rows, each answering on tap with no key caps. An error card sits 8 above the composer and clears when the field changes or the error resolves.

```figure uses=chat,touch,button,label
<div class="docs-pr-split">
  <div class="docs-pr-col">
    <div class="docs-pr-col-head"><span class="docs-pr-col-title">Composer &amp; queued row</span></div>
    <div class="penna-stack" data-gap="related">
      <div class="penna-touch-row penna-chat-queued" data-penna="phone.row" role="button" tabindex="0" style="width: 100%"><i data-icon="undo-2"></i><span class="penna-touch-row-lines"><span>2 queued · Then run the tests</span></span></div>
      <div class="penna-chat-composer">
        <span class="input-underlay" data-penna="field.composer"></span>
        <div class="penna-chat-field input-chrome" data-penna="field.composer" data-size="touch">
          <button class="penna-button" data-penna="button.send" data-shape="icon" type="button" aria-label="Chat options"><i data-icon="sliders"></i></button>
          <span class="penna-chat-input" role="textbox" aria-label="Message">Run the menu story too</span>
          <button class="penna-button" data-penna="button.send" data-shape="icon" data-emphasis="solid" data-solid="primary" type="button" aria-label="Send"><i data-icon="arrow-up"></i></button>
        </div>
      </div>
    </div>
  </div>
  <div class="docs-pr-col">
    <div class="docs-pr-col-head"><span class="docs-pr-col-title">Attachments over field</span></div>
    <div class="penna-chat-composer">
      <span class="input-underlay" data-penna="field.composer"></span>
      <div class="penna-chat-field input-chrome" data-penna="field.composer" data-size="touch">
        <div class="penna-chat-attach">
          <span class="penna-chat-thumb" style="background-image: linear-gradient(160deg, color-mix(in srgb, var(--penna-accent) 40%, transparent), transparent 60%), repeating-linear-gradient(0deg, color-mix(in srgb, var(--penna-fg) 18%, transparent) 0 6px, transparent 6px 14px)"><button class="penna-chat-thumb-remove" type="button" aria-label="Remove the image"><span class="penna-chat-thumb-disc" data-penna="floating"><i data-icon="xmark"></i></span></button></span>
          <span class="penna-chat-file"><i data-icon="file"></i><span class="penna-chat-file-copy"><span class="penna-chat-file-name">theme_css.rs</span><span class="penna-chat-file-meta">28 KB</span></span><button class="penna-chat-thumb-remove" type="button" aria-label="Remove the file"><span class="penna-chat-thumb-disc" data-penna="floating"><i data-icon="xmark"></i></span></button></span>
        </div>
        <button class="penna-button" data-penna="button.send" data-shape="icon" type="button" aria-label="Chat options"><i data-icon="sliders"></i></button>
        <span class="penna-chat-input" role="textbox" aria-label="Message">This is the menu now</span>
        <button class="penna-button" data-penna="button.send" data-shape="icon" data-emphasis="solid" data-solid="primary" type="button" aria-label="Send"><i data-icon="arrow-up"></i></button>
      </div>
    </div>
  </div>
  <div class="docs-pr-col">
    <div class="docs-pr-col-head"><span class="docs-pr-col-title">Ask card</span></div>
    <div class="penna-chat-ask-card" data-penna="field.composer">
      <div class="penna-chat-ask-what" role="button" tabindex="0"><div class="penna-chat-ask-what-head"><span class="penna-chat-ask-what-title">Run this command?</span></div><span class="penna-chat-ask-what-detail">cargo test -p penna pill_menu --all-features</span></div>
      <div class="penna-touch-row" data-penna="phone.row" role="button" tabindex="0"><span class="penna-touch-row-lines"><span>Allow once</span></span></div>
      <div class="penna-touch-row" data-penna="phone.row" role="button" tabindex="0"><span class="penna-touch-row-lines"><span>Always allow cargo test</span></span></div>
      <div class="penna-touch-row" data-penna="phone.row" role="button" tabindex="0"><span class="penna-touch-row-lines"><span>Deny</span></span></div>
    </div>
  </div>
  <div class="docs-pr-col">
    <div class="docs-pr-col-head"><span class="docs-pr-col-title">Error card</span></div>
    <div class="penna-chat-error" data-penna="field.composer">
      <span class="penna-chat-error-glyph"><i data-icon="triangle-alert"></i></span>
      <span class="penna-chat-error-copy"><strong>Session disconnected</strong><span>Reconnect to continue this chat.</span></span>
    </div>
  </div>
</div>
```

## API (`penna::blocks::chat`)

| Rust | HTML | Does |
| --- | --- | --- |
| `activity_row(id, mark, label, end, cx)` | `.penna-chat-activity`, `.penna-chat-resolve` | Tool, thought, live, or summary row (`ActivityEnd::{None, Disclosure, Open}`) |
| `ActivityMark::{bare, plain}`, `ActivityCount` | `.penna-chat-glyph`, `.penna-chat-detail`, `.penna-chat-counts` | Leading glyph, detail, tag, and per-kind counts (`touch: bool` field) |
| `UserMessage`, `message_action`, `shown_on_hover` | `.penna-chat-message`, `.penna-chat-bubble` | User message bubble with optional attachments and hover action |
| `reply_bubble`, `notice_row`, `plan_list`, `PlanState` | `.penna-chat-reply`, `.penna-chat-notice`, `.penna-chat-plan` | Replies, notices, and step plans |
| `Composer`, `ChatComposer`, `composer_ground` | `.penna-chat-composer`, `.penna-chat-field`, `.penna-chat-frame` | Message input, prefix, footer, and floating `over` layer |
| `composer_add_button`, `composer_action_button`, `ComposerAction` | `[data-penna="button.send"]` | Leading add button and Send/Queue/Stop action |
| `context_meter`, `queue_chip`, `chrome_button` | `.penna-chat-meter`, `.penna-chat-queue`, `.penna-chat-chrome` | Context usage ring, queued return chip, and footer chrome buttons |
| `suggestion_list`, `suggestion_row` | `.penna-chat-list`, `.penna-chat-suggestion` | Slash and `@` mention suggestions |
| `ask_question`, `ask_header`, `ask_body`, `ask_inset`, `ask_sheet`, `ask_note`, `ask_count` | `.penna-chat-ask`, `.penna-chat-ask-header`, `.penna-chat-ask-inset` | Ask headers, command wells, plan sheets, and pagers |
| `ask_options`, `AskOption`, `ask_divider`, `ask_leading`, `ask_send`, `ask_skip`, `ask_hints` | `.penna-chat-ask-option`, `.penna-chat-ask-hints` | Numbered choices, picked accent keycaps, send/skip, and hints |
| `plan_tray`, `TrayPlan` | `.penna-chat-tray`, `.penna-chat-plan-chip` | Collapsible plan chip above the composer |
| `welcome_state`, `empty_state` | `.penna-chat-welcome`, `.penna-chat-empty` | Empty desktop chat states |
| `PhoneComposer`, `phone_options_button`, `phone_close_button`, `phone_action_button` | `.penna-chat-field[data-size="touch"]`, `.penna-chat-above` | Phone composer row and `Something else` header |
| `phone_attach_tile`, `phone_thumbnail`, `phone_file_tile` | `.penna-chat-tiles`, `.penna-chat-thumb`, `.penna-chat-file` | Attach sheet tiles and 88 px composer attachments with remove badge |
| `AskCard`, `phone_error_card`, `phone_queued_row`, `phone_jump_button` | `.penna-chat-ask-card`, `.penna-chat-error`, `.penna-chat-queued` | Phone ask card, error card, queued row, and jump-to-latest button |
| `phone_plan_tray`, `touch_transcript_text`, `transcript_text` | `.penna-chat-activity[data-size="touch"]` | Touch transcript text (`phone_plan_tray` is native-only) |
