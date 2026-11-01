# Pele

pele is a native Rust port of [Pele](https://github.com/obsidianmd/pele) at
revision `36e584d59152181927e0d8bf70fddcd2a5f33d6c`. It renders Mermaid text to
static SVG with no JavaScript engine, DOM or browser.

It covers flowcharts, sequence, class, state, entity relationship and gantt
diagrams, pie charts and quadrant charts. `pele::supports` says whether a source
is one of those; other diagram families return an `unsupported-diagram` error.
[PORT_STATUS.json](PORT_STATUS.json) lists the ported features, the remaining
gaps and the recorded compatibility results.

## Rendering

```rust
let options = pele::RenderOptions::default();
let result = pele::render("flowchart LR\n A[Source] --> B[SVG]", &options)?;
```

The result carries the SVG markup, its size, the diagram type and the links.
Themes take concrete SVG colors. `RenderOptions.measurer` lets a host measure
labels with its own fonts, and `RenderOptions.transparent` leaves the canvas
unpainted for a host that draws its own backdrop.

The development binary reads JSON lines on stdin and answers each with `ok: true`
and the result, or `ok: false` and a structured error:

```sh
echo '{"source":"flowchart LR\n A[Source] --> B[SVG]"}' | cargo run --quiet --bin pele
```

## Checks

```sh
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

The tests use checked-in oracle fixtures and need no Node or upstream checkout.
They cover native rendering and resvg 0.46 rasterization, source diagnostics,
layered and compound layout coordinates, edge paths, chart geometry, extended
shapes, text measurement, escaping and limits. The class, state, ER and gantt
suites compare whole SVG bodies with upstream output.

## Comparing with upstream

Check out Pele at the revision above and run its `npm ci` and `npm run build`.
Then:

```sh
cargo build --bin pele
PELE_SOURCE=/path/to/upstream/pele node tools/compat.mjs
PELE_SOURCE=/path/to/upstream/pele node tools/compat.mjs --all-docs --output report.json
```

The full documentation comparison reports the known gaps and so exits nonzero.
Rendering and matching outer dimensions do not prove parser or pixel
equivalence.
