use serde_json::{Value, json};
use zz_terminal::Modifiers;

use super::*;
use crate::ChromeProfile;

#[derive(Default)]
struct References {
    keys: Vec<KeyCode>,
    inputs: Vec<KeyInput>,
    actions: Vec<ChromeAction>,
}

fn id(value: u64) -> Value {
    json!({"$":"Id","high":(value >> 32) as u32,"low":value as u32})
}

fn reference(domain: u32, index: usize) -> Value {
    id((u64::from(domain) + (index / 8) as u64) << 32 | (index % 8) as u64)
}

fn some(value: Value) -> Value {
    Value::Object(serde_json::Map::from_iter([
        ("$".to_owned(), Value::String("Some".to_owned())),
        ("value".to_owned(), value),
    ]))
}

fn none() -> Value {
    json!({"$":"None"})
}

fn surface(kind: SurfaceKind) -> Value {
    json!({"$":match kind {SurfaceKind::Terminal=>"Terminal",SurfaceKind::Browser=>"Browser",SurfaceKind::Other=>"Other"}})
}

fn owner(value: InputOwner) -> Value {
    match value {
        InputOwner::None => json!({"$":"NoOwner"}),
        InputOwner::Pane(pane, kind) => {
            json!({"$":"Pane","pane":id(pane.0),"surface":surface(kind)})
        }
        InputOwner::Sidebar => json!({"$":"Sidebar"}),
        InputOwner::Overlay => json!({"$":"Overlay"}),
        InputOwner::NativeEditor => json!({"$":"NativeEditor"}),
    }
}

impl References {
    fn key(&mut self, key: KeyCode) -> Value {
        let index = self
            .keys
            .iter()
            .position(|existing| *existing == key)
            .unwrap_or_else(|| {
                self.keys.push(key);
                self.keys.len() - 1
            });
        json!({"$":"KeyToken","id":reference(0x4B45_5900,index)})
    }

    fn input(&mut self, input: &KeyInput) -> Value {
        let index = self
            .inputs
            .iter()
            .position(|existing| existing == input)
            .unwrap_or_else(|| {
                self.inputs.push(input.clone());
                self.inputs.len() - 1
            });
        let key = self.key(input.key);
        let unshifted = input
            .unshifted_codepoint
            .map_or_else(none, |key| some(self.key(KeyCode::Character(key))));
        json!({"$":"Input","key":key,"unshifted":unshifted,"action":{"$":match input.action {KeyAction::Press=>"KeyPressed",KeyAction::Repeat=>"Repeat",KeyAction::Release=>"Release"}},"payload":{"$":"Payload","id":reference(0x5041_5900,index)}})
    }

    fn action(&mut self, action: ChromeAction) -> Value {
        let index = self
            .actions
            .iter()
            .position(|existing| *existing == action)
            .unwrap_or_else(|| {
                self.actions.push(action);
                self.actions.len() - 1
            });
        json!({"$":"ChromeAction","id":reference(0x4143_5400,index)})
    }

    fn effect(&mut self, effect: &Effect) -> Value {
        match effect {
            Effect::ForwardKey { pane, input } => {
                json!({"$":"Forward","pane":id(pane.0),"input":self.input(input)})
            }
            Effect::Chrome(action) => json!({"$":"Chrome","action":self.action(*action)}),
            Effect::RequestFocus(target) => json!({"$":"RequestFocus","owner":owner(*target)}),
        }
    }

    fn effects(&mut self, effects: &[Effect]) -> Value {
        Value::Array(effects.iter().map(|effect| self.effect(effect)).collect())
    }

    fn state(&mut self, router: &InputRouter, map: &Value) -> Value {
        json!({"$":"State","keymap":map,"owner":owner(router.owner),"pending":router.pending.map_or_else(none,|target|some(owner(target))),"observed":owner(router.observed),"restore":owner(router.restore),"active":router.active.map_or_else(none,|(pane,kind)|some(json!({"$":"Active","pane":id(pane.0),"surface":surface(kind)}))),"capture":router.capture.map_or_else(none,|pane|some(id(pane.0))),"claims":{"$":"Claims","held":router.claim.held.iter().map(|(key,pane)|json!({"$":"Held","key":self.key(*key),"pane":id(pane.0)})).collect::<Vec<_>>(),"local":router.claim.local_releases.iter().map(|key|self.key(*key)).collect::<Vec<_>>()},"effects":self.effects(&router.effects)})
    }

    fn lookup(&mut self, router: &InputRouter, input: &KeyInput) -> Value {
        let values = [UI_TABLE, SIDEBAR_TABLE, TERMINAL_TABLE, BROWSER_TABLE].map(|table| {
            router
                .keymap
                .resolve(table, input)
                .map_or_else(none, |action| some(self.action(action)))
        });
        json!({"$":"Lookups","ui":values[0],"sidebar":values[1],"terminal":values[2],"browser":values[3]})
    }

    fn dictionaries(&self) -> Value {
        json!({"keys":self.keys.iter().enumerate().map(|(index,key)|json!({"id":reference(0x4B45_5900,index),"value":key})).collect::<Vec<_>>(),"inputs":self.inputs.iter().enumerate().map(|(index,input)|json!({"id":reference(0x5041_5900,index),"value":input})).collect::<Vec<_>>(),"actions":self.actions.iter().enumerate().map(|(index,action)|json!({"id":reference(0x4143_5400,index),"debug":format!("{action:?}"),"name":action.name()})).collect::<Vec<_>>()})
    }
}

fn event(value: InputEvent) -> Value {
    match value {
        InputEvent::ActivatePane(pane, kind) => {
            json!({"$":"ActivatePane","pane":id(pane.0),"surface":surface(kind)})
        }
        InputEvent::SetActivePane(pane, kind) => {
            json!({"$":"SetActivePane","pane":id(pane.0),"surface":surface(kind)})
        }
        InputEvent::FocusSidebar => json!({"$":"FocusSidebar"}),
        InputEvent::FocusNativeEditor => json!({"$":"FocusNativeEditor"}),
        InputEvent::OverlayOpened => json!({"$":"OverlayOpened"}),
        InputEvent::OverlayClosed => json!({"$":"OverlayClosed"}),
        InputEvent::NativeFocusObserved(target) => {
            json!({"$":"NativeFocusObserved","owner":owner(target)})
        }
        InputEvent::PaneRemoved(pane) => json!({"$":"PaneRemoved","pane":id(pane.0)}),
        InputEvent::Detached => json!({"$":"Detached"}),
        InputEvent::WindowDeactivated => json!({"$":"WindowDeactivated"}),
    }
}

#[derive(Clone)]
enum Command {
    Event(InputEvent),
    Capture(Option<PaneId>),
    Suppress(KeyCode),
    Key(KeyInput, PrefixView),
    Drain,
    Plain(bool),
}

fn input(key: KeyCode, action: KeyAction, modifiers: u8, unshifted: Option<char>) -> KeyInput {
    KeyInput {
        key,
        action,
        modifiers: Modifiers::from_bits(modifiers).expect("fixture modifiers are valid"),
        text: Some(format!("{key:?}\0{modifiers}").into_boxed_str()),
        unshifted_codepoint: unshifted,
    }
}

fn keyboard(key: KeyCode, action: KeyAction, modifiers: u8, armed: bool, claimed: bool) -> Command {
    Command::Key(
        input(key, action, modifiers, None),
        PrefixView { armed, claimed },
    )
}

fn keymap(index: usize) -> ChromeKeymap {
    let mut map = ChromeKeymap::for_profile(match index % 3 {
        0 => ChromeProfile::Tui,
        1 => ChromeProfile::Desktop,
        _ => ChromeProfile::DesktopApple,
    });
    if index.is_multiple_of(5) {
        for (table, key, _) in map.bindings() {
            map.unbind(&table, &key);
        }
    } else if index % 5 == 4 {
        for (table, key, action) in [
            (UI_TABLE, "x", ChromeAction::UiZoomIn),
            (TERMINAL_TABLE, "x", ChromeAction::TerminalCopy),
            (SIDEBAR_TABLE, "x", ChromeAction::SidebarCancel),
            (BROWSER_TABLE, "x", ChromeAction::BrowserCopy),
            (UI_TABLE, "C-x", ChromeAction::OpenCommandPalette),
        ] {
            map.bind(table, key, action.name())
                .expect("fixture chrome action exists");
        }
    }
    if index == 5 || index == 6 {
        for (table, key, _) in map.bindings() {
            map.unbind(&table, &key);
        }
        for (table, action) in [
            (SIDEBAR_TABLE, ChromeAction::SidebarConfirm),
            (TERMINAL_TABLE, ChromeAction::TerminalSearch),
            (BROWSER_TABLE, ChromeAction::BrowserReload),
        ] {
            map.bind(table, "x", action.name())
                .expect("fixture owner action exists");
        }
        if index == 6 {
            map.bind(UI_TABLE, "x", ChromeAction::UiZoomOut.name())
                .expect("fixture UI action exists");
        }
    }
    map
}

fn routing_matrix(owner: InputOwner) -> Vec<Command> {
    let mut commands = Vec::new();
    for armed in [false, true] {
        for claimed in [false, true] {
            for captured in [false, true] {
                for active in [false, true] {
                    for action in [KeyAction::Press, KeyAction::Repeat, KeyAction::Release] {
                        commands.push(Command::Event(InputEvent::Detached));
                        commands.push(Command::Capture(captured.then_some(PaneId(u64::MAX))));
                        commands.push(Command::Event(InputEvent::NativeFocusObserved(owner)));
                        if active {
                            commands.push(Command::Event(InputEvent::SetActivePane(
                                PaneId(0x1_0000_0001),
                                SurfaceKind::Other,
                            )));
                        }
                        commands.extend([
                            Command::Drain,
                            keyboard(KeyCode::Character('x'), action, 0, armed, claimed),
                            Command::Drain,
                            Command::Plain(false),
                            Command::Plain(true),
                        ]);
                    }
                }
            }
        }
    }
    commands
}

fn focused() -> Vec<Command> {
    use Command::{Capture, Drain, Event, Plain, Suppress};
    use InputEvent::{
        ActivatePane, Detached, FocusNativeEditor, FocusSidebar, NativeFocusObserved,
        OverlayClosed, OverlayOpened, PaneRemoved, SetActivePane, WindowDeactivated,
    };
    let a = PaneId(1);
    let b = PaneId(0x8000_0000_0000_0001);
    let x = KeyCode::Character('x');
    let mut commands = vec![
        Event(ActivatePane(a, SurfaceKind::Terminal)),
        Event(NativeFocusObserved(InputOwner::Pane(
            a,
            SurfaceKind::Terminal,
        ))),
        Drain,
        keyboard(x, KeyAction::Press, 0, true, true),
        Drain,
        Plain(true),
        Event(SetActivePane(b, SurfaceKind::Browser)),
        keyboard(x, KeyAction::Repeat, 0, true, true),
        keyboard(x, KeyAction::Repeat, 0, false, true),
        keyboard(x, KeyAction::Release, 15, true, true),
        Drain,
        Capture(Some(a)),
        keyboard(x, KeyAction::Press, 2, false, false),
        Event(OverlayOpened),
        keyboard(x, KeyAction::Repeat, 2, false, false),
        keyboard(x, KeyAction::Release, 0, false, false),
        Event(PaneRemoved(a)),
        Event(Detached),
        keyboard(x, KeyAction::Press, 0, false, false),
        Event(WindowDeactivated),
        keyboard(x, KeyAction::Release, 0, false, false),
        Capture(None),
        Event(OverlayClosed),
        Event(ActivatePane(a, SurfaceKind::Terminal)),
        Event(NativeFocusObserved(InputOwner::Sidebar)),
        Event(NativeFocusObserved(InputOwner::Pane(
            a,
            SurfaceKind::Terminal,
        ))),
        Event(FocusNativeEditor),
        Event(OverlayOpened),
        Event(ActivatePane(b, SurfaceKind::Browser)),
        Event(FocusSidebar),
        Event(OverlayClosed),
        Drain,
    ];
    for ch in ['a', 'b', 'c', 'd'] {
        commands.push(Suppress(KeyCode::Character(ch)));
    }
    commands.push(keyboard(
        KeyCode::Character('b'),
        KeyAction::Release,
        0,
        false,
        false,
    ));
    commands.push(Event(ActivatePane(a, SurfaceKind::Terminal)));
    for ch in ['a', 'b', 'c', 'd'] {
        commands.push(keyboard(
            KeyCode::Character(ch),
            KeyAction::Press,
            0,
            false,
            true,
        ));
    }
    commands.extend([
        keyboard(KeyCode::Character('b'), KeyAction::Release, 0, false, false),
        Suppress(KeyCode::Character('a')),
        Event(OverlayOpened),
        keyboard(KeyCode::Character('a'), KeyAction::Release, 0, false, false),
        Event(OverlayClosed),
        Command::Key(
            input(KeyCode::Character('A'), KeyAction::Press, 1, Some('a')),
            PrefixView {
                armed: false,
                claimed: true,
            },
        ),
        Command::Key(
            input(KeyCode::Character('A'), KeyAction::Release, 0, Some('a')),
            PrefixView::default(),
        ),
        Drain,
        Plain(false),
        Plain(true),
    ]);
    commands
}

fn next(seed: &mut u64) -> u64 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 7;
    *seed ^= *seed << 17;
    *seed
}

fn generated(mut seed: u64) -> Vec<Command> {
    let panes = [
        PaneId(0),
        PaneId(1),
        PaneId(0x1_0000_0001),
        PaneId(u64::MAX),
        PaneId(0x8000_0000_0000_0001),
    ];
    let keys = [
        KeyCode::Character('x'),
        KeyCode::Character('a'),
        KeyCode::Character('A'),
        KeyCode::Character('ß'),
        KeyCode::Character('\0'),
        KeyCode::Character('界'),
        KeyCode::Enter,
        KeyCode::Escape,
        KeyCode::Tab,
        KeyCode::ArrowUp,
        KeyCode::ArrowDown,
        KeyCode::Function(1),
        KeyCode::Function(255),
        KeyCode::User(u16::MAX),
        KeyCode::Unidentified,
    ];
    let mut commands = Vec::new();
    for _ in 0..128 {
        let value = next(&mut seed);
        let pane = panes[(value >> 8) as usize % panes.len()];
        let kind = [
            SurfaceKind::Terminal,
            SurfaceKind::Browser,
            SurfaceKind::Other,
        ][(value >> 16) as usize % 3];
        let key = keys[(value >> 24) as usize % keys.len()];
        commands.push(match value % 19 {
            0 => Command::Event(InputEvent::ActivatePane(pane, kind)),
            1 => Command::Event(InputEvent::SetActivePane(pane, kind)),
            2 => Command::Event(InputEvent::FocusSidebar),
            3 => Command::Event(InputEvent::FocusNativeEditor),
            4 => Command::Event(InputEvent::OverlayOpened),
            5 => Command::Event(InputEvent::OverlayClosed),
            6 => Command::Event(InputEvent::NativeFocusObserved(InputOwner::Pane(
                pane, kind,
            ))),
            7 => Command::Event(InputEvent::PaneRemoved(pane)),
            8 => Command::Event(InputEvent::Detached),
            9 => Command::Event(InputEvent::WindowDeactivated),
            10 => Command::Capture((value & 128 != 0).then_some(pane)),
            11 => Command::Suppress(key),
            12 => Command::Drain,
            13 => Command::Plain(value & 128 != 0),
            _ => Command::Key(
                input(
                    key,
                    [KeyAction::Press, KeyAction::Repeat, KeyAction::Release]
                        [(value >> 32) as usize % 3],
                    ((value >> 40) & 15) as u8,
                    (value & 64 != 0).then_some('a'),
                ),
                PrefixView {
                    armed: value & 256 != 0,
                    claimed: value & 512 != 0,
                },
            ),
        });
    }
    commands
}

fn run(name: &str, profile: usize, commands: Vec<Command>, references: &mut References) -> Value {
    let mut router = InputRouter::new(keymap(profile));
    let map = reference(0x4D41_5000, profile);
    let initial = references.state(&router, &map);
    let bindings = router.keymap.bindings().into_iter().map(|(table,key,action)|json!({"table":table,"key":key,"action":references.action(action)})).collect::<Vec<_>>();
    let mut steps = Vec::new();
    for command in commands {
        let before = references.state(&router, &map);
        let mut operation = match &command {
            Command::Event(value) => json!({"kind":"event","event":event(*value)}),
            Command::Capture(pane) => {
                json!({"kind":"capture","pane":pane.map_or_else(none,|pane|some(id(pane.0)))})
            }
            Command::Suppress(key) => json!({"kind":"suppress","key":references.key(*key)}),
            Command::Key(input, prefix) => {
                json!({"kind":"key","input":references.input(input),"prefix":{"$":"Prefix","armed":prefix.armed,"claimed":prefix.claimed},"lookup":references.lookup(&router,input)})
            }
            Command::Drain => json!({"kind":"drain"}),
            Command::Plain(releases) => {
                json!({"kind":"plain","releases":releases,"ui_binds":router.keymap.binds(UI_TABLE),"terminal_binds":router.keymap.binds(TERMINAL_TABLE)})
            }
        };
        let answer = match command {
            Command::Event(value) => {
                router.event(value);
                Value::Null
            }
            Command::Capture(pane) => {
                router.set_capture(pane);
                Value::Null
            }
            Command::Suppress(key) => {
                router.suppress_release(key);
                Value::Null
            }
            Command::Key(input, prefix) => {
                json!({"$":match router.key(&input,prefix) {Disposition::Consumed=>"Consumed",Disposition::Native=>"Native"}})
            }
            Command::Drain => references.effects(&router.drain_effects()),
            Command::Plain(releases) => json!(router.passes_plain_keys(releases)),
        };
        operation["before"] = before;
        operation["after"] = references.state(&router, &map);
        operation["answer"] = answer;
        operation["effective_owner"] = owner(router.owner());
        steps.push(operation);
    }
    json!({"name":name,"initial":initial,"bindings":bindings,"steps":steps})
}

pub fn vectors() -> Value {
    let mut references = References::default();
    let mut traces = Vec::new();
    for profile in 0..5 {
        traces.push(run(
            &format!("focused-{profile}"),
            profile,
            focused(),
            &mut references,
        ));
    }
    for profile in [0, 5, 6] {
        for (index, target) in [
            InputOwner::None,
            InputOwner::Pane(PaneId(1), SurfaceKind::Terminal),
            InputOwner::Pane(PaneId(1), SurfaceKind::Browser),
            InputOwner::Pane(PaneId(1), SurfaceKind::Other),
            InputOwner::Sidebar,
            InputOwner::Overlay,
            InputOwner::NativeEditor,
        ]
        .into_iter()
        .enumerate()
        {
            traces.push(run(
                &format!("matrix-{profile}-{index}"),
                profile,
                routing_matrix(target),
                &mut references,
            ));
        }
    }
    for seed in 1..=96 {
        traces.push(run(
            &format!("generated-{seed}"),
            seed % 5,
            generated(0xA921_6B34_05CC_103D ^ seed as u64),
            &mut references,
        ));
    }
    json!({"reference":"zz_client::InputRouter + actual ChromeKeymap","scope":"focus, ordered press claims, input routing, effects and plain-key eligibility; keymap resolution and raw key payloads are typed opaque references","traces":traces,"references":references.dictionaries()})
}
