/// <reference types="@webgpu/types" />
import type { Frame } from "@/editor/frame";

export type RendererKind = "webgpu" | "webgl2";

export type Renderer = {
  kind: RendererKind;
  canvas: HTMLCanvasElement;
  draw: (frame: Frame) => void;
  destroy: () => void;
};

// BT.709 full range, matching `recast_render::nv12`.
const WGSL = /* wgsl */ `
struct Out {
  @builtin(position) position: vec4f,
  @location(0) uv: vec2f,
};

@vertex
fn vs(@builtin(vertex_index) i: u32) -> Out {
  let p = vec2f(f32((i << 1u) & 2u), f32(i & 2u)) * 2.0 - 1.0;
  var out: Out;
  out.position = vec4f(p, 0.0, 1.0);
  out.uv = vec2f((p.x + 1.0) * 0.5, (1.0 - p.y) * 0.5);
  return out;
}

@group(0) @binding(0) var smp: sampler;
@group(0) @binding(1) var yTex: texture_2d<f32>;
@group(0) @binding(2) var uvTex: texture_2d<f32>;

@fragment
fn fs(in: Out) -> @location(0) vec4f {
  let y = textureSample(yTex, smp, in.uv).r;
  let c = textureSample(uvTex, smp, in.uv).rg - vec2f(0.5);
  return vec4f(
    y + 1.5748 * c.y,
    y - 0.1873 * c.x - 0.4681 * c.y,
    y + 1.8556 * c.x,
    1.0,
  );
}
`;

const GLSL_VERTEX = `#version 300 es
out vec2 uv;
void main() {
  vec2 p = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2)) * 2.0 - 1.0;
  uv = vec2((p.x + 1.0) * 0.5, (1.0 - p.y) * 0.5);
  gl_Position = vec4(p, 0.0, 1.0);
}`;

const GLSL_FRAGMENT = `#version 300 es
precision mediump float;
in vec2 uv;
uniform sampler2D yTex;
uniform sampler2D uvTex;
out vec4 color;
void main() {
  float y = texture(yTex, uv).r;
  vec2 c = texture(uvTex, uv).rg - 0.5;
  color = vec4(
    y + 1.5748 * c.y,
    y - 0.1873 * c.x - 0.4681 * c.y,
    y + 1.8556 * c.x,
    1.0
  );
}`;

function resizeCanvas(
  canvas: HTMLCanvasElement,
  width: number,
  height: number,
) {
  if (canvas.width !== width) canvas.width = width;
  if (canvas.height !== height) canvas.height = height;
}

async function createWebGpu(
  canvas: HTMLCanvasElement,
): Promise<Renderer | null> {
  if (!("gpu" in navigator) || !navigator.gpu) return null;
  const adapter = await navigator.gpu.requestAdapter();
  if (!adapter) return null;
  const device = await adapter.requestDevice();
  const context = canvas.getContext("webgpu");
  if (!context) {
    device.destroy();
    return null;
  }
  const format = navigator.gpu.getPreferredCanvasFormat();
  context.configure({ device, format, alphaMode: "opaque" });
  const module = device.createShaderModule({ code: WGSL });
  const pipeline = device.createRenderPipeline({
    layout: "auto",
    vertex: { module, entryPoint: "vs" },
    fragment: { module, entryPoint: "fs", targets: [{ format }] },
    primitive: { topology: "triangle-list" },
  });
  const sampler = device.createSampler({
    magFilter: "linear",
    minFilter: "linear",
  });
  let planes: {
    width: number;
    height: number;
    y: GPUTexture;
    uv: GPUTexture;
    bind: GPUBindGroup;
  } | null = null;

  const ensurePlanes = (width: number, height: number) => {
    if (planes && planes.width === width && planes.height === height)
      return planes;
    planes?.y.destroy();
    planes?.uv.destroy();
    const usage = GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.COPY_DST;
    const y = device.createTexture({
      size: [width, height],
      format: "r8unorm",
      usage,
    });
    const uv = device.createTexture({
      size: [width / 2, height / 2],
      format: "rg8unorm",
      usage,
    });
    const bind = device.createBindGroup({
      layout: pipeline.getBindGroupLayout(0),
      entries: [
        { binding: 0, resource: sampler },
        { binding: 1, resource: y.createView() },
        { binding: 2, resource: uv.createView() },
      ],
    });
    planes = { width, height, y, uv, bind };
    return planes;
  };

  return {
    kind: "webgpu",
    canvas,
    draw: ({ header, y, uv }) => {
      const { width, height } = header;
      resizeCanvas(canvas, width, height);
      const p = ensurePlanes(width, height);
      device.queue.writeTexture({ texture: p.y }, y, { bytesPerRow: width }, [
        width,
        height,
      ]);
      device.queue.writeTexture({ texture: p.uv }, uv, { bytesPerRow: width }, [
        width / 2,
        height / 2,
      ]);
      const encoder = device.createCommandEncoder();
      const pass = encoder.beginRenderPass({
        colorAttachments: [
          {
            view: context.getCurrentTexture().createView(),
            loadOp: "clear",
            storeOp: "store",
            clearValue: { r: 0, g: 0, b: 0, a: 1 },
          },
        ],
      });
      pass.setPipeline(pipeline);
      pass.setBindGroup(0, p.bind);
      pass.draw(3);
      pass.end();
      device.queue.submit([encoder.finish()]);
    },
    destroy: () => {
      planes?.y.destroy();
      planes?.uv.destroy();
      context.unconfigure();
      device.destroy();
    },
  };
}

function compile(gl: WebGL2RenderingContext, type: number, source: string) {
  const shader = gl.createShader(type)!;
  gl.shaderSource(shader, source);
  gl.compileShader(shader);
  if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) {
    throw new Error(gl.getShaderInfoLog(shader) ?? "shader error");
  }
  return shader;
}

function createWebGl(canvas: HTMLCanvasElement): Renderer | null {
  const gl = canvas.getContext("webgl2", {
    alpha: false,
    antialias: false,
    depth: false,
    preserveDrawingBuffer: false,
  });
  if (!gl) return null;
  const program = gl.createProgram()!;
  gl.attachShader(program, compile(gl, gl.VERTEX_SHADER, GLSL_VERTEX));
  gl.attachShader(program, compile(gl, gl.FRAGMENT_SHADER, GLSL_FRAGMENT));
  gl.linkProgram(program);
  if (!gl.getProgramParameter(program, gl.LINK_STATUS)) {
    throw new Error(gl.getProgramInfoLog(program) ?? "program error");
  }
  gl.useProgram(program);
  gl.uniform1i(gl.getUniformLocation(program, "yTex"), 0);
  gl.uniform1i(gl.getUniformLocation(program, "uvTex"), 1);
  gl.pixelStorei(gl.UNPACK_ALIGNMENT, 1);
  const vao = gl.createVertexArray();
  gl.bindVertexArray(vao);

  const texture = (unit: number) => {
    const t = gl.createTexture()!;
    gl.activeTexture(gl.TEXTURE0 + unit);
    gl.bindTexture(gl.TEXTURE_2D, t);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
    return t;
  };
  const yTex = texture(0);
  const uvTex = texture(1);
  let size = { width: 0, height: 0 };

  return {
    kind: "webgl2",
    canvas,
    draw: ({ header, y, uv }) => {
      const { width, height } = header;
      resizeCanvas(canvas, width, height);
      const sameSize = size.width === width && size.height === height;
      gl.activeTexture(gl.TEXTURE0);
      gl.bindTexture(gl.TEXTURE_2D, yTex);
      if (sameSize) {
        gl.texSubImage2D(
          gl.TEXTURE_2D,
          0,
          0,
          0,
          width,
          height,
          gl.RED,
          gl.UNSIGNED_BYTE,
          y,
        );
      } else {
        gl.texImage2D(
          gl.TEXTURE_2D,
          0,
          gl.R8,
          width,
          height,
          0,
          gl.RED,
          gl.UNSIGNED_BYTE,
          y,
        );
      }
      gl.activeTexture(gl.TEXTURE1);
      gl.bindTexture(gl.TEXTURE_2D, uvTex);
      if (sameSize) {
        gl.texSubImage2D(
          gl.TEXTURE_2D,
          0,
          0,
          0,
          width / 2,
          height / 2,
          gl.RG,
          gl.UNSIGNED_BYTE,
          uv,
        );
      } else {
        gl.texImage2D(
          gl.TEXTURE_2D,
          0,
          gl.RG8,
          width / 2,
          height / 2,
          0,
          gl.RG,
          gl.UNSIGNED_BYTE,
          uv,
        );
      }
      size = { width, height };
      gl.viewport(0, 0, width, height);
      gl.drawArrays(gl.TRIANGLES, 0, 3);
    },
    destroy: () => {
      gl.deleteTexture(yTex);
      gl.deleteTexture(uvTex);
      gl.deleteProgram(program);
      gl.getExtension("WEBGL_lose_context")?.loseContext();
    },
  };
}

/**
 * Draws NV12 frames with WebGPU when the webview offers it, otherwise WebGL2.
 * Each attempt gets its own canvas, since a canvas keeps the first kind of
 * context it hands out.
 */
export async function createRenderer(
  prefer: RendererKind | "auto" = "auto",
): Promise<Renderer> {
  if (prefer !== "webgl2") {
    try {
      const gpu = await createWebGpu(document.createElement("canvas"));
      if (gpu) return gpu;
    } catch (e) {
      console.warn("WebGPU is unavailable", e);
    }
  }
  const gl = createWebGl(document.createElement("canvas"));
  if (!gl) throw new Error("Neither WebGPU nor WebGL2 is available");
  return gl;
}
