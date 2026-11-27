use super::test_support::{drain, event};
use super::*;

#[test]
fn patched_key_tables_replace_insert_and_remove_by_name() {
    let table = |name: &str, keys: &[&str]| KeyTableSnapshot {
        name: name.to_owned(),
        bindings: keys
            .iter()
            .map(|key| KeyBindingSnapshot {
                key: (*key).to_owned(),
                commands: Vec::new(),
                repeat: false,
                note: None,
            })
            .collect(),
    };
    let mut core = ClientCore::new();
    core.handle_message(event(EventPayload::KeyTablesChanged {
        tables: vec![
            table("copy-mode", &["q"]),
            table("prefix", &["c"]),
            table("root", &["F1"]),
        ],
    }));
    drain(&mut core);
    core.handle_message(event(EventPayload::KeyTablesPatched {
        tables: vec![table("resize", &["h"]), table("root", &["F2"])],
        removed: vec!["copy-mode".to_owned()],
    }));
    assert_eq!(drain(&mut core), vec![CoreEvent::KeyTablesChanged]);
    assert_eq!(
        core.key_tables(),
        [
            table("prefix", &["c"]),
            table("resize", &["h"]),
            table("root", &["F2"]),
        ]
    );
}

#[test]
fn key_table_active_is_stored_and_every_publication_emits() {
    let mut core = ClientCore::new();
    for (table, repeat) in [
        (Some("prefix"), false),
        (Some("prefix"), false),
        (Some("resize"), true),
        (None, false),
    ] {
        core.handle_message(ProtocolMessage::Event(Event {
            sequence: 0,
            payload: EventPayload::KeyTableActive {
                table: table.map(str::to_owned),
                repeat,
            },
        }));
        assert_eq!(core.key_table(), table.map(|table| (table, repeat)));
        assert_eq!(drain(&mut core), vec![CoreEvent::KeyTableChanged]);
    }
}

#[test]
fn an_active_key_table_claims_every_key_but_platform_chords() {
    let mut core = ClientCore::new();
    let key = |character: char, platform: bool| zz_terminal::KeyInput {
        action: zz_terminal::KeyAction::Press,
        key: zz_terminal::KeyCode::Character(character),
        modifiers: zz_terminal::Modifiers::new(false, false, false, platform),
        text: Some(character.to_string().into_boxed_str()),
        unshifted_codepoint: None,
    };
    let publish = |core: &mut ClientCore, table: Option<&str>, repeat: bool| {
        core.handle_message(ProtocolMessage::Event(Event {
            sequence: 0,
            payload: EventPayload::KeyTableActive {
                table: table.map(str::to_owned),
                repeat,
            },
        }));
    };
    assert!(!core.claims_prefix_input(&key('x', false)));
    publish(&mut core, Some("resize"), false);
    assert!(core.claims_prefix_input(&key('x', false)));
    assert!(!core.claims_prefix_input(&key('x', true)));
    publish(&mut core, Some("resize"), true);
    assert!(core.claims_prefix_input(&key('h', false)));
    publish(&mut core, None, false);
    assert!(!core.claims_prefix_input(&key('x', false)));
    publish(&mut core, Some("resize"), false);
    core.reset_session();
    assert!(!core.claims_prefix_input(&key('x', false)));
}

#[test]
fn prefix_claims_follow_both_live_options_and_armed_state() {
    let mut core = ClientCore::new();
    let set = |core: &mut ClientCore, key, value: &str| {
        core.mux_options
            .set(key, value, zz_protocol::MuxOptionSource::RuntimeCommand);
        core.refresh_prefix_keys();
    };
    set(&mut core, zz_protocol::MuxOptionKey::Prefix, "Ctrl-a");
    set(&mut core, zz_protocol::MuxOptionKey::Prefix2, "Alt-Space");
    let input = |character: char, control: bool, alt: bool| zz_terminal::KeyInput {
        action: zz_terminal::KeyAction::Press,
        key: zz_terminal::KeyCode::Character(character),
        modifiers: zz_terminal::Modifiers::new(false, control, alt, false),
        text: Some(character.to_string().into_boxed_str()),
        unshifted_codepoint: Some(character),
    };
    assert!(core.claims_prefix_input(&input('a', true, false)));
    assert!(core.claims_prefix_input(&input(' ', false, true)));
    assert!(!core.claims_prefix_input(&input('b', true, false)));
    assert!(!core.claims_prefix_input(&input('x', false, false)));
    core.prefix_armed = true;
    assert!(core.claims_prefix_input(&input('x', false, false)));
    core.prefix_armed = false;
    set(&mut core, zz_protocol::MuxOptionKey::Prefix2, "none");
    assert!(!core.claims_prefix_input(&input(' ', false, true)));
    set(&mut core, zz_protocol::MuxOptionKey::Prefix, "None");
    assert!(!core.claims_prefix_input(&input('a', true, false)));
}
