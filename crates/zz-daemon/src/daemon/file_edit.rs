use std::{fs::File, io::Read};

use super::*;

const FILE_EDIT_LIMIT: u64 = zz_protocol::MAX_FILE_EDIT_BYTES as u64;
const MAX_CONCURRENT_FILE_WRITES: usize = 4;

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
        } else if inner.attachment.client_flags.contains(client) {
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

    pub(super) fn receive_file_write(
        &self,
        client: ClientId,
        kind: ClientKind,
        chunk: &FileWriteChunk,
        outbound: &Arc<OutboundMailbox>,
    ) {
        let request_id = chunk.request_id;
        let refuse = |error: String| {
            let _ = outbound.enqueue_reliable(&ProtocolMessage::FileWriteResponse {
                request_id,
                result: Err(bounded_error(error)),
            });
        };
        if chunk.offset == 0
            && let Some(error) = self.file_edit_refusal(client, kind, chunk.pane, &chunk.path)
        {
            refuse(error);
            return;
        }
        let assembled =
            append_file_write(&mut self.inner.lock().transfers.file_writes, client, chunk);
        let (pane, path, text) = match assembled {
            Ok(Some(write)) => write,
            Ok(None) => return,
            Err(error) => {
                refuse(error);
                return;
            }
        };
        if let Some(error) = self.file_edit_refusal(client, kind, pane, &path) {
            refuse(error);
            return;
        }
        spawn_file_edit(outbound, move || ProtocolMessage::FileWriteResponse {
            request_id,
            result: zz_config::atomic_write(Path::new(&path), text.as_bytes())
                .map_err(|error| bounded_error(format!("Could not save {path}: {error}"))),
        });
    }
}

impl Shared {
    pub(super) fn save_attachment(
        &self,
        client: ClientId,
        kind: ClientKind,
        upload: (u64, PaneId, String, Vec<u8>),
        outbound: &Arc<OutboundMailbox>,
    ) {
        let (request_id, pane, name, bytes) = upload;
        if let Some(error) = self.file_edit_refusal(client, kind, pane, "/") {
            let _ = outbound.enqueue_reliable(&ProtocolMessage::AgentFileAttached {
                request_id,
                result: Err(bounded_error(error)),
            });
            return;
        }
        let root = std::env::temp_dir().join(format!(
            "{}-attachments",
            zz_protocol::app_identity::DIRECTORY
        ));
        spawn_file_edit(outbound, move || ProtocolMessage::AgentFileAttached {
            request_id,
            result: write_attachment(&root, pane, request_id, &name, &bytes)
                .map_err(|error| bounded_error(format!("Could not attach {name}: {error}"))),
        });
    }
}

fn write_attachment(
    root: &Path,
    pane: PaneId,
    request_id: u64,
    name: &str,
    bytes: &[u8],
) -> std::io::Result<String> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis());
    let dir = root
        .join(pane.0.to_string())
        .join(format!("{stamp}-{request_id}"));
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(attachment_name(name));
    std::fs::write(&path, bytes)?;
    Ok(path.to_string_lossy().into_owned())
}

fn attachment_name(name: &str) -> String {
    let base = name
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or_default()
        .chars()
        .filter(|character| !character.is_control())
        .collect::<String>();
    let base = base.trim();
    if base.is_empty() || base.chars().all(|character| character == '.') {
        "attachment".to_owned()
    } else {
        base.to_owned()
    }
}

pub(super) struct FileWriteChunk {
    pub request_id: u64,
    pub pane: PaneId,
    pub path: String,
    pub offset: u64,
    pub last: bool,
    pub bytes: Vec<u8>,
}

pub(super) struct PendingFileWrite {
    pane: PaneId,
    path: String,
    bytes: Vec<u8>,
}

fn append_file_write(
    writes: &mut BTreeMap<(ClientId, u64), PendingFileWrite>,
    client: ClientId,
    chunk: &FileWriteChunk,
) -> Result<Option<(PaneId, String, String)>, String> {
    let key = (client, chunk.request_id);
    let mut write = if chunk.offset == 0 {
        writes.remove(&key);
        if writes
            .keys()
            .filter(|(writer, _)| *writer == client)
            .count()
            >= MAX_CONCURRENT_FILE_WRITES
        {
            return Err(format!(
                "too many saves in flight (limit {MAX_CONCURRENT_FILE_WRITES})"
            ));
        }
        PendingFileWrite {
            pane: chunk.pane,
            path: chunk.path.clone(),
            bytes: Vec::new(),
        }
    } else {
        let Some(write) = writes.remove(&key) else {
            return Ok(None);
        };
        write
    };
    if write.pane != chunk.pane
        || write.path != chunk.path
        || u64::try_from(write.bytes.len()) != Ok(chunk.offset)
    {
        return Err("the save arrived out of order".to_owned());
    }
    if write.bytes.len().saturating_add(chunk.bytes.len()) > zz_protocol::MAX_FILE_EDIT_BYTES {
        return Err(format!(
            "{} is larger than the {} MiB limit for saving",
            write.path,
            zz_protocol::MAX_FILE_EDIT_BYTES / (1024 * 1024)
        ));
    }
    write.bytes.extend_from_slice(&chunk.bytes);
    if !chunk.last {
        writes.insert(key, write);
        return Ok(None);
    }
    let text = String::from_utf8(write.bytes)
        .map_err(|_| format!("the text for {} is not valid UTF-8", write.path))?;
    Ok(Some((write.pane, write.path, text)))
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

        let chunk = |request_id, offset, last, bytes: &[u8]| FileWriteChunk {
            request_id,
            pane,
            path: path_text.clone(),
            offset,
            last,
            bytes: bytes.to_vec(),
        };
        shared.receive_file_write(
            client,
            ClientKind::Interactive,
            &chunk(3, 0, false, b"af"),
            &outbound,
        );
        assert_eq!(fs::read_to_string(&path).expect("unchanged"), "before\n");
        shared.receive_file_write(
            client,
            ClientKind::Interactive,
            &chunk(3, 2, true, b"ter\n"),
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

        shared.inner.lock().attachment.client_flags.insert(client);
        shared.receive_file_write(
            client,
            ClientKind::Interactive,
            &chunk(5, 0, true, b"watcher\n"),
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
    fn an_attached_client_saves_an_attachment_and_gets_its_path() {
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
                &CommandInvocation::new("new-session", ["-d", "-s", "attach"]),
            )
            .expect("session");
        let pane = context.pane.expect("pane id");
        let outbound = OutboundMailbox::new();
        outbound.accept_client_capabilities(&[zz_protocol::AGENT_FILES_CAPABILITY.to_owned()]);
        let upload = |request_id| {
            (
                request_id,
                pane,
                "../crash.log".to_owned(),
                b"boom\n".to_vec(),
            )
        };

        shared.save_attachment(client, ClientKind::Interactive, upload(1), &outbound);
        assert_eq!(
            answer(&outbound),
            ProtocolMessage::AgentFileAttached {
                request_id: 1,
                result: Err(format!("{pane} is not in an attached session")),
            }
        );

        shared
            .attach(client, context.session.expect("session id"))
            .expect("attach session");
        shared.save_attachment(client, ClientKind::Interactive, upload(2), &outbound);
        let ProtocolMessage::AgentFileAttached {
            request_id: 2,
            result: Ok(saved),
        } = answer(&outbound)
        else {
            panic!("the attachment should be saved");
        };
        let saved = Path::new(&saved);
        assert_eq!(saved.file_name(), Some("crash.log".as_ref()));
        assert_eq!(fs::read(saved).expect("saved file"), b"boom\n");
        let folder = saved.parent().expect("upload folder");
        fs::remove_dir_all(folder).expect("clean up");
        let _ = fs::remove_dir(folder.parent().expect("pane folder"));
    }

    #[test]
    fn an_attachment_keeps_only_its_own_name() {
        assert_eq!(
            [
                "../../etc/passwd",
                "C:\\Users\\a\\report.pdf",
                "..",
                " \u{7}crash.log ",
                "",
            ]
            .map(attachment_name),
            [
                "passwd",
                "report.pdf",
                "attachment",
                "crash.log",
                "attachment"
            ]
        );
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
