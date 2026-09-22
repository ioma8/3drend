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
    Vertex { x, y, z, nx: 0.0, ny: 0.0, nz: 0.0, u, v }
}

// Counter-clockwise quad when viewed from outside, split into two tris.
// The face normal (from the winding) is stamped onto all four corners, so
// `box_mesh` gets per-pixel-lit sides for free.
pub fn quad(mesh: &mut Mesh, tex: u32, a: Vertex, b: Vertex, c: Vertex, d: Vertex) {
    let (abx, aby, abz) = (b.x - a.x, b.y - a.y, b.z - a.z);
    let (acx, acy, acz) = (c.x - a.x, c.y - a.y, c.z - a.z);
    let (nx, ny, nz) = (aby * acz - abz * acy, abz * acx - abx * acz, abx * acy - aby * acx);
    let nl = (nx * nx + ny * ny + nz * nz).sqrt();
    let (nx, ny, nz) = if nl == 0.0 { (0.0, 0.0, 0.0) } else { (nx / nl, ny / nl, nz / nl) };
    let face = |mut v: Vertex| {
        v.nx = nx;
        v.ny = ny;
        v.nz = nz;
        v
    };
    let (a, b, c, d) = (face(a), face(b), face(c), face(d));
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

/// Apply scale, rotation around Y, and translation (world placement).
/// Normals get the same Y rotation and are renormalized (scale is uniform
/// positive, so direction only changes under rotation).
pub fn transform_mesh(mesh: &mut Mesh, sx: f32, sy: f32, sz: f32, rot_y: f32, tx: f32, ty: f32, tz: f32) {
    let (c, s) = (rot_y.cos(), rot_y.sin());
    for t in &mut mesh.tris {
        for p in [&mut t.a, &mut t.b, &mut t.c] {
            let (x, y, z) = (p.x * sx, p.y * sy, p.z * sz);
            p.x = x * c + z * s + tx;
            p.y = y + ty;
            p.z = -x * s + z * c + tz;
            let (nx, nz) = (p.nx * c + p.nz * s, -p.nx * s + p.nz * c);
            let nl = (nx * nx + p.ny * p.ny + nz * nz).sqrt();
            if nl != 0.0 {
                p.nx = nx / nl;
                p.ny = p.ny / nl;
                p.nz = nz / nl;
            }
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
