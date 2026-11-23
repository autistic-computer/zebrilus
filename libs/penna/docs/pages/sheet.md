# Sheet

Holds a list or a page you drill into and returns you where you were: a picker, a chat's options, a browser's tabs. A short ask (confirm, rename, a few actions, one control) is a modal in the middle of the screen. To go into something, use a [zoom card](zoom-card.html).

## Anatomy

```figure uses=sheet,chat,touch,button,label
<div class="docs-phone" style="position: relative; width: min(100%, 390px); height: 610px">
  <div class="penna-sheet" data-still style="--penna-sheet-rest: 8px">
    <div class="penna-sheet-scrim"></div>
    <div class="penna-sheet-frame">
      <div class="penna-sheet-panel" data-penna="drawer" role="dialog" aria-label="Claude Code">
        <div class="penna-sheet-grip"><div class="penna-sheet-grabber" data-penna="grabber"></div></div>
        <div class="penna-sheet-header"><span class="penna-sheet-title"><span data-penna-text>Claude Code</span></span><button class="penna-sheet-disc" type="button" aria-label="Close"><span data-penna="button"><i data-icon="xmark"></i></span></button></div>
        <div class="penna-sheet-content">
          <div class="penna-sheet-groups">
            <div class="penna-chat-tiles">
              <button class="penna-chat-tile" data-penna="settings.card" type="button"><i data-icon="camera"></i><span>Camera</span></button>
              <button class="penna-chat-tile" data-penna="settings.card" type="button"><i data-icon="image"></i><span>Photos</span></button>
              <button class="penna-chat-tile" data-penna="settings.card" type="button"><i data-icon="file"></i><span>File</span></button>
            </div>
            <div class="penna-touch-group" data-penna="settings.card" data-ruled data-ground>
              <div class="penna-touch-row" data-penna="phone.row" role="button" tabindex="0"><i data-icon="list-tree"></i><span class="penna-touch-row-lines"><span>Mention</span></span><i data-icon="chevron-right"></i></div>
            </div>
            <div class="penna-touch-group" data-penna="settings.card" data-ruled data-ground>
              <div class="penna-touch-row" data-penna="phone.row" role="button" tabindex="0"><i data-icon="sparkle"></i><span class="penna-touch-row-lines"><span>Model</span></span><span class="penna-touch-row-value"><span>Default</span></span><i data-icon="chevron-right"></i></div>
              <div class="penna-touch-row" data-penna="phone.row" role="button" tabindex="0"><i data-icon="cpu"></i><span class="penna-touch-row-lines"><span>Effort</span></span><span class="penna-touch-row-value"><span>Default</span></span><i data-icon="chevron-right"></i></div>
              <div class="penna-touch-row" data-penna="phone.row" role="button" tabindex="0"><i data-icon="hand"></i><span class="penna-touch-row-lines"><span>Permissions</span></span><span class="penna-touch-row-value"><span>Ask</span></span><i data-icon="chevron-right"></i></div>
              <div class="penna-touch-row" data-penna="phone.row" role="button" tabindex="0"><i data-icon="user"></i><span class="penna-touch-row-lines"><span>Account</span></span><span class="penna-touch-row-value"><span>Work</span></span><i data-icon="chevron-right"></i></div>
            </div>
            <p class="penna-sheet-note">Context 0%</p>
          </div>
        </div>
      </div>
    </div>
  </div>
</div>
```

1. **Panel**: the phone container corner, twice `radius` (`drawer`).
2. **Grabber**: 36 by 5. Drag down past half its height, or fling, to dismiss. Starting to dismiss drops the virtual keyboard right away so the sheet and keyboard leave together.
3. **Header**: 44 tall, centered title in `touch-small` at 600, and the 44 close disc at its end.
4. **Groups**: 16 below the header, 8 from the sides and bottom, 16 apart, their corner the panel's less 8. Rows are flush with a hairline from the label to the end.
5. **Note**: a muted line under the last group, for the whole sheet.

```rust
use penna::blocks::sheet::{
    bottom_sheet, sheet_close, sheet_groups, sheet_inset, sheet_note,
};
use penna::blocks::touch::{TouchGroup, TouchRow, TouchTrailing};

let content = sheet_groups()
    .child(
        TouchGroup::new().ruled().row(
            TouchRow::new("model", "Model")
                .icon(IconName::Sparkle)
                .trailing(TouchTrailing::Value("Default".into())),
        ),
    )
    .child(sheet_note("Context 0%", cx));

bottom_sheet(
    "chat-options",
    "Claude Code",
    sheet_close("close", cx),
    content,
    sheet_inset(window),
    on_dismiss,
)
```

## Interactive dismiss and drill-in

Drag the sheet down by its header or tap its scrim or close button to dismiss, then tap the trigger button to open it again. Going into a subpage adds a back disc at the start while keeping close at the end; natively, `PageTurn` slides the new page in over 240 ms while easing sheet height (native-only).

```figure uses=sheet,touch,button,label
<div class="docs-pr-split">
  <div class="docs-pr-col">
    <div class="docs-pr-col-head">
      <span class="docs-pr-col-title">Live dismiss</span>
      <span class="docs-lk-mono">Drag down or tap scrim</span>
    </div>
    <div class="docs-phone" style="position: relative; height: 320px; display: flex; align-items: center; justify-content: center">
      <button class="penna-button" data-penna="button" data-size="touch" data-sheet-open="demo-live-sheet" type="button"><span class="penna-label" data-penna-text>Open sheet</span></button>
      <div class="penna-sheet" id="demo-live-sheet" data-reopenable style="--penna-sheet-rest: 8px">
        <div class="penna-sheet-scrim"></div>
        <div class="penna-sheet-frame">
          <div class="penna-sheet-panel" data-penna="drawer" role="dialog" aria-label="Permissions">
            <div class="penna-sheet-grip"><div class="penna-sheet-grabber" data-penna="grabber"></div></div>
            <div class="penna-sheet-header"><button class="penna-sheet-disc" data-side="start" type="button" aria-label="Back"><span data-penna="button"><i data-icon="chevron-left"></i></span></button><span class="penna-sheet-title"><span data-penna-text>Permissions</span></span><button class="penna-sheet-disc" data-sheet-close type="button" aria-label="Close"><span data-penna="button"><i data-icon="xmark"></i></span></button></div>
            <div class="penna-sheet-content">
              <div class="penna-sheet-groups">
                <div class="penna-touch-group" data-penna="settings.card" data-ruled data-ground>
                  <div class="penna-touch-row" data-penna="phone.row" role="button" tabindex="0"><span class="penna-touch-row-lines"><span>Ask every time</span></span><span class="penna-touch-row-state"><i data-icon="check"></i></span></div>
                  <div class="penna-touch-row" data-penna="phone.row" role="button" tabindex="0"><span class="penna-touch-row-lines"><span>Accept edits</span></span></div>
                  <div class="penna-touch-row" data-penna="phone.row" role="button" tabindex="0"><span class="penna-touch-row-lines"><span>Plan only</span></span></div>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>
  <div class="docs-pr-col">
    <div class="docs-pr-col-head">
      <span class="docs-pr-col-title">Form sheet</span>
      <span class="docs-lk-mono">56 px field + main action</span>
    </div>
    <div class="docs-phone" style="position: relative; height: 424px">
      <div class="penna-sheet" data-still style="--penna-sheet-rest: 8px">
        <div class="penna-sheet-scrim"></div>
        <div class="penna-sheet-frame">
          <div class="penna-sheet-panel" data-penna="drawer" role="dialog" aria-label="Add host">
            <div class="penna-sheet-grip"><div class="penna-sheet-grabber" data-penna="grabber"></div></div>
            <div class="penna-sheet-header"><span class="penna-sheet-title"><span data-penna-text>Add host</span></span><button class="penna-sheet-disc" type="button" aria-label="Close"><span data-penna="button"><i data-icon="xmark"></i></span></button></div>
            <div class="penna-sheet-content">
              <div style="display: flex; flex-direction: column; gap: 12px; padding: 0 16px 16px">
                <label class="penna-input" data-penna="field" data-size="touch-large"><input placeholder="user@host" aria-label="Host address" /></label>
                <button class="penna-button" data-penna="button" data-tone="primary" data-prominent data-size="touch-large" type="button" disabled><span class="penna-label" data-penna-text>Connect</span></button>
              </div>
              <div class="penna-sheet-groups">
                <div class="penna-touch-group" data-penna="settings.card" data-ruled data-ground><div class="penna-touch-row" data-penna="phone.row" role="button" tabindex="0"><i data-icon="copy"></i><span class="penna-touch-row-lines"><span>Copy SSH key</span></span></div></div>
                <p class="penna-sheet-note">Add this phone's key to authorized_keys to sign in without a password.</p>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>
</div>
```

A select on a phone opens a sheet of its options in one ruled group with the chosen option checked (fonts in their own typeface) and search/scopes above the group. A form sheet puts a 56 px field first with its placeholder as the only hint, then a 56 px main action disabled until input is valid, then any secondary row and note underneath.

## Modal

Short asks use `bottom_sheet(...).modal()` centered over the screen (up to 340 px wide, 20 px padding, no drag). A tap on the scrim cancels; in light mode the scrim steps up to 28% ink so the card stands off busy screens.

```figure uses=sheet,touch,button,input,slider,label
<div class="docs-pr-split">
  <div class="docs-pr-col">
    <div class="docs-pr-col-head"><span class="docs-pr-col-title">Action menu</span></div>
    <div class="docs-phone" style="position: relative; height: 280px">
      <div class="penna-modal" data-still>
        <div class="penna-modal-scrim"></div>
        <div class="penna-modal-area">
          <div class="penna-modal-card" data-penna="drawer" role="dialog" aria-label="bash">
            <div class="penna-modal-head"><span data-penna-text>bash</span><button class="penna-sheet-disc" type="button" aria-label="Close"><span data-penna="button"><i data-icon="xmark"></i></span></button></div>
            <div class="penna-sheet-groups" data-menu>
              <div class="penna-touch-group" data-penna="settings.card" data-ruled data-ground>
                <div class="penna-touch-row" data-penna="phone.row" role="button" tabindex="0"><i data-icon="pencil"></i><span class="penna-touch-row-lines"><span>Rename</span></span></div>
                <div class="penna-touch-row" data-penna="phone.row" data-tone="danger" role="button" tabindex="0"><i data-icon="xmark"></i><span class="penna-touch-row-lines"><span>Close window</span></span></div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>
  <div class="docs-pr-col">
    <div class="docs-pr-col-head"><span class="docs-pr-col-title">Confirm</span></div>
    <div class="docs-phone" style="position: relative; height: 280px">
      <div class="penna-modal" data-still>
        <div class="penna-modal-scrim"></div>
        <div class="penna-modal-area">
          <div class="penna-modal-card" data-penna="drawer" role="dialog" aria-label="Close this pane?">
            <div class="penna-modal-head"><span data-penna-text>Close this pane?</span></div>
            <p class="penna-modal-detail">The chat stops. Its transcript stays.</p>
            <div class="penna-modal-bar">
              <button class="penna-button" data-penna="button" data-size="touch" type="button"><span class="penna-label" data-penna-text>Cancel</span></button>
              <button class="penna-button" data-penna="button" data-tone="danger" data-prominent data-size="touch" type="button"><span class="penna-label" data-penna-text>Close</span></button>
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>
  <div class="docs-pr-col">
    <div class="docs-pr-col-head"><span class="docs-pr-col-title">Rename</span></div>
    <div class="docs-phone" style="position: relative; height: 280px">
      <div class="penna-modal" data-still>
        <div class="penna-modal-scrim"></div>
        <div class="penna-modal-area">
          <div class="penna-modal-card" data-penna="drawer" role="dialog" aria-label="Rename window">
            <div class="penna-modal-head"><span data-penna-text>Rename window</span></div>
            <label class="penna-input" data-penna="field" data-size="touch"><input value="cargo watch" aria-label="Window name" /></label>
            <div class="penna-modal-bar">
              <button class="penna-button" data-penna="button" data-size="touch" type="button"><span class="penna-label" data-penna-text>Cancel</span></button>
              <button class="penna-button" data-penna="button" data-tone="accent" data-prominent data-size="touch" type="button"><span class="penna-label" data-penna-text>Rename</span></button>
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>
  <div class="docs-pr-col">
    <div class="docs-pr-col-head"><span class="docs-pr-col-title">Control</span></div>
    <div class="docs-phone" style="position: relative; height: 280px">
      <div class="penna-modal" data-still>
        <div class="penna-modal-scrim"></div>
        <div class="penna-modal-area">
          <div class="penna-modal-card" data-penna="drawer" role="dialog" aria-label="Font size">
            <div class="penna-modal-head"><span data-penna-text>Font size</span><button class="penna-sheet-disc" type="button" aria-label="Close"><span data-penna="button"><i data-icon="xmark"></i></span></button></div>
            <div class="penna-modal-control">
              <div class="penna-slider" data-min="11" data-max="20" data-value="14" data-step="1" aria-label="Font size" style="flex: 1"><div class="penna-rail"><div class="penna-fill"></div><div class="penna-knob"></div></div></div>
              <span data-penna-text>14 px</span>
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>
</div>
```

- **Action menu**: item name, close disc, actions in one ruled group with the destructive action last in danger ink.
- **Confirm**: question, muted detail line, and 48 px Cancel + danger verb (no close disc). When labels cannot share a line at large text, `modal_buttons_stack` (`data-stack`) stacks them with the verb on top.
- **Rename & SSH prompts**: 48 px field (masked for passwords/passphrases, with mono fingerprint for SSH host prompts), Cancel, and an accent verb disabled until the text changes.
- **Control**: label, close disc, and a single slider with its value beside it.

```rust
use penna::blocks::sheet::{bottom_sheet, modal_buttons, ModalAction, ModalTone};

bottom_sheet(
    "confirm-close",
    "Close this pane?",
    (),
    modal_buttons(
        "confirm-close",
        ModalAction::new("close", "Close", ModalTone::Danger, on_confirm),
        window,
        cx,
    ),
    px(0.0),
    on_cancel,
)
.modal()
.detail("The chat stops. Its transcript stays.")
```

## API (`penna::blocks::sheet`)

| Rust | HTML | Does |
| --- | --- | --- |
| `bottom_sheet(id, title, actions, content, bottom, on_dismiss)` | `.penna-sheet`, `.penna-sheet-panel[data-penna="drawer"]` | Bottom sheet with grabber, centered title, and dismiss gesture |
| `BottomSheet::{back, turned_from, content_scroll, full}` | `.penna-sheet-disc[data-side="start"]`, `.penna-sheet-content` | Drill-in back button, `PageTurn` slide (native-only), scroll handoff, full height |
| `BottomSheet::{modal, detail, cut_middle}` | `.penna-modal`, `.penna-modal-card[data-penna="drawer"]`, `.penna-modal-detail` | Centered phone modal, muted subtitle, middle-ellipsis title |
| `sheet_close(id, cx)`, `sheet_back(id, cx)` | `.penna-sheet-disc` | 44 px circular close and back buttons |
| `sheet_groups()`, `sheet_note(text, cx)`, `TouchGroup::new().ruled()` | `.penna-sheet-groups`, `.penna-sheet-note`, `.penna-touch-group[data-ruled]` | Ruled option groups and footer note |
| `modal_buttons`, `modal_buttons_labelled`, `modal_buttons_stack` | `.penna-modal-bar`, `[data-stack]` | 48 px Cancel + verb bar (stacks at large text) |
| `ModalAction::new(id, label, tone, on_click)`, `ModalTone::{Danger, Accent}` | `[data-tone="danger"\|"accent"][data-prominent]` | Confirm/rename modal action button (`Disableable`) |
| `sheet_ground(cx)`, `sheet_group_ground(cx)`, `sheet_inset(window)` | none | Grounds for sheet controls/groups and bottom safe-area inset |
| `close_sheet(id, cx)`, `floating_sheet(sheet, window)` | `pennaSheet.{open, close, openModal, closeModal}` | Programmatic dismiss and floating wrapper |
