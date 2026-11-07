use std::error::Error;

use serde_json::{Value, json};
use zz_protocol::{
    ClipboardProducer, Event, EventPayload, InputMessage, PaneId, PasteUploadPurpose,
    ProtocolMessage, decode_protocol_frame, encode_protocol_message,
};
use zz_terminal::{ClipboardTarget, TerminalViewAction};

fn raw_case(name: &str, mut bytes: Vec<u8>) -> Result<Value, Box<dyn Error>> {
    let length = u32::try_from(bytes.len() - 4)?;
    bytes[..4].copy_from_slice(&length.to_le_bytes());
    let canonical = decode_protocol_frame(&bytes)
        .ok()
        .map(|message| encode_protocol_message(&message))
        .transpose()?;
    Ok(json!({"name":name,"bytes":bytes,"accepted":canonical.is_some(),"canonical":canonical}))
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut cases = Vec::new();
    for producer in [ClipboardProducer::Server, ClipboardProducer::Application] {
        for target in [ClipboardTarget::Clipboard, ClipboardTarget::Primary] {
            for request_id in [0, 1, u64::MAX] {
                for text in ["", "clipboard", "é世界🦀", "a\0b\r\n"] {
                    let message = ProtocolMessage::Event(Event {
                        sequence: u64::MAX,
                        payload: EventPayload::Clipboard {
                            pane: PaneId(u64::MAX),
                            request_id,
                            target,
                            text: text.to_owned(),
                            producer,
                        },
                    });
                    let mut case = raw_case(
                        &format!("write-{producer:?}-{target:?}-{request_id}-{text:?}"),
                        encode_protocol_message(&message)?,
                    )?;
                    case["args"] = json!([
                        6,
                        u64::MAX,
                        u64::MAX,
                        request_id,
                        target as u8,
                        text,
                        producer as u8
                    ]);
                    cases.push(case);
                }
            }
        }
    }
    let query = ProtocolMessage::Event(Event {
        sequence: 0,
        payload: EventPayload::ClipboardQuery,
    });
    let mut case = raw_case("query", encode_protocol_message(&query)?)?;
    case["args"] = json!([63, 0]);
    cases.push(case);
    for data in [
        vec![],
        vec![0],
        vec![0, 127, 128, 255],
        "clipboard 世界".as_bytes().to_vec(),
    ] {
        let message = ProtocolMessage::Input(InputMessage::ClipboardReply { data: data.clone() });
        let mut case = raw_case("reply", encode_protocol_message(&message)?)?;
        case["args"] = json!([24, data]);
        cases.push(case);
    }
    for text in ["q", "é", "e\u{301}", "世界", "a\nb", "\r", "🦀"] {
        for composed in [false, true] {
            let input = if !composed && (text.contains(['\n', '\r']) || text.chars().count() > 1) {
                InputMessage::TerminalView {
                    pane: PaneId(7),
                    action: TerminalViewAction::Paste(text.to_owned()),
                }
            } else {
                InputMessage::Text {
                    pane: PaneId(7),
                    text: text.to_owned(),
                }
            };
            let mut case = raw_case(
                "ordinary-commit",
                encode_protocol_message(&ProtocolMessage::Input(input))?,
            )?;
            case["args"] = json!([0, composed, text]);
            cases.push(case);
        }
    }
    for total in [1, 6 * 1024 * 1024] {
        let message = ProtocolMessage::PasteUploadBegin {
            upload_id: u64::MAX,
            pane: PaneId(u64::MAX),
            purpose: PasteUploadPurpose::PastePath,
            extension: "png".to_owned(),
            total_bytes: total,
        };
        let mut case = raw_case("image-begin", encode_protocol_message(&message)?)?;
        case["args"] = json!([100, u64::MAX, u64::MAX, 0, "png", total]);
        cases.push(case);
    }
    for data in [vec![], vec![0, 127, 128, 255], vec![0; 1024 * 1024]] {
        let message = ProtocolMessage::PasteUploadChunk {
            upload_id: u64::MAX,
            bytes: data.clone(),
        };
        let mut case = raw_case("image-chunk", encode_protocol_message(&message)?)?;
        case["args"] = json!([101, u64::MAX, data]);
        cases.push(case);
    }
    for (purpose, extension, total_bytes) in [
        (PasteUploadPurpose::PastePath, "", 1),
        (PasteUploadPurpose::PastePath, "PNG", 1),
        (PasteUploadPurpose::PastePath, "png", 0),
        (PasteUploadPurpose::PastePath, "png", 6 * 1024 * 1024 + 1),
        (PasteUploadPurpose::RecordPastedImage, "txt", 1),
    ] {
        let message = ProtocolMessage::PasteUploadBegin {
            upload_id: 1,
            pane: PaneId(7),
            purpose,
            extension: extension.to_owned(),
            total_bytes,
        };
        let mut raw = encode_protocol_message(&ProtocolMessage::TreeSync)?;
        raw.truncate(8);
        raw.extend(postcard::to_stdvec(&message)?);
        cases.push(raw_case("invalid-image-begin", raw)?);
    }
    let mut raw = encode_protocol_message(&ProtocolMessage::TreeSync)?;
    raw.truncate(8);
    raw.extend(postcard::to_stdvec(&ProtocolMessage::PasteUploadChunk {
        upload_id: 1,
        bytes: vec![0; 1024 * 1024 + 1],
    })?);
    cases.push(raw_case("oversized-image-chunk", raw)?);
    let base = encode_protocol_message(&ProtocolMessage::Event(Event {
        sequence: 0,
        payload: EventPayload::Clipboard {
            pane: PaneId(0),
            request_id: 0,
            target: ClipboardTarget::Clipboard,
            text: "x".to_owned(),
            producer: ClipboardProducer::Server,
        },
    }))?;
    for end in 10..base.len() {
        cases.push(raw_case("truncated", base[..end].to_vec())?);
    }
    for (name, index, value) in [
        ("bad-target", 13, 2),
        ("bad-utf8", 15, 255),
        ("bad-producer", 16, 2),
    ] {
        let mut bytes = base.clone();
        bytes[index] = value;
        cases.push(raw_case(name, bytes)?);
    }
    let mut trailing = base;
    trailing.extend([0, 255]);
    cases.push(raw_case("trailing", trailing)?);
    println!(
        "{}",
        json!({"source":"actual native clipboard serde and decoder","cases":cases})
    );
    Ok(())
}
