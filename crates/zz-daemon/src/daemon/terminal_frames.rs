use super::*;

pub(super) struct PaneFrameFanout {
    diff: TerminalDiffScratch,
    tail: PatchTail,
    scratch: Vec<u8>,
    patches: Vec<(TerminalTransition, Arc<[u8]>)>,
}

impl PaneFrameFanout {
    pub(super) fn new() -> Self {
        Self {
            diff: TerminalDiffScratch::default(),
            tail: PatchTail::default(),
            scratch: Vec::new(),
            patches: Vec::new(),
        }
    }

    pub(super) fn release(&mut self) {
        self.diff.release_shared();
        self.patches.clear();
    }

    pub(super) fn enqueue(
        &mut self,
        subscriber: &OutboundMailbox,
        frames: &TerminalFrames,
        pane: PaneId,
        base: Option<&TerminalViewport>,
        current: &TerminalViewport,
        delivery: TerminalDelivery,
    ) -> TerminalEnqueue {
        let Some(base) = base else {
            return subscriber.enqueue_terminal_viewport(pane, current, delivery, frames);
        };
        let transition = TerminalTransition {
            base: Some(viewport_generation(base)),
            current: viewport_generation(current),
        };
        if let Some((_, frame)) = self.patches.iter().find(|(seen, _)| *seen == transition) {
            let frame = Arc::clone(frame);
            return subscriber.enqueue_terminal_with(pane, transition, delivery, || Ok(frame));
        }
        let Self {
            diff,
            tail,
            scratch,
            patches,
        } = self;
        let Some(patch) = TerminalViewport::diff_shared(base, current, diff) else {
            return subscriber.enqueue_terminal_viewport(pane, current, delivery, frames);
        };
        subscriber.enqueue_terminal_with(pane, transition, delivery, || {
            let frame = encode_terminal_patch_frame(pane, &patch, tail, scratch)?;
            patches.push((transition, Arc::clone(&frame)));
            Ok(frame)
        })
    }
}

pub(super) const MAX_CACHED_FULL_FRAMES: usize = 16;

pub(super) const MAX_CACHED_FULL_FRAME_BYTES: usize = 1024 * 1024;

pub(super) struct CachedFullFrame {
    pane: PaneId,
    generation: TerminalGeneration,
    cells: usize,
    pub(super) frame: Arc<[u8]>,
}

#[derive(Default)]
pub(super) struct CachedFullFrames {
    pub(super) frames: VecDeque<CachedFullFrame>,
    pub(super) bytes: usize,
}

#[derive(Default)]
pub(super) struct TerminalFrames {
    pub(super) full: Mutex<CachedFullFrames>,
}

impl TerminalFrames {
    pub(super) fn full(
        &self,
        pane: PaneId,
        viewport: &TerminalViewport,
    ) -> Result<Arc<[u8]>, ProtocolError> {
        let generation = viewport_generation(viewport);
        let cells = Arc::as_ptr(&viewport.cells).cast::<u8>().addr();
        let cached = self.full.lock().frames.iter().find_map(|cached| {
            (cached.pane == pane && cached.generation == generation && cached.cells == cells)
                .then(|| Arc::clone(&cached.frame))
        });
        if let Some(frame) = cached {
            return Ok(frame);
        }
        let frame = encode_terminal_viewport_frame(pane, viewport)?;
        let mut full = self.full.lock();
        let CachedFullFrames { frames, bytes } = &mut *full;
        if let Some(index) = frames.iter().position(|cached| cached.pane == pane)
            && let Some(replaced) = frames.remove(index)
        {
            *bytes -= replaced.frame.len();
        }
        if frame.len() > MAX_CACHED_FULL_FRAME_BYTES {
            return Ok(frame);
        }
        while frames.len() >= MAX_CACHED_FULL_FRAMES
            || *bytes + frame.len() > MAX_CACHED_FULL_FRAME_BYTES
        {
            let Some(evicted) = frames.pop_front() else {
                break;
            };
            *bytes -= evicted.frame.len();
        }
        *bytes += frame.len();
        frames.push_back(CachedFullFrame {
            pane,
            generation,
            cells,
            frame: Arc::clone(&frame),
        });
        Ok(frame)
    }
}

#[cfg(test)]
thread_local! {
    static TERMINAL_ENCODES: std::cell::Cell<(usize, usize)> = const { std::cell::Cell::new((0, 0)) };
}

#[cfg(test)]
pub(super) fn terminal_encodes() -> (usize, usize) {
    TERMINAL_ENCODES.with(std::cell::Cell::get)
}

fn terminal_stream_sequence(viewport: &TerminalViewport) -> u64 {
    viewport.view_generation
}

fn encode_terminal_patch_frame(
    pane: PaneId,
    patch: &TerminalPatchRef<'_>,
    tail: &mut PatchTail,
    scratch: &mut Vec<u8>,
) -> Result<Arc<[u8]>, ProtocolError> {
    #[cfg(test)]
    TERMINAL_ENCODES.with(|count| count.update(|(patches, fulls)| (patches + 1, fulls)));
    encode_terminal_patch_event_into(
        pane,
        terminal_stream_sequence(patch.current),
        patch,
        tail,
        scratch,
    )?;
    Ok(Arc::from(scratch.as_slice()))
}

fn encode_terminal_viewport_frame(
    pane: PaneId,
    viewport: &TerminalViewport,
) -> Result<Arc<[u8]>, ProtocolError> {
    #[cfg(test)]
    TERMINAL_ENCODES.with(|count| count.update(|(patches, fulls)| (patches, fulls + 1)));
    let mut frame = Vec::new();
    encode_terminal_viewport_event_into(
        pane,
        terminal_stream_sequence(viewport),
        viewport,
        &mut frame,
    )?;
    Ok(frame.into())
}

#[derive(Clone, Copy)]
pub(super) enum TerminalDelivery {
    Foreground,
    Preview { foreground_panes: usize },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TerminalStreamKind {
    Foreground,
    Preview,
}

pub(super) fn viewport_generation(viewport: &TerminalViewport) -> TerminalGeneration {
    TerminalGeneration {
        content: viewport.generation,
        view: viewport.view_generation,
        dictionary: viewport.dictionary_generation,
        columns: viewport.columns,
        rows: viewport.rows,
    }
}
