# Chat

The pieces of an agent chat. Each takes plain data and callbacks, so an app keeps its own transcript model and feeds the block what to draw.

## Activity rows

One row per tool call, thought or summary: an icon or a running spinner, the label, a smaller detail in the same muted ink on its baseline, a tag, per-kind counts and a chevron that opens it. While the agent works, the live line sweeps a soft crest across its muted label and its glyph breathes on the same 2.6s cycle.

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

The field with a leading button, the context meter and the action button, which sends, queues behind a running turn or stops it. ChatComposer adds a prefix above, a footer below and an `over` layer that floats over the chat from the composer's top edge without moving it, where zz puts its @ menu. It is glass when glass is on; otherwise it rests on the activity cards' wash, opaque and with no edge.

```rust example story=chat/composer
ChatComposer {
    composer: Composer {
        input,
        leading: Some(composer_add_button("add", "Mention a chat, host, session or file", cx).into_any_element()),
        usage: Some(context_meter("usage", 0.34, "34% of the context used", cx)),
        action: composer_action_button("send", ComposerAction::Send, true, cx).into_any_element(),
        attachments: None,
        command_hint: None,
        ask: None,
        secret: false,
        ground: None,
    },
    prefix: vec![queue_chip("queued", 2, true, on_return, cx).into_any_element()],
    over: None,
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

## On a phone

The same chat drawn for a thumb. The composer is one row: a 44 plus, the field at `touch-body` growing to five lines, and a 44 action that sends, stops or queues. Send is one look whatever the permission mode. The plus opens a sheet with Mention, which lists what the desktop's @ menu does and writes the picked one into the field by name, then the chat's settings (Model, Effort, Permissions, Account) and one muted line for the context and the account's limits. A pasted image sits over the field as an 88 thumbnail, its remove control a 24 badge on its corner with a 44 target. Tool steps are 44 rows, and a held message opens a sheet with Copy and Fork from here.

A permission request or a question takes the composer's place as a card, with no scrim: what is asked in one or two lines, then the answers as touch rows, each answering on tap. No key caps and no answer button.

```figure uses=chat,touch,button,label
<div class="docs-show" data-gap="wide" style="align-items: flex-end">
  <div class="docs-cell" style="width: 370px"><div class="penna-chat-composer"><div class="penna-chat-field input-chrome" data-penna="field.composer" data-size="touch"><button class="penna-button" data-penna="button.send" data-shape="icon" aria-label="Chat options"><i data-icon="plus"></i></button><span class="penna-chat-input" role="textbox">Run the menu story too</span><button class="penna-button" data-penna="button.send" data-shape="icon" data-emphasis="solid" data-solid="primary" aria-label="Send"><i data-icon="arrow-up"></i></button></div></div><span class="docs-note">Composer row</span></div>
  <div class="docs-cell" style="width: 370px"><div class="penna-chat-composer"><div class="penna-chat-field input-chrome" data-penna="field.composer" data-size="touch"><div class="penna-chat-attach"><span class="penna-chat-thumb" style="background-image: linear-gradient(160deg, color-mix(in srgb, var(--penna-accent) 40%, transparent), transparent 60%), repeating-linear-gradient(0deg, color-mix(in srgb, var(--penna-fg) 18%, transparent) 0 6px, transparent 6px 14px)"><button class="penna-chat-thumb-remove" aria-label="Remove the image"><span class="penna-chat-thumb-disc" data-penna="floating"><i data-icon="xmark"></i></span></button></span></div><button class="penna-button" data-penna="button.send" data-shape="icon" aria-label="Chat options"><i data-icon="plus"></i></button><span class="penna-chat-input" role="textbox">This is the menu now</span><button class="penna-button" data-penna="button.send" data-shape="icon" data-emphasis="solid" data-solid="primary" aria-label="Send"><i data-icon="arrow-up"></i></button></div></div><span class="docs-note">A pasted image, its badge 24 with a 44 target</span></div>
  <div class="docs-cell" style="width: 370px"><div class="penna-chat-ask-card" data-penna="field.composer"><div class="penna-chat-ask-what" role="button"><div class="penna-chat-ask-what-head"><span class="penna-chat-ask-what-title">Run this command?</span></div><span class="penna-chat-ask-what-detail">cargo test -p penna pill_menu --all-features</span></div><div class="penna-touch-row" data-penna="phone.row" role="button"><span class="penna-touch-row-lines"><span>Allow once</span></span></div><div class="penna-touch-row" data-penna="phone.row" role="button"><span class="penna-touch-row-lines"><span>Always allow cargo test</span></span></div><div class="penna-touch-row" data-penna="phone.row" role="button"><span class="penna-touch-row-lines"><span>Deny</span></span></div></div><span class="docs-note">Ask card</span></div>
</div>
```

Not built natively yet: the phone draws the desktop's composer, chips and inline ask.

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
