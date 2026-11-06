use std::error::Error;

use serde_json::{Value, json};
use zz_protocol::{
    ClientHello, ClientInstanceId, ClientKind, CommandInvocation, CommandRequest, CommandResponse,
    PROTOCOL_VERSION, PaneId, ProtocolMessage, RawText, ServerError, StdoutClaim,
    decode_protocol_frame, encode_protocol_message,
};

fn encoded(message: &ProtocolMessage) -> Result<Value, Box<dyn Error>> {
    Ok(json!(encode_protocol_message(message)?))
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut hello = Vec::new();
    for (instance, process, origin) in [
        (0, 0, None),
        (7, 42, Some(11)),
        (u64::MAX, u32::MAX, Some(u64::MAX)),
    ] {
        let bytes = encoded(&ProtocolMessage::ClientHello(ClientHello {
            protocol_version: PROTOCOL_VERSION,
            client_instance_id: ClientInstanceId(instance),
            kind: ClientKind::Command,
            device_name: Some("zz Mojo".to_owned()),
            capabilities: vec![ClientHello::CLIENT_UTF8_CAPABILITY.to_owned()],
            color_scheme: None,
            origin: origin.map(PaneId),
            working_directory: None,
            environment: Vec::new(),
            process_id: process,
        }))?;
        hello.push(json!({"instance": instance.to_string(), "process": process, "origin": origin.map(|id| id.to_string()), "bytes": bytes}));
    }
    let mut requests = Vec::new();
    for (request, name, args) in [
        (0, "list-sessions", Vec::new()),
        (127, "display-message", vec!["-p", "hello\t世界\n"]),
        (
            128,
            "send-keys",
            vec!["-t", "%18446744073709551615", "-l", "a'b\\c"],
        ),
        (
            u64::MAX,
            "list-panes",
            vec!["-a", "-F", "#{pane_id}\t#{pane_title}"],
        ),
    ] {
        let bytes = encoded(&ProtocolMessage::CommandRequest(CommandRequest {
            request_id: request,
            command: CommandInvocation::new(name, args.clone()),
            prepared: false,
        }))?;
        requests.push(
            json!({"request": request.to_string(), "name": name, "args": args, "bytes": bytes}),
        );
    }
    let mut responses = Vec::new();
    for (claim, stdout_claim) in [
        (0, StdoutClaim::None),
        (1, StdoutClaim::Print),
        (2, StdoutClaim::Raw),
    ] {
        let bytes = encoded(&ProtocolMessage::CommandResponse(
            CommandResponse::Success {
                request_id: 128,
                output: RawText::from_bytes(vec![0, 255, 10, 65]),
                exit_code: 7,
                stderr: "warning\n世界".to_owned(),
                stdout_claim,
            },
        ))?;
        responses.push(json!({"request": "128", "output": [0, 255, 10, 65], "exit_code": 7, "stderr": "warning\n世界", "claim": claim, "bytes": bytes}));
    }
    for error in [
        ServerError::ProtocolMismatch {
            client: 117,
            server: 118,
        },
        ServerError::MissingTarget("%3".to_owned()),
        ServerError::AmbiguousTarget("session".to_owned()),
        ServerError::InvalidTarget("bad".to_owned()),
        ServerError::UnsupportedCommand("missing".to_owned()),
        ServerError::InvalidCommand("bad".to_owned()),
    ] {
        let message = error.to_string();
        let bytes = encoded(&ProtocolMessage::CommandResponse(CommandResponse::Error {
            request_id: u64::MAX,
            error,
            output: RawText::from_bytes(vec![0, 255]),
        }))?;
        responses.push(json!({"request": u64::MAX.to_string(), "output": [0, 255], "exit_code": 1, "stderr": message, "claim": 0, "bytes": bytes}));
    }
    let mut extensions = Vec::new();
    for response in &responses {
        let mut extended = response.clone();
        let original: Vec<u8> = serde_json::from_value(response["bytes"].clone())?;
        let mut bytes = original.clone();
        bytes.extend_from_slice(&[0, 255, 128]);
        let length = u32::try_from(bytes.len() - 4)?;
        bytes[..4].copy_from_slice(&length.to_le_bytes());
        let decoded = decode_protocol_frame(&bytes)?;
        if encode_protocol_message(&decoded)? != original {
            return Err("extended command response changed its decoded value".into());
        }
        extended["bytes"] = json!(bytes);
        extensions.push(extended);
    }
    responses.extend(extensions);
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"protocol_version": PROTOCOL_VERSION, "hello": hello, "requests": requests, "responses": responses})
        )?
    );
    Ok(())
}
