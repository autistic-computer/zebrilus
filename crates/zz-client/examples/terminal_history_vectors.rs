#[path = "terminal_history_vectors/retained.rs"]
mod retained;

use std::{sync::Arc, time::Duration};

use serde_json::{Value, json};
use web_time::Instant;
use zz_client::{
    local_scroll::{
        LocalScrollEffect, LocalScrollState, LocalScrollStep, LocalScrollSync,
        daemon_follow_offset, local_scroll_needs_prefetch, pixel_scroll_position,
        scroll_fraction_offset,
    },
    scrollback::{
        DeferredBackfill, HistoryFollowUp, HistoryPacer, HistoryRing, HistoryRow,
        RetainedTerminalViewport, history_request_range, new_retained_viewport,
    },
};
use zz_terminal::{ScrollbarState, SessionStatus, TerminalMode, TerminalViewport};

fn uint(output: &mut Vec<u8>, mut value: u64) {
    while value >= 128 {
        output.push((value as u8 & 127) | 128);
        value >>= 7;
    }
    output.push(value as u8);
}

fn signed(output: &mut Vec<u8>, value: i64) {
    uint(output, ((value as u64) << 1) ^ ((value >> 63) as u64));
}

fn optional(output: &mut Vec<u8>, value: Option<u64>) {
    uint(output, u64::from(value.is_some()));
    if let Some(value) = value {
        uint(output, value);
    }
}

fn record(name: &str, kind: &str, request: Vec<u8>, expected: Vec<u8>) -> Value {
    json!({"name":name,"kind":kind,"request":Value::from(request),"expected":Value::from(expected)})
}

#[derive(Clone, Copy)]
struct Context {
    total: u32,
    offset: u32,
    length: u32,
    mode: u8,
    mouse: bool,
    history: usize,
    below: usize,
    invalidations: u64,
    columns: u16,
    rows: u16,
}

impl Context {
    fn normal() -> Self {
        Self {
            total: 1024,
            offset: 1000,
            length: 24,
            mode: 0,
            mouse: false,
            history: 512,
            below: 0,
            invalidations: 0,
            columns: 80,
            rows: 24,
        }
    }

    fn write(self, output: &mut Vec<u8>) {
        for value in [
            u64::from(self.total),
            u64::from(self.offset),
            u64::from(self.length),
            u64::from(self.mode),
            u64::from(self.mouse),
            self.history as u64,
            self.below as u64,
            self.invalidations,
            u64::from(self.columns),
            u64::from(self.rows),
        ] {
            uint(output, value);
        }
    }

    fn retained(self) -> RetainedTerminalViewport {
        let mut viewport = TerminalViewport::blank(self.columns, self.rows, SessionStatus::Running);
        viewport.scrollbar = ScrollbarState {
            total: self.total,
            offset: self.offset,
            len: self.length,
        };
        viewport.mouse_tracking = self.mouse;
        viewport.mode = match self.mode {
            0 => TerminalMode::Live,
            1 => TerminalMode::Copy {
                position: 0,
                total: self.total,
                hide_position: false,
            },
            _ => TerminalMode::View {
                position: 0,
                total: self.total,
            },
        };
        let mut counter = 1;
        let mut retained = new_retained_viewport(viewport, &mut counter);
        let dictionary = Arc::default();
        retained.history.rows = (0..self.history)
            .map(|revision| HistoryRow {
                cells: Box::from([]),
                dictionary: Arc::clone(&dictionary),
                revision: revision as u64,
            })
            .collect();
        retained.below.rows = (0..self.below)
            .map(|revision| HistoryRow {
                cells: Box::from([]),
                dictionary: Arc::clone(&dictionary),
                revision: revision as u64,
            })
            .collect();
        retained.history_invalidations = self.invalidations;
        retained
    }
}

fn put_step(output: &mut Vec<u8>, step: Option<LocalScrollStep>, last: &mut u64) {
    uint(output, u64::from(step.is_some()));
    if let Some(step) = step {
        uint(output, u64::from(step.redraw));
        uint(output, step.effects.len() as u64);
        for effect in step.effects {
            let (kind, value) = match effect {
                LocalScrollEffect::ScrollToOffset(value) => (0, u64::from(value)),
                LocalScrollEffect::Prefetch(value) => (1, u64::from(value)),
                LocalScrollEffect::Sync(value) => {
                    *last = value;
                    (2, value)
                }
            };
            uint(output, kind);
            uint(output, value);
        }
    }
}

#[derive(Clone, Copy)]
struct Action {
    kind: u8,
    value: i64,
    height: f32,
    context: Context,
    now: u64,
}

fn trace(name: &str, actions: &[Action]) -> Value {
    let origin = Instant::now();
    let mut state = LocalScrollState::new(0);
    let mut request = vec![];
    let mut expected = vec![];
    let mut last = 0_u64;
    uint(&mut request, 0);
    uint(&mut request, actions.len() as u64);
    for action in actions {
        uint(&mut request, u64::from(action.kind));
        action.context.write(&mut request);
        uint(&mut request, action.now);
        let now = origin + Duration::from_nanos(action.now);
        let retained = action.context.retained();
        match action.kind {
            0 => {
                let delta = f32::from_bits(action.value as u32);
                uint(&mut request, u64::from(delta.to_bits()));
                uint(&mut request, u64::from(action.height.to_bits()));
                put_step(
                    &mut expected,
                    state.scroll_by_pixels(delta, action.height, &retained, now),
                    &mut last,
                );
            }
            1 => {
                signed(&mut request, action.value);
                put_step(
                    &mut expected,
                    Some(state.scroll_by_rows(action.value, &retained, now)),
                    &mut last,
                );
            }
            2 => {
                uint(&mut request, action.value as u64);
                put_step(
                    &mut expected,
                    Some(state.scroll_to(action.value as u32, &retained, now)),
                    &mut last,
                );
            }
            3 => {
                let generation = if action.value == 0 {
                    last
                } else {
                    last.wrapping_sub(1)
                };
                uint(&mut request, generation);
                match state.sync(generation, &retained) {
                    None => uint(&mut expected, 0),
                    Some(LocalScrollSync::Retired) => uint(&mut expected, 1),
                    Some(LocalScrollSync::Requested(target)) => {
                        uint(&mut expected, 2);
                        uint(&mut expected, u64::from(target));
                    }
                }
            }
            4 => {
                let generation = if action.value == 0 {
                    last
                } else {
                    last.wrapping_sub(1)
                };
                uint(&mut request, generation);
                uint(&mut expected, u64::from(state.expire(generation, now)));
            }
            5 => {
                uint(&mut request, u64::from(action.value != 0));
                let observation = state.observe(&retained, action.value != 0, now);
                uint(&mut expected, u64::from(observation.changed));
                optional(&mut expected, observation.restore.map(u64::from));
            }
            6 => uint(&mut expected, u64::from(state.cancel())),
            7 => uint(&mut expected, u64::from(state.clear())),
            8 => uint(&mut expected, u64::from(state.clear_sub_row())),
            9 => optional(&mut expected, state.flush().map(u64::from)),
            10 => optional(&mut expected, state.release().map(u64::from)),
            11 => {
                uint(&mut request, u64::from(action.height.to_bits()));
                let (older, newer) = state.overscroll_room(&retained, action.height);
                uint(&mut expected, u64::from(older.to_bits()));
                uint(&mut expected, u64::from(newer.to_bits()));
            }
            _ => unreachable!(),
        }
        optional(&mut expected, state.target().map(u64::from));
        uint(&mut expected, u64::from(state.sub_row().to_bits()));
    }
    record(name, "local", request, expected)
}

fn local_vectors(vectors: &mut Vec<Value>) {
    for target in [0, 1, 100, 3_000_000, u32::MAX] {
        for sub_row in [0.0_f32, 0.25, 5.0, 19.999] {
            for delta in [-1000.0_f32, -45.0, -20.0, -0.25, 0.0, 0.25, 5.0, 20.0, 45.0] {
                for height in [0.1_f32, 17.5, 20.0] {
                    let oldest = target.saturating_sub(50);
                    let newest = target.saturating_add(50);
                    let mut request = vec![];
                    for value in [
                        target,
                        sub_row.to_bits(),
                        delta.to_bits(),
                        height.to_bits(),
                        oldest,
                        newest,
                    ] {
                        uint(&mut request, u64::from(value));
                    }
                    let result =
                        pixel_scroll_position(target, sub_row, delta, height, oldest, newest);
                    let mut expected = vec![];
                    uint(&mut expected, u64::from(result.target));
                    uint(&mut expected, u64::from(result.sub_row.to_bits()));
                    vectors.push(record("fractional-pixels", "pixel", request, expected));
                }
            }
        }
    }
    for target in [0, 1, 500, 981, 1000, u32::MAX] {
        for reference in [0, 500, 988, 1000, u32::MAX] {
            for rows in [0_u16, 1, 4, 24, u16::MAX] {
                for toward_newer in [false, true] {
                    let mut request = vec![];
                    for value in [
                        u64::from(target),
                        u64::from(reference),
                        u64::from(rows),
                        u64::from(u32::MAX),
                        u64::from(toward_newer),
                    ] {
                        uint(&mut request, value);
                    }
                    let mut expected = vec![];
                    optional(
                        &mut expected,
                        daemon_follow_offset(target, reference, rows, u32::MAX, toward_newer)
                            .map(u64::from),
                    );
                    vectors.push(record("daemon-follow", "follow", request, expected));
                }
            }
        }
    }
    for fraction in [0, 1, 65_535, u32::MAX / 2, u32::MAX - 1, u32::MAX] {
        for maximum in [0, 1, 1000, u32::MAX] {
            let mut request = vec![];
            uint(&mut request, u64::from(fraction));
            uint(&mut request, u64::from(maximum));
            let mut expected = vec![];
            uint(
                &mut expected,
                u64::from(scroll_fraction_offset(fraction, maximum)),
            );
            vectors.push(record(
                "exact-scroll-fraction",
                "fraction",
                request,
                expected,
            ));
        }
    }
    for history in [0, 1, 20, 512, u32::MAX as usize + 1] {
        for target in [0, 79, 80, 99, 100, u32::MAX] {
            let context = Context {
                history,
                offset: 100,
                length: 10,
                ..Context::normal()
            };
            let mut request = vec![];
            uint(&mut request, u64::from(target));
            context.write(&mut request);
            let mut expected = vec![];
            uint(
                &mut expected,
                u64::from(local_scroll_needs_prefetch(
                    target,
                    ScrollbarState {
                        total: context.total,
                        offset: context.offset,
                        len: context.length,
                    },
                    history,
                )),
            );
            vectors.push(record("cold-edge-prefetch", "prefetch", request, expected));
        }
    }
    for seed in 0..96_u32 {
        let mut random = seed.wrapping_add(1);
        let mut actions = vec![];
        let mut context = Context::normal();
        let mut now = 0;
        for index in 0..36 {
            random = random.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            now += [1, 119_999_999, 120_000_000, 1_999_999_999, 2_000_000_000][random as usize % 5];
            context.offset = [980, 990, 1000][random as usize % 3];
            context.mode = u8::from(random.trailing_zeros() >= 5);
            context.mouse = random.trailing_zeros() >= 6;
            context.history = if random.trailing_zeros() >= 4 { 0 } else { 512 };
            context.below = if random.trailing_zeros() >= 3 { 24 } else { 0 };
            context.invalidations += u64::from(random & 31 == 1);
            context.columns = if random & 31 == 2 { 81 } else { 80 };
            let kind = (index % 12) as u8;
            let value = match kind {
                0 => i64::from([0.25_f32, 5.0, 45.0, -45.0][random as usize % 4].to_bits()),
                1 => [-100, -5, 5, i64::MIN, i64::MAX][random as usize % 5],
                2 => [0, 480, 990, 1000][random as usize % 4],
                _ => i64::from(random & 1),
            };
            actions.push(Action {
                kind,
                value,
                height: 17.5,
                context,
                now,
            });
        }
        vectors.push(trace("state-effects-stale-generations", &actions));
    }
    let context = Context {
        total: 3_000_024,
        offset: 3_000_000,
        history: 1000,
        ..Context::normal()
    };
    let mut actions = vec![];
    for index in 0..80 {
        actions.push(Action {
            kind: 0,
            value: i64::from(0.25_f32.to_bits()),
            height: 17.5,
            context,
            now: index * 1_000_000,
        });
    }
    for (kind, value, now) in [
        (3, 0, 200_000_000),
        (9, 0, 201_000_000),
        (6, 0, 202_000_000),
        (3, 0, 203_000_000),
    ] {
        actions.push(Action {
            kind,
            value,
            height: 17.5,
            context,
            now,
        });
    }
    vectors.push(trace("deep-fractional-gesture-and-cancel", &actions));
    let base = Context {
        history: 1000,
        ..Context::normal()
    };
    let mut actions = vec![];
    for (kind, value, context, now) in [
        (0, i64::from(500_f32.to_bits()), base, 0),
        (0, i64::from(500_f32.to_bits()), base, 1),
        (0, i64::from((-1000_f32).to_bits()), base, 2),
        (
            0,
            i64::from((-1000_f32).to_bits()),
            Context {
                offset: 980,
                below: 24,
                ..base
            },
            3,
        ),
        (
            0,
            i64::from((-1000_f32).to_bits()),
            Context {
                offset: 976,
                below: 24,
                ..base
            },
            4,
        ),
        (9, 0, base, 5),
        (9, 0, base, 6),
        (3, 1, base, 7),
        (3, 0, base, 8),
        (4, 0, base, 2_000_000_003),
        (4, 0, base, 2_000_000_004),
        (2, 990, base, 3_000_000_000),
        (
            5,
            0,
            Context {
                invalidations: 1,
                ..base
            },
            3_000_000_001,
        ),
        (2, 990, base, 3_000_000_002),
        (
            5,
            0,
            Context {
                invalidations: 1,
                columns: 81,
                ..base
            },
            3_000_000_003,
        ),
        (2, 990, base, 3_000_000_004),
        (5, 1, base, 3_000_000_005),
        (2, 990, base, 3_000_000_006),
        (
            5,
            0,
            Context {
                offset: 990,
                ..base
            },
            3_000_000_007,
        ),
    ] {
        actions.push(Action {
            kind,
            value,
            height: 20.0,
            context,
            now,
        });
    }
    vectors.push(trace(
        "coalesced-request-floor-acknowledgement-restore-and-expiry",
        &actions,
    ));
}

#[derive(Clone, Copy)]
struct FetchContext {
    offset: u32,
    visible: u32,
    rows: usize,
    limit: usize,
    mutations: u64,
}

impl FetchContext {
    fn write(self, output: &mut Vec<u8>) {
        for value in [
            u64::from(self.offset),
            u64::from(self.visible),
            self.rows as u64,
            self.limit as u64,
            self.mutations,
        ] {
            uint(output, value);
        }
    }

    fn retained(self) -> RetainedTerminalViewport {
        let mut retained = Context {
            offset: self.offset,
            length: self.visible,
            history: self.rows,
            rows: 1,
            columns: 1,
            ..Context::normal()
        }
        .retained();
        let rows = std::mem::take(&mut retained.history.rows);
        retained.history = HistoryRing::with_limit(self.limit);
        retained.history.rows = rows;
        retained.history_mutations = self.mutations;
        retained
    }
}

fn put_range(output: &mut Vec<u8>, value: Option<(u32, u32)>) {
    uint(output, u64::from(value.is_some()));
    if let Some((start, count)) = value {
        uint(output, u64::from(start));
        uint(output, u64::from(count));
    }
}

fn pacer_vectors(vectors: &mut Vec<Value>) {
    for offset in [0_u32, 1, 40, 1000, u32::MAX] {
        for rows in [0, 1, 30, 512] {
            for limit in [0, 32, 10_000] {
                let context = FetchContext {
                    offset,
                    visible: 24,
                    rows,
                    limit,
                    mutations: 0,
                };
                let retained = context.retained();
                for budget in [0, 1, 512, 10_000, usize::MAX] {
                    for target in [None, Some(0), Some(offset), Some(u32::MAX)] {
                        let mut request = vec![];
                        context.write(&mut request);
                        uint(&mut request, budget as u64);
                        optional(&mut request, target.map(u64::from));
                        let mut expected = vec![];
                        put_range(
                            &mut expected,
                            history_request_range(&retained, budget, target),
                        );
                        vectors.push(record("history-fetch-range", "range", request, expected));
                    }
                }
            }
        }
    }
    let panes = [0_u64, 42, u64::MAX];
    for seed in 0..128_u32 {
        let mut random = seed + 1;
        let origin = Instant::now();
        let mut pacer = HistoryPacer::default();
        let mut request = vec![];
        let mut expected = vec![];
        let count = 48;
        uint(&mut request, count);
        let mut now = 0;
        for index in 0..count {
            random = random.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            now += [1, 2_999_999_999, 3_000_000_000][random as usize % 3];
            let kind = index % 7;
            let pane = panes[random as usize % panes.len()];
            uint(&mut request, kind);
            uint(&mut request, pane);
            match kind {
                0 => {
                    let target = if random.trailing_zeros() >= 2 {
                        None
                    } else {
                        Some([0, 500, 990, u32::MAX][random as usize % 4])
                    };
                    optional(&mut request, target.map(u64::from));
                    let budget = [0, 1, 24, 512, 10_000][random as usize % 5];
                    uint(&mut request, budget as u64);
                    let context = FetchContext {
                        offset: 1000,
                        visible: 24,
                        rows: [0, 20, 512][random as usize % 3],
                        limit: 10_000,
                        mutations: u64::from(random & 3),
                    };
                    let retained = (random & 7 != 0).then(|| context.retained());
                    uint(&mut request, u64::from(retained.is_some()));
                    if retained.is_some() {
                        context.write(&mut request);
                    }
                    uint(&mut request, now);
                    put_range(
                        &mut expected,
                        pacer.request(
                            zz_protocol::PaneId(pane),
                            target,
                            budget,
                            retained.as_ref(),
                            origin + Duration::from_nanos(now),
                        ),
                    );
                }
                1 => {
                    let mutations = (random & 3 != 0).then_some(u64::from(random & 3));
                    optional(&mut request, mutations);
                    let (apply, follow) = pacer.chunk_arrived(zz_protocol::PaneId(pane), mutations);
                    uint(&mut expected, u64::from(apply));
                    let (kind, value) = match follow {
                        HistoryFollowUp::Prefetch(target) => (0, u64::from(target)),
                        HistoryFollowUp::Defer(mutations) => (1, mutations),
                        HistoryFollowUp::Backfill => (2, 0),
                    };
                    uint(&mut expected, kind);
                    uint(&mut expected, value);
                }
                2 => {
                    let mutations = u64::from(random & 3);
                    uint(&mut request, mutations);
                    uint(
                        &mut expected,
                        u64::from(pacer.defer(zz_protocol::PaneId(pane), mutations)),
                    );
                }
                3 => {
                    let mutations = (random & 3 != 0).then_some(u64::from(random & 3));
                    optional(&mut request, mutations);
                    let result = match pacer.resume_deferred(zz_protocol::PaneId(pane), mutations) {
                        DeferredBackfill::Request => 0,
                        DeferredBackfill::Wait => 1,
                        DeferredBackfill::Done => 2,
                    };
                    uint(&mut expected, result);
                }
                4 => pacer.forget(zz_protocol::PaneId(pane)),
                5 => pacer.clear_deferred(),
                6 => pacer.clear(),
                _ => unreachable!(),
            }
            for pane in panes {
                let pending = pacer.pending(zz_protocol::PaneId(pane));
                uint(&mut expected, u64::from(pending.is_some()));
                if let Some(value) = pending {
                    optional(&mut expected, value.map(u64::from));
                }
                optional(&mut expected, pacer.deferred(zz_protocol::PaneId(pane)));
            }
        }
        vectors.push(record(
            "history-pacing-mutations-and-retry",
            "pacer",
            request,
            expected,
        ));
    }
}

fn pacing_merge_vector(vectors: &mut Vec<Value>) {
    let actions = [
        (0, None, Some(0), 0),
        (0, Some(500), Some(0), 1),
        (0, Some(750), Some(0), 2),
        (0, None, Some(0), 2_999_999_999),
        (0, Some(400), Some(0), 3_000_000_000),
        (1, None, Some(1), 3_000_000_001),
        (0, None, Some(1), 3_000_000_002),
        (1, None, Some(2), 3_000_000_003),
        (2, None, Some(2), 3_000_000_004),
        (2, None, Some(3), 3_000_000_005),
        (0, None, Some(3), 3_000_000_006),
        (0, Some(800), Some(3), 3_000_000_007),
        (1, None, Some(3), 3_000_000_008),
        (3, None, None, 3_000_000_009),
        (3, None, Some(4), 3_000_000_010),
        (3, None, Some(4), 3_000_000_011),
        (0, None, Some(4), 3_000_000_012),
        (0, None, None, 6_000_000_012),
        (1, None, Some(4), 6_000_000_013),
        (2, None, Some(5), 6_000_000_014),
        (4, None, None, 6_000_000_015),
    ];
    let origin = Instant::now();
    let mut pacer = HistoryPacer::default();
    let mut request = vec![];
    let mut expected = vec![];
    uint(&mut request, actions.len() as u64);
    let pane = zz_protocol::PaneId(0);
    for (kind, target, mutations, now) in actions {
        uint(&mut request, kind);
        uint(&mut request, pane.0);
        match kind {
            0 => {
                optional(&mut request, target.map(u64::from));
                uint(&mut request, 2000);
                uint(&mut request, u64::from(mutations.is_some()));
                let context = FetchContext {
                    offset: 1000,
                    visible: 24,
                    rows: 0,
                    limit: 10_000,
                    mutations: mutations.unwrap_or_default(),
                };
                if mutations.is_some() {
                    context.write(&mut request);
                }
                uint(&mut request, now);
                let retained = mutations.map(|_| context.retained());
                put_range(
                    &mut expected,
                    pacer.request(
                        pane,
                        target,
                        2000,
                        retained.as_ref(),
                        origin + Duration::from_nanos(now),
                    ),
                );
            }
            1 => {
                optional(&mut request, mutations);
                let (apply, follow) = pacer.chunk_arrived(pane, mutations);
                uint(&mut expected, u64::from(apply));
                let (tag, value) = match follow {
                    HistoryFollowUp::Prefetch(target) => (0, u64::from(target)),
                    HistoryFollowUp::Defer(mutations) => (1, mutations),
                    HistoryFollowUp::Backfill => (2, 0),
                };
                uint(&mut expected, tag);
                uint(&mut expected, value);
            }
            2 => {
                let mutations = mutations.expect("deferred trace supplies mutation");
                uint(&mut request, mutations);
                uint(&mut expected, u64::from(pacer.defer(pane, mutations)));
            }
            3 => {
                optional(&mut request, mutations);
                uint(
                    &mut expected,
                    match pacer.resume_deferred(pane, mutations) {
                        DeferredBackfill::Request => 0,
                        DeferredBackfill::Wait => 1,
                        DeferredBackfill::Done => 2,
                    },
                );
            }
            4 => pacer.forget(pane),
            _ => unreachable!(),
        }
        for id in [0, 42, u64::MAX] {
            let pending = pacer.pending(zz_protocol::PaneId(id));
            uint(&mut expected, u64::from(pending.is_some()));
            if let Some(value) = pending {
                optional(&mut expected, value.map(u64::from));
            }
            optional(&mut expected, pacer.deferred(zz_protocol::PaneId(id)));
        }
    }
    vectors.push(record(
        "pending-prefetch-minimum-retry-stale-completion-and-quiet-backfill",
        "pacer",
        request,
        expected,
    ));
}

fn history_wire_vectors(vectors: &mut Vec<Value>) {
    for pane in [0, u64::MAX] {
        for start in [0, 1, u32::MAX] {
            for count in [0, 1, 512, u32::MAX] {
                let mut request = vec![];
                uint(&mut request, pane);
                uint(&mut request, u64::from(start));
                uint(&mut request, u64::from(count));
                let expected = zz_protocol::encode_protocol_message(
                    &zz_protocol::ProtocolMessage::HistoryRequest {
                        pane: zz_protocol::PaneId(pane),
                        start,
                        count,
                    },
                )
                .expect("history request fixture encodes");
                vectors.push(record("history-request-frame", "wire", request, expected));
            }
        }
    }
}

fn budget_vectors(vectors: &mut Vec<Value>) {
    for value in [
        "",
        "0",
        "+0",
        "00001",
        "+00020",
        "2000",
        "10001",
        "18446744073709551615",
        "18446744073709551616",
        "-1",
        " 10",
        "10 ",
        "a",
    ] {
        let mut options = zz_protocol::MuxOptions::default();
        options.set(
            zz_protocol::MuxOptionKey::HistoryTrickle,
            value,
            zz_protocol::MuxOptionSource::RuntimeCommand,
        );
        let message = zz_protocol::ProtocolMessage::Event(zz_protocol::Event {
            sequence: u64::MAX,
            payload: zz_protocol::EventPayload::MuxOptionsChanged { options },
        });
        let request =
            zz_protocol::encode_protocol_message(&message).expect("history option frame encodes");
        let mut core = zz_client::ClientCore::new();
        core.handle_message(message);
        let mut expected = vec![];
        uint(&mut expected, core.history_trickle_budget() as u64);
        vectors.push(record(
            "history-trickle-parsing",
            "budget",
            request,
            expected,
        ));
    }
    let message = zz_protocol::ProtocolMessage::Event(zz_protocol::Event {
        sequence: 1,
        payload: zz_protocol::EventPayload::MuxOptionsChanged {
            options: zz_protocol::MuxOptions::default(),
        },
    });
    let request =
        zz_protocol::encode_protocol_message(&message).expect("empty option update encodes");
    let mut core = zz_client::ClientCore::new();
    core.handle_message(message);
    let mut expected = vec![];
    uint(&mut expected, core.history_trickle_budget() as u64);
    vectors.push(record(
        "history-trickle-missing-entry",
        "budget",
        request,
        expected,
    ));
}

fn main() {
    let mut vectors = Vec::new();
    local_vectors(&mut vectors);
    pacer_vectors(&mut vectors);
    pacing_merge_vector(&mut vectors);
    history_wire_vectors(&mut vectors);
    retained::vectors(&mut vectors);
    budget_vectors(&mut vectors);
    println!(
        "{}",
        json!({"reference":"zz_client::scrollback + local_scroll", "vectors":vectors})
    );
}
