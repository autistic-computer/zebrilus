use super::*;

impl Shared {
    pub(super) fn input_paste(
        &self,
        client: ClientId,
        pane: PaneId,
        text: &str,
    ) -> Result<(), ServerError> {
        for sink in self.input_sinks(client, pane)? {
            match sink {
                PaneSink::Terminal(terminal) => terminal.view_action(
                    TerminalViewId(client.0),
                    zz_terminal::TerminalViewAction::Paste(text.to_owned()),
                ),
                PaneSink::Browser(target) => self.publish_for_pane(
                    target,
                    &EventPayload::BrowserCommand {
                        pane: target,
                        command: BrowserCommand::SendKeys(vec![zz_protocol::KeyToken::Literal(
                            text.to_owned(),
                        )]),
                    },
                ),
            }
        }
        Ok(())
    }

    pub(super) fn begin_paste_upload(
        &self,
        client: ClientId,
        kind: ClientKind,
        upload_id: u64,
        pane: PaneId,
        purpose: PasteUploadPurpose,
        extension: String,
        total_bytes: u32,
    ) {
        if kind != ClientKind::Interactive {
            return;
        }
        if self.inner.lock().attachment.client_flags.contains(client) {
            return;
        }
        if !zz_protocol::paste_upload_extension_is_valid(&extension)
            || purpose == PasteUploadPurpose::RecordPastedImage
                && PastedImageFormat::from_extension(&extension).is_none()
            || total_bytes == 0
            || total_bytes > zz_protocol::MAX_PASTE_UPLOAD_BYTES
        {
            self.reject_paste_upload(
                client,
                Some(pane),
                "the paste upload is malformed".to_owned(),
            );
            return;
        }
        let total_bytes = total_bytes as usize;
        let mut inner = self.inner.lock();
        if !client_is_attached_to_pane(&inner, client, pane) {
            drop(inner);
            self.reject_paste_upload(
                client,
                Some(pane),
                format!("cannot paste an image into {pane}: the pane is not attached"),
            );
            return;
        }
        let in_flight = inner
            .transfers
            .paste_uploads
            .keys()
            .filter(|(owner, id)| *owner == client && *id != upload_id)
            .count();
        if in_flight >= MAX_CONCURRENT_PASTE_UPLOADS {
            drop(inner);
            self.reject_paste_upload(
                client,
                Some(pane),
                format!("too many image pastes in flight (limit {MAX_CONCURRENT_PASTE_UPLOADS})"),
            );
            return;
        }
        inner.transfers.paste_uploads.insert(
            (client, upload_id),
            PasteUpload {
                pane,
                purpose,
                extension,
                total_bytes,
                bytes: Vec::with_capacity(total_bytes),
            },
        );
    }

    pub(super) fn extend_paste_upload(&self, client: ClientId, upload_id: u64, bytes: &[u8]) {
        let key = (client, upload_id);
        let finished = {
            let mut inner = self.inner.lock();
            let Some(upload) = inner.transfers.paste_uploads.get(&key) else {
                return;
            };
            if upload.bytes.len().saturating_add(bytes.len()) > upload.total_bytes {
                let pane = upload.pane;
                inner.transfers.paste_uploads.remove(&key);
                drop(inner);
                self.reject_paste_upload(
                    client,
                    Some(pane),
                    "the pasted image sent more bytes than it declared".to_owned(),
                );
                return;
            }
            let upload = inner
                .transfers
                .paste_uploads
                .get_mut(&key)
                .expect("the upload was present a statement ago");
            upload.bytes.extend_from_slice(bytes);
            if upload.bytes.len() < upload.total_bytes {
                return;
            }
            inner.transfers.paste_uploads.remove(&key)
        };
        let Some(upload) = finished else {
            return;
        };
        self.finish_paste_upload(client, upload_id, upload);
    }

    fn finish_paste_upload(&self, client: ClientId, upload_id: u64, upload: PasteUpload) {
        if upload.purpose == PasteUploadPurpose::RecordPastedImage {
            self.record_pasted_image(client, upload);
            return;
        }
        let path = match write_paste_upload(
            &self.paste_directory,
            &format!("paste-{client}-{upload_id}"),
            &upload.extension,
            &upload.bytes,
        ) {
            Ok(path) => path,
            Err(error) => {
                self.reject_paste_upload(
                    client,
                    Some(upload.pane),
                    format!("could not save the pasted image: {error}"),
                );
                return;
            }
        };
        prune_paste_uploads(&self.paste_directory, PASTE_UPLOAD_RETENTION);
        let Some(path) = path.to_str() else {
            self.reject_paste_upload(
                client,
                Some(upload.pane),
                "the pasted image landed on a path that is not valid UTF-8".to_owned(),
            );
            return;
        };
        if let Some(format) = PastedImageFormat::from_extension(&upload.extension) {
            self.stage_pending_pasted_image(client, upload.pane, format, Arc::from(upload.bytes));
        }
        if let Err(error) = self.input_paste(client, upload.pane, path) {
            self.reject_paste_upload(
                client,
                Some(upload.pane),
                format!("could not paste the image path: {error}"),
            );
        }
    }

    fn record_pasted_image(&self, client: ClientId, upload: PasteUpload) {
        let Some(format) = PastedImageFormat::from_extension(&upload.extension) else {
            self.reject_paste_upload(
                client,
                Some(upload.pane),
                "the recorded pasted image format is unsupported".to_owned(),
            );
            return;
        };
        if !self.stage_pending_pasted_image(client, upload.pane, format, Arc::from(upload.bytes)) {
            self.reject_paste_upload(
                client,
                Some(upload.pane),
                format!(
                    "cannot record an image for {}: the pane is no longer attached",
                    upload.pane
                ),
            );
        }
    }

    fn stage_pending_pasted_image(
        &self,
        client: ClientId,
        pane: PaneId,
        format: PastedImageFormat,
        bytes: Arc<[u8]>,
    ) -> bool {
        static NEXT_PASTED_IMAGE_TOKEN: AtomicU64 = AtomicU64::new(1);

        let token = NEXT_PASTED_IMAGE_TOKEN
            .fetch_add(1, Ordering::Relaxed)
            .max(1);
        let registered = {
            let inner = self.inner.lock();
            if client_is_attached_to_pane(&inner, client, pane) {
                inner.terminals.get(&pane).cloned().map(|terminal| {
                    let admission = self
                        .pasted_images
                        .lock()
                        .entry(pane)
                        .or_default()
                        .push_pending(PendingPastedImage {
                            token,
                            format,
                            bytes,
                        });
                    (terminal, admission)
                })
            } else {
                None
            }
        };
        let Some((terminal, admission)) = registered else {
            return false;
        };
        for number in admission.evicted_numbers {
            terminal.unbind_pasted_image(number);
        }
        if admission.retained {
            terminal.open_pending_paste(token);
        }
        true
    }

    pub(super) fn bind_pasted_image(
        &self,
        pane: PaneId,
        terminal: &Arc<TerminalSession>,
        token: u64,
        number: u32,
    ) {
        if !self.is_current_terminal(pane, terminal) {
            return;
        }
        let evicted = self
            .pasted_images
            .lock()
            .get_mut(&pane)
            .and_then(|images| images.bind(token, number));
        let Some(evicted) = evicted else {
            terminal.unbind_pasted_image(number);
            return;
        };
        for number in evicted {
            terminal.unbind_pasted_image(number);
        }
    }

    pub(super) fn expire_pending_pasted_image(
        &self,
        pane: PaneId,
        terminal: &Arc<TerminalSession>,
        token: u64,
    ) {
        if !self.is_current_terminal(pane, terminal) {
            return;
        }
        if let Some(images) = self.pasted_images.lock().get_mut(&pane) {
            images.expire(token);
        }
    }

    pub(super) fn fetch_pasted_image(&self, client: ClientId, pane: PaneId, number: u32) {
        let subscriber = {
            let inner = self.inner.lock();
            client_is_attached_to_pane(&inner, client, pane)
                .then(|| {
                    inner
                        .client(client)
                        .and_then(|c| c.subscriber.as_ref())
                        .cloned()
                })
                .flatten()
        };
        let Some(subscriber) = subscriber else {
            return;
        };
        let image = {
            let mut panes = self.pasted_images.lock();
            panes
                .get_mut(&pane)
                .and_then(|images| images.images.get_mut(&number))
                .and_then(|image| {
                    pasted_image_frames(pane, number, image).map(|frames| (image.token, frames))
                })
        };
        let Some((token, frames)) = image else {
            let _ = subscriber
                .enqueue_reliable(&ProtocolMessage::PastedImageUnavailable { pane, number });
            return;
        };
        let _ = subscriber.enqueue_pasted_image(pane, number, token, &frames);
    }

    fn reject_paste_upload(&self, client: ClientId, pane: Option<PaneId>, text: String) {
        log::warn!(
            target: "zz_daemon::diagnostics::input",
            "paste upload rejected client={client} pane={pane:?}: {text}"
        );
        self.publish_to_client(
            client,
            EventPayload::ClientMessage {
                pane,
                kind: ClientMessageKind::Error,
                text,
            },
        );
    }
}

pub(super) const MAX_CONCURRENT_PASTE_UPLOADS: usize = 2;

pub(super) const PASTE_UPLOAD_RETENTION: usize = 8;

pub(super) fn write_paste_upload(
    directory: &Path,
    stem: &str,
    extension: &str,
    bytes: &[u8],
) -> Result<PathBuf, std::io::Error> {
    fs::create_dir_all(directory)?;
    #[cfg(unix)]
    fs::set_permissions(
        directory,
        <fs::Permissions as std::os::unix::fs::PermissionsExt>::from_mode(0o700),
    )?;
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut attempt = 0u32;
    loop {
        let path = if attempt == 0 {
            directory.join(format!("{stem}.{extension}"))
        } else {
            directory.join(format!("{stem}-{attempt}.{extension}"))
        };
        match options.open(&path) {
            Ok(mut file) => {
                file.write_all(bytes)?;
                file.flush()?;
                return Ok(path);
            }
            Err(error) if error.kind() == ErrorKind::AlreadyExists => attempt += 1,
            Err(error) => return Err(error),
        }
    }
}

pub(super) fn prune_paste_uploads(directory: &Path, keep: usize) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    let mut uploads = Vec::new();
    for entry in entries.flatten() {
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if !metadata.is_file() {
            continue;
        }
        uploads.push((metadata.modified().unwrap_or(UNIX_EPOCH), entry.path()));
    }
    if uploads.len() <= keep {
        return;
    }
    uploads.sort_unstable_by_key(|(modified, _)| Reverse(*modified));
    for (_, path) in uploads.into_iter().skip(keep) {
        let _ = fs::remove_file(path);
    }
}

fn pasted_image_frames(
    pane: PaneId,
    number: u32,
    image: &mut StoredPastedImage,
) -> Option<Arc<[Vec<u8>]>> {
    if let Some(frames) = &image.frames {
        return Some(Arc::clone(frames));
    }
    let total_bytes = u32::try_from(image.bytes.len()).ok()?;
    let mut frames = Vec::with_capacity(
        1 + image
            .bytes
            .len()
            .div_ceil(zz_protocol::MAX_PASTE_UPLOAD_CHUNK_BYTES),
    );
    let begin = ProtocolMessage::PastedImageBegin {
        pane,
        number,
        format: image.format,
        total_bytes,
    };
    let mut encoded = Vec::new();
    encode_protocol_message_into(&begin, &mut encoded).ok()?;
    frames.push(encoded);
    for chunk in image
        .bytes
        .chunks(zz_protocol::MAX_PASTE_UPLOAD_CHUNK_BYTES)
    {
        let message = ProtocolMessage::PastedImageChunk {
            pane,
            number,
            bytes: chunk.to_vec(),
        };
        let mut encoded = Vec::new();
        encode_protocol_message_into(&message, &mut encoded).ok()?;
        frames.push(encoded);
    }
    let frames: Arc<[Vec<u8>]> = frames.into();
    image.frames = Some(Arc::clone(&frames));
    Some(frames)
}

pub(super) const MAX_PASTED_IMAGES_PER_PANE: usize = 8;

pub(super) const MAX_PASTED_IMAGE_BYTES_PER_PANE: usize = 24 * 1024 * 1024;

pub(super) fn paste_upload_directory(socket_path: &Path) -> PathBuf {
    #[cfg(unix)]
    let root = socket_path
        .parent()
        .map_or_else(|| PathBuf::from("paste"), |parent| parent.join("paste"));
    #[cfg(not(unix))]
    let root = std::env::temp_dir().join("zz").join("paste");
    root.join(
        socket_path
            .file_name()
            .unwrap_or_else(|| OsStr::new("default")),
    )
}

#[derive(Debug)]
pub(super) struct PendingPastedImage {
    pub(super) token: u64,
    pub(super) format: PastedImageFormat,
    pub(super) bytes: Arc<[u8]>,
}

#[derive(Debug)]
pub(super) struct StoredPastedImage {
    pub(super) token: u64,
    pub(super) format: PastedImageFormat,
    pub(super) bytes: Arc<[u8]>,
    pub(super) frames: Option<Arc<[Vec<u8>]>>,
}

#[derive(Default, Debug)]
pub(super) struct PanePastedImages {
    pub(super) pending: VecDeque<PendingPastedImage>,
    pub(super) images: BTreeMap<u32, StoredPastedImage>,
    pub(super) order: VecDeque<(u32, u64)>,
    pub(super) pending_bytes: usize,
    pub(super) stored_bytes: usize,
}

pub(super) struct PastedImageAdmission {
    pub(super) evicted_numbers: Vec<u32>,
    pub(super) retained: bool,
}

impl PanePastedImages {
    pub(super) fn push_pending(&mut self, pending: PendingPastedImage) -> PastedImageAdmission {
        let token = pending.token;
        self.pending_bytes = self.pending_bytes.saturating_add(pending.bytes.len());
        self.pending.push_back(pending);
        let evicted_numbers = self.enforce_limits();
        PastedImageAdmission {
            evicted_numbers,
            retained: self.pending.iter().any(|pending| pending.token == token),
        }
    }

    pub(super) fn bind(&mut self, token: u64, number: u32) -> Option<Vec<u32>> {
        let position = self
            .pending
            .iter()
            .position(|pending| pending.token == token)?;
        let pending = self.pending.remove(position)?;
        self.pending_bytes = self.pending_bytes.saturating_sub(pending.bytes.len());
        if let Some(replaced) = self.images.remove(&number) {
            self.stored_bytes = self.stored_bytes.saturating_sub(replaced.bytes.len());
            self.order.retain(|(stored_number, stored_token)| {
                *stored_number != number || *stored_token != replaced.token
            });
        }
        self.stored_bytes = self.stored_bytes.saturating_add(pending.bytes.len());
        self.order.push_back((number, token));
        self.images.insert(
            number,
            StoredPastedImage {
                token,
                format: pending.format,
                bytes: pending.bytes,
                frames: None,
            },
        );
        Some(self.enforce_limits())
    }

    fn enforce_limits(&mut self) -> Vec<u32> {
        let mut evicted = Vec::new();
        while self.images.len().saturating_add(self.pending.len()) > MAX_PASTED_IMAGES_PER_PANE
            || self.stored_bytes.saturating_add(self.pending_bytes)
                > MAX_PASTED_IMAGE_BYTES_PER_PANE
        {
            while self.order.front().is_some_and(|(number, token)| {
                self.images
                    .get(number)
                    .is_none_or(|image| image.token != *token)
            }) {
                self.order.pop_front();
            }
            if let Some((evicted_number, evicted_token)) = self.order.pop_front() {
                if let Some(image) = self.images.get(&evicted_number)
                    && image.token == evicted_token
                {
                    let image = self
                        .images
                        .remove(&evicted_number)
                        .expect("the pasted image was present");
                    self.stored_bytes = self.stored_bytes.saturating_sub(image.bytes.len());
                    evicted.push(evicted_number);
                }
            } else if let Some(pending) = self.pending.pop_back() {
                self.pending_bytes = self.pending_bytes.saturating_sub(pending.bytes.len());
            } else {
                break;
            }
        }
        evicted
    }

    fn expire(&mut self, token: u64) {
        if let Some(position) = self
            .pending
            .iter()
            .position(|pending| pending.token == token)
            && let Some(pending) = self.pending.remove(position)
        {
            self.pending_bytes = self.pending_bytes.saturating_sub(pending.bytes.len());
        }
    }
}
