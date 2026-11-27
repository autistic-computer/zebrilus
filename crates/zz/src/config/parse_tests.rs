use crate::keymap::ChromeOverride;
use std::fs;

use super::test_support::*;
use super::*;

#[test]
fn config_defaults_match_the_default_theme() {
    let defaults = zz_config::AppConfig::default();
    let theme = interface_theme::default_theme();
    assert_eq!(
        (defaults.chrome_tint_hue.value, defaults.chrome_tint.value),
        (theme.neutral.hue, theme.neutral.amount)
    );
}

#[test]
fn parser_applies_the_browser_search_provider() {
    let parsed = parse_config("browser-search-provider = duckduckgo\n");

    assert!(parsed.diagnostics.is_empty());
    assert_eq!(
        parsed.browser.search_provider,
        ConfigValue {
            value: SearchProvider::DuckDuckGo,
            provenance: ConfigProvenance::Override,
        }
    );
}

#[test]
fn browser_search_provider_rejects_unknown_engines() {
    let parsed = parse_config("browser-search-provider = bing\n");

    assert_eq!(parsed.diagnostics.len(), 1);
    assert_eq!(
        parsed.browser.search_provider.value,
        DEFAULT_BROWSER_SEARCH_PROVIDER
    );
    assert_eq!(
        parsed.browser.search_provider.provenance,
        ConfigProvenance::Override
    );
}

#[test]
fn parser_collects_chrome_binding_overrides() {
    let parsed = parse_config(
        "chrome-keybind = browser:Cmd-Shift-p=browser-new-tab\n\
         chrome-keybind = terminal:C-S-y=terminal-copy  # a note\n\
         chrome-unbind = sidebar:q\n",
    );

    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    assert_eq!(
        parsed.chrome_overrides,
        vec![
            ChromeOverride::Bind {
                table: "browser",
                key: "D-S-p".to_owned(),
                action: "browser-new-tab".to_owned(),
            },
            ChromeOverride::Bind {
                table: "terminal",
                key: "C-S-y".to_owned(),
                action: "terminal-copy".to_owned(),
            },
            ChromeOverride::Unbind {
                table: "sidebar",
                key: "q".to_owned(),
            },
        ]
    );
}

#[test]
fn chrome_binding_overrides_report_what_they_cannot_honour() {
    let parsed = parse_config(
        "chrome-keybind = browser:D-t\n\
         chrome-keybind = pane:D-t=browser-new-tab\n\
         chrome-keybind = browser:D-t=teleport\n\
         chrome-unbind = browser\n",
    );

    assert_eq!(parsed.diagnostics.len(), 4);
    assert!(parsed.chrome_overrides.is_empty());
    assert!(
        parsed.diagnostics[1]
            .message
            .contains("unknown chrome table"),
        "{:?}",
        parsed.diagnostics[1],
    );
    assert!(
        parsed.diagnostics[2]
            .message
            .contains("unknown chrome action"),
        "{:?}",
        parsed.diagnostics[2],
    );
}

#[test]
fn parser_applies_and_normalizes_the_browser_element_selector_hotkey() {
    let parsed = parse_config("browser-element-selector-hotkey = shift-alt-e\n");

    assert!(parsed.diagnostics.is_empty());
    assert_eq!(
        parsed.browser.element_selector_hotkey.value,
        normalize_browser_hotkey("shift-alt-e").expect("valid hotkey")
    );
    assert_eq!(
        parsed.browser.element_selector_hotkey.provenance,
        ConfigProvenance::Override
    );
}

#[test]
fn browser_element_selector_hotkey_rejects_plain_or_shift_only_typing() {
    assert!(normalize_browser_hotkey("c").is_err());
    assert!(normalize_browser_hotkey("shift-c").is_err());
    let parsed = parse_config("browser-element-selector-hotkey = c\n");

    assert_eq!(parsed.diagnostics.len(), 1);
    assert_eq!(
        parsed.browser.element_selector_hotkey.value,
        DEFAULT_BROWSER_ELEMENT_SELECTOR_HOTKEY
    );
    assert_eq!(
        parsed.browser.element_selector_hotkey.provenance,
        ConfigProvenance::Override
    );
}

#[test]
fn parser_applies_all_supported_keys() {
    let parsed = parse_config(
        "\
         # $XDG_CONFIG_HOME/zz/config\n\
         pane-gaps = true\n\
         pane-inactive-opacity = 0.85\n\
         pane-margin = 6 # inline comments are allowed\n\
         window-corner-radius = 10\n\
         motion = off\n\
         notifications = false\n\
         sounds = false\n\
         quit-daemon-on-exit = true\n\
         auto-restart-stale-daemon = true\n\
         check-for-updates = false\n\
         experimental-editor-pane = true\n\
         editor-font-size = 15\n\
         editor-line-numbers = false\n\
         editor-relative-line-numbers = false\n\
         editor-soft-wrap = false\n\
         editor-vim-mode = false\n",
    );

    assert!(parsed.diagnostics.is_empty());
    assert!(parsed.config.experimental_editor_pane.value);
    assert_f32_eq(parsed.config.editor_font_size.value, 15.0);
    assert_eq!(
        parsed.config.editor_font_size.provenance,
        ConfigProvenance::Override
    );
    assert!(!parsed.config.editor_line_numbers.value);
    assert!(!parsed.config.editor_relative_line_numbers.value);
    assert!(!parsed.config.editor_soft_wrap.value);
    assert!(!parsed.config.editor_vim_mode.value);
    assert_eq!(
        parsed.config.editor_line_numbers.provenance,
        ConfigProvenance::Override
    );
    assert_eq!(
        parsed.config.editor_relative_line_numbers.provenance,
        ConfigProvenance::Override
    );
    assert_eq!(
        parsed.config.editor_soft_wrap.provenance,
        ConfigProvenance::Override
    );
    assert_eq!(
        parsed.config.editor_vim_mode.provenance,
        ConfigProvenance::Override
    );
    assert!(parsed.config.pane_gaps.value);
    assert_f32_eq(parsed.config.pane_inactive_opacity.value, 0.85);
    assert_f32_eq(parsed.config.pane_margin.value, 6.0);
    assert_eq!(
        parsed.config.pane_gaps.provenance,
        ConfigProvenance::Override
    );
    assert_eq!(
        parsed.config.pane_inactive_opacity.provenance,
        ConfigProvenance::Override
    );
    assert_eq!(
        parsed.config.pane_margin.provenance,
        ConfigProvenance::Override
    );
    assert_f32_eq(parsed.config.window_corner_radius.value, 10.0);
    assert_eq!(
        parsed.config.window_corner_radius.provenance,
        ConfigProvenance::Override
    );
    assert_eq!(
        parsed.config.motion.value,
        zz_ui::interface_theme::MotionSetting::Off
    );
    assert_eq!(parsed.config.motion.provenance, ConfigProvenance::Override);
    assert!(!parsed.config.notifications.value);
    assert!(AppConfig::default().notifications.value);
    assert!(!parsed.config.sounds.value);
    assert!(AppConfig::default().sounds.value);
    assert!(parsed.config.quit_daemon_on_exit.value);
    assert_eq!(
        parsed.config.quit_daemon_on_exit.provenance,
        ConfigProvenance::Override
    );
    assert!(parsed.config.auto_restart_stale_daemon.value);
    assert_eq!(
        parsed.config.auto_restart_stale_daemon.provenance,
        ConfigProvenance::Override
    );
    assert!(!AppConfig::default().auto_restart_stale_daemon.value);
    assert!(!parsed.config.check_for_updates.value);
    assert_eq!(
        parsed.config.check_for_updates.provenance,
        ConfigProvenance::Override
    );
    assert!(AppConfig::default().check_for_updates.value);
}

#[test]
fn parser_applies_the_theme_keys() {
    let parsed = parse_config(
        "theme-mode = dark\n\
         app-icon = light\n\
         control-shape = pill # trailing comments still work\n",
    );

    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    assert_eq!(parsed.config.theme_mode.value, ThemeModeSetting::Dark);
    assert_eq!(
        parsed.config.theme_mode.provenance,
        ConfigProvenance::Override
    );
    assert_eq!(parsed.config.app_icon.value, AppIconSetting::Light);
    assert_eq!(
        parsed.config.app_icon.provenance,
        ConfigProvenance::Override
    );
    assert_eq!(
        parsed.config.control_shape,
        ConfigValue {
            value: interface_theme::ControlShapeSetting::Pill,
            provenance: ConfigProvenance::Override,
        }
    );
}

#[test]
fn unset_theme_keys_take_zz_look_and_follow_the_system() {
    let parsed = parse_config("pane-gaps = true\n");
    assert_eq!(parsed.config.theme_mode.value, ThemeModeSetting::System);
    assert_eq!(parsed.config.app_icon.value, AppIconSetting::Automatic);
    assert_eq!(
        theme_overrides_from(&parsed.config, &parsed.glass),
        interface_theme::ThemeOverrides {
            radius: Some(12.25),
            controls: Some(interface_theme::Controls::Rounded),
            glass: Some(interface_theme::Glass::Liquid),
            ..interface_theme::ThemeOverrides::default()
        }
    );
    assert_eq!((interface_theme::corner_share(12.25) * 100.0).round(), 70.0);
}

#[test]
fn invalid_theme_values_warn_and_keep_the_previous_value() {
    let parsed = parse_config(
        "theme-mode = midnight\n\
         app-icon = rainbow\n\
         control-shape = square\n",
    );

    assert_eq!(parsed.diagnostics.len(), 3);
    assert_eq!(parsed.config.theme_mode.value, ThemeModeSetting::System);
    assert_eq!(parsed.config.app_icon.value, AppIconSetting::Automatic);
    assert_eq!(
        parsed.config.control_shape.value,
        interface_theme::ControlShapeSetting::Rounded
    );
}

#[test]
fn retired_keys_are_unsupported() {
    let parsed = parse_config(
        "frame-content-corner-radius = 12.5\n\
         pane-content-corner-radius = 12.5\n\
         show-fps = true\n\
         show-app-fps = true\n\
         show-browser-fps = true\n\
         corner-shape = round\n\
         pane-shadow = false\n\
         pane-gaps = true\n",
    );

    assert_eq!(
        parsed.config,
        zz_config::AppConfig {
            pane_gaps: ConfigValue {
                value: true,
                provenance: ConfigProvenance::Override,
            },
            ..zz_config::AppConfig::default()
        }
    );
    assert_eq!(parsed.diagnostics.len(), 7);
    assert!(
        parsed
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.message.contains("unsupported key"))
    );
}

#[test]
fn agent_adapter_keys_reach_the_daemon() {
    let parsed = parse_config(
        "agent-command = {\"command\":\"node\",\"args\":[\"agent.js\"],\"env\":{\"TOKEN\":\"#not-a-comment\"}} # command comment\n\
         agent-claude-code-command = claude-agent-acp --stdio\n\
         agent-auto-approve = false\n",
    );

    assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
    assert_eq!(
        parsed.daemon_entries,
        [
            (
                "agent-command".to_owned(),
                "{\"command\":\"node\",\"args\":[\"agent.js\"],\"env\":{\"TOKEN\":\"#not-a-comment\"}}"
                    .to_owned()
            ),
            (
                "agent-claude-code-command".to_owned(),
                "claude-agent-acp --stdio".to_owned()
            ),
            ("agent-auto-approve".to_owned(), "false".to_owned()),
        ],
        "the daemon spawns the adapter, so its keys travel as mux options"
    );
}

#[test]
fn daemon_owned_entries_preserve_file_order_and_repeated_keys() {
    let parsed = parse_config(
        "\
         background = #101010\n\
         palette = 1=#112233\n\
         pane-margin = 24\n\
         font-family = First Mono\n\
         palette = 2=#445566 # trailing comment\n\
         font-family = Emoji Fallback\n",
    );

    assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
    assert_eq!(
        parsed.daemon_entries,
        [
            ("background".to_owned(), "#101010".to_owned()),
            ("palette".to_owned(), "1=#112233".to_owned()),
            ("font-family".to_owned(), "First Mono".to_owned()),
            ("palette".to_owned(), "2=#445566".to_owned()),
            ("font-family".to_owned(), "Emoji Fallback".to_owned()),
        ]
    );
}

#[test]
fn all_mux_options_are_collected_raw_in_file_order() {
    let parsed = parse_config(
        "\
         prefix = C-a\n\
         mode-keys = vi\n\
         history-limit = not-parsed-here\n\
         word-separators = !@#\n\
         copy-command = pbcopy --flag\n\
         set-clipboard = external\n\
         buffer-limit = 12\n\
         synchronize-panes = on\n\
         prefix = C-Space\n",
    );

    assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
    assert_eq!(
        parsed.daemon_entries,
        [
            ("prefix".to_owned(), "C-a".to_owned()),
            ("mode-keys".to_owned(), "vi".to_owned()),
            ("history-limit".to_owned(), "not-parsed-here".to_owned()),
            ("word-separators".to_owned(), "!@#".to_owned()),
            ("copy-command".to_owned(), "pbcopy --flag".to_owned()),
            ("set-clipboard".to_owned(), "external".to_owned()),
            ("buffer-limit".to_owned(), "12".to_owned()),
            ("synchronize-panes".to_owned(), "on".to_owned()),
            ("prefix".to_owned(), "C-Space".to_owned()),
        ]
    );
}

#[test]
fn experimental_pane_flags_apply_locally_and_forward_to_the_daemon() {
    let parsed = parse_config(
        "\
         experimental-editor-pane = false\n",
    );

    assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
    assert!(!parsed.config.experimental_editor_pane.value);
    assert_eq!(
        parsed.daemon_entries,
        [("experimental-editor-pane".to_owned(), "false".to_owned()),]
    );
}

#[test]
fn hash_comment_rule_preserves_colors_separators_and_quoted_commands() {
    let parsed = parse_config(
        "background = #112233 # trailing color comment\n\
         word-separators = !\"#$%&'()*+,-./:;<=>?@[\\]^`{|}~\n\
         copy-command = sh -c 'printf #copied' target file # trailing command comment\n",
    );

    assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
    assert_eq!(
        parsed.daemon_entries,
        [
            ("background".to_owned(), "#112233".to_owned()),
            (
                "word-separators".to_owned(),
                "!\"#$%&'()*+,-./:;<=>?@[\\]^`{|}~".to_owned(),
            ),
            (
                "copy-command".to_owned(),
                "sh -c 'printf #copied' target file".to_owned(),
            ),
        ]
    );
}

#[test]
fn invalid_and_unknown_entries_warn_without_replacing_defaults() {
    let parsed = parse_config(
        "\
         chrome-tint = -1\n\
         pane-margin = NaN\n\
         window-corner-radius = 300\n\
         pointing-hand = perhaps\n\
         future-setting = 12\n\
         malformed\n",
    );

    assert_f32_eq(
        parsed.config.chrome_tint.value,
        zz_config::DEFAULT_CHROME_TINT,
    );
    assert_f32_eq(
        parsed.config.pane_margin.value,
        zz_config::DEFAULT_PANE_MARGIN,
    );
    assert_f32_eq(parsed.config.window_corner_radius.value, 13.5);
    assert_eq!(
        parsed.config.window_corner_radius.provenance,
        ConfigProvenance::Override
    );
    assert_eq!(
        parsed.config.pointing_hand.value,
        zz_config::DEFAULT_POINTING_HAND
    );
    assert_eq!(
        parsed.config.pointing_hand.provenance,
        ConfigProvenance::Override
    );
    assert_eq!(
        parsed.config.chrome_tint.provenance,
        ConfigProvenance::Override
    );
    assert_eq!(
        parsed.config.pane_margin.provenance,
        ConfigProvenance::Override
    );
    assert_eq!(parsed.diagnostics.len(), 6);
    assert!(parsed.diagnostics[0].message.contains("between 0 and 1"));
    assert!(parsed.diagnostics[1].message.contains("finite"));
    assert!(parsed.diagnostics[2].message.contains("between 0 and 32"));
    assert!(parsed.diagnostics[3].message.contains("expected a boolean"));
    assert!(parsed.diagnostics[4].message.contains("unsupported key"));
    assert_eq!(parsed.diagnostics[5].message, "expected `key = value`");
}

#[test]
fn parser_accepts_a_tmux_spelled_switch_and_validates_numeric_values() {
    let parsed = parse_config("pane-gaps = yes\npane-margin = 99\npane-inactive-opacity = 1.1\n");

    assert!(parsed.config.pane_gaps.value);
    assert_f32_eq(
        parsed.config.pane_margin.value,
        zz_config::DEFAULT_PANE_MARGIN,
    );
    assert_f32_eq(parsed.config.pane_inactive_opacity.value, 0.7);
    assert_eq!(
        parsed.config.pane_gaps.provenance,
        ConfigProvenance::Override
    );
    assert_eq!(
        parsed.config.pane_margin.provenance,
        ConfigProvenance::Override
    );
    assert_eq!(
        parsed.config.pane_inactive_opacity.provenance,
        ConfigProvenance::Override
    );
    assert_eq!(parsed.diagnostics.len(), 2);
    assert!(parsed.diagnostics[0].message.contains("between 0 and 32"));
    assert!(parsed.diagnostics[1].message.contains("between 0 and 1"));
}

#[test]
fn later_valid_entries_replace_earlier_values() {
    let parsed = parse_config(
        "\
         window-corner-radius = 12\n\
         window-corner-radius = nope\n\
         window-corner-radius = 16\n",
    );

    assert_f32_eq(parsed.config.window_corner_radius.value, 16.0);
    assert_eq!(parsed.diagnostics.len(), 1);
}

#[test]
fn invalid_duplicate_retains_the_previous_valid_value() {
    let parsed = parse_config(
        "window-corner-radius = 12\n\
         window-corner-radius = nope\n",
    );

    assert_f32_eq(parsed.config.window_corner_radius.value, 12.0);
    assert_eq!(parsed.diagnostics.len(), 1);
}

#[test]
fn parse_boolean_accepts_the_tmux_flag_spellings() {
    for value in ["on", "yes", "1", "true"] {
        assert_eq!(parse_boolean(value), Ok(true), "{value}");
    }
    for value in ["off", "no", "0", "false"] {
        assert_eq!(parse_boolean(value), Ok(false), "{value}");
    }
    assert!(parse_boolean("sometimes").is_err());
}

#[test]
fn named_config_keys_round_trip_and_retired_ones_do_not() {
    let named = [
        "window-corner-radius",
        "motion",
        "tray",
        "notifications",
        "sounds",
        "quit-daemon-on-exit",
        "auto-restart-stale-daemon",
        "check-for-updates",
        "experimental-editor-pane",
        "pane-gaps",
        "pane-glow-strength",
        "pane-inactive-opacity",
        "pane-margin",
        "widget-corner-radius",
        "chrome-contrast",
        "pointing-hand",
        "chrome-accent",
        "chrome-tint",
        "chrome-tint-hue",
        "chrome-density",
        "editor-font-size",
        "editor-line-numbers",
        "editor-relative-line-numbers",
        "editor-soft-wrap",
        "editor-vim-mode",
        "browser-element-selector-hotkey",
        "browser-search-provider",
        "browser-egress",
        "theme-mode",
        "control-shape",
        "interface-glass",
        "ui-font-family",
        "ui-mono-font-family",
        "ui-chat-font-family",
        "app-icon",
    ];
    for key in named {
        assert_eq!(ConfigKey::parse(key).map(ConfigKey::as_str), Some(key));
    }
    for key in RETIRED_APPEARANCE_KEYS
        .into_iter()
        .chain(RETIRED_PANE_KEYS)
        .chain(RETIRED_STATUS_KEYS)
        .chain(zz_config::RETIRED_LOOK_KEYS)
        .chain(zz_config::RETIRED_MATERIAL_KEYS)
        .chain(zz_config::RETIRED_AGENT_KEYS)
        .chain(zz_config::RETIRED_THEME_KEYS)
        .chain(zz_config::RETIRED_NAVIGATION_KEYS)
        .chain(zz_config::RETIRED_WINDOW_KEYS)
    {
        assert_eq!(ConfigKey::parse(key), None, "{key}");
    }
}

#[test]
fn file_loader_rejects_oversized_configuration() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let path = directory.path().join(CONFIG_FILE_NAME);
    fs::write(&path, vec![b' '; MAX_CONFIG_BYTES + 1]).expect("write test configuration");

    let error = load_config(&path).expect_err("oversized configuration should fail");
    assert_eq!(error.kind(), ErrorKind::InvalidData);
}
