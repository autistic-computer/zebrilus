use super::*;

pub(super) const MAX_PENDING_TERMINALS: usize = 128;

#[cfg(feature = "agent")]
pub(super) const MAX_PENDING_AGENT_BYTES: usize = MAX_AGENT_UPDATES_BYTES + MAX_GUI_TEXT_BYTES;

#[cfg(feature = "agent")]
const MAX_PENDING_AGENT_REPLAY_BYTES: usize = 5 * MAX_AGENT_UPDATES_BYTES + MAX_GUI_TEXT_BYTES;

pub(super) const MAX_OUTBOUND_BYTES: usize = 72 * 1024 * 1024;

pub(super) const MAX_PREVIEW_OUTBOUND_BYTES: usize = MAX_OUTBOUND_BYTES - MAX_ENCODED_FRAME_BYTES;

pub(super) const MAX_RECYCLED_FRAME_BUFFERS: usize = 8;

pub(super) const MAX_RECYCLED_FRAME_CAPACITY: usize = 8 * 1024 * 1024;

pub(super) struct OutboundMailbox {
    pub(super) state: Mutex<OutboundState>,
    pub(super) ready: Condvar,
    #[cfg(unix)]
    pub(super) loop_waker: Mutex<Option<(Arc<mio::Waker>, thread::ThreadId)>>,
    pub(super) terminals_frozen: AtomicBool,
    control: std::sync::OnceLock<shard_sink::ControlFeed>,
}

#[derive(Clone, Debug)]
pub(super) enum OutboundFrame {
    Owned(Vec<u8>),
    Shared(Arc<[u8]>),
    Grouped {
        encoded: Vec<u8>,
        frames: Vec<OutboundFrame>,
    },
    DeferredGrouped {
        sequence: u64,
        encoded_len: usize,
        frames: Vec<OutboundFrame>,
    },
    #[cfg(unix)]
    Partial {
        frame: Box<OutboundFrame>,
        offset: usize,
    },
}

impl std::ops::Deref for OutboundFrame {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        match self {
            Self::Owned(frame) => frame,
            Self::Shared(frame) => frame,
            Self::Grouped { encoded, .. } => encoded,
            Self::DeferredGrouped { .. } => unreachable!("deferred group has not been collected"),
            #[cfg(unix)]
            Self::Partial { frame, offset } => &frame.as_ref().as_ref()[*offset..],
        }
    }
}

impl AsRef<[u8]> for OutboundFrame {
    fn as_ref(&self) -> &[u8] {
        self
    }
}

impl From<Vec<u8>> for OutboundFrame {
    fn from(frame: Vec<u8>) -> Self {
        Self::Owned(frame)
    }
}

impl From<Arc<[u8]>> for OutboundFrame {
    fn from(frame: Arc<[u8]>) -> Self {
        Self::Shared(frame)
    }
}

impl OutboundFrame {
    pub(super) fn is_partial(&self) -> bool {
        #[cfg(unix)]
        {
            matches!(self, Self::Partial { .. })
        }
        #[cfg(not(unix))]
        {
            false
        }
    }

    pub(super) fn len(&self) -> usize {
        match self {
            Self::DeferredGrouped { encoded_len, .. } => *encoded_len,
            _ => self.as_ref().len(),
        }
    }

    pub(super) fn materialize(self) -> Self {
        let Self::DeferredGrouped {
            sequence, frames, ..
        } = self
        else {
            return self;
        };
        let mut encoded = Vec::new();
        zz_protocol::encode_batch_frames_into(sequence, &frames, &mut encoded)
            .expect("admitted deferred group fits its encoded bounds");
        Self::Grouped { encoded, frames }
    }

    #[cfg(test)]
    pub(super) fn into_vec(self) -> Vec<u8> {
        match self.materialize() {
            Self::Owned(frame) => frame,
            Self::Shared(frame) => frame.to_vec(),
            Self::Grouped { encoded, .. } => encoded,
            Self::DeferredGrouped { .. } => unreachable!("group was materialized"),
            #[cfg(unix)]
            Self::Partial { frame, offset } => frame.as_ref().as_ref()[offset..].to_vec(),
        }
    }
}

#[derive(Default)]
pub(super) struct OutboundState {
    pub(super) reliable: VecDeque<OutboundFrame>,
    pub(super) command_output: Option<PendingCommandOutput>,
    pub(super) agent: BTreeMap<PaneId, PendingAgent>,
    pub(super) agent_order: VecDeque<PaneId>,
    pub(super) terminals: BTreeMap<PaneId, PendingTerminal>,
    pub(super) delivered_terminals: BTreeMap<PaneId, TerminalGeneration>,
    pub(super) delivered_images: BTreeMap<PaneId, BTreeMap<u32, u64>>,
    delivered_pasted_images: BTreeMap<PaneId, BTreeMap<u32, u64>>,
    pub(super) terminal_order: VecDeque<PaneId>,
    pub(super) preview_refreshes: BTreeSet<PaneId>,
    pub(super) preview_refresh_order: VecDeque<PaneId>,
    pub(super) recycled_frames: Vec<Vec<u8>>,
    pub(super) recycled_capacity: usize,
    pub(super) queued_bytes: usize,
    pub(super) written_bytes: u64,
    pub(super) discarded_bytes: u64,
    pub(super) closed: bool,
    pub(super) writer_finished: bool,
    pub(super) writer_inflight_bytes: usize,
    pub(super) writer_inflight_messages: usize,
    writer_batch_reliable: usize,
    #[cfg(unix)]
    pub(super) quiet_socket: Option<std::os::fd::OwnedFd>,
    #[cfg(unix)]
    pub(super) direct_socket: Option<std::os::fd::OwnedFd>,
    pub(super) terminals_held: bool,
    pub(super) attach_batch: bool,
    #[cfg(unix)]
    pub(super) attach_rounds: u64,
    #[cfg(unix)]
    pub(super) attach_settling: Option<AttachSettling>,
    pub(super) buffered: bool,
    pub(super) ctrl_collecting: ControlCollection,
    #[cfg(unix)]
    pub(super) flow: Option<flow::FlowWindow>,
    client_capabilities: Vec<&'static str>,
}

#[cfg(unix)]
pub(super) struct AttachSettling {
    pub(super) round: u64,
    pub(super) pending: usize,
    pub(super) flush: Option<bool>,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum ControlCollection {
    #[default]
    None,
    Attach,
    Quiet,
}

#[cfg(any(windows, test))]
pub(super) struct OutboundWriterGuard<'a>(pub(super) &'a OutboundMailbox);

#[cfg(any(windows, test))]
impl Drop for OutboundWriterGuard<'_> {
    fn drop(&mut self) {
        self.0.mark_writer_finished();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TerminalEnqueue {
    Queued,
    NeedsFull,
    Dropped,
    Closed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum KittyImageEnqueue {
    Queued,
    AlreadyDelivered,
    Closed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PastedImageEnqueue {
    Queued,
    AlreadyDelivered,
    Closed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct TerminalGeneration {
    pub(super) content: u64,
    pub(super) view: u64,
    pub(super) dictionary: u32,
    pub(super) columns: u16,
    pub(super) rows: u16,
}

impl TerminalGeneration {
    pub(super) fn precedes(self, other: Self) -> bool {
        self.view < other.view && self.content <= other.content
    }
}

fn newer_terminal_delivered(
    state: &OutboundState,
    pane: PaneId,
    current: TerminalGeneration,
) -> bool {
    state
        .terminals
        .get(&pane)
        .is_some_and(|pending| current.precedes(pending.current))
        || state
            .delivered_terminals
            .get(&pane)
            .is_some_and(|delivered| current.precedes(*delivered))
}

fn terminal_update_redundant(
    state: &OutboundState,
    pane: PaneId,
    transition: TerminalTransition,
    preview: bool,
) -> bool {
    let current = transition.current;
    transition.base.is_none() && newer_terminal_delivered(state, pane, current)
        || state.terminals.get(&pane).is_some_and(|pending| {
            pending.current == current
                && (preview || !pending.preview)
                && (pending.full || transition.base.is_some())
        })
        || state.delivered_terminals.get(&pane) == Some(&current)
}

fn forget_delivered_terminal_state(state: &mut OutboundState, pane: PaneId) {
    state.delivered_terminals.remove(&pane);
    if state
        .terminals
        .get(&pane)
        .is_some_and(|pending| !pending.full)
    {
        remove_pending_terminal(state, pane);
    }
}

fn forget_delivered_terminals_state(state: &mut OutboundState) {
    state.delivered_terminals.clear();
    let patches = state
        .terminals
        .iter()
        .filter_map(|(pane, pending)| (!pending.full).then_some(*pane))
        .collect::<Vec<_>>();
    for pane in patches {
        remove_pending_terminal(state, pane);
    }
}

pub(super) struct PendingTerminal {
    pub(super) encoded: Arc<[u8]>,
    pub(super) current: TerminalGeneration,
    pub(super) preview: bool,
    pub(super) full: bool,
}

pub(super) struct PendingCommandOutput {
    output_id: u64,
    pub(super) encoded: Vec<u8>,
}

#[derive(Default)]
pub(super) struct PendingAgent {
    pub(super) frames: VecDeque<(u64, OutboundFrame)>,
    pub(super) bytes: usize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct TerminalTransition {
    pub(super) base: Option<TerminalGeneration>,
    pub(super) current: TerminalGeneration,
}

#[cfg(test)]
fn patch_transition(patch: &zz_terminal::TerminalViewportPatch) -> TerminalTransition {
    TerminalTransition {
        base: Some(TerminalGeneration {
            content: patch.base_generation,
            view: patch.base_view_generation,
            dictionary: patch.dictionary_generation,
            columns: patch.columns,
            rows: patch.rows,
        }),
        current: TerminalGeneration {
            content: patch.generation,
            view: patch.view_generation,
            dictionary: patch.dictionary_generation,
            columns: patch.columns,
            rows: patch.rows,
        },
    }
}

#[cfg(test)]
pub(super) fn terminal_transition(
    pane: PaneId,
    message: &ProtocolMessage,
) -> Option<TerminalTransition> {
    let ProtocolMessage::Event(Event { payload, .. }) = message else {
        return None;
    };
    match payload {
        EventPayload::TerminalViewport {
            pane: target,
            viewport,
        } if *target == pane => Some(TerminalTransition {
            base: None,
            current: viewport_generation(viewport),
        }),
        EventPayload::TerminalPatch {
            pane: target,
            patch,
        } if *target == pane => Some(patch_transition(patch)),
        _ => None,
    }
}

impl OutboundMailbox {
    pub(super) fn notify_one(&self) {
        #[cfg(unix)]
        if let Some((waker, owner)) = self.loop_waker.lock().as_ref() {
            if *owner != thread::current().id() && !shard_sink::hold_loop_wake(waker) {
                let _ = waker.wake();
            }
            return;
        }
        self.ready.notify_one();
    }

    pub(super) fn notify_all(&self) {
        self.ready.notify_all();
        #[cfg(unix)]
        if let Some((waker, owner)) = self.loop_waker.lock().as_ref()
            && *owner != thread::current().id()
        {
            let _ = waker.wake();
        }
    }

    pub(super) fn new() -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(OutboundState::default()),
            ready: Condvar::new(),
            #[cfg(unix)]
            loop_waker: Mutex::new(None),
            terminals_frozen: AtomicBool::new(false),
            control: std::sync::OnceLock::new(),
        })
    }

    pub(super) fn control_feed(&self) -> Option<&shard_sink::ControlFeed> {
        self.control.get()
    }

    pub(super) fn control_ordered(
        &self,
        order: shard_sink::ControlOrder,
        line: impl FnOnce(&OutboundMailbox) -> bool + Send + 'static,
    ) -> bool {
        match self.control_feed() {
            Some(feed) => feed.order(self, order, line),
            None => line(self),
        }
    }

    pub(super) fn ensure_control_feed(
        &self,
        wake: &Arc<shard_sink::ControlWake>,
    ) -> &shard_sink::ControlFeed {
        self.control
            .get_or_init(|| shard_sink::ControlFeed::new(Arc::clone(wake)))
    }

    pub(super) fn terminals_frozen(&self) -> bool {
        self.terminals_frozen.load(Ordering::Acquire)
    }

    pub(super) fn is_closed(&self) -> bool {
        self.state.lock().closed
    }

    pub(super) fn terminal_pending(&self, pane: PaneId) -> bool {
        self.state.lock().terminals.contains_key(&pane)
    }

    pub(super) fn encode_message(
        &self,
        message: &ProtocolMessage,
    ) -> Result<Vec<u8>, ProtocolError> {
        self.encode_with(|frame| encode_protocol_message_into(message, frame))
    }

    fn encode_with(
        &self,
        encode: impl FnOnce(&mut Vec<u8>) -> Result<(), ProtocolError>,
    ) -> Result<Vec<u8>, ProtocolError> {
        let mut frame = {
            let mut state = self.state.lock();
            take_recycled_frame(&mut state)
        };
        if let Err(error) = encode(&mut frame) {
            self.recycle_frame(frame);
            return Err(error);
        }
        Ok(frame)
    }

    pub(super) fn recycle_frame(&self, frame: Vec<u8>) {
        let mut state = self.state.lock();
        recycle_outbound_frame(&mut state, frame);
    }

    pub(super) fn stats(&self) -> (u64, u64) {
        let state = self.state.lock();
        (state.written_bytes, state.discarded_bytes)
    }

    pub(super) fn accept_client_capabilities(&self, declared: &[String]) {
        self.state.lock().client_capabilities = zz_protocol::CLIENT_CAPABILITY_NAMES
            .iter()
            .chain(zz_protocol::OPT_IN_CLIENT_CAPABILITY_NAMES)
            .copied()
            .filter(|name| declared.iter().any(|capability| capability == name))
            .collect();
    }

    #[cfg(all(feature = "agent", unix))]
    pub(super) fn has_client_capability(&self, capability: &str) -> bool {
        self.state.lock().client_capabilities.contains(&capability)
    }

    #[must_use]
    pub(super) fn enqueue_reliable(&self, message: &ProtocolMessage) -> bool {
        self.enqueue_reliable_with_wakeup(message, true)
    }

    #[must_use]
    pub(super) fn enqueue_reliable_with_wakeup(
        &self,
        message: &ProtocolMessage,
        wakeup: bool,
    ) -> bool {
        let encoded = match self.encode_message(message) {
            Ok(encoded) => encoded,
            Err(error) => {
                log::error!(
                    target: "zz_daemon::diagnostics::outbound",
                    "failed to encode outbound control message: {error}"
                );
                return false;
            }
        };
        self.enqueue_reliable_frame(message, encoded.into(), wakeup)
    }

    pub(super) fn enqueue_reliable_frame(
        &self,
        message: &ProtocolMessage,
        encoded: OutboundFrame,
        wakeup: bool,
    ) -> bool {
        let order = shard_sink::control_order(message);
        let removed_pane = match message {
            ProtocolMessage::Event(Event {
                payload: EventPayload::PaneRemoved(pane),
                ..
            }) => Some(*pane),
            _ => None,
        };
        let removed_images = match message {
            ProtocolMessage::Event(Event {
                payload: EventPayload::KittyImagesRemoved { pane, image_ids },
                ..
            }) => Some((*pane, image_ids.as_slice())),
            _ => None,
        };
        let closes_command_output = match message {
            ProtocolMessage::Event(Event {
                payload:
                    EventPayload::CommandOutput {
                        output_id,
                        viewport: None,
                        ..
                    },
                ..
            }) => Some(*output_id),
            _ => None,
        };
        let mut state = self.state.lock();
        if state.closed {
            return false;
        }
        if let Some(capability) = message.required_capability()
            && !state.client_capabilities.contains(&capability)
        {
            return true;
        }
        if let Some(pane) = removed_pane {
            remove_pending_terminal(&mut state, pane);
            clear_preview_refresh(&mut state, pane);
            clear_pending_agent(&mut state, pane);
            state.delivered_terminals.remove(&pane);
            state.delivered_images.remove(&pane);
            state.delivered_pasted_images.remove(&pane);
        }
        if let Some((pane, image_ids)) = removed_images
            && let Some(delivered) = state.delivered_images.get_mut(&pane)
        {
            for image_id in image_ids {
                delivered.remove(image_id);
            }
            if delivered.is_empty() {
                state.delivered_images.remove(&pane);
            }
        }
        if let Some(output_id) = closes_command_output
            && state
                .command_output
                .as_ref()
                .is_some_and(|pending| output_id == 0 || pending.output_id <= output_id)
        {
            let pending = state
                .command_output
                .take()
                .expect("matching command output is pending");
            state.queued_bytes = state.queued_bytes.saturating_sub(pending.encoded.len());
            discard_outbound_frame(&mut state, pending.encoded);
        }
        if let Some(order) = order
            && let Some(feed) = self.control_feed()
        {
            drop(state);
            return feed.order(self, order, move |mailbox| {
                mailbox.push_reliable(mailbox.state.lock(), encoded, wakeup)
            });
        }
        self.push_reliable(state, encoded, wakeup)
    }

    fn push_reliable(
        &self,
        mut state: parking_lot::MutexGuard<'_, OutboundState>,
        encoded: OutboundFrame,
        wakeup: bool,
    ) -> bool {
        if state.closed {
            return false;
        }
        if state.attach_batch && state.reliable.len() >= MAX_RELIABLE_MESSAGES / 2 {
            state.attach_batch = false;
        }
        if !reserve_outbound_bytes(&mut state, encoded.len(), 0) {
            shed_preview_terminals(&mut state);
        }
        if (!state.buffered
            && state
                .reliable
                .len()
                .saturating_add(state.writer_inflight_messages)
                >= MAX_RELIABLE_MESSAGES)
            || !reserve_outbound_bytes(&mut state, encoded.len(), 0)
        {
            close_outbound_too_far_behind(&mut state);
            self.notify_all();
            return false;
        }
        state.queued_bytes += encoded.len();
        state.reliable.push_back(encoded);
        let wakeup = wakeup && state.ctrl_collecting == ControlCollection::None;
        drop(state);
        if wakeup {
            self.notify_one();
        }
        true
    }

    #[must_use]
    pub(super) fn enqueue_encoded_reliable(&self, encoded: impl Into<OutboundFrame>) -> bool {
        let encoded = encoded.into();
        self.control_ordered(shard_sink::ControlOrder::AfterOutput, move |mailbox| {
            mailbox.enqueue_encoded_reliable_with(encoded, |_| {})
        })
    }

    #[must_use]
    pub(super) fn enqueue_attached(&self, message: &ProtocolMessage) -> bool {
        let encoded = match self.encode_message(message) {
            Ok(encoded) => encoded,
            Err(error) => {
                log::error!(
                    target: "zz_daemon::diagnostics::outbound",
                    "failed to encode outbound attach message: {error}"
                );
                return false;
            }
        };
        self.control_ordered(shard_sink::ControlOrder::AfterOutput, move |mailbox| {
            mailbox.enqueue_encoded_reliable_with(encoded.into(), |state| {
                forget_delivered_terminals_state(state);
                state.terminals_held = false;
            })
        })
    }

    pub(super) fn hold_terminals(&self) {
        let mut state = self.state.lock();
        state.terminals_held = true;
        state.attach_batch = true;
    }

    pub(super) fn release_terminals(&self) {
        let mut state = self.state.lock();
        let held = std::mem::take(&mut state.terminals_held);
        if !std::mem::take(&mut state.attach_batch) && !held {
            return;
        }
        drop(state);
        self.notify_one();
    }

    pub(super) fn enqueue_encoded_reliable_with(
        &self,
        encoded: OutboundFrame,
        before_push: impl FnOnce(&mut OutboundState),
    ) -> bool {
        self.enqueue_encoded_reliable_with_locked(self.state.lock(), encoded, before_push)
    }

    pub(super) fn enqueue_encoded_reliable_with_locked(
        &self,
        mut state: parking_lot::MutexGuard<'_, OutboundState>,
        encoded: OutboundFrame,
        before_push: impl FnOnce(&mut OutboundState),
    ) -> bool {
        if state.closed {
            return false;
        }
        if state.attach_batch && state.reliable.len() >= MAX_RELIABLE_MESSAGES / 2 {
            state.attach_batch = false;
        }
        if !reserve_outbound_bytes(&mut state, encoded.len(), 0) {
            shed_preview_terminals(&mut state);
        }
        if (!state.buffered
            && state
                .reliable
                .len()
                .saturating_add(state.writer_inflight_messages)
                >= MAX_RELIABLE_MESSAGES)
            || !reserve_outbound_bytes(&mut state, encoded.len(), 0)
        {
            close_outbound_too_far_behind(&mut state);
            self.notify_all();
            return false;
        }
        before_push(&mut state);
        state.queued_bytes += encoded.len();
        state.reliable.push_back(encoded);
        let wakeup = state.ctrl_collecting == ControlCollection::None;
        drop(state);
        if wakeup {
            self.notify_one();
        }
        true
    }

    pub(super) fn forget_delivered_terminal(&self, pane: PaneId) {
        forget_delivered_terminal_state(&mut self.state.lock(), pane);
    }

    pub(super) fn forget_delivered_terminals(&self) {
        forget_delivered_terminals_state(&mut self.state.lock());
    }

    pub(super) fn enqueue_kitty_image(
        &self,
        pane: PaneId,
        image_id: u32,
        generation: u64,
        frames: &[Vec<u8>],
    ) -> KittyImageEnqueue {
        let mut state = self.state.lock();
        if state.closed {
            return KittyImageEnqueue::Closed;
        }
        if state
            .delivered_images
            .get(&pane)
            .and_then(|images| images.get(&image_id))
            == Some(&generation)
        {
            return KittyImageEnqueue::AlreadyDelivered;
        }
        let Some(frame_bytes) = frames
            .iter()
            .try_fold(0_usize, |total, frame| total.checked_add(frame.len()))
        else {
            close_outbound(&mut state);
            self.notify_all();
            return KittyImageEnqueue::Closed;
        };
        if !reserve_outbound_bytes(&mut state, frame_bytes, 0) {
            shed_preview_terminals(&mut state);
        }
        if state
            .reliable
            .len()
            .saturating_add(state.writer_inflight_messages)
            .saturating_add(frames.len())
            > MAX_RELIABLE_MESSAGES
            || !reserve_outbound_bytes(&mut state, frame_bytes, 0)
        {
            close_outbound(&mut state);
            self.notify_all();
            return KittyImageEnqueue::Closed;
        }
        for frame in frames {
            state.queued_bytes += frame.len();
            state.reliable.push_back(frame.clone().into());
        }
        state
            .delivered_images
            .entry(pane)
            .or_default()
            .insert(image_id, generation);
        drop(state);
        self.notify_one();
        KittyImageEnqueue::Queued
    }

    #[cfg(test)]
    pub(super) fn enqueue_kitty_images_removed(&self, pane: PaneId, image_ids: &[u32]) {
        let image_ids = {
            let state = self.state.lock();
            let Some(delivered) = state.delivered_images.get(&pane) else {
                return;
            };
            image_ids
                .iter()
                .copied()
                .filter(|image_id| delivered.contains_key(image_id))
                .collect::<Vec<_>>()
        };
        for image_ids in image_ids.chunks(zz_protocol::MAX_KITTY_IMAGE_REMOVALS) {
            let message = ProtocolMessage::Event(Event {
                sequence: Shared::next_sequence(),
                payload: EventPayload::KittyImagesRemoved {
                    pane,
                    image_ids: image_ids.to_vec(),
                },
            });
            if !self.enqueue_reliable(&message) {
                break;
            }
        }
    }

    pub(super) fn enqueue_kitty_image_removed_if_generation(
        &self,
        pane: PaneId,
        image_id: u32,
        generation: u64,
    ) {
        let message = ProtocolMessage::Event(Event {
            sequence: Shared::next_sequence(),
            payload: EventPayload::KittyImagesRemoved {
                pane,
                image_ids: vec![image_id],
            },
        });
        let Ok(encoded) = self.encode_message(&message) else {
            return;
        };
        let state = self.state.lock();
        if state
            .delivered_images
            .get(&pane)
            .and_then(|images| images.get(&image_id))
            != Some(&generation)
        {
            return;
        }
        self.enqueue_encoded_reliable_with_locked(state, encoded.into(), |state| {
            if let Some(images) = state.delivered_images.get_mut(&pane) {
                images.remove(&image_id);
                if images.is_empty() {
                    state.delivered_images.remove(&pane);
                }
            }
        });
    }

    pub(super) fn enqueue_pasted_image(
        &self,
        pane: PaneId,
        number: u32,
        token: u64,
        frames: &[Vec<u8>],
    ) -> PastedImageEnqueue {
        let mut state = self.state.lock();
        if state.closed {
            return PastedImageEnqueue::Closed;
        }
        if state
            .delivered_pasted_images
            .get(&pane)
            .and_then(|images| images.get(&number))
            == Some(&token)
        {
            return PastedImageEnqueue::AlreadyDelivered;
        }
        let Some(frame_bytes) = frames
            .iter()
            .try_fold(0_usize, |total, frame| total.checked_add(frame.len()))
        else {
            close_outbound(&mut state);
            self.notify_all();
            return PastedImageEnqueue::Closed;
        };
        if !reserve_outbound_bytes(&mut state, frame_bytes, 0) {
            shed_preview_terminals(&mut state);
        }
        if state
            .reliable
            .len()
            .saturating_add(state.writer_inflight_messages)
            .saturating_add(frames.len())
            > MAX_RELIABLE_MESSAGES
            || !reserve_outbound_bytes(&mut state, frame_bytes, 0)
        {
            close_outbound(&mut state);
            self.notify_all();
            return PastedImageEnqueue::Closed;
        }
        for frame in frames {
            state.queued_bytes += frame.len();
            state.reliable.push_back(frame.clone().into());
        }
        state
            .delivered_pasted_images
            .entry(pane)
            .or_default()
            .insert(number, token);
        drop(state);
        self.notify_one();
        PastedImageEnqueue::Queued
    }

    #[cfg(feature = "agent")]
    pub(super) fn enqueue_agent(
        &self,
        pane: PaneId,
        first_seq: u64,
        encoded: impl Into<OutboundFrame>,
    ) -> bool {
        let encoded = encoded.into();
        let lagged_from = {
            let mut state = self.state.lock();
            if state.closed {
                return false;
            }
            let (queued_bytes, queued_first) =
                state.agent.get(&pane).map_or((0, first_seq), |queued| {
                    (
                        queued.bytes,
                        queued.frames.front().map_or(first_seq, |(seq, _)| *seq),
                    )
                });
            let exceeds_lane = queued_bytes.saturating_add(encoded.len()) > MAX_PENDING_AGENT_BYTES;
            if !exceeds_lane && !reserve_outbound_bytes(&mut state, encoded.len(), 0) {
                shed_preview_terminals(&mut state);
            }
            if exceeds_lane || !reserve_outbound_bytes(&mut state, encoded.len(), 0) {
                clear_pending_agent(&mut state, pane);
                Some(queued_first)
            } else {
                let fresh = queued_bytes == 0;
                state.queued_bytes += encoded.len();
                let queued = state.agent.entry(pane).or_default();
                queued.bytes = queued.bytes.saturating_add(encoded.len());
                queued.frames.push_back((first_seq, encoded));
                if fresh {
                    state.agent_order.push_back(pane);
                }
                None
            }
        };
        let Some(next_seq) = lagged_from else {
            self.notify_one();
            return true;
        };
        log::warn!(
            target: "zz_daemon::diagnostics::outbound",
            "agent lane for {pane} overflowed; asking for a replay from {next_seq}",
        );
        self.enqueue_reliable(&Shared::event(EventPayload::AgentLagged { pane, next_seq }))
    }

    #[cfg(feature = "agent")]
    pub(super) fn enqueue_agent_replay(&self, pane: PaneId, frames: Vec<(u64, Vec<u8>)>) -> bool {
        self.enqueue_shared_agent_replay(
            pane,
            frames
                .into_iter()
                .map(|(sequence, frame)| (sequence, frame.into()))
                .collect(),
        )
    }

    #[cfg(feature = "agent")]
    fn enqueue_shared_agent_replay(&self, pane: PaneId, frames: Vec<(u64, OutboundFrame)>) -> bool {
        let Some(bytes) = frames
            .iter()
            .try_fold(0_usize, |total, (_, frame)| total.checked_add(frame.len()))
        else {
            return false;
        };
        let mut state = self.state.lock();
        if state.closed {
            return false;
        }
        let replaced = state.agent.get(&pane).map_or(0, |queued| queued.bytes);
        if bytes <= MAX_PENDING_AGENT_REPLAY_BYTES
            && !reserve_outbound_bytes(&mut state, bytes, replaced)
        {
            shed_preview_terminals(&mut state);
        }
        if bytes > MAX_PENDING_AGENT_REPLAY_BYTES
            || !reserve_outbound_bytes(&mut state, bytes, replaced)
        {
            close_outbound(&mut state);
            drop(state);
            self.notify_all();
            return false;
        }
        clear_pending_agent(&mut state, pane);
        if frames.is_empty() {
            return true;
        }
        state.queued_bytes = state.queued_bytes.saturating_add(bytes);
        let queued = state.agent.entry(pane).or_default();
        queued.bytes = bytes;
        queued.frames.extend(frames);
        state.agent_order.push_back(pane);
        drop(state);
        self.notify_one();
        true
    }

    #[cfg(feature = "agent")]
    pub(super) fn cancel_agent(&self, pane: PaneId) {
        let mut state = self.state.lock();
        clear_pending_agent(&mut state, pane);
    }

    #[cfg(test)]
    pub(super) fn enqueue_terminal(
        &self,
        pane: PaneId,
        message: &ProtocolMessage,
    ) -> TerminalEnqueue {
        let Some(transition) = terminal_transition(pane, message) else {
            log::error!("refusing a non-terminal update in the terminal mailbox for {pane}");
            return TerminalEnqueue::Closed;
        };
        self.enqueue_terminal_with(pane, transition, TerminalDelivery::Foreground, || {
            zz_protocol::encode_protocol_message(message).map(Arc::from)
        })
    }

    #[cfg(test)]
    pub(super) fn enqueue_terminal_preview(
        &self,
        pane: PaneId,
        message: &ProtocolMessage,
        foreground_panes: usize,
    ) -> TerminalEnqueue {
        let Some(transition) = terminal_transition(pane, message) else {
            log::error!("refusing a non-terminal preview in the terminal mailbox for {pane}");
            return TerminalEnqueue::Closed;
        };
        self.enqueue_terminal_with(
            pane,
            transition,
            TerminalDelivery::Preview { foreground_panes },
            || zz_protocol::encode_protocol_message(message).map(Arc::from),
        )
    }

    pub(super) fn enqueue_terminal_viewport(
        &self,
        pane: PaneId,
        viewport: &TerminalViewport,
        delivery: TerminalDelivery,
        frames: &TerminalFrames,
    ) -> TerminalEnqueue {
        self.enqueue_terminal_with(
            pane,
            TerminalTransition {
                base: None,
                current: viewport_generation(viewport),
            },
            delivery,
            || frames.full(pane, viewport),
        )
    }

    pub(super) fn enqueue_terminal_with(
        &self,
        pane: PaneId,
        transition: TerminalTransition,
        delivery: TerminalDelivery,
        encode: impl FnOnce() -> Result<Arc<[u8]>, ProtocolError>,
    ) -> TerminalEnqueue {
        {
            let mut state = self.state.lock();
            if state.closed {
                return TerminalEnqueue::Closed;
            }
            if matches!(delivery, TerminalDelivery::Foreground) {
                clear_preview_refresh(&mut state, pane);
            }
            let preview = matches!(delivery, TerminalDelivery::Preview { .. });
            if terminal_update_redundant(&state, pane, transition, preview) {
                return TerminalEnqueue::Dropped;
            }
            if state.terminals.contains_key(&pane) {
                return match delivery {
                    TerminalDelivery::Foreground => TerminalEnqueue::NeedsFull,
                    TerminalDelivery::Preview { .. } => {
                        mark_preview_refresh(&mut state, pane);
                        TerminalEnqueue::Dropped
                    }
                };
            }
            if transition.base.is_some()
                && transition.base != state.delivered_terminals.get(&pane).copied()
            {
                return TerminalEnqueue::NeedsFull;
            }
            if let TerminalDelivery::Preview { foreground_panes } = delivery
                && (state.terminals.len() >= MAX_PENDING_TERMINALS.saturating_sub(foreground_panes)
                    || state.queued_bytes >= MAX_PREVIEW_OUTBOUND_BYTES)
            {
                if foreground_panes < MAX_PENDING_TERMINALS {
                    mark_preview_refresh(&mut state, pane);
                }
                return TerminalEnqueue::Dropped;
            }
        }
        let Ok(encoded) = encode() else {
            log::error!("failed to encode terminal update for {pane}");
            return TerminalEnqueue::Closed;
        };
        #[cfg(unix)]
        let off_loop = self
            .loop_waker
            .lock()
            .as_ref()
            .is_none_or(|(_, owner)| *owner != thread::current().id());
        let mut state = self.state.lock();
        if state.closed {
            return TerminalEnqueue::Closed;
        }
        if matches!(delivery, TerminalDelivery::Foreground) {
            clear_preview_refresh(&mut state, pane);
        }
        let preview = matches!(delivery, TerminalDelivery::Preview { .. });
        if terminal_update_redundant(&state, pane, transition, preview) {
            return TerminalEnqueue::Dropped;
        }
        if state.terminals.contains_key(&pane) {
            return match delivery {
                TerminalDelivery::Foreground => TerminalEnqueue::NeedsFull,
                TerminalDelivery::Preview { .. } => {
                    mark_preview_refresh(&mut state, pane);
                    TerminalEnqueue::Dropped
                }
            };
        }
        if transition.base.is_some()
            && transition.base != state.delivered_terminals.get(&pane).copied()
        {
            return TerminalEnqueue::NeedsFull;
        }
        match delivery {
            TerminalDelivery::Foreground => {
                if state.terminals.len() >= MAX_PENDING_TERMINALS
                    || !reserve_outbound_bytes(&mut state, encoded.len(), 0)
                {
                    shed_preview_terminals(&mut state);
                }
                if state.terminals.len() >= MAX_PENDING_TERMINALS
                    || !reserve_outbound_bytes(&mut state, encoded.len(), 0)
                {
                    close_outbound(&mut state);
                    self.notify_all();
                    return TerminalEnqueue::Closed;
                }
            }
            TerminalDelivery::Preview { foreground_panes } => {
                if state.terminals.len() >= MAX_PENDING_TERMINALS.saturating_sub(foreground_panes)
                    || state
                        .queued_bytes
                        .checked_add(encoded.len())
                        .is_none_or(|total| total > MAX_PREVIEW_OUTBOUND_BYTES)
                {
                    if foreground_panes < MAX_PENDING_TERMINALS
                        && encoded.len() <= MAX_PREVIEW_OUTBOUND_BYTES
                    {
                        mark_preview_refresh(&mut state, pane);
                    }
                    return TerminalEnqueue::Dropped;
                }
            }
        }
        if matches!(delivery, TerminalDelivery::Preview { .. }) {
            clear_preview_refresh(&mut state, pane);
        }
        #[cfg(unix)]
        if off_loop && let Some(written) = write_direct_terminal(&mut state, &encoded) {
            state.delivered_terminals.insert(pane, transition.current);
            if written == encoded.len() {
                return TerminalEnqueue::Queued;
            }
            state.queued_bytes += encoded.len() - written;
            state.reliable.push_back(OutboundFrame::Partial {
                frame: Box::new(OutboundFrame::Shared(encoded)),
                offset: written,
            });
            drop(state);
            self.notify_one();
            return TerminalEnqueue::Queued;
        }
        let idle = state.terminals.is_empty();
        state.queued_bytes += encoded.len();
        state.terminals.insert(
            pane,
            PendingTerminal {
                encoded,
                current: transition.current,
                preview: matches!(delivery, TerminalDelivery::Preview { .. }),
                full: transition.base.is_none(),
            },
        );
        state.terminal_order.push_back(pane);
        drop(state);
        if idle {
            self.notify_one();
        }
        TerminalEnqueue::Queued
    }

    #[cfg(test)]
    pub(super) fn replace_terminal(&self, pane: PaneId, message: &ProtocolMessage) -> bool {
        let Some(transition) = terminal_transition(pane, message) else {
            log::error!("refusing a non-terminal update in the terminal mailbox for {pane}");
            return false;
        };
        self.replace_terminal_with(pane, transition, || {
            zz_protocol::encode_protocol_message(message).map(Arc::from)
        })
    }

    pub(super) fn replace_terminal_viewport(
        &self,
        pane: PaneId,
        viewport: &TerminalViewport,
        frames: &TerminalFrames,
    ) -> bool {
        self.replace_terminal_with(
            pane,
            TerminalTransition {
                base: None,
                current: viewport_generation(viewport),
            },
            || frames.full(pane, viewport),
        )
    }

    fn replace_terminal_with(
        &self,
        pane: PaneId,
        transition: TerminalTransition,
        encode: impl FnOnce() -> Result<Arc<[u8]>, ProtocolError>,
    ) -> bool {
        {
            let state = self.state.lock();
            if state.closed {
                return false;
            }
            if terminal_update_redundant(&state, pane, transition, false) {
                return false;
            }
        }
        let Ok(encoded) = encode() else {
            log::error!("failed to encode coalesced terminal update for {pane}");
            return false;
        };
        let mut state = self.state.lock();
        if state.closed {
            return false;
        }
        if terminal_update_redundant(&state, pane, transition, false) {
            return false;
        }
        clear_preview_refresh(&mut state, pane);
        let mut replaced_len = state
            .terminals
            .get(&pane)
            .map_or(0, |pending| pending.encoded.len());
        if replaced_len == 0 && state.terminals.len() >= MAX_PENDING_TERMINALS
            || !reserve_outbound_bytes(&mut state, encoded.len(), replaced_len)
        {
            shed_preview_terminals(&mut state);
            replaced_len = state
                .terminals
                .get(&pane)
                .map_or(0, |pending| pending.encoded.len());
        }
        if replaced_len == 0 && state.terminals.len() >= MAX_PENDING_TERMINALS
            || !reserve_outbound_bytes(&mut state, encoded.len(), replaced_len)
        {
            close_outbound(&mut state);
            self.notify_all();
            return false;
        }
        state.queued_bytes = state
            .queued_bytes
            .saturating_sub(replaced_len)
            .saturating_add(encoded.len());
        let idle = state.terminals.is_empty();
        let replaced = state.terminals.insert(
            pane,
            PendingTerminal {
                encoded,
                current: transition.current,
                preview: false,
                full: transition.base.is_none(),
            },
        );
        if let Some(replaced) = replaced {
            discard_outbound_frame(&mut state, replaced.encoded);
        } else {
            state.terminal_order.push_back(pane);
        }
        drop(state);
        if idle {
            self.notify_one();
        }
        true
    }

    pub(super) fn replace_command_output(&self, message: &ProtocolMessage) -> bool {
        let output_id = match message {
            ProtocolMessage::Event(Event {
                payload:
                    EventPayload::CommandOutput {
                        output_id,
                        viewport: Some(_),
                        ..
                    },
                ..
            }) => *output_id,
            _ => return false,
        };
        let Ok(encoded) = self.encode_message(message) else {
            log::error!("failed to encode command-output viewport");
            return false;
        };
        self.replace_encoded_command_output(output_id, encoded)
    }

    pub(super) fn replace_encoded_command_output(&self, output_id: u64, encoded: Vec<u8>) -> bool {
        let mut state = self.state.lock();
        if state.closed {
            return false;
        }
        if state
            .command_output
            .as_ref()
            .is_some_and(|pending| pending.output_id > output_id)
        {
            recycle_outbound_frame(&mut state, encoded);
            return true;
        }
        let replaced_len = state
            .command_output
            .as_ref()
            .map_or(0, |pending| pending.encoded.len());
        if !reserve_outbound_bytes(&mut state, encoded.len(), replaced_len) {
            shed_preview_terminals(&mut state);
        }
        if !reserve_outbound_bytes(&mut state, encoded.len(), replaced_len) {
            close_outbound(&mut state);
            self.notify_all();
            return false;
        }
        state.queued_bytes = state
            .queued_bytes
            .saturating_sub(replaced_len)
            .saturating_add(encoded.len());
        let replaced = state
            .command_output
            .replace(PendingCommandOutput { output_id, encoded });
        if let Some(replaced) = replaced {
            discard_outbound_frame(&mut state, replaced.encoded);
        }
        drop(state);
        self.notify_one();
        true
    }

    #[cfg(test)]
    pub(super) fn recv(&self) -> Option<Vec<u8>> {
        let mut state = self.state.lock();
        loop {
            if let Some(frame) = pop_ready_frame(&mut state) {
                state.writer_inflight_bytes = frame.len();
                return Some(frame.into_vec());
            }
            if state.closed {
                return None;
            }
            self.ready.wait(&mut state);
        }
    }

    #[cfg(unix)]
    pub(super) fn try_recv_batch(&self, frames: &mut Vec<OutboundFrame>, max_bytes: usize) {
        let mut guard = self.state.lock();
        let state = &mut *guard;
        let written = state.written_bytes;
        let now = state.flow.is_some().then(Instant::now);
        let empty = state
            .flow
            .as_mut()
            .zip(now)
            .is_some_and(|(flow, now)| flow.sending(written, now));
        let mut bytes = 0;
        let mut messages = 0;
        let base = frames.len();
        while bytes < max_bytes {
            if let Some(flow) = state.flow.as_mut()
                && !flow.room(written + bytes as u64)
            {
                flow.block();
                break;
            }
            let reliable = !state.reliable.is_empty();
            let Some(frame) = pop_ready_frame(state) else {
                break;
            };
            let frame = frame.materialize();
            bytes += frame.len();
            frames.push(frame);
            if reliable {
                messages = frames.len() - base;
            }
            if let Some((flow, now)) = state.flow.as_mut().zip(now)
                && flow.ping_due(written + bytes as u64, false)
            {
                bytes += push_flow_ping(flow, frames, written + bytes as u64, now);
            }
        }
        if bytes != 0
            && let Some((flow, now)) = state.flow.as_mut().zip(now)
            && flow.ping_due(written + bytes as u64, empty)
        {
            bytes += push_flow_ping(flow, frames, written + bytes as u64, now);
        }
        state.writer_inflight_bytes = bytes;
        state.writer_inflight_messages = messages;
        state.writer_batch_reliable = messages;
    }

    #[cfg(unix)]
    pub(super) fn record_write(&self, bytes: usize) {
        let mut state = self.state.lock();
        state.writer_inflight_bytes = state.writer_inflight_bytes.saturating_sub(bytes);
        state.written_bytes = state.written_bytes.saturating_add(bytes as u64);
    }

    #[cfg(unix)]
    pub(super) fn complete_inflight_frame(&self, index: usize) {
        let mut state = self.state.lock();
        if index < state.writer_batch_reliable {
            state.writer_inflight_messages = state.writer_inflight_messages.saturating_sub(1);
        }
    }

    #[cfg(unix)]
    pub(super) fn recycle_written_batch(&self, frames: &mut Vec<OutboundFrame>) {
        let mut state = self.state.lock();
        for frame in frames.drain(..) {
            recycle_outbound_frame(&mut state, frame);
        }
        state.writer_inflight_bytes = 0;
        state.writer_inflight_messages = 0;
        state.writer_batch_reliable = 0;
    }

    #[cfg(any(windows, test))]
    pub(super) fn recv_batch(&self, frames: &mut Vec<OutboundFrame>, max_bytes: usize) -> bool {
        let mut state = self.state.lock();
        loop {
            let mut bytes = 0_usize;
            while bytes < max_bytes {
                let Some(frame) = pop_ready_frame(&mut state) else {
                    break;
                };
                bytes = bytes.saturating_add(frame.len());
                frames.push(frame);
            }
            if !frames.is_empty() {
                state.writer_inflight_bytes = bytes;
                return true;
            }
            if state.closed {
                return false;
            }
            self.ready.wait(&mut state);
        }
    }

    #[cfg(any(windows, test))]
    pub(super) fn finish_batch(&self, frames: &mut Vec<OutboundFrame>) {
        let mut state = self.state.lock();
        for frame in frames.drain(..) {
            state.written_bytes = state.written_bytes.saturating_add(frame.len() as u64);
            recycle_outbound_frame(&mut state, frame);
        }
        state.writer_inflight_bytes = 0;
        state.writer_inflight_messages = 0;
    }

    #[cfg(unix)]
    pub(super) fn enable_flow(&self) {
        self.state.lock().flow = Some(flow::FlowWindow::new(Instant::now()));
    }

    #[cfg(unix)]
    pub(super) fn acknowledge(&self, token: u64) {
        if let Some(flow) = self.state.lock().flow.as_mut() {
            flow.acknowledge(token, Instant::now());
        }
    }

    pub(super) fn close(&self) {
        let mut state = self.state.lock();
        close_outbound(&mut state);
        drop(state);
        self.close_control_feed();
        self.notify_all();
    }

    fn close_control_feed(&self) {
        if let Some(feed) = self.control_feed() {
            feed.close();
        }
    }

    pub(super) fn close_after_flush(&self) {
        if let Some(feed) = self.control_feed() {
            feed.finish(self);
        }
        let mut state = self.state.lock();
        state.closed = true;
        #[cfg(unix)]
        drop(state.quiet_socket.take());
        #[cfg(unix)]
        drop(state.direct_socket.take());
        if matches!(
            state.ctrl_collecting,
            ControlCollection::Quiet | ControlCollection::Attach
        ) {
            self.flush_control_batch_locked(state, false, false);
            return;
        }
        drop(state);
        self.notify_all();
    }

    #[cfg(any(windows, test))]
    pub(super) fn buffered() -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(OutboundState {
                buffered: true,
                ..OutboundState::default()
            }),
            ready: Condvar::new(),
            #[cfg(unix)]
            loop_waker: Mutex::new(None),
            terminals_frozen: AtomicBool::new(false),
            control: std::sync::OnceLock::new(),
        })
    }

    #[cfg(windows)]
    pub(super) fn stop_buffering(&self) {
        self.state.lock().buffered = false;
    }

    #[cfg(any(windows, test))]
    pub(super) fn drain_reliable_into(&self, output: &mut Vec<u8>) {
        let mut state = self.state.lock();
        while let Some(frame) = state.reliable.pop_front() {
            state.queued_bytes = state.queued_bytes.saturating_sub(frame.len());
            let frame = frame.materialize();
            output.extend_from_slice(&frame);
            recycle_outbound_frame(&mut state, frame);
        }
    }

    pub(super) fn mark_writer_finished(&self) {
        let mut state = self.state.lock();
        if std::thread::panicking() {
            close_outbound(&mut state);
        }
        #[cfg(unix)]
        drop(state.quiet_socket.take());
        #[cfg(unix)]
        drop(state.direct_socket.take());
        state.discarded_bytes = state
            .discarded_bytes
            .saturating_add(state.writer_inflight_bytes as u64);
        state.writer_inflight_bytes = 0;
        state.writer_inflight_messages = 0;
        state.writer_finished = true;
        drop(state);
        self.notify_all();
    }

    pub(super) fn wait_writer_finished(&self, deadline: Instant) -> bool {
        let mut state = self.state.lock();
        while !state.writer_finished {
            if self.ready.wait_until(&mut state, deadline).timed_out() {
                return state.writer_finished;
            }
        }
        true
    }

    pub(super) fn queued_reliable(&self) -> Option<(usize, usize)> {
        let state = self.state.lock();
        (!state.closed).then_some((
            state
                .queued_bytes
                .saturating_add(state.writer_inflight_bytes),
            state
                .reliable
                .len()
                .saturating_add(state.writer_inflight_messages),
        ))
    }

    pub(super) fn is_open(&self) -> bool {
        !self.state.lock().closed
    }

    pub(super) fn close_too_far_behind(&self) {
        let mut state = self.state.lock();
        close_outbound_too_far_behind(&mut state);
        drop(state);
        self.close_control_feed();
        self.notify_all();
    }

    pub(super) fn take_preview_refreshes(&self) -> Vec<PaneId> {
        take_preview_refreshes(&mut self.state.lock())
    }

    #[cfg(test)]
    pub(super) fn cancel_terminal(&self, pane: PaneId) {
        let mut state = self.state.lock();
        remove_pending_terminal(&mut state, pane);
        clear_preview_refresh(&mut state, pane);
        state.delivered_terminals.remove(&pane);
        state.delivered_images.remove(&pane);
        state.delivered_pasted_images.remove(&pane);
    }

    pub(super) fn suspend_terminal(&self, pane: PaneId) {
        let mut state = self.state.lock();
        remove_pending_terminal(&mut state, pane);
        clear_preview_refresh(&mut state, pane);
        state.delivered_terminals.remove(&pane);
    }

    pub(super) fn reset_kitty_images(&self) {
        self.state.lock().delivered_images.clear();
    }

    pub(super) fn reset_pasted_images(&self) {
        self.state.lock().delivered_pasted_images.clear();
    }

    pub(super) fn log_diagnostic_snapshot(&self, client: ClientId, reason: &str) {
        let state = self.state.lock();
        log::info!(
            target: "zz_daemon::diagnostics::outbound",
            "snapshot reason={reason} client={client} reliable_messages={} command_output_bytes={} agent_panes={} agent_frames={} terminal_messages={} delivered_terminals={} delivered_image_panes={} terminal_order={} preview_refreshes={} recycled_frames={} recycled_capacity={} queued_bytes={} closed={}",
            state.reliable.len(),
            state
                .command_output
                .as_ref()
                .map_or(0, |pending| pending.encoded.len()),
            state.agent.len(),
            state.agent.values().map(|queued| queued.frames.len()).sum::<usize>(),
            state.terminals.len(),
            state.delivered_terminals.len(),
            state.delivered_images.len(),
            state.terminal_order.len(),
            state.preview_refreshes.len(),
            state.recycled_frames.len(),
            state.recycled_capacity,
            state.queued_bytes,
            state.closed,
        );
        log::trace!(
            target: "zz_daemon::diagnostics::outbound",
            "snapshot reason={reason} client={client} reliable_frame_lengths={:?} command_output_capacity={:?} terminals={:#?} delivered_terminals={:#?} terminal_order={:#?} preview_refresh_order={:#?} recycled_frame_capacities={:?}",
            state.reliable.iter().map(OutboundFrame::len).collect::<Vec<_>>(),
            state
                .command_output
                .as_ref()
                .map(|pending| pending.encoded.capacity()),
            state.terminals.iter().map(|(pane, pending)| (*pane, pending.encoded.len(), pending.current)).collect::<Vec<_>>(),
            state.delivered_terminals,
            state.terminal_order,
            state.preview_refresh_order,
            state.recycled_frames.iter().map(Vec::capacity).collect::<Vec<_>>(),
        );
    }
}

pub(super) fn pop_ready_frame(state: &mut OutboundState) -> Option<OutboundFrame> {
    let partial = state
        .reliable
        .front()
        .is_some_and(OutboundFrame::is_partial);
    if !partial && (state.attach_batch || state.ctrl_collecting != ControlCollection::None) {
        return None;
    }
    if let Some(frame) = state.reliable.pop_front() {
        state.queued_bytes = state.queued_bytes.saturating_sub(frame.len());
        return Some(frame);
    }
    if let Some(pending) = state.command_output.take() {
        state.queued_bytes = state.queued_bytes.saturating_sub(pending.encoded.len());
        return Some(pending.encoded.into());
    }
    while let Some(pane) = state.agent_order.pop_front() {
        let Some(queued) = state.agent.get_mut(&pane) else {
            continue;
        };
        let Some((_, frame)) = queued.frames.pop_front() else {
            state.agent.remove(&pane);
            continue;
        };
        queued.bytes = queued.bytes.saturating_sub(frame.len());
        if queued.frames.is_empty() {
            state.agent.remove(&pane);
        } else {
            state.agent_order.push_back(pane);
        }
        state.queued_bytes = state.queued_bytes.saturating_sub(frame.len());
        return Some(frame);
    }
    if state.terminals_held {
        return None;
    }
    while let Some(pane) = state.terminal_order.pop_front() {
        if let Some(pending) = state.terminals.remove(&pane) {
            state.queued_bytes = state.queued_bytes.saturating_sub(pending.encoded.len());
            state.delivered_terminals.insert(pane, pending.current);
            return Some(pending.encoded.into());
        }
    }
    None
}

#[cfg(unix)]
fn push_flow_ping(
    flow: &mut flow::FlowWindow,
    frames: &mut Vec<OutboundFrame>,
    sent: u64,
    now: Instant,
) -> usize {
    let rtt_us = flow.rtt_us();
    let mut encoded = Vec::new();
    let mut token = sent;
    loop {
        encoded.clear();
        encode_protocol_message_into(&ProtocolMessage::Ping { token, rtt_us }, &mut encoded)
            .expect("a ping always encodes");
        let next = sent + encoded.len() as u64;
        if next == token {
            break;
        }
        token = next;
    }
    flow.pinged(token, now);
    let len = encoded.len();
    frames.push(OutboundFrame::Owned(encoded));
    len
}

pub(super) fn take_recycled_frame(state: &mut OutboundState) -> Vec<u8> {
    let Some(frame) = state.recycled_frames.pop() else {
        return Vec::new();
    };
    state.recycled_capacity = state.recycled_capacity.saturating_sub(frame.capacity());
    frame
}

pub(super) fn recycle_outbound_frame(state: &mut OutboundState, frame: impl Into<OutboundFrame>) {
    if state.closed || state.recycled_frames.len() >= MAX_RECYCLED_FRAME_BUFFERS {
        return;
    }
    let mut frame = match frame.into() {
        OutboundFrame::Owned(frame) => frame,
        OutboundFrame::Shared(_) => return,
        OutboundFrame::Grouped { encoded, frames } => {
            recycle_outbound_frame(state, encoded);
            for frame in frames {
                recycle_outbound_frame(state, frame);
            }
            return;
        }
        OutboundFrame::DeferredGrouped { frames, .. } => {
            for frame in frames {
                recycle_outbound_frame(state, frame);
            }
            return;
        }
        #[cfg(unix)]
        OutboundFrame::Partial { frame, .. } => {
            recycle_outbound_frame(state, *frame);
            return;
        }
    };
    let capacity = frame.capacity();
    if capacity == 0
        || state
            .recycled_capacity
            .checked_add(capacity)
            .is_none_or(|total| total > MAX_RECYCLED_FRAME_CAPACITY)
    {
        return;
    }
    frame.clear();
    state.recycled_capacity += capacity;
    state.recycled_frames.push(frame);
}

fn discard_outbound_frame(state: &mut OutboundState, frame: impl Into<OutboundFrame>) {
    let frame = frame.into();
    state.discarded_bytes = state.discarded_bytes.saturating_add(frame.len() as u64);
    recycle_outbound_frame(state, frame);
}

fn clear_pending_agent(state: &mut OutboundState, pane: PaneId) {
    if let Some(queued) = state.agent.remove(&pane) {
        state.queued_bytes = state.queued_bytes.saturating_sub(queued.bytes);
        for (_, frame) in queued.frames {
            discard_outbound_frame(state, frame);
        }
    }
    state.agent_order.retain(|queued| *queued != pane);
}

fn remove_pending_terminal(state: &mut OutboundState, pane: PaneId) {
    if let Some(pending) = state.terminals.remove(&pane) {
        state.queued_bytes = state.queued_bytes.saturating_sub(pending.encoded.len());
        discard_outbound_frame(state, pending.encoded);
    }
    state.terminal_order.retain(|queued| *queued != pane);
}

pub(super) fn mark_preview_refresh(state: &mut OutboundState, pane: PaneId) {
    if state.preview_refreshes.len() < MAX_PENDING_TERMINALS && state.preview_refreshes.insert(pane)
    {
        state.preview_refresh_order.push_back(pane);
    }
}

fn clear_preview_refresh(state: &mut OutboundState, pane: PaneId) {
    if state.preview_refreshes.remove(&pane) {
        state.preview_refresh_order.retain(|queued| *queued != pane);
    }
}

pub(super) fn take_preview_refreshes(state: &mut OutboundState) -> Vec<PaneId> {
    let OutboundState {
        preview_refreshes,
        preview_refresh_order,
        ..
    } = state;
    preview_refresh_order
        .drain(..)
        .filter(|pane| preview_refreshes.remove(pane))
        .collect()
}

fn shed_preview_terminals(state: &mut OutboundState) {
    let panes = state
        .terminals
        .iter()
        .filter_map(|(pane, pending)| pending.preview.then_some(*pane))
        .collect::<Vec<_>>();
    for pane in panes {
        mark_preview_refresh(state, pane);
        remove_pending_terminal(state, pane);
    }
}

pub(super) fn reserve_outbound_bytes(
    state: &mut OutboundState,
    incoming: usize,
    replaced: usize,
) -> bool {
    state
        .queued_bytes
        .saturating_add(state.writer_inflight_bytes)
        .saturating_sub(replaced)
        .checked_add(incoming)
        .is_some_and(|total| total <= MAX_OUTBOUND_BYTES)
}

#[cfg(unix)]
pub(super) fn try_write_quiet_group(state: &mut OutboundState) -> bool {
    if state.closed
        || state.buffered
        || state.writer_finished
        || state.writer_inflight_bytes != 0
        || state.reliable.len() != 1
        || state.command_output.is_some()
        || !state.agent.is_empty()
        || !state.terminals.is_empty()
        || !state.preview_refreshes.is_empty()
    {
        return false;
    }
    let Some(socket) = &state.quiet_socket else {
        return false;
    };
    let Some(frame @ OutboundFrame::Grouped { .. }) = state.reliable.front() else {
        return false;
    };
    if state.queued_bytes != frame.len() {
        return false;
    }
    let written = match send_nonblocking(socket, frame) {
        Ok(0) => return false,
        Ok(written) => written,
        Err(error) if error == rustix::io::Errno::INTR || error == rustix::io::Errno::AGAIN => {
            return false;
        }
        Err(_) => {
            close_outbound(state);
            return false;
        }
    };
    let frame = state.reliable.pop_front().expect("sole quiet group");
    state.queued_bytes = state.queued_bytes.saturating_sub(written);
    state.written_bytes = state.written_bytes.saturating_add(written as u64);
    if written == frame.len() {
        recycle_outbound_frame(state, frame);
        true
    } else {
        state.reliable.push_front(OutboundFrame::Partial {
            frame: Box::new(frame),
            offset: written,
        });
        false
    }
}

#[cfg(unix)]
pub(super) fn send_nonblocking(
    socket: &std::os::fd::OwnedFd,
    bytes: &[u8],
) -> rustix::io::Result<usize> {
    let flags = rustix::net::SendFlags::DONTWAIT;
    #[cfg(not(any(target_vendor = "apple", target_os = "redox", target_os = "vita")))]
    let flags = flags | rustix::net::SendFlags::NOSIGNAL;
    rustix::net::send(socket, bytes, flags)
}

#[cfg(unix)]
fn write_direct_terminal(state: &mut OutboundState, frame: &[u8]) -> Option<usize> {
    if state.queued_bytes != 0
        || !state.reliable.is_empty()
        || !state.terminals.is_empty()
        || state.writer_inflight_bytes != 0
        || state.writer_finished
        || state.attach_batch
        || state.terminals_held
        || state.attach_settling.is_some()
        || state.ctrl_collecting != ControlCollection::None
    {
        return None;
    }
    let socket = state.direct_socket.as_ref()?;
    match send_nonblocking(socket, frame) {
        Ok(0) => None,
        Ok(written) => {
            state.written_bytes = state.written_bytes.saturating_add(written as u64);
            Some(written)
        }
        Err(error) if error == rustix::io::Errno::INTR || error == rustix::io::Errno::AGAIN => None,
        Err(_) => {
            state.direct_socket = None;
            None
        }
    }
}

fn close_outbound(state: &mut OutboundState) {
    #[cfg(unix)]
    if let Some(socket) = state.quiet_socket.take() {
        let _ = rustix::net::shutdown(&socket, rustix::net::Shutdown::Both);
    }
    #[cfg(unix)]
    drop(state.direct_socket.take());
    state.discarded_bytes = state
        .discarded_bytes
        .saturating_add(state.queued_bytes as u64);
    state.closed = true;
    state.attach_batch = false;
    state.terminals_held = false;
    state.ctrl_collecting = ControlCollection::None;
    state.reliable.clear();
    state.command_output = None;
    state.agent.clear();
    state.agent_order.clear();
    state.terminals.clear();
    state.delivered_terminals.clear();
    state.delivered_images.clear();
    state.delivered_pasted_images.clear();
    state.terminal_order.clear();
    state.preview_refreshes.clear();
    state.preview_refresh_order.clear();
    state.recycled_frames.clear();
    state.recycled_capacity = 0;
    state.queued_bytes = 0;
}

pub(super) fn close_outbound_too_far_behind(state: &mut OutboundState) {
    let message = ProtocolMessage::Event(Event {
        sequence: 0,
        payload: EventPayload::ControlExit {
            reason: "too far behind".to_owned(),
        },
    });
    let mut encoded = Vec::new();
    let encoded = encode_protocol_message_into(&message, &mut encoded)
        .is_ok()
        .then_some(encoded);
    let partial = state
        .reliable
        .front()
        .is_some_and(OutboundFrame::is_partial)
        .then(|| state.reliable.pop_front())
        .flatten();
    let keep_partial = partial.as_ref().is_none_or(|partial| {
        encoded.as_ref().is_some_and(|encoded| {
            partial
                .len()
                .checked_add(encoded.len())
                .is_some_and(|bytes| bytes <= MAX_OUTBOUND_BYTES)
                && MAX_RELIABLE_MESSAGES >= 2
        })
    });
    if keep_partial && let Some(partial) = &partial {
        state.queued_bytes = state.queued_bytes.saturating_sub(partial.len());
    }
    #[cfg(unix)]
    let socket = state.quiet_socket.take();
    #[cfg(unix)]
    if (!keep_partial || encoded.is_none())
        && let Some(socket) = &socket
    {
        let _ = rustix::net::shutdown(socket, rustix::net::Shutdown::Both);
    }
    close_outbound(state);
    if !keep_partial {
        return;
    }
    #[cfg(unix)]
    if socket.is_some() {
        state.attach_batch = false;
        state.terminals_held = false;
    }
    if let Some(partial) = partial {
        state.queued_bytes = partial.len();
        state.reliable.push_back(partial);
        state.attach_batch = false;
        state.terminals_held = false;
    }
    if let Some(encoded) = encoded {
        state.queued_bytes += encoded.len();
        state.reliable.push_back(encoded.into());
    }
}
