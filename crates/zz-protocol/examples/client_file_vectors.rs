use std::{error::Error, fs, path::Path};

use serde_json::{Value, json};
use zz_protocol::{
    ClientFileOperation, ClientFileRequest, ClientFileResponse, CommandInvocation, CommandRequest,
    CommandResponse, ProtocolMessage, RawText, ServerError, decode_protocol_frame,
    encode_protocol_message,
};

fn raw_fixture(bytes: &[u8], kind: &str) -> Result<Value, Box<dyn Error>> {
    let canonical = decode_protocol_frame(bytes)
        .ok()
        .map(|message| encode_protocol_message(&message))
        .transpose()?;
    Ok(json!({"kind":kind,"bytes":bytes,"canonical":canonical,"accepted":canonical.is_some()}))
}

fn fixture(message: &ProtocolMessage, kind: &str) -> Result<Value, Box<dyn Error>> {
    raw_fixture(&unchecked_frame(message)?, kind)
}

fn unchecked_frame(message: &ProtocolMessage) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut bytes = encode_protocol_message(&ProtocolMessage::TreeSync)?;
    bytes.truncate(8);
    bytes.extend(postcard::to_stdvec(message)?);
    let length = u32::try_from(bytes.len() - 4)?;
    bytes[..4].copy_from_slice(&length.to_le_bytes());
    Ok(bytes)
}

fn large_fixtures(directory: &Path) -> Result<(), Box<dyn Error>> {
    let mut cases = Vec::new();
    for length in [32 * 1024 * 1024, 32 * 1024 * 1024 + 1] {
        for kind in ["request", "response"] {
            let data = vec![128; length];
            let message = if kind == "request" {
                ProtocolMessage::ClientFileRequest(ClientFileRequest {
                    request_id: u64::MAX,
                    path: "bounded".to_owned(),
                    operation: ClientFileOperation::Write { append: true, data },
                })
            } else {
                ProtocolMessage::ClientFileResponse(ClientFileResponse {
                    request_id: u64::MAX,
                    data,
                    error: None,
                })
            };
            let frame = unchecked_frame(&message)?;
            let decoded = decode_protocol_frame(&frame);
            let accepted = decoded.is_ok();
            if let Ok(decoded) = decoded {
                let canonical = encode_protocol_message(&decoded)?;
                if canonical != frame {
                    return Err("large source fixture changed during roundtrip".into());
                }
            }
            let name = format!("{kind}-{length}.bin");
            fs::write(directory.join(&name), frame)?;
            cases.push(json!({"kind":kind,"path":name,"accepted":accepted}));
        }
    }
    println!("{}", serde_json::to_string(&cases)?);
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let arguments: Vec<_> = std::env::args_os().collect();
    if arguments.len() == 3 && arguments[1] == "--large" {
        return large_fixtures(Path::new(&arguments[2]));
    }
    let mut cases = Vec::new();
    for error in [
        ServerError::ProtocolMismatch {
            client: 117,
            server: 118,
        },
        ServerError::MissingTarget("%7".to_owned()),
        ServerError::AmbiguousTarget("世界".to_owned()),
        ServerError::InvalidTarget("bad".to_owned()),
        ServerError::UnsupportedCommand("unknown".to_owned()),
        ServerError::InvalidCommand("file 世界\nerror".to_owned()),
        ServerError::PaneNotAttached(zz_protocol::PaneId(u64::MAX)),
        ServerError::PaneExited(zz_protocol::PaneId(0)),
        ServerError::Internal("internal".to_owned()),
        ServerError::SessionNotFound("missing".to_owned()),
        ServerError::WindowNotFound("missing".to_owned()),
        ServerError::PaneNotFound("missing".to_owned()),
        ServerError::CommandParse("parse".to_owned()),
        ServerError::PostAdmissionCallback(Box::new(ServerError::NativeCommandParse(
            "nested".to_owned(),
        ))),
        ServerError::NativeCommandParse("native parse".to_owned()),
        ServerError::NativeUnsupportedCommand("native unknown".to_owned()),
        ServerError::NativeInvalidCommand("native bad".to_owned()),
    ] {
        let message = error.tmux_message();
        let code = error.exit_code();
        let mut case = fixture(
            &ProtocolMessage::CommandResponse(CommandResponse::Error {
                request_id: u64::MAX,
                error,
                output: RawText::default(),
            }),
            "cli-error",
        )?;
        case["error"] = json!(message);
        case["code"] = json!(code);
        cases.push(case);
    }
    for request in [0, 127, 128, u64::MAX] {
        let mut command = CommandInvocation::new("load-buffer", ["-b", "世界", "-"]);
        command.set_stdin_available(true);
        cases.push(fixture(
            &ProtocolMessage::CommandRequest(CommandRequest {
                request_id: request,
                command,
                prepared: false,
            }),
            "command",
        )?);
        for path in [
            String::new(),
            "a space/世界\n".to_owned(),
            "embedded\0null".to_owned(),
        ] {
            for operation in [
                ClientFileOperation::Read,
                ClientFileOperation::Write {
                    append: false,
                    data: vec![0, 255, 1, 128],
                },
                ClientFileOperation::Write {
                    append: true,
                    data: Vec::new(),
                },
                ClientFileOperation::ReadStdin { binary: false },
                ClientFileOperation::ReadStdin { binary: true },
                ClientFileOperation::ReadStdinChunk,
            ] {
                cases.push(fixture(
                    &ProtocolMessage::ClientFileRequest(ClientFileRequest {
                        request_id: request,
                        path: path.clone(),
                        operation,
                    }),
                    "request",
                )?);
            }
        }
        for error in [
            None,
            Some(String::new()),
            Some("Bad file descriptor".to_owned()),
            Some("error 世界\n".to_owned()),
        ] {
            cases.push(fixture(
                &ProtocolMessage::ClientFileResponse(ClientFileResponse {
                    request_id: request,
                    data: vec![0, 255, 128, 10],
                    error,
                }),
                "response",
            )?);
        }
    }
    for size in [16_384, 16_385] {
        cases.push(fixture(
            &ProtocolMessage::ClientFileRequest(ClientFileRequest {
                request_id: 1,
                path: "x".repeat(size),
                operation: ClientFileOperation::Read,
            }),
            "request",
        )?);
    }
    for size in [65_536, 65_537] {
        cases.push(fixture(
            &ProtocolMessage::ClientFileResponse(ClientFileResponse {
                request_id: 1,
                data: Vec::new(),
                error: Some("e".repeat(size)),
            }),
            "response",
        )?);
    }
    let base: Vec<u8> = serde_json::from_value(cases[18]["bytes"].clone())?;
    for cut in 8..base.len() {
        let mut bytes = base[..cut].to_vec();
        let length = u32::try_from(bytes.len() - 4)?;
        bytes[..4].copy_from_slice(&length.to_le_bytes());
        cases.push(raw_fixture(&bytes, "request")?);
    }
    let mut extended = base;
    extended.extend_from_slice(&[0, 255, 128]);
    let length = u32::try_from(extended.len() - 4)?;
    extended[..4].copy_from_slice(&length.to_le_bytes());
    cases.push(raw_fixture(&extended, "request")?);
    for payload in [
        vec![34, 0, 0, 4],
        vec![34, 0, 0, 1, 2, 0],
        vec![34, 0, 0, 2, 2],
        vec![34, 0, 1, 255, 0],
        vec![35, 0, 0, 2],
        vec![35, 0, 0, 1, 1, 255],
    ] {
        let mut bytes = encode_protocol_message(&ProtocolMessage::TreeSync)?;
        bytes.truncate(8);
        bytes.extend(payload);
        let length = u32::try_from(bytes.len() - 4)?;
        bytes[..4].copy_from_slice(&length.to_le_bytes());
        let kind = if bytes[8] == 34 {
            "request"
        } else {
            "response"
        };
        cases.push(raw_fixture(&bytes, kind)?);
    }
    println!("{}", serde_json::to_string(&cases)?);
    Ok(())
}
