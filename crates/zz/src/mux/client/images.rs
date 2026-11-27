use super::*;

impl MuxClient {
    pub(crate) fn send_paste_upload(
        &self,
        upload_id: u64,
        pane: PaneId,
        extension: String,
        bytes: &[u8],
    ) -> bool {
        let Some(client) = &self.attached_connection().client else {
            log::trace!(target: "zz::diagnostics::mux", "paste upload skipped: no client");
            return false;
        };
        if let Err(error) = client.send_paste_upload(upload_id, pane, extension, bytes) {
            log::warn!("failed to upload a pasted image: {error}");
            return false;
        }
        true
    }

    pub(crate) fn record_pasted_image(
        &self,
        upload_id: u64,
        pane: PaneId,
        extension: String,
        bytes: &[u8],
    ) -> bool {
        let Some(client) = &self.attached_connection().client else {
            log::trace!(target: "zz::diagnostics::mux", "pasted image record skipped: no client");
            return false;
        };
        if let Err(error) = client.record_pasted_image(upload_id, pane, extension, bytes) {
            log::warn!("failed to record a pasted image: {error}");
            return false;
        }
        true
    }

    pub(super) fn kitty_image_cache(&mut self, pane: PaneId) -> Arc<RwLock<KittyImageCache>> {
        self.kitty_images
            .entry(pane)
            .or_insert_with(|| Arc::new(RwLock::new(KittyImageCache::default())))
            .clone()
    }

    pub(super) fn begin_kitty_image(
        &mut self,
        pane: PaneId,
        image_id: u32,
        generation: u64,
        width: u32,
        height: u32,
        total_bytes: u32,
    ) {
        let cache = self.kitty_image_cache(pane);
        if cache.read().contains(image_id, generation) {
            self.kitty_image_assemblies
                .retain(|(target, id, _), _| *target != pane || *id != image_id);
            return;
        }
        self.kitty_image_assemblies
            .retain(|(target, id, _), _| *target != pane || *id != image_id);
        let total_bytes = usize::try_from(total_bytes).unwrap_or(usize::MAX);
        self.kitty_image_assemblies.insert(
            (pane, image_id, generation),
            KittyImageAssembly {
                width,
                height,
                total_bytes,
                bytes: Vec::with_capacity(total_bytes),
            },
        );
    }

    pub(super) fn push_kitty_image_chunk(
        &mut self,
        pane: PaneId,
        image_id: u32,
        generation: u64,
        bytes: &[u8],
    ) {
        let key = (pane, image_id, generation);
        let complete = {
            let Some(assembly) = self.kitty_image_assemblies.get_mut(&key) else {
                return;
            };
            if assembly
                .bytes
                .len()
                .checked_add(bytes.len())
                .is_none_or(|next| next > assembly.total_bytes)
            {
                log::warn!(
                    "discarding over-declared Kitty image {image_id} generation {generation} for {pane}"
                );
                self.kitty_image_assemblies.remove(&key);
                return;
            }
            assembly.bytes.extend_from_slice(bytes);
            assembly.bytes.len() == assembly.total_bytes
        };
        if !complete {
            return;
        }
        let assembly = self
            .kitty_image_assemblies
            .remove(&key)
            .expect("completed Kitty assembly remains present");
        let cache = self.kitty_image_cache(pane);
        let retain_replaced = Arc::strong_count(&cache) > 2;
        let mut cache = cache.write();
        if cache.contains(image_id, generation) {
            return;
        }
        let expected = usize::try_from(assembly.width)
            .ok()
            .and_then(|width| {
                usize::try_from(assembly.height)
                    .ok()
                    .and_then(|height| width.checked_mul(height))
            })
            .and_then(|pixels| pixels.checked_mul(4));
        if expected != Some(assembly.bytes.len()) {
            log::warn!(
                "discarding malformed Kitty image {image_id} generation {generation} for {pane}"
            );
            return;
        }
        let Some(buffer) = ImageBuffer::<Rgba<u8>, Vec<u8>>::from_raw(
            assembly.width,
            assembly.height,
            assembly.bytes,
        ) else {
            return;
        };
        cache.insert(
            image_id,
            generation,
            Arc::new(RenderImage::new(vec![ImageFrame::new(buffer)])),
            retain_replaced,
        );
    }

    pub(super) fn remove_kitty_images(&mut self, pane: PaneId, image_ids: &[u32]) {
        self.kitty_image_assemblies
            .retain(|(target, id, _), _| *target != pane || !image_ids.contains(id));
        if let Some(cache) = self.kitty_images.get(&pane) {
            let retain_removed = Arc::strong_count(cache) > 1;
            let mut cache = cache.write();
            for image_id in image_ids {
                cache.remove(*image_id, retain_removed);
            }
        }
    }

    pub(super) fn clear_kitty_images(&mut self, pane: PaneId) {
        self.kitty_image_assemblies
            .retain(|(target, _, _), _| *target != pane);
        if let Some(cache) = self.kitty_images.remove(&pane) {
            cache.write().clear();
        }
    }

    pub(super) fn clear_all_kitty_images(&mut self) {
        self.kitty_image_assemblies.clear();
        for cache in std::mem::take(&mut self.kitty_images).into_values() {
            cache.write().clear();
        }
    }

    pub(super) fn clear_all_pasted_images(&mut self) {
        self.pane_images.clear();
        self.pasted_image_assemblies.clear();
        self.pending_pasted_image_previews.clear();
    }

    pub(crate) fn pasted_image(&self, pane: PaneId, number: u32) -> Option<Arc<Image>> {
        self.pane_images
            .get(&pane)
            .and_then(|images| images.get(number))
    }

    pub(crate) fn pasted_image_revision(&self, pane: PaneId) -> u64 {
        self.pane_images
            .get(&pane)
            .map_or(0, |images| images.revision)
    }

    pub(crate) fn fetch_artifact(&mut self, request: ArtifactRequest) {
        let Some(client) = &self.attached_connection().client else {
            request.respond(Err("zz is not connected to its daemon".to_owned()));
            return;
        };
        match client.fetch_artifact(&request.token, &request.path) {
            Ok(request_id) => {
                self.artifact_fetches.insert(
                    request_id,
                    ArtifactFetch {
                        request,
                        begun: None,
                        bytes: Vec::new(),
                    },
                );
            }
            Err(error) => request.respond(Err(error.to_string())),
        }
    }

    pub(crate) fn send_artifact_reply(&self, reply: &ArtifactReply) {
        if let Some(client) = &self.attached_connection().client
            && let Err(error) = client.send_artifact_reply(&reply.token, &reply.text)
        {
            log::warn!("failed to send a page reply: {error}");
        }
    }

    pub(super) fn artifact_progress(&mut self, request_id: u64) {
        let complete = self
            .artifact_fetches
            .get(&request_id)
            .and_then(|fetch| fetch.begun.as_ref().map(|begun| (begun, fetch.bytes.len())))
            .is_some_and(|(begun, received)| received >= begun.total);
        if complete && let Some(fetch) = self.artifact_fetches.remove(&request_id) {
            let begun = fetch.begun.unwrap_or_default();
            fetch.request.respond(Ok(ArtifactFile {
                mime: begun.mime,
                bytes: fetch.bytes,
                reply: begun.reply,
            }));
        }
    }

    pub(crate) fn prefetch_pasted_image(&self, pane: PaneId, number: u32) -> bool {
        let Some(client) = &self.attached_connection().client else {
            return false;
        };
        if let Err(error) = client.fetch_pasted_image(pane, number) {
            log::warn!("failed to fetch pasted image #{number} for {pane}: {error}");
            return false;
        }
        true
    }

    pub(crate) fn open_pasted_image(&mut self, pane: PaneId, number: u32, cx: &mut Context<Self>) {
        if let Some(image) = self.pasted_image(pane, number) {
            cx.emit(AttachmentPreviewRequest { image });
            return;
        }
        self.pending_pasted_image_previews.insert((pane, number));
        if !self.prefetch_pasted_image(pane, number) {
            self.pending_pasted_image_previews.remove(&(pane, number));
        }
    }

    pub(super) fn begin_pasted_image(
        &mut self,
        pane: PaneId,
        number: u32,
        format: PastedImageFormat,
        total_bytes: u32,
    ) {
        let total_bytes = usize::try_from(total_bytes).unwrap_or(usize::MAX);
        self.pasted_image_assemblies.remove(&(pane, number));
        if let Some(images) = self.pane_images.get_mut(&pane) {
            images.remove(number);
        }
        self.pasted_image_assemblies.insert(
            (pane, number),
            PastedImageAssembly {
                format,
                total_bytes,
                bytes: Vec::with_capacity(total_bytes),
            },
        );
    }

    pub(super) fn push_pasted_image_chunk(
        &mut self,
        pane: PaneId,
        number: u32,
        bytes: &[u8],
        cx: &mut Context<Self>,
    ) {
        let key = (pane, number);
        let complete = {
            let Some(assembly) = self.pasted_image_assemblies.get_mut(&key) else {
                return;
            };
            if assembly
                .bytes
                .len()
                .checked_add(bytes.len())
                .is_none_or(|next| next > assembly.total_bytes)
            {
                self.pasted_image_assemblies.remove(&key);
                self.pending_pasted_image_previews.remove(&key);
                return;
            }
            assembly.bytes.extend_from_slice(bytes);
            assembly.bytes.len() == assembly.total_bytes
        };
        if !complete {
            return;
        }
        let assembly = self
            .pasted_image_assemblies
            .remove(&key)
            .expect("the completed pasted-image assembly remains present");
        let image = Arc::new(Image::from_bytes(
            ala_image_format(assembly.format),
            assembly.bytes,
        ));
        self.pane_images
            .entry(pane)
            .or_default()
            .insert(number, Arc::clone(&image));
        if self.pending_pasted_image_previews.remove(&key) {
            cx.emit(AttachmentPreviewRequest { image });
        }
    }

    pub(super) fn pasted_image_unavailable(&mut self, pane: PaneId, number: u32) {
        self.pasted_image_assemblies.remove(&(pane, number));
        self.pending_pasted_image_previews.remove(&(pane, number));
        if let Some(images) = self.pane_images.get_mut(&pane) {
            images.remove(number);
        }
    }
}
