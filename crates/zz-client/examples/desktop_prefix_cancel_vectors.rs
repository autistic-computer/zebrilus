use std::{error::Error, fs};
use zz_client::desktop_input::{
    DesktopDialogPrefix, desktop_dialog_prefix, desktop_prefix_acknowledged, desktop_prefix_reset,
    next_prefix_cancel_request,
};
use zz_protocol::{InputMessage, ProtocolMessage, encode_protocol_message};

fn optional(output: &mut Vec<u8>, value: Option<u64>) {
    output.push(u8::from(value.is_some()));
    output.extend_from_slice(&value.unwrap_or_default().to_le_bytes());
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut output = Vec::new();
    let pending = [None, Some(0), Some(1), Some(2), Some(u64::MAX)];
    for previous in [0, 1, u64::MAX - 1, u64::MAX] {
        for pending in pending {
            for sent in [false, true] {
                for active in [false, true] {
                    for acknowledged in [None, Some(0), Some(1), Some(2), Some(u64::MAX)] {
                        for accepted in [false, true] {
                            let before = DesktopDialogPrefix { sent, pending };
                            let (mut after, request) =
                                desktop_dialog_prefix(before, active, acknowledged);
                            let mut next = previous;
                            let mut frame = Vec::new();
                            if request && accepted {
                                next = next_prefix_cancel_request(previous);
                                after.pending = Some(next);
                                after.sent = true;
                                frame = encode_protocol_message(&ProtocolMessage::Input(
                                    InputMessage::CancelPrefix { request_id: next },
                                ))?;
                            }
                            output.extend_from_slice(&previous.to_le_bytes());
                            optional(&mut output, pending);
                            output.extend([u8::from(sent), u8::from(active)]);
                            optional(&mut output, acknowledged);
                            output.extend([u8::from(accepted), u8::from(request)]);
                            output.extend_from_slice(&next.to_le_bytes());
                            output.push(u8::from(after.sent));
                            optional(&mut output, after.pending);
                            output.extend_from_slice(&u32::try_from(frame.len())?.to_le_bytes());
                            output.extend(frame);
                            optional(
                                &mut output,
                                desktop_prefix_acknowledged(acknowledged, previous),
                            );
                            optional(&mut output, desktop_prefix_reset(acknowledged, previous));
                        }
                    }
                }
            }
        }
    }
    fs::write(
        std::env::args().nth(1).ok_or("missing output path")?,
        output,
    )?;
    Ok(())
}
