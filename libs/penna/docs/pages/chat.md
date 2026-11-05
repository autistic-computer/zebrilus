# Chat

The pieces of an agent chat. Each takes plain data and callbacks, so an app keeps its own transcript model and feeds the block what to draw.

## Activity rows

One row per tool call, thought or summary: an icon or a running spinner, the label, a faint detail, a tag, per-kind counts and a chevron that opens it.

```rust example story=chat/activity
let mut mark = ActivityMark::plain(IconName::SquareTerminal);
mark.tint = Some(cx.theme().danger);
mark.tag = Some(("exit 1".into(), Tone::Danger));
activity_row("tool-7", mark, "cargo clippy".into(), ActivityEnd::None, cx)
```

## Messages

The user's message in a bubble with an action that shows on hover, a reply bubble, a notice and a plan.

```rust example story=chat/messages
UserMessage::new(("message", id))
    .body(body)
    .action(rewind, true)
reply_bubble(("reply", id), Some("Side answer".into()), body, cx)
notice_row(("notice", id), body, cx)
plan_list(&[(PlanState::Current, "Give pinned tabs a 28px floor".into())], cx)
```

## Composer

The field with a leading button, the context meter and the action button, which sends, queues behind a running turn or stops it. ChatComposer adds a prefix above and a footer below.

```rust example story=chat/composer
ChatComposer {
    composer: Composer {
        input,
        leading: Some(composer_add_button("add", "Mention a file", cx).into_any_element()),
        usage: Some(context_meter("usage", 0.34, "34% of the context used", cx)),
        action: composer_action_button("send", ComposerAction::Send, true, cx).into_any_element(),
        attachments: None,
        command_hint: None,
        ground: None,
    },
    prefix: vec![queue_chip("queued", 2, true, on_return, cx).into_any_element()],
    footer: vec![chrome_button("folder").label("~/src/app").into_any_element()],
    controls: vec![],
}
```

## Suggestions

Slash commands and mentions offered while typing.

```rust example story=chat/suggestions
suggestion_list("suggestions", rows, cx)
suggestion_row("review", "/review", Some("Review the current changes".into()), Some("skill".into()), true, cx)
```

## Action card

A question the agent asks before it acts, with numbered options.

```rust example story=chat/card
permission_card("Run cargo test?", Some("1 of 2".into()), options, close, cx)
permission_option(("option", 0), 0, "Allow once", false, true, true, cx)
```

## Task tray

The plan and the background tasks above the composer, each opening its panel.

```rust example story=chat/tasks
task_tray("tray", Some(plan), &tasks, "2 running".into(), open, true, phase, on_action, cx)
```

## Empty states

```rust example story=chat/empty
welcome_state(Icon::new(IconName::Bot).size(px(40.0)), "Ask the agent", detail, cx)
empty_state("No messages yet", false, view, cx)
```

## API

| Rust | HTML | What it does |
| --- | --- | --- |
| `activity_row(id, mark, label, end, cx)` | `.penna-chat-activity` | A tool, thought or summary row |
| `ActivityMark` | `.penna-chat-glyph`, `.penna-chat-detail`, `.penna-chat-counts` | What the row shows besides its label |
| `UserMessage` | `.penna-chat-message`, `.penna-chat-bubble` | The user's message, with an action on hover |
| `reply_bubble`, `notice_row`, `plan_list` | `.penna-chat-reply`, `.penna-chat-notice`, `.penna-chat-plan` | Replies, notices and plans |
| `Composer`, `ChatComposer` | `.penna-chat-composer`, `.penna-chat-field` | The message field and its frame |
| `composer_action_button`, `context_meter`, `queue_chip` | `button.send`, `.penna-chat-meter` | The composer's controls |
| `suggestion_list`, `suggestion_row` | `.penna-chat-list`, `.penna-chat-suggestion` | Slash and mention suggestions |
| `permission_card`, `permission_option`, `CardTile` | `.penna-chat-card`, `.penna-chat-tile` | Questions with options |
| `task_tray` | `.penna-chat-tray` | Plan and background tasks |
