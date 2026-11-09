use std::{error::Error, fs, path::Path};

use zz_client::{
    ChromeKeymap, ChromeProfile, ClientCore, Disposition, Effect, InputEvent, PrefixView,
    SurfaceKind,
    desktop_input::{
        DesktopDrag, DesktopGuard, DesktopGuardContext, DesktopKeyContext, desktop_guard,
        desktop_intercepts, desktop_key_projection, desktop_router,
    },
    prefix_binding::{binds_input, is_all_keys_input, is_sidebar_picker_input},
};
use zz_protocol::{
    ClientId, ClientInstanceId, CommandInvocation, Event, EventPayload, InputMessage,
    KeyBindingSnapshot, KeyTableSnapshot, MouseBindings, PROTOCOL_VERSION, PaneId, ProtocolMessage,
    SourceSpan, Welcome, encode_protocol_message,
};
use zz_terminal::{KeyAction, KeyCode, KeyInput, Modifiers};

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

    fn blob(&mut self, value: &[u8]) {
        self.uint(value.len() as u64);
        self.0.extend_from_slice(value);
    }

    fn text(&mut self, value: &str) {
        self.blob(value.as_bytes());
    }

    fn flag(&mut self, value: bool) {
        self.uint(u64::from(value));
    }

    fn pane(&mut self, value: Option<PaneId>) {
        self.flag(value.is_some());
        if let Some(pane) = value {
            self.uint(pane.0);
        }
    }

    fn active(&mut self, value: Option<(PaneId, SurfaceKind)>) {
        self.flag(value.is_some());
        if let Some((pane, kind)) = value {
            self.uint(pane.0);
            self.uint(surface(kind));
        }
    }
}

const fn surface(kind: SurfaceKind) -> u64 {
    match kind {
        SurfaceKind::Terminal => 0,
        SurfaceKind::Browser => 1,
        SurfaceKind::Other => 2,
    }
}

fn guard_vectors() -> Packet {
    let mut result = Packet::default();
    let names = [
        "escape", "shift", "control", "alt", "platform", "function", "Escape", "x", "", "λ",
        "enter",
    ];
    result.uint((2 * names.len() * 2 * 2 * 3 * 16 * 2) as u64);
    for window in [false, true] {
        for key in names {
            for overlay in [false, true] {
                for cancel_pending in [false, true] {
                    for (drag_code, drag) in
                        [DesktopDrag::None, DesktopDrag::Free, DesktopDrag::Prefix]
                            .into_iter()
                            .enumerate()
                    {
                        for bits in 0..16 {
                            for function in [false, true] {
                                let context = DesktopGuardContext {
                                    key,
                                    overlay,
                                    cancel_pending,
                                    drag,
                                    modifiers: Modifiers::from_bits(bits).expect("valid modifiers"),
                                    function,
                                };
                                result.flag(window);
                                result.text(key);
                                result.flag(overlay);
                                result.flag(cancel_pending);
                                result.uint(drag_code as u64);
                                result.uint(u64::from(bits));
                                result.flag(function);
                                result.flag(desktop_intercepts(window, key));
                                result.uint(match desktop_guard(context) {
                                    DesktopGuard::Native => 0,
                                    DesktopGuard::CancelDrag => 1,
                                    DesktopGuard::WaitPrefixCancel => 2,
                                    DesktopGuard::Route => 3,
                                });
                            }
                        }
                    }
                }
            }
        }
    }
    result
}

fn projection_vectors() -> Packet {
    let mut result = Packet::default();
    let active = [
        None,
        Some((PaneId(1), SurfaceKind::Terminal)),
        Some((PaneId(1), SurfaceKind::Browser)),
        Some((PaneId(u64::MAX), SurfaceKind::Other)),
    ];
    let captures = [None, Some(PaneId(0)), Some(PaneId(u64::MAX))];
    result.uint((active.len() * active.len() * captures.len() * 2 * 2 * 2 * 16 * 2) as u64);
    for before in active {
        for active in active {
            for capture in captures {
                for armed in [false, true] {
                    for claimed in [false, true] {
                        for overlay in [false, true] {
                            for bits in 0..16 {
                                for function in [false, true] {
                                    let projection = desktop_key_projection(DesktopKeyContext {
                                        before,
                                        active,
                                        capture,
                                        prefix: PrefixView { armed, claimed },
                                        overlay,
                                        modifiers: Modifiers::from_bits(bits)
                                            .expect("valid modifiers"),
                                        function,
                                    });
                                    result.active(before);
                                    result.active(active);
                                    result.pane(capture);
                                    result.flag(armed);
                                    result.flag(claimed);
                                    result.flag(overlay);
                                    result.uint(u64::from(bits));
                                    result.flag(function);
                                    match projection.event {
                                        None => result.uint(0),
                                        Some(InputEvent::SetActivePane(pane, kind)) => {
                                            result.uint(1);
                                            result.uint(pane.0);
                                            result.uint(surface(kind));
                                        }
                                        Some(InputEvent::Detached) => result.uint(2),
                                        _ => unreachable!(),
                                    }
                                    result.pane(projection.capture);
                                    result.flag(projection.prefix.armed);
                                    result.flag(projection.prefix.claimed);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    result
}

fn table(name: &str, bindings: Vec<KeyBindingSnapshot>) -> KeyTableSnapshot {
    KeyTableSnapshot {
        name: name.into(),
        bindings,
    }
}

fn binding(key: &str, name: &str, args: &[&str]) -> KeyBindingSnapshot {
    let mut command = CommandInvocation::new(name, args.iter().copied());
    command.source = Some(SourceSpan {
        source: "fixture\0λ".into(),
        line: u32::MAX,
        column: 1,
    });
    command.set_stdin([0xff, 0, 0x80].as_slice());
    command.set_stdin_available(true);
    KeyBindingSnapshot {
        key: key.into(),
        commands: vec![command],
        repeat: true,
        note: Some("note\0λ".into()),
    }
}

fn inputs() -> Vec<KeyInput> {
    let mut result = Vec::new();
    for key in [
        KeyCode::Character('x'),
        KeyCode::Character('X'),
        KeyCode::Character('?'),
        KeyCode::Escape,
        KeyCode::Tab,
        KeyCode::Unidentified,
        KeyCode::Function(255),
        KeyCode::User(u16::MAX),
    ] {
        for bits in 0..16 {
            for action in [KeyAction::Press, KeyAction::Repeat, KeyAction::Release] {
                for text in [
                    None,
                    Some("x"),
                    Some("?"),
                    Some("λ"),
                    Some("e\u{301}"),
                    Some("\u{1c}"),
                ] {
                    result.push(KeyInput {
                        action,
                        key,
                        modifiers: Modifiers::from_bits(bits).expect("valid modifiers"),
                        text: text.map(Into::into),
                        unshifted_codepoint: Some('x'),
                    });
                }
            }
        }
    }
    result
}

fn event(payload: EventPayload) -> ProtocolMessage {
    ProtocolMessage::Event(Event {
        sequence: 0,
        payload,
    })
}

fn prefix_vectors() -> Result<Packet, Box<dyn Error>> {
    let mut result = Packet::default();
    let inputs = inputs();
    result.uint(inputs.len() as u64);
    for input in &inputs {
        result.blob(&encode_protocol_message(&ProtocolMessage::Input(
            InputMessage::Key {
                pane: PaneId(0),
                input: input.clone(),
                text_follows: false,
            },
        ))?);
    }
    let mut updates = Vec::new();
    for name in [
        "list-keys",
        "lsk",
        "list-k",
        "list-",
        "choose-tree",
        "choose-t",
        "choose-treeX",
        "send-prefix",
    ] {
        for args in [
            vec![],
            vec!["-N"],
            vec!["-s"],
            vec!["-w"],
            vec!["-Zs"],
            vec!["-w", "--"],
            vec!["-s", "--", "x"],
            vec!["-sw"],
            vec!["-sZs"],
            vec!["-s", "-?"],
            vec!["-s", "-F", "x"],
            vec!["-s", "-"],
            vec!["-s", "\u{fffd}"],
        ] {
            updates.push(event(EventPayload::KeyTablesChanged {
                tables: vec![table("prefix", vec![binding("Any", name, &args)])],
            }));
        }
    }
    let first = vec![
        binding("x", "list-keys", &["-N"]),
        binding("C-x", "choose-tree", &["-s"]),
        binding("?", "choose-tree", &["-w"]),
        binding("Escape", "send-prefix", &[]),
        binding("", "choose-tree", &["-Zs"]),
    ];
    let mut duplicate = first.clone();
    duplicate.push(binding("Ctrl-x", "send-prefix", &[]));
    duplicate.push(binding("x", "choose-tree", &["-w"]));
    let mut raw = binding("Any", "choose-tree", &["-s"]);
    raw.commands[0] = CommandInvocation::new("choose-tree", [vec![45, 115], vec![255, 0, 128]])
        .with_command_blocks([0, 1])
        .into_expanded_alias_group();
    raw.commands[0].set_stdin([0, 255, 0, 128].as_slice());
    updates.push(event(EventPayload::KeyTablesChanged {
        tables: vec![table("prefix", vec![raw])],
    }));
    updates.extend([
        event(EventPayload::KeyTablesChanged {
            tables: vec![
                table("z", vec![]),
                table("prefix", first.clone()),
                table("prefix", duplicate.clone()),
                table("a", vec![]),
            ],
        }),
        event(EventPayload::KeyTableActive {
            table: Some("prefix".into()),
            repeat: false,
        }),
        event(EventPayload::KeyTablesPatched {
            tables: vec![table("a", vec![]), table("z", vec![])],
            removed: vec!["x".into()],
        }),
        event(EventPayload::KeyTablesHashChanged {
            hash: u64::MAX,
            mouse: MouseBindings::default(),
        }),
        event(EventPayload::KeyTablesPatched {
            tables: vec![
                table("prefix", duplicate.clone()),
                table("prefix", first.clone()),
            ],
            removed: vec!["prefix".into(), "z".into()],
        }),
        event(EventPayload::KeyTableActive {
            table: Some("prefix".into()),
            repeat: true,
        }),
        event(EventPayload::KeyTablesPatched {
            tables: vec![table("prefix", duplicate.clone())],
            removed: vec![],
        }),
        event(EventPayload::KeyTablesPatched {
            tables: vec![],
            removed: vec!["prefix".into()],
        }),
        event(EventPayload::KeyTableActive {
            table: None,
            repeat: true,
        }),
        event(EventPayload::KeyTablesChanged {
            tables: vec![table("prefix", duplicate)],
        }),
        event(EventPayload::KeyTablesChanged { tables: vec![] }),
        event(EventPayload::KeyTableActive {
            table: Some("prefix".into()),
            repeat: false,
        }),
        ProtocolMessage::Welcome(Welcome {
            protocol_version: PROTOCOL_VERSION,
            server_id: 1,
            client_id: ClientId(2),
            client_instance_id: ClientInstanceId(3),
            caps: (1 << 29) | (1 << 30),
            clock: u64::MAX,
            platform: zz_protocol::HostPlatform::default(),
        }),
    ]);
    let mut core = ClientCore::new();
    result.uint(updates.len() as u64);
    for update in updates {
        result.blob(&encode_protocol_message(&update)?);
        core.handle_message(update);
        result.blob(&encode_protocol_message(&event(
            EventPayload::KeyTablesChanged {
                tables: vec![table("prefix", core.prefix_bindings().to_vec())],
            },
        ))?);
        result.flag(core.key_table().is_some());
        if let Some((table, repeat)) = core.key_table() {
            result.text(table);
            result.flag(repeat);
        }
        for input in &inputs {
            let bound = binds_input(core.prefix_bindings(), input);
            result.uint(
                u64::from(bound)
                    | (u64::from(is_all_keys_input(core.prefix_bindings(), input)) << 1)
                    | (u64::from(is_sidebar_picker_input(core.prefix_bindings(), input)) << 2)
                    | (u64::from(core.key_table() == Some(("prefix", false)) && !bound) << 3),
            );
        }
    }
    Ok(result)
}

fn constructor_vectors() -> Packet {
    let mut result = Packet::default();
    let inputs = inputs();
    result.uint(6);
    for (profile_index, profile) in [
        ChromeProfile::Tui,
        ChromeProfile::Desktop,
        ChromeProfile::DesktopApple,
    ]
    .into_iter()
    .enumerate()
    {
        for custom in [false, true] {
            let mut configured = ChromeKeymap::for_profile(profile);
            if custom {
                configured
                    .bind("ui", "C-x", "ui-zoom-in")
                    .expect("known action");
                configured
                    .bind("prefix", "x", "ui-zoom-reset")
                    .expect("known action");
                configured
                    .bind("terminal", "x", "ui-zoom-out")
                    .expect("known action");
                configured
                    .bind("ui", "M-x", "new-window")
                    .expect("known action");
            }
            result.uint(profile_index as u64);
            result.flag(custom);
            result.uint(inputs.len() as u64);
            for input in &inputs {
                let mut router = desktop_router(configured.clone());
                result.flag(router.key(input, PrefixView::default()) == Disposition::Native);
                let effects = router.drain_effects();
                result.uint(effects.len() as u64);
                for effect in effects {
                    match effect {
                        Effect::Chrome(action) => result.text(action.name()),
                        _ => unreachable!(),
                    }
                }
            }
        }
    }
    result
}

fn main() -> Result<(), Box<dyn Error>> {
    let directory = std::env::args_os()
        .nth(1)
        .ok_or("expected output directory")?;
    let directory = Path::new(&directory);
    fs::create_dir_all(directory)?;
    fs::write(directory.join("guard.bin"), guard_vectors().0)?;
    fs::write(directory.join("projection.bin"), projection_vectors().0)?;
    fs::write(directory.join("prefix.bin"), prefix_vectors()?.0)?;
    fs::write(directory.join("constructor.bin"), constructor_vectors().0)?;
    Ok(())
}
