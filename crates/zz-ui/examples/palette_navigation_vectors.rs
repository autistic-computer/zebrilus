use std::io::{self, Write};

use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
struct Case {
    rows: Vec<(u8, bool, u32, u8, u32)>,
    current: Option<u32>,
    selected: Option<usize>,
    direction: isize,
}

#[derive(Deserialize)]
struct Request {
    cases: Vec<Case>,
    layers: Vec<(u8, usize)>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let request: Request = serde_json::from_reader(io::stdin().lock())?;
    let output: Vec<Value> = request
        .cases
        .into_iter()
        .map(|case| {
            let result = zz_ui::command::palette_row_navigation(
                &case.rows,
                case.current,
                case.selected,
                case.direction,
            );
            json!({
                "initial": result.initial,
                "next": result.next,
                "parents": result.parents,
                "children": result.children,
            })
        })
        .collect();
    let layers: Vec<_> = request
        .layers
        .into_iter()
        .map(|(bits, count)| zz_ui::command::palette_layer_pop(bits, count))
        .collect();
    let mut stdout = io::BufWriter::new(io::stdout().lock());
    serde_json::to_writer(&mut stdout, &json!({"cases": output, "layers": layers}))?;
    stdout.write_all(b"\n")?;
    Ok(())
}
