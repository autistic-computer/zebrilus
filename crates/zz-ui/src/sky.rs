use std::{
    cell::RefCell,
    f32::consts::TAU,
    fmt::Write as _,
    hash::{DefaultHasher, Hash as _, Hasher as _},
    rc::Rc,
    sync::LazyLock,
};

use ala::{
    AnyElement, App, Bounds, CustomShader, Div, Element, ElementId, EntityId, GlobalElementId,
    Hsla, InspectorElementId, IntoElement, LayoutId, Pixels, Rgba, Task, Window, div, prelude::*,
};
use penna::ActiveTheme as _;
use web_time::{Duration, Instant};

const PAD: f32 = 14.0;
const FADE: f32 = 72.0;
const REVEAL: f32 = 0.75;
const STILL: f32 = 38.38;
const WRAP_GAP: f32 = 60.0;
const WIDE: f32 = 700.0;
const HEADER: usize = 10;
const MAX_ZONES: usize = 8;
const MAX_BIRDS: usize = 4;
const MAX_FAR_BIRDS: usize = 4;
const STEP: Duration = Duration::from_micros(41_667);
const MAX_GAP: Duration = Duration::from_millis(250);
const FLAME: [f32; 3] = [242.0 / 255.0, 88.0 / 255.0, 32.0 / 255.0];
const MOON_GOLD: [f32; 3] = [246.0 / 255.0, 195.0 / 255.0, 48.0 / 255.0];
const DARK_RAMP: [f32; 4] = [0.0, 0.14, 0.28, 0.46];
const LIGHT_RAMP: [f32; 4] = [0.0, 0.12, 0.26, 0.42];

const BIRD_WIDTH: f32 = 12.0;
const BIRD_HEIGHT: f32 = 8.0;
const FAR_WIDTH: f32 = 7.0;
const FAR_HEIGHT: f32 = 3.0;

const BIRD_FRAMES: [[&str; 8]; 4] = [
    [
        "..WW........",
        "...WW.......",
        "....WW.###..",
        "#...WW##E#BB",
        ".#####W###..",
        "..########..",
        "...######...",
        "............",
    ],
    [
        "............",
        "............",
        "..WWW..###..",
        "#..WWWW#E#BB",
        ".#########..",
        "..########..",
        "...######...",
        "............",
    ],
    [
        "............",
        "............",
        ".......###..",
        "#....###E#BB",
        ".####WW###..",
        "..###WWW##..",
        "....WWWWW...",
        ".....WW.....",
    ],
    [
        "............",
        "............",
        ".....W.###..",
        "#...WW##E#BB",
        ".###WW####..",
        "..########..",
        "...######...",
        "............",
    ],
];

const FAR_FRAMES: [[&str; 3]; 3] = [
    ["#.....#", ".##.##.", "...#..."],
    [".......", "##.#.##", "..###.."],
    ["...#...", ".#####.", "#.....#"],
];

struct BirdSpec {
    lane: f32,
    scale: f32,
    speed: f32,
    delay: f32,
}

struct FarSpec {
    lane: f32,
    scale: f32,
    speed: f32,
    phase: f32,
}

const FLOCK: [BirdSpec; MAX_BIRDS] = [
    BirdSpec {
        lane: 0.24,
        scale: 2.5,
        speed: 30.0,
        delay: 0.0,
    },
    BirdSpec {
        lane: 0.68,
        scale: 2.5,
        speed: 23.0,
        delay: 2.3,
    },
    BirdSpec {
        lane: 0.44,
        scale: 3.5,
        speed: 38.0,
        delay: 4.5,
    },
    BirdSpec {
        lane: 0.16,
        scale: 2.5,
        speed: 27.0,
        delay: 6.8,
    },
];

const FAR_FLOCK: [FarSpec; MAX_FAR_BIRDS] = [
    FarSpec {
        lane: 0.18,
        scale: 2.0,
        speed: 15.0,
        phase: 0.2,
    },
    FarSpec {
        lane: 0.30,
        scale: 2.0,
        speed: 17.0,
        phase: 1.8,
    },
    FarSpec {
        lane: 0.48,
        scale: 2.0,
        speed: 14.0,
        phase: 3.4,
    },
    FarSpec {
        lane: 0.25,
        scale: 1.5,
        speed: 12.0,
        phase: 4.7,
    },
];

const SHADER_BODY: &str = r"
struct Sky {
    frame: vec4<f32>,
    view: vec4<f32>,
    parallax: vec4<f32>,
    accent: vec4<f32>,
    ink: vec4<f32>,
    bg: vec4<f32>,
    flame: vec4<f32>,
    ramp: vec4<f32>,
    counts: vec4<f32>,
    seed: vec4<f32>,
    zones: array<vec4<f32>, 8>,
    birds: array<vec4<f32>, 4>,
    far_birds: array<vec4<f32>, 4>,
}

@group(0) @binding(1) var<uniform> sky: Sky;

fn hash21(px: f32, py: f32) -> f32 {
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
    let a = hash21(i.x, i.y);
    let b = hash21(i.x + 1.0, i.y);
    let c = hash21(i.x, i.y + 1.0);
    let d = hash21(i.x + 1.0, i.y + 1.0);
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

fn fbm_rot(start: vec2<f32>) -> f32 {
    var p = start;
    var sum = 0.0;
    var amp = 0.5;
    let rot = mat2x2<f32>(vec2<f32>(0.8, 0.6), vec2<f32>(-0.6, 0.8));
    for (var i = 0; i < 4; i++) {
        sum += amp * noise(p);
        p = (rot * p) * 2.02 + vec2<f32>(17.1, 31.7);
        amp *= 0.5;
    }
    return sum / 0.9375;
}

fn fbm2(p: vec2<f32>) -> f32 {
    let rot = mat2x2<f32>(vec2<f32>(0.8, 0.6), vec2<f32>(-0.6, 0.8));
    return (0.5 * noise(p) + 0.25 * noise((rot * p) * 2.02 + vec2<f32>(17.1, 31.7))) / 0.75;
}

fn bayer8(x: u32, y: u32) -> f32 {
    var rank = 0u;
    for (var i = 0u; i < 3u; i++) {
        rank = rank * 4u + (((((x ^ y) >> i) & 1u) << 1u) | ((y >> i) & 1u));
    }
    return (f32(rank) + 0.5) / 64.0;
}

fn bayer4(x: u32, y: u32) -> f32 {
    var rank = 0u;
    for (var i = 0u; i < 2u; i++) {
        rank = rank * 4u + (((((x ^ y) >> i) & 1u) << 1u) | ((y >> i) & 1u));
    }
    return (f32(rank) + 0.5) / 16.0;
}

fn smin(a: f32, b: f32, k: f32) -> f32 {
    let h = clamp(0.5 + 0.5 * (b - a) / k, 0.0, 1.0);
    return mix(b, a, h) - k * h * (1.0 - h);
}

fn smooth_clearance(p: vec2<f32>) -> f32 {
    var nearest = 1.0e3;
    let count = i32(sky.counts.x);
    let fade = sky.counts.w;
    for (var i = 0; i < 8; i++) {
        if i >= count {
            break;
        }
        let z = sky.zones[i];
        let away = max(max(z.xy - p, p - z.zw), vec2<f32>(0.0)) / fade;
        nearest = smin(nearest, length(away), 0.28);
    }
    return max(0.0, nearest);
}

fn sample_moon(p: vec2<f32>, center: vec2<f32>, r: f32, quant_px: f32, moon_rgb: vec3<f32>) -> vec4<f32> {
    let mp = floor(p / quant_px) * quant_px + quant_px * 0.5;
    let d1 = length(mp - center) / r;
    let d2 = length(mp - center - vec2<f32>(0.46 * r, -0.24 * r)) / (0.84 * r);
    if d1 >= 1.0 {
        return vec4<f32>(0.0);
    }
    let lit = (1.0 - smoothstep(0.92, 1.0, d1)) * smoothstep(0.88, 1.05, d2);
    let crater = noise(mp * 0.22);
    let col = mix(moon_rgb * 0.88, min(moon_rgb * 1.06, vec3<f32>(1.0)), crater);
    if lit > 0.15 {
        return vec4<f32>(col, lit);
    }
    let earthshine = smoothstep(0.78, 0.98, d1) * 0.14;
    return vec4<f32>(mix(sky.bg.rgb, moon_rgb, 0.35), earthshine);
}

fn star_field(p: vec2<f32>, open: f32, cloud_mask: f32) -> f32 {
    if sky.view.w < 0.5 || open < 0.35 {
        return 0.0;
    }
    let cell_size = 22.0;
    let sc = floor(p / cell_size);
    let rnd = hash21(sc.x + 31.0, sc.y + 17.0);
    let uv = p / max(sky.frame.yz, vec2<f32>(1.0));
    let sky_prob = 0.32 * (1.0 - uv.y * 0.62);
    if rnd > sky_prob {
        return 0.0;
    }
    let offset = vec2<f32>(
        hash21(sc.x + 97.0, sc.y + 11.0),
        hash21(sc.x + 53.0, sc.y + 79.0),
    ) * (cell_size - 8.0) + 4.0;
    let center = sc * cell_size + offset;
    let d = abs(p - center);
    let tier = hash21(sc.x + 211.0, sc.y + 43.0);
    let twinkle = 0.72 + 0.28 * sin(sky.frame.w * (0.9 + tier * 1.6) + rnd * 47.0);
    var val = 0.0;
    if tier > 0.86 && uv.y < 0.68 {
        let arm = 2.0 + 2.2 * sin(sky.frame.w * 1.2 + rnd * 29.0);
        if (d.x < 0.65 && d.y < arm) || (d.y < 0.65 && d.x < arm) {
            val = 0.92 * twinkle;
        }
    } else {
        let rad = select(0.8, 1.1, tier > 0.5);
        if max(d.x, d.y) < rad {
            val = select(0.45, 0.82, tier > 0.5) * twinkle;
        }
    }
    return val * smoothstep(0.35, 0.75, open) * (1.0 - smoothstep(0.2, 0.65, cloud_mask));
}

fn shooting_star(p: vec2<f32>, open: f32) -> f32 {
    if sky.view.w < 0.5 || open < 0.55 {
        return 0.0;
    }
    let period = 9.5;
    let slot = floor(sky.frame.w / period);
    let phase = fract(sky.frame.w / period);
    if phase > 0.085 {
        return 0.0;
    }
    let prog = phase / 0.085;
    let rx = 0.45 + 0.42 * hash21(slot, 13.0);
    let ry = 0.08 + 0.18 * hash21(slot, 47.0);
    let start = sky.frame.yz * vec2<f32>(rx, ry);
    let dir = normalize(vec2<f32>(-1.0, 0.42));
    let head = start + dir * prog * 240.0;
    let rel = p - head;
    let along = dot(rel, -dir);
    let perp = abs(dot(rel, vec2<f32>(-dir.y, dir.x)));
    if along < 0.0 || along > 58.0 || perp > 1.1 {
        return 0.0;
    }
    let tail = exp(-along * 0.065) * (1.0 - smoothstep(0.0, 1.1, perp));
    return tail * sin(prog * 3.14159) * smoothstep(0.55, 0.85, open);
}

fn blend_over(dst: vec4<f32>, src_rgb: vec3<f32>, src_a: f32) -> vec4<f32> {
    let a = clamp(src_a, 0.0, 1.0);
    return vec4<f32>(src_rgb * a + dst.rgb * (1.0 - a), a + dst.a * (1.0 - a));
}

@fragment
fn sky_fragment(@location(0) position: vec2<f32>) -> @location(0) vec4<f32> {
    let css = (position + sky.view.xy) / sky.frame.x;
    let cell_px = 2.0;
    let cell = floor(max(css / cell_px, vec2<f32>(0.0)));
    let revealed = bayer8(u32(cell.x), u32(cell.y)) < sky.view.z;

    let cc = (cell + 0.5) * cell_px;
    let cuv = cc / max(sky.frame.yz, vec2<f32>(1.0));
    let d = smooth_clearance(cc);
    let n_open = fbm2(cc / 120.0);
    let open = select(0.0, smoothstep(0.08, 1.0, d + (n_open - 0.5) * 0.14), revealed);

    let p_vec = sky.parallax.xy;
    let moon_pos = sky.parallax.zw - p_vec * vec2<f32>(8.0, 5.0);
    let p_back = (cc + sky.seed.xy + p_vec * vec2<f32>(15.0, 9.0)) / vec2<f32>(300.0, 180.0);
    let p_front = (cc + sky.seed.xy + p_vec * vec2<f32>(30.0, 17.0)) / vec2<f32>(230.0, 145.0);
    let t = sky.frame.w;

    let b1 = fbm_rot(p_back * vec2<f32>(1.0, 2.0) - vec2<f32>(t * 0.009, 0.0));
    let back_ribbon = pow(max(0.0, 1.0 - abs(b1 * 2.0 - 1.0)), 1.55);
    let back_arch = pow(max(0.0, 1.0 - (cuv.y + p_vec.y * 0.015) * 1.32), 1.4)
        * (0.32 + 0.68 * smoothstep(0.05, 0.85, cuv.x + p_vec.x * 0.015));

    let f1 = fbm_rot(p_front * vec2<f32>(1.1, 2.3) - vec2<f32>(t * 0.015, 0.0));
    let f2 = fbm_rot(p_front * vec2<f32>(1.95, 3.7) + vec2<f32>(f1 * 0.88 - t * 0.024, 1.3));
    let front_ribbon = pow(max(0.0, 1.0 - abs(f2 * 2.0 - 1.0)), 1.85);
    let front_mask = smoothstep(0.0, 0.72, 1.0 - cuv.y * 0.85);

    let ray = cc - moon_pos;
    let ray_proj = dot(normalize(vec2<f32>(-0.65, 0.76)), ray);
    let ray_perp = abs(dot(normalize(vec2<f32>(-0.76, -0.65)), ray));
    let shaft = smoothstep(0.0, 260.0, ray_proj) * exp(-ray_proj * 0.004) * exp(-ray_perp * 0.016) * 0.28;

    let tone = clamp((back_arch * 0.48 + back_ribbon * 0.24 * front_mask + front_ribbon * 0.50 * front_mask + shaft) * open, 0.0, 1.0);
    let scan = select(-0.04, 0.04, (u32(cell.y) & 1u) == 0u);
    let thresh = bayer4(u32(cell.x), u32(cell.y)) + scan;
    let level = tone * 3.0;
    let below = min(floor(level + 1.0e-4), 3.0);
    var step_idx = below;
    if level - below > thresh && below < 3.0 {
        step_idx = below + 1.0;
    }

    var out_col = vec4<f32>(0.0);
    let alpha = sky.ramp[u32(step_idx)];
    let sun_hi = vec3<f32>(1.0, 0.98, 0.95);
    let moon_rgb = vec3<f32>(0.96, 0.80, 0.34);

    if revealed && alpha > 0.0 {
        var rgb = sky.accent.rgb;
        if sky.view.w > 0.5 {
            if step_idx > 2.5 {
                rgb = mix(sky.accent.rgb, sky.ink.rgb, 0.24);
            }
        } else {
            if step_idx > 2.5 {
                rgb = mix(sky.accent.rgb, sun_hi, 0.16);
            }
            let sun_veil = exp(-length(cc - moon_pos) * 0.016) * open;
            if sun_veil > thresh * 0.8 {
                rgb = mix(rgb, sun_hi, 0.35);
            }
        }
        out_col = vec4<f32>(rgb * alpha, alpha);
    }

    if revealed && sky.view.w > 0.5 && sky.seed.z > 0.5 {
        let halo = exp(-length(cc - moon_pos) * 0.022) * open;
        if halo * 0.5 > thresh * 0.65 {
            out_col = blend_over(out_col, moon_rgb, 0.22);
        }
        let star_css = css + p_vec * vec2<f32>(4.0, 2.5);
        let st = star_field(star_css, open, tone * 0.75);
        if st > 0.0 && cuv.y < 0.72 {
            out_col = blend_over(out_col, sky.ink.rgb, st);
        }
        let shoot = shooting_star(star_css, open);
        if shoot > 0.0 {
            out_col = blend_over(out_col, vec3<f32>(1.0, 0.96, 0.88), shoot);
        }
        if open > 0.4 {
            let m = sample_moon(css, moon_pos, 15.5, 2.0, moon_rgb);
            if m.a > 0.0 {
                out_col = blend_over(out_col, m.rgb, m.a);
            }
        }
    }

    let far_tint = mix(sky.accent.rgb, sky.ink.rgb, 0.45);
    let far_count = i32(sky.counts.z);
    for (var i = 0; i < 4; i++) {
        if i >= far_count {
            break;
        }
        let fb = sky.far_birds[i];
        let fp = floor((css - fb.xy) / fb.z);
        if fp.x >= 0.0 && fp.y >= 0.0 && fp.x < 7.0 && fp.y < 3.0 {
            let mask = far_sprite(u32(fb.w) * 3u + u32(fp.y));
            if ((mask >> u32(fp.x)) & 1u) == 1u {
                out_col = blend_over(out_col, far_tint, 0.68);
            }
        }
    }

    let wing_tone = mix(sky.ink.rgb, sky.accent.rgb, select(0.28, 0.36, sky.view.w > 0.5));
    let bird_count = i32(sky.counts.y);
    for (var i = 0; i < 4; i++) {
        if i >= bird_count {
            break;
        }
        let b = sky.birds[i];
        let sp = floor((css - b.xy) / b.z);
        if sp.x >= 0.0 && sp.y >= 0.0 && sp.x < 12.0 && sp.y < 8.0 {
            let bits = bird_sprite(u32(b.w) * 8u + u32(sp.y));
            let col = u32(sp.x);
            if ((bits.y >> col) & 1u) == 1u {
                out_col = vec4<f32>(sky.flame.rgb, 1.0);
            } else if ((bits.x >> (16u + col)) & 1u) == 1u {
                out_col = vec4<f32>(wing_tone, 1.0);
            } else if ((bits.x >> col) & 1u) == 1u {
                out_col = vec4<f32>(sky.ink.rgb, 1.0);
            } else if ((bits.y >> (16u + col)) & 1u) == 1u {
                out_col = vec4<f32>(sky.bg.rgb, 1.0);
            }
        }
    }

    return out_col;
}
";

static EPOCH: LazyLock<Instant> = LazyLock::new(Instant::now);

static SHADER: LazyLock<CustomShader> = LazyLock::new(|| CustomShader::new(wgsl()));

fn wgsl() -> String {
    let mut source = String::from(SHADER_BODY);
    source.push_str("\nfn bird_sprite(index: u32) -> vec2<u32> {\n    switch index {\n");
    for (frame, rows) in BIRD_FRAMES.iter().enumerate() {
        for (row, line) in rows.iter().enumerate() {
            let mut body_wing = 0u32;
            let mut beak_eye = 0u32;
            for (column, mark) in line.chars().enumerate() {
                match mark {
                    '#' => body_wing |= 1 << column,
                    'W' => body_wing |= 1 << (16 + column),
                    'B' => beak_eye |= 1 << column,
                    'E' => beak_eye |= 1 << (16 + column),
                    _ => {}
                }
            }
            let index = frame * 8 + row;
            let _ = writeln!(
                source,
                "        case {index}u: {{ return vec2<u32>({body_wing}u, {beak_eye}u); }}"
            );
        }
    }
    source.push_str(
        "        default: { return vec2<u32>(0u, 0u); }\n    }\n}\n\nfn far_sprite(index: u32) -> u32 {\n    switch index {\n",
    );
    for (frame, rows) in FAR_FRAMES.iter().enumerate() {
        for (row, line) in rows.iter().enumerate() {
            let bits = line.chars().enumerate().fold(0u32, |bits, (column, mark)| {
                bits | match mark {
                    '#' => 1 << column,
                    _ => 0,
                }
            });
            let index = frame * 3 + row;
            let _ = writeln!(source, "        case {index}u: {{ return {bits}u; }}");
        }
    }
    source.push_str("        default: { return 0u; }\n    }\n}\n");
    let _ = MOON_GOLD;
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
        let still = cx.reduce_motion();
        let running = !still && window.is_window_active();
        let view = window.current_view();
        let mouse = window.mouse_position();
        let (elapsed, parallax) =
            window.with_element_state::<SkyClock, _>(global_id, |clock, _| {
                let now = Instant::now();
                let mut clock = clock.unwrap_or(SkyClock {
                    elapsed: Duration::ZERO,
                    last: now,
                    parallax: [0.0, 0.0],
                    tick: None,
                });
                if running {
                    let raw_dt = (now - clock.last).min(MAX_GAP);
                    let dt = ala::animation_time(raw_dt);
                    clock.elapsed += dt;
                    let target = if bounds.contains(&mouse) {
                        let width = bounds.size.width.as_f32().max(1.0);
                        let height = bounds.size.height.as_f32().max(1.0);
                        [
                            (((mouse.x - bounds.origin.x).as_f32() / width) * 2.0 - 1.0)
                                .clamp(-1.0, 1.0),
                            (((mouse.y - bounds.origin.y).as_f32() / height) * 2.0 - 1.0)
                                .clamp(-1.0, 1.0),
                        ]
                    } else {
                        [0.0, 0.0]
                    };
                    let ease = 1.0 - (-dt.as_secs_f32() * 4.5).exp();
                    clock.parallax[0] += (target[0] - clock.parallax[0]) * ease;
                    clock.parallax[1] += (target[1] - clock.parallax[1]) * ease;
                }
                clock.last = now;
                clock.tick = running.then(|| tick(view, cx));
                let out_parallax = if still { [0.0, 0.0] } else { clock.parallax };
                ((clock.elapsed, out_parallax), clock)
            });
        let clock = if still {
            Clock::Still
        } else {
            Clock::Running(elapsed.as_secs_f32())
        };
        let mut hasher = DefaultHasher::new();
        self.id.hash(&mut hasher);
        let theme = cx.theme();
        let scene = Scene {
            bounds,
            scale: window.scale_factor(),
            zones: &self.zones.0.borrow(),
            clock,
            parallax,
            seed: hasher.finish(),
            dark: theme.is_dark(),
            accent: theme.accent,
            ink: theme.foreground,
            bg: theme.background,
        };
        let uniforms = scene.uniforms();
        window.paint_shader_layer(bounds, &SHADER, uniforms, |_| {});
    }
}

struct SkyClock {
    elapsed: Duration,
    last: Instant,
    parallax: [f32; 2],
    tick: Option<Task<()>>,
}

fn tick(view: EntityId, cx: &mut App) -> Task<()> {
    let into = EPOCH.elapsed().as_nanos() % STEP.as_nanos();
    let wait = STEP.saturating_sub(Duration::from_nanos(
        u64::try_from(into).unwrap_or_default(),
    ));
    let wait = if wait < STEP / 8 { wait + STEP } else { wait };
    cx.spawn(async move |cx| {
        cx.background_executor().timer(wait).await;
        cx.update(|cx| cx.notify(view));
    })
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
    parallax: [f32; 2],
    seed: u64,
    dark: bool,
    accent: Hsla,
    ink: Hsla,
    bg: Hsla,
}

impl Scene<'_> {
    fn uniforms(&self) -> Vec<u8> {
        let width = self.bounds.size.width.as_f32();
        let height = self.bounds.size.height.as_f32();
        let origin = self.bounds.origin;
        let zones = self.zone_css(width, height);
        let moon = moon_spot(width, height, &zones);
        let (moon_x, moon_y, moon_visible) = match moon {
            Some([x, y]) => (x, y, 1.0),
            None => (width * 0.84, (height * 0.16).max(32.0), 0.0),
        };
        let birds = self.birds(width, height, &zones);
        let far_birds = self.far_birds(width, height, &zones);

        let mut values: Vec<[f32; 4]> = vec![
            [self.scale, width, height, self.clock.time()],
            [
                (-origin.x.as_f32() * self.scale).max(0.0),
                (-origin.y.as_f32() * self.scale).max(0.0),
                self.clock.reveal(),
                if self.dark { 1.0 } else { 0.0 },
            ],
            [self.parallax[0], self.parallax[1], moon_x, moon_y],
            rgb(self.accent),
            rgb(self.ink),
            rgb(self.bg),
            [FLAME[0], FLAME[1], FLAME[2], 1.0],
            if self.dark { DARK_RAMP } else { LIGHT_RAMP },
            [
                zones.len() as f32,
                birds.len() as f32,
                far_birds.len() as f32,
                FADE,
            ],
            [
                (self.seed & 0xfff) as f32,
                ((self.seed >> 12) & 0xfff) as f32,
                moon_visible,
                0.0,
            ],
        ];
        values.extend(zones.iter().copied());
        values.resize(HEADER + MAX_ZONES, [0.0; 4]);
        values.extend(birds.iter().copied());
        values.resize(HEADER + MAX_ZONES + MAX_BIRDS, [0.0; 4]);
        values.extend(far_birds.iter().copied());
        values.resize(HEADER + MAX_ZONES + MAX_BIRDS + MAX_FAR_BIRDS, [0.0; 4]);
        values
            .iter()
            .flatten()
            .flat_map(|value| value.to_ne_bytes())
            .collect()
    }

    fn zone_css(&self, width: f32, height: f32) -> Vec<[f32; 4]> {
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
            .collect()
    }

    fn birds(&self, width: f32, height: f32, zones: &[[f32; 4]]) -> Vec<[f32; 4]> {
        let count = if width >= WIDE { FLOCK.len() } else { 3 };
        let phase = ((self.seed >> 24) & 0xffff) as f32 / 65536.0;
        let time = self.clock.time();
        let flight = match self.clock {
            Clock::Running(time) => (time - REVEAL).max(0.0),
            Clock::Still => STILL,
        };
        let scenic = [0.22, 0.62, 0.42, 0.78];
        FLOCK
            .iter()
            .take(count)
            .enumerate()
            .filter_map(|(index, bird)| {
                let bands = open_bands(zones, height, BIRD_HEIGHT * bird.scale);
                let (top, bottom) = *bands.get(index % bands.len().max(1))?;
                let order = index as f32;
                let span = BIRD_WIDTH * bird.scale;
                let (x, frame) = match self.clock {
                    Clock::Still => (
                        width * scenic[index % scenic.len()] - span / 2.0,
                        glide_frame((order * 0.24 + phase).rem_euclid(1.0)),
                    ),
                    Clock::Running(_) => {
                        let start = if bird.scale < 3.0 {
                            width * (0.12 + order * 0.32)
                        } else {
                            -span - bird.speed * bird.delay
                        };
                        let lap = width + span + WRAP_GAP;
                        let travel =
                            (start + span + phase * lap + bird.speed * flight).rem_euclid(lap);
                        let cyc = (order * 0.24 + phase + flight * 0.42).rem_euclid(1.0);
                        (travel - span, glide_frame(cyc))
                    }
                };
                let bob = match self.clock {
                    Clock::Running(_) => flight,
                    Clock::Still => time,
                };
                let low = (top + 4.0).max(8.0);
                let high = (bottom - BIRD_HEIGHT * bird.scale - 4.0).max(low);
                let drift = (bob * 0.65 + order * 1.9 + phase * TAU).sin() * 5.0;
                let y = (low + bird.lane * (high - low) + drift).clamp(low, high);
                let (dx, dy) = if bird.scale >= 3.0 {
                    (54.0, 30.0)
                } else {
                    (38.0, 22.0)
                };
                Some([
                    x - self.parallax[0] * dx,
                    y - self.parallax[1] * dy,
                    bird.scale,
                    frame as f32,
                ])
            })
            .collect()
    }

    fn far_birds(&self, width: f32, height: f32, zones: &[[f32; 4]]) -> Vec<[f32; 4]> {
        let count = if width >= WIDE { FAR_FLOCK.len() } else { 2 };
        let phase = ((self.seed >> 40) & 0xffff) as f32 / 65536.0;
        let time = self.clock.time();
        let flight = match self.clock {
            Clock::Running(time) => (time - REVEAL).max(0.0),
            Clock::Still => STILL,
        };
        let scenic = [0.34, 0.56, 0.72, 0.48];
        FAR_FLOCK
            .iter()
            .take(count)
            .enumerate()
            .filter_map(|(index, bird)| {
                let mut bands = open_bands(zones, height, FAR_HEIGHT * bird.scale);
                bands.retain(|(top, _)| *top < height * 0.48);
                if bands.is_empty() {
                    let top_limit = zones
                        .iter()
                        .filter(|z| z[1] > 8.0 && z[1] < height - PAD - 2.0)
                        .map(|z| z[1])
                        .fold(height * 0.48, f32::min);
                    if top_limit - FAR_HEIGHT * bird.scale >= 16.0 {
                        bands.push((8.0, top_limit));
                    }
                }
                let (top, bottom) = *bands.get(index % bands.len().max(1))?;
                let order = index as f32;
                let span = FAR_WIDTH * bird.scale;
                let (x, frame) = match self.clock {
                    Clock::Still => (width * scenic[index % scenic.len()] - span / 2.0, index % 3),
                    Clock::Running(_) => {
                        let start = width * (0.1 + order * 0.23);
                        let lap = width + span + WRAP_GAP;
                        let travel =
                            (start + span + phase * lap + bird.speed * flight).rem_euclid(lap);
                        let frame =
                            ((order * 0.31 + phase + flight * 0.36) * 7.0).floor() as usize % 3;
                        (travel - span, frame)
                    }
                };
                let bob = match self.clock {
                    Clock::Running(_) => flight,
                    Clock::Still => time,
                };
                let low = (top + 4.0).max(8.0);
                let high = (bottom - FAR_HEIGHT * bird.scale - 4.0).max(low);
                let drift = (bob * 0.65 + bird.phase + phase * TAU).sin() * 5.0;
                let y = (low + bird.lane * (high - low) + drift).clamp(low, high);
                Some([
                    x - self.parallax[0] * 20.0,
                    y - self.parallax[1] * 12.0,
                    bird.scale,
                    frame as f32,
                ])
            })
            .collect()
    }
}

fn glide_frame(cycle: f32) -> usize {
    const SEQ: [usize; 8] = [0, 1, 2, 3, 0, 1, 2, 1];
    if cycle < 0.42 {
        let idx = ((cycle / 0.42) * 8.0).floor() as usize;
        SEQ[idx.min(7)]
    } else {
        1
    }
}

fn open_bands(zones: &[[f32; 4]], height: f32, bird_height: f32) -> Vec<(f32, f32)> {
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
    if height > edge {
        bands.push((edge, height));
    }
    bands
        .into_iter()
        .filter(|(top, bottom)| bottom - top - bird_height >= 16.0)
        .collect()
}

fn moon_spot(width: f32, height: f32, zones: &[[f32; 4]]) -> Option<[f32; 2]> {
    let reach = [-22.0f32, 0.0, 22.0];
    let clearance = |x: f32, y: f32| {
        if zones.is_empty() {
            return 1.0;
        }
        zones
            .iter()
            .map(|[l, t, r, b]| {
                (l - x)
                    .max(x - r)
                    .max(0.0)
                    .hypot((t - y).max(y - b).max(0.0))
                    / FADE
            })
            .fold(f32::INFINITY, f32::min)
    };
    let spots = [
        [0.84, 0.16],
        [0.88, 0.12],
        [0.74, 0.14],
        [0.84, 0.10],
        [0.16, 0.16],
    ];
    spots
        .into_iter()
        .map(|[sx, sy]| {
            [
                (width * sx).clamp(32.0, (width - 32.0).max(32.0)),
                (height * sy).max(32.0),
            ]
        })
        .find(|&[x, y]| {
            reach
                .iter()
                .all(|&dx| reach.iter().all(|&dy| clearance(x + dx, y + dy) >= 0.85))
        })
}

fn rgb(color: Hsla) -> [f32; 4] {
    let Rgba { r, g, b, .. } = color.to_rgb();
    [r, g, b, 1.0]
}

#[cfg(test)]
mod tests {
    use ala::{point, px, size};

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
        assert_eq!(layouter[uniform.ty].size, 416);
        assert_eq!((HEADER + MAX_ZONES + MAX_BIRDS + MAX_FAR_BIRDS) * 16, 416);
    }

    #[test]
    fn parallax_scales_depth_by_plane() {
        let theme_color = Hsla::default();
        let zones = [Bounds::new(
            point(px(40.0), px(520.0)),
            size(px(420.0), px(220.0)),
        )];
        let make = |parallax: [f32; 2]| Scene {
            bounds: Bounds::new(point(px(0.0), px(0.0)), size(px(1200.0), px(800.0))),
            scale: 2.0,
            zones: &zones,
            clock: Clock::Still,
            parallax,
            seed: 42,
            dark: true,
            accent: theme_color,
            ink: theme_color,
            bg: theme_color,
        };
        let rest = make([0.0, 0.0]);
        let shifted = make([1.0, 0.5]);
        assert_eq!(rest.uniforms().len(), 416);

        let z_rest = rest.zone_css(1200.0, 800.0);
        let z_shift = shifted.zone_css(1200.0, 800.0);
        let b_rest = rest.birds(1200.0, 800.0, &z_rest);
        let b_shift = shifted.birds(1200.0, 800.0, &z_shift);
        let f_rest = rest.far_birds(1200.0, 800.0, &z_rest);
        let f_shift = shifted.far_birds(1200.0, 800.0, &z_shift);

        assert_eq!(b_rest.len(), 4);
        assert_eq!(f_rest.len(), 4);
        assert!((b_rest[2][0] - b_shift[2][0] - 54.0).abs() < 1e-4);
        assert!((b_rest[0][0] - b_shift[0][0] - 38.0).abs() < 1e-4);
        assert!((f_rest[0][0] - f_shift[0][0] - 20.0).abs() < 1e-4);
    }
}
