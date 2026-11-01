# Pele for Rust

A native Rust port of [Pele](https://github.com/obsidianmd/pele), rendering Mermaid
text to static SVG. The render path requires no JavaScript engine, DOM, or browser.

The port currently covers flowcharts, sequence diagrams, class, state, entity
relationship and gantt diagrams, pie charts, and quadrant charts. Other diagram
families return an explicit unsupported error. See [PORT_STATUS.json](PORT_STATUS.json)
for the supported features, remaining gaps, and recorded compatibility results.

```rust
fn main() -> Result<(), pele::PeleError> {
    let options = pele::RenderOptions::default();
    let result = pele::render("flowchart LR\n A[Source] --> B[SVG]", &options)?;
    println!("{}", result.svg);
    Ok(())
}
```

The result includes SVG markup, dimensions, diagram type, and link metadata. Themes
use concrete SVG values. Hosts can supply a native text measurer through
`RenderOptions.measurer` to match their fonts.

The development CLI accepts JSON-lines requests on stdin:

```sh
echo '{"source":"flowchart LR\n A[Source] --> B[SVG]"}' | cargo run --quiet --bin pele
```

Each response contains either `ok: true` and the rendering result or `ok: false`
and a structured error.

Build and check with Rust 1.99 or newer:

```sh
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

The ordinary tests use checked-in oracle fixtures and do not require Node or an
upstream checkout. They cover native rendering and resvg 0.46 rasterization,
source diagnostics, layered and compound layout coordinates, edge paths, chart
geometry, extended shapes, text measurement, escaping, and limits. The class,
state, ER and gantt suites compare whole SVG bodies with upstream output.

To compare with the original renderer, prepare a checkout of Pele revision
`36e584d59152181927e0d8bf70fddcd2a5f33d6c` and run its `npm ci` and
`npm run build`. Then:

```sh
cargo build --bin pele
PELE_SOURCE=/path/to/upstream/pele node tools/compat.mjs
PELE_SOURCE=/path/to/upstream/pele node tools/compat.mjs --all-docs --output report.json
```

The full documentation comparison currently reports known gaps and therefore
returns a nonzero exit code. Renderability and matching outer dimensions do not
establish complete parser or pixel equivalence.

Source provenance and the upstream MIT notice are preserved in [NOTICE](NOTICE).
The code is distributed under the [MIT license](LICENSE).
