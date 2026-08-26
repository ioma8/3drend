//! Native desktop frontend: winit window + the same core as the wasm app
//! (world, camera, renderer). Assets load from disk instead of fetch.
//!
//! Build/run: `cargo run --features native [-- --frames N --dump FILE]`
//! `DREND_ASSETS` overrides the asset directory (default `./public`).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use drend::obj::Texture;
use drend::renderer::{Renderer, SurfaceFactory};
use drend::world::{build_world, ModelAssets, World};
use drend::App;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

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
    let mut frontend = Frontend { window: None, app: None, last: None, fps_clock: Instant::now(), fps_count: 0, frames: 0, max_frames, dump, uncapped };
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
        // The surface holds its own Arc<Window>, so it is 'static; the window
        // lives as long as the app.
        let attrs = winit::window::WindowAttributes::default()
            .with_title("3drend")
            .with_inner_size(PhysicalSize::new(VIEW_W, VIEW_H));
        let window = Arc::new(event_loop.create_window(attrs).map_err(|e| e.to_string())?);
        window.focus_window();
        window.set_visible(true);

        let assets = std::env::var("DREND_ASSETS").unwrap_or_else(|_| String::from("./public"));
        let (textures, world) = load_world(Path::new(&assets))?;

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
            &textures,
            &world.meshes,
        ))?;
        self.app = Some(App::from_parts(renderer, world));
        self.last = Some(Instant::now());
        self.window = Some(window);
        eprintln!("3drend: window ready (present: {present:?})");
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

/// Load the same scene the wasm glue assembles, from disk:
/// 6 world textures in index order (0..5), then the 4 models.
fn load_world(dir: &Path) -> Result<(Vec<Texture>, World), String> {
    let mut textures = Vec::new();
    for (name, ext) in [("worldmap", "jpg"), ("grasslight", "jpg"), ("wall1", "png"), ("wall2", "png"), ("roof", "png"), ("tree", "png")] {
        textures.push(load_texture(&dir.join("textures").join(format!("{name}.{ext}")))?);
    }
    let models = dir.join("models");
    let tree = load_model(&models, "tree.obj", 5)?; // tree texture index (world textures above)
    let spider = load_model(&models, "spider.obj", -1)?;
    let wuson = load_model(&models, "WusonOBJ.obj", -1)?;
    let backpack = load_model(&models, "backpack.obj", -1)?;
    let world = build_world(&mut textures, tree, spider, wuson, backpack);
    Ok((textures, world))
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
