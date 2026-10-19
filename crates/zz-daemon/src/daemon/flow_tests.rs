use std::time::{Duration, Instant};

use zz_protocol::decode_protocol_frame;

use super::flow::FlowWindow;
use super::tests::terminal_test_message;
use super::*;

const PROP: Duration = Duration::from_millis(50);

struct Link {
    capacity: u64,
    prop: Duration,
}

impl Link {
    fn round_trip(&self, in_flight: u64) -> Duration {
        let bdp = u128::from(self.capacity) * PROP.as_micros() / 1_000_000;
        let queued = u128::from(in_flight).saturating_sub(bdp);
        let queue = Duration::from_micros(
            u64::try_from(queued * 1_000_000 / u128::from(self.capacity)).unwrap(),
        );
        self.prop + queue
    }
}

fn run_rounds(
    flow: &mut FlowWindow,
    link: &Link,
    now: &mut Instant,
    sent: &mut u64,
    rounds: usize,
) {
    for _ in 0..rounds {
        let window = flow.window();
        flow.sending(*sent, *now);
        *sent += window;
        flow.pinged(*sent, *now);
        flow.block();
        *now += link.round_trip(window);
        flow.acknowledge(*sent, *now);
    }
}

fn seeded(now: Instant) -> FlowWindow {
    let mut flow = FlowWindow::new(now);
    assert!(flow.sending(0, now));
    flow.pinged(100, now);
    flow.acknowledge(100, now + PROP);
    flow
}

fn near(actual: u64, expected: u64) -> bool {
    actual.abs_diff(expected) * 20 <= expected
}

#[test]
fn an_idle_answer_sets_the_round_trip_without_shrinking_the_window() {
    let now = Instant::now();
    let flow = seeded(now);
    assert_eq!(flow.rtt_us(), 50_000);
    assert_eq!(flow.window(), 64 * 1024);
}

#[test]
fn the_window_grows_a_quarter_per_round_while_the_link_has_room() {
    let mut now = Instant::now();
    let mut flow = seeded(now);
    now += PROP;
    let link = Link {
        capacity: 1_000_000_000,
        prop: PROP,
    };
    let mut sent = 100;
    let before = flow.window();
    run_rounds(&mut flow, &link, &mut now, &mut sent, 1);
    assert!(near(flow.window(), before * 5 / 4), "{}", flow.window());
    run_rounds(&mut flow, &link, &mut now, &mut sent, 40);
    assert_eq!(flow.window(), 8 * 1024 * 1024);
}

#[test]
fn a_full_link_settles_a_quarter_above_its_bandwidth_delay_product() {
    let mut now = Instant::now();
    let mut flow = seeded(now);
    now += PROP;
    let link = Link {
        capacity: 2_000_000,
        prop: PROP,
    };
    let mut sent = 100;
    run_rounds(&mut flow, &link, &mut now, &mut sent, 30);
    assert!(near(flow.window(), 125_000), "{}", flow.window());
}

#[test]
fn a_slower_link_shrinks_the_window_within_a_few_rounds() {
    let mut now = Instant::now();
    let mut flow = seeded(now);
    now += PROP;
    let mut sent = 100;
    let fast = Link {
        capacity: 2_000_000,
        prop: PROP,
    };
    run_rounds(&mut flow, &fast, &mut now, &mut sent, 30);
    let slow = Link {
        capacity: 400_000,
        prop: PROP,
    };
    run_rounds(&mut flow, &slow, &mut now, &mut sent, 30);
    assert!(near(flow.window(), 25_000), "{}", flow.window());
}

#[test]
fn the_window_never_drops_under_its_floor() {
    let mut now = Instant::now();
    let mut flow = seeded(now);
    now += PROP;
    let mut sent = 100;
    let link = Link {
        capacity: 20_000,
        prop: PROP,
    };
    run_rounds(&mut flow, &link, &mut now, &mut sent, 30);
    assert_eq!(flow.window(), 16 * 1024);
}

#[test]
fn idle_answers_never_shrink_a_measured_window() {
    let mut now = Instant::now();
    let mut flow = seeded(now);
    now += PROP;
    let mut sent = 100;
    let link = Link {
        capacity: 2_000_000,
        prop: PROP,
    };
    run_rounds(&mut flow, &link, &mut now, &mut sent, 30);
    let measured = flow.window();
    for _ in 0..20 {
        now += Duration::from_secs(1);
        flow.sending(sent, now);
        sent += 200;
        flow.pinged(sent, now);
        now += PROP;
        flow.acknowledge(sent, now);
    }
    assert_eq!(flow.window(), measured);
}

#[test]
fn pings_go_out_at_the_gap_when_forced_and_never_twice_for_the_same_bytes() {
    let now = Instant::now();
    let mut flow = FlowWindow::new(now);
    assert!(!flow.ping_due(1000, false));
    assert!(flow.ping_due(1000, true));
    assert!(flow.ping_due(16 * 1024, false));
    flow.pinged(16 * 1024, now);
    assert!(!flow.ping_due(16 * 1024, true));
}

#[test]
fn a_stale_or_unknown_answer_changes_nothing() {
    let now = Instant::now();
    let mut flow = seeded(now);
    flow.acknowledge(100, now + Duration::from_secs(1));
    flow.acknowledge(7, now + Duration::from_secs(1));
    assert_eq!(flow.rtt_us(), 50_000);
    assert!(flow.room(100 + 64 * 1024));
    flow.pinged(200, now);
    assert!(!flow.room(100 + 64 * 1024));
    assert!(flow.room(99 + 64 * 1024));
}

fn flow_mailbox() -> Arc<OutboundMailbox> {
    let mailbox = OutboundMailbox::new();
    mailbox.enable_flow();
    mailbox
}

fn batch(mailbox: &OutboundMailbox) -> Vec<OutboundFrame> {
    let mut frames = Vec::new();
    mailbox.try_recv_batch(&mut frames, usize::MAX);
    let written = frames.iter().map(OutboundFrame::len).sum();
    mailbox.record_write(written);
    let decoded = frames
        .iter()
        .map(|frame| frame.to_vec())
        .collect::<Vec<_>>();
    mailbox.recycle_written_batch(&mut frames);
    decoded.into_iter().map(OutboundFrame::Owned).collect()
}

fn ping_token(frame: &[u8]) -> Option<u64> {
    match decode_protocol_frame(frame) {
        Ok(ProtocolMessage::Ping { token, .. }) => Some(token),
        _ => None,
    }
}

fn payloads(frames: &[OutboundFrame]) -> usize {
    frames
        .iter()
        .filter(|frame| ping_token(frame).is_none())
        .count()
}

#[test]
fn a_shut_window_holds_frames_until_a_ping_inside_it_is_answered() {
    let mailbox = flow_mailbox();
    for _ in 0..5 {
        assert!(mailbox.enqueue_encoded_reliable(vec![0_u8; 20_000]));
    }
    let first = batch(&mailbox);
    assert_eq!(payloads(&first), 4);
    let tokens = first
        .iter()
        .filter_map(|frame| ping_token(frame))
        .collect::<Vec<_>>();
    assert_eq!(tokens.len(), 4);
    assert!(batch(&mailbox).is_empty());
    mailbox.acknowledge(tokens[3]);
    let second = batch(&mailbox);
    assert_eq!(payloads(&second), 1);
}

#[test]
fn a_window_with_no_ping_in_flight_never_waits() {
    let mailbox = flow_mailbox();
    for _ in 0..8 {
        assert!(mailbox.enqueue_encoded_reliable(vec![0_u8; 20_000]));
    }
    let first = batch(&mailbox);
    let last = first
        .iter()
        .filter_map(|frame| ping_token(frame))
        .next_back()
        .unwrap();
    mailbox.acknowledge(last);
    assert_eq!(payloads(&batch(&mailbox)), 4);
}

#[test]
fn frames_enqueued_behind_a_shut_window_coalesce_to_the_newest() {
    let mailbox = flow_mailbox();
    for _ in 0..4 {
        assert!(mailbox.enqueue_encoded_reliable(vec![0_u8; 20_000]));
    }
    let first = batch(&mailbox);
    let token = first
        .iter()
        .filter_map(|frame| ping_token(frame))
        .next_back()
        .unwrap();
    let pane = PaneId(4);
    let older = terminal_test_message(pane, 1, 1);
    let newer = terminal_test_message(pane, 2, 2);
    assert_eq!(
        mailbox.enqueue_terminal(pane, &older),
        TerminalEnqueue::Queued
    );
    assert!(batch(&mailbox).is_empty());
    assert_eq!(
        mailbox.enqueue_terminal(pane, &newer),
        TerminalEnqueue::NeedsFull
    );
    assert!(mailbox.replace_terminal(pane, &newer));
    mailbox.acknowledge(token);
    let frames = batch(&mailbox);
    let terminals = frames
        .iter()
        .filter(|frame| ping_token(frame).is_none())
        .map(|frame| decode_protocol_frame(frame).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(terminals, vec![newer]);
}

#[test]
fn a_mailbox_without_flow_neither_pings_nor_waits() {
    let mailbox = OutboundMailbox::new();
    for _ in 0..8 {
        assert!(mailbox.enqueue_encoded_reliable(vec![0_u8; 20_000]));
    }
    let frames = batch(&mailbox);
    assert_eq!(frames.len(), 8);
    assert!(frames.iter().all(|frame| frame.len() == 20_000));
}
