//! 3drend — vector-projection 3D engine rendered with WebGPU (wgpu crate,
//! compiled to WASM). The Rust core owns the world, camera, and renderer;
//! a thin JS glue supplies decoded assets, input, and the frame loop.

mod math;
mod obj;
mod renderer;
mod world;

use crate::obj::Texture;
use crate::renderer::Renderer;
use crate::world::{build_world, Footprint, Marker, ModelAssets};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

const VIEW_W: u32 = 640;
const VIEW_H: u32 = 360;
const MOVE_SPEED: f32 = 40.0; // world units / second
const TURN_SPEED: f32 = 1.6; // radians / second

/// RGBA image bytes. Deserialized through `deserialize_any` so
/// serde-wasm-bindgen uses its fast `byte_buf` copy path; plain `Vec<u8>`
/// would go through per-element sequence access (seconds for large images).
#[derive(Clone)]
struct Rgba(Vec<u8>);

impl<'de> serde::Deserialize<'de> for Rgba {
    fn deserialize<D>(d: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct V;
        impl<'de> serde::de::Visitor<'de> for V {
            type Value = Rgba;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a byte buffer")
            }
            fn visit_byte_buf<E: serde::de::Error>(self, v: Vec<u8>) -> Result<Rgba, E> {
                Ok(Rgba(v))
            }
        }
        d.deserialize_any(V)
    }
}

// Assets decoded by the JS glue, in the order it fetches them.
#[derive(Deserialize)]
struct TexIn {
    w: u32,
    h: u32,
    data: Rgba, // RGBA
}

#[derive(Deserialize)]
struct ImageIn {
    file: String, // raw map_Kd value from the MTL, e.g. `.\tex.jpg`
    w: u32,
    h: u32,
    data: Rgba, // RGBA
}

#[derive(Deserialize)]
struct ModelIn {
    obj: String,
    #[serde(default)]
    mtl: Option<String>,
    #[serde(default)]
    images: Vec<ImageIn>,
    #[serde(default)]
    fallback: i32,
}

#[derive(Deserialize)]
struct Assets {
    world: Vec<TexIn>,
    tree: ModelIn,
    spider: ModelIn,
    wuson: ModelIn,
    backpack: ModelIn,
}

#[derive(Clone, Copy, Default)]
struct Keys {
    w: bool,
    s: bool,
    a: bool,
    d: bool,
    left: bool,
    right: bool,
    up: bool,
    down: bool,
}

#[derive(Clone, Copy, Serialize)]
struct Camera {
    x: f32,
    y: f32,
    z: f32,
    yaw: f32,
    pitch: f32,
}

/// The whole app: camera, input state, renderer, and the minimap data.
#[wasm_bindgen]
pub struct App {
    renderer: Renderer,
    cam: Camera,
    keys: Keys,
    proj: math::Mat4,
    view_proj: math::Mat4,
    footprints: Vec<Footprint>,
    markers: Vec<Marker>,
}

#[wasm_bindgen]
impl App {
    /// Create the renderer and build the world from JS-decoded assets.
    #[wasm_bindgen(js_name = create)]
    pub async fn create(canvas: web_sys::HtmlCanvasElement, assets: JsValue) -> Result<App, JsValue> {
        // Move the decoded image bytes straight into Rust textures: zero
        // copies after the serde transfer.
        let Assets { world, tree, spider, wuson, backpack } = serde_wasm_bindgen::from_value(assets)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        let mut textures: Vec<Texture> = world
            .into_iter()
            .map(|t| Texture { w: t.w, h: t.h, data: t.data.0 })
            .collect();
        let mk = |m: ModelIn| ModelAssets {
            obj_text: m.obj,
            mtl_text: m.mtl,
            images: m.images.into_iter().map(|i| (i.file, Texture { w: i.w, h: i.h, data: i.data.0 })).collect(),
            fallback: m.fallback,
        };
        let world = build_world(&mut textures, mk(tree), mk(spider), mk(wuson), mk(backpack));

        let renderer = Renderer::new(canvas, VIEW_W, VIEW_H, &textures, &world.meshes).await?;
        Ok(App {
            cam: Camera { x: 0.0, y: 30.0, z: -70.0, yaw: 0.0, pitch: -0.4 },
            keys: Keys::default(),
            proj: math::Mat4::perspective(70.0_f32.to_radians(), VIEW_W as f32 / VIEW_H as f32, 0.05, 1000.0),
            view_proj: math::Mat4([0.0; 16]),
            footprints: world.footprints,
            markers: world.markers,
            renderer,
        })
    }

    /// Set the state of one key (called from JS on keydown/keyup).
    pub fn key(&mut self, code: &str, down: bool) {
        let k = &mut self.keys;
        match code {
            "w" => k.w = down,
            "s" => k.s = down,
            "a" => k.a = down,
            "d" => k.d = down,
            "ArrowLeft" => k.left = down,
            "ArrowRight" => k.right = down,
            "ArrowUp" => k.up = down,
            "ArrowDown" => k.down = down,
            _ => {}
        }
    }

    /// Advance the simulation and render one frame.
    pub fn tick(&mut self, dt: f32) {
        let dt = dt.min(0.05);
        // movement: forward = (sin yaw, 0, cos yaw)
        let (fx, fz) = (self.cam.yaw.sin(), self.cam.yaw.cos());
        let (rx, rz) = (self.cam.yaw.cos(), -self.cam.yaw.sin());
        let (mut mx, mut mz) = (0.0f32, 0.0f32);
        if self.keys.w {
            mx += fx;
            mz += fz;
        }
        if self.keys.s {
            mx -= fx;
            mz -= fz;
        }
        if self.keys.a {
            mx -= rx;
            mz -= rz;
        }
        if self.keys.d {
            mx += rx;
            mz += rz;
        }
        let mlen = (mx * mx + mz * mz).sqrt();
        if mlen > 0.0 {
            self.cam.x += (mx / mlen) * MOVE_SPEED * dt;
            self.cam.z += (mz / mlen) * MOVE_SPEED * dt;
        }
        if self.keys.left {
            self.cam.yaw -= TURN_SPEED * dt;
        }
        if self.keys.right {
            self.cam.yaw += TURN_SPEED * dt;
        }
        if self.keys.up {
            self.cam.pitch = (self.cam.pitch + TURN_SPEED * 0.5 * dt).min(1.5);
        }
        if self.keys.down {
            self.cam.pitch = (self.cam.pitch - TURN_SPEED * 0.5 * dt).max(-1.5);
        }

        let view = math::Mat4::view(self.cam.yaw, self.cam.pitch, [self.cam.x, self.cam.y, self.cam.z]);
        self.view_proj = self.proj.multiply(&view);
        self.renderer.render(&self.view_proj);
    }

    pub fn cam(&self) -> JsValue {
        serde_wasm_bindgen::to_value(&self.cam).unwrap()
    }

    pub fn footprints(&self) -> JsValue {
        serde_wasm_bindgen::to_value(&self.footprints).unwrap()
    }

    pub fn markers(&self) -> JsValue {
        serde_wasm_bindgen::to_value(&self.markers).unwrap()
    }

    /// Render the current frame offscreen and read back RGBA bytes
    /// (verification seam).
    #[wasm_bindgen(js_name = readFrame)]
    pub async fn read_frame(&mut self) -> Result<Vec<u8>, JsValue> {
        self.renderer.read_frame().await
    }
}

