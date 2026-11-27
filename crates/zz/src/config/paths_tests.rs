use std::{env, fs, path::PathBuf};

use super::*;

fn absolute_test_root(name: &str) -> PathBuf {
    env::current_dir()
        .expect("current directory")
        .join("target/config-path-tests")
        .join(name)
}

fn expected_config_path(base: &Path) -> PathBuf {
    base.join(CONFIG_DIRECTORY_NAME).join(CONFIG_FILE_NAME)
}

#[ala::test]
fn missing_watched_file_restores_built_in_defaults(cx: &mut ala::TestAppContext) {
    cx.update(|cx| {
        cx.set_global(AppConfig(
            parse_config("pane-gaps = true\npane-margin = 9\n").config,
        ));
        install_config(None, None, cx);
    });

    assert_eq!(cx.update(|cx| resolved_config(cx)), AppConfig::default());
}

#[test]
fn unix_candidates_follow_xdg_then_home_config() {
    let xdg = absolute_test_root("unix-xdg");
    let home = absolute_test_root("unix-home");
    let candidates = config_candidates_for(
        ConfigPlatform::Unix,
        ConfigEnvironment {
            xdg_config_home: Some(&xdg),
            home: Some(&home),
            ..ConfigEnvironment::default()
        },
    );

    assert_eq!(
        candidates,
        vec![
            expected_config_path(&xdg),
            expected_config_path(&home.join(".config")),
        ]
    );
}

#[test]
fn fresh_write_path_prefers_xdg_then_home_config() {
    let xdg = absolute_test_root("write-xdg");
    let home = absolute_test_root("write-home");
    let candidates = |xdg_config_home| {
        config_candidates_for(
            ConfigPlatform::Unix,
            ConfigEnvironment {
                xdg_config_home,
                home: Some(&home),
                ..ConfigEnvironment::default()
            },
        )
    };

    assert_eq!(
        config_write_path(&candidates(Some(&xdg))).unwrap(),
        expected_config_path(&xdg)
    );
    assert_eq!(
        config_write_path(&candidates(Some(Path::new("relative")))).unwrap(),
        expected_config_path(&home.join(".config"))
    );
    assert_eq!(
        config_write_path(&[]).unwrap_err().kind(),
        ErrorKind::NotFound
    );
}

#[test]
fn macos_candidates_include_xdg_home_and_application_support() {
    let xdg = absolute_test_root("macos-xdg");
    let home = absolute_test_root("macos-home");
    let candidates = config_candidates_for(
        ConfigPlatform::Macos,
        ConfigEnvironment {
            xdg_config_home: Some(&xdg),
            home: Some(&home),
            ..ConfigEnvironment::default()
        },
    );

    assert_eq!(
        candidates,
        vec![
            expected_config_path(&xdg),
            expected_config_path(&home.join(".config")),
            expected_config_path(&home.join("Library/Application Support")),
        ]
    );
}

#[test]
fn windows_candidates_include_roaming_local_and_config_fallbacks() {
    let xdg = absolute_test_root("windows-xdg");
    let home = absolute_test_root("windows-home");
    let appdata = absolute_test_root("windows-roaming");
    let local_appdata = absolute_test_root("windows-local");
    let user_profile = absolute_test_root("windows-profile");
    let candidates = config_candidates_for(
        ConfigPlatform::Windows,
        ConfigEnvironment {
            xdg_config_home: Some(&xdg),
            home: Some(&home),
            appdata: Some(&appdata),
            local_appdata: Some(&local_appdata),
            user_profile: Some(&user_profile),
        },
    );

    assert_eq!(
        candidates,
        vec![
            expected_config_path(&xdg),
            expected_config_path(&appdata),
            expected_config_path(&local_appdata),
            expected_config_path(&user_profile.join(".config")),
            expected_config_path(&home.join(".config")),
        ]
    );
}

#[test]
fn candidates_ignore_relative_roots_and_remove_duplicates() {
    let appdata = absolute_test_root("duplicates");
    let candidates = config_candidates_for(
        ConfigPlatform::Windows,
        ConfigEnvironment {
            xdg_config_home: Some(Path::new("relative")),
            appdata: Some(&appdata),
            local_appdata: Some(&appdata),
            ..ConfigEnvironment::default()
        },
    );

    assert_eq!(candidates, vec![expected_config_path(&appdata)]);
}

#[test]
fn discovery_selects_the_first_existing_candidate() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let missing = directory.path().join("missing/config");
    let existing = directory.path().join("existing/config");
    fs::create_dir_all(existing.parent().expect("config parent")).expect("create config directory");
    fs::write(&existing, "pane-margin = 12\n").expect("write config");

    assert_eq!(
        discover_config_path(&[missing, existing.clone()]),
        Some(existing)
    );
}

#[test]
fn file_stamp_tracks_candidate_appearance_precedence_and_deletion() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let first = directory.path().join("first/config");
    let second = directory.path().join("second/config");
    let candidates = [first.clone(), second.clone()];

    assert_eq!(
        ConfigFileStamp::detect(&candidates),
        ConfigFileStamp::default()
    );

    fs::create_dir_all(second.parent().expect("second config parent"))
        .expect("create second config directory");
    fs::write(&second, "pane-margin = 12\n").expect("write second config");
    assert_eq!(
        ConfigFileStamp::detect(&candidates).path,
        Some(second.clone())
    );

    fs::create_dir_all(first.parent().expect("first config parent"))
        .expect("create first config directory");
    fs::write(&first, "pane-margin = 24\n").expect("write first config");
    assert_eq!(
        ConfigFileStamp::detect(&candidates).path,
        Some(first.clone())
    );

    fs::remove_file(&first).expect("remove first config");
    assert_eq!(
        ConfigFileStamp::detect(&candidates).path,
        Some(second.clone())
    );
    fs::remove_file(&second).expect("remove second config");
    assert_eq!(
        ConfigFileStamp::detect(&candidates),
        ConfigFileStamp::default()
    );
}

#[test]
fn file_stamp_changes_when_configuration_length_changes() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let path = directory.path().join(CONFIG_FILE_NAME);
    fs::write(&path, "pane-margin = 1\n").expect("write initial config");
    let initial = ConfigFileStamp::detect(std::slice::from_ref(&path));

    fs::write(&path, "pane-margin = 120\n").expect("rewrite config");
    let changed = ConfigFileStamp::detect(std::slice::from_ref(&path));

    assert_ne!(initial, changed);
}
