// Software 3D engine, same vector method as the original renderer:
// project polygon vertices, fill them, depth-order them. No raycasting,
// no raytracing. Extended from vertical wall quads to arbitrary 3D
// triangles with perspective-correct texturing.

export interface Vec3 {
  x: number;
  y: number;
  z: number;
}

export interface TriVertex {
  x: number;
  y: number;
  z: number;
  u: number; // texture u in [0,1]
  v: number; // texture v in [0,1], 0 = texture top row
}

export interface Tri {
  a: TriVertex;
  b: TriVertex;
  c: TriVertex;
  tex: number; // index into Engine.textures; -1 = flat color
  color: number; // packed 0xRRGGBB, used when tex < 0
  shade: number; // precomputed face brightness 0..1
  twoSided: boolean;
}

export interface Mesh {
  tris: Tri[];
}

export interface Texture {
  w: number;
  h: number;
  data: Uint32Array; // ABGR packed rows, row 0 = image top
}

export interface Camera {
  x: number;
  y: number;
  z: number;
  yaw: number; // radians; 0 = looking toward +z
  pitch: number; // radians; positive = looking up
}

const NEAR = 0.05;

interface PVert {
  x: number;
  y: number;
  iz: number;
  uz: number;
  vz: number;
}

interface PTri {
  a: PVert;
  b: PVert;
  c: PVert;
  tex: number;
  color: number;
  shade: number;
  depth: number; // centroid camera-space z, used for painter order
}

interface CVertex {
  x: number;
  y: number;
  z: number;
  u: number;
  v: number;
}

export class Engine {
  w: number;
  h: number;
  camera: Camera;
  textures: Texture[] = [];
  stats = { tris: 0, pixels: 0 };

  private buf: Uint32Array;
  private zbuf: Float32Array;
  private img: ImageData;
  private focal: number;
  private sky: number;
  lastFrame = { tris: 0, pixels: 0 };

  // per-frame pool
  private drawn: PTri[] = [];

  constructor(w: number, h: number) {
    this.w = w;
    this.h = h;
    this.camera = { x: 0, y: 30, z: -70, yaw: 0, pitch: -0.4 };
    this.buf = new Uint32Array(w * h);
    this.zbuf = new Float32Array(w * h);
    this.img = new ImageData(w, h);
    this.focal = (h / 2) / Math.tan((70 * Math.PI) / 180 / 2);
    this.sky = 0xff9cc8e8 >>> 0; // light blue ABGR
  }

  clear(): void {
    this.buf.fill(this.sky);
    this.zbuf.fill(0); // 0 = infinitely far; nearer triangles have larger 1/z
    this.drawn.length = 0;
    this.stats.tris = 0;
    this.stats.pixels = 0;
  }

  // Project a world-space triangle into camera space, clip against the near
  // plane, backface-cull, perspective-project. Push onto `drawn` when visible.
  private project(t: Tri, cosY: number, sinY: number, cosP: number, sinP: number): void {
    const cam = this.camera;
    // camera space (yaw then pitch) + attributes
    const verts: CVertex[] = [];
    const src = [t.a, t.b, t.c];
    for (let i = 0; i < 3; i++) {
      const p = src[i];
      const rx = p.x - cam.x;
      const ry = p.y - cam.y;
      const rz = p.z - cam.z;
      const y1 = rx * cosY - rz * sinY;
      const z1 = rx * sinY + rz * cosY;
      const y2 = ry * cosP - z1 * sinP;
      const z2 = ry * sinP + z1 * cosP;
      verts.push({ x: y1, y: y2, z: z2, u: p.u, v: p.v });
    }
    if (verts[0].z <= NEAR && verts[1].z <= NEAR && verts[2].z <= NEAR) {
      return;
    }
    if (verts[0].z <= NEAR || verts[1].z <= NEAR || verts[2].z <= NEAR) {
      // clip against the near plane; may produce up to two triangles
      const clipped = clipNear(verts);
      for (const tri of clipped) {
        this.finish(tri[0], tri[1], tri[2], t);
      }
      return;
    }
    this.finish(verts[0], verts[1], verts[2], t);
  }

  private finish(a: CVertex, b: CVertex, c: CVertex, t: Tri): void {
    // backface cull: normal points toward camera when dot(n, centroid) < 0
    if (!t.twoSided) {
      const abx = b.x - a.x;
      const aby = b.y - a.y;
      const abz = b.z - a.z;
      const acx = c.x - a.x;
      const acy = c.y - a.y;
      const acz = c.z - a.z;
      const nx = aby * acz - abz * acy;
      const ny = abz * acx - abx * acz;
      const nz = abx * acy - aby * acx;
      const cx = (a.x + b.x + c.x) / 3;
      const cy = (a.y + b.y + c.y) / 3;
      const cz = (a.z + b.z + c.z) / 3;
      if (nx * cx + ny * cy + nz * cz >= 0) {
        return;
      }
    }
    if (a.z <= NEAR || b.z <= NEAR || c.z <= NEAR) {
      return; // degenerate after clip
    }
    const iza = 1 / a.z;
    const izb = 1 / b.z;
    const izc = 1 / c.z;
    this.drawn.push({
      a: { x: this.w / 2 + this.focal * (a.x / a.z), y: this.h / 2 - this.focal * (a.y / a.z), iz: iza, uz: a.u * iza, vz: a.v * iza },
      b: { x: this.w / 2 + this.focal * (b.x / b.z), y: this.h / 2 - this.focal * (b.y / b.z), iz: izb, uz: b.u * izb, vz: b.v * izb },
      c: { x: this.w / 2 + this.focal * (c.x / c.z), y: this.h / 2 - this.focal * (c.y / c.z), iz: izc, uz: c.u * izc, vz: c.v * izc },
      tex: t.tex,
      color: t.color,
      shade: t.shade,
      depth: (a.z + b.z + c.z) / 3,
    });
    this.stats.tris++;
  }

  render(meshes: Mesh[]): void {
    this.clear();
    const cam = this.camera;
    const cosY = Math.cos(cam.yaw);
    const sinY = Math.sin(cam.yaw);
    const cosP = Math.cos(cam.pitch);
    const sinP = Math.sin(cam.pitch);
    for (const mesh of meshes) {
      for (const tri of mesh.tris) {
        this.project(tri, cosY, sinY, cosP, sinP);
      }
    }
    // painter order: far triangles first, so nearer ones overdraw
    this.drawn.sort((a, b) => b.depth - a.depth);
    for (const t of this.drawn) {
      this.raster(t);
    }
  }

  // Scanline rasterizer with per-pixel z-buffer and perspective-correct
  // texture coordinates (interpolating 1/z, u/z, v/z in screen space).
  private raster(t: PTri): void {
    const tex = t.tex >= 0 ? this.textures[t.tex] : null;
    const tw = tex ? tex.w : 1;
    const th = tex ? tex.h : 1;
    const shade = t.shade;
    // sort vertices by y
    let v0 = t.a;
    let v1 = t.b;
    let v2 = t.c;
    if (v1.y < v0.y) [v0, v1] = [v1, v0];
    if (v2.y < v0.y) [v0, v2] = [v2, v0];
    if (v2.y < v1.y) [v1, v2] = [v2, v1];
    const yTop = Math.max(0, Math.ceil(v0.y));
    const yBot = Math.min(this.h - 1, Math.floor(v2.y));
    if (yBot < yTop) return;
    const invDx = (v1.y - v0.y) !== 0 ? 1 / (v1.y - v0.y) : 0;
    const invDx2 = (v2.y - v0.y) !== 0 ? 1 / (v2.y - v0.y) : 0;
    const invDx3 = (v2.y - v1.y) !== 0 ? 1 / (v2.y - v1.y) : 0;
    const buf = this.buf;
    const zbuf = this.zbuf;
    const w = this.w;
    for (let y = yTop; y <= yBot; y++) {
      // long edge v0->v2
      const tL = (y - v0.y) * invDx2;
      const longX = v0.x + (v2.x - v0.x) * tL;
      const longIz = v0.iz + (v2.iz - v0.iz) * tL;
      const longUz = v0.uz + (v2.uz - v0.uz) * tL;
      const longVz = v0.vz + (v2.vz - v0.vz) * tL;
      // secondary edge: v0->v1 above mid, v1->v2 below
      let xS: number;
      let sIz: number;
      let sUz: number;
      let sVz: number;
      if (y < v1.y) {
        const tS = (y - v0.y) * invDx;
        xS = v0.x + (v1.x - v0.x) * tS;
        sIz = v0.iz + (v1.iz - v0.iz) * tS;
        sUz = v0.uz + (v1.uz - v0.uz) * tS;
        sVz = v0.vz + (v1.vz - v0.vz) * tS;
      } else {
        const tS = (y - v1.y) * invDx3;
        xS = v1.x + (v2.x - v1.x) * tS;
        sIz = v1.iz + (v2.iz - v1.iz) * tS;
        sUz = v1.uz + (v2.uz - v1.uz) * tS;
        sVz = v1.vz + (v2.vz - v1.vz) * tS;
      }
      let xL: number;
      let lIz: number;
      let lUz: number;
      let lVz: number;
      let xR: number;
      let rIz: number;
      let rUz: number;
      let rVz: number;
      if (longX <= xS) {
        xL = longX; lIz = longIz; lUz = longUz; lVz = longVz;
        xR = xS; rIz = sIz; rUz = sUz; rVz = sVz;
      } else {
        xL = xS; lIz = sIz; lUz = sUz; lVz = sVz;
        xR = longX; rIz = longIz; rUz = longUz; rVz = longVz;
      }
      const x0 = Math.max(0, Math.ceil(xL));
      const x1 = Math.min(w - 1, Math.floor(xR));
      if (x1 < x0) continue;
      const rowOff = y * w;
      const span = xR - xL;
      const invSpan = span !== 0 ? 1 / span : 0;
      for (let x = x0; x <= x1; x++) {
        const tt = (x - xL) * invSpan;
        const iz = lIz + (rIz - lIz) * tt;
        if (iz <= zbuf[rowOff + x]) continue;
        zbuf[rowOff + x] = iz;
        this.stats.pixels++;
        let px: number;
        if (tex) {
          const u = (lUz + (rUz - lUz) * tt) / iz;
          const v = (lVz + (rVz - lVz) * tt) / iz;
          let ux = (u * tw) | 0;
          let uy = (v * th) | 0;
          ux = ((ux % tw) + tw) % tw;
          uy = ((uy % th) + th) % th;
          px = tex.data[uy * tw + ux];
        } else {
          px = 0xff000000 | t.color;
        }
        if (shade !== 1) {
          const r = ((px & 255) * shade) | 0;
          const g = (((px >> 8) & 255) * shade) | 0;
          const b = (((px >> 16) & 255) * shade) | 0;
          px = 0xff000000 | (b << 16) | (g << 8) | r;
        }
        buf[rowOff + x] = px;
      }
    }
  }

  present(ctx: CanvasRenderingContext2D): void {
    this.img.data.set(new Uint8ClampedArray(this.buf.buffer));
    this.lastFrame.tris = this.stats.tris;
    this.lastFrame.pixels = this.stats.pixels;
    ctx.putImageData(this.img, 0, 0);
  }
}

// Clip a camera-space triangle against z = NEAR; returns 0..2 triangles.
function clipNear(verts: CVertex[]): [CVertex, CVertex, CVertex][] {
  const out: CVertex[] = [];
  const n = verts.length;
  for (let i = 0; i < n; i++) {
    const cur = verts[i];
    const nxt = verts[(i + 1) % n];
    const curIn = cur.z > NEAR;
    const nxtIn = nxt.z > NEAR;
    if (curIn) out.push(cur);
    if (curIn !== nxtIn) {
      const tt = (NEAR - cur.z) / (nxt.z - cur.z);
      out.push({
        x: cur.x + (nxt.x - cur.x) * tt,
        y: cur.y + (nxt.y - cur.y) * tt,
        z: NEAR,
        u: cur.u + (nxt.u - cur.u) * tt,
        v: cur.v + (nxt.v - cur.v) * tt,
      });
    }
  }
  const tris: [CVertex, CVertex, CVertex][] = [];
  if (out.length >= 3) {
    for (let i = 1; i < out.length - 1; i++) {
      tris.push([out[0], out[i], out[i + 1]]);
    }
  }
  return tris;
}

export function addTri(mesh: Mesh, a: TriVertex, b: TriVertex, c: TriVertex, tex: number, color: number, shade: number, twoSided: boolean): void {
  mesh.tris.push({ a, b, c, tex, color, shade, twoSided });
}

// Load a PNG/JPEG into a sampling Texture.
export function makeTexture(img: HTMLImageElement): Texture {
  const c = document.createElement('canvas');
  c.width = img.naturalWidth;
  c.height = img.naturalHeight;
  const g = c.getContext('2d');
  if (!g) throw new Error('no 2d context');
  g.drawImage(img, 0, 0);
  const id = g.getImageData(0, 0, c.width, c.height);
  return { w: c.width, h: c.height, data: new Uint32Array(id.data.buffer) };
}

export function loadImage(url: string): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const img = new Image();
    img.onload = () => { resolve(img); };
    img.onerror = () => { reject(new Error('failed to load ' + url)); };
    img.src = url;
  });
}

// ---- OBJ + MTL loader ----------------------------------------------------

interface ObjMaterial {
  tex: number;
  color: number;
}

export async function loadObj(url: string, textures: Texture[], fallbackTex = -1): Promise<Mesh> {
  const base = url.slice(0, url.lastIndexOf('/') + 1);
  const text = await (await fetch(url)).text();
  const materials = new Map<string, ObjMaterial>();
  const imgCache = new Map<string, Texture>();
  const getTex = async (file: string): Promise<number> => {
    const key = base + file;
    let tex = imgCache.get(key);
    if (!tex) {
      tex = makeTexture(await loadImage(key));
      imgCache.set(key, tex);
    }
    const idx = textures.indexOf(tex);
    if (idx >= 0) return idx;
    textures.push(tex);
    return textures.length - 1;
  };
  // parse mtllib references first
  const mtlFiles: string[] = [];
  for (const line of text.split('\n')) {
    if (line.startsWith('mtllib ')) mtlFiles.push(line.slice(7).trim());
  }
  for (const file of mtlFiles) {
    const mtlText = await (await fetch(base + file)).text();
    let name = '';
    let tex = -1;
    let color = 0x9c9c9c;
    for (const line of mtlText.split('\n')) {
      const s = line.trim();
      if (s.startsWith('newmtl ')) {
        if (name) materials.set(name, { tex, color });
        name = s.slice(7).trim();
        tex = -1;
        color = 0x9c9c9c;
      } else if (s.startsWith('map_Kd ')) {
        tex = await getTex(s.slice(7).trim());
      } else if (s.startsWith('Kd ')) {
        const p = s.split(/\s+/).slice(1).map(Number);
        if (p.length >= 3) color = (Math.round(p[0] * 255) << 16) | (Math.round(p[1] * 255) << 8) | Math.round(p[2] * 255);
      }
    }
    if (name) materials.set(name, { tex: tex >= 0 ? tex : fallbackTex, color });
  }
  // geometry
  const vs: number[] = [];
  const vts: number[] = [];
  const mesh: Mesh = { tris: [] };
  let cur = materials.get('default') ?? { tex: -1, color: 0x9c9c9c };
  const faces: number[][] = [];
  for (const raw of text.split('\n')) {
    const line = raw.trim();
    if (line.startsWith('v ')) {
      const p = line.split(/\s+/).slice(1).map(Number);
      vs.push(p[0], p[1], p[2]);
    } else if (line.startsWith('vt ')) {
      const p = line.split(/\s+/).slice(1).map(Number);
      vts.push(p[0], p[1]);
    } else if (line.startsWith('usemtl ')) {
      cur = materials.get(line.slice(7).trim()) ?? { tex: -1, color: 0x9c9c9c };
    } else if (line.startsWith('f ')) {
      const face: number[] = [];
      for (const part of line.split(/\s+/).slice(1)) {
        const idx = part.split('/');
        const vi = parseInt(idx[0], 10);
        const ti = idx.length > 1 && idx[1] !== '' ? parseInt(idx[1], 10) : -1;
        face.push(vi, ti);
      }
      if (face.length >= 6) faces.push(face);
    }
  }
  const v = (i: number): number => {
    const k = i < 0 ? vs.length + i * 3 : (i - 1) * 3;
    return k;
  };
  for (const face of faces) {
    const n = face.length / 2;
    const mk = (fi: number): TriVertex => {
      const vi = face[fi * 2];
      const ti = face[fi * 2 + 1];
      const k = v(vi);
      let u = 0;
      let vv = 0;
      if (ti >= 0) {
        const tk = ti < 0 ? vts.length + ti * 2 : (ti - 1) * 2;
        u = vts[tk];
        vv = 1 - vts[tk + 1]; // OBJ v origin is bottom-left
      }
      return { x: vs[k], y: vs[k + 1], z: vs[k + 2], u, v: vv };
    };
    for (let i = 1; i < n - 1; i++) {
      addTri(mesh, mk(0), mk(i), mk(i + 1), cur.tex, cur.color, 1, false);
    }
  }
  return mesh;
}

// Apply scale, rotation around Y, and translation to a mesh (world placement).
export function transformMesh(mesh: Mesh, sx: number, sy: number, sz: number, rotY: number, tx: number, ty: number, tz: number): void {
  const c = Math.cos(rotY);
  const s = Math.sin(rotY);
  for (const tri of mesh.tris) {
    for (const p of [tri.a, tri.b, tri.c]) {
      const x = p.x * sx;
      const y = p.y * sy;
      const z = p.z * sz;
      p.x = x * c + z * s + tx;
      p.y = y + ty;
      p.z = -x * s + z * c + tz;
    }
  }
}

// Flat shading from a fixed light direction, per face.
export function shadeMesh(mesh: Mesh, lx: number, ly: number, lz: number): void {
  const len = Math.hypot(lx, ly, lz);
  lx /= len;
  ly /= len;
  lz /= len;
  for (const tri of mesh.tris) {
    const abx = tri.b.x - tri.a.x;
    const aby = tri.b.y - tri.a.y;
    const abz = tri.b.z - tri.a.z;
    const acx = tri.c.x - tri.a.x;
    const acy = tri.c.y - tri.a.y;
    const acz = tri.c.z - tri.a.z;
    const nx = aby * acz - abz * acy;
    const ny = abz * acx - abx * acz;
    const nz = abx * acy - aby * acx;
    const nl = Math.hypot(nx, ny, nz);
    if (nl === 0) {
      tri.shade = 1;
      continue;
    }
    const d = (nx * lx + ny * ly + nz * lz) / nl;
    tri.shade = Math.max(0.35, Math.min(1, 0.45 + 0.55 * d));
  }
}
