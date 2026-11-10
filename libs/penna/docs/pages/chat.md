# Chat

The pieces of an agent chat. Each takes plain data and callbacks, so an app keeps its own transcript model and feeds the block what to draw.

## Activity rows

One row per tool call, thought or summary: an icon or a running spinner, the label, a faint detail, a tag, per-kind counts and a chevron that opens it. While the agent works, the live line sweeps a soft crest across its muted label and its glyph breathes on the same 2.6s cycle.

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

The field with a leading button, the context meter and the action button, which sends, queues behind a running turn or stops it. ChatComposer adds a prefix above and a footer below. It is glass when glass is on; otherwise it rests on the activity cards' wash, opaque and with no edge.

```rust example story=chat/composer
ChatComposer {
    composer: Composer {
        input,
        leading: Some(composer_add_button("add", "Mention a file", cx).into_any_element()),
        usage: Some(context_meter("usage", 0.34, "34% of the context used", cx)),
        action: composer_action_button("send", ComposerAction::Send, true, cx).into_any_element(),
        attachments: None,
        command_hint: None,
        ask: None,
        secret: false,
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

## Ask

A question, a tool approval or a plan approval grows out of the top of the composer, with numbered options and an input row that says what typing does. A question is its own header; an approval names itself and shows what it wants to run. The chat keeps one row per ask.

```rust example story=chat/ask
Composer {
    input,
    leading: Some(ask_leading(ask_picked_key(Some("4".into()), on)).into_any_element()),
    usage: Some(ask_send("send", "Next", on).into_any_element()),
    action: composer_action_button("stop", ComposerAction::Stop, true, on).into_any_element(),
    attachments: None,
    command_hint: None,
    ask: Some(
        v_flex()
            .child(ask_question("What should the morph cover?", on, cx).child(ask_count("2 of 3", on)))
            .child(
                ask_options()
                    .child(AskOption::new(("option", 0), "Questions", on).key("1").picked(true))
                    .child(AskOption::new(("option", 1), "Tool approvals", on).key("2").picked(false)),
            )
            .into_any_element(),
    ),
    secret: false,
    ground,
}
```

When several answers can be picked, a picked answer lights its key: no checkbox, and the same key takes it back. The hints lead with how many are picked.

## Plan tray

The plan's chip above the composer, opening its steps.

```rust example story=chat/tasks
plan_tray("tray", plan, open, on_toggle, cx)
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
| `ask_question`, `ask_header`, `ask_body`, `ask_inset`, `ask_sheet`, `ask_options`, `AskOption`, `ask_key`, `ask_picked_key`, `ask_send`, `ask_skip`, `ask_hints`, `AskHint` | `.penna-chat-ask`, `.penna-chat-ask-option` | Questions and approvals inside the composer |
| `plan_tray` | `.penna-chat-tray` | The plan over the composer |
