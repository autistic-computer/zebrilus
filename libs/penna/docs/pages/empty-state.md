# Empty state

On a phone, an empty screen says what is missing; its main action is how to get it.

## Anatomy

```figure uses=touch,button
<div style="display: flex; flex-direction: column; width: 370px; max-width: 100%; height: 460px; border-radius: 28px; background: var(--penna-bg); box-shadow: 0 0 0 1px var(--penna-divider)">
  <div style="height: 96px"></div>
  <div class="penna-empty"><i data-icon="laptop"></i><b>No machines yet</b></div>
  <div style="flex: 1"></div>
  <div class="penna-action-bar"><button class="penna-button" data-penna="button" data-tone="primary" data-prominent data-size="touch-large"><span class="penna-label" data-penna-text>Add machine</span></button></div>
</div>
```

1. **Icon or spinner**: 32 muted icon when empty, or a 16 spinner when connecting.
2. **Title**: `title`, what is missing or under way.
3. **Line**: `touch-small`, muted, only when there is a state to say ("Waiting for ssh on fabrico@xps"), never how to use the screen.
4. **Action**: the screen's main action, in the action bar, never a button under the text.

## API

| Rust | HTML | Does |
| --- | --- | --- |
| `EmptyState::new(title)` | `.penna-empty` | Centered phone empty state |
| `.icon(name)`, `.spinner()` | `<i data-icon>`, `.penna-touch-spinner` | Leading mark above the title |
| `.detail(text)` | `<span>` under `<b>` | Optional state line |
