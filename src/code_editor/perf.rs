use std::time::{Duration, Instant};

use ala::TestAppContext;

use super::state::{CodeEditorState, Undo};

const LINES: usize = 5_000;
const TYPED: usize = 200;
const ROUNDS: usize = 5;

#[ala::test]
#[ignore = "perf harness, run with --ignored --nocapture"]
fn perf_code_editor_undo(cx: &mut TestAppContext) {
    cx.update(crate::init);
    let source = (0..LINES)
        .map(|line| format!("fn handler_{line}(pane: usize) -> usize {{ pane * {line} + 1 }}\n"))
        .collect::<String>();
    let (editor, cx) = cx.add_window_view({
        let source = source.clone();
        move |window, cx| {
            CodeEditorState::new(window, cx)
                .language("rust")
                .default_value(&source)
        }
    });
    let middle = source
        .match_indices('\n')
        .nth(LINES / 2)
        .map(|(ix, _)| ix)
        .unwrap();

    let mut samples = Vec::with_capacity(ROUNDS);
    for _ in 0..ROUNDS {
        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                editor.set_selected_range(middle..middle, cx);
                for ix in 0..TYPED {
                    let character = char::from(b'a' + (ix % 26) as u8).to_string();
                    ala::EntityInputHandler::replace_text_in_range(
                        editor, None, &character, window, cx,
                    );
                }
            });
        });
        let elapsed = cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                let start = Instant::now();
                editor.undo(&Undo, window, cx);
                start.elapsed()
            })
        });
        samples.push(elapsed);
        editor.read_with(cx, |editor, _| assert_eq!(editor.value().as_ref(), source));
    }
    samples.sort();
    let median = samples[samples.len() / 2];
    let total: Duration = samples.iter().sum();
    println!(
        "PERF undo lines={LINES} typed={TYPED} rounds={ROUNDS} undo_median_us={} undo_min_us={} undo_max_us={} undo_total_us={}",
        median.as_micros(),
        samples[0].as_micros(),
        samples[samples.len() - 1].as_micros(),
        total.as_micros(),
    );
}
