//! 3drend — vector-projection 3D engine rendered with WebGPU (wgpu crate).
//! The core (world, camera, renderer) is platform-agnostic; frontends:
//! - wasm: this module's wasm-bindgen glue + a thin JS shell
//! - native: `src/main.rs` (winit) via the `native` cargo feature

pub mod app;
pub mod math;
pub mod obj;
pub mod renderer;
pub mod world;

use crate::app::{Camera, KeyState};
use crate::obj::Texture;
use crate::renderer::{Renderer, SurfaceFactory};
use crate::world::{build_world, Footprint, Marker, ModelAssets, World};
use serde::Deserialize;
use wasm_bindgen::prelude::*;

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

/// The whole app: camera, input state, renderer, and the minimap data.
/// Shared by both frontends.
#[wasm_bindgen]
pub struct App {
    renderer: Renderer,
    cam: Camera,
    keys: KeyState,
    proj: math::Mat4,
    view_proj: math::Mat4,
    footprints: Vec<Footprint>,
    markers: Vec<Marker>,
}

impl App {
    /// Platform-agnostic constructor from a ready renderer + world.
    pub fn from_parts(renderer: Renderer, world: World) -> App {
        let (w, h) = renderer.dims();
        App {
            cam: Camera::default(),
            keys: KeyState::default(),
            proj: math::Mat4::perspective(70.0_f32.to_radians(), w as f32 / h as f32, 0.05, 1000.0),
            view_proj: math::Mat4([0.0; 16]),
            footprints: world.footprints,
            markers: world.markers,
            renderer,
        }
    }
}

// Surface factory for the wasm canvas. The same factory is reused if the
// surface is ever lost and must be recreated.
#[cfg(target_arch = "wasm32")]
fn canvas_surface_factory(canvas: web_sys::HtmlCanvasElement) -> SurfaceFactory {
    Box::new(move |instance| {
        instance
            .create_surface(wgpu::SurfaceTarget::Canvas(canvas.clone()))
            .map_err(|e| e.to_string())
    })
}
#[cfg(not(target_arch = "wasm32"))]
fn canvas_surface_factory(_canvas: web_sys::HtmlCanvasElement) -> SurfaceFactory {
    Box::new(|_| Err(String::from("wasm-only surface creation")))
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

        let width = canvas.width().max(1);
        let height = canvas.height().max(1);
        let renderer = Renderer::new(
            canvas_surface_factory(canvas),
            wgpu::PresentMode::Fifo,
            width,
            height,
            &textures,
            &world.meshes,
        )
        .await
        .map_err(|e| JsValue::from_str(&e))?;
        Ok(App::from_parts(renderer, world))
    }

    /// Set the state of one key (called from JS on keydown/keyup).
    pub fn key(&mut self, code: &str, down: bool) {
        self.keys.set(code, down);
    }

    /// Resize the render surface in physical pixels and update projection.
    pub fn resize(&mut self, width: u32, height: u32) {
        if self.renderer.resize(width, height) {
            self.proj = math::Mat4::perspective(
                70.0_f32.to_radians(),
                width as f32 / height as f32,
                0.05,
                1000.0,
            );
        }
    }

    /// Advance the simulation and render one frame.
    pub fn tick(&mut self, dt: f32) {
        self.cam.step(&self.keys, dt);
        let view = self.cam.view();
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
        self.renderer.read_frame().await.map_err(|e| JsValue::from_str(&e))
    }
}
