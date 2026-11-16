use std::{fs::File, io::Read};

use super::*;

const FILE_EDIT_LIMIT: u64 = zz_protocol::MAX_FILE_EDIT_BYTES as u64;

pub fn read_text_file(path: &Path, limit: u64) -> Result<String, String> {
    let too_large = || {
        format!(
            "{} is larger than the {} MiB editor limit",
            path.display(),
            limit / (1024 * 1024)
        )
    };
    let metadata = fs::metadata(path)
        .map_err(|error| format!("Could not open {}: {error}", path.display()))?;
    if metadata.len() > limit {
        return Err(too_large());
    }
    let mut file =
        File::open(path).map_err(|error| format!("Could not open {}: {error}", path.display()))?;
    let mut bytes = Vec::with_capacity(usize::try_from(metadata.len()).unwrap_or_default());
    Read::take(&mut file, limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("Could not read {}: {error}", path.display()))?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > limit {
        return Err(too_large());
    }
    if bytes.contains(&0) {
        return Err(format!(
            "{} contains NUL bytes and appears to be binary",
            path.display()
        ));
    }
    String::from_utf8(bytes).map_err(|_| format!("{} is not valid UTF-8", path.display()))
}

fn bounded_error(mut error: String) -> String {
    if error.len() > zz_protocol::MAX_FILE_EDIT_ERROR_BYTES {
        let mut end = zz_protocol::MAX_FILE_EDIT_ERROR_BYTES;
        while !error.is_char_boundary(end) {
            end -= 1;
        }
        error.truncate(end);
    }
    error
}

fn spawn_file_edit(
    outbound: &Arc<OutboundMailbox>,
    work: impl FnOnce() -> ProtocolMessage + Send + 'static,
) {
    let outbound = Arc::clone(outbound);
    let spawned = thread::Builder::new()
        .name("zz-file-edit".to_owned())
        .spawn(move || {
            let _ = outbound.enqueue_reliable(&work());
        });
    if let Err(error) = spawned {
        log::warn!("could not start a file read or write: {error}");
    }
}

impl Shared {
    fn file_edit_refusal(
        &self,
        client: ClientId,
        kind: ClientKind,
        pane: PaneId,
        path: &str,
    ) -> Option<String> {
        let inner = self.inner.lock();
        if kind != ClientKind::Interactive {
            Some("file access needs an interactive client".to_owned())
        } else if inner.client_flags.contains(client) {
            Some("client is read-only".to_owned())
        } else if !client_is_attached_to_pane(&inner, client, pane) {
            Some(format!("{pane} is not in an attached session"))
        } else if !Path::new(path).is_absolute() {
            Some("file paths must be absolute".to_owned())
        } else {
            None
        }
    }

    pub(super) fn start_file_read(
        &self,
        client: ClientId,
        kind: ClientKind,
        request: (u64, PaneId, String),
        outbound: &Arc<OutboundMailbox>,
    ) {
        let (request_id, pane, path) = request;
        if let Some(error) = self.file_edit_refusal(client, kind, pane, &path) {
            let _ = outbound.enqueue_reliable(&ProtocolMessage::FileReadResponse {
                request_id,
                result: Err(error),
            });
            return;
        }
        spawn_file_edit(outbound, move || ProtocolMessage::FileReadResponse {
            request_id,
            result: read_text_file(Path::new(&path), FILE_EDIT_LIMIT).map_err(bounded_error),
        });
    }

    pub(super) fn start_file_write(
        &self,
        client: ClientId,
        kind: ClientKind,
        request: (u64, PaneId, String, String),
        outbound: &Arc<OutboundMailbox>,
    ) {
        let (request_id, pane, path, text) = request;
        if let Some(error) = self.file_edit_refusal(client, kind, pane, &path) {
            let _ = outbound.enqueue_reliable(&ProtocolMessage::FileWriteResponse {
                request_id,
                result: Err(error),
            });
            return;
        }
        spawn_file_edit(outbound, move || ProtocolMessage::FileWriteResponse {
            request_id,
            result: zz_config::atomic_write(Path::new(&path), text.as_bytes())
                .map_err(|error| bounded_error(format!("Could not save {path}: {error}"))),
        });
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::mpsc, time::Duration};

    use super::*;

    fn answer(mailbox: &Arc<OutboundMailbox>) -> ProtocolMessage {
        let (sender, answers) = mpsc::channel();
        let reader = Arc::clone(mailbox);
        thread::spawn(move || {
            if let Some(frame) = reader.recv() {
                let _ = sender.send(zz_protocol::decode_protocol_frame(&frame).expect("decode"));
            }
        });
        answers
            .recv_timeout(Duration::from_secs(10))
            .expect("the daemon answered")
    }

    #[test]
    fn attached_clients_read_and_write_files_and_watchers_cannot() {
        let shared = Arc::new(Shared::new(1));
        let mailbox = OutboundMailbox::new();
        let (client, _) =
            shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&mailbox));
        let mut context = ExecutionContext::default();
        shared
            .execute(
                client,
                ClientKind::Interactive,
                &mut context,
                &CommandInvocation::new("new-session", ["-d", "-s", "files"]),
            )
            .expect("session");
        let pane = context.pane.expect("pane id");
        let scratch = tempfile::tempdir().expect("temp dir");
        let path = scratch.path().join("notes.txt");
        let path_text = path.to_str().expect("utf-8 temp dir").to_owned();
        fs::write(&path, "before\n").expect("fixture");

        let outbound = OutboundMailbox::new();
        outbound.accept_client_capabilities(&[zz_protocol::FILE_EDIT_CAPABILITY.to_owned()]);
        shared.start_file_read(
            client,
            ClientKind::Interactive,
            (1, pane, path_text.clone()),
            &outbound,
        );
        assert_eq!(
            answer(&outbound),
            ProtocolMessage::FileReadResponse {
                request_id: 1,
                result: Err(format!("{pane} is not in an attached session")),
            }
        );

        shared
            .attach(client, context.session.expect("session id"))
            .expect("attach session");
        shared.start_file_read(
            client,
            ClientKind::Interactive,
            (2, pane, path_text.clone()),
            &outbound,
        );
        assert_eq!(
            answer(&outbound),
            ProtocolMessage::FileReadResponse {
                request_id: 2,
                result: Ok("before\n".to_owned()),
            }
        );

        shared.start_file_write(
            client,
            ClientKind::Interactive,
            (3, pane, path_text.clone(), "after\n".to_owned()),
            &outbound,
        );
        assert_eq!(
            answer(&outbound),
            ProtocolMessage::FileWriteResponse {
                request_id: 3,
                result: Ok(()),
            }
        );
        assert_eq!(fs::read_to_string(&path).expect("saved"), "after\n");

        shared.start_file_read(
            client,
            ClientKind::Interactive,
            (4, pane, "notes.txt".to_owned()),
            &outbound,
        );
        assert_eq!(
            answer(&outbound),
            ProtocolMessage::FileReadResponse {
                request_id: 4,
                result: Err("file paths must be absolute".to_owned()),
            }
        );

        shared.inner.lock().client_flags.insert(client);
        shared.start_file_write(
            client,
            ClientKind::Interactive,
            (5, pane, path_text, "watcher\n".to_owned()),
            &outbound,
        );
        assert_eq!(
            answer(&outbound),
            ProtocolMessage::FileWriteResponse {
                request_id: 5,
                result: Err("client is read-only".to_owned()),
            }
        );
        assert_eq!(fs::read_to_string(&path).expect("unchanged"), "after\n");
    }

    #[test]
    fn bounded_reader_rejects_binary_invalid_utf8_and_oversized_files() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let limit = 8 * 1024 * 1024;

        let binary = directory.path().join("binary");
        fs::write(&binary, b"a\0b").expect("binary fixture");
        assert_eq!(
            read_text_file(&binary, limit),
            Err(format!(
                "{} contains NUL bytes and appears to be binary",
                binary.display()
            ))
        );

        let invalid = directory.path().join("invalid");
        fs::write(&invalid, [0xff]).expect("invalid UTF-8 fixture");
        assert_eq!(
            read_text_file(&invalid, limit),
            Err(format!("{} is not valid UTF-8", invalid.display()))
        );

        let oversized = directory.path().join("oversized");
        let mut file = File::create(&oversized).expect("oversized fixture");
        file.write_all(b"x").expect("fixture byte");
        file.set_len(limit + 1).expect("extend fixture");
        assert_eq!(
            read_text_file(&oversized, limit),
            Err(format!(
                "{} is larger than the 8 MiB editor limit",
                oversized.display()
            ))
        );
    }
}
