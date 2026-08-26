//! World assembly: ground plane, terrain, buildings, and downloaded models
//! placed in the scene (port of the TS `world.ts`).

use crate::obj::{load_obj, Mesh, Texture, Tri, Vertex};
use serde::Serialize;

#[derive(Clone, Copy, Serialize)]
pub struct Footprint {
    pub x: f32,
    pub z: f32,
    pub w: f32,
    pub d: f32,
}

#[derive(Clone, Serialize)]
pub struct Marker {
    pub x: f32,
    pub z: f32,
    pub label: String,
}

pub struct World {
    pub meshes: Vec<Mesh>,
    pub footprints: Vec<Footprint>,
    pub markers: Vec<Marker>,
}

/// Raw assets for one downloaded model, decoded by the JS glue.
#[derive(Clone)]
pub struct ModelAssets {
    pub obj_text: String,
    pub mtl_text: Option<String>,
    /// (raw `map_Kd` file name, decoded image)
    pub images: Vec<(String, Texture)>,
    pub fallback: i32,
}

fn vert(x: f32, y: f32, z: f32, u: f32, v: f32) -> Vertex {
    Vertex { x, y, z, u, v }
}

// Counter-clockwise quad when viewed from outside, split into two tris.
fn quad(mesh: &mut Mesh, tex: u32, a: Vertex, b: Vertex, c: Vertex, d: Vertex) {
    mesh.tris.push(Tri { a, b, c, tex: tex as i32, color: 0, shade: 1.0 });
    mesh.tris.push(Tri { a, b: c, c: d, tex: tex as i32, color: 0, shade: 1.0 });
}

// Box with base at y=0. Sides textured with side_tex (v=1 bottom, v=0 top),
// top with top_tex. All faces wound outward.
fn box_mesh(mesh: &mut Mesh, cx: f32, cz: f32, w: f32, h: f32, d: f32, side_tex: u32, top_tex: u32) {
    let (x0, x1) = (cx - w / 2.0, cx + w / 2.0);
    let (z0, z1) = (cz - d / 2.0, cz + d / 2.0);
    // +Z face
    quad(mesh, side_tex, vert(x0, 0.0, z1, 0.0, 1.0), vert(x1, 0.0, z1, 1.0, 1.0), vert(x1, h, z1, 1.0, 0.0), vert(x0, h, z1, 0.0, 0.0));
    // -Z face
    quad(mesh, side_tex, vert(x1, 0.0, z0, 0.0, 1.0), vert(x0, 0.0, z0, 1.0, 1.0), vert(x0, h, z0, 1.0, 0.0), vert(x1, h, z0, 0.0, 0.0));
    // +X face
    quad(mesh, side_tex, vert(x1, 0.0, z1, 0.0, 1.0), vert(x1, 0.0, z0, 1.0, 1.0), vert(x1, h, z0, 1.0, 0.0), vert(x1, h, z1, 0.0, 0.0));
    // -X face
    quad(mesh, side_tex, vert(x0, 0.0, z0, 0.0, 1.0), vert(x0, 0.0, z1, 1.0, 1.0), vert(x0, h, z1, 1.0, 0.0), vert(x0, h, z0, 0.0, 0.0));
    // top (+Y)
    quad(mesh, top_tex, vert(x0, h, z1, 0.0, 1.0), vert(x1, h, z1, 1.0, 1.0), vert(x1, h, z0, 1.0, 0.0), vert(x0, h, z0, 0.0, 0.0));
    // bottom (-Y)
    quad(mesh, side_tex, vert(x0, 0.0, z0, 0.0, 0.0), vert(x1, 0.0, z0, 1.0, 0.0), vert(x1, 0.0, z1, 1.0, 1.0), vert(x0, 0.0, z1, 0.0, 1.0));
}

// Pyramid roof over the box footprint, apex rh above the walls.
fn pyramid_roof(mesh: &mut Mesh, cx: f32, cz: f32, w: f32, h: f32, d: f32, rh: f32, tex: u32) {
    let (x0, x1) = (cx - w / 2.0, cx + w / 2.0);
    let (z0, z1) = (cz - d / 2.0, cz + d / 2.0);
    let apex = vert(cx, h + rh, cz, 0.5, 0.5);
    let tex = tex as i32;
    for (a, b) in [
        (vert(x0, h, z1, 0.0, 1.0), vert(x1, h, z1, 1.0, 1.0)),
        (vert(x1, h, z1, 0.0, 1.0), vert(x1, h, z0, 1.0, 1.0)),
        (vert(x1, h, z0, 0.0, 1.0), vert(x0, h, z0, 1.0, 1.0)),
        (vert(x0, h, z0, 0.0, 1.0), vert(x0, h, z1, 1.0, 1.0)),
    ] {
        mesh.tris.push(Tri { a, b, c: apex, tex, color: 0, shade: 1.0 });
    }
}

// The world-map texture spans the whole plane.
fn ground(mesh: &mut Mesh, tex: u32, size: f32, cells: u32) {
    let cell = size / cells as f32;
    let off = size / 2.0;
    for i in 0..cells {
        for j in 0..cells {
            let (x0, x1) = (-off + i as f32 * cell, -off + (i + 1) as f32 * cell);
            let (z0, z1) = (-off + j as f32 * cell, -off + (j + 1) as f32 * cell);
            let (u0, u1) = (i as f32 / cells as f32, (i + 1) as f32 / cells as f32);
            let (v0, v1) = (j as f32 / cells as f32, (j + 1) as f32 / cells as f32);
            quad(mesh, tex, vert(x0, 0.0, z0, u0, v0), vert(x1, 0.0, z0, u1, v0), vert(x1, 0.0, z1, u1, v1), vert(x0, 0.0, z1, u0, v1));
        }
    }
}

// Deterministic heightfield terrain (rolling hills), same LCG as the TS
// engine (float64 arithmetic, so the shape is bit-identical).
fn terrain(mesh: &mut Mesh, tex: u32, cx: f32, cz: f32, size: f32, cells: u32, amp: f32, seed: u32) {
    let mut s: u32 = seed;
    let mut rnd = || -> f64 {
        s = ((s as f64 * 1103515245.0 + 12345.0) as i64 as u32) & 0x7fffffff;
        s as f64 / 0x7fffffff as f64
    };
    let n = cells + 1;
    let mut raw = vec![vec![0.0f64; n as usize]; n as usize];
    for row in &mut raw {
        for v in row {
            *v = rnd();
        }
    }
    let h = |i: i64, j: i64| -> f64 {
        let (ix, jy) = (i.clamp(0, n as i64 - 1), j.clamp(0, n as i64 - 1));
        raw[ix as usize][jy as usize]
    };
    let cell = size as f64 / cells as f64;
    let off = size as f64 / 2.0;
    for i in 0..cells {
        for j in 0..cells {
            let (x0, x1) = (cx as f64 - off + i as f64 * cell, cx as f64 - off + (i + 1) as f64 * cell);
            let (z0, z1) = (cz as f64 - off + j as f64 * cell, cz as f64 - off + (j + 1) as f64 * cell);
            let y00 = amp as f64 * (h(i as i64, j as i64) * 0.5 + h(i as i64 - 1, j as i64) * 0.25 + h(i as i64, j as i64 - 1) * 0.25);
            let y10 = amp as f64 * (h(i as i64 + 1, j as i64) * 0.5 + h(i as i64, j as i64) * 0.25 + h(i as i64 + 1, j as i64 - 1) * 0.25);
            let y11 = amp as f64 * (h(i as i64 + 1, j as i64 + 1) * 0.5 + h(i as i64, j as i64 + 1) * 0.25 + h(i as i64 + 1, j as i64) * 0.25);
            let y01 = amp as f64 * (h(i as i64, j as i64 + 1) * 0.5 + h(i as i64 - 1, j as i64 + 1) * 0.25 + h(i as i64, j as i64) * 0.25);
            let (u0, u1) = (i as f64 / 4.0, (i + 1) as f64 / 4.0);
            let (v0, v1) = (j as f64 / 4.0, (j + 1) as f64 / 4.0);
            quad(
                mesh, tex,
                vert(x0 as f32, y00 as f32, z0 as f32, u0 as f32, v0 as f32),
                vert(x1 as f32, y10 as f32, z0 as f32, u1 as f32, v0 as f32),
                vert(x1 as f32, y11 as f32, z1 as f32, u1 as f32, v1 as f32),
                vert(x0 as f32, y01 as f32, z1 as f32, u0 as f32, v1 as f32),
            );
        }
    }
}

/// Flat shading from a fixed light direction, per face.
pub fn shade_mesh(mesh: &mut Mesh, lx: f32, ly: f32, lz: f32) {
    let len = (lx * lx + ly * ly + lz * lz).sqrt();
    let (lx, ly, lz) = (lx / len, ly / len, lz / len);
    for t in &mut mesh.tris {
        let (abx, aby, abz) = (t.b.x - t.a.x, t.b.y - t.a.y, t.b.z - t.a.z);
        let (acx, acy, acz) = (t.c.x - t.a.x, t.c.y - t.a.y, t.c.z - t.a.z);
        let (nx, ny, nz) = (aby * acz - abz * acy, abz * acx - abx * acz, abx * acy - aby * acx);
        let nl = (nx * nx + ny * ny + nz * nz).sqrt();
        if nl == 0.0 {
            t.shade = 1.0;
            continue;
        }
        let d = (nx * lx + ny * ly + nz * lz) / nl;
        t.shade = (0.45 + 0.55 * d).clamp(0.35, 1.0);
    }
}

/// Apply scale, rotation around Y, and translation (world placement).
pub fn transform_mesh(mesh: &mut Mesh, sx: f32, sy: f32, sz: f32, rot_y: f32, tx: f32, ty: f32, tz: f32) {
    let (c, s) = (rot_y.cos(), rot_y.sin());
    for t in &mut mesh.tris {
        for p in [&mut t.a, &mut t.b, &mut t.c] {
            let (x, y, z) = (p.x * sx, p.y * sy, p.z * sz);
            p.x = x * c + z * s + tx;
            p.y = y + ty;
            p.z = -x * s + z * c + tz;
        }
    }
}

/// Fixed light direction (normalized) for flat shading.
const LIGHT: [f32; 3] = [-0.4, 0.8, 0.35];

/// Clone a model mesh, place it in the world, shade it, and record a marker.
fn place_model(
    meshes: &mut Vec<Mesh>,
    markers: &mut Vec<Marker>,
    src: &Mesh,
    scale: [f32; 3],
    rot_y: f32,
    pos: [f32; 3],
    label: &str,
) {
    let mut m = src.clone();
    transform_mesh(&mut m, scale[0], scale[1], scale[2], rot_y, pos[0], pos[1], pos[2]);
    shade_mesh(&mut m, LIGHT[0], LIGHT[1], LIGHT[2]);
    meshes.push(m);
    markers.push(Marker { x: pos[0], z: pos[2], label: label.to_string() });
}


/// Assemble the fixed scene. Texture indices: 0 worldmap, 1 grasslight,
/// 2 wall1, 3 wall2, 4 roof, 5 tree (the order the JS glue uploads them).
/// Models are loaded in the same order the TS engine loaded them so texture
/// indices from `map_Kd` images stay stable.
pub fn build_world(
    textures: &mut Vec<Texture>,
    tree: ModelAssets,
    spider: ModelAssets,
    wuson: ModelAssets,
    backpack: ModelAssets,
) -> World {
    let mut meshes: Vec<Mesh> = Vec::new();
    let mut footprints = Vec::new();
    let mut markers = Vec::new();

    let mut g = Mesh::default();
    ground(&mut g, 0, 200.0, 20);
    meshes.push(g);

    // grass hills from the heightfield map (north-west area)
    let mut hill = Mesh::default();
    terrain(&mut hill, 1, -80.0, 80.0, 70.0, 12, 6.0, 7);
    shade_mesh(&mut hill, LIGHT[0], LIGHT[1], LIGHT[2]);
    meshes.push(hill);

    // buildings: cx, cz, w, h, d, sideTex, roofTex, pyramid
    for (cx, cz, w, h, d, side, roof, pyramid) in [
        (-40.0, 20.0, 40.0, 24.0, 40.0, 2u32, 4u32, true),
        (50.0, -10.0, 30.0, 16.0, 30.0, 3, 4, false),
        (20.0, 40.0, 20.0, 12.0, 20.0, 2, 4, true),
        (-60.0, -30.0, 24.0, 18.0, 24.0, 3, 4, false),
        (75.0, 45.0, 26.0, 20.0, 26.0, 2, 4, true),
    ] {
        let mut m = Mesh::default();
        box_mesh(&mut m, cx, cz, w, h, d, side, roof);
        if pyramid {
            pyramid_roof(&mut m, cx, cz, w, h, d, h / 2.0, roof);
        }
        meshes.push(m);
        footprints.push(Footprint { x: cx - w / 2.0, z: cz - d / 2.0, w, d });
    }

    // downloaded models
    let tree_mesh = load_obj(&tree.obj_text, tree.mtl_text.as_deref(), tree.images, textures, tree.fallback);
    let spider_mesh = load_obj(&spider.obj_text, spider.mtl_text.as_deref(), spider.images, textures, spider.fallback);
    let wuson_mesh = load_obj(&wuson.obj_text, wuson.mtl_text.as_deref(), wuson.images, textures, wuson.fallback);
    let backpack_mesh = load_obj(&backpack.obj_text, backpack.mtl_text.as_deref(), backpack.images, textures, backpack.fallback);

    for (tx, tz, rot) in [(-10.0, 10.0, 0.5f32), (30.0, -30.0, 2.2), (-35.0, -15.0, 4.1), (10.0, -50.0, 1.2)] {
        place_model(&mut meshes, &mut markers, &tree_mesh, [7.0; 3], rot, [tx, 0.0, tz], "tree");
    }
    place_model(&mut meshes, &mut markers, &spider_mesh, [0.1; 3], 0.8, [15.0, 0.0, 5.0], "spider");
    place_model(&mut meshes, &mut markers, &wuson_mesh, [6.6; 3], 2.356, [-25.0, 0.0, 25.0], "wuson");

    // the complex downloaded model, displayed on a pedestal in the square
    let mut ped = Mesh::default();
    box_mesh(&mut ped, 0.0, 0.0, 6.0, 1.2, 6.0, 3, 4);
    shade_mesh(&mut ped, LIGHT[0], LIGHT[1], LIGHT[2]);
    meshes.push(ped);
    place_model(&mut meshes, &mut markers, &backpack_mesh, [3.0; 3], 0.6, [0.0, 1.2, 0.0], "backpack");

    World { meshes, footprints, markers }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::obj::Tri;

    fn tri(mesh: &Mesh, i: usize) -> &Tri {
        &mesh.tris[i]
    }

    fn empty_assets() -> ModelAssets {
        ModelAssets { obj_text: String::new(), mtl_text: None, images: vec![], fallback: -1 }
    }

    #[test]
    fn world_has_expected_triangle_count() {
        let mut textures = vec![Texture { w: 1, h: 1, data: vec![0; 4] }; 6];
        let a = empty_assets();
        let w = build_world(&mut textures, a.clone(), a.clone(), a.clone(), a.clone());
        // ground 20x20x2 + terrain 12x12x2 + 5 boxes x12 + 3 pyramids x4 + pedestal 12
        assert_eq!(w.meshes.iter().map(|m| m.tris.len()).sum::<usize>(), 800 + 288 + 60 + 12 + 12);
        assert_eq!(w.footprints.len(), 5);
        assert_eq!(w.markers.len(), 7); // 4 trees + spider + wuson + backpack
    }

    #[test]
    fn building_footprint_math() {
        let mut textures = vec![Texture { w: 1, h: 1, data: vec![0; 4] }; 6];
        let a = empty_assets();
        let w = build_world(&mut textures, a.clone(), a.clone(), a.clone(), a.clone());
        let first = w.footprints[0];
        assert_eq!((first.x, first.z, first.w, first.d), (-60.0, 0.0, 40.0, 40.0));
    }

    #[test]
    fn transform_mesh_matches_ts_math() {
        let mut m = Mesh::default();
        m.tris.push(Tri { a: vert(1.0, 2.0, 3.0, 0.0, 0.0), b: vert(0.0, 0.0, 0.0, 0.0, 0.0), c: vert(0.0, 0.0, 0.0, 0.0, 0.0), tex: -1, color: 0, shade: 1.0 });
        transform_mesh(&mut m, 2.0, 2.0, 2.0, 0.0, 10.0, 1.0, 20.0);
        assert_eq!(m.tris[0].a.x, 12.0);
        assert_eq!(m.tris[0].a.y, 5.0);
        assert_eq!(m.tris[0].a.z, 26.0);
    }
}
