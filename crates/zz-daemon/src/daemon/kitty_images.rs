use super::*;

impl Shared {
    pub(super) fn kitty_image_frames(
        &self,
        pane: PaneId,
        terminal: &Arc<TerminalSession>,
        image_id: u32,
        generation: u64,
    ) -> Option<Arc<[Vec<u8>]>> {
        let key = KittyImageKey {
            pane,
            image_id,
            generation,
        };
        self.kitty_image_frames
            .lock()
            .get(&key)
            .filter(|entry| {
                entry
                    .terminal
                    .upgrade()
                    .is_some_and(|current| Arc::ptr_eq(&current, terminal))
            })
            .map(|entry| Arc::clone(&entry.frames))
    }

    pub(super) fn store_kitty_image_frames(
        &self,
        pane: PaneId,
        terminal: &Arc<TerminalSession>,
        image: &zz_terminal::KittyImage,
    ) -> Option<Arc<[Vec<u8>]>> {
        let image_id = image.image_id;
        let generation = image.generation;
        let key = KittyImageKey {
            pane,
            image_id,
            generation,
        };
        let total_bytes = u32::try_from(image.bgra.len()).ok()?;
        let mut frames =
            Vec::with_capacity(1 + image.bgra.len().div_ceil(MAX_KITTY_IMAGE_CHUNK_BYTES));
        let begin = Self::event(EventPayload::KittyImageBegin {
            pane,
            image_id,
            generation,
            width: image.width,
            height: image.height,
            total_bytes,
        });
        let mut encoded = Vec::new();
        if let Err(error) = encode_protocol_message_into(&begin, &mut encoded) {
            log::warn!("could not encode Kitty image {image_id} header for {pane}: {error}");
            return None;
        }
        frames.push(encoded);
        for chunk in image.bgra.chunks(MAX_KITTY_IMAGE_CHUNK_BYTES) {
            let message = Self::event(EventPayload::KittyImageChunk {
                pane,
                image_id,
                generation,
                bytes: chunk.to_vec(),
            });
            let mut encoded = Vec::new();
            if let Err(error) = encode_protocol_message_into(&message, &mut encoded) {
                log::warn!("could not encode Kitty image {image_id} chunk for {pane}: {error}");
                return None;
            }
            frames.push(encoded);
        }
        let frames: Arc<[Vec<u8>]> = frames.into();
        self.kitty_image_frames.lock().insert(
            key,
            KittyImageFrames {
                terminal: Arc::downgrade(terminal),
                frames: Arc::clone(&frames),
            },
        );
        Some(frames)
    }

    pub(super) fn request_kitty_images(
        &self,
        pane: PaneId,
        terminal: &Arc<TerminalSession>,
        referenced: &BTreeSet<(u32, u64)>,
        finish: impl FnOnce(&Arc<Shared>, bool) + Send + 'static,
    ) -> bool {
        let missing = referenced
            .iter()
            .filter(|(image, generation)| {
                self.kitty_image_frames(pane, terminal, *image, *generation)
                    .is_none()
            })
            .copied()
            .collect::<Vec<_>>();
        if missing.is_empty() {
            return true;
        }
        let remaining = Arc::new(std::sync::atomic::AtomicUsize::new(missing.len()));
        let finish = Arc::new(Mutex::new(Some(finish)));
        let success = Arc::new(AtomicBool::new(true));
        for (image_id, generation) in missing {
            let request = terminal.kitty_image_request(image_id, self.terminal_requests.notifier());
            let terminal = Arc::clone(terminal);
            let remaining = Arc::clone(&remaining);
            let finish = Arc::clone(&finish);
            let success = Arc::clone(&success);
            self.terminal_requests
                .submit(request, move |shared, result| {
                    if shared.is_current_image_terminal(pane, &terminal)
                        && let Ok(Some(image)) = result
                        && image.generation == generation
                    {
                        if shared
                            .store_kitty_image_frames(pane, &terminal, &image)
                            .is_none()
                        {
                            success.store(false, Ordering::Release);
                        }
                    } else {
                        success.store(false, Ordering::Release);
                    }
                    if remaining.fetch_sub(1, Ordering::AcqRel) == 1 {
                        finish.lock().take().unwrap()(shared, success.load(Ordering::Acquire));
                    }
                });
        }
        false
    }

    pub(super) fn is_current_image_terminal(
        &self,
        pane: PaneId,
        terminal: &Arc<TerminalSession>,
    ) -> bool {
        let inner = self.inner.lock();
        inner
            .terminals
            .get(&pane)
            .is_some_and(|current| Arc::ptr_eq(current, terminal))
            || inner.clients.values().any(|client| {
                client.command_output.as_ref().is_some_and(|output| {
                    output.pane == pane && Arc::ptr_eq(&output.terminal, terminal)
                })
            })
    }

    pub(super) fn enqueue_kitty_images_for_viewport(
        &self,
        outbound: &OutboundMailbox,
        pane: PaneId,
        terminal: &Arc<TerminalSession>,
        viewport: &TerminalViewport,
    ) -> bool {
        let referenced = viewport
            .kitty_placements
            .iter()
            .map(|placement| (placement.image_id, placement.image_generation))
            .collect::<BTreeSet<_>>();
        if referenced.is_empty() {
            return true;
        }
        let subscriber = self
            .inner
            .lock()
            .clients
            .iter()
            .find_map(|(client, state)| {
                state
                    .subscriber
                    .as_ref()
                    .filter(|subscriber| std::ptr::eq(subscriber.as_ref(), outbound))
                    .map(|subscriber| (*client, Arc::clone(subscriber)))
            });
        let terminal_ready = Arc::clone(terminal);
        if !self.request_kitty_images(pane, terminal, &referenced, move |shared, success| {
            if success
                && shared.is_current_image_terminal(pane, &terminal_ready)
                && let Some((client, subscriber)) = subscriber
            {
                shared.send_full(client, pane, &subscriber);
            }
        }) {
            return false;
        }
        for (image_id, generation) in referenced {
            let Some(frames) = self.kitty_image_frames(pane, terminal, image_id, generation) else {
                continue;
            };
            match outbound.enqueue_kitty_image(pane, image_id, generation, &frames) {
                KittyImageEnqueue::AlreadyDelivered | KittyImageEnqueue::Queued => {}
                KittyImageEnqueue::Closed => return false,
            }
        }
        true
    }

    pub(super) fn evict_absent_kitty_images(
        &self,
        pane: PaneId,
        terminal: &Arc<TerminalSession>,
        referenced: &BTreeSet<(u32, u64)>,
    ) {
        let candidates = self
            .kitty_image_frames
            .lock()
            .keys()
            .filter(|key| key.pane == pane && !referenced.contains(&(key.image_id, key.generation)))
            .copied()
            .collect::<Vec<_>>();
        for key in candidates {
            let request = terminal
                .kitty_image_generation_request(key.image_id, self.terminal_requests.notifier());
            let terminal = Arc::clone(terminal);
            self.terminal_requests
                .submit(request, move |shared, result| {
                    if !shared.is_current_image_terminal(pane, &terminal) {
                        return;
                    }
                    let Ok(generation) = result else {
                        return;
                    };
                    if generation == Some(key.generation) {
                        return;
                    }
                    let removed = shared.kitty_image_frames.lock().remove(&key).is_some();
                    if removed {
                        let subscribers = shared
                            .inner
                            .lock()
                            .clients
                            .values()
                            .filter_map(|client| client.subscriber.clone())
                            .collect::<Vec<_>>();
                        for subscriber in subscribers {
                            subscriber.enqueue_kitty_image_removed_if_generation(
                                pane,
                                key.image_id,
                                key.generation,
                            );
                        }
                    }
                });
        }
    }
}

const MAX_KITTY_IMAGE_CHUNK_BYTES: usize = 1024 * 1024;
