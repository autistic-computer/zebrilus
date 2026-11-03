# Installation

## Rust

Add ala, its platform crate and penna to your `Cargo.toml`:

```toml
[dependencies]
ala = { git = "https://github.com/autistic-computer/ala" }
ala-platform = { git = "https://github.com/autistic-computer/ala" }
penna = { git = "https://github.com/autistic-computer/penna" }
```

Call `penna::init` once when the app starts, before opening a window:

```rust
fn main() {
    ala_platform::application().run(|cx| {
        penna::init(cx);
    });
}
```

## HTML

Generate the theme tokens, then load the kit and the CSS of the widgets you use:

```html
<link rel="stylesheet" href="web/penna.css" />
<link rel="stylesheet" href="src/components/button/button.css" />
<script src="web/kit.js" data-tokens="web/tokens/"></script>
```

`kit.js` loads the tokens for the picked theme and mode and remembers the choice. Run `cargo run --example theme_css` to write the tokens into `web/tokens`.
