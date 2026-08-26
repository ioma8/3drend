//! Reusable mesh-construction primitives shared by the game (and used to
//! import and place downloaded models).

use crate::obj::{Mesh, Texture, Tri, Vertex};

/// Raw assets for one downloaded model, decoded by the frontend.
#[derive(Clone)]
pub struct ModelAssets {
    pub obj_text: String,
    pub mtl_text: Option<String>,
    /// (raw `map_Kd` file name, decoded image)
    pub images: Vec<(String, Texture)>,
    pub fallback: i32,
}

pub fn vert(x: f32, y: f32, z: f32, u: f32, v: f32) -> Vertex {
    Vertex { x, y, z, u, v }
}

// Counter-clockwise quad when viewed from outside, split into two tris.
pub fn quad(mesh: &mut Mesh, tex: u32, a: Vertex, b: Vertex, c: Vertex, d: Vertex) {
    mesh.tris.push(Tri { a, b, c, tex: tex as i32, color: 0, shade: 1.0 });
    mesh.tris.push(Tri { a, b: c, c: d, tex: tex as i32, color: 0, shade: 1.0 });
}

// Box with base at y=0. Sides textured with side_tex (v=1 bottom, v=0 top),
// top with top_tex. All faces wound outward.
pub fn box_mesh(mesh: &mut Mesh, cx: f32, cz: f32, w: f32, h: f32, d: f32, side_tex: u32, top_tex: u32) {
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

#[cfg(test)]
mod tests {
    use super::{transform_mesh, vert, Mesh, Tri};

    #[test]
    fn transform_mesh_matches_reference_math() {
        let mut m = Mesh::default();
        m.tris.push(Tri {
            a: vert(1.0, 2.0, 3.0, 0.0, 0.0),
            b: vert(0.0, 0.0, 0.0, 0.0, 0.0),
            c: vert(0.0, 0.0, 0.0, 0.0, 0.0),
            tex: -1,
            color: 0,
            shade: 1.0,
        });
        transform_mesh(&mut m, 2.0, 2.0, 2.0, 0.0, 10.0, 1.0, 20.0);
        assert_eq!(m.tris[0].a.x, 12.0);
        assert_eq!(m.tris[0].a.y, 5.0);
        assert_eq!(m.tris[0].a.z, 26.0);
    }
}
