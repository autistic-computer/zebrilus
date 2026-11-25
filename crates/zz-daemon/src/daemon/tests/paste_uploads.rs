use super::*;

#[cfg(unix)]
#[test]
fn paste_path_image_upload_writes_a_private_file_stages_preview_and_pastes_the_path() {
    use std::os::unix::fs::PermissionsExt as _;

    let directory = tempfile::tempdir().expect("temporary directory");
    let (shared, mailbox, client, pane) =
        paste_upload_fixture(&directory.path().join("paste"), "paste-upload");
    let terminal = Arc::clone(&shared.inner.lock().terminals[&pane]);
    terminal.resize(200, 10, 8, 18);
    wait_for_terminal_dimensions(&terminal, TerminalViewId(client.0), 200, 10);
    take_reliable_messages(&mailbox);

    let image = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a];
    shared.begin_paste_upload(
        client,
        ClientKind::Interactive,
        1,
        pane,
        PasteUploadPurpose::PastePath,
        "png".to_owned(),
        6,
    );
    shared.extend_paste_upload(client, 1, &image[..3]);
    assert!(
        shared.inner.lock().paste_uploads.contains_key(&(client, 1)),
        "a partial upload should stay open"
    );
    shared.extend_paste_upload(client, 1, &image[3..]);
    assert!(
        shared.inner.lock().paste_uploads.is_empty(),
        "reaching the declared total should finish the upload"
    );

    let file_name = format!("paste-{client}-1.png");
    let written = shared.paste_directory.join(&file_name);
    assert_eq!(fs::read(&written).expect("uploaded image"), image);
    assert_eq!(
        fs::metadata(&written)
            .expect("upload metadata")
            .permissions()
            .mode()
            & 0o777,
        0o600,
    );
    assert_eq!(
        fs::metadata(&shared.paste_directory)
            .expect("upload directory metadata")
            .permissions()
            .mode()
            & 0o777,
        0o700,
    );
    {
        let pasted_images = shared.pasted_images.lock();
        let stored = pasted_images.get(&pane).expect("pane image state");
        assert_eq!(stored.pending.len(), 1);
        assert_eq!(stored.pending_bytes, image.len());
        assert_eq!(stored.pending[0].format, PastedImageFormat::Png);
        assert_eq!(stored.pending[0].bytes.as_ref(), image);
        assert!(stored.images.is_empty());
    }
    assert!(
        client_message_texts(take_reliable_messages(&mailbox)).is_empty(),
        "a completed upload should not report an error"
    );

    wait_for_pasted_path(&shared, client, pane, &file_name);
}

#[test]
fn paste_path_non_image_upload_pastes_without_staging_or_rejection() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let (shared, mailbox, client, pane) =
        paste_upload_fixture(&directory.path().join("paste"), "paste-upload-non-image");
    let terminal = Arc::clone(&shared.inner.lock().terminals[&pane]);
    terminal.resize(200, 10, 8, 18);
    wait_for_terminal_dimensions(&terminal, TerminalViewId(client.0), 200, 10);
    take_reliable_messages(&mailbox);

    let bytes = b"remote path payload";
    shared.begin_paste_upload(
        client,
        ClientKind::Interactive,
        2,
        pane,
        PasteUploadPurpose::PastePath,
        "txt".to_owned(),
        u32::try_from(bytes.len()).expect("payload length"),
    );
    shared.extend_paste_upload(client, 2, bytes);

    let file_name = format!("paste-{client}-2.txt");
    assert_eq!(
        fs::read(shared.paste_directory.join(&file_name)).expect("uploaded file"),
        bytes
    );
    assert!(!shared.pasted_images.lock().contains_key(&pane));
    assert!(client_message_texts(take_reliable_messages(&mailbox)).is_empty());

    wait_for_pasted_path(&shared, client, pane, &file_name);
}

#[test]
fn record_only_paste_upload_stays_encoded_and_opens_a_pending_binding_window() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let (shared, mailbox, client, pane) =
        paste_upload_fixture(&directory.path().join("paste"), "paste-record-only");
    take_reliable_messages(&mailbox);

    let image = [0x52, 0x49, 0x46, 0x46, 0x57, 0x45, 0x42, 0x50];
    shared.begin_paste_upload(
        client,
        ClientKind::Interactive,
        71,
        pane,
        PasteUploadPurpose::RecordPastedImage,
        "webp".to_owned(),
        u32::try_from(image.len()).expect("image length"),
    );
    shared.extend_paste_upload(client, 71, &image[..3]);
    shared.extend_paste_upload(client, 71, &image[3..]);

    assert!(shared.inner.lock().paste_uploads.is_empty());
    let pasted_images = shared.pasted_images.lock();
    let stored = pasted_images.get(&pane).expect("pane image state");
    assert_eq!(stored.pending.len(), 1);
    assert_eq!(stored.pending_bytes, image.len());
    assert_eq!(stored.pending[0].format, PastedImageFormat::Webp);
    assert_eq!(stored.pending[0].bytes.as_ref(), image);
    assert!(stored.images.is_empty());
    assert!(!shared.paste_directory.exists());
    assert!(client_message_texts(take_reliable_messages(&mailbox)).is_empty());
}

#[test]
fn paste_uploads_are_bounded_per_client_and_dropped_on_overflow() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let (shared, mailbox, client, pane) =
        paste_upload_fixture(&directory.path().join("paste"), "paste-upload-bounds");
    take_reliable_messages(&mailbox);

    shared.extend_paste_upload(client, 404, &[0; 8]);
    assert!(shared.inner.lock().paste_uploads.is_empty());
    assert!(client_message_texts(take_reliable_messages(&mailbox)).is_empty());

    shared.begin_paste_upload(
        client,
        ClientKind::Interactive,
        1,
        pane,
        PasteUploadPurpose::PastePath,
        "png".to_owned(),
        4,
    );
    shared.extend_paste_upload(client, 1, &[0; 5]);
    assert!(
        shared.inner.lock().paste_uploads.is_empty(),
        "an overflowing upload should be dropped"
    );
    assert_eq!(
        client_message_texts(take_reliable_messages(&mailbox)).len(),
        1
    );
    assert!(
        !shared
            .paste_directory
            .join(format!("paste-{client}-1.png"))
            .exists()
    );

    for upload_id in 1..=MAX_CONCURRENT_PASTE_UPLOADS as u64 {
        shared.begin_paste_upload(
            client,
            ClientKind::Interactive,
            upload_id,
            pane,
            PasteUploadPurpose::PastePath,
            "png".to_owned(),
            64,
        );
    }
    shared.begin_paste_upload(
        client,
        ClientKind::Interactive,
        1,
        pane,
        PasteUploadPurpose::PastePath,
        "png".to_owned(),
        32,
    );
    assert!(client_message_texts(take_reliable_messages(&mailbox)).is_empty());
    shared.begin_paste_upload(
        client,
        ClientKind::Interactive,
        9,
        pane,
        PasteUploadPurpose::PastePath,
        "png".to_owned(),
        64,
    );
    assert_eq!(
        shared.inner.lock().paste_uploads.len(),
        MAX_CONCURRENT_PASTE_UPLOADS
    );
    assert_eq!(
        client_message_texts(take_reliable_messages(&mailbox)).len(),
        1
    );

    shared.begin_paste_upload(
        client,
        ClientKind::Interactive,
        10,
        PaneId(9_999_999),
        PasteUploadPurpose::PastePath,
        "png".to_owned(),
        64,
    );
    assert_eq!(
        client_message_texts(take_reliable_messages(&mailbox)).len(),
        1
    );

    shared.unregister(client);
    assert!(
        shared.inner.lock().paste_uploads.is_empty(),
        "a disconnect should take its client's uploads with it"
    );
}

#[test]
fn paste_uploads_from_separate_daemons_never_share_a_directory_or_a_file() {
    let sockets = tempfile::tempdir().expect("temporary directory");
    let first_directory = paste_upload_directory(&sockets.path().join("default.sock"));
    let second_directory = paste_upload_directory(&sockets.path().join("other.sock"));
    assert_ne!(first_directory, second_directory);
    assert_eq!(first_directory.parent(), second_directory.parent());

    let directory = tempfile::tempdir().expect("temporary directory");
    let paste = directory.path().join("paste");
    let first = paste_upload_fixture(&paste, "first-daemon");
    let second = paste_upload_fixture(&paste, "second-daemon");
    assert_eq!(
        first.2, second.2,
        "both daemons hand out the same first client id"
    );
    for ((shared, mailbox, client, pane), bytes) in [
        (&first, b"first".as_slice()),
        (&second, b"second".as_slice()),
    ] {
        take_reliable_messages(mailbox);
        shared.begin_paste_upload(
            *client,
            ClientKind::Interactive,
            1,
            *pane,
            PasteUploadPurpose::PastePath,
            "txt".to_owned(),
            u32::try_from(bytes.len()).expect("payload length"),
        );
        shared.extend_paste_upload(*client, 1, bytes);
        assert!(client_message_texts(take_reliable_messages(mailbox)).is_empty());
    }

    let client = first.2;
    assert_eq!(
        fs::read(paste.join(format!("paste-{client}-1.txt"))).expect("first upload"),
        b"first"
    );
    assert_eq!(
        fs::read(paste.join(format!("paste-{client}-1-1.txt"))).expect("second upload"),
        b"second"
    );
    assert_eq!(fs::read_dir(&paste).expect("upload directory").count(), 2);
}

#[test]
fn paste_upload_retention_keeps_only_the_newest_files() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let paste = directory.path().join("paste");
    for index in 0..PASTE_UPLOAD_RETENTION + 4 {
        thread::sleep(Duration::from_millis(10));
        write_paste_upload(&paste, &format!("paste-c1-{index}"), "png", b"upload")
            .expect("write upload");
        prune_paste_uploads(&paste, PASTE_UPLOAD_RETENTION);
    }

    let remaining = fs::read_dir(&paste)
        .expect("upload directory")
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect::<BTreeSet<_>>();
    assert_eq!(remaining.len(), PASTE_UPLOAD_RETENTION);
    assert!(remaining.contains(&format!("paste-c1-{}.png", PASTE_UPLOAD_RETENTION + 3)));
    assert!(!remaining.contains("paste-c1-0.png"));
}
