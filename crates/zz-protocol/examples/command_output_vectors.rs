use std::error::Error;

use serde_json::{Value, json};
use zz_protocol::{InputMessage, PaneId, ProtocolMessage, encode_protocol_message};
use zz_terminal::{ClipboardTarget, TerminalViewAction};

fn case(name: &str, args: &Value, input: InputMessage) -> Result<Value, Box<dyn Error>> {
    Ok(
        json!({"name":name,"args":args,"bytes":encode_protocol_message(&ProtocolMessage::Input(input))?}),
    )
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut cases = Vec::new();
    for (columns, rows, cell_width_px, cell_height_px) in [
        (0, 0, 0, 0),
        (1, 1, 1, 1),
        (80, 24, 16, 36),
        (u16::MAX, u16::MAX, u32::MAX, u32::MAX),
    ] {
        cases.push(case(
            &format!("resize-{columns}-{rows}"),
            &json!([6, columns, rows, cell_width_px, cell_height_px]),
            InputMessage::ResizeCommandOutput {
                columns,
                rows,
                cell_width_px,
                cell_height_px,
            },
        )?);
    }
    for output in [false, true] {
        let pane = PaneId(u64::MAX);
        let view = |action| {
            if output {
                InputMessage::CommandOutputView { action }
            } else {
                InputMessage::TerminalView { pane, action }
            }
        };
        for focused in [false, true] {
            cases.push(case(
                &format!("focus-{output}-{focused}"),
                &json!([23, output, focused]),
                view(TerminalViewAction::Focus(focused)),
            )?);
        }
        for value in [0, u32::MAX] {
            cases.push(case(
                &format!("offset-{output}-{value}"),
                &json!([25, output, value]),
                view(TerminalViewAction::ScrollToOffset(value)),
            )?);
        }
        for target in [ClipboardTarget::Clipboard, ClipboardTarget::Primary] {
            for request_id in [0, u64::MAX] {
                cases.push(case(
                    &format!("copy-{output}-{target:?}-{request_id}"),
                    &json!([22, output, request_id, target as u8]),
                    view(TerminalViewAction::CopySelection { request_id, target }),
                )?);
            }
        }
        for text in ["", "q", "é", "e\u{301}", "世界", "a\nb", "\r", "🦀"] {
            cases.push(case(
                &format!("paste-{output}-{text:?}"),
                &json!([24, output, text]),
                view(TerminalViewAction::Paste(text.into())),
            )?);
        }
    }
    for text in ["q", "é", "e\u{301}", "世界", "a\nb", "\r", "🦀"] {
        for composed in [false, true] {
            let paste = !composed && (text.contains(['\n', '\r']) || text.chars().count() > 1);
            let input = if paste {
                InputMessage::CommandOutputView {
                    action: TerminalViewAction::Paste(text.into()),
                }
            } else {
                InputMessage::Text {
                    pane: PaneId(7),
                    text: text.into(),
                }
            };
            cases.push(case(
                &format!("commit-{composed}-{text:?}"),
                &json!([0, composed, text]),
                input,
            )?);
        }
    }
    println!(
        "{}",
        json!({"source":"actual native command-output serde plus clients/app TerminalPane commit classification","cases":cases})
    );
    Ok(())
}
