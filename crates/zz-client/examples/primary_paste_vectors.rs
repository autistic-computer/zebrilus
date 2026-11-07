use std::io;

use serde_json::json;
use zz_client::terminal_view::{PrimaryPasteState, should_paste_primary};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut cases = Vec::new();
    for output in [false, true] {
        for tracking in [false, true] {
            for shift in [false, true] {
                cases.push(json!({
                    "output": output,
                    "tracking": tracking,
                    "shift": shift,
                    "paste": should_paste_primary(output, tracking, shift),
                }));
            }
        }
    }
    let mut traces = Vec::new();
    for index in 0..9_u32.pow(4) {
        let mut state = PrimaryPasteState::default();
        let mut remaining = index;
        let mut steps = Vec::new();
        for _ in 0..4 {
            let op = remaining % 9;
            remaining /= 9;
            let handled = if op == 0 {
                state.release()
            } else {
                let flags = op - 1;
                state.press(should_paste_primary(
                    flags & 1 != 0,
                    flags & 2 != 0,
                    flags & 4 != 0,
                ))
            };
            steps.push(json!({"op": op, "handled": handled, "pressed": state.is_pressed()}));
        }
        traces.push(steps);
    }
    serde_json::to_writer(
        io::stdout().lock(),
        &json!({"policy": cases, "traces": traces}),
    )?;
    Ok(())
}
