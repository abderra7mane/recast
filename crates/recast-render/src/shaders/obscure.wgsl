// Blur and pixelate: averages the screen in cells, then smooths the cells of blurred
// areas. Each area's cells sit in one rectangle of a shared atlas; `composite.wgsl`
// reads them back.

struct Area {
    // the area in screen texels: x, y, width, height
    rect: vec4f,
    // its cells in the atlas: x, y, columns, rows
    grid: vec4f,
    // cell size in texels; 1 for pixelate, 0 for blur
    info: vec4f,
}

struct Areas {
    // count
    count: vec4f,
    items: array<Area, 64>,
}

@group(0) @binding(0) var<uniform> areas: Areas;
@group(0) @binding(1) var source: texture_2d<f32>;
@group(0) @binding(2) var linear_sampler: sampler;

struct VertexOut {
    @builtin(position) pos: vec4f,
    @location(0) @interpolate(flat) item: u32,
}

@vertex
fn vs(@builtin(vertex_index) i: u32, @builtin(instance_index) item: u32) -> VertexOut {
    let corner = vec2f(f32((i << 1u) & 2u), f32(i & 2u));
    var out: VertexOut;
    out.pos = vec4f(corner * 2.0 - 1.0, 0.0, 1.0);
    out.item = item;
    return out;
}

// The average of the screen texels in one cell; cells on the far edges may be smaller.
// A bilinear sample where four texels meet averages all four in one read.
@fragment
fn fs_cells(in: VertexOut) -> @location(0) vec4f {
    let area = areas.items[in.item];
    let cell = vec2i(floor(in.pos.xy)) - vec2i(area.grid.xy);
    let block = i32(area.info.x);
    let start = vec2i(area.rect.xy) + cell * block;
    let end = min(start + vec2i(block), vec2i(area.rect.xy + area.rect.zw));
    let size = vec2f(textureDimensions(source));
    var sum = vec4f(0.0);
    for (var y = start.y; y < end.y; y += 2) {
        for (var x = start.x; x < end.x; x += 2) {
            if x + 1 < end.x && y + 1 < end.y {
                let corner = vec2f(f32(x + 1), f32(y + 1)) / size;
                sum += textureSampleLevel(source, linear_sampler, corner, 0.0) * 4.0;
            } else {
                for (var j = y; j < min(y + 2, end.y); j++) {
                    for (var i = x; i < min(x + 2, end.x); i++) {
                        sum += textureLoad(source, vec2i(i, j), 0);
                    }
                }
            }
        }
    }
    let count = max((end.x - start.x) * (end.y - start.y), 1);
    return sum / f32(count);
}

fn weight(offset: i32) -> f32 {
    return select(0.25, 0.5, offset == 0);
}

// Each cell averaged with its neighbours, weighted 1-2-1 on both axes, twice, like the
// screenshot editor's blur.
@fragment
fn fs_smooth(in: VertexOut) -> @location(0) vec4f {
    let area = areas.items[in.item];
    let origin = vec2i(area.grid.xy);
    let last = vec2i(area.grid.zw) - 1;
    let cell = vec2i(floor(in.pos.xy)) - origin;
    var sum = vec4f(0.0);
    for (var b = -1; b <= 1; b++) {
        for (var a = -1; a <= 1; a++) {
            let near = clamp(cell + vec2i(a, b), vec2i(0), last);
            for (var j = -1; j <= 1; j++) {
                for (var i = -1; i <= 1; i++) {
                    let texel = origin + clamp(near + vec2i(i, j), vec2i(0), last);
                    let w = weight(a) * weight(b) * weight(i) * weight(j);
                    sum += textureLoad(source, texel, 0) * w;
                }
            }
        }
    }
    return sum;
}
