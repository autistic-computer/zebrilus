use super::*;

pub(super) fn compact_hello(kind: ClientKind) -> zz_protocol::Hello {
    zz_protocol::Hello::from_client(ClientHello {
        protocol_version: PROTOCOL_VERSION,
        client_instance_id: ClientInstanceId(919),
        kind,
        device_name: None,
        capabilities: vec![
            zz_protocol::PANE_FRAME_CAPABILITY.to_owned(),
            ClientHello::CLIENT_TERMINAL_CAPABILITY.to_owned(),
        ],
        color_scheme: None,
        origin: None,
        working_directory: None,
        environment: Vec::new(),
        process_id: std::process::id(),
        revision: zz_protocol::REVISION,
    })
}

pub(super) fn compact_command(
    shared: &Arc<Shared>,
    context: &mut ExecutionContext,
    name: &str,
    args: &[&str],
) {
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            context,
            &CommandInvocation::new(name, args.iter().copied()),
        )
        .expect(name);
}

pub(super) fn compact_registered(
    shared: &Arc<Shared>,
    subscriptions: zz_protocol::Subscriptions,
) -> (ClientId, Arc<OutboundMailbox>) {
    let mut hello = compact_hello(ClientKind::Interactive);
    hello.subscriptions = subscriptions;
    let (client, _) = shared.register_welcome(&hello).expect("register");
    let mailbox = OutboundMailbox::new();
    shared.subscribe(client, Arc::clone(&mailbox));
    if let Some(client) = shared.inner.lock().client_mut(client) {
        client.ctrl_initializing = false;
    }
    (client, mailbox)
}

pub(super) fn reliable_children(mailbox: &OutboundMailbox) -> Vec<ProtocolMessage> {
    tests::take_reliable_messages(mailbox)
        .into_iter()
        .flat_map(|message| match message {
            ProtocolMessage::Batch(batch) => batch.messages().expect("children"),
            message => vec![message],
        })
        .collect()
}

pub(super) fn bounded_reliable_children(
    mailbox: &OutboundMailbox,
    bound: usize,
) -> Vec<ProtocolMessage> {
    let envelopes = tests::take_reliable_messages(mailbox);
    let bytes = envelopes
        .iter()
        .map(|message| {
            zz_protocol::encode_protocol_message(message)
                .expect("encode")
                .len()
        })
        .sum::<usize>();
    eprintln!("CTRL publication total_wire_bytes={bytes} bound={bound}");
    assert!(
        bytes <= bound,
        "publication {bytes} bytes including every batch and view"
    );
    envelopes
        .into_iter()
        .flat_map(|message| match message {
            ProtocolMessage::Batch(batch) => batch.messages().expect("children"),
            message => vec![message],
        })
        .collect()
}

pub(super) fn compact_exec_request(commands: Vec<CommandInvocation>) -> zz_protocol::ExecRequest {
    zz_protocol::ExecRequest {
        protocol_version: PROTOCOL_VERSION,
        flags: zz_protocol::ExecFlags::default(),
        client_instance_id: ClientInstanceId(919),
        origin: None,
        working_directory: None,
        tty: None,
        size: None,
        features: 0,
        startup_reentry: None,
        spawned_server_id: None,
        expect_server_id: None,
        process_id: std::process::id(),
        environment: ClientEnvironmentBlob::default(),
        commands,
        raw_control_line: None,
    }
}
