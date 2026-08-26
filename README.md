# 3drend

A polygon-projection 3D engine written in Rust, with a Doom-style game on top:
a grid maze with sliding doors, full 3D-model enemies, and three hitscan guns.
Vertices are projected, triangles rasterized with WebGPU/wgpu, fragments
depth-tested. No raycasting, no ray tracing.

One engine implementation in Rust (`src/`); two thin frontends use it:

- **HTML/WebGPU:** Rust compiled to WASM with wasm-bindgen. `public/js/main.js`
  only fetches/decodes assets, forwards pointer-lock/keyboard input, drives
  `requestAnimationFrame`, and resizes the canvas. No game or rendering logic.
- **Native/winit:** `src/main.rs` loads the same assets from disk and runs the
  same `Game`, camera, and renderer with mouse look and keyboard input.

## Controls

- `W`/`S` forward/back, `A`/`D` strafe
- Mouse (pointer-lock in the browser, captured cursor natively) look
- Click / `Space` fire
- Right-click / `E` open or close the door ahead
- `1`/`2`/`3` select pistol / SMG / shotgun

## Browser frontend

```sh
rustup target add wasm32-unknown-unknown
cargo binstall wasm-pack   # or: cargo install wasm-pack
./build.sh
node serve.mjs
```

Open <http://localhost:3000>. Generated wasm-bindgen output goes to
`public/pkg/` and is not committed.

## Native winit frontend

```sh
cargo run --features native
cargo run --release --features native -- --uncapped
cargo run --release --features native -- --frames 60 --dump frame.rgba   # render N frames, dump RGBA, exit
```

`DREND_ASSETS` overrides the asset directory (default `./public`).

## Architecture

| Path | Responsibility |
|---|---|
| `src/game.rs` | Doom game: maze, doors, enemies, guns, hitscan, collision |
| `src/app.rs` | Shared camera and input state |
| `src/math.rs` | Column-major camera and projection matrices |
| `src/obj.rs` | OBJ/MTL parsing and texture/mesh types |
| `src/world.rs` | Mesh-construction primitives (box, quad, shading, transform) |
| `src/renderer.rs` | wgpu pipeline, per-frame meshes, HUD overlay, resize, surface recovery |
| `src/lib.rs` | wasm-bindgen adapter and shared `App` |
| `src/main.rs` | winit window, native asset I/O, mouse/keyboard input |
| `public/js/main.js` | Browser-only I/O/input/rAF adapter |

## Verification

```sh
cargo test
cargo check --features native
cargo check --target wasm32-unknown-unknown
```

The browser and native frontends share one game simulation and renderer; the
native `--dump` mode writes the frame for offline checks.

## macOS 26 wgpu patch

macOS 26 reports `NSWindow.occlusionState` with values that do not match wgpu's
legacy visible-bit check, causing every native frame acquisition to return
`Occluded`. `vendor/wgpu-hal` contains a one-line patch that checks
`NSWindow.isVisible` instead; Cargo applies it through `[patch.crates-io]`.
Remove it when upstream resolves [wgpu #9430](https://github.com/gfx-rs/wgpu/issues/9430).
