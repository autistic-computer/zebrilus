use std::{error::Error, fs, hash::Hasher};

use zz_client::ClientCore;
use zz_protocol::{
    ClientId, ClientInstanceId, CommandInvocation, Event, EventPayload, KeyBindingSnapshot,
    KeyTableSnapshot, MouseBindings, PROTOCOL_VERSION, ProtocolMessage, Welcome,
    encode_protocol_message,
};

#[derive(Default)]
struct Packet(Vec<u8>);

impl Packet {
    fn uint(&mut self, mut value: u64) {
        while value >= 128 {
            self.0.push((value as u8 & 127) | 128);
            value >>= 7;
        }
        self.0.push(value as u8);
    }

    fn bytes(&mut self, bytes: &[u8]) {
        self.uint(bytes.len() as u64);
        self.0.extend_from_slice(bytes);
    }
}

fn table(name: &str, index: usize) -> KeyTableSnapshot {
    let keys = [
        "x",
        "MouseDown1Pane",
        "C-M-S-WheelUpControl+255",
        "M-C-MouseDragEnd3StatusLeft",
        "Ctrl-MouseDown1Pane",
        "WheelDownControl256",
        "WheelUpControl000",
        "S-MouseUp2Empty",
    ];
    KeyTableSnapshot {
        name: name.into(),
        bindings: vec![KeyBindingSnapshot {
            key: keys[index % keys.len()].into(),
            commands: vec![CommandInvocation::new("list-keys", ["-N"])],
            repeat: index.is_multiple_of(2),
            note: Some(format!("note {index}")),
        }],
    }
}

fn event(payload: EventPayload) -> ProtocolMessage {
    ProtocolMessage::Event(Event {
        sequence: 0,
        payload,
    })
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut packet = Packet::default();
    let get = encode_protocol_message(&ProtocolMessage::GetKeyTables)?;
    packet.bytes(&get);
    packet.bytes(&encode_protocol_message(&ProtocolMessage::Welcome(
        Welcome {
            protocol_version: PROTOCOL_VERSION,
            server_id: 1,
            client_id: ClientId(2),
            client_instance_id: ClientInstanceId(3),
            caps: (1 << 29) | (1 << 30),
            clock: 4,
        },
    ))?);
    packet.uint(129);
    for length in 0..129 {
        let bytes: Vec<_> = (0..length).map(|index| (index * 37) as u8).collect();
        let mut hasher = std::hash::DefaultHasher::new();
        hasher.write(&bytes);
        packet.bytes(&bytes);
        packet.uint(hasher.finish());
    }
    let mut messages = Vec::new();
    for names in [
        vec!["z", "prefix", "prefix", "a", "root", "copy-mode"],
        vec!["z", "a", "m", "prefix", "b"],
        vec!["prefix", "root", "copy-mode-vi"],
    ] {
        messages.push(event(EventPayload::KeyTablesChanged {
            tables: names
                .iter()
                .enumerate()
                .map(|(index, name)| table(name, index))
                .collect(),
        }));
        for index in 0..80 {
            let name = ["root", "prefix", "a", "x", "z", "copy-mode", "copy-mode-vi"][index % 7];
            let incoming = if index % 9 == 0 {
                vec![table(name, index), table(name, index + 1)]
            } else {
                vec![table(name, index)]
            };
            messages.push(event(EventPayload::KeyTablesPatched {
                tables: incoming,
                removed: if index % 5 == 0 {
                    vec![name.into()]
                } else {
                    Vec::new()
                },
            }));
            if index % 11 == 0 {
                messages.push(event(EventPayload::KeyTableActive {
                    table: Some("prefix".into()),
                    repeat: index.is_multiple_of(2),
                }));
            }
            if index % 19 == 0 {
                messages.push(event(EventPayload::KeyTablesHashChanged {
                    hash: u64::MAX - index as u64,
                    mouse: MouseBindings::from_tables(&[
                        table("root", index),
                        table("copy-mode", index + 1),
                    ]),
                }));
            }
        }
    }
    packet.uint(messages.len() as u64);
    let mut core = ClientCore::new();
    for message in messages {
        packet.bytes(&encode_protocol_message(&message)?);
        core.handle_message(message);
        packet.bytes(&encode_protocol_message(&event(
            EventPayload::KeyTablesChanged {
                tables: core.key_tables().to_vec(),
            },
        ))?);
        packet.bytes(&encode_protocol_message(&event(
            EventPayload::KeyTablesHashChanged {
                hash: core.key_tables_hash(),
                mouse: core.mouse_bindings().clone(),
            },
        ))?);
        packet.uint(u64::from(core.key_table().is_some()));
        if let Some((name, repeat)) = core.key_table() {
            packet.bytes(name.as_bytes());
            packet.uint(u64::from(repeat));
        }
    }
    fs::write(
        std::env::args_os().nth(1).ok_or("expected output path")?,
        packet.0,
    )?;
    Ok(())
}
