//! Minimal column-major 4x4 matrix math, ported 1:1 from the TS engine.
//!
//! The values are pinned by unit tests to numbers verified against the
//! previous renderers, so a regression here is caught without a GPU.

/// Column-major 4x4 matrix; element `m[col * 4 + row]`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mat4(pub [f32; 16]);

impl Mat4 {
    pub fn multiply(&self, b: &Mat4) -> Mat4 {
        let a = &self.0;
        let b = &b.0;
        let mut o = [0.0; 16];
        for c in 0..4 {
            for r in 0..4 {
                let mut s = 0.0;
                for k in 0..4 {
                    s += a[k * 4 + r] * b[c * 4 + k];
                }
                o[c * 4 + r] = s;
            }
        }
        Mat4(o)
    }

    /// Standard perspective projection (right-handed clip space).
    pub fn perspective(fov_y: f32, aspect: f32, near: f32, far: f32) -> Mat4 {
        let f = 1.0 / (fov_y / 2.0).tan();
        Mat4([
            f / aspect, 0.0, 0.0, 0.0,
            0.0, f, 0.0, 0.0,
            0.0, 0.0, far / (near - far), -1.0,
            0.0, 0.0, (far * near) / (near - far), 0.0,
        ])
    }

    /// View matrix for yaw (0 = facing +z), pitch (positive = looking up)
    /// and camera position. Matches the TS `viewMatrix`: column-major lookAt,
    /// then a pitch rotation on view space, where z points backward so the
    /// angle is negated relative to the CPU engine's depth-space pitch.
    pub fn view(yaw: f32, pitch: f32, pos: [f32; 3]) -> Mat4 {
        let (cy, sy) = (yaw.cos(), yaw.sin());
        let f = [sy, 0.0, cy];
        let r = [cy, 0.0, -sy];
        let u = [0.0, 1.0, 0.0];
        let m = Mat4([
            r[0], u[0], -f[0], 0.0,
            r[1], u[1], -f[1], 0.0,
            r[2], u[2], -f[2], 0.0,
            -(r[0] * pos[0] + r[1] * pos[1] + r[2] * pos[2]),
            -(u[0] * pos[0] + u[1] * pos[1] + u[2] * pos[2]),
            f[0] * pos[0] + f[1] * pos[1] + f[2] * pos[2],
            1.0,
        ]);
        let (c, s) = (pitch.cos(), -pitch.sin());
        let rx = Mat4([
            1.0, 0.0, 0.0, 0.0,
            0.0, c, s, 0.0,
            0.0, -s, c, 0.0,
            0.0, 0.0, 0.0, 1.0,
        ]);
        rx.multiply(&m)
    }
}

#[cfg(test)]
mod tests {
    use super::Mat4;

    const W: f32 = 640.0;
    const H: f32 = 360.0;

    /// Project a world point through the full view-projection pipeline the
    /// renderer uses. Reference values were verified in a browser against the
    /// previous TS renderer (default camera: yaw 0, pitch -0.4, (0,30,-70)).
    fn project(world: [f32; 3]) -> (f32, f32) {
        let vp = Mat4::perspective(70.0_f32.to_radians(), W / H, 0.05, 1000.0)
            .multiply(&Mat4::view(0.0, -0.4, [0.0, 30.0, -70.0]));
        let x = world[0];
        let y = world[1];
        let z = world[2];
        let cx = vp.0[0] * x + vp.0[4] * y + vp.0[8] * z + vp.0[12];
        let cy = vp.0[1] * x + vp.0[5] * y + vp.0[9] * z + vp.0[13];
        let cw = vp.0[3] * x + vp.0[7] * y + vp.0[11] * z + vp.0[15];
        (((cx / cw + 1.0) / 2.0 * W).round(), ((1.0 - cy / cw) / 2.0 * H).round())
    }

    #[test]
    fn building_projects_where_the_previous_engine_rendered_it() {
        // Wall-center vertex of the -40,20 building at the default camera.
        // Reference verified in node against the previous renderers' math
        // (and the brick probe pixel (201,104) lies inside this wall).
        let (sx, sy) = project([-40.0, 12.0, 20.0]);
        assert_eq!((sx, sy), (206.0, 127.0));
    }

    #[test]
    fn camera_position_maps_to_origin() {
        let v = Mat4::view(0.0, -0.4, [0.0, 30.0, -70.0]);
        let p = [
            v.0[0] * 0.0 + v.0[4] * 30.0 + v.0[8] * -70.0 + v.0[12],
            v.0[1] * 0.0 + v.0[5] * 30.0 + v.0[9] * -70.0 + v.0[13],
            v.0[2] * 0.0 + v.0[6] * 30.0 + v.0[10] * -70.0 + v.0[14],
        ];
        assert!(p.iter().all(|v| v.abs() < 1e-4), "cam maps to {p:?}");
    }
}
