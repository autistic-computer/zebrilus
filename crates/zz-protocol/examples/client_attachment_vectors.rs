use std::{error::Error, fmt::Write as _};

use serde_json::{Value, json};
use zz_protocol::{
    ClientExitAction, Event, EventPayload, ProtocolMessage, decode_protocol_frame,
    encode_protocol_message,
};

fn hex(bytes: &[u8]) -> Result<String, std::fmt::Error> {
    let mut result = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(result, "{byte:02x}")?;
    }
    Ok(result)
}

fn frame(message: &ProtocolMessage) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut bytes = encode_protocol_message(&ProtocolMessage::TreeSync)?;
    bytes.truncate(8);
    bytes.extend(postcard::to_stdvec(message)?);
    let length = u32::try_from(bytes.len() - 4)?;
    bytes[..4].copy_from_slice(&length.to_le_bytes());
    Ok(bytes)
}

fn record(
    cases: &mut Vec<Value>,
    name: &str,
    kind: u8,
    mut bytes: Vec<u8>,
    refusal: bool,
) -> Result<(), Box<dyn Error>> {
    if bytes.len() >= 4 {
        let size = u32::try_from(bytes.len() - 4)?;
        bytes[..4].copy_from_slice(&size.to_le_bytes());
    }
    let decoded = decode_protocol_frame(&bytes).ok();
    let target = decoded.as_ref().is_some_and(|message| {
        matches!(
            (kind, message),
            (
                0,
                ProtocolMessage::Event(Event {
                    payload: EventPayload::Detached { .. },
                    ..
                })
            ) | (
                1,
                ProtocolMessage::Event(Event {
                    payload: EventPayload::ServerStopping,
                    ..
                })
            )
        )
    });
    let status = if refusal || decoded.is_none() {
        0
    } else if target {
        1
    } else {
        2
    };
    let expected = if status == 1 {
        Some(hex(&encode_protocol_message(
            decoded.as_ref().ok_or("missing decoded event")?,
        )?)?)
    } else {
        None
    };
    cases.push(json!({"name":name,"kind":kind,"status":status,"bytes":hex(&bytes)?,"expected":expected,"refusal":refusal}));
    Ok(())
}

fn detached(
    sequence: u64,
    session: u64,
    by: Option<&str>,
    reason: &str,
    action: &ClientExitAction,
) -> Result<ProtocolMessage, Box<dyn Error>> {
    Ok(ProtocolMessage::Event(Event {
        sequence,
        payload: serde_json::from_value(
            json!({"Detached":{"session":session,"by":by,"reason":reason,"action":action}}),
        )?,
    }))
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut cases = Vec::new();
    for sequence in [0, 1, 127, 128, u64::MAX] {
        for session in [0, 1, 127, 128, u64::MAX] {
            for by in [None, Some(""), Some("device λ"), Some("世界\0\n🦀")] {
                for reason in ["Requested", "Evicted", "SessionDestroyed", "ServerStopping"] {
                    for action in [
                        ClientExitAction::Exit,
                        ClientExitAction::ParentHangup,
                        ClientExitAction::Exec {
                            command: "exec '世界'\0\n".to_owned(),
                            shell: "/bin/λ shell".to_owned(),
                        },
                    ] {
                        record(
                            &mut cases,
                            "detached",
                            0,
                            frame(&detached(sequence, session, by, reason, &action)?)?,
                            false,
                        )?;
                    }
                }
            }
        }
        let stopping = frame(&ProtocolMessage::Event(Event {
            sequence,
            payload: EventPayload::ServerStopping,
        }))?;
        record(&mut cases, "stopping", 1, stopping.clone(), false)?;
        record(&mut cases, "other-event", 0, stopping, false)?;
    }
    for size in [1024 * 1024, 1024 * 1024 + 1] {
        for field in 0..3 {
            let text = "x".repeat(size);
            let (by, command, shell) = match field {
                0 => (Some(text), String::new(), String::new()),
                1 => (None, text, String::new()),
                _ => (None, String::new(), text),
            };
            record(
                &mut cases,
                "text-bound",
                0,
                frame(&detached(
                    128,
                    u64::MAX,
                    by.as_deref(),
                    "Requested",
                    &ClientExitAction::Exec { command, shell },
                )?)?,
                size > 1024 * 1024,
            )?;
        }
    }
    let base = frame(&detached(
        1,
        2,
        Some("λ"),
        "Evicted",
        &ClientExitAction::Exec {
            command: "echo 世界".to_owned(),
            shell: "/bin/sh".to_owned(),
        },
    )?)?;
    for length in 8..base.len() {
        record(
            &mut cases,
            "truncated-detached",
            0,
            base[..length].to_vec(),
            false,
        )?;
    }
    let simple = frame(&detached(0, 0, None, "Requested", &ClientExitAction::Exit)?)?;
    for (offset, values) in [
        (8, &[76_u8, 127, 255][..]),
        (10, &[66_u8, 127, 255][..]),
        (12, &[2_u8, 255][..]),
        (13, &[4_u8, 127, 255][..]),
        (14, &[3_u8, 127, 255][..]),
    ] {
        for value in values {
            let mut bytes = simple.clone();
            bytes[offset] = *value;
            record(&mut cases, "invalid-tag", 0, bytes, false)?;
        }
    }
    for (offset, values) in [
        (8, vec![0x8a, 0]),
        (8, vec![0x8a, 0x80, 0x80, 0x80, 0]),
        (9, vec![0x80, 0]),
        (10, vec![0x96, 0]),
        (11, vec![0x80, 0]),
        (13, vec![0x80, 0]),
        (14, vec![0x80, 0]),
    ] {
        let mut bytes = simple.clone();
        bytes.splice(offset..=offset, values);
        record(&mut cases, "noncanonical-varint", 0, bytes, false)?;
    }
    for offset in [8, 10, 13, 14] {
        let mut bytes = simple.clone();
        let low = bytes[offset] | 128;
        bytes.splice(offset..=offset, [low, 0x80, 0x80, 0x80, 0x80, 0]);
        record(&mut cases, "overlong-enum", 0, bytes, false)?;
    }
    for offset in [9, 11] {
        let mut bytes = simple.clone();
        bytes.splice(
            offset..=offset,
            [0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 2],
        );
        record(&mut cases, "overflow-id", 0, bytes, false)?;
    }
    let mut invalid_utf8 = base.clone();
    invalid_utf8[14] = 0xff;
    record(&mut cases, "invalid-utf8", 0, invalid_utf8, false)?;
    for kind in [0, 1] {
        let original = if kind == 0 {
            simple.clone()
        } else {
            frame(&ProtocolMessage::Event(Event {
                sequence: 0,
                payload: EventPayload::ServerStopping,
            }))?
        };
        let mut extra = original.clone();
        extra.push(0);
        record(&mut cases, "trailing", kind, extra, false)?;
        let mut version = original;
        version[4] ^= 127;
        record(&mut cases, "wrong-version", kind, version, false)?;
        record(
            &mut cases,
            "other-message",
            kind,
            frame(&ProtocolMessage::TreeSync)?,
            false,
        )?;
    }
    record(&mut cases, "other-event", 1, base, false)?;
    println!("{}", json!({"cases":cases}));
    Ok(())
}
