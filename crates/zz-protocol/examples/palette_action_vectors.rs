use std::error::Error;

use serde_json::json;
use zz_protocol::{
    ChooseTreeAction, CommandInvocation, CommandPromptAction, CommandRequest, InputMessage,
    ProtocolMessage, RawText, SourceSpan, decode_protocol_frame, encode_protocol_message,
};

fn command(name: &str, metadata: bool) -> CommandInvocation {
    let mut command = CommandInvocation::new(
        name,
        [
            RawText::from(""),
            RawText::from("λ quoted ' name\n"),
            RawText::from_bytes(vec![0, 255, 128]),
            RawText::from(";"),
        ],
    );
    if metadata {
        command = command
            .with_source(SourceSpan {
                source: "palette λ".to_owned(),
                line: u32::MAX,
                column: 0,
            })
            .with_command_blocks([1, 0])
            .into_expanded_alias_group();
        command.set_stdin(RawText::from_bytes(vec![0, 255, 10]));
        command.set_stdin_available(true);
    }
    command
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut cases = Vec::new();
    for request_id in [0, 127, 128, u64::MAX] {
        for name in [
            "new-session",
            "rename-session",
            "if-shell",
            "palette-name",
            "λ",
        ] {
            for metadata in [false, true] {
                let message = ProtocolMessage::CommandRequest(CommandRequest {
                    request_id,
                    command: command(name, metadata),
                    prepared: false,
                });
                let frame = encode_protocol_message(&message)?;
                assert_eq!(decode_protocol_frame(&frame)?, message);
                cases.push(json!({"kind":0,"frame":frame}));
            }
        }
        let frame = encode_protocol_message(&ProtocolMessage::Input(InputMessage::CancelPrefix {
            request_id,
        }))?;
        cases.push(json!({"kind":1,"request":request_id,"frame":frame}));
    }
    for (kind, input) in [
        (
            2,
            InputMessage::CommandPrompt {
                action: CommandPromptAction::Close,
            },
        ),
        (
            3,
            InputMessage::ChooseTree {
                action: ChooseTreeAction::Close,
            },
        ),
    ] {
        cases.push(
            json!({"kind":kind,"frame":encode_protocol_message(&ProtocolMessage::Input(input))?}),
        );
    }
    println!("{}", serde_json::to_string(&cases)?);
    Ok(())
}
