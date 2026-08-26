/// <reference types="@webgpu/types" />
// WebGPU renderer: same world, same camera math as the CPU engine, but the
// triangle rasterization (depth test, texturing) runs on the GPU.

import type { Mesh, Texture, Tri } from './engine3d';

const STRIDE = 32; // pos3 (12) + uv2 (8) + color3 (12)
const NEAR = 0.05;
const FAR = 1000;

const SHADER = `
struct Uniforms {
  viewProj: mat4x4<f32>,
};
@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var tex: texture_2d<f32>;
@group(0) @binding(2) var smp: sampler;

struct VSIn {
  @location(0) pos: vec3<f32>,
  @location(1) uv: vec2<f32>,
  @location(2) color: vec3<f32>,
};
struct VSOut {
  @builtin(position) clip: vec4<f32>,
  @location(0) uv: vec2<f32>,
  @location(1) color: vec3<f32>,
};

@vertex fn vs(in: VSIn) -> VSOut {
  var out: VSOut;
  out.clip = u.viewProj * vec4<f32>(in.pos, 1.0);
  out.uv = in.uv;
  out.color = in.color;
  return out;
}

@fragment fn fs(in: VSOut) -> @location(0) vec4<f32> {
  return vec4<f32>(textureSample(tex, smp, in.uv).rgb * in.color, 1.0);
}
`;

interface DrawGroup {
  buffer: GPUBuffer;
  count: number;
  bindGroup: GPUBindGroup;
}

export class WgpuRenderer {
  device: GPUDevice;
  private context: GPUCanvasContext;
  private pipeline: GPURenderPipeline;
  private depth: GPUTexture;
  private uniformBuf: GPUBuffer;
  private groups: DrawGroup[] = [];
  private format: GPUTextureFormat;
  private w: number;
  private h: number;
  private sky: [number, number, number];

  static async create(canvas: HTMLCanvasElement, w: number, h: number, textures: Texture[], meshes: Mesh[]): Promise<WgpuRenderer> {
    const adapter = await navigator.gpu.requestAdapter();
    if (!adapter) throw new Error('no WebGPU adapter');
    const device = await adapter.requestDevice();
    const context = canvas.getContext('webgpu');
    if (!context) throw new Error('no webgpu context');
    const format = navigator.gpu.getPreferredCanvasFormat();
    context.configure({ device, format, alphaMode: 'opaque' });
    return new WgpuRenderer(device, context, format, canvas, w, h, textures, meshes);
  }

  private constructor(device: GPUDevice, context: GPUCanvasContext, format: GPUTextureFormat, canvas: HTMLCanvasElement, w: number, h: number, textures: Texture[], meshes: Mesh[]) {
    this.device = device;
    this.context = context;
    this.w = w;
    this.h = h;
    this.format = format;
    this.sky = [232 / 255, 200 / 255, 156 / 255];

    this.uniformBuf = device.createBuffer({ size: 64, usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST });

    const layoutEntries: GPUBindGroupLayoutEntry[] = [
      { binding: 0, visibility: GPUShaderStage.VERTEX, buffer: { type: 'uniform' } },
      { binding: 1, visibility: GPUShaderStage.FRAGMENT, texture: {} },
      { binding: 2, visibility: GPUShaderStage.FRAGMENT, sampler: {} },
    ];
    const layout = device.createBindGroupLayout({ entries: layoutEntries });
    const pipelineLayout = device.createPipelineLayout({ bindGroupLayouts: [layout] });
    const vertexAttrs: GPUVertexAttribute[] = [
      { shaderLocation: 0, offset: 0, format: 'float32x3' },
      { shaderLocation: 1, offset: 12, format: 'float32x2' },
      { shaderLocation: 2, offset: 20, format: 'float32x3' },
    ];
    const vertexLayouts: GPUVertexBufferLayout[] = [{ arrayStride: STRIDE, attributes: vertexAttrs }];
    this.pipeline = device.createRenderPipeline({
      layout: pipelineLayout,
      vertex: {
        module: device.createShaderModule({ code: SHADER }),
        entryPoint: 'vs',
        buffers: vertexLayouts,
      },
      fragment: {
        module: device.createShaderModule({ code: SHADER }),
        entryPoint: 'fs',
        targets: [{ format }],
      },
      primitive: { topology: 'triangle-list', cullMode: 'none' },
      depthStencil: {
        format: 'depth24plus',
        depthWriteEnabled: true,
        depthCompare: 'less',
      },
    });

    this.depth = device.createTexture({
      size: { width: w, height: h },
      format: 'depth24plus',
      usage: GPUTextureUsage.RENDER_ATTACHMENT,
    });

    // GPU copies of all textures (padded rows satisfy writeTexture alignment)
    const sampler = device.createSampler({ magFilter: 'linear', minFilter: 'linear', addressModeU: 'repeat', addressModeV: 'repeat' });
    const gpuTextures: GPUTexture[] = [];
    for (const tex of textures) {
      gpuTextures.push(this.uploadTexture(tex));
    }
    // flat-color triangles use a 1x1 white texture; the vertex color carries the shade
    gpuTextures.push(this.makeWhite());

    const byTex = new Map<number, Tri[]>();
    for (const mesh of meshes) {
      for (const tri of mesh.tris) {
        const key = tri.tex >= 0 ? tri.tex : gpuTextures.length - 1;
        const list = byTex.get(key);
        if (list) list.push(tri);
        else byTex.set(key, [tri]);
      }
    }
    for (const [texIndex, tris] of Array.from(byTex)) {
      const flatColor = texIndex === gpuTextures.length - 1;
      const verts = new Float32Array(tris.length * 3 * 8);
      let o = 0;
      for (const tri of tris) {
        for (const p of [tri.a, tri.b, tri.c]) {
          verts[o++] = p.x;
          verts[o++] = p.y;
          verts[o++] = p.z;
          verts[o++] = p.u;
          verts[o++] = p.v;
          const cr = flatColor ? (((tri.color >> 16) & 255) / 255) * tri.shade : tri.shade;
          const cg = flatColor ? (((tri.color >> 8) & 255) / 255) * tri.shade : tri.shade;
          const cb = flatColor ? ((tri.color & 255) / 255) * tri.shade : tri.shade;
          verts[o++] = cr;
          verts[o++] = cg;
          verts[o++] = cb;
        }
      }
      const buffer = device.createBuffer({ size: verts.byteLength, usage: GPUBufferUsage.VERTEX | GPUBufferUsage.COPY_DST });
      device.queue.writeBuffer(buffer, 0, verts);
      const view = gpuTextures[texIndex].createView();
      const bindGroup = device.createBindGroup({
        layout,
        entries: [
          { binding: 0, resource: { buffer: this.uniformBuf } },
          { binding: 1, resource: view },
          { binding: 2, resource: sampler },
        ],
      });
      this.groups.push({ buffer, count: tris.length * 3, bindGroup });
    }
  }

  private uploadTexture(tex: Texture): GPUTexture {
    const gpu = this.device.createTexture({
      size: { width: tex.w, height: tex.h },
      format: 'rgba8unorm',
      usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.COPY_DST,
    });
    const rowBytes = tex.w * 4;
    const padded = Math.ceil(rowBytes / 256) * 256;
    const bytes = new Uint8Array(tex.data.buffer);
    if (padded === rowBytes) {
      this.device.queue.writeTexture({ texture: gpu }, bytes, { bytesPerRow: padded }, { width: tex.w, height: tex.h });
    } else {
      const paddedBuf = new Uint8Array(padded * tex.h);
      for (let y = 0; y < tex.h; y++) paddedBuf.set(bytes.subarray(y * rowBytes, (y + 1) * rowBytes), y * padded);
      this.device.queue.writeTexture({ texture: gpu }, paddedBuf, { bytesPerRow: padded, rowsPerImage: tex.h }, { width: tex.w, height: tex.h });
    }
    return gpu;
  }

  private makeWhite(): GPUTexture {
    const gpu = this.device.createTexture({
      size: { width: 1, height: 1 },
      format: 'rgba8unorm',
      usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.COPY_DST,
    });
    this.device.queue.writeTexture({ texture: gpu }, new Uint8Array([255, 255, 255, 255]), { bytesPerRow: 4 }, { width: 1, height: 1 });
    return gpu;
  }

  render(viewProj: Float32Array): void {
    this.device.queue.writeBuffer(this.uniformBuf, 0, viewProj);
    const enc = this.device.createCommandEncoder();
    const colorAttachments: GPURenderPassColorAttachment[] = [
      {
        view: this.context.getCurrentTexture().createView(),
        clearValue: { r: this.sky[0], g: this.sky[1], b: this.sky[2], a: 1 },
        loadOp: 'clear',
        storeOp: 'store',
      },
    ];
    const pass = enc.beginRenderPass({
      colorAttachments,
      depthStencilAttachment: {
        view: this.depth.createView(),
        depthClearValue: 1,
        depthLoadOp: 'clear',
        depthStoreOp: 'store',
      },
    });
    pass.setPipeline(this.pipeline);
    for (const g of this.groups) {
      pass.setBindGroup(0, g.bindGroup);
      pass.setVertexBuffer(0, g.buffer);
      pass.draw(g.count);
    }
    pass.end();
    this.device.queue.submit([enc.finish()]);
  }
  // Render the current camera into an offscreen texture and read it back as
  // RGBA bytes (canvas textures are not COPY_SRC, so we need a side target).
  async readFrame(): Promise<Uint8Array> {
    const w = this.w;
    const h = this.h;
    const off = this.device.createTexture({
      size: { width: w, height: h },
      format: this.format,
      usage: GPUTextureUsage.RENDER_ATTACHMENT | GPUTextureUsage.COPY_SRC,
    });
    const enc = this.device.createCommandEncoder();
    const colorAttachments: GPURenderPassColorAttachment[] = [
      {
        view: off.createView(),
        clearValue: { r: this.sky[0], g: this.sky[1], b: this.sky[2], a: 1 },
        loadOp: 'clear',
        storeOp: 'store',
      },
    ];
    const pass = enc.beginRenderPass({
      colorAttachments,
      depthStencilAttachment: {
        view: this.depth.createView(),
        depthClearValue: 1,
        depthLoadOp: 'clear',
        depthStoreOp: 'store',
      },
    });
    pass.setPipeline(this.pipeline);
    for (const g of this.groups) {
      pass.setBindGroup(0, g.bindGroup);
      pass.setVertexBuffer(0, g.buffer);
      pass.draw(g.count);
    }
    pass.end();
    const buf = this.device.createBuffer({ size: w * h * 4, usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ });
    enc.copyTextureToBuffer({ texture: off }, { buffer: buf, bytesPerRow: w * 4 }, { width: w, height: h });
    this.device.queue.submit([enc.finish()]);
    await buf.mapAsync(GPUMapMode.READ);
    const data = new Uint8Array(buf.getMappedRange()).slice();
    buf.unmap();
    if (this.format === 'bgra8unorm') {
      for (let i = 0; i < data.length; i += 4) {
        const t = data[i];
        data[i] = data[i + 2];
        data[i + 2] = t;
      }
    }
    return data;
  }
}

// ---- camera matrices (column-major, same convention as the CPU engine) ----

export function viewMatrix(yaw: number, pitch: number, pos: { x: number; y: number; z: number }): Float32Array {
  const cy = Math.cos(yaw);
  const sy = Math.sin(yaw);
  const f = [sy, 0, cy];
  const r = [cy, 0, -sy];
  const u = [0, 1, 0];
  // column-major view matrix: columns are the right/up/-forward basis vectors,
  // translation lives in the w row (standard lookAt layout)
  const m = new Float32Array(16);
  m[0] = r[0]; m[1] = u[0]; m[2] = -f[0]; m[3] = 0;
  m[4] = r[1]; m[5] = u[1]; m[6] = -f[1]; m[7] = 0;
  m[8] = r[2]; m[9] = u[2]; m[10] = -f[2]; m[11] = 0;
  m[12] = -(r[0] * pos.x + r[1] * pos.y + r[2] * pos.z);
  m[13] = -(u[0] * pos.x + u[1] * pos.y + u[2] * pos.z);
  m[14] = f[0] * pos.x + f[1] * pos.y + f[2] * pos.z;
  m[15] = 1;
  // pitch: view space has z pointing backward, so the rotation angle is
  // negated relative to the CPU engine's depth-space pitch
  const c = Math.cos(pitch);
  const s = -Math.sin(pitch);
  const rx = new Float32Array([1, 0, 0, 0, 0, c, s, 0, 0, -s, c, 0, 0, 0, 0, 1]);
  return mat4Multiply(rx, m);
}

export function projectionMatrix(fovY: number, aspect: number): Float32Array {
  const f = 1 / Math.tan(fovY / 2);
  return new Float32Array([
    f / aspect, 0, 0, 0,
    0, f, 0, 0,
    0, 0, FAR / (NEAR - FAR), -1,
    0, 0, (FAR * NEAR) / (NEAR - FAR), 0,
  ]);
}

export function mat4Multiply(a: Float32Array, b: Float32Array): Float32Array {
  const o = new Float32Array(16);
  for (let c = 0; c < 4; c++) {
    for (let r = 0; r < 4; r++) {
      let s = 0;
      for (let k = 0; k < 4; k++) s += a[k * 4 + r] * b[c * 4 + k];
      o[c * 4 + r] = s;
    }
  }
  return o;
}
