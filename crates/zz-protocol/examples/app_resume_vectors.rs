use std::error::Error;

use serde_json::json;
use zz_protocol::{
    AttachOperation, ClientEnvironmentBlob, ClientHello, ClientInstanceId, ClientKind,
    CommandInvocation, ExecExit, ExecOutcome, ExecResume, ExecResumeKind, Hello, KeySubscription,
    PROTOCOL_VERSION, PreparedCommand, PreparedCommandResult, ProtocolMessage, RawText,
    ServerError, SourceSpan, Subscriptions, TreeSubscription, encode_protocol_message,
};

fn main() -> Result<(), Box<dyn Error>> {
    let mut rows = Vec::new();
    for index in 0..16 {
        let invocation = CommandInvocation::new(
            "new-session",
            [RawText::from_bytes(vec![0, 255, b'x']), "世界".into()],
        )
        .with_source(SourceSpan {
            source: "default-client-command".to_owned(),
            line: u32::MAX,
            column: 42,
        })
        .with_command_blocks([1]);
        let prepared = PreparedCommand {
            invocation,
            canonical_name: (index & 1 != 0).then(|| "new-session".to_owned()),
            alias_matched: index & 2 != 0,
            result: if index & 4 == 0 {
                PreparedCommandResult::Ready
            } else {
                PreparedCommandResult::Error(ServerError::CommandParse("actual error".to_owned()))
            },
        };
        let commands = vec![prepared];
        let exit = ExecExit {
            server_id: u64::MAX,
            outcome: ExecOutcome::Resume(ExecResume {
                kind: ExecResumeKind::NewSession,
                commands: commands.clone(),
            }),
        };
        let native_search = index & 8 != 0;
        let mut capabilities = vec![
            ClientHello::CLIENT_TERMINAL_CAPABILITY.to_owned(),
            ClientHello::CLIENT_UTF8_CAPABILITY.to_owned(),
            "control-plane-v2".to_owned(),
        ];
        if native_search {
            capabilities.push("client-native-terminal-search-v1".to_owned());
        }
        let hello = Hello {
            client: ClientHello {
                protocol_version: PROTOCOL_VERSION,
                client_instance_id: ClientInstanceId(u64::MAX),
                kind: ClientKind::Interactive,
                device_name: Some("zz Mojo".to_owned()),
                capabilities,
                color_scheme: None,
                origin: None,
                working_directory: None,
                environment: Vec::new(),
                process_id: u32::MAX,
            },
            viewport: None,
            subscriptions: Subscriptions {
                tree: TreeSubscription::All,
                status: true,
                options: (1 << 21) - 1,
                keys: KeySubscription::None,
                pane_stream: true,
            },
            attach: Some(AttachOperation::Commands(commands)),
            environment: ClientEnvironmentBlob::from_bytes(Vec::new()),
        };
        rows.push(json!({
            "exit": encode_protocol_message(&ProtocolMessage::ExecExit(exit))?,
            "hello": encode_protocol_message(&ProtocolMessage::Hello(hello))?,
            "search": native_search,
        }));
    }
    println!("{}", serde_json::to_string(&rows)?);
    Ok(())
}
