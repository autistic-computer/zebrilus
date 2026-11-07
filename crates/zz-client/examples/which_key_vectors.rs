use std::{error::Error, fs, path::Path};

use zz_client::which_key::{WhichKeyRow, action_rows, opens_all_keys, rows};
use zz_protocol::{
    CommandInvocation, Event, EventPayload, KeyBindingSnapshot, KeyTableSnapshot, KeyTables,
    ProtocolMessage, SourceSpan, command_specs, encode_protocol_message,
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

    fn rows(&mut self, value: &[WhichKeyRow]) {
        self.uint(value.len() as u64);
        for row in value {
            self.uint(row.keys.len() as u64);
            for set in &row.keys {
                self.uint(set.keys.len() as u64);
                for key in &set.keys {
                    self.text(key);
                }
                self.flag(set.yours);
            }
            self.text(&row.label);
            self.uint(row.group.map_or(0, |group| group as u64 + 1));
            self.flag(row.repeat);
            self.flag(row.core);
            self.text(row.first_key());
            self.flag(row.yours());
        }
    }
}

fn frame(tables: &[KeyTableSnapshot]) -> Result<Vec<u8>, Box<dyn Error>> {
    Ok(encode_protocol_message(&ProtocolMessage::Event(Event {
        sequence: 0,
        payload: EventPayload::KeyTablesChanged {
            tables: tables.to_vec(),
        },
    }))?)
}

fn binding(key: &str, name: &str, args: &[&str]) -> KeyBindingSnapshot {
    KeyBindingSnapshot {
        key: key.into(),
        commands: vec![CommandInvocation::new(name, args.iter().copied())],
        repeat: false,
        note: None,
    }
}

fn table(name: &str, bindings: Vec<KeyBindingSnapshot>) -> Vec<KeyTableSnapshot> {
    vec![KeyTableSnapshot {
        name: name.into(),
        bindings,
    }]
}

fn record(
    packet: &mut Packet,
    tables: &[KeyTableSnapshot],
    name: &str,
    prefix: &str,
    count: &mut usize,
) -> Result<(), Box<dyn Error>> {
    packet.bytes(&frame(tables)?);
    packet.text(name);
    packet.text(prefix);
    packet.rows(&rows(tables, name, prefix));
    packet.rows(&action_rows(tables, name, prefix));
    let bindings = tables
        .iter()
        .find(|table| table.name == name)
        .map_or(&[][..], |table| table.bindings.as_slice());
    packet.uint(bindings.len() as u64);
    for binding in bindings {
        packet.flag(opens_all_keys(&binding.commands));
    }
    *count += 1;
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let directory = std::env::args_os()
        .nth(1)
        .ok_or("expected output directory")?;
    let directory = Path::new(&directory);
    fs::create_dir_all(directory)?;
    let stock: Vec<_> = KeyTables::default()
        .snapshot()
        .into_iter()
        .filter(|table| table.name == "prefix")
        .collect();
    fs::write(directory.join("stock.bin"), frame(&stock)?)?;
    let mut names: Vec<_> = command_specs()
        .flat_map(|spec| std::iter::once(spec.name).chain(spec.aliases.iter().copied()))
        .collect();
    names.sort_unstable();
    names.dedup();
    fs::write(
        directory.join("implemented.json"),
        serde_json::to_vec(&names)?,
    )?;
    let mut packet = Packet::default();
    let mut count = 0;
    for prefix in [
        "C-b",
        "C-a",
        "Space",
        "None",
        "M-λ",
        "Ctrl-Alt-a",
        "   C-x  ",
        "x",
        "?",
    ] {
        let mut defaults = KeyTables::default();
        defaults.set_prefix(prefix);
        let tables = defaults.snapshot();
        for name in ["prefix", "root", "copy-mode", "copy-mode-vi", "missing"] {
            record(&mut packet, &tables, name, prefix, &mut count)?;
        }
        record(&mut packet, &stock, "prefix", prefix, &mut count)?;
    }
    let commands = [
        "split-window",
        "select-pane",
        "resize-pane",
        "kill-pane",
        "last-pane",
        "break-pane",
        "swap-pane",
        "display-panes",
        "new-pane",
        "if-shell",
        "switch-client",
        "rotate-window",
        "next-layout",
        "select-layout",
        "new-window",
        "next-window",
        "previous-window",
        "select-window",
        "choose-tree",
        "last-window",
        "rename-window",
        "kill-window",
        "find-window",
        "move-window",
        "detach-client",
        "rename-session",
        "new-session",
        "choose-client",
        "kill-session",
        "copy-mode",
        "paste-buffer",
        "choose-buffer",
        "list-buffers",
        "delete-buffer",
        "command-prompt",
        "choose-path",
        "list-keys",
        "reload-config",
        "source-file",
        "send-last-output",
        "display-message",
        "show-messages",
        "send-prefix",
        "clock-mode",
        "unknown",
        "splitw",
        "selectp",
        "resizep",
        "next",
        "previous",
        "list-k",
        "s",
    ];
    let arguments: &[&[&str]] = &[
        &[],
        &["-h"],
        &["-L"],
        &["-D"],
        &["-U"],
        &["-R"],
        &["-Z"],
        &["-m"],
        &["-M"],
        &["-T"],
        &["-t", ":.+"],
        &["-R", "1"],
        &["-L", "+001"],
        &["-D", "0"],
        &["-U", "65535"],
        &["-R", "65536"],
        &["-R", "-1", " 2", "+"],
        &["-n"],
        &["-p"],
        &["-l"],
        &["-a"],
        &["-E"],
        &["even-horizontal"],
        &["tiled"],
        &["bogus"],
        &["-t", ":=+000255"],
        &["-t", "256"],
        &["-t", "-1"],
        &["-t", "::=0"],
        &["-t"],
        &["-swZ"],
        &["--windows"],
        &["-u"],
        &["-N"],
        &["-N", "-N"],
        &["--kind", "terminal"],
        &["--kind=picker"],
        &["-h", "--kind", "browser"],
        &["--kind", "", "--kind=agent"],
        &["--kind=agent", "--kind", "editor"],
        &["--kind"],
        &["pane_floating_flag"],
        &["-U", "-D", "-R", "+02"],
    ];
    for name in commands {
        for args in arguments {
            for table_name in ["prefix", "custom"] {
                record(
                    &mut packet,
                    &table(table_name, vec![binding("x", name, args)]),
                    table_name,
                    "C-b",
                    &mut count,
                )?;
            }
        }
        for outer in [
            "confirm-before",
            "command-prompt",
            "confirm",
            "command-p",
            "unknown",
        ] {
            for syntax in [
                format!(" {{ {name} -T }} "),
                format!("{{{{{name} -L}}}}"),
                format!("\u{85}{name}\u{2003}-R\u{a0}+002\u{2029}"),
                format!("{name} '-h'"),
            ] {
                record(
                    &mut packet,
                    &table("prefix", vec![binding("x", outer, &[&syntax])]),
                    "prefix",
                    "C-b",
                    &mut count,
                )?;
            }
        }
    }
    for source in &stock[0].bindings {
        for variant in 0..11 {
            let mut altered = source.clone();
            match variant {
                0 => altered.repeat = !altered.repeat,
                1 => altered.note = Some(String::new()),
                2 => altered.note = Some(" ".into()),
                3 => altered.note = Some("custom label".into()),
                4 => altered.note = None,
                5 => altered
                    .commands
                    .push(CommandInvocation::new("display", ["feedback"])),
                6 => {
                    altered.commands[0].source = Some(SourceSpan {
                        source: "config".into(),
                        line: 2,
                        column: 3,
                    });
                }
                7 => altered.commands[0] = altered.commands[0].clone().into_expanded_alias_group(),
                8 => altered.commands[0].set_stdin(""),
                9 => altered.commands[0].set_stdin_available(true),
                _ => altered.commands[0] = altered.commands[0].clone().with_command_blocks([0]),
            }
            record(
                &mut packet,
                &table("prefix", vec![altered]),
                "prefix",
                "C-b",
                &mut count,
            )?;
        }
    }
    let keys = [
        "a",
        "b",
        "λ",
        "🙂",
        "e\u{301}",
        "Left",
        "C-a",
        "C-b",
        "C-λ",
        "M-a",
        "S-a",
        "C-M-a",
        "M-C-a",
        "C-C-a",
        "C-",
        "S-C-S-a",
        "None",
        "MouseDown1Pane",
        "xMousex",
        "Wheel",
        "Click",
        "click",
    ];
    for rotate in 0..keys.len() {
        let mut bindings = Vec::new();
        for (index, key) in keys
            .iter()
            .cycle()
            .skip(rotate)
            .take(keys.len())
            .enumerate()
        {
            let mut value = binding(key, "select-pane", &[["-L", "-D", "-U", "-R"][index % 4]]);
            value.repeat = index % 3 != 0;
            bindings.push(value);
        }
        for table_name in ["prefix", "custom"] {
            record(
                &mut packet,
                &table(table_name, bindings.clone()),
                table_name,
                "C-b",
                &mut count,
            )?;
        }
    }
    for name in names {
        let mut bindings = vec![binding("a", name, &["-N"])];
        bindings.push(KeyBindingSnapshot {
            key: "b".into(),
            commands: vec![
                CommandInvocation::new("new-pane", ["-d"]),
                CommandInvocation::new("unknown", ["x"]),
                CommandInvocation::new("switch-mode", ["-wow"]),
            ],
            repeat: true,
            note: None,
        });
        bindings.push(KeyBindingSnapshot {
            key: "c".into(),
            commands: vec![
                CommandInvocation::new("new-pane", ["-d"]),
                CommandInvocation::new("switch-mode", ["s"]),
            ],
            repeat: false,
            note: None,
        });
        bindings.push(KeyBindingSnapshot {
            key: "d".into(),
            commands: vec![
                CommandInvocation::new("unknown", [vec![0xff, 0x80, 0, b'a']]),
                CommandInvocation::new("display", ["hidden"]),
                CommandInvocation::new("unknown", ["tail"]),
            ],
            repeat: false,
            note: None,
        });
        let mut tables = table("prefix", bindings);
        tables.extend(table("prefix", vec![binding("ignored", "kill-pane", &[])]));
        record(&mut packet, &tables, "prefix", "C-b", &mut count)?;
    }
    record(
        &mut packet,
        &table(
            "prefix",
            vec![KeyBindingSnapshot {
                key: "empty".into(),
                commands: Vec::new(),
                repeat: false,
                note: None,
            }],
        ),
        "prefix",
        "C-b",
        &mut count,
    )?;
    let mut output = Packet::default();
    output.uint(count as u64);
    output.0.extend(packet.0);
    fs::write(directory.join("cases.bin"), output.0)?;
    println!("{count}");
    Ok(())
}
