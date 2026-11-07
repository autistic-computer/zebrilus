use std::{collections::BTreeSet, error::Error, fs};

use zz_client::chooser::{
    buffer_row_text, chooser_has_key_gutter, chooser_key_label, chooser_modal_height,
    chooser_subtitle,
};
use zz_protocol::{
    ChooseBufferItem, ChooseBufferSearchState, ChooseBufferState, Event, EventPayload,
    ProtocolMessage, encode_protocol_message,
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

    fn bytes(&mut self, value: &[u8]) {
        self.uint(value.len() as u64);
        self.0.extend_from_slice(value);
    }

    fn text(&mut self, value: &str) {
        self.bytes(value.as_bytes());
    }

    fn flag(&mut self, value: bool) {
        self.uint(u64::from(value));
    }
}

fn item(index: usize, size_bytes: u64, created_unix_seconds: u64) -> ChooseBufferItem {
    ChooseBufferItem {
        name: format!("buffer{index} — λ🦀"),
        preview: format!("preview {index}\0\t next line"),
        size_bytes,
        created_unix_seconds,
        key: match index % 4 {
            0 => String::new(),
            1 => "M-a".into(),
            _ => index.to_string(),
        },
        text: if index % 7 == 3 {
            "Custom row replaces every original column".into()
        } else {
            String::new()
        },
        tagged: index.is_multiple_of(3),
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut sizes = BTreeSet::from([0, 1, 999, u64::MAX, u64::MAX / 10, u64::MAX / 10 + 1]);
    for exponent in 0..64 {
        let value = 1_u64 << exponent;
        sizes.extend([value.saturating_sub(1), value, value.saturating_add(1)]);
    }
    for unit in [1_024_u64, 1_048_576, 1_073_741_824] {
        for whole in [1_u64, 9, 10, 99, 1_023] {
            let base = whole.saturating_mul(unit);
            for delta in [
                0,
                1,
                unit / 20 - 1,
                unit / 20,
                unit / 20 + 1,
                unit / 2,
                unit - 1,
            ] {
                sizes.extend([base.saturating_sub(delta), base.saturating_add(delta)]);
            }
        }
    }
    let sizes: Vec<_> = sizes.into_iter().collect();
    let times = [
        0,
        1,
        59,
        60,
        61,
        3_599,
        3_600,
        3_601,
        86_399,
        86_400,
        86_401,
        1_700_000_000,
        u64::MAX,
    ];
    let mut packet = Packet::default();
    packet.uint((sizes.len() * times.len()) as u64);
    for (index, bytes) in sizes.iter().enumerate() {
        for now in times {
            let created = times[index % times.len()];
            let mut value = item(0, *bytes, created);
            value.text.clear();
            let (_, _, size, age) = buffer_row_text(&value, now);
            packet.uint(*bytes);
            packet.uint(created);
            packet.uint(now);
            packet.text(&size);
            packet.text(&age);
        }
    }
    packet.uint(6 * 16);
    let mut rows = 0;
    for count in [0, 1, 7, 10, 11, 64] {
        for flags in 0..16_u8 {
            let now = times[usize::from(flags) % times.len()];
            let marked = if flags & 1 == 0 { "" } else { "λ🦀" };
            let state = ChooseBufferState {
                items: (0..count)
                    .map(|index| {
                        item(
                            index,
                            sizes[index % sizes.len()],
                            times[index % times.len()],
                        )
                    })
                    .collect(),
                search: (flags & 3 != 0).then(|| ChooseBufferSearchState {
                    query: "find buffer".into(),
                    reverse: flags & 2 != 0,
                }),
                selected: if flags & 1 == 0 { u32::MAX } else { 2 },
                filter_no_matches: flags & 4 != 0,
                help: flags & 8 != 0,
                prompt: if flags & 4 == 0 {
                    String::new()
                } else {
                    "Select a buffer".into()
                },
            };
            packet.bytes(&encode_protocol_message(&ProtocolMessage::Event(Event {
                sequence: 0,
                payload: EventPayload::ChooseBuffer {
                    state: Some(state.clone()),
                },
            }))?);
            packet.uint(now);
            packet.text(marked);
            packet.text("Paste buffer");
            packet.text(&chooser_subtitle(
                format!("{count} buffers"),
                state.filter_no_matches,
            ));
            packet.uint(u64::from(
                chooser_modal_height(count, state.help, !state.prompt.is_empty()).to_bits(),
            ));
            packet.flag(chooser_has_key_gutter(
                state.items.iter().map(|item| item.key.as_str()),
            ));
            packet.uint(u64::from(state.selected));
            packet.uint(count as u64);
            for item in &state.items {
                let (name, preview, size, age) = buffer_row_text(item, now);
                for text in [chooser_key_label(&item.key), name, preview, size, age] {
                    packet.text(&text);
                }
                packet.flag(item.tagged);
                rows += 1;
            }
            packet.flag(state.search.is_some());
            if let Some(search) = &state.search {
                packet.text(if search.reverse { "?" } else { "/" });
                packet.text(&format!("{}{marked}", search.query));
            }
        }
    }
    let length = packet.0.len();
    fs::write(
        std::env::args_os()
            .nth(1)
            .ok_or("expected buffer vector path")?,
        packet.0,
    )?;
    println!("{} 96 {rows} {length}", sizes.len() * times.len());
    Ok(())
}
