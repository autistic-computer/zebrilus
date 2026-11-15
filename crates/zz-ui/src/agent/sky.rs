use std::{cell::RefCell, fmt::Write as _, rc::Rc, sync::LazyLock};

use ala::{
    AnyElement, App, Bounds, CustomShader, Div, Element, ElementId, GlobalElementId, Hsla,
    InspectorElementId, IntoElement, LayoutId, Pixels, Rgba, Window, div, prelude::*,
};
use penna::{ActiveTheme as _, foundation::pulse::pulse_phase};
use web_time::{Duration, Instant};

const CELL: f32 = 4.0;
const PAD: f32 = 10.0;
const FADE: f32 = 72.0;
const REVEAL: f32 = 0.8;
const STILL: f32 = 40.0;
const WRAP_GAP: f32 = 20.0;
const WIDE: f32 = 700.0;
const HEADER: usize = 7;
const MAX_ZONES: usize = 8;
const PULSE: Duration = Duration::from_secs(1);
const FLAME: [f32; 3] = [242.0 / 255.0, 88.0 / 255.0, 32.0 / 255.0];
const DARK_RAMP: [f32; 4] = [0.0, 0.12, 0.24, 0.4];
const LIGHT_RAMP: [f32; 4] = [0.4, 0.24, 0.12, 0.0];

const UP: [&str; 7] = [
    "..W........",
    "..WW.####..",
    "...WW##E#BB",
    "#..WWW####.",
    ".#########.",
    "..#######..",
    "...........",
];
const DOWN: [&str; 7] = [
    "...........",
    ".....####..",
    "....###E#BB",
    "#..#######.",
    ".##WWW####.",
    "..#WWW###..",
    "...WW......",
];
const SPRITE_WIDTH: f32 = 11.0;
const SPRITE_HEIGHT: f32 = 7.0;

struct Bird {
    lane: f32,
    speed: f32,
    delay: f32,
    scale: f32,
}

const FLOCK: [Bird; 5] = [
    Bird {
        lane: 0.2,
        speed: 7.0,
        delay: 0.0,
        scale: 1.0,
    },
    Bird {
        lane: 0.75,
        speed: 5.5,
        delay: 2.5,
        scale: 1.0,
    },
    Bird {
        lane: 0.45,
        speed: 8.5,
        delay: 5.0,
        scale: 1.0,
    },
    Bird {
        lane: 0.1,
        speed: 12.0,
        delay: 1.2,
        scale: 2.0,
    },
    Bird {
        lane: 0.6,
        speed: 10.0,
        delay: 7.5,
        scale: 2.0,
    },
];

const SHADER_BODY: &str = r"
struct Sky {
    frame: vec4<f32>,
    view: vec4<f32>,
    accent: vec4<f32>,
    ink: vec4<f32>,
    flame: vec4<f32>,
    ramp: vec4<f32>,
    counts: vec4<f32>,
    zones: array<vec4<f32>, 8>,
    birds: array<vec4<f32>, 5>,
}

@group(0) @binding(1) var<uniform> sky: Sky;

fn hash(px: f32, py: f32) -> f32 {
    var x = bitcast<u32>(i32(floor(px)) + 65536);
    var y = bitcast<u32>(i32(floor(py)) + 65536);
    x = x * 1664525u + 1013904223u;
    y = y * 1664525u + 1013904223u;
    x = x + y * 1664525u;
    y = y + x * 1664525u;
    x = x ^ (x >> 16u);
    y = y ^ (y >> 16u);
    x = x + y * 1664525u;
    x = x ^ (x >> 16u);
    return f32(x >> 8u) / 16777215.0;
}

fn noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = p - i;
    let u = f * f * (3.0 - 2.0 * f);
    let a = hash(i.x, i.y);
    let b = hash(i.x + 1.0, i.y);
    let c = hash(i.x, i.y + 1.0);
    let d = hash(i.x + 1.0, i.y + 1.0);
    let top = a + (b - a) * u.x;
    let bottom = c + (d - c) * u.x;
    return top + (bottom - top) * u.y;
}

fn fbm(start: vec2<f32>) -> f32 {
    var p = start;
    var sum = 0.0;
    var amp = 0.5;
    for (var i = 0; i < 4; i++) {
        sum += amp * noise(p);
        p = p * 2.03 + vec2<f32>(13.7, 7.3);
        amp *= 0.5;
    }
    return sum / 0.9375;
}

fn bayer(x: u32, y: u32) -> f32 {
    var rank = 0u;
    for (var i = 0u; i < 3u; i++) {
        rank = rank * 4u + (((((x ^ y) >> i) & 1u) << 1u) | ((y >> i) & 1u));
    }
    return (f32(rank) + 0.5) / 64.0;
}

@fragment
fn sky_fragment(@location(0) position: vec2<f32>) -> @location(0) vec4<f32> {
    let local = position + sky.view.xy;
    let cell = floor(local / sky.frame.x);
    let threshold = bayer(u32(cell.x), u32(cell.y));
    if threshold >= sky.view.z {
        return vec4<f32>(0.0);
    }

    var bird = vec4<f32>(0.0);
    for (var i = 0; i < i32(sky.counts.y); i++) {
        let b = sky.birds[i];
        let s = floor((cell - b.xy) / b.z);
        if s.x >= 0.0 && s.y >= 0.0 && s.x < 11.0 && s.y < 7.0 {
            let bits = sprite(u32(b.w) * 7u + u32(s.y));
            let column = u32(s.x);
            if ((bits >> (16u + column)) & 1u) == 1u {
                bird = vec4<f32>(sky.flame.rgb, 1.0);
            } else if ((bits >> column) & 1u) == 1u {
                bird = vec4<f32>(sky.ink.rgb, 1.0);
            }
        }
    }
    if bird.a > 0.0 {
        return bird;
    }

    let center = cell + 0.5;
    var near = 1.0e9;
    for (var i = 0; i < i32(sky.counts.x); i++) {
        let z = sky.zones[i];
        let d = max(max(z.xy - center, center - z.zw), vec2<f32>(0.0));
        near = min(near, length(d));
    }
    let open = smoothstep(0.0, 1.0, near / sky.counts.z);
    if open <= 0.0 {
        return vec4<f32>(0.0);
    }

    let t = sky.frame.w;
    let p = center / 160.0;
    let bias = (0.45 * (center.x / sky.frame.y) + 0.5 * (1.0 - center.y / sky.frame.z) - 0.55) * 0.4;
    let density = fbm(vec2<f32>(p.x * 2.8 - t * 0.018, p.y * 3.8)) * 0.6
        + fbm(vec2<f32>(p.x * 5.5 - t * 0.07, p.y * 7.0 + 4.7)) * 0.4
        + bias;
    let sprinkle = smoothstep(0.35, 0.9, density) * open;
    if sprinkle <= 0.0 {
        return vec4<f32>(0.0);
    }
    var tone = 0.48 * pow(sprinkle, 0.75);
    if sky.view.w < 0.5 {
        tone = 1.0 - 0.72 * sprinkle;
    }
    let level = clamp(tone, 0.0, 1.0) * 3.0;
    let below = min(floor(level + 1.0e-4), 3.0);
    var step = below;
    if level - below > threshold && below < 3.0 {
        step = below + 1.0;
    }
    let alpha = sky.ramp[u32(step)];
    return vec4<f32>(sky.accent.rgb * alpha, alpha);
}
";

static SHADER: LazyLock<CustomShader> = LazyLock::new(|| CustomShader::new(wgsl()));

fn wgsl() -> String {
    let mut source = String::from(SHADER_BODY);
    source.push_str("\nfn sprite(index: u32) -> u32 {\n    switch index {\n");
    for (frame, rows) in [UP, DOWN].iter().enumerate() {
        for (row, line) in rows.iter().enumerate() {
            let bits = line.chars().enumerate().fold(0u32, |bits, (column, mark)| {
                bits | match mark {
                    '#' | 'W' => 1 << column,
                    'B' => 1 << (16 + column),
                    _ => 0,
                }
            });
            let index = frame * 7 + row;
            let _ = writeln!(source, "        case {index}u: {{ return {bits}u; }}");
        }
    }
    source.push_str("        default: { return 0u; }\n    }\n}\n");
    source
}

#[derive(Clone, Default)]
pub struct SkyZones(Rc<RefCell<Vec<Bounds<Pixels>>>>);

impl SkyZones {
    pub fn mark(&self, element: Div) -> Div {
        let zones = self.0.clone();
        element.on_children_prepainted(move |bounds, _, _| zones.borrow_mut().extend(bounds))
    }
}

pub struct Sky {
    id: ElementId,
    zones: SkyZones,
    fill: AnyElement,
}

pub fn sky(id: impl Into<ElementId>, zones: &SkyZones) -> Sky {
    Sky {
        id: id.into(),
        zones: zones.clone(),
        fill: div().absolute().inset_0().into_any_element(),
    }
}

impl IntoElement for Sky {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for Sky {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        (self.fill.request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        (): &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.zones.0.borrow_mut().clear();
        self.fill.prepaint(window, cx);
    }

    fn paint(
        &mut self,
        global_id: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        (): &mut Self::RequestLayoutState,
        (): &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        if cfg!(target_os = "windows") || bounds.size.width <= Pixels::ZERO {
            return;
        }
        let Some(global_id) = global_id else {
            return;
        };
        let shown = window.with_element_state::<Instant, _>(global_id, |shown, _| {
            let shown = shown.unwrap_or_else(Instant::now);
            (shown, shown)
        });
        let clock = if cx.reduce_motion() {
            Clock::Still
        } else {
            pulse_phase(PULSE, window.current_view(), cx);
            Clock::Running(ala::animation_time(shown.elapsed()).as_secs_f32())
        };
        let theme = cx.theme();
        let scene = Scene {
            bounds,
            scale: window.scale_factor(),
            zones: &self.zones.0.borrow(),
            clock,
            dark: theme.is_dark(),
            accent: theme.accent,
            ink: theme.foreground,
        };
        let uniforms = scene.uniforms();
        window.paint_shader_layer(bounds, &SHADER, uniforms, |_| {});
    }
}

#[derive(Clone, Copy)]
enum Clock {
    Running(f32),
    Still,
}

impl Clock {
    fn time(self) -> f32 {
        match self {
            Self::Running(time) => time,
            Self::Still => STILL,
        }
    }

    fn reveal(self) -> f32 {
        match self {
            Self::Running(time) => (time / REVEAL).min(1.0),
            Self::Still => 1.0,
        }
    }
}

struct Scene<'a> {
    bounds: Bounds<Pixels>,
    scale: f32,
    zones: &'a [Bounds<Pixels>],
    clock: Clock,
    dark: bool,
    accent: Hsla,
    ink: Hsla,
}

impl Scene<'_> {
    fn uniforms(&self) -> Vec<u8> {
        let width = self.bounds.size.width.as_f32();
        let height = self.bounds.size.height.as_f32();
        let cols = (width / CELL).ceil();
        let rows = (height / CELL).ceil();
        let origin = self.bounds.origin;
        let zones = self.zone_cells(width, height);
        let birds = self.birds(cols, rows, width, &zones);

        let mut values: Vec<[f32; 4]> = vec![
            [CELL * self.scale, cols, rows, self.clock.time()],
            [
                (-origin.x.as_f32() * self.scale).max(0.0),
                (-origin.y.as_f32() * self.scale).max(0.0),
                self.clock.reveal(),
                if self.dark { 1.0 } else { 0.0 },
            ],
            rgb(self.accent),
            rgb(self.ink),
            [FLAME[0], FLAME[1], FLAME[2], 1.0],
            if self.dark { DARK_RAMP } else { LIGHT_RAMP },
            [zones.len() as f32, birds.len() as f32, FADE / CELL, 0.0],
        ];
        values.extend(zones.iter().copied());
        values.resize(HEADER + MAX_ZONES, [0.0; 4]);
        values.extend(birds.iter().copied());
        values.resize(HEADER + MAX_ZONES + FLOCK.len(), [0.0; 4]);
        values
            .iter()
            .flatten()
            .flat_map(|value| value.to_ne_bytes())
            .collect()
    }

    fn zone_cells(&self, width: f32, height: f32) -> Vec<[f32; 4]> {
        let origin = self.bounds.origin;
        let bottom = [-PAD, height - PAD, width + PAD, height + PAD];
        std::iter::once(bottom)
            .chain(self.zones.iter().map(|zone| {
                [
                    (zone.left() - origin.x).as_f32() - PAD,
                    (zone.top() - origin.y).as_f32() - PAD,
                    (zone.right() - origin.x).as_f32() + PAD,
                    (zone.bottom() - origin.y).as_f32() + PAD,
                ]
            }))
            .take(MAX_ZONES)
            .map(|zone| zone.map(|edge| edge / CELL))
            .collect()
    }

    fn birds(&self, cols: f32, rows: f32, width: f32, zones: &[[f32; 4]]) -> Vec<[f32; 4]> {
        let count = if width >= WIDE { FLOCK.len() } else { 3 };
        let bands = open_bands(zones, rows);
        let time = self.clock.time();
        let flight = match self.clock {
            Clock::Running(time) => (time - REVEAL).max(0.0),
            Clock::Still => STILL,
        };
        FLOCK
            .iter()
            .take(count)
            .enumerate()
            .map(|(index, bird)| {
                let order = index as f32;
                let span = SPRITE_WIDTH * bird.scale;
                let (x, wing) = match self.clock {
                    Clock::Still => {
                        let spread = 0.16 + (index % 3) as f32 * 0.27 + (index / 3) as f32 * 0.135;
                        (cols * spread - span / 2.0, (index & 1) as f32)
                    }
                    Clock::Running(_) => {
                        let start = if bird.scale > 1.0 {
                            -span - bird.speed * bird.delay
                        } else {
                            cols * (0.08 + order * 0.3)
                        };
                        let travel =
                            (start + span + bird.speed * flight).rem_euclid(cols + span + WRAP_GAP);
                        let wing = ((order * 0.7 + flight * 2.2).floor() as i32 & 1) as f32;
                        (travel - span, wing)
                    }
                };
                let bob = match self.clock {
                    Clock::Running(_) => flight,
                    Clock::Still => time,
                };
                let (top, bottom) = bands
                    .get(index % bands.len().max(1))
                    .copied()
                    .unwrap_or((0.0, rows));
                let low = (top.ceil() + 1.0).max(2.0);
                let high = bottom.floor() - SPRITE_HEIGHT * bird.scale - 1.0;
                let drift = (bob * 0.9 + order * 1.9).sin() * 1.2;
                let y = (low + bird.lane * (high - low) + drift)
                    .clamp(low, low.max(high))
                    .round();
                [x.floor(), y, bird.scale, wing]
            })
            .collect()
    }
}

fn open_bands(zones: &[[f32; 4]], rows: f32) -> Vec<(f32, f32)> {
    let mut spans: Vec<(f32, f32)> = zones.iter().map(|zone| (zone[1], zone[3])).collect();
    spans.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut bands = Vec::new();
    let mut edge = 0.0f32;
    for (top, bottom) in spans {
        if top > edge {
            bands.push((edge, top));
        }
        edge = edge.max(bottom);
    }
    if rows > edge {
        bands.push((edge, rows));
    }
    bands
        .into_iter()
        .filter(|(top, bottom)| {
            bottom.floor() - SPRITE_HEIGHT - 1.0 - (top.ceil() + 1.0).max(2.0) >= 2.0
        })
        .collect()
}

fn rgb(color: Hsla) -> [f32; 4] {
    let Rgba { r, g, b, .. } = color.to_rgb();
    [r, g, b, 1.0]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shader_validates_and_takes_every_uniform() {
        let source = wgsl();
        let module = naga::front::wgsl::parse_str(&source).expect("sky shader parses");
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .expect("sky shader validates");
        let mut layouter = naga::proc::Layouter::default();
        layouter.update(module.to_ctx()).expect("layout");
        let (_, uniform) = module
            .global_variables
            .iter()
            .find(|(_, global)| global.space == naga::AddressSpace::Uniform)
            .expect("uniform block");
        assert_eq!(layouter[uniform.ty].size, 320);
        assert_eq!((HEADER + MAX_ZONES + FLOCK.len()) * 16, 320);
    }
}
