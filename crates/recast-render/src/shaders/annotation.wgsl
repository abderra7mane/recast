// An annotation image. Blurred and pixelated areas on later tracks cover it: inside them
// the frame shows the blurred recording, as if the image were under the blur.

struct Params {
    // x, y, width, height (pixels)
    rect: vec4f,
    // canvas width, height; opacity; the first area drawn over this image
    canvas: vec4f,
    // where the screen is in the frame: x, y, width, height (pixels)
    content: vec4f,
    // visible part of the screen: x, y, width, height (0..1)
    view: vec4f,
    // screen texture width, height (texels)
    source: vec4f,
}

// As in `obscure.wgsl`.
struct Area {
    rect: vec4f,
    grid: vec4f,
    info: vec4f,
}

struct Areas {
    count: vec4f,
    items: array<Area, 64>,
}

@group(0) @binding(0) var<uniform> u: Params;
@group(0) @binding(1) var image_tex: texture_2d<f32>;
@group(0) @binding(2) var linear_sampler: sampler;
@group(0) @binding(3) var<uniform> areas: Areas;

struct VertexOut {
    @builtin(position) pos: vec4f,
    @location(0) uv: vec2f,
}

@vertex
fn vs(@builtin(vertex_index) i: u32) -> VertexOut {
    let corner = vec2f(f32(i & 1u), f32((i >> 1u) & 1u));
    let p = u.rect.xy + corner * u.rect.zw;
    var out: VertexOut;
    out.pos = vec4f(p.x / u.canvas.x * 2.0 - 1.0, 1.0 - p.y / u.canvas.y * 2.0, 0.0, 1.0);
    out.uv = corner;
    return out;
}

@fragment
fn fs(in: VertexOut) -> @location(0) vec4f {
    let color = textureSample(image_tex, linear_sampler, in.uv) * u.canvas.z;
    let local = (in.pos.xy - u.content.xy) / u.content.zw;
    let texel = (u.view.xy + local * u.view.zw) * u.source.xy;
    for (var i = u32(u.canvas.w); i < u32(areas.count.x); i++) {
        let rel = texel - areas.items[i].rect.xy;
        if all(rel >= vec2f(0.0)) && all(rel < areas.items[i].rect.zw) {
            return vec4f(0.0);
        }
    }
    return color;
}
