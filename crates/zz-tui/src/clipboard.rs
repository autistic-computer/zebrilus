use base64::{Engine as _, engine::general_purpose::STANDARD};
use zz_protocol::ClipboardProducer;
use zz_terminal::ClipboardTarget;

pub(crate) const MAX_OSC52_PAYLOAD_BYTES: usize = 1024 * 1024;
pub(crate) const QUERY: &[u8] = b"\x1b]52;;?\x07";

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum Osc52 {
    Empty,
    Encoded(Vec<u8>),
    TooLarge,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Selection {
    TerminalDefault,
    Named(ClipboardTarget),
}

impl Selection {
    pub(crate) const fn for_producer(producer: ClipboardProducer, target: ClipboardTarget) -> Self {
        match producer {
            ClipboardProducer::Server => Self::TerminalDefault,
            ClipboardProducer::Application => Self::Named(target),
        }
    }
}

pub(crate) fn encode(selection: Selection, text: &str) -> Osc52 {
    if text.is_empty() {
        return Osc52::Empty;
    }
    let encoded_len = text.len().div_ceil(3).saturating_mul(4);
    if encoded_len > MAX_OSC52_PAYLOAD_BYTES {
        return Osc52::TooLarge;
    }

    let payload = STANDARD.encode(text);
    let selections: &[u8] = match selection {
        Selection::TerminalDefault => b"",
        Selection::Named(ClipboardTarget::Clipboard) => b"c",
        Selection::Named(ClipboardTarget::Primary) => b"pc",
    };
    let writes = selections.len().max(1);
    let mut output = Vec::with_capacity(writes.saturating_mul(payload.len().saturating_add(8)));
    for index in 0..writes {
        output.extend_from_slice(b"\x1b]52;");
        if let Some(field) = selections.get(index) {
            output.push(*field);
        }
        output.push(b';');
        output.extend_from_slice(payload.as_bytes());
        output.push(0x07);
    }
    Osc52::Encoded(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_server_selection_names_no_field_and_an_application_write_keeps_its_own() {
        for target in [ClipboardTarget::Clipboard, ClipboardTarget::Primary] {
            assert_eq!(
                encode(
                    Selection::for_producer(ClipboardProducer::Server, target),
                    "hello"
                ),
                Osc52::Encoded(b"\x1b]52;;aGVsbG8=\x07".to_vec()),
                "the pin writes one empty-field selection whichever target the copy names"
            );
        }
        assert_eq!(
            encode(
                Selection::for_producer(ClipboardProducer::Application, ClipboardTarget::Clipboard),
                "hello"
            ),
            Osc52::Encoded(b"\x1b]52;c;aGVsbG8=\x07".to_vec())
        );
        assert_eq!(
            encode(
                Selection::for_producer(ClipboardProducer::Application, ClipboardTarget::Primary),
                "hello"
            ),
            Osc52::Encoded(b"\x1b]52;p;aGVsbG8=\x07\x1b]52;c;aGVsbG8=\x07".to_vec())
        );
        assert_eq!(
            encode(Selection::Named(ClipboardTarget::Clipboard), ""),
            Osc52::Empty
        );
    }

    #[test]
    fn encoded_payload_is_capped_at_one_mibibyte() {
        let accepted = "a".repeat(786_432);
        let rejected = "a".repeat(786_433);

        let Osc52::Encoded(encoded) =
            encode(Selection::Named(ClipboardTarget::Clipboard), &accepted)
        else {
            panic!("payload at the cap should be accepted");
        };
        assert_eq!(encoded.len(), MAX_OSC52_PAYLOAD_BYTES + 8);
        assert_eq!(
            encode(Selection::Named(ClipboardTarget::Clipboard), &rejected),
            Osc52::TooLarge
        );
    }
}
