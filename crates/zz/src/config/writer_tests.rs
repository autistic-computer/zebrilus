use std::fs;

use super::test_support::*;
use super::*;

#[test]
fn appearance_editor_view_hides_app_side_lines_and_round_trips() {
    let source = "# header comment\n\
                  pane-gaps = true\n\
                  font-family = \"Berkeley Mono\"\n\
                  \n\
                  prefix = C-a\n\
                  agent-command = codex\n\
                  host-blue = ssh blue\n\
                  background = #282C34\n\
                  unknown-key = kept\n\
                  chrome-preset-dark = nord\n";
    let view = appearance_editor_view(source);
    assert_eq!(
        view,
        "# header comment\n\
         font-family = \"Berkeley Mono\"\n\
         \n\
         background = #282C34\n\
         unknown-key = kept\n"
    );

    let directory = tempfile::tempdir().expect("temporary directory");
    let path = directory.path().join("config");
    fs::write(&path, source).expect("write configuration");
    save_appearance_editor(&path, &view).expect("save the unchanged view");
    let merged = fs::read_to_string(&path).expect("read merged configuration");
    assert_eq!(
        merged,
        "pane-gaps = true\n\
         prefix = C-a\n\
         agent-command = codex\n\
         host-blue = ssh blue\n\
         chrome-preset-dark = nord\n\
         # header comment\n\
         font-family = \"Berkeley Mono\"\n\
         \n\
         background = #282C34\n\
         unknown-key = kept\n"
    );
    assert_eq!(appearance_editor_view(&merged), view);
}

#[test]
fn appearance_editor_rejects_app_side_keys_and_leaves_the_file_alone() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let path = directory.path().join("config");
    fs::write(&path, "pane-gaps = true\n").expect("write configuration");
    let error = save_appearance_editor(&path, "font-size = 14\npane-margin = 4\n")
        .expect_err("app-side keys must not pass through the appearance editor");
    assert_eq!(error.kind(), ErrorKind::InvalidInput);
    assert!(error.to_string().contains("pane-margin"));
    assert_eq!(
        fs::read_to_string(&path).expect("read unchanged configuration"),
        "pane-gaps = true\n"
    );
}

#[test]
fn appearance_editor_saves_into_a_missing_file() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let path = directory.path().join("config");
    save_appearance_editor(&path, "font-size = 14\n").expect("create the file");
    assert_eq!(
        fs::read_to_string(&path).expect("read created configuration"),
        "font-size = 14\n"
    );
}

#[test]
fn writer_uses_the_same_hash_comment_boundary_as_the_parser() {
    let source = "background = #112233 # keep color comment\n\
                  palette = 1=#445566 # keep palette comment\n\
                  word-separators = !@# # keep mux comment\n";

    let edited = edit_config_source(source, "background", Some("#AABBCC"));
    let edited = edit_config_source(&edited, "palette", Some("1=#DDEEFF"));
    let edited = edit_config_source(&edited, "word-separators", Some("!#$"));

    assert_eq!(
        edited,
        "background = #AABBCC # keep color comment\n\
         palette = 1=#DDEEFF # keep palette comment\n\
         word-separators = !#$ # keep mux comment\n"
    );
    assert_eq!(
        parse_config(&edited).daemon_entries,
        [
            ("background".to_owned(), "#AABBCC".to_owned()),
            ("palette".to_owned(), "1=#DDEEFF".to_owned()),
            ("word-separators".to_owned(), "!#$".to_owned()),
        ]
    );
}

#[test]
fn writer_edits_only_the_last_occurrence_and_preserves_surrounding_bytes() {
    let source = "# keep this comment\r\n\
                  window-corner-radius=12 # earlier value\r\n\
                  future syntax without equals\r\n\
                  window-corner-radius = 20.0  # effective value\r\n\
                  unknown-key = untouched\r\n";

    assert_eq!(
        edit_config_source(source, ConfigKey::WindowCornerRadius.as_str(), Some("24"),),
        "# keep this comment\r\n\
         window-corner-radius=12 # earlier value\r\n\
         future syntax without equals\r\n\
         window-corner-radius = 24  # effective value\r\n\
         unknown-key = untouched\r\n"
    );
}

#[test]
fn writer_preserves_compact_spacing_and_inline_comment_on_edited_line() {
    assert_eq!(
        edit_config_source(
            "quit-daemon-on-exit=true # keep\n",
            ConfigKey::QuitDaemonOnExit.as_str(),
            Some("false"),
        ),
        "quit-daemon-on-exit=false # keep\n"
    );
}

#[test]
fn writer_reset_removes_only_the_last_matching_line() {
    let source = "# before\n\
                  pane-margin = 8\n\
                  unknown line stays byte-identical\n\
                  pane-margin=18 # reset this one\n\
                  # after\n";

    assert_eq!(
        edit_config_source(source, ConfigKey::PaneMargin.as_str(), None),
        "# before\n\
         pane-margin = 8\n\
         unknown line stays byte-identical\n\
         # after\n"
    );
}

#[test]
fn pane_chrome_keys_round_trip_through_the_config_writer() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let path = directory.path().join(CONFIG_FILE_NAME);
    for (key, value) in [
        (ConfigKey::PaneGaps, "true"),
        (ConfigKey::PaneInactiveOpacity, "0.85"),
        (ConfigKey::PaneMargin, "2.5"),
    ] {
        write_config_edit_at(&path, key.as_str(), Some(value)).expect("write pane chrome setting");
    }

    let source = fs::read_to_string(path).expect("read pane chrome settings");
    let parsed = parse_config(&source);
    assert!(parsed.diagnostics.is_empty());
    assert!(parsed.config.pane_gaps.value);
    assert_f32_eq(parsed.config.pane_inactive_opacity.value, 0.85);
    assert_f32_eq(parsed.config.pane_margin.value, 2.5);
}

#[test]
fn the_app_icon_setting_round_trips_through_the_config_writer() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let path = directory.path().join(CONFIG_FILE_NAME);
    for setting in AppIconSetting::ALL {
        write_config_edit_at(&path, ConfigKey::AppIcon.as_str(), Some(setting.as_str()))
            .expect("write app icon");

        let source = fs::read_to_string(&path).expect("read app icon setting");
        let parsed = parse_config(&source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        assert_eq!(parsed.config.app_icon.value, setting);
    }
}

#[test]
fn writer_creates_a_fresh_file_and_parent_directories_atomically() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let path = directory.path().join("nested/zz/config");

    write_config_edit_at(&path, ConfigKey::PaneMargin.as_str(), Some("19"))
        .expect("create fresh configuration");

    assert_eq!(
        fs::read_to_string(&path).expect("read fresh configuration"),
        "pane-margin = 19\n"
    );
    let entries = fs::read_dir(path.parent().expect("configuration parent"))
        .expect("read configuration directory")
        .map(|entry| entry.expect("directory entry").file_name())
        .collect::<Vec<_>>();
    assert_eq!(entries, [std::ffi::OsString::from(CONFIG_FILE_NAME)]);
}

#[test]
fn writer_rejects_an_edit_past_the_bound_without_touching_the_file() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let path = directory.path().join(CONFIG_FILE_NAME);
    let source = " ".repeat(MAX_CONFIG_BYTES);
    fs::write(&path, &source).expect("write bounded configuration");

    let error = write_config_edit_at(&path, ConfigKey::Tray.as_str(), Some("false"))
        .expect_err("appending past the read bound must fail");

    assert_eq!(error.kind(), ErrorKind::InvalidData);
    assert_eq!(
        fs::read_to_string(&path).expect("read unchanged configuration"),
        source
    );
}
