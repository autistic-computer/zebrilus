struct Resolve {
    left: f32,
    width: f32,
    count: f32,
    elapsed: f32,
    stagger: f32,
    span: f32,
    blur: f32,
    pad: f32,
}

@group(0) @binding(0) var label: texture_2d<f32>;
@group(0) @binding(1) var<uniform> resolve: Resolve;
@group(0) @binding(2) var taps_sampler: sampler;

const TAPS: u32 = 16u;
const GOLDEN: f32 = 2.39996323;

fn ease(t: f32) -> f32 {
    let rest = 1.0 - t;
    return 1.0 - rest * rest * rest * rest * rest;
}

fn shown(x: f32) -> f32 {
    let along = clamp((x - resolve.left) / max(resolve.width, 1.0), 0.0, 1.0);
    let at = max(along * resolve.count - 0.5, 0.0);
    return ease(clamp((resolve.elapsed - at * resolve.stagger) / resolve.span, 0.0, 1.0));
}

@fragment
fn resolve_label(
    @builtin(position) position: vec4<f32>,
    @location(0) local: vec2<f32>,
) -> @location(0) vec4<f32> {
    let amount = shown(position.x);
    if amount <= 0.0 {
        return vec4<f32>(0.0);
    }
    let size = vec2<f32>(textureDimensions(label));
    let uv = local / size;
    let sigma = resolve.blur * (1.0 - amount);
    var color = textureSampleLevel(label, taps_sampler, uv, 0.0);
    if sigma > 0.05 {
        var total = 1.0;
        for (var tap = 0u; tap < TAPS; tap++) {
            let reach = sqrt((f32(tap) + 0.5) / f32(TAPS)) * 2.0 * sigma;
            let turn = f32(tap) * GOLDEN;
            let offset = vec2<f32>(cos(turn), sin(turn)) * reach;
            let weight = exp(-0.5 * reach * reach / (sigma * sigma));
            color += textureSampleLevel(label, taps_sampler, uv + offset / size, 0.0) * weight;
            total += weight;
        }
        color /= total;
    }
    return color * amount;
}
