# Disclosure

A collapsible section with a cap-aligned animated chevron (`220 ms ease_out_quint`) for optional details, inspector groups, and log outputs.

## Collapsible section

Click either trigger row to expand or collapse its body. Controlled (`open` + `on_toggle`) and uncontrolled (`default_open`) modes share the same layout.

```figure uses=disclosure,icon
<div style="display: flex; flex-direction: column; gap: 6px; width: 100%; max-width: 460px">
  <div class="penna-disclosure">
    <button class="penna-disclosure-trigger" data-penna="menu-item" type="button" aria-expanded="true">
      <i class="penna-disclosure-chev" data-icon="chevron-right"></i>
      <span class="penna-disclosure-title">Build environment</span>
      <span class="penna-disclosure-meta">3 vars</span>
    </button>
    <div class="penna-disclosure-body">RUSTFLAGS="-D warnings" · CARGO_INCREMENTAL=1 · target/docs</div>
  </div>
  <div class="penna-disclosure">
    <button class="penna-disclosure-trigger" data-penna="menu-item" type="button" aria-expanded="false">
      <i class="penna-disclosure-chev" data-icon="chevron-right"></i>
      <span class="penna-disclosure-title">Network peers</span>
      <span class="penna-disclosure-meta">2 hosts</span>
    </button>
    <div class="penna-disclosure-body" hidden>ubuntu.local (0.9 ms) · thinkpad.local (2.4 ms)</div>
  </div>
</div>
```

```rust
Collapsible::new("build-env", "Build environment")
    .meta("3 vars")
    .default_open(true)
    .child(env_details)
```

## API (`penna::Collapsible`)

| Rust | HTML (`uses=disclosure`) | Role |
| --- | --- | --- |
| `Collapsible::new(id, title)` | `.penna-disclosure` | Collapsible section with `Role::Button` trigger |
| `.meta(text)` | `.penna-disclosure-meta` | Trailing muted count or summary |
| `.default_open(bool)` | `aria-expanded` / `hidden` | Initial uncontrolled expansion state |
| `.open(bool)` / `.on_toggle(f)` | `change` event (`{ open }`) | Controlled open state and change callback |
