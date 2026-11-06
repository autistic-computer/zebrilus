use std::{error::Error, ffi::OsString, os::unix::ffi::OsStringExt as _, path::PathBuf};

use serde_json::{Value, json};
use zz_protocol::{
    AttachOperation, ClientHello, ClientInstanceId, ClientKind, ClientPath, Hello, KeySubscription,
    PROTOCOL_VERSION, PaneId, ProtocolMessage, RawText, Subscriptions, TreeSubscription,
    encode_protocol_message,
};

fn fixture(cwd: Option<&[u8]>, environment: &[Vec<u8>]) -> Value {
    let client = ClientHello {
        protocol_version: PROTOCOL_VERSION,
        client_instance_id: ClientInstanceId(u64::MAX),
        kind: ClientKind::Command,
        device_name: Some("zz Mojo".to_owned()),
        capabilities: vec![ClientHello::CLIENT_UTF8_CAPABILITY.to_owned()],
        color_scheme: None,
        origin: Some(PaneId(u64::MAX)),
        working_directory: cwd
            .map(<[u8]>::to_vec)
            .map(OsString::from_vec)
            .map(PathBuf::from)
            .as_deref()
            .and_then(ClientPath::from_path),
        environment: environment
            .iter()
            .cloned()
            .map(RawText::from_bytes)
            .collect(),
        process_id: u32::MAX,
    };
    let command = encode_protocol_message(&ProtocolMessage::ClientHello(client.clone()));
    let mut interactive = client;
    interactive.kind = ClientKind::Interactive;
    interactive.capabilities = vec![
        ClientHello::CLIENT_TERMINAL_CAPABILITY.to_owned(),
        ClientHello::CLIENT_UTF8_CAPABILITY.to_owned(),
        "control-plane-v2".to_owned(),
    ];
    let mut hello = Hello::from_client(interactive);
    hello.subscriptions = Subscriptions {
        tree: TreeSubscription::All,
        status: true,
        options: (1 << 21) - 1,
        keys: KeySubscription::None,
        pane_stream: true,
    };
    hello.attach = Some(AttachOperation::Session("$7".to_owned()));
    let interactive = encode_protocol_message(&ProtocolMessage::Hello(hello));
    json!({
        "cwd": cwd,
        "environment": environment,
        "command": command.as_ref().ok(),
        "interactive": interactive.as_ref().ok(),
        "command_rejected": command.is_err(),
        "interactive_rejected": interactive.is_err(),
    })
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut cases = vec![
        fixture(None, &[]),
        fixture(Some(&[]), &[b"A=".to_vec()]),
        fixture(
            Some("/tmp/a space/世界".as_bytes()),
            &[b"A=1".to_vec(), "LANG=世界".as_bytes().to_vec()],
        ),
        fixture(
            Some(b"/tmp/\xff"),
            &[b"A=\xff".to_vec(), b"\xff=raw".to_vec(), b"EQ=a=b".to_vec()],
        ),
        fixture(Some(&vec![b'x'; 16_384]), &[]),
        fixture(None, &[b"A=first".to_vec(), b"A=last".to_vec()]),
        fixture(Some(&vec![b'x'; 16_385]), &[]),
        fixture(None, &[b"NO_SEPARATOR".to_vec()]),
        fixture(None, &[b"=empty-name".to_vec()]),
        fixture(None, &[b"A=embedded\0null".to_vec()]),
    ];
    let mut oversized = vec![b'x'; 16_368];
    oversized[1] = b'=';
    cases.push(fixture(None, &[oversized]));
    println!("{}", serde_json::to_string(&cases)?);
    Ok(())
}
