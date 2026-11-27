use super::test_support::*;
use super::*;
use ala::{AsKeystroke, KeyContext, Keymap, Keystroke};
use std::{any::TypeId, collections::HashSet};

fn assert_binding<A: ala::Action>(keymap: &Keymap, source: &str) {
    let bindings = browser_bindings_for(keymap, source);
    assert_eq!(bindings.len(), 1, "expected one binding for {source}");
    assert_eq!(bindings[0].action().as_any().type_id(), TypeId::of::<A>());
}

fn browser_binding_identity(binding: &KeyBinding) -> String {
    let action = binding.action();
    if let Some(command) = edit_action_command(action) {
        return format!("edit:{command:?}");
    }
    if let Some(action) = action.as_any().downcast_ref::<SelectTab>() {
        return format!("tab:{}", action.index);
    }
    if action
        .as_any()
        .downcast_ref::<ConfiguredElementSelector>()
        .is_some()
    {
        return "element-selector".to_owned();
    }
    for (type_id, name) in [
        (TypeId::of::<NoAction>(), "raw-tab"),
        (TypeId::of::<ZoomIn>(), "zoom-in"),
        (TypeId::of::<ZoomOut>(), "zoom-out"),
        (TypeId::of::<ResetZoom>(), "zoom-reset"),
        (TypeId::of::<ToggleDevTools>(), "devtools"),
        (TypeId::of::<NewTab>(), "new-tab"),
        (TypeId::of::<ClosePane>(), "close-pane"),
        (TypeId::of::<NextTab>(), "next-tab"),
        (TypeId::of::<PreviousTab>(), "previous-tab"),
        (TypeId::of::<SelectLastTab>(), "last-tab"),
        (TypeId::of::<FocusAddress>(), "focus-address"),
        (TypeId::of::<Reload>(), "reload"),
        (TypeId::of::<HardReload>(), "hard-reload"),
        (TypeId::of::<crate::menus::Find>(), "find"),
        (TypeId::of::<GoBack>(), "back"),
        (TypeId::of::<GoForward>(), "forward"),
        (TypeId::of::<OmniboxNext>(), "omnibox-next"),
        (TypeId::of::<OmniboxPrevious>(), "omnibox-previous"),
        (TypeId::of::<OmniboxDelete>(), "omnibox-delete"),
    ] {
        if action.as_any().type_id() == type_id {
            return name.to_owned();
        }
    }
    panic!("unrecognized browser binding action: {}", action.name());
}

fn audited_binding(source: &str, action: &str) -> (Keystroke, String) {
    (
        Keystroke::parse(source).expect("valid audited browser keystroke"),
        action.to_owned(),
    )
}

#[test]
fn page_zoom_shortcuts_use_the_platform_browser_convention() {
    let keymap = Keymap::new(browser_key_bindings());
    #[cfg(target_os = "macos")]
    let (zoom_in, zoom_plus, zoom_out, reset) = ("cmd-=", "cmd-+", "cmd--", "cmd-0");
    #[cfg(not(target_os = "macos"))]
    let (zoom_in, zoom_plus, zoom_out, reset) = ("ctrl-=", "ctrl-+", "ctrl--", "ctrl-0");

    assert_binding::<ZoomIn>(&keymap, zoom_in);
    assert_binding::<ZoomIn>(&keymap, zoom_plus);
    assert_binding::<ZoomOut>(&keymap, zoom_out);
    assert_binding::<ResetZoom>(&keymap, reset);
}

#[test]
fn browser_keymap_matches_the_audited_supported_set() {
    #[cfg(target_os = "macos")]
    let expected = [
        ("tab", "raw-tab"),
        ("shift-tab", "raw-tab"),
        ("cmd-z", "edit:Undo"),
        ("cmd-shift-z", "edit:Redo"),
        ("cmd-x", "edit:Cut"),
        ("cmd-c", "edit:Copy"),
        ("cmd-v", "edit:Paste"),
        ("cmd-shift-v", "edit:PasteAndMatchStyle"),
        ("cmd-a", "edit:SelectAll"),
        ("cmd-=", "zoom-in"),
        ("cmd-+", "zoom-in"),
        ("cmd--", "zoom-out"),
        ("cmd-0", "zoom-reset"),
        ("cmd-alt-i", "devtools"),
        ("cmd-t", "new-tab"),
        ("ctrl-tab", "next-tab"),
        ("ctrl-shift-tab", "previous-tab"),
        ("cmd-alt-right", "next-tab"),
        ("cmd-alt-left", "previous-tab"),
        ("cmd-shift-]", "next-tab"),
        ("cmd-shift-[", "previous-tab"),
        ("cmd-9", "last-tab"),
        ("cmd-l", "focus-address"),
        ("cmd-r", "reload"),
        ("cmd-shift-r", "hard-reload"),
        ("cmd-f", "find"),
        ("cmd-[", "back"),
        ("cmd-]", "forward"),
        ("cmd-shift-c", "element-selector"),
        ("cmd-1", "tab:0"),
        ("cmd-2", "tab:1"),
        ("cmd-3", "tab:2"),
        ("cmd-4", "tab:3"),
        ("cmd-5", "tab:4"),
        ("cmd-6", "tab:5"),
        ("cmd-7", "tab:6"),
        ("cmd-8", "tab:7"),
        ("down", "omnibox-next"),
        ("up", "omnibox-previous"),
        ("shift-delete", "omnibox-delete"),
    ];
    #[cfg(not(target_os = "macos"))]
    let expected = [
        ("tab", "raw-tab"),
        ("shift-tab", "raw-tab"),
        ("ctrl-z", "edit:Undo"),
        ("ctrl-y", "edit:Redo"),
        ("ctrl-shift-z", "edit:Redo"),
        ("ctrl-x", "edit:Cut"),
        ("ctrl-c", "edit:Copy"),
        ("ctrl-v", "edit:Paste"),
        ("ctrl-shift-v", "edit:PasteAndMatchStyle"),
        ("ctrl-a", "edit:SelectAll"),
        ("ctrl-=", "zoom-in"),
        ("ctrl-+", "zoom-in"),
        ("ctrl--", "zoom-out"),
        ("ctrl-0", "zoom-reset"),
        ("ctrl-shift-i", "devtools"),
        ("ctrl-t", "new-tab"),
        ("ctrl-w", "close-pane"),
        ("ctrl-tab", "next-tab"),
        ("ctrl-shift-tab", "previous-tab"),
        ("ctrl-pagedown", "next-tab"),
        ("ctrl-pageup", "previous-tab"),
        ("ctrl-9", "last-tab"),
        ("ctrl-l", "focus-address"),
        ("ctrl-r", "reload"),
        ("ctrl-shift-r", "hard-reload"),
        ("ctrl-f", "find"),
        ("f5", "reload"),
        ("alt-left", "back"),
        ("alt-right", "forward"),
        ("ctrl-shift-c", "element-selector"),
        ("ctrl-1", "tab:0"),
        ("ctrl-2", "tab:1"),
        ("ctrl-3", "tab:2"),
        ("ctrl-4", "tab:3"),
        ("ctrl-5", "tab:4"),
        ("ctrl-6", "tab:5"),
        ("ctrl-7", "tab:6"),
        ("ctrl-8", "tab:7"),
        ("down", "omnibox-next"),
        ("up", "omnibox-previous"),
        ("shift-delete", "omnibox-delete"),
    ];
    let bindings = browser_key_bindings();
    let actual = bindings
        .iter()
        .map(|binding| {
            let [keystroke] = binding.keystrokes() else {
                panic!("browser bindings must be single keystrokes");
            };
            (
                keystroke.as_keystroke().clone(),
                browser_binding_identity(binding),
            )
        })
        .collect::<HashSet<_>>();
    let expected = expected
        .into_iter()
        .map(|(source, action)| audited_binding(source, action))
        .collect::<HashSet<_>>();

    assert_eq!(actual, expected);
    assert_eq!(bindings.len(), expected.len());
}

#[test]
fn page_zoom_outranks_application_ui_scaling_in_the_browser_context() {
    let mut bindings =
        crate::ui_scale::key_bindings(&crate::keymap::test_chords(zz_client::UI_TABLE));
    bindings.extend(browser_key_bindings());
    let keymap = Keymap::new(bindings);
    let contexts = [
        KeyContext::parse(penna::ROOT_KEY_CONTEXT).expect("valid zz root context"),
        KeyContext::parse(BROWSER_KEY_CONTEXT).expect("valid browser context"),
    ];
    #[cfg(target_os = "macos")]
    let shortcuts = [
        ("cmd-=", TypeId::of::<ZoomIn>()),
        ("cmd-+", TypeId::of::<ZoomIn>()),
        ("cmd--", TypeId::of::<ZoomOut>()),
        ("cmd-0", TypeId::of::<ResetZoom>()),
    ];
    #[cfg(not(target_os = "macos"))]
    let shortcuts = [
        ("ctrl-=", TypeId::of::<ZoomIn>()),
        ("ctrl-+", TypeId::of::<ZoomIn>()),
        ("ctrl--", TypeId::of::<ZoomOut>()),
        ("ctrl-0", TypeId::of::<ResetZoom>()),
    ];

    for (source, action_type) in shortcuts {
        let keystroke = Keystroke::parse(source).expect("valid zoom shortcut");
        let (bindings, pending) = keymap.bindings_for_input(&[keystroke], &contexts);
        assert!(!pending);
        assert_eq!(bindings[0].action().as_any().type_id(), action_type);
    }
}

#[test]
fn tab_shortcuts_use_the_platform_browser_convention() {
    let keymap = Keymap::new(browser_key_bindings());

    assert_binding::<NextTab>(&keymap, "ctrl-tab");
    assert_binding::<PreviousTab>(&keymap, "ctrl-shift-tab");
    #[cfg(target_os = "macos")]
    {
        assert_binding::<NewTab>(&keymap, "cmd-t");
        assert!(browser_bindings_for(&keymap, "cmd-w").is_empty());
        assert_binding::<NextTab>(&keymap, "cmd-alt-right");
        assert_binding::<PreviousTab>(&keymap, "cmd-alt-left");
        assert_binding::<NextTab>(&keymap, "cmd-shift-]");
        assert_binding::<PreviousTab>(&keymap, "cmd-shift-[");
        assert_binding::<SelectLastTab>(&keymap, "cmd-9");
    }
    #[cfg(not(target_os = "macos"))]
    {
        assert_binding::<NewTab>(&keymap, "ctrl-t");
        assert_binding::<ClosePane>(&keymap, "ctrl-w");
        assert_binding::<NextTab>(&keymap, "ctrl-pagedown");
        assert_binding::<PreviousTab>(&keymap, "ctrl-pageup");
        assert_binding::<SelectLastTab>(&keymap, "ctrl-9");
    }
}

#[test]
fn navigation_shortcuts_use_the_platform_browser_convention() {
    let keymap = Keymap::new(browser_key_bindings());

    #[cfg(target_os = "macos")]
    let (back, forward, reload, address) = ("cmd-[", "cmd-]", "cmd-r", "cmd-l");
    #[cfg(not(target_os = "macos"))]
    let (back, forward, reload, address) = ("alt-left", "alt-right", "ctrl-r", "ctrl-l");

    assert_binding::<GoBack>(&keymap, back);
    assert_binding::<GoForward>(&keymap, forward);
    assert_binding::<Reload>(&keymap, reload);
    assert_binding::<FocusAddress>(&keymap, address);
}

#[test]
fn configured_element_selector_binding_is_browser_scoped_and_carries_its_chord() {
    let hotkey = crate::config::DEFAULT_BROWSER_ELEMENT_SELECTOR_HOTKEY;
    let keymap = Keymap::new(browser_key_bindings());
    let bindings = browser_bindings_for(&keymap, hotkey);
    let chord = if cfg!(target_os = "macos") {
        "D-S-c"
    } else {
        "C-S-c"
    };

    assert_eq!(bindings.len(), 1);
    assert_eq!(
        bindings[0]
            .action()
            .as_any()
            .downcast_ref::<ConfiguredElementSelector>(),
        Some(&ConfiguredElementSelector {
            chord: chord.to_owned()
        }),
    );
}

#[cfg(target_os = "macos")]
#[test]
fn cmd_w_resolves_to_close_pane_in_every_pane_context() {
    let mut bindings = browser_key_bindings();
    bindings.extend(crate::macos_app::key_bindings());
    let keymap = Keymap::new(bindings);

    for pane_context in ["Browser", "Terminal", "Editor", "Agent"] {
        let contexts = [
            KeyContext::parse("Root").expect("valid root context"),
            KeyContext::parse(pane_context).expect("valid pane context"),
        ];
        let keystroke = Keystroke::parse("cmd-w").expect("valid keystroke");
        let (bindings, pending) = keymap.bindings_for_input(&[keystroke], &contexts);
        assert!(!pending);
        assert_eq!(
            bindings[0].action().as_any().type_id(),
            TypeId::of::<ClosePane>(),
            "cmd-w must dispatch ClosePane in the {pane_context} context"
        );
    }
}

#[test]
fn digit_shortcuts_jump_straight_to_tab_slots() {
    let keymap = Keymap::new(browser_key_bindings());
    #[cfg(target_os = "macos")]
    let modifier = "cmd";
    #[cfg(not(target_os = "macos"))]
    let modifier = "ctrl";

    for index in 0..8 {
        let source = format!("{modifier}-{}", index + 1);
        let bindings = browser_bindings_for(&keymap, &source);
        assert_eq!(bindings.len(), 1, "expected one binding for {source}");
        assert_eq!(
            bindings[0].action().as_any().downcast_ref::<SelectTab>(),
            Some(&SelectTab { index }),
        );
    }
}
