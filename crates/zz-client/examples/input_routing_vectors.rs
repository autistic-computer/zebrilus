use std::error::Error;

use serde_json::{Value, json};
use zz_client::ClientCore;
use zz_protocol::{
    Event, EventPayload, InputMessage, MuxOptionKey, MuxOptionSource, MuxOptionValue, MuxOptions,
    PaneId, ProtocolMessage, encode_protocol_message, input_key_name,
};
use zz_terminal::{KeyAction, KeyCode, KeyInput, Modifiers};

fn event(payload: EventPayload) -> ProtocolMessage {
    ProtocolMessage::Event(Event {
        sequence: 7,
        payload,
    })
}

fn queries() -> Vec<KeyInput> {
    let mut values = Vec::new();
    for action in [KeyAction::Press, KeyAction::Repeat, KeyAction::Release] {
        for bits in 0..16 {
            for key in [
                KeyCode::Character('a'),
                KeyCode::Character('b'),
                KeyCode::Character(' '),
                KeyCode::Enter,
                KeyCode::Tab,
                KeyCode::ArrowLeft,
                KeyCode::Function(24),
                KeyCode::User(u16::MAX),
                KeyCode::Unidentified,
            ] {
                values.push(KeyInput {
                    action,
                    key,
                    modifiers: Modifiers::from_bits(bits).expect("four fixture flags"),
                    text: Some("é".into()),
                    unshifted_codepoint: None,
                });
            }
        }
    }
    values
}

fn case(name: &str, events: Vec<ProtocolMessage>) -> Result<Value, Box<dyn Error>> {
    let mut core = ClientCore::new();
    let mut encoded = Vec::new();
    for event in events {
        encoded.push(encode_protocol_message(&event)?);
        core.handle_message(event);
    }
    let queries = queries();
    let names = queries
        .iter()
        .map(|input| input_key_name(input).to_string())
        .collect::<Vec<_>>();
    let claims = queries
        .iter()
        .map(|input| core.claims_prefix_input(input))
        .collect::<Vec<_>>();
    let queries = queries
        .into_iter()
        .map(|input| {
            encode_protocol_message(&ProtocolMessage::Input(InputMessage::Key {
                pane: PaneId(7),
                input,
                text_follows: false,
            }))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let table = core.key_table();
    Ok(
        json!({"name":name,"events":encoded,"queries":queries,"armed":core.prefix_armed(),"table":table.map(|(name,_)|name),"repeat":table.is_some_and(|(_,repeat)|repeat),"names":names,"claims":claims}),
    )
}

fn main() -> Result<(), Box<dyn Error>> {
    let patch = |prefix: &str, prefix2: &str| {
        event(EventPayload::MuxOptionsPatched {
            options: MuxOptions::from_entries([
                (
                    MuxOptionKey::Prefix,
                    MuxOptionValue {
                        value: prefix.to_owned(),
                        source: MuxOptionSource::RuntimeCommand,
                    },
                ),
                (
                    MuxOptionKey::Prefix2,
                    MuxOptionValue {
                        value: prefix2.to_owned(),
                        source: MuxOptionSource::Override,
                    },
                ),
            ]),
        })
    };
    let cases = vec![
        case("default", vec![])?,
        case(
            "armed",
            vec![event(EventPayload::PrefixArmed { armed: true })],
        )?,
        case(
            "cancel-ack-keeps-armed",
            vec![
                event(EventPayload::PrefixArmed { armed: true }),
                event(EventPayload::PrefixCancelled {
                    request_id: u64::MAX,
                }),
            ],
        )?,
        case(
            "disarmed",
            vec![
                event(EventPayload::PrefixArmed { armed: true }),
                event(EventPayload::PrefixArmed { armed: false }),
            ],
        )?,
        case(
            "active-table",
            vec![event(EventPayload::KeyTableActive {
                table: Some("resize".to_owned()),
                repeat: false,
            })],
        )?,
        case(
            "repeat-table",
            vec![event(EventPayload::KeyTableActive {
                table: Some("prefix".to_owned()),
                repeat: true,
            })],
        )?,
        case(
            "cleared-table",
            vec![
                event(EventPayload::KeyTableActive {
                    table: Some("resize".to_owned()),
                    repeat: true,
                }),
                event(EventPayload::KeyTableActive {
                    table: None,
                    repeat: false,
                }),
            ],
        )?,
        case(
            "cleared-table-repeat-ignored",
            vec![event(EventPayload::KeyTableActive {
                table: None,
                repeat: true,
            })],
        )?,
        case("patched-prefixes", vec![patch("a", "Alt-Ctrl-Space")])?,
        case("disabled-prefixes", vec![patch("none", "NONE")])?,
        case(
            "full-options-replace-patch",
            vec![
                patch("a", "M-b"),
                event(EventPayload::MuxOptionsChanged {
                    options: MuxOptions::default(),
                }),
            ],
        )?,
        case(
            "prefix-canonical-order",
            vec![patch("Alt-Ctrl-b", " Space ")],
        )?,
    ];
    println!(
        "{}",
        json!({"reference":"actual zz-client ClientCore routing reducer and claims_prefix_input","cases":cases})
    );
    Ok(())
}
