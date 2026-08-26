//! Shared camera and input state used by every frontend (wasm + winit).
//! The movement math is one implementation both runtimes execute, so
//! behavior (speeds, clamps, yaw convention) can never drift apart.

use crate::math::Mat4;
use serde::Serialize;

pub const MOVE_SPEED: f32 = 40.0; // world units / second
pub const TURN_SPEED: f32 = 1.6; // radians / second

/// Pressed-key state, driven by the frontend's key events. Codes match the
/// JS glue ("w", "s", "a", "d", "ArrowLeft", ...) and the winit key mapping.
#[derive(Clone, Copy, Default)]
pub struct KeyState {
    pub w: bool,
    pub s: bool,
    pub a: bool,
    pub d: bool,
    pub left: bool,
    pub right: bool,
    pub up: bool,
    pub down: bool,
}

impl KeyState {
    pub fn set(&mut self, code: &str, down: bool) {
        match code {
            "w" => self.w = down,
            "s" => self.s = down,
            "a" => self.a = down,
            "d" => self.d = down,
            "ArrowLeft" => self.left = down,
            "ArrowRight" => self.right = down,
            "ArrowUp" => self.up = down,
            "ArrowDown" => self.down = down,
            _ => {}
        }
    }
}

/// First-person camera: yaw 0 faces +z, positive pitch looks up.
#[derive(Clone, Copy, Serialize)]
pub struct Camera {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub yaw: f32,
    pub pitch: f32,
}

impl Camera {
    pub fn new(x: f32, y: f32, z: f32, yaw: f32, pitch: f32) -> Camera {
        Camera { x, y, z, yaw, pitch }
    }

    /// Default spawn: above the square, looking toward the buildings.
    pub fn default() -> Camera {
        Camera::new(0.0, 30.0, -70.0, 0.0, -0.4)
    }

    /// Advance movement and turning for `dt` seconds.
    pub fn step(&mut self, keys: &KeyState, dt: f32) {
        let dt = dt.min(0.05);
        // movement: forward = (sin yaw, 0, cos yaw)
        let (fx, fz) = (self.yaw.sin(), self.yaw.cos());
        let (rx, rz) = (self.yaw.cos(), -self.yaw.sin());
        let (mut mx, mut mz) = (0.0f32, 0.0f32);
        if keys.w {
            mx += fx;
            mz += fz;
        }
        if keys.s {
            mx -= fx;
            mz -= fz;
        }
        if keys.a {
            mx -= rx;
            mz -= rz;
        }
        if keys.d {
            mx += rx;
            mz += rz;
        }
        let mlen = (mx * mx + mz * mz).sqrt();
        if mlen > 0.0 {
            self.x += (mx / mlen) * MOVE_SPEED * dt;
            self.z += (mz / mlen) * MOVE_SPEED * dt;
        }
        if keys.left {
            self.yaw -= TURN_SPEED * dt;
        }
        if keys.right {
            self.yaw += TURN_SPEED * dt;
        }
        if keys.up {
            self.pitch = (self.pitch + TURN_SPEED * 0.5 * dt).min(1.5);
        }
        if keys.down {
            self.pitch = (self.pitch - TURN_SPEED * 0.5 * dt).max(-1.5);
        }
    }

    /// View matrix for the current pose.
    pub fn view(&self) -> Mat4 {
        Mat4::view(self.yaw, self.pitch, [self.x, self.y, self.z])
    }
}

#[cfg(test)]
mod tests {
    use super::{Camera, KeyState, MOVE_SPEED, TURN_SPEED};

    fn hold(keys: &mut KeyState, code: &str) {
        keys.set(code, true);
    }

    #[test]
    fn forward_moves_at_move_speed() {
        let mut cam = Camera::default();
        let mut keys = KeyState::default();
        hold(&mut keys, "w");
        cam.step(&keys, 0.04); // below the 0.05 clamp
        assert!((cam.z - (-70.0 + MOVE_SPEED * 0.04)).abs() < 1e-4, "z = {}", cam.z);
    }

    #[test]
    fn turning_yaws_by_turn_speed() {
        let mut cam = Camera::default();
        let mut keys = KeyState::default();
        hold(&mut keys, "ArrowLeft");
        cam.step(&keys, 0.04);
        assert!((cam.yaw + TURN_SPEED * 0.04).abs() < 1e-4, "yaw = {}", cam.yaw);
    }

    #[test]
    fn dt_is_clamped() {
        let mut cam = Camera::default();
        let mut keys = KeyState::default();
        hold(&mut keys, "w");
        cam.step(&keys, 10.0);
        assert!((cam.z - (-70.0 + MOVE_SPEED * 0.05)).abs() < 1e-3);
    }
}
