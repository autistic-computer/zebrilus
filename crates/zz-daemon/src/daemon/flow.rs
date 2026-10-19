use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

const INITIAL_WINDOW: u64 = 64 * 1024;
const MIN_WINDOW: u64 = 16 * 1024;
const MAX_WINDOW: u64 = 8 * 1024 * 1024;
const MIN_PING_GAP: u64 = 4 * 1024;
const RATE_SAMPLES: usize = 40;
const MAX_PINGS: usize = 256;
const LOG_EVERY: Duration = Duration::from_secs(1);

pub(super) struct FlowWindow {
    acked: u64,
    acked_at: Instant,
    pinged: u64,
    pings: VecDeque<SentPing>,
    min_rtt: Option<Duration>,
    rates: VecDeque<u64>,
    window: u64,
    blocked: u64,
    logged_at: Instant,
}

struct SentPing {
    token: u64,
    sent_at: Instant,
    acked: u64,
    acked_at: Instant,
    blocked: u64,
}

impl FlowWindow {
    pub(super) fn new(now: Instant) -> Self {
        Self {
            acked: 0,
            acked_at: now,
            pinged: 0,
            pings: VecDeque::new(),
            min_rtt: None,
            rates: VecDeque::with_capacity(RATE_SAMPLES),
            window: INITIAL_WINDOW,
            blocked: 0,
            logged_at: now,
        }
    }

    pub(super) fn block(&mut self) {
        self.blocked += 1;
    }

    pub(super) fn room(&self, sent: u64) -> bool {
        self.pings.is_empty() || sent.saturating_sub(self.acked) < self.window
    }

    pub(super) fn sending(&mut self, sent: u64, now: Instant) -> bool {
        let empty = sent <= self.acked;
        if empty {
            self.acked_at = now;
        }
        empty
    }

    pub(super) fn ping_due(&self, sent: u64, force: bool) -> bool {
        sent > self.pinged && (force || sent - self.pinged >= self.ping_gap())
    }

    pub(super) fn pinged(&mut self, token: u64, now: Instant) {
        if self.pings.len() >= MAX_PINGS {
            self.pings.pop_front();
        }
        self.pings.push_back(SentPing {
            token,
            sent_at: now,
            acked: self.acked,
            acked_at: self.acked_at,
            blocked: self.blocked,
        });
        self.pinged = token;
    }

    pub(super) fn acknowledge(&mut self, token: u64, now: Instant) {
        let mut answered = None;
        while self.pings.front().is_some_and(|ping| ping.token <= token) {
            let ping = self.pings.pop_front().expect("front ping");
            if ping.token == token {
                answered = Some(ping);
            }
        }
        let Some(ping) = answered else {
            return;
        };
        if token <= self.acked {
            return;
        }
        let rtt = now.saturating_duration_since(ping.sent_at);
        let min_rtt = self.min_rtt.map_or(rtt, |seen| seen.min(rtt));
        self.min_rtt = Some(min_rtt);
        let rate = bytes_per_second(
            token - ping.acked,
            now.saturating_duration_since(ping.acked_at),
        );
        self.acked = token;
        self.acked_at = now;
        let limited = self.blocked > ping.blocked;
        if limited {
            if self.rates.len() == RATE_SAMPLES {
                self.rates.pop_front();
            }
            self.rates.push_back(rate);
            let fastest = self.rates.iter().copied().max().unwrap_or(rate);
            self.window = fit(fastest, min_rtt);
        } else {
            self.window = self.window.max(fit(rate, min_rtt));
        }
        if now.saturating_duration_since(self.logged_at) >= LOG_EVERY {
            self.logged_at = now;
            log::debug!(
                target: "zz_daemon::flow",
                "rtt_min_us={} rtt_us={} rate_bps={} fastest_bps={} window={} limited={}",
                min_rtt.as_micros(),
                rtt.as_micros(),
                rate,
                self.rates.iter().copied().max().unwrap_or(0),
                self.window,
                limited,
            );
        }
    }

    pub(super) fn rtt_us(&self) -> u32 {
        self.min_rtt
            .map_or(0, |rtt| u32::try_from(rtt.as_micros()).unwrap_or(u32::MAX))
    }

    #[cfg(test)]
    pub(super) fn window(&self) -> u64 {
        self.window
    }

    fn ping_gap(&self) -> u64 {
        (self.window / 4).max(MIN_PING_GAP)
    }
}

fn bytes_per_second(bytes: u64, interval: Duration) -> u64 {
    let micros = interval.as_micros().max(1);
    u64::try_from(u128::from(bytes) * 1_000_000 / micros).unwrap_or(u64::MAX)
}

fn fit(rate: u64, rtt: Duration) -> u64 {
    let bdp = u128::from(rate) * rtt.as_micros() / 1_000_000;
    u64::try_from(bdp * 5 / 4)
        .unwrap_or(u64::MAX)
        .clamp(MIN_WINDOW, MAX_WINDOW)
}
