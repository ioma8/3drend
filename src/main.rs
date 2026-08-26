//! Native desktop frontend: winit window + the same core as the wasm app
//! (game, camera, renderer). Assets load from disk instead of fetch.
//!
//! Build/run: `cargo run --features native [-- --frames N --dump FILE]`
//! `DREND_ASSETS` overrides the asset directory (default `./public`).
//! Controls: WASD move, mouse look, click/space fire, E use, 1/2/3 weapons.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use drend::game::Game;
use drend::obj::Texture;
use drend::renderer::{Renderer, SurfaceFactory};
use drend::world::ModelAssets;
use drend::App;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{DeviceEvent, ElementState, KeyEvent, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Window, WindowId};

const VIEW_W: u32 = 640;
const VIEW_H: u32 = 360;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut max_frames = None;
    let mut dump = None;
    let mut uncapped = false;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--frames" => max_frames = args.next().and_then(|s| s.parse().ok()),
            "--dump" => dump = args.next().map(PathBuf::from),
            "--uncapped" => uncapped = true,
            _ => eprintln!("unknown argument: {a}"),
        }
    }

    let event_loop = EventLoop::new()?;
    let mut frontend = Frontend {
        window: None,
        app: None,
        last: None,
        fps_clock: Instant::now(),
        fps_count: 0,
        frames: 0,
        max_frames,
        dump,
        uncapped,
    };
    event_loop.run_app(&mut frontend)?;
    Ok(())
}

struct Frontend {
    window: Option<Arc<Window>>,
    app: Option<App>,
    last: Option<Instant>,
    fps_clock: Instant,
    fps_count: u32,
    frames: u32,
    max_frames: Option<u32>,
    dump: Option<PathBuf>,
    uncapped: bool,
}

impl Frontend {
    fn start(&mut self, event_loop: &ActiveEventLoop) -> Result<(), String> {
        if self.window.is_some() {
            return Ok(());
        }
        let attrs = winit::window::WindowAttributes::default()
            .with_title("3drend")
            .with_inner_size(PhysicalSize::new(VIEW_W, VIEW_H));
        let window = Arc::new(event_loop.create_window(attrs).map_err(|e| e.to_string())?);
        window.set_cursor_grab(CursorGrabMode::Locked).ok();
        window.set_cursor_visible(false);
        window.focus_window();
        window.set_visible(true);

        let assets = std::env::var("DREND_ASSETS").unwrap_or_else(|_| String::from("./public"));
        let (textures, spider, wuson) = load_assets(Path::new(&assets))?;
        let game = Game::new(textures, spider, wuson);

        let size = window.inner_size();
        let surface_window = window.clone();
        let create_surface: SurfaceFactory = Box::new(move |instance| {
            instance
                .create_surface(surface_window.clone())
                .map_err(|e| e.to_string())
        });
        let present = if self.uncapped { wgpu::PresentMode::Immediate } else { wgpu::PresentMode::Fifo };
        let renderer = pollster::block_on(Renderer::new(
            create_surface,
            present,
            size.width.max(1),
            size.height.max(1),
            game.textures(),
        ))?;
        self.app = Some(App::from_parts(renderer, game));
        self.last = Some(Instant::now());
        self.window = Some(window);
        eprintln!("3drend: ready (present: {present:?})");
        Ok(())
    }

    fn render_frame(&mut self, event_loop: &ActiveEventLoop) {
        let Some(app) = &mut self.app else { return };
        let now = Instant::now();
        let dt = self.last.map(|l| (now - l).as_secs_f32().min(0.05)).unwrap_or(0.0);
        self.last = Some(now);
        app.tick(dt);

        self.frames += 1;
        self.fps_count += 1;
        if self.fps_clock.elapsed().as_secs_f32() >= 1.0 {
            eprintln!("fps: {}", self.fps_count);
            self.fps_clock = Instant::now();
            self.fps_count = 0;
        }
        if let Some(max) = self.max_frames {
            if self.frames >= max {
                if let Some(path) = &self.dump {
                    let data = pollster::block_on(app.read_frame()).expect("read frame");
                    std::fs::write(path, &data).expect("write dump");
                    eprintln!("dumped {} bytes to {}", data.len(), path.display());
                }
                event_loop.exit();
            }
        }
    }
}

impl ApplicationHandler for Frontend {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if let Err(e) = self.start(event_loop) {
            eprintln!("failed to start: {e}");
            event_loop.exit();
        }
    }

    fn device_event(&mut self, _event_loop: &ActiveEventLoop, _device_id: winit::event::DeviceId, event: DeviceEvent) {
        if let DeviceEvent::MouseMotion { delta } = event {
            if let Some(app) = &mut self.app {
                app.look(delta.0 as f32, delta.1 as f32);
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::KeyboardInput { event: KeyEvent { physical_key: PhysicalKey::Code(code), state, .. }, .. } => {
                let code = match code {
                    KeyCode::KeyW => "w",
                    KeyCode::KeyS => "s",
                    KeyCode::KeyA => "a",
                    KeyCode::KeyD => "d",
                    KeyCode::ArrowLeft => "ArrowLeft",
                    KeyCode::ArrowRight => "ArrowRight",
                    KeyCode::ArrowUp => "ArrowUp",
                    KeyCode::ArrowDown => "ArrowDown",
                    KeyCode::Space => "Space",
                    KeyCode::KeyE => "e",
                    KeyCode::Digit1 => "1",
                    KeyCode::Digit2 => "2",
                    KeyCode::Digit3 => "3",
                    _ => return,
                };
                let down = state == ElementState::Pressed;
                if let Some(app) = &mut self.app {
                    app.key(code, down);
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let code = match button {
                    MouseButton::Left => "Space",
                    MouseButton::Right => "e",
                    _ => return,
                };
                let down = state == ElementState::Pressed;
                if let Some(app) = &mut self.app {
                    app.key(code, down);
                }
            }
            WindowEvent::RedrawRequested => self.render_frame(event_loop),
            WindowEvent::Resized(size) => {
                if let Some(app) = &mut self.app {
                    app.resize(size.width, size.height);
                }
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                if let (Some(app), Some(window)) = (&mut self.app, &self.window) {
                    let size = window.inner_size();
                    app.resize(size.width, size.height);
                    window.request_redraw();
                }
            }
            WindowEvent::Occluded(false) => {
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            if let Some(w) = &self.window {
                w.request_redraw();
            }
        }
    }
}

/// Load the Doom assets: 4 world textures (floor, wall, door, ceiling) plus
/// the two enemy models.
fn load_assets(dir: &Path) -> Result<(Vec<Texture>, ModelAssets, ModelAssets), String> {
    let mut textures = Vec::new();
    for (name, ext) in [("grasslight", "jpg"), ("wall1", "png"), ("wall2", "png"), ("roof", "png")] {
        textures.push(load_texture(&dir.join("textures").join(format!("{name}.{ext}")))?);
    }
    let models = dir.join("models");
    let spider = load_model(&models, "spider.obj", -1)?;
    let wuson = load_model(&models, "WusonOBJ.obj", -1)?;
    Ok((textures, spider, wuson))
}

fn load_texture(path: &Path) -> Result<Texture, String> {
    let img = image::open(path).map_err(|e| format!("{}: {e}", path.display()))?.to_rgba8();
    let (w, h) = img.dimensions();
    Ok(Texture { w, h, data: img.into_raw() })
}

/// Fetch one model like the JS glue does: obj text, optional sibling mtl,
/// and decoded images keyed by the raw `map_Kd` values.
fn load_model(dir: &Path, name: &str, fallback: i32) -> Result<ModelAssets, String> {
    let obj_text = std::fs::read_to_string(dir.join(name))
        .map_err(|e| format!("{}: {e}", dir.join(name).display()))?;
    let mtl_file = obj_text
        .lines()
        .find_map(|l| l.strip_prefix("mtllib "))
        .map(|s| s.trim().to_string());
    let mtl_text = match &mtl_file {
        Some(f) => std::fs::read_to_string(dir.join(f)).ok(),
        None => None,
    };
    let mut images = Vec::new();
    if let Some(mtl) = &mtl_text {
        for line in mtl.lines() {
            if let Some(file) = line.trim().strip_prefix("map_Kd ") {
                let file = file.trim();
                // The MTL may carry a Windows `.\` prefix; browsers normalize
                // it in URLs, the filesystem does not.
                let path = file.strip_prefix(".\\").unwrap_or(file);
                let tex = load_texture(&dir.join(path))?;
                images.push((file.to_string(), tex));
            }
        }
    }
    Ok(ModelAssets { obj_text, mtl_text, images, fallback })
}
