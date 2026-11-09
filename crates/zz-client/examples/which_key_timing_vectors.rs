use std::{error::Error, fs};

use zz_client::which_key::timing::{WhichKeyChange, WhichKeyTiming, arm_table};

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

    fn flag(&mut self, value: bool) {
        self.uint(u64::from(value));
    }

    fn text(&mut self, value: &str) {
        self.uint(value.len() as u64);
        self.0.extend_from_slice(value.as_bytes());
    }

    fn state(&mut self, state: WhichKeyTiming) {
        self.uint(state.generation);
        self.flag(state.timer);
        self.flag(state.visible);
    }

    fn case(&mut self, before: WhichKeyTiming, operation: u8, arguments: &[u64]) -> WhichKeyTiming {
        self.state(before);
        self.uint(u64::from(operation));
        self.uint(arguments.len() as u64);
        for &argument in arguments {
            self.uint(argument);
        }
        let yes = |index: usize| arguments[index] != 0;
        let next: WhichKeyChange = match operation {
            0 => before.hide(),
            1 => before.input(yes(0), yes(1), yes(2)),
            2 => before.table_changed(yes(0), yes(1)),
            3 => before.fire(arguments[0], yes(1), yes(2), yes(3)),
            4 => before.render(yes(0)),
            5 => before.show_all(yes(0)),
            _ => unreachable!(),
        };
        self.state(next.state);
        self.flag(next.notify);
        self.flag(next.build);
        next.state
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut cases = Packet::default();
    let mut count = 0;
    for generation in [0, 1, 2, u64::MAX - 1, u64::MAX] {
        for state_bits in 0..4 {
            let state = WhichKeyTiming {
                generation,
                timer: state_bits & 1 != 0,
                visible: state_bits & 2 != 0,
            };
            cases.case(state, 0, &[]);
            count += 1;
            for bits in 0..8_u64 {
                cases.case(state, 1, &[bits & 1, bits >> 1 & 1, bits >> 2 & 1]);
                count += 1;
            }
            for bits in 0..4_u64 {
                cases.case(state, 2, &[bits & 1, bits >> 1 & 1]);
                count += 1;
            }
            for fired in [generation, generation.wrapping_add(1), 0, u64::MAX] {
                for bits in 0..8_u64 {
                    cases.case(state, 3, &[fired, bits & 1, bits >> 1 & 1, bits >> 2 & 1]);
                    count += 1;
                }
            }
            for flag in 0..2 {
                cases.case(state, 4, &[flag]);
                cases.case(state, 5, &[flag]);
                count += 2;
            }
        }
    }
    for seed in 0..32_u64 {
        let mut state = WhichKeyTiming {
            generation: u64::MAX - seed,
            ..WhichKeyTiming::default()
        };
        for index in 0..96_u64 {
            let operation = (seed + index) % 6;
            let arguments = match operation {
                0 => vec![],
                1 => vec![1, index & 1, seed & 1],
                2 => vec![index & 1, (index + seed) >> 1 & 1],
                3 => vec![
                    state.generation.wrapping_sub(index & 1),
                    1,
                    (seed + index) & 1,
                    index >> 1 & 1,
                ],
                _ => vec![index & 1],
            };
            state = cases.case(state, operation as u8, &arguments);
            count += 1;
        }
    }
    let mut packet = Packet::default();
    packet.uint(count);
    packet.0.extend(cases.0);
    packet.uint(13);
    packet.flag(false);
    packet.flag(false);
    for name in ["", "prefix", "copy-mode", "custom λ", "None", "\0"] {
        for repeat in [false, true] {
            packet.flag(true);
            packet.text(name);
            packet.flag(repeat);
            let result = arm_table(Some((name, repeat)));
            packet.flag(result.is_some());
            if let Some(result) = result {
                packet.text(result);
            }
        }
    }
    let length = packet.0.len();
    fs::write(
        std::env::args_os()
            .nth(1)
            .ok_or("expected timing vector path")?,
        packet.0,
    )?;
    println!("{count} 13 {length}");
    Ok(())
}
