use serde_json::{Value, json};
use std::{error::Error, fs};
use zz_client::CHROME_TABLES;
use zz_config::keymap::{
    ChromeOverride, configured_keymap, parse_bind, parse_keystroke, parse_unbind,
};

fn entry(entry: &ChromeOverride) -> Value {
    match entry {
        ChromeOverride::Bind { table, key, action } => {
            json!({"bind": true, "table": table, "key": key, "action": action})
        }
        ChromeOverride::Unbind { table, key } => {
            json!({"bind": false, "table": table, "key": key, "action": ""})
        }
    }
}

fn map(overrides: &[ChromeOverride], hotkey: &str) -> Value {
    let map = configured_keymap(overrides, hotkey);
    let mut bindings = Vec::new();
    for table in CHROME_TABLES {
        for (key, action) in map.table_bindings(table) {
            bindings.push(json!([table, key.to_string(), action.name()]));
        }
    }
    json!(bindings)
}

fn main() -> Result<(), Box<dyn Error>> {
    let bases = [
        "", "x", "X", "é", "中", " ", "-", "=", "Enter", "PageUp", "F13", "ctrl", "shift", "alt",
        "super", "fn", "a->b", "a->b-c", "a--z", "a-b", "\u{1c}", "\0", "x ", " x",
    ];
    let modifiers = [
        "",
        "ctrl-",
        "CTRL-",
        "alt-",
        "shift-",
        "secondary-",
        "cmd-",
        "super-",
        "win-",
        "fn-",
        "ctrl-shift-",
        "fn-alt-",
        "ctrl-ctrl-",
        "shift-secondary-fn-",
    ];
    let mut strokes = Vec::new();
    let mut overrides = Vec::new();
    for modifier in modifiers {
        for base in bases {
            let text = format!("{modifier}{base}");
            let value = parse_keystroke(&text).map(|stroke| json!({"key": stroke.key.to_string(), "function": stroke.function, "spelling": stroke.spelling()}));
            strokes.push(json!({"text": text, "value": value}));
            for table in [
                "ui", "sidebar", "browser", "terminal", "prefix", "invalid", " ui ",
            ] {
                for action in ["terminal-copy", "ui-zoom-in", "select-window-9", "invalid"] {
                    let text = format!("{table}:{text}={action}");
                    let value = parse_bind(&text);
                    overrides.push(json!({"text": text, "bind": true, "value": value.as_ref().ok().map(entry), "error": value.err()}));
                }
            }
        }
    }
    for text in [
        "",
        "=x",
        "ui=terminal-copy",
        "ui:=terminal-copy",
        "ui:C-==terminal-copy",
        "terminal:x=invalid=terminal-copy",
        "\u{1c}ui:x=terminal-copy",
        "ui:x=terminal-copy\u{1f}",
        "ui:x= terminal-copy ",
    ] {
        let value = parse_bind(text);
        overrides.push(json!({"text": text, "bind": true, "value": value.as_ref().ok().map(entry), "error": value.err()}));
    }
    for text in [
        "ui:x",
        "terminal:Enter",
        " browser : ctrl-c ",
        "unknown:x",
        "none",
        "ui:",
        "ui:S-X",
        "prefix:x=y",
    ] {
        let value = parse_unbind(text);
        overrides.push(json!({"text": text, "bind": false, "value": value.as_ref().ok().map(entry), "error": value.err()}));
    }
    let lines = [
        "chrome-keybind = terminal:C-x=terminal-copy",
        "chrome-keybind = ui:C-x=ui-zoom-in",
        "chrome-keybind = browser:ctrl-c=browser-copy",
        "chrome-unbind = terminal:C-x",
        "chrome-keybind = prefix:y=select-window-9 # trailing",
        "chrome-keybind = invalid:C-x=new-window",
        "browser-element-selector-hotkey = fn-shift-c",
        "browser-element-selector-hotkey = ctrl-X",
        "browser-element-selector-hotkey = x",
        "browser-element-selector-hotkey = ctrl-a->b-c",
        "picker-focus-sidebar = YES",
        "picker-focus-sidebar = false # comment",
        "picker-focus-sidebar = bad",
        "which-key-delay = 0.49999997",
        "which-key-delay = 0.5",
        "which-key-delay = 1999.5",
        "which-key-delay = 2000",
        "which-key-delay = NaN",
        "which-key-delay = inf",
        "which-key-delay = 1_0",
        "which-key-delay = 2001",
        "which-key-delay = -1",
        "which-key-delay = 1e2",
        "which-key-delay = 0.500000029802322387695312500000000000000000001",
        "which-key-delay = +0.0",
        "which-key-delay = -0.0",
        "which-key-delay = 1e-50",
        "which-key-delay = +NaN",
        "which-key-delay = -NaN",
        "which-key-delay = Infinity",
        "which-key-delay = NAN(payload)",
        "chrome-keybind = terminal:C-#=terminal-copy",
        "chrome-keybind = terminal:'x # y'=terminal-copy",
    ];
    let mut configs = Vec::new();
    for index in 0..180 {
        let mut source = String::from("# actual key configuration\n");
        for offset in 0..=(index % 13) {
            source.push_str(lines[(index * 11 + offset * 7) % lines.len()]);
            source.push('\n');
        }
        let parsed = zz_config::parse_config(&source, "System");
        configs.push(json!({"source": source, "hotkey": parsed.browser.element_selector_hotkey.value, "picker": false, "delay": parsed.config.which_key_delay.value.to_bits(), "overrides": parsed.chrome_overrides.iter().map(entry).collect::<Vec<_>>(), "diagnostics": parsed.diagnostics.iter().map(|value| json!([value.line, value.message])).collect::<Vec<_>>(), "bindings": map(&parsed.chrome_overrides, &parsed.browser.element_selector_hotkey.value)}));
    }
    let values = [
        None,
        Some(""),
        Some("relative"),
        Some("/a"),
        Some("/a/"),
        Some("/a//"),
        Some("/a/."),
        Some("/"),
    ];
    let mut candidates = Vec::new();
    for (platform_id, platform) in [
        zz_config::ConfigPlatform::Unix,
        zz_config::ConfigPlatform::Macos,
        zz_config::ConfigPlatform::Windows,
    ]
    .into_iter()
    .enumerate()
    {
        for index in 0..128 {
            let fields = [
                values[index % 8],
                values[index / 8 % 8],
                values[(index + 3) % 8],
                values[(index / 3 + 5) % 8],
                values[(index / 5 + 1) % 8],
            ];
            let environment = zz_config::ConfigEnvironment {
                xdg_config_home: fields[0].map(std::path::Path::new),
                home: fields[1].map(std::path::Path::new),
                appdata: fields[2].map(std::path::Path::new),
                local_appdata: fields[3].map(std::path::Path::new),
                user_profile: fields[4].map(std::path::Path::new),
            };
            let paths = zz_config::config_candidates_for(platform, environment)
                .into_iter()
                .map(|path| path.to_string_lossy().into_owned())
                .collect::<Vec<_>>();
            candidates
                .push(json!({"platform": platform_id, "environment": fields, "paths": paths}));
        }
    }
    fs::write(
        std::env::args().nth(1).ok_or("missing output path")?,
        serde_json::to_vec(
            &json!({"macos": cfg!(target_os = "macos"), "strokes": strokes, "overrides": overrides, "configs": configs, "candidates": candidates, "identity": zz_config::CONFIG_DIRECTORY_NAME}),
        )?,
    )?;
    Ok(())
}
