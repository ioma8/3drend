//! 3drend — polygon-projection 3D engine with a Doom-style game on top.
//! The core (game, camera, renderer) is platform-agnostic; frontends:
//! - wasm: this module's wasm-bindgen glue + a thin JS shell
//! - native: `src/main.rs` (winit) via the `native` cargo feature

pub mod app;
pub mod game;
pub mod math;
pub mod obj;
pub mod renderer;
pub mod world;

use crate::app::{KeyState, TURN_SPEED};
use crate::game::{Game, Input};
use crate::obj::Texture;
use crate::renderer::{Renderer, SurfaceFactory};
use crate::world::ModelAssets;
use serde::Deserialize;
use wasm_bindgen::prelude::*;

const MOUSE_SENS: f32 = 0.0025; // radians per pixel

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
    spider: ModelIn,
    wuson: ModelIn,
}

/// The whole app: the game simulation, input state, and the renderer.
#[wasm_bindgen]
pub struct App {
    renderer: Renderer,
    game: Game,
    keys: KeyState,
    mouse_dx: f32,
    mouse_dy: f32,
    proj: math::Mat4,
    view_proj: math::Mat4,
}

impl App {
    /// Platform-agnostic constructor from a ready renderer + game.
    pub fn from_parts(renderer: Renderer, game: Game) -> App {
        let (w, h) = renderer.dims();
        App {
            renderer,
            game,
            keys: KeyState::default(),
            mouse_dx: 0.0,
            mouse_dy: 0.0,
            proj: math::Mat4::perspective(70.0_f32.to_radians(), w as f32 / h as f32, 0.05, 1000.0),
            view_proj: math::Mat4([0.0; 16]),
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
    /// Create the renderer and the game from JS-decoded assets.
    #[wasm_bindgen(js_name = create)]
    pub async fn create(canvas: web_sys::HtmlCanvasElement, assets: JsValue) -> Result<App, JsValue> {
        let Assets { world, spider, wuson } = serde_wasm_bindgen::from_value(assets)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        let world: Vec<Texture> = world
            .into_iter()
            .map(|t| Texture { w: t.w, h: t.h, data: t.data.0 })
            .collect();
        let mk = |m: ModelIn| ModelAssets {
            obj_text: m.obj,
            mtl_text: m.mtl,
            images: m.images.into_iter().map(|i| (i.file, Texture { w: i.w, h: i.h, data: i.data.0 })).collect(),
            fallback: m.fallback,
        };
        let game = Game::new(world, mk(spider), mk(wuson));

        let width = canvas.width().max(1);
        let height = canvas.height().max(1);
        let renderer = Renderer::new(
            canvas_surface_factory(canvas),
            wgpu::PresentMode::Fifo,
            width,
            height,
            game.textures(),
        )
        .await
        .map_err(|e| JsValue::from_str(&e))?;
        Ok(App::from_parts(renderer, game))
    }

    /// Set the state of one key (called from JS on keydown/keyup).
    pub fn key(&mut self, code: &str, down: bool) {
        self.keys.set(code, down);
    }

    /// Accumulate mouse movement (radians applied on the next tick).
    pub fn look(&mut self, dx: f32, dy: f32) {
        self.mouse_dx += dx;
        self.mouse_dy += dy;
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
        let k = &self.keys;
        let forward = (k.w as i32 - k.s as i32) as f32;
        let strafe = (k.d as i32 - k.a as i32) as f32;
        let turn = ((k.right as i32 - k.left as i32) as f32) * TURN_SPEED * dt;
        let look_yaw = self.mouse_dx * MOUSE_SENS;
        let look_pitch = -self.mouse_dy * MOUSE_SENS + ((k.up as i32 - k.down as i32) as f32) * TURN_SPEED * 0.5 * dt;
        self.mouse_dx = 0.0;
        self.mouse_dy = 0.0;
        let switch = if k.digit_1 { 0 } else if k.digit_2 { 1 } else if k.digit_3 { 2 } else { -1 };

        let input = Input {
            forward,
            strafe,
            turn,
            look_yaw,
            look_pitch,
            fire: k.space,
            use_door: k.key_e,
            switch,
        };
        self.game.update(dt, &input);

        let view = self.game.camera().view();
        self.view_proj = self.proj.multiply(&view);
        let meshes = self.game.meshes();
        let hud = self.game.hud();
        self.renderer.render(&self.view_proj, &meshes, &hud);
    }

    /// Render the current frame offscreen and read back RGBA bytes
    /// (verification seam; HUD excluded).
    #[wasm_bindgen(js_name = readFrame)]
    pub async fn read_frame(&mut self) -> Result<Vec<u8>, JsValue> {
        let meshes = self.game.meshes();
        self.renderer.read_frame(&meshes).await.map_err(|e| JsValue::from_str(&e))
    }
}
