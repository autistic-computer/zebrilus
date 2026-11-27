use super::test_support::{drain, event};
use super::*;

#[test]
fn a_terminal_negotiation_is_kept_for_the_terminal_client() {
    let mut core = ClientCore::new();
    assert!(core.terminal_negotiation().is_none());
    core.handle_message(event(EventPayload::TerminalNegotiation {
        features: vec!["RGB".to_owned()],
        user_keys: vec!["\x1b[99~".to_owned()],
        application_escape: Vec::new(),
    }));
    assert_eq!(drain(&mut core), [CoreEvent::MuxOptionsChanged]);
    assert_eq!(
        core.terminal_negotiation(),
        Some(&(
            vec!["RGB".to_owned()],
            vec!["\x1b[99~".to_owned()],
            Vec::new()
        ))
    );
}

#[test]
fn selected_option_patch_preserves_options_outside_subscription() {
    let mut core = ClientCore::new();
    let patch = MuxOptions::from_entries([(
        zz_protocol::MuxOptionKey::ExtendedKeys,
        zz_protocol::MuxOptionValue {
            value: "always".to_owned(),
            source: zz_protocol::MuxOptionSource::RuntimeCommand,
        },
    )]);
    core.handle_message(event(EventPayload::MuxOptionsPatched { options: patch }));
    assert_eq!(
        core.mux_options()
            .get(zz_protocol::MuxOptionKey::ExtendedKeys)
            .unwrap()
            .value,
        "always"
    );
    assert_eq!(
        core.mux_options()
            .get(zz_protocol::MuxOptionKey::Prefix)
            .unwrap()
            .value,
        "C-b"
    );
}
