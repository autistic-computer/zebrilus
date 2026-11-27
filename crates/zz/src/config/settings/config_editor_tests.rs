use super::*;

#[test]
fn config_editor_source_round_trips_and_preserves_file_on_oversize_save() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let path = directory.path().join("config");
    config::write_config_editor_source(&path, "theme-mode = dark\n", 64).expect("initial write");
    assert_eq!(
        config::read_config_editor_source(&path, 64).expect("read source"),
        "theme-mode = dark\n"
    );

    let error = config::write_config_editor_source(&path, "too large", 4)
        .expect_err("oversized editor content must fail");
    assert_eq!(error.kind(), ErrorKind::InvalidData);
    assert_eq!(
        std::fs::read_to_string(path).expect("read unchanged file"),
        "theme-mode = dark\n"
    );
}

#[test]
fn missing_config_editor_source_starts_empty() {
    let directory = tempfile::tempdir().expect("temporary directory");
    assert_eq!(
        config::read_config_editor_source(&directory.path().join("missing"), 64)
            .expect("missing source"),
        ""
    );
}
