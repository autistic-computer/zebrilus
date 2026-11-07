use serde_json::json;
use zz_client::terminal_view::mode_text;
use zz_terminal::TerminalMode;

fn main() {
    let mut cases = Vec::new();
    for kind in 0..3 {
        for position in [0, 7, u32::MAX] {
            for total in [0, 29, u32::MAX] {
                for hide in [false, true] {
                    for unseen in [0, 31, u32::MAX] {
                        let mode = match kind {
                            0 => TerminalMode::Live,
                            1 => TerminalMode::Copy {
                                position,
                                total,
                                hide_position: hide,
                            },
                            _ => TerminalMode::View { position, total },
                        };
                        cases.push(json!({"args":[kind,position,total,hide,unseen],"expected":mode_text(mode, unseen)}));
                    }
                }
            }
        }
    }
    println!(
        "{}",
        json!({"source":"actual zz-client terminal_view::mode_text","cases":cases})
    );
}
