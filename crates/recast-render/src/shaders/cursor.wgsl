struct Params {
    // x, y, width, height (pixels)
    rect: vec4f,
    // canvas width, height; opacity
    canvas: vec4f,
}

@group(0) @binding(0) var<uniform> u: Params;
@group(0) @binding(1) var cursor_tex: texture_2d<f32>;
@group(0) @binding(2) var linear_sampler: sampler;

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
    return textureSample(cursor_tex, linear_sampler, in.uv) * u.canvas.z;
}
