//! Asset types and the OBJ + MTL parser, ported from the TS loader.
//! Textures are decoded in JS (canvas/ImageData); Rust only parses text
//! geometry and material assignments.

use std::collections::HashMap;

/// RGBA image bytes, row 0 = image top.
#[derive(Clone, Debug)]
pub struct Texture {
    pub w: u32,
    pub h: u32,
    pub data: Vec<u8>,
}

#[derive(Clone, Copy, Debug)]
pub struct Vertex {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    /// Texture u in [0, 1].
    pub u: f32,
    /// Texture v in [0, 1], 0 = texture top row.
    pub v: f32,
}

#[derive(Clone, Debug)]
pub struct Tri {
    pub a: Vertex,
    pub b: Vertex,
    pub c: Vertex,
    /// Index into the shared texture list; -1 = flat color.
    pub tex: i32,
    /// Packed 0xRRGGBB, used when tex < 0.
    pub color: u32,
    /// Precomputed face brightness 0..1.
    pub shade: f32,
}

#[derive(Clone, Debug, Default)]
pub struct Mesh {
    pub tris: Vec<Tri>,
}

struct ObjMaterial {
    tex: i32,
    color: u32,
}

/// Parse an OBJ (with optional MTL text) into triangles. `images` maps the
/// raw `map_Kd` file names (as written in the MTL, e.g. `.\wal67ar_small.jpg`)
/// to decoded textures; new textures are appended to `textures`. Materials
/// without a `map_Kd` fall back to `fallback_tex` (-1 = flat color).
///
/// Mirrors the TS `loadObj` exactly, including the quirks: the last material
/// in the MTL gets the fallback texture, earlier ones keep `-1`; OBJ v-origin
/// is bottom-left so v is flipped; faces are fan-triangulated.
pub fn load_obj(
    obj_text: &str,
    mtl_text: Option<&str>,
    images: &[(String, Texture)],
    textures: &mut Vec<Texture>,
    fallback_tex: i32,
) -> Mesh {
    let mut materials: HashMap<String, ObjMaterial> = HashMap::new();
    if let Some(mtl) = mtl_text {
        let mut name = String::new();
        let mut tex: i32 = -1;
        let mut color: u32 = 0x9c9c9c;
        for line in mtl.lines() {
            let s = line.trim();
            if let Some(rest) = s.strip_prefix("newmtl ") {
                if !name.is_empty() {
                    materials.insert(name.clone(), ObjMaterial { tex, color });
                }
                name = rest.trim().to_string();
                tex = -1;
                color = 0x9c9c9c;
            } else if let Some(file) = s.strip_prefix("map_Kd ") {
                tex = find_texture(file.trim(), images, textures);
            } else if let Some(rest) = s.strip_prefix("Kd ") {
                let p: Vec<f32> = rest.split_whitespace().filter_map(|t| t.parse().ok()).collect();
                if p.len() >= 3 {
                    color = (p[0] * 255.0).round() as u32 * 0x10000
                        | (p[1] * 255.0).round() as u32 * 0x100
                        | (p[2] * 255.0).round() as u32;
                }
            }
        }
        if !name.is_empty() {
            materials.insert(
                name,
                ObjMaterial { tex: if tex >= 0 { tex } else { fallback_tex }, color },
            );
        }
    }

    let mut vs: Vec<f32> = Vec::new();
    let mut vts: Vec<f32> = Vec::new();
    let mut faces: Vec<Vec<(i32, i32)>> = Vec::new();
    let mut usemtl: Vec<(usize, String)> = Vec::new(); // face index -> material
    for raw in obj_text.lines() {
        let line = raw.trim();
        if let Some(rest) = line.strip_prefix("v ") {
            let p: Vec<f32> = rest.split_whitespace().filter_map(|t| t.parse().ok()).collect();
            vs.extend_from_slice(&p[..3]);
        } else if let Some(rest) = line.strip_prefix("vt ") {
            let p: Vec<f32> = rest.split_whitespace().filter_map(|t| t.parse().ok()).collect();
            vts.extend_from_slice(&p[..2]);
        } else if let Some(rest) = line.strip_prefix("usemtl ") {
            usemtl.push((faces.len(), rest.trim().to_string()));
        } else if let Some(rest) = line.strip_prefix("f ") {
            let mut face = Vec::new();
            for part in rest.split_whitespace() {
                let mut idx = part.split('/');
                let vi: i32 = idx.next().and_then(|t| t.parse().ok()).unwrap_or(0);
                let ti: i32 = match idx.next() {
                    Some(t) if !t.is_empty() => t.parse().unwrap_or(-1),
                    _ => -1,
                };
                face.push((vi, ti));
            }
            if face.len() >= 3 {
                faces.push(face);
            }
        }
    }

    let default_mat = ObjMaterial { tex: -1, color: 0x9c9c9c };
    let mut mat_at: HashMap<usize, ObjMaterial> = HashMap::new();
    for (at, name) in usemtl {
        let mat = materials.get(&name).unwrap_or(&default_mat);
        mat_at.insert(at, ObjMaterial { tex: mat.tex, color: mat.color });
    }

    let v = |i: i32| -> usize {
        if i < 0 {
            (vs.len() as i64 + i as i64 * 3) as usize
        } else {
            ((i - 1) * 3) as usize
        }
    };
    let vt = |i: i32| -> Option<usize> {
        if i < 0 {
            Some((vts.len() as i64 + i as i64 * 2) as usize)
        } else {
            Some(((i - 1) * 2) as usize)
        }
    };

    let mut mesh = Mesh::default();
    for (fi, face) in faces.iter().enumerate() {
        let mat = mat_at.get(&fi).unwrap_or(&default_mat);
        let mk = |fi: usize| -> Vertex {
            let (vi, ti) = face[fi];
            let k = v(vi);
            let (u, vv) = match ti {
                t if t >= 0 => {
                    let tk = vt(t).unwrap();
                    (vts[tk], 1.0 - vts[tk + 1]) // OBJ v origin is bottom-left
                }
                _ => (0.0, 0.0),
            };
            Vertex { x: vs[k], y: vs[k + 1], z: vs[k + 2], u, v: vv }
        };
        for i in 1..face.len() - 1 {
            mesh.tris.push(Tri {
                a: mk(0),
                b: mk(i),
                c: mk(i + 1),
                tex: mat.tex,
                color: mat.color,
                shade: 1.0,
            });
        }
    }
    mesh
}

fn find_texture(file: &str, images: &[(String, Texture)], textures: &mut Vec<Texture>) -> i32 {
    for (name, tex) in images {
        if name == file {
            textures.push(tex.clone());
            return (textures.len() - 1) as i32;
        }
    }
    -1
}

#[cfg(test)]
mod tests {
    use super::{load_obj, Texture};

    #[test]
    fn parses_quads_vt_and_negative_indices() {
        let obj = "v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nvt 0 1\nvt 1 1\nvt 1 0\nvt 0 0\nf 1/1 2/2 3/3 4/4\nf -4/-4 -3/-3 -2/-2 -1/-1\n";
        let m = load_obj(obj, None, &[], &mut vec![], -1);
        assert_eq!(m.tris.len(), 4, "two quads, two tris each");
        // v flip: vt (0,1) is the bottom-left OBJ corner -> v = 0
        assert_eq!(m.tris[0].a.u, 0.0);
        assert_eq!(m.tris[0].a.v, 0.0);
        assert_eq!(m.tris[0].c.z, 0.0);
        // negative indices refer to the last vertices, same vt -> same flipped v
        assert_eq!(m.tris[3].a.u, 0.0);
        assert_eq!(m.tris[3].a.v, 0.0);
    }

    #[test]
    fn mtl_kd_and_map_kd() {
        let mtl = "newmtl mat\nKd 0.5 0.25 0.125\n";
        let obj = "v 0 0 0\nv 1 0 0\nv 0 1 0\nusemtl mat\nf 1 2 3\n";
        let m = load_obj(obj, Some(mtl), &[], &mut vec![], -1);
        assert_eq!(m.tris.len(), 1);
        assert_eq!(m.tris[0].color, 0x804020);
        assert_eq!(m.tris[0].tex, -1, "no map_Kd -> flat");
    }

    #[test]
    fn map_kd_appends_texture() {
        let mtl = "newmtl mat\nmap_Kd .\\tex.jpg\n";
        let obj = "v 0 0 0\nv 1 0 0\nv 0 1 0\nusemtl mat\nf 1 2 3\n";
        let mut textures = vec![];
        let img = Texture { w: 2, h: 2, data: vec![0; 16] };
        let m = load_obj(obj, Some(mtl), &[(String::from(".\\tex.jpg"), img)], &mut textures, -1);
        assert_eq!(textures.len(), 1, "image appended to shared list");
        assert_eq!(m.tris[0].tex, 0);
    }

    #[test]
    fn missing_mtl_falls_back_to_flat_gray() {
        let obj = "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n";
        let m = load_obj(obj, None, &[], &mut vec![], 7);
        assert_eq!(m.tris[0].tex, -1, "no material -> flat");
        assert_eq!(m.tris[0].color, 0x9c9c9c);
    }
}
