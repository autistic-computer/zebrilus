use ala::{KeyBinding, KeyContext, Keymap, Keystroke, NoAction};
use serde_json::{Value, json};
use std::{error::Error, fs};
use zz_client::{
    ChromeAction, ChromeKeymap, ChromeProfile,
    desktop_surface::{
        BrowserNativeBinding, TERMINAL_RAW_CHORDS, browser_native_binding, sidebar_native_binding,
        terminal_native_binding, ui_native_binding, ui_percent_for_zoom, ui_zoom_for_percent,
    },
};
use zz_config::keymap::{BoundChromeChords, bound_chords, dropped_chords, native_chords};

#[derive(Clone, Debug, PartialEq, ala::Action)]
#[action(no_json, no_register)]
struct NativeIntent {
    record: usize,
}

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

fn browser_code(binding: BrowserNativeBinding) -> (u8, u8) {
    match binding {
        BrowserNativeBinding::None => (0, 0),
        BrowserNativeBinding::ZoomIn => (1, 0),
        BrowserNativeBinding::ZoomOut => (2, 0),
        BrowserNativeBinding::ZoomReset => (3, 0),
        BrowserNativeBinding::DevTools => (4, 0),
        BrowserNativeBinding::NewTab => (5, 0),
        BrowserNativeBinding::NextTab => (6, 0),
        BrowserNativeBinding::PreviousTab => (7, 0),
        BrowserNativeBinding::SelectTab(index) => (8, index),
        BrowserNativeBinding::SelectLastTab => (9, 0),
        BrowserNativeBinding::Back => (10, 0),
        BrowserNativeBinding::Forward => (11, 0),
        BrowserNativeBinding::Reload => (12, 0),
        BrowserNativeBinding::FocusAddress => (13, 0),
        BrowserNativeBinding::ElementSelector => (14, 0),
        BrowserNativeBinding::Undo => (15, 0),
        BrowserNativeBinding::Redo => (16, 0),
        BrowserNativeBinding::Cut => (17, 0),
        BrowserNativeBinding::Copy => (18, 0),
        BrowserNativeBinding::Paste => (19, 0),
        BrowserNativeBinding::PasteAndMatchStyle => (20, 0),
        BrowserNativeBinding::SelectAll => (21, 0),
        BrowserNativeBinding::ClosePane => (22, 0),
    }
}

fn key(stroke: &Keystroke) -> Value {
    let modifiers = u8::from(stroke.modifiers.shift)
        | (u8::from(stroke.modifiers.control) << 1)
        | (u8::from(stroke.modifiers.alt) << 2)
        | (u8::from(stroke.modifiers.platform) << 3);
    json!({"key": stroke.key, "modifiers": modifiers, "function": stroke.modifiers.function, "character": stroke.key_char})
}

fn classification(context: u8, action: ChromeAction) -> (u8, u8) {
    match context {
        0 => (terminal_native_binding(action) as u8, 0),
        1 => (sidebar_native_binding(action) as u8, 0),
        2 => browser_code(browser_native_binding(action)),
        _ => (ui_native_binding(action) as u8, 0),
    }
}

fn context_name(context: u8) -> &'static str {
    match context {
        0 => "Terminal",
        1 => "WorkspaceTree",
        2 => "Browser",
        _ => "PennaRoot",
    }
}

fn table(context: u8) -> &'static str {
    match context {
        0 => "terminal",
        1 => "sidebar",
        2 => "browser",
        _ => "ui",
    }
}

fn emit(
    keymap: &mut Keymap,
    output: &mut Vec<Value>,
    context: u8,
    chord: &str,
    source: &str,
    action: Option<ChromeAction>,
    binding: (u8, u8),
) -> Result<(), Box<dyn Error>> {
    let record = output.len();
    let strokes = source
        .split_whitespace()
        .map(Keystroke::parse)
        .collect::<Result<Vec<_>, _>>()?;
    output.push(json!({"context":context,"chord":chord,"action":action.map(code),"kind":binding.0,"index":binding.1,"strokes":strokes.iter().map(key).collect::<Vec<_>>()}));
    if action.is_some() {
        keymap.add_bindings([KeyBinding::new(
            source,
            NativeIntent { record },
            Some(context_name(context)),
        )]);
    } else {
        keymap.add_bindings([KeyBinding::new(
            source,
            NoAction,
            Some(context_name(context)),
        )]);
    }
    Ok(())
}

fn mutation(step: u8) -> Option<(&'static str, &'static str, Option<&'static str>)> {
    Some(match step {
        0 => return None,
        1 => ("terminal", "C-x", Some("terminal-copy")),
        2 => ("terminal", "C-x", Some("sidebar-cancel")),
        3 | 5 => ("terminal", "C-x", None),
        4 => ("terminal", "C-x", Some("terminal-paste")),
        6 => ("terminal", "fn-x", Some("terminal-search")),
        7 => ("terminal", "a b", Some("terminal-copy")),
        8 => ("terminal", "a c", Some("terminal-select-all")),
        9 => ("terminal", "a b", Some("terminal-font-increase")),
        10 => ("terminal", "a->b", Some("terminal-paste")),
        11 => ("terminal", "a", Some("terminal-clear-history")),
        12 => ("ui", "C-x", Some("ui-zoom-in")),
        13 => ("sidebar", "j", Some("sidebar-select-down")),
        14 => ("sidebar", "j", Some("terminal-copy")),
        15 => ("sidebar", "j", None),
        16 => ("browser", "C-x", Some("close-pane")),
        17 => ("browser", "C-x", Some("terminal-copy")),
        18 | 20 => ("browser", "C-x", None),
        19 => ("browser", "C-x", Some("browser-new-tab")),
        21 => ("terminal", "a\u{2003}b", Some("terminal-paste")),
        22 => ("terminal", "C-X", Some("terminal-select-all")),
        _ => ("ui", "C-x", None),
    })
}

fn main() -> Result<(), Box<dyn Error>> {
    let bases = [
        "",
        "x",
        "X",
        "é",
        "中",
        " ",
        "space",
        "-",
        "ctrl",
        "fn",
        "a->b",
        "a->b-c",
        "a->b--",
        "\0",
        "a b",
        "a\u{2003}b",
    ];
    let modifiers = [
        "",
        "ctrl-",
        "alt-",
        "shift-",
        "fn-",
        "cmd-",
        "secondary-",
        "ctrl-shift-",
        "alt-shift-fn-",
    ];
    let mut parsed = Vec::new();
    for modifier in modifiers {
        for base in bases {
            let source = format!("{modifier}{base}");
            parsed.push(
                json!({"source":source,"value":Keystroke::parse(&source).ok().as_ref().map(key)}),
            );
        }
    }
    let mut inputs = vec![Vec::new()];
    for source in [
        "ctrl-x",
        "ctrl-shift-x",
        "fn-x",
        "a",
        "b",
        "c",
        "shift-a",
        "ctrl-=",
        "ctrl-+",
        "ctrl-0",
        "tab",
        "shift-tab",
        "j",
        "é",
        "a->b",
    ] {
        inputs.push(vec![Keystroke::parse(source)?]);
    }
    for source in [["a", "b"], ["a", "c"], ["ctrl-x", "a"]] {
        inputs.push(
            source
                .into_iter()
                .map(Keystroke::parse)
                .collect::<Result<Vec<_>, _>>()?,
        );
    }
    for source in [
        "alt-shift-fn-x",
        "ctrl-alt-shift-fn-x",
        "cmd-alt-shift-fn-x",
        "x",
    ] {
        let mut stroke = Keystroke::parse(source)?;
        stroke.key_char = Some("a".into());
        inputs.push(vec![stroke]);
    }
    let mut profiles = Vec::new();
    for (profile, kind) in [
        (0_u8, ChromeProfile::Tui),
        (1, ChromeProfile::Desktop),
        (2, ChromeProfile::DesktopApple),
    ] {
        let mut map = ChromeKeymap::for_profile(kind);
        let mut previous = BoundChromeChords::new();
        let mut keymap = Keymap::default();
        let mut records = Vec::new();
        let mut steps = Vec::new();
        for step in 0..24_u8 {
            let operation = mutation(step);
            if let Some((table, key, action)) = operation {
                if let Some(action) = action {
                    map.bind(table, key, action).map_err(|error| error.0)?;
                } else {
                    map.unbind(table, key);
                }
            }
            let bound = bound_chords(&map);
            let dropped = dropped_chords(&previous, &bound);
            let first_record = records.len();
            for context in [3_u8, 2, 0, 1] {
                if step == 0 && context == 0 {
                    for source in TERMINAL_RAW_CHORDS {
                        emit(
                            &mut keymap,
                            &mut records,
                            context,
                            source,
                            source,
                            None,
                            (1, 0),
                        )?;
                    }
                }
                let mut chords = native_chords(&map, &dropped, table(context));
                if context == 0 {
                    chords.sort_by_key(|chord| !chord.key.starts_with("D-"));
                }
                for chord in chords {
                    let binding = classification(context, chord.action);
                    if binding.0 == 0 {
                        continue;
                    }
                    let action =
                        (chord.live && !(context == 0 && binding.0 == 1)).then_some(chord.action);
                    emit(
                        &mut keymap,
                        &mut records,
                        context,
                        &chord.key,
                        &chord.source,
                        action,
                        binding,
                    )?;
                }
            }
            let mut queries = Vec::new();
            for context in 0..5_u8 {
                let mut stack = Vec::new();
                if context != 4 {
                    stack.push(KeyContext::parse("PennaRoot")?);
                    if context < 3 {
                        stack.push(KeyContext::parse(context_name(context))?);
                    }
                }
                for input in &inputs {
                    let (bindings, pending) = keymap.bindings_for_input(input, &stack);
                    let matched = bindings
                        .iter()
                        .map(|binding| {
                            binding
                                .action()
                                .as_any()
                                .downcast_ref::<NativeIntent>()
                                .map(|action| action.record)
                                .ok_or("unexpected source action")
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    queries.push(json!({"context":context,"input":input.iter().map(key).collect::<Vec<_>>(),"pending":pending,"matches":matched}));
                }
            }
            steps.push(json!({"operation":operation,"bound":bound.iter().map(|((table,key),action)| json!([table,key,code(*action)])).collect::<Vec<_>>(),"dropped":dropped.iter().map(|(table,key,action)|json!([table,key,code(*action)])).collect::<Vec<_>>(),"emitted":&records[first_record..],"queries":queries}));
            previous = bound;
        }
        profiles.push(json!({"profile":profile,"steps":steps}));
    }
    let mut zoom = 1.0_f32;
    let mut zooms = Vec::new();
    for step in 0..1024_u32 {
        let operation = step % 4;
        let value =
            [50.0_f32, 300.0, 105.0, 49.0, 301.0, 100.0, 137.25][usize::try_from(step % 7)?];
        let percent = match operation {
            0 => value,
            1 => ui_percent_for_zoom(zoom) + 10.0,
            2 => ui_percent_for_zoom(zoom) - 10.0,
            _ => 100.0,
        };
        let next = ui_zoom_for_percent(percent);
        let changed = next != zoom;
        zoom = next;
        zooms.push(json!([
            operation,
            value.to_bits(),
            u8::from(changed),
            zoom.to_bits()
        ]));
    }
    fs::write(
        std::env::args().nth(1).ok_or("missing output")?,
        serde_json::to_vec(
            &json!({"macos":cfg!(target_os="macos"),"windows":cfg!(target_os="windows"),"parsed":parsed,"profiles":profiles,"zooms":zooms}),
        )?,
    )?;
    Ok(())
}
