# Motion

Transitions clarify where a surface came from and where it went. Everything runs on a small set of short durations, respects `Motion::Fast` at double speed, and stops completely under `Motion::Off` or system reduced motion.

## Motion lab

Switch speed between `Default` (1×), `Fast` (2×), and `Off`, then replay or toggle each primitive to feel how penna times surfaces, disclosures, rows, and loaders.

```figure uses=button,icon,panel,popover,segmented,spinner,switch
<div class="docs-mo-lab" data-docs-motion>
  <div class="docs-mo-bar">
    <div class="penna-segmented" data-penna="field" data-size="small" data-value="0" role="radiogroup" aria-label="Motion speed" data-mo-speed>
      <div class="penna-segments" data-penna="button.quiet" data-state="current">
        <span class="penna-segment-pill"></span>
        <span class="penna-segment" role="radio" aria-checked="true"><span class="penna-label" data-penna-text>Default 1×</span></span>
        <span class="penna-segment" role="radio" aria-checked="false"><span class="penna-label" data-penna-text>Fast 2×</span></span>
        <span class="penna-segment" role="radio" aria-checked="false"><span class="penna-label" data-penna-text>Off</span></span>
      </div>
    </div>
    <button class="penna-button" data-penna="button" data-size="small" type="button" data-mo-replay><i data-icon="redo-2"></i><span class="penna-label" data-penna-text>Replay all</span></button>
  </div>

  <div class="docs-mo-grid">
    <div class="docs-mo-card" data-penna="panel" data-ground>
      <div class="docs-mo-head">
        <span class="docs-pr-col-title">Surface presence</span>
        <span class="docs-lk-mono" data-mo-ms="surface">160 ms in · 100 ms out</span>
      </div>
      <div class="docs-mo-stage">
        <div class="docs-mo-popover" data-penna="popover" data-ground data-mo-surface data-shown="true">
          <span class="penna-type" data-type="small" data-strong>Floating menu</span>
          <span class="penna-type" data-type="caption" data-muted>6 px rise · ease-out-quint</span>
        </div>
      </div>
      <div class="docs-mo-foot">
        <span class="docs-lk-mono">Presence · surface_enter</span>
        <button class="penna-button" data-penna="button.quiet" data-size="xsmall" type="button" data-mo-toggle-surface><span class="penna-label" data-penna-text>Toggle</span></button>
      </div>
    </div>

    <div class="docs-mo-card" data-penna="panel" data-ground>
      <div class="docs-mo-head">
        <span class="docs-pr-col-title">Disclosure turn</span>
        <span class="docs-lk-mono" data-mo-ms="turn">220 ms · 90°</span>
      </div>
      <div class="docs-mo-stage">
        <button class="docs-mo-disc-btn" data-penna="menu-item" type="button" data-mo-disclosure aria-expanded="false">
          <i class="docs-mo-chev" data-icon="chevron-right"></i>
          <span class="penna-type" data-type="small" data-strong>src/foundation</span>
          <span class="docs-spacer"></span>
          <span class="penna-type" data-type="caption" data-muted>4 files</span>
        </button>
        <div class="docs-mo-disc-body" data-mo-disc-body hidden>
          <span class="docs-lk-mono">motion.rs · animation.rs · theme.rs</span>
        </div>
      </div>
      <div class="docs-mo-foot">
        <span class="docs-lk-mono">Disclosure::new</span>
        <span class="docs-lk-mono">ease-out-quint</span>
      </div>
    </div>

    <div class="docs-mo-card" data-penna="panel" data-ground>
      <div class="docs-mo-head">
        <span class="docs-pr-col-title">Row enter</span>
        <span class="docs-lk-mono" data-mo-ms="enter">200 ms · 8 px drop</span>
      </div>
      <div class="docs-mo-stage">
        <div class="docs-mo-rows" data-mo-rows>
          <div class="docs-mo-row" data-penna="card" data-ground><i data-icon="square-terminal"></i><span>work · cargo test</span></div>
          <div class="docs-mo-row" data-penna="card" data-ground><i data-icon="git-branch"></i><span>main · 3 files changed</span></div>
        </div>
      </div>
      <div class="docs-mo-foot">
        <span class="docs-lk-mono">enter / enter_since</span>
        <button class="penna-button" data-penna="button.quiet" data-size="xsmall" type="button" data-mo-replay-rows><span class="penna-label" data-penna-text>Replay</span></button>
      </div>
    </div>

    <div class="docs-mo-card" data-penna="panel" data-ground>
      <div class="docs-mo-head">
        <span class="docs-pr-col-title">Appear &amp; spin</span>
        <span class="docs-lk-mono" data-mo-ms="spin">120 ms · 900 ms loop</span>
      </div>
      <div class="docs-mo-stage docs-mo-stage-row">
        <div class="docs-mo-appear" data-mo-appear>
          <span class="penna-badge" data-penna="badge" data-tone="success"><i data-icon="circle-check"></i><span class="badge-text">Connected</span></span>
        </div>
        <span class="penna-spinner docs-mo-spinner" data-size="large" data-color="accent" data-mo-spinner><i data-icon="loader"></i></span>
      </div>
      <div class="docs-mo-foot">
        <span class="docs-lk-mono">appear · SPIN</span>
        <button class="penna-button" data-penna="button.quiet" data-size="xsmall" type="button" data-mo-replay-appear><span class="penna-label" data-penna-text>Fade in</span></button>
      </div>
    </div>
  </div>
</div>
```

## Durations and curves

Every constant lives in `src/foundation/motion.rs` and `src/foundation/animation.rs`. Entering elements decelerate with `ease_out_quint`; leaving overlays accelerate out (`cubic-bezier(0.4, 0, 1, 1)`) in 100 ms so dismissal never feels sluggish.

| Constant | Default | Fast (2×) | Easing | Used for |
| --- | --- | --- | --- | --- |
| `SURFACE_LEAVE_DURATION` | `100 ms` | `50 ms` | `accelerate(0.4, 0, 1, 1)` | Closing popovers, menus, sheets, and dialogs |
| `APPEAR` | `120 ms` | `60 ms` | Linear opacity | Subtle status badges and newly mounted controls |
| `SURFACE_ENTER_DURATION` | `160 ms` | `80 ms` | `ease_out_quint` (`6 px` rise) | Opening floating surfaces via `surface_enter` |
| `ENTER` | `200 ms` | `100 ms` | `ease_out_quint` (`8 px` drop) | Appended chat turns, toasts, and list insertions |
| `TURN` | `220 ms` | `110 ms` | `ease_out_quint` | `Disclosure` chevron `0 → 90°` rotation |
| `SPIN` | `900 ms` | `450 ms` | Linear loop | `Spinner` full rotation |

## Presence and exit mounting

Closing a floating surface must not unmount it on the frame `open` becomes `false`. `Presence` tracks both state and timestamp so the surface stays mounted for `SURFACE_LEAVE_DURATION` (100 ms), and cancels exit smoothly if reopened mid-flight.

```rust
use penna::foundation::motion::{Disclosure, Motion, Presence, enter};

let mut popover = Presence::default();
popover.set(true, cx.background_executor().now());

if let Some(shown) = popover.shown(window, cx) {
    render_menu(shown);
}

let chevron = Disclosure::new("folder-chevron", expanded, px(14.0));
let item = enter("new-row", row);
```

## Direct manipulation and reduced motion

- **Pointer drags stay 1:1**: never ease a slider thumb, resize sash, or scroll position while a pointer or finger is moving it. Motion starts only after release or keyboard input.
- **No oscillation**: desktop UI never uses bouncy springs. `ease_out_quint` reaches rest cleanly without overshoot.
- **`Motion::Off`**: skips straight to `1.0` (or `None` when closed), disables `Spinner` rotation, and mirrors `@media (prefers-reduced-motion: reduce)` in HTML twins.

## API (`penna::foundation::motion`)

| Symbol | Role |
| --- | --- |
| `Motion::{Off, Default, Fast}` | Global speed knob (`reduced()`, `speed() -> 1.0 \| 2.0`) |
| `Presence` | Stateful mount & exit progress (`set`, `at`, `shown`, `mounted`) |
| `surface_enter(el, id, top)` | 160 ms `ease_out_quint` opacity + 6 px rise |
| `Disclosure::new(id, open, size)` | Animated 220 ms chevron rotating `0..FRAC_PI_2` |
| `enter(id, row)` / `enter_since(...)` | 200 ms opacity + 8 px drop for inserted rows |
| `appear(id, control)` | 120 ms opacity fade-in |
| `turned(id, on, cx)` / `eased(...)` | Low-level state-change timestamp and eased progress |
