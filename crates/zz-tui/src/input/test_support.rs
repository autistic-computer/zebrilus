use super::*;

pub(super) fn parse_tty(bytes: &[u8]) -> (Vec<TerminalEvent>, bool) {
    let mut parser = crate::terminal_event::EventParser::default();
    let mut events = Vec::new();
    parser.push(bytes, &mut events);
    (events, parser.is_idle())
}

pub(super) fn tty_text_samples() -> Vec<char> {
    let mut samples = (0x80..=0x10_ffff_u32)
        .step_by(37)
        .filter_map(char::from_u32)
        .collect::<Vec<_>>();
    samples.extend([
        '\u{85}',
        '\u{a0}',
        '\u{e9}',
        '\u{c9}',
        '\u{df}',
        '\u{f1}',
        '\u{20ac}',
        '\u{4e2d}',
        '\u{fffd}',
        '\u{1f600}',
        '\u{10ffff}',
    ]);
    samples
}

pub(super) fn tty_sequence_samples() -> Vec<Vec<u8>> {
    let numbers = (0..=30)
        .map(|number: u16| number.to_string())
        .chain(["", "03", "200", "+1"].map(str::to_owned))
        .collect::<Vec<_>>();
    let modifiers = (0..=17)
        .map(|modifier: u8| Some(modifier.to_string()))
        .chain(["", "+2", "05", "2:1", "2:3"].map(|modifier| Some(modifier.to_owned())))
        .chain([None])
        .collect::<Vec<_>>();
    let mut samples = Vec::new();
    for number in &numbers {
        for modifier in &modifiers {
            let parameters = match modifier {
                Some(modifier) => format!("{number};{modifier}"),
                None => number.clone(),
            };
            for final_byte in 0x40..=0x7e_u8 {
                let mut sample = format!("\x1b[{parameters}").into_bytes();
                sample.push(final_byte);
                samples.push(sample);
            }
        }
    }
    samples.extend((0..=u8::MAX).map(|final_byte| vec![0x1b, b'O', final_byte]));
    samples
}
