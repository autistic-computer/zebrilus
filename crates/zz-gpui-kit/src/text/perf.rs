use std::{
    cell::Cell,
    rc::Rc,
    sync::atomic::Ordering,
    time::{Duration, Instant},
};

use zz_gpui::{
    AppContext as _, Context, Entity, IntoElement, Modifiers, ParentElement as _, Render,
    Styled as _, TestAppContext, VisualTestContext, Window, div, point, px,
};

use super::{TextView, TextViewState, state::parse_counters};
use crate::Root;

const CHUNK_BYTES: usize = 40;
const BURST: usize = 4;
const CODE_LINES: usize = 40;

struct Host {
    state: Entity<TextViewState>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(px(720.))
            .child(TextView::new(&self.state).selectable(true))
    }
}

fn host<'a>(
    source: &str,
    cx: &'a mut TestAppContext,
) -> (Entity<TextViewState>, &'a mut VisualTestContext) {
    cx.update(crate::init);
    let source = source.to_string();
    let (root, cx) = cx.add_window_view(move |window, cx| {
        let host = cx.new(|cx| Host {
            state: cx.new(|cx| TextViewState::markdown(&source, cx)),
        });
        Root::new(host, window, cx)
    });
    let state = root.read_with(cx, |root, cx| {
        root.view()
            .clone()
            .downcast::<Host>()
            .unwrap()
            .read(cx)
            .state
            .clone()
    });
    cx.run_until_parked();
    draw(cx);
    (state, cx)
}

fn draw(cx: &mut VisualTestContext) -> Duration {
    cx.update(|window, cx| {
        let start = Instant::now();
        window.draw(cx).clear(cx);
        start.elapsed()
    })
}

fn code(lang: &str, ix: usize) -> String {
    let body = match lang {
        "rust" => (0..CODE_LINES)
            .map(|line| {
                format!(
                    "    let value_{line} = pane.split({line}, Axis::Horizontal).map(|p| p.id() + {ix});\n"
                )
            })
            .collect::<String>(),
        "json" => (0..CODE_LINES)
            .map(|line| format!("  \"key_{line}\": {{ \"pane\": {line}, \"ok\": true }},\n"))
            .collect(),
        "toml" => (0..CODE_LINES)
            .map(|line| format!("option_{line} = {{ value = {line}, enabled = true }}\n"))
            .collect(),
        "bash" => (0..CODE_LINES)
            .map(|line| format!("zz split-window -h -t session:{line} && echo done {line}\n"))
            .collect(),
        _ => (0..CODE_LINES)
            .map(|line| format!("    result_{line} = compute(pane={line}, depth={ix})\n"))
            .collect(),
    };
    format!("```{lang}\n{body}```\n\n")
}

fn table(ix: usize) -> String {
    let mut table =
        String::from("| Command | Flag | Effect | Notes |\n| --- | :---: | --- | ---: |\n");
    for row in 0..8 {
        table.push_str(&format!(
            "| `split-window {row}` | `-h` | splits pane {row} of table {ix} | [docs](https://zz.example/t{ix}/{row}) |\n"
        ));
    }
    table.push('\n');
    table
}

fn transcript() -> String {
    let langs = ["rust", "json", "toml", "bash", "python", "rust"];
    let mut source = String::new();
    for section in 0..12 {
        source.push_str(&format!(
            "## Step {section}: wiring the `pane_{section}` handler\n\n"
        ));
        for part in 0..3 {
            source.push_str(&format!(
                "The daemon keeps **session {section}.{part}** alive while the client reconnects. \
                 See [the protocol notes](https://zz.example/protocol/{section}) and the \
                 `Reducer::apply` path before touching `snapshot_{section}`. Nothing here blocks \
                 the *render* loop, and the [key table](https://zz.example/keys/{section}) stays \
                 the source of truth.\n\n"
            ));
        }
        source.push_str(&format!(
            "- Parse the reply for step {section}\n  - check `status` first\n    - then the \
             [payload](https://zz.example/p/{section})\n  - fall back to `defaults`\n- Render \
             the result\n  1. layout\n  2. paint with `theme.foreground`\n\n"
        ));
        if section % 4 == 1 {
            source.push_str(&table(section));
        }
        if section % 2 == 0 {
            source.push_str(&code(langs[section / 2], section));
        }
        source.push_str(&format!(
            "> Note {section}: a blockquote with `inline code` and a [link](https://zz.example/n/{section}).\n\n"
        ));
    }
    source
}

fn chunks(source: &str) -> Vec<&str> {
    let mut chunks = Vec::new();
    let mut start = 0;
    while start < source.len() {
        let mut end = (start + CHUNK_BYTES).min(source.len());
        while !source.is_char_boundary(end) {
            end += 1;
        }
        chunks.push(&source[start..end]);
        start = end;
    }
    chunks
}

fn percentile(samples: &[Duration], pct: usize) -> Duration {
    let mut sorted = samples.to_vec();
    sorted.sort();
    sorted[(sorted.len() - 1) * pct / 100]
}

fn micros(duration: Duration) -> u128 {
    duration.as_micros()
}

fn reset_counters() {
    parse_counters::BACKGROUND.store(0, Ordering::SeqCst);
    parse_counters::COMMITTED.store(0, Ordering::SeqCst);
}

fn counters() -> (usize, usize) {
    (
        parse_counters::BACKGROUND.load(Ordering::SeqCst),
        parse_counters::COMMITTED.load(Ordering::SeqCst),
    )
}

#[zz_gpui::test]
#[ignore = "perf harness, run with --ignored --nocapture"]
fn perf_markdown_stream(cx: &mut TestAppContext) {
    let source = transcript();
    assert!(
        (28 * 1024..34 * 1024).contains(&source.len()),
        "{}",
        source.len()
    );
    assert_eq!(source.matches("```").count(), 12);
    let chunks = chunks(&source);

    let (state, cx) = host("", cx);
    reset_counters();
    let mut parse = Vec::with_capacity(chunks.len());
    let mut paint = Vec::with_capacity(chunks.len());
    for chunk in &chunks {
        let start = Instant::now();
        state.update(cx, |state, cx| state.push_str(chunk, cx));
        cx.run_until_parked();
        parse.push(start.elapsed());
        paint.push(draw(cx));
    }
    let (paced_parses, paced_commits) = counters();
    state.read_with(cx, |state, _| assert_eq!(state.source().as_str(), source));

    state.update(cx, |state, cx| state.set_text("", cx));
    cx.run_until_parked();
    draw(cx);
    let dispatcher = cx.executor().dispatcher().clone();
    let dispatcher = dispatcher.as_test().unwrap();
    reset_counters();
    let burst_start = Instant::now();
    for frame in chunks.chunks(BURST) {
        for chunk in frame {
            state.update(cx, |state, cx| state.push_str(chunk, cx));
            while dispatcher.tick(true) {}
        }
        cx.run_until_parked();
        draw(cx);
    }
    let burst = burst_start.elapsed();
    let (burst_parses, burst_commits) = counters();
    state.read_with(cx, |state, _| assert_eq!(state.source().as_str(), source));

    println!(
        "PERF stream chunks={} parse_total_us={} parse_p50_us={} parse_p95_us={} draw_total_us={} draw_p50_us={} draw_p95_us={} bg_parses={} commits={} burst_total_us={} burst_bg_parses={} burst_commits={}",
        chunks.len(),
        micros(parse.iter().sum()),
        micros(percentile(&parse, 50)),
        micros(percentile(&parse, 95)),
        micros(paint.iter().sum()),
        micros(percentile(&paint, 50)),
        micros(percentile(&paint, 95)),
        paced_parses,
        paced_commits,
        micros(burst),
        burst_parses,
        burst_commits,
    );
}

#[zz_gpui::test]
#[ignore = "perf harness, run with --ignored --nocapture"]
fn perf_link_hover_sweep(cx: &mut TestAppContext) {
    let mut paragraph = String::new();
    let mut rendered = 0;
    let mut ix = 0;
    while rendered < 2_000 {
        let word = if ix % 6 == 5 {
            paragraph.push_str(&format!("[link {ix}](https://zz.example/{ix}) "));
            format!("link {ix} ")
        } else {
            let word = format!("word{ix} ");
            paragraph.push_str(&word);
            word
        };
        rendered += word.chars().count();
        ix += 1;
    }
    let (state, cx) = host(&paragraph, cx);
    let bounds = state.read_with(cx, |state, _| state.bounds());
    let glyphs = state.read_with(cx, |state, _| state.parsed_content.document.text().len());
    let (line_height, advance) = cx.update(|window, _| {
        let font_size = window.text_style().font_size.to_pixels(window.rem_size());
        (window.line_height(), font_size * 0.6)
    });
    let notified = Rc::new(Cell::new(0usize));
    let _subscription = cx.update(|_, cx| {
        let notified = notified.clone();
        cx.observe(&state, move |_, _| notified.set(notified.get() + 1))
    });

    let mut moves = 0;
    let mut draws = 0;
    let start = Instant::now();
    let mut y = bounds.top() + line_height / 2.;
    while y < bounds.bottom() && moves < glyphs {
        let mut x = bounds.left() + advance / 2.;
        while x < bounds.right() && moves < glyphs {
            let before = notified.get();
            cx.simulate_mouse_move(point(x, y), None, Modifiers::default());
            moves += 1;
            if notified.get() != before {
                draw(cx);
                draws += 1;
            }
            x += advance;
        }
        y += line_height;
    }
    let elapsed = start.elapsed();
    println!(
        "PERF hover glyphs={glyphs} moves={moves} invalidations={} draws={draws} total_us={}",
        notified.get(),
        micros(elapsed),
    );
}
