# Stack menu

A floating column of 52 px thumb-friendly chips that rises out of a phone pill or key anchor on tap or press-and-hold. Slide onto a chip and release to trigger in one motion, or tap to leave the stack open.

## Anchored above a pill

`StackMenu` sits over a light scrim (`StackMenu::scrim`), groups chips on `floating` glass surfaces (`StackLook::Rest`, `Quiet`, and `Danger`), and places a 56 px primary close disc directly over the pill (`StackClose::Disc`) so the thumb can dismiss without reaching.

```figure uses=touch,compact
<div style="position: relative; width: 100%; max-width: 390px; height: 424px; overflow: hidden; border: 1px solid var(--penna-divider); border-radius: calc(var(--penna-phone-radius, 24px) * var(--penna-corner-grow, 1.42)); corner-shape: superellipse(var(--penna-window-k)); background: var(--penna-bg)">
  <div style="position: absolute; left: 0; right: 0; bottom: 0">
    <div class="penna-compact-bar">
      <button class="penna-compact-pill" data-penna="floating" style="visibility: hidden" type="button"><span class="penna-pill-tile" data-penna="phone.pill.icon"><i data-icon="square-terminal"></i></span><span class="penna-pill-copy"><span class="penna-pill-title">zsh</span></span></button>
    </div>
  </div>
  <div class="penna-stack" data-open data-from=".penna-compact-pill">
    <div class="penna-stack-scrim"></div>
    <div class="penna-stack-column">
      <div class="penna-stack-group">
        <button class="penna-stack-chip" data-penna="floating" data-look="quiet" type="button"><i data-icon="keyboard"></i><span data-penna-text>All bindings</span></button>
      </div>
      <div class="penna-stack-group">
        <button class="penna-stack-chip" data-penna="floating" type="button"><i data-icon="pencil"></i><span data-penna-text>Rename</span></button>
        <button class="penna-stack-chip" data-penna="floating" data-tone="danger" type="button"><i data-icon="xmark"></i><span data-penna-text>Close</span></button>
      </div>
      <div class="penna-stack-group">
        <span class="penna-stack-label" data-penna="floating"><span data-penna-text>New in zz</span></span>
        <button class="penna-stack-chip" data-penna="floating" data-state="selected" type="button"><i data-icon="square-terminal"></i><span data-penna-text>New terminal</span></button>
        <button class="penna-stack-chip" data-penna="floating" type="button"><i data-icon="chat-plus"></i><span data-penna-text>New chat</span></button>
      </div>
    </div>
    <button class="penna-stack-close" data-penna="button" data-tone="primary" data-prominent type="button" aria-label="Close"><i data-icon="xmark"></i></button>
  </div>
</div>
```

```rust
let menu = cx.new(|cx| StackMenu::new(StackClose::Disc, cx));
menu.update(cx, |menu, cx| {
    menu.set_groups(
        vec![
            StackGroup::new([
                StackChip::new("rename", "Rename", IconName::Pencil),
                StackChip::new("close", "Close", IconName::Xmark).look(StackLook::Danger),
            ]),
            StackGroup::new([
                StackChip::new("term", "New terminal", IconName::SquareTerminal),
                StackChip::new("chat", "New chat", IconName::ChatPlus),
            ])
            .label("New in zz"),
        ],
        cx,
    );
});
```

## Gesture and chord rules

- **Hold-and-slide (`StackGesture`):** holding an anchor for `STACK_HOLD` (`280ms`, within `STACK_SLOP = 10px`) opens the stack; dragging across chips lifts the active chip and releasing fires `StackMenuEvent::Pick(id)`.
- **Glyph rows and chord grids:** `StackGroup::row` lays out up to three 52 × 52 square icon chips (`data-row` + `data-glyph`); `StackChip::text` renders label-only chord chips (up to 6 in one column, or 2 columns for larger groups) anchored directly to a `StackKey` (`StackClose::Anchor`).
- **Address chips:** `.place()` middle-truncates long paths or URLs via `middle_line` so both the host and leaf stay readable.

## API

| Rust (`penna::blocks::touch`) | HTML (`uses=touch,compact`) | Role |
| --- | --- | --- |
| `StackMenu::new(StackClose::Disc \| Anchor, cx)` | `.penna-stack[data-open][data-from]` | Floating anchored stack container |
| `StackGroup::new(chips)` / `.row(chips)` | `.penna-stack-group` (`[data-row]`, `[data-chords]`) | Chip column, glyph row, or chord grid |
| `StackChip::new(id, label, icon)` / `::text` | `button.penna-stack-chip[data-penna="floating"]` | 52 px floating action chip |
| `.look(StackLook::{Rest, Quiet, Danger})` | `data-look="quiet"`, `data-tone="danger"` | Visual emphasis |
| `StackKey::new(label, tap, cx)` | Keyboard bar anchor button | Modifier key that latches or opens a stack |
| `StackMenuEvent::Pick(SharedString)` | Click / release on `.penna-stack-chip` | Chosen action identifier |
