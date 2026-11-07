use std::error::Error;

use serde_json::json;
use zz_protocol::{
    ClientEnvironmentBlob, ClientHello, ClientInstanceId, ClientKind, Hello, KeySubscription,
    PROTOCOL_VERSION, PaneId, ProtocolMessage, Subscriptions, TreeSubscription,
    encode_protocol_message,
};

fn main() -> Result<(), Box<dyn Error>> {
    let mut cases = Vec::new();
    for search in [false, true] {
        for paths in [false, true] {
            for chooser in [false, true] {
                for origin in [None, Some(PaneId(99))] {
                    let mut capabilities = vec![
                        "client-terminal-v1".to_owned(),
                        "client-utf8-v1".to_owned(),
                        "control-plane-v2".to_owned(),
                    ];
                    if search {
                        capabilities.push("client-native-terminal-search-v1".to_owned());
                    }
                    if paths {
                        capabilities.push(ClientHello::CLIENT_PATH_PICKER_CAPABILITY.to_owned());
                    }
                    if chooser {
                        capabilities.push(ClientHello::CLIENT_NATIVE_CHOOSER_CAPABILITY.to_owned());
                    }
                    let hello = ProtocolMessage::Hello(Hello {
                        client: ClientHello {
                            protocol_version: PROTOCOL_VERSION,
                            client_instance_id: ClientInstanceId(42),
                            kind: ClientKind::Interactive,
                            device_name: Some("zz Mojo".to_owned()),
                            capabilities,
                            color_scheme: None,
                            origin,
                            working_directory: None,
                            environment: Vec::new(),
                            process_id: 7,
                        },
                        viewport: None,
                        subscriptions: Subscriptions {
                            tree: TreeSubscription::All,
                            status: true,
                            options: 31,
                            keys: KeySubscription::None,
                            pane_stream: true,
                        },
                        attach: None,
                        environment: ClientEnvironmentBlob::from_bytes(Vec::new()),
                    });
                    cases.push(json!({"search":search,"paths":paths,"chooser":chooser,"origin":origin.map(|pane|pane.0),"bytes":encode_protocol_message(&hello)?}));
                }
            }
        }
    }
    let output = std::env::args().nth(1).ok_or("missing output path")?;
    std::fs::write(output, serde_json::to_vec(&cases)?)?;
    Ok(())
}
