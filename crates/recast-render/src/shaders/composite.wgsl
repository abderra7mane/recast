struct Params {
    // canvas width, height; content x, y (pixels)
    canvas_content: vec4f,
    // content width, height; corner radius; shadow blur (pixels)
    content_size: vec4f,
    // shadow offset y (pixels), shadow opacity, background kind, ripple count
    shadow: vec4f,
    // visible part of the screen: x, y, width, height (0..1)
    view: vec4f,
    bg_a: vec4f,
    bg_b: vec4f,
    // gradient direction x, y
    gradient: vec4f,
    ripple_color: vec4f,
    // screen texture width, height (texels); ripple ring thickness (pixels);
    // 1 when the screen has premultiplied alpha, 0 when it is opaque
    source: vec4f,
    // x, y, radius (pixels), opacity
    ripples: array<vec4f, 16>,
}

@group(0) @binding(0) var<uniform> u: Params;
@group(0) @binding(1) var screen_tex: texture_2d<f32>;
@group(0) @binding(2) var bg_tex: texture_2d<f32>;
@group(0) @binding(3) var linear_sampler: sampler;

@vertex
fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4f {
    let corner = vec2f(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4f(corner * 2.0 - 1.0, 0.0, 1.0);
}

fn sd_round_rect(p: vec2f, half_size: vec2f, radius: f32) -> f32 {
    let q = abs(p) - half_size + vec2f(radius);
    return length(max(q, vec2f(0.0))) + min(max(q.x, q.y), 0.0) - radius;
}

fn hash(p: vec2f) -> f32 {
    let h = dot(p, vec2f(127.1, 311.7));
    return fract(sin(h) * 43758.5453);
}

fn background(p: vec2f) -> vec3f {
    let canvas = u.canvas_content.xy;
    let kind = u.shadow.z;
    if kind < 0.5 {
        return u.bg_a.rgb;
    }
    if kind < 1.5 {
        let dir = u.gradient.xy;
        let extent = 0.5 * (abs(dir.x) * canvas.x + abs(dir.y) * canvas.y);
        let t = dot(p - canvas * 0.5, dir) / max(extent, 1e-3) * 0.5 + 0.5;
        return mix(u.bg_a.rgb, u.bg_b.rgb, clamp(t, 0.0, 1.0));
    }
    return textureSampleLevel(bg_tex, linear_sampler, p / canvas, 0.0).rgb;
}

// Catmull-Rom weights for the texels at offsets -1, 0, 1 and 2 from a sample `t` past texel 0.
fn catmull_rom(t: f32) -> vec4f {
    return vec4f(
        t * (-0.5 + t * (1.0 - 0.5 * t)),
        1.0 + t * t * (-2.5 + 1.5 * t),
        t * (0.5 + t * (2.0 - 1.5 * t)),
        t * t * (-0.5 + 0.5 * t),
    );
}

// Bicubic Catmull-Rom, kept within the four nearest texels so edges get no halos.
fn sample_bicubic(uv: vec2f) -> vec4f {
    let size = u.source.xy;
    let pos = uv * size - 0.5;
    let base = floor(pos);
    let wx = catmull_rom(pos.x - base.x);
    let wy = catmull_rom(pos.y - base.y);
    let last = vec2i(size) - 1;
    var sum = vec4f(0.0);
    var low = vec4f(1.0);
    var high = vec4f(0.0);
    for (var j = 0; j < 4; j++) {
        for (var i = 0; i < 4; i++) {
            let texel = clamp(vec2i(base) + vec2i(i - 1, j - 1), vec2i(0), last);
            let color = textureLoad(screen_tex, texel, 0);
            sum += color * wx[i] * wy[j];
            if (i == 1 || i == 2) && (j == 1 || j == 2) {
                low = min(low, color);
                high = max(high, color);
            }
        }
    }
    return clamp(sum, low, high);
}

// Enlarges the screen with a bicubic filter and box-filters it over the area one output
// pixel covers when it is smaller, so text stays legible both ways.
fn sample_screen(uv: vec2f) -> vec4f {
    let texels_per_pixel = u.view.zw * u.source.xy / u.content_size.xy;
    let most = max(texels_per_pixel.x, texels_per_pixel.y);
    // The tolerance keeps a 1:1 mapping off by float rounding at a single tap.
    let taps = clamp(ceil(most - 1e-3), 1.0, 4.0);
    var color: vec4f;
    if most < 1.0 - 1e-3 {
        color = sample_bicubic(uv);
    } else if taps <= 1.0 {
        color = textureSampleLevel(screen_tex, linear_sampler, uv, 0.0);
    } else {
        let footprint = texels_per_pixel / u.source.xy;
        let n = i32(taps);
        var sum = vec4f(0.0);
        for (var y = 0; y < n; y++) {
            for (var x = 0; x < n; x++) {
                let offset = (vec2f(f32(x), f32(y)) + 0.5) / taps - 0.5;
                sum += textureSampleLevel(screen_tex, linear_sampler, uv + offset * footprint, 0.0);
            }
        }
        color = sum / (taps * taps);
    }
    if u.source.w < 0.5 {
        color.a = 1.0;
    }
    return color;
}

@fragment
fn fs(@builtin(position) pos: vec4f) -> @location(0) vec4f {
    let p = pos.xy;
    var color = background(p);

    let content_min = u.canvas_content.zw;
    let size = u.content_size.xy;
    let radius = u.content_size.z;
    let half_size = size * 0.5;
    let center = content_min + half_size;

    let blur = max(u.content_size.w, 0.5);
    let shadow_distance = sd_round_rect(p - center - vec2f(0.0, u.shadow.x), half_size, radius);
    let shadow = u.shadow.y * (1.0 - smoothstep(-blur, blur, shadow_distance));
    color *= 1.0 - shadow;

    let distance = sd_round_rect(p - center, half_size, radius);
    let coverage = clamp(0.5 - distance, 0.0, 1.0);
    if coverage > 0.0 {
        let local = (p - content_min) / size;
        let sampled = sample_screen(u.view.xy + local * u.view.zw);
        var screen = sampled.rgb;
        let half_ring = u.source.z * 0.5;
        for (var i = 0u; i < u32(u.shadow.w); i++) {
            let ripple = u.ripples[i];
            let d = length(p - ripple.xy);
            let ring = 1.0 - smoothstep(half_ring - 0.75, half_ring + 0.75, abs(d - ripple.z));
            let fill = 0.2 * (1.0 - smoothstep(ripple.z - 1.0, ripple.z + 1.0, d));
            let alpha = clamp(ring + fill, 0.0, 1.0) * ripple.w * u.ripple_color.a;
            screen = mix(screen, u.ripple_color.rgb, alpha);
        }
        color = color * (1.0 - sampled.a * coverage) + screen * coverage;
    }

    let dither = (hash(p) - 0.5) / 255.0;
    return vec4f(color + vec3f(dither), 1.0);
}
