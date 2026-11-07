use std::{
    error::Error,
    fs::File,
    io::{self, BufWriter, Write},
};

use serde_json::{Value, json};
use zz_client::{ChromeAction, ChromeKey, ChromeKeymap, ChromeProfile};
use zz_protocol::{InputMessage, PaneId, ProtocolMessage, encode_protocol_message};
use zz_terminal::{KeyAction, KeyCode, KeyInput, Modifiers};

fn code(value: ChromeAction) -> u32 {
    match value {
        ChromeAction::NewSession => 0,
        ChromeAction::NewWindow => 256,
        ChromeAction::SelectWindow(index) => 2 * 256 + u32::from(index),
        ChromeAction::SplitRight => 768,
        ChromeAction::SplitDown => 1024,
        ChromeAction::Detach => 1280,
        ChromeAction::ToggleSidebar => 1536,
        ChromeAction::ClosePane => 1792,
        ChromeAction::OpenSettings => 2048,
        ChromeAction::OpenCommandPalette => 2304,
        ChromeAction::UiZoomIn => 2560,
        ChromeAction::UiZoomOut => 2816,
        ChromeAction::UiZoomReset => 3072,
        ChromeAction::SidebarCancel => 3328,
        ChromeAction::SidebarConfirm => 3584,
        ChromeAction::SidebarRename => 3840,
        ChromeAction::SidebarCommandPalette => 4096,
        ChromeAction::SidebarSelectUp => 4352,
        ChromeAction::SidebarSelectDown => 4608,
        ChromeAction::SidebarSelectLeft => 4864,
        ChromeAction::SidebarSelectRight => 5120,
        ChromeAction::SidebarSelectFirst => 5376,
        ChromeAction::SidebarSelectLast => 5632,
        ChromeAction::TerminalFontIncrease => 5888,
        ChromeAction::TerminalFontDecrease => 6144,
        ChromeAction::TerminalSearch => 6400,
        ChromeAction::TerminalCopy => 6656,
        ChromeAction::TerminalSelectAll => 6912,
        ChromeAction::TerminalClearHistory => 7168,
        ChromeAction::TerminalPaste => 7424,
        ChromeAction::BrowserZoomIn => 7680,
        ChromeAction::BrowserZoomOut => 7936,
        ChromeAction::BrowserZoomReset => 8192,
        ChromeAction::BrowserDevTools => 8448,
        ChromeAction::BrowserNewTab => 8704,
        ChromeAction::BrowserNextTab => 8960,
        ChromeAction::BrowserPreviousTab => 9216,
        ChromeAction::BrowserSelectTab(index) => 37 * 256 + u32::from(index),
        ChromeAction::BrowserSelectLastTab => 9728,
        ChromeAction::BrowserBack => 9984,
        ChromeAction::BrowserForward => 10240,
        ChromeAction::BrowserReload => 10496,
        ChromeAction::BrowserFocusAddress => 10752,
        ChromeAction::BrowserElementSelector => 11008,
        ChromeAction::BrowserUndo => 11264,
        ChromeAction::BrowserRedo => 11520,
        ChromeAction::BrowserCut => 11776,
        ChromeAction::BrowserCopy => 12032,
        ChromeAction::BrowserPaste => 12288,
        ChromeAction::BrowserPasteAndMatchStyle => 12544,
        ChromeAction::BrowserSelectAll => 12800,
    }
}

fn action(value: Option<ChromeAction>) -> i64 {
    value.map_or(-1, |value| i64::from(code(value)))
}

fn parsed(value: &str) -> Value {
    ChromeKey::parse(value).map_or(
        Value::Null,
        |key| json!({"modifiers":bits(&key),"base":key.base,"display":key.to_string()}),
    )
}

fn bits(key: &ChromeKey) -> u8 {
    u8::from(key.shift)
        | (u8::from(key.control) << 1)
        | (u8::from(key.alt) << 2)
        | (u8::from(key.command) << 3)
}

fn all_actions() -> Vec<ChromeAction> {
    let mut values = vec![
        ChromeAction::NewSession,
        ChromeAction::NewWindow,
        ChromeAction::SplitRight,
        ChromeAction::SplitDown,
        ChromeAction::Detach,
        ChromeAction::ToggleSidebar,
        ChromeAction::ClosePane,
        ChromeAction::OpenSettings,
        ChromeAction::OpenCommandPalette,
        ChromeAction::UiZoomIn,
        ChromeAction::UiZoomOut,
        ChromeAction::UiZoomReset,
        ChromeAction::SidebarCancel,
        ChromeAction::SidebarConfirm,
        ChromeAction::SidebarRename,
        ChromeAction::SidebarCommandPalette,
        ChromeAction::SidebarSelectUp,
        ChromeAction::SidebarSelectDown,
        ChromeAction::SidebarSelectLeft,
        ChromeAction::SidebarSelectRight,
        ChromeAction::SidebarSelectFirst,
        ChromeAction::SidebarSelectLast,
        ChromeAction::TerminalFontIncrease,
        ChromeAction::TerminalFontDecrease,
        ChromeAction::TerminalSearch,
        ChromeAction::TerminalCopy,
        ChromeAction::TerminalSelectAll,
        ChromeAction::TerminalClearHistory,
        ChromeAction::TerminalPaste,
        ChromeAction::BrowserZoomIn,
        ChromeAction::BrowserZoomOut,
        ChromeAction::BrowserZoomReset,
        ChromeAction::BrowserDevTools,
        ChromeAction::BrowserNewTab,
        ChromeAction::BrowserNextTab,
        ChromeAction::BrowserPreviousTab,
        ChromeAction::BrowserSelectLastTab,
        ChromeAction::BrowserBack,
        ChromeAction::BrowserForward,
        ChromeAction::BrowserReload,
        ChromeAction::BrowserFocusAddress,
        ChromeAction::BrowserElementSelector,
        ChromeAction::BrowserUndo,
        ChromeAction::BrowserRedo,
        ChromeAction::BrowserCut,
        ChromeAction::BrowserCopy,
        ChromeAction::BrowserPaste,
        ChromeAction::BrowserPasteAndMatchStyle,
        ChromeAction::BrowserSelectAll,
    ];
    for index in 0..=u8::MAX {
        values.push(ChromeAction::SelectWindow(index));
        values.push(ChromeAction::BrowserSelectTab(index));
    }
    values.sort_by_key(|value| code(*value));
    values
}

fn profiles() -> [ChromeProfile; 3] {
    [
        ChromeProfile::Tui,
        ChromeProfile::Desktop,
        ChromeProfile::DesktopApple,
    ]
}

fn tables() -> [&'static str; 9] {
    [
        "ui", "sidebar", "terminal", "browser", "prefix", "custom", "missing", "", "空\0",
    ]
}

fn snapshot(map: &ChromeKeymap) -> Value {
    let bindings = map
        .bindings()
        .into_iter()
        .map(|(table, key, value)| json!([table, key, code(value)]))
        .collect::<Vec<_>>();
    let tables = tables().into_iter().map(|table| json!({"name":table,"present":map.binds(table),"bindings":map.table_bindings(table).into_iter().map(|(key,value)|json!([bits(&key),key.base,key.to_string(),code(value)])).collect::<Vec<_>>()})).collect::<Vec<_>>();
    json!({"bindings":bindings,"tables":tables})
}

fn input(
    key: KeyCode,
    bits: u8,
    action: KeyAction,
    text: Option<&str>,
    unshifted: Option<char>,
) -> KeyInput {
    KeyInput {
        key,
        action,
        modifiers: Modifiers::from_bits(bits).expect("fixture modifiers are valid"),
        text: text.map(Into::into),
        unshifted_codepoint: unshifted,
    }
}

fn inputs() -> Vec<KeyInput> {
    let mut keys = (0..=127)
        .map(|n| KeyCode::Character(char::from(n)))
        .collect::<Vec<_>>();
    keys.extend(
        [
            'é', '界', '\u{85}', '\u{a0}', '\u{1680}', '\u{2003}', '\u{2028}', '\u{202f}',
            '\u{205f}', '\u{3000}',
        ]
        .into_iter()
        .map(KeyCode::Character),
    );
    keys.extend([
        KeyCode::Backspace,
        KeyCode::Enter,
        KeyCode::Tab,
        KeyCode::Escape,
        KeyCode::Delete,
        KeyCode::Insert,
        KeyCode::Home,
        KeyCode::End,
        KeyCode::PageUp,
        KeyCode::PageDown,
        KeyCode::ArrowUp,
        KeyCode::ArrowDown,
        KeyCode::ArrowLeft,
        KeyCode::ArrowRight,
        KeyCode::Function(0),
        KeyCode::Function(5),
        KeyCode::Function(255),
        KeyCode::User(0),
        KeyCode::User(u16::MAX),
        KeyCode::Unidentified,
    ]);
    let mut result = Vec::new();
    for key in keys {
        for bits in 0..16 {
            for action in [KeyAction::Press, KeyAction::Repeat, KeyAction::Release] {
                result.push(input(key, bits, action, None, None));
            }
        }
    }
    for key in [
        KeyCode::Character('a'),
        KeyCode::Character('A'),
        KeyCode::Character(' '),
        KeyCode::Enter,
        KeyCode::Tab,
        KeyCode::Unidentified,
        KeyCode::Function(5),
        KeyCode::User(u16::MAX),
    ] {
        for bits in 0..16 {
            for action in [KeyAction::Press, KeyAction::Repeat, KeyAction::Release] {
                for text in ["a", "A", "é", " ", "\0", "\u{1c}", "ab", "\u{2003}"] {
                    for unshifted in [None, Some('a')] {
                        result.push(input(key, bits, action, Some(text), unshifted));
                    }
                }
            }
        }
    }
    result
}

#[derive(Clone)]
enum Op {
    Bind(String, String, String),
    Unbind(String, String),
    ActionFor(String, String),
    Resolve(String, usize),
}

fn run(profile: ChromeProfile, operations: Vec<Op>, inputs: &[KeyInput]) -> Value {
    let mut map = ChromeKeymap::for_profile(profile);
    let initial = snapshot(&map);
    let mut steps = Vec::new();
    for operation in operations {
        let before = snapshot(&map);
        let (request, answer) = match operation {
            Op::Bind(table, key, value) => {
                let error = map.bind(&table, &key, &value).err().map(|error| error.0);
                (json!([0, table, key, value]), json!(error))
            }
            Op::Unbind(table, key) => {
                let removed = map.unbind(&table, &key);
                (json!([1, table, key]), json!(removed))
            }
            Op::ActionFor(table, key) => (
                json!([2, table, key]),
                json!(action(map.action_for(&table, &key))),
            ),
            Op::Resolve(table, index) => (
                json!([3, table, index]),
                json!(action(map.resolve(&table, &inputs[index]))),
            ),
        };
        let after = snapshot(&map);
        steps
            .push(json!({"request":request,"answer":answer,"changed":before!=after,"after":after}));
    }
    let resolved = tables().into_iter().map(|table| json!({"table":table,"actions":inputs.iter().map(|input|action(map.resolve(table,input))).collect::<Vec<_>>()})).collect::<Vec<_>>();
    json!({"initial":initial,"steps":steps,"after":snapshot(&map),"resolved":resolved})
}

fn spellings() -> Vec<String> {
    let mut values = [
        "",
        " ",
        "\t",
        "\u{2003}",
        "D-",
        "Ctrl-",
        "Alt-",
        "Shift-C-",
        " C-a ",
        "C- ",
        "Space",
        "none",
        "NONE",
        "nOnE",
        "C-S-A",
        "S-a",
        "S-A",
        "D-A",
        "Ctrl-Shift-F",
        "C-M-x",
        "M-C-x",
        "C-C-M-S-D-a",
        "S-Tab",
        "BTab",
        "Shift-BTab",
        "Cmd-Super-x",
        "c-x",
        "C-\u{1c}",
        "C-\u{85}",
        "C- x",
        "D- 空\0",
        "C-\0",
        "C-\u{2003}x",
        "é",
        "界",
        "a\u{301}",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect::<Vec<_>>();
    for prefix in [
        "",
        "D-",
        "C-",
        "M-",
        "S-",
        "Ctrl-Alt-Shift-",
        "Cmd-Ctrl-Alt-Shift-",
        "Super-S-C-M-",
        "S-M-C-D-",
    ] {
        for base in [
            "a",
            "A",
            "z",
            "Z",
            "+",
            "-",
            "=",
            " ",
            "Space",
            "Enter",
            "Tab",
            "F0",
            "F255",
            "F256",
            "User65535",
            "Any",
            "None",
            "é",
            "É",
            "界",
            "a\u{301}",
            "\0",
            "\u{1c}",
            "\u{85}",
            "\u{2003}",
            "S-x",
        ] {
            values.push(format!("{prefix}{base}"));
        }
    }
    for point in 0..=255 {
        let character = char::from_u32(point).expect("byte is a scalar");
        values.push(format!("{character}C-a{character}"));
    }
    for character in [
        '\u{1680}', '\u{2000}', '\u{2003}', '\u{2028}', '\u{2029}', '\u{202f}', '\u{205f}',
        '\u{3000}',
    ] {
        values.push(format!("{character}C-a{character}"));
    }
    values
}

fn main() -> Result<(), Box<dyn Error>> {
    let inputs = inputs();
    let raw_inputs = inputs
        .iter()
        .map(|input| {
            let frame = encode_protocol_message(&ProtocolMessage::Input(InputMessage::Key {
                pane: PaneId(7),
                input: input.clone(),
                text_follows: false,
            }))
            .expect("fixture input serializes");
            json!({"wire":frame,"key":input.key,"unshifted":input.unshifted_codepoint})
        })
        .collect::<Vec<_>>();
    let actions = all_actions().into_iter().map(|value|json!({"code":code(value),"name":value.name(),"parsed":action(ChromeAction::from_name(value.name()))})).collect::<Vec<_>>();
    let mut names = actions
        .iter()
        .map(|value| value["name"].as_str().expect("action name").to_owned())
        .collect::<Vec<_>>();
    for prefix in ["select-window-", "browser-select-tab-"] {
        for suffix in [
            "",
            "0",
            "1",
            "8",
            "9",
            "10",
            "255",
            "256",
            "+1",
            "+0008",
            "000000000000000001",
            "-1",
            " 1",
            "1 ",
            "++1",
            "١",
            "\0",
        ] {
            names.push(format!("{prefix}{suffix}"));
        }
    }
    names.extend(
        ["New-Session", "", " unknown", "terminal-copy "]
            .into_iter()
            .map(str::to_owned),
    );
    let names = names
        .into_iter()
        .map(|name| json!({"action":action(ChromeAction::from_name(&name)),"name":name}))
        .collect::<Vec<_>>();
    let parsed = spellings()
        .into_iter()
        .map(|value| json!({"value":parsed(&value),"spelling":value}))
        .collect::<Vec<_>>();
    let mut normalized = Vec::new();
    for flags in 0..16 {
        for base in [
            "a", "A", "z", "Z", "é", "É", "界", "a\u{301}", "", "a ", "\0", "\u{1c}",
        ] {
            let key = ChromeKey {
                command: flags & 8 != 0,
                control: flags & 2 != 0,
                alt: flags & 4 != 0,
                shift: flags & 1 != 0,
                base: base.to_owned(),
            }
            .normalized();
            normalized.push(json!({"flags":flags,"base":base,"modifiers":bits(&key),"normalized":key.base,"display":key.to_string()}));
        }
    }
    let mut cases = Vec::new();
    for profile in profiles() {
        cases.push(run(profile, Vec::new(), &inputs));
    }
    let binding_keys = [
        "Any",
        "a",
        "A",
        "S-a",
        "C-a",
        "C-S-a",
        "D-a",
        "D-S-a",
        "é",
        " ",
        "Space",
        "Ctrl-",
        "Shift-C-",
        "C-\u{1c}",
        "C-\0",
        "none",
        "NONE",
        "D-M-S-C-A",
        "S-Tab",
        "BTab",
        "",
    ];
    let mut operations = Vec::new();
    for (index, key) in binding_keys.into_iter().enumerate() {
        let table = tables()[index % 9];
        let value = if index % 2 == 0 {
            "detach"
        } else {
            "browser-reload"
        };
        operations.push(Op::Bind(table.to_owned(), key.to_owned(), value.to_owned()));
        operations.push(Op::ActionFor(table.to_owned(), key.to_owned()));
        operations.push(Op::Bind(table.to_owned(), key.to_owned(), value.to_owned()));
        operations.push(Op::Bind(
            table.to_owned(),
            key.to_owned(),
            "no-such-action".to_owned(),
        ));
    }
    for profile in profiles() {
        cases.push(run(profile, operations.clone(), &inputs));
    }
    let mut operations = Vec::new();
    for key in [
        "Any", "a", "é", "A", "S-A", "C-S-a", "D-a", "D-S-a", "BTab", "S-Tab", "C-\u{1c}", "C-\0",
        "", "Ctrl-", "Shift-C-", "Space",
    ] {
        operations.push(Op::Bind(
            "custom".to_owned(),
            key.to_owned(),
            "new-session".to_owned(),
        ));
        operations.push(Op::Bind(
            "custom".to_owned(),
            key.to_owned(),
            "detach".to_owned(),
        ));
        operations.push(Op::ActionFor("custom".to_owned(), key.to_owned()));
    }
    cases.push(run(ChromeProfile::Desktop, operations.clone(), &inputs));
    for key in [
        "Any", "a", "é", "A", "S-A", "C-S-a", "D-a", "D-S-a", "BTab", "S-Tab", "C-\u{1c}", "C-\0",
        "", "Ctrl-", "Shift-C-", "Space",
    ] {
        operations.push(Op::Unbind("custom".to_owned(), key.to_owned()));
        operations.push(Op::Unbind("custom".to_owned(), key.to_owned()));
    }
    cases.push(run(ChromeProfile::Tui, operations, &inputs));
    let mut seed = 0x1234_5678_9abc_def0u64;
    let mut operations = Vec::new();
    for index in 0..256 {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let table = tables()[seed as usize % 9].to_owned();
        let key = binding_keys[(seed >> 8) as usize % binding_keys.len()].to_owned();
        operations.push(match index % 4 {
            0 => Op::Bind(
                table,
                key,
                if seed & 1 == 0 {
                    "terminal-copy"
                } else {
                    "select-window-+01"
                }
                .to_owned(),
            ),
            1 => Op::Unbind(table, key),
            2 => Op::ActionFor(table, key),
            _ => Op::Resolve(table, (seed >> 24) as usize % inputs.len()),
        });
    }
    cases.push(run(ChromeProfile::DesktopApple, operations, &inputs));
    let defaults = profiles()
        .into_iter()
        .map(|profile| {
            ChromeKeymap::for_profile(profile)
                .bindings()
                .into_iter()
                .map(|(table, key, action)| json!([table, key, code(action)]))
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let value = json!({"reference":"actual ChromeKey/ChromeKeymap/ChromeAction and protocol KeyTables","desktop":if matches!(ChromeProfile::DESKTOP,ChromeProfile::DesktopApple){2}else{1},"actions":actions,"names":names,"parsed":parsed,"normalized":normalized,"inputs":raw_inputs,"defaults":defaults,"profiles":[0,1,2,0,1,2,1,0,2],"cases":cases});
    let path = std::env::args_os().nth(1);
    if let Some(path) = path {
        let mut output = BufWriter::new(File::create(path)?);
        serde_json::to_writer(&mut output, &value)?;
        output.write_all(b"\n")?;
        output.flush()?;
    } else {
        let mut output = io::stdout().lock();
        serde_json::to_writer(&mut output, &value)?;
        output.write_all(b"\n")?;
    }
    Ok(())
}
