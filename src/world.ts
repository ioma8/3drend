// World assembly: ground plane, buildings, and downloaded models placed in
// the scene. All meshes are polygons rendered by the vector engine.

import { Engine, Mesh, addTri, loadObj, shadeMesh, transformMesh, TriVertex } from './engine3d';

export interface WorldTextures {
  worldmap: number;
  grasslight: number;
  wall1: number;
  wall2: number;
  roof: number;
  tree: number;
}

export interface Footprint {
  x: number;
  z: number;
  w: number;
  d: number;
}

export interface Marker {
  x: number;
  z: number;
  label: string;
}

export interface World {
  meshes: Mesh[];
  footprints: Footprint[];
  markers: Marker[];
}

const V = (x: number, y: number, z: number, u: number, v: number): TriVertex => ({ x, y, z, u, v });

// Counter-clockwise quad when viewed from outside, split into two tris.
function quad(mesh: Mesh, tex: number, a: TriVertex, b: TriVertex, c: TriVertex, d: TriVertex, twoSided = false): void {
  addTri(mesh, a, b, c, tex, 0, 1, twoSided);
  addTri(mesh, a, c, d, tex, 0, 1, twoSided);
}

// Box with base at y=0. Side faces textured with sideTex (v=1 bottom, v=0 top),
// top with topTex. All faces wound outward (single-sided).
function box(mesh: Mesh, cx: number, cz: number, w: number, h: number, d: number, sideTex: number, topTex: number): void {
  const x0 = cx - w / 2;
  const x1 = cx + w / 2;
  const z0 = cz - d / 2;
  const z1 = cz + d / 2;
  // +Z face
  quad(mesh, sideTex, V(x0, 0, z1, 0, 1), V(x1, 0, z1, 1, 1), V(x1, h, z1, 1, 0), V(x0, h, z1, 0, 0));
  // -Z face
  quad(mesh, sideTex, V(x1, 0, z0, 0, 1), V(x0, 0, z0, 1, 1), V(x0, h, z0, 1, 0), V(x1, h, z0, 0, 0));
  // +X face
  quad(mesh, sideTex, V(x1, 0, z1, 0, 1), V(x1, 0, z0, 1, 1), V(x1, h, z0, 1, 0), V(x1, h, z1, 0, 0));
  // -X face
  quad(mesh, sideTex, V(x0, 0, z0, 0, 1), V(x0, 0, z1, 1, 1), V(x0, h, z1, 1, 0), V(x0, h, z0, 0, 0));
  // top (+Y)
  quad(mesh, topTex, V(x0, h, z1, 0, 1), V(x1, h, z1, 1, 1), V(x1, h, z0, 1, 0), V(x0, h, z0, 0, 0));
  // bottom (-Y), culled unless seen from below; keep single-sided
  quad(mesh, sideTex, V(x0, 0, z0, 0, 0), V(x1, 0, z0, 1, 0), V(x1, 0, z1, 1, 1), V(x0, 0, z1, 0, 1));
}

// Pyramid roof over the box footprint, apex rh above the walls.
function pyramidRoof(mesh: Mesh, cx: number, cz: number, w: number, h: number, d: number, rh: number, tex: number): void {
  const x0 = cx - w / 2;
  const x1 = cx + w / 2;
  const z0 = cz - d / 2;
  const z1 = cz + d / 2;
  const apex = V(cx, h + rh, cz, 0.5, 0.5);
  addTri(mesh, V(x0, h, z1, 0, 1), V(x1, h, z1, 1, 1), apex, tex, 0, 1, false);
  addTri(mesh, V(x1, h, z1, 0, 1), V(x1, h, z0, 1, 1), apex, tex, 0, 1, false);
  addTri(mesh, V(x1, h, z0, 0, 1), V(x0, h, z0, 1, 1), apex, tex, 0, 1, false);
  addTri(mesh, V(x0, h, z0, 0, 1), V(x0, h, z1, 1, 1), apex, tex, 0, 1, false);
}

// The downloaded world-map texture spans the whole plane.
function ground(mesh: Mesh, tex: number, size: number, cells: number): void {
  const cell = size / cells;
  const off = size / 2;
  for (let i = 0; i < cells; i++) {
    for (let j = 0; j < cells; j++) {
      const x0 = -off + i * cell;
      const x1 = x0 + cell;
      const z0 = -off + j * cell;
      const z1 = z0 + cell;
      const u0 = i / cells;
      const u1 = (i + 1) / cells;
      const v0 = j / cells;
      const v1 = (j + 1) / cells;
      quad(mesh, tex,
        V(x0, 0, z0, u0, v0),
        V(x1, 0, z0, u1, v0),
        V(x1, 0, z1, u1, v1),
        V(x0, 0, z1, u0, v1),
        true);
    }
  }
}

// Deterministic heightfield terrain (rolling hills) from a small map of
// heights, textured with the downloaded grass texture.
function terrain(mesh: Mesh, tex: number, cx: number, cz: number, size: number, cells: number, amp: number, seed: number): void {
  let s = seed;
  const rnd = (): number => {
    s = (s * 1103515245 + 12345) & 0x7fffffff;
    return s / 0x7fffffff;
  };
  // smoothed random height map
  const n = cells + 1;
  const raw: number[][] = [];
  for (let i = 0; i < n; i++) {
    raw.push([]);
    for (let j = 0; j < n; j++) raw[i].push(rnd());
  }
  const h = (i: number, j: number): number => {
    const ix = Math.min(n - 1, Math.max(0, i));
    const jy = Math.min(n - 1, Math.max(0, j));
    return raw[ix][jy];
  };
  const cell = size / cells;
  const off = size / 2;
  for (let i = 0; i < cells; i++) {
    for (let j = 0; j < cells; j++) {
      const x0 = cx - off + i * cell;
      const x1 = x0 + cell;
      const z0 = cz - off + j * cell;
      const z1 = z0 + cell;
      const y00 = amp * (h(i, j) * 0.5 + h(i - 1, j) * 0.25 + h(i, j - 1) * 0.25);
      const y10 = amp * (h(i + 1, j) * 0.5 + h(i, j) * 0.25 + h(i + 1, j - 1) * 0.25);
      const y11 = amp * (h(i + 1, j + 1) * 0.5 + h(i, j + 1) * 0.25 + h(i + 1, j) * 0.25);
      const y01 = amp * (h(i, j + 1) * 0.5 + h(i - 1, j + 1) * 0.25 + h(i, j) * 0.25);
      const u0 = i / 4;
      const u1 = (i + 1) / 4;
      const v0 = j / 4;
      const v1 = (j + 1) / 4;
      quad(mesh, tex,
        V(x0, y00, z0, u0, v0),
        V(x1, y10, z0, u1, v0),
        V(x1, y11, z1, u1, v1),
        V(x0, y01, z1, u0, v1),
        false);
    }
  }
}

export async function buildWorld(engine: Engine, tex: WorldTextures): Promise<World> {
  const meshes: Mesh[] = [];
  const footprints: Footprint[] = [];
  const markers: Marker[] = [];

  const g = { tris: [] as Mesh['tris'] };
  ground(g, tex.worldmap, 200, 20);
  meshes.push(g);

  // grass hills from the heightfield map (north-west area)
  const hill: Mesh = { tris: [] };
  terrain(hill, tex.grasslight, -80, 80, 70, 12, 6, 7);
  shadeMesh(hill, -0.4, 0.8, 0.35);
  meshes.push(hill);

  // buildings
  const buildings: [number, number, number, number, number, number, number, boolean][] = [
    // cx, cz, w, h, d, sideTex, roofTex, pyramid
    [-40, 20, 40, 24, 40, tex.wall1, tex.roof, true],
    [50, -10, 30, 16, 30, tex.wall2, tex.roof, false],
    [20, 40, 20, 12, 20, tex.wall1, tex.roof, true],
    [-60, -30, 24, 18, 24, tex.wall2, tex.roof, false],
    [75, 45, 26, 20, 26, tex.wall1, tex.roof, true],
  ];
  for (const [cx, cz, w, h, d, sideTex, roofTex, pyramid] of buildings) {
    const m: Mesh = { tris: [] };
    box(m, cx, cz, w, h, d, sideTex, roofTex);
    if (pyramid) {
      pyramidRoof(m, cx, cz, w, h, d, h / 2, roofTex);
    }
    meshes.push(m);
    footprints.push({ x: cx - w / 2, z: cz - d / 2, w, d });
  }

  // downloaded models
  const tree = await loadObj('/models/tree.obj', engine.textures, tex.tree);
  const spider = await loadObj('/models/spider.obj', engine.textures);
  const wuson = await loadObj('/models/WusonOBJ.obj', engine.textures);
  const backpack = await loadObj('/models/backpack.obj', engine.textures);
  const trees: [number, number, number][] = [
    // x, z, rotation
    [-10, 10, 0.5],
    [30, -30, 2.2],
    [-35, -15, 4.1],
    [10, -50, 1.2],
  ];

  for (const [tx, tz, rot] of trees) {
    const m = cloneMesh(tree);
    // tree.obj is in centimetres; scale to ~8 units tall
    transformMesh(m, 7, 7, 7, rot, tx, 0, tz);
    shadeMesh(m, -0.4, 0.8, 0.35);
    meshes.push(m);
    markers.push({ x: tx, z: tz, label: 'tree' });
  }

  const sp = cloneMesh(spider);
  // spider.obj is in centimetres; scale to ~8 units tall
  transformMesh(sp, 0.1, 0.1, 0.1, 0.8, 15, 0, 5);
  shadeMesh(sp, -0.4, 0.8, 0.35);
  meshes.push(sp);
  markers.push({ x: 15, z: 5, label: 'spider' });

  const wu = cloneMesh(wuson);
  transformMesh(wu, 6.6, 6.6, 6.6, 2.356, -25, 0, 25);
  shadeMesh(wu, -0.4, 0.8, 0.35);
  meshes.push(wu);
  markers.push({ x: -25, z: 25, label: 'wuson' });

  // the complex downloaded model, displayed on a pedestal in the square
  const ped: Mesh = { tris: [] };
  box(ped, 0, 0, 6, 1.2, 6, tex.wall2, tex.roof);
  shadeMesh(ped, -0.4, 0.8, 0.35);
  meshes.push(ped);
  const bp = cloneMesh(backpack);
  transformMesh(bp, 3, 3, 3, 0.6, 0, 1.2, 0);
  shadeMesh(bp, -0.4, 0.8, 0.35);
  meshes.push(bp);
  markers.push({ x: 0, z: 0, label: 'backpack' });

  return { meshes, footprints, markers };
}

function cloneMesh(src: Mesh): Mesh {
  return { tris: src.tris.map((t) => ({ ...t, a: { ...t.a }, b: { ...t.b }, c: { ...t.c } })) };
}
