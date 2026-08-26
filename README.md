# 3drend

A polygon-projection 3D engine written in Rust. It projects mesh vertices, rasterizes triangles with WebGPU/wgpu, and depth-tests fragments. No raycasting or ray tracing.

The repository has one engine implementation: the Rust core in `src/`. Two thin frontends use it:

- **HTML/WebGPU:** Rust compiled to WASM with wasm-bindgen. `public/js/main.js` only fetches/decodes browser assets, forwards input, drives `requestAnimationFrame`, and draws the minimap. It contains no rendering or world logic.
- **Native/winit:** `src/main.rs` loads the same assets from disk and supplies a native wgpu surface. It runs the same `App`, camera, world, mesh parser, and renderer.

## Browser frontend

Requires Rust, the `wasm32-unknown-unknown` target, and wasm-pack.

```sh
rustup target add wasm32-unknown-unknown
cargo binstall wasm-pack   # or: cargo install wasm-pack
./build.sh
node serve.mjs
```

Open <http://localhost:3000>. Generated wasm-bindgen output goes to `public/pkg/` and is not committed.

The canvas backing store follows its displayed size and device-pixel ratio via `ResizeObserver`; projection, surface, depth, and readback resources resize together.

## Native winit frontend

```sh
cargo run --features native
cargo run --release --features native -- --uncapped
```

Optional verification mode renders a fixed number of frames, writes tightly packed RGBA, then exits:

```sh
cargo run --release --features native -- --frames 60 --dump frame.rgba
```

Set `DREND_ASSETS=/path/to/public` to override the asset root. Default: `./public`.

The native frontend handles live window and DPI changes. The renderer retries transient acquisition failures, reconfigures outdated surfaces, and recreates lost surfaces from the frontend's surface factory.

## Controls

- `W` / `S`: forward / backward
- `A` / `D`: strafe
- `←` / `→`: yaw
- `↑` / `↓`: pitch

## Architecture

| Path | Responsibility |
|---|---|
| `src/app.rs` | Shared camera and input state |
| `src/math.rs` | Column-major camera and projection matrices |
| `src/obj.rs` | OBJ/MTL parsing and texture/mesh types |
| `src/world.rs` | Ground, terrain, buildings, model placement |
| `src/renderer.rs` | Platform-independent wgpu pipeline, presentation/readback, resize, and surface recovery |
| `src/lib.rs` | wasm-bindgen adapter and shared `App` |
| `src/main.rs` | winit window, native asset I/O, event loop |
| `public/index.html` | Browser UI shell |
| `public/js/main.js` | Browser-only I/O/input/rAF/minimap adapter |

## Verification

```sh
cargo test
cargo check --features native
cargo check --target wasm32-unknown-unknown
```

The browser and native frontends have matching numeric output: default-camera brick probe `(201,104) = (178,62,46)`, with equivalent sky/brick/ocean coverage.

## macOS 26 wgpu patch

macOS 26 reports `NSWindow.occlusionState` with values that do not match wgpu's legacy visible-bit check, causing every native frame acquisition to return `Occluded`. `vendor/wgpu-hal` contains a one-line patch that checks `NSWindow.isVisible` instead; Cargo applies it through `[patch.crates-io]`. Remove the patch when upstream resolves [wgpu #9430](https://github.com/gfx-rs/wgpu/issues/9430).
