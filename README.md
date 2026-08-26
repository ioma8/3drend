# 3drend

A 3D engine built on the vector/polygon-projection method — project polygon
vertices, rasterize triangles, depth-test per pixel. No raycasting, no
raytracing.

This branch renders with WebGPU through the **wgpu crate**, compiled to WASM
with wasm-bindgen. All world building, OBJ/MTL parsing, camera math, and
rendering live in Rust (`src/`); a thin JS glue (`public/js/main.js`) handles
asset I/O, image decode, input, and the frame loop.

## Build

Requires Rust with the `wasm32-unknown-unknown` target and wasm-pack:

```sh
rustup target add wasm32-unknown-unknown
cargo binstall wasm-pack   # or: cargo install wasm-pack
./build.sh                 # wasm-pack build --target web --release --out-dir public/pkg
```

## Native (winit)

The same core, compiled to a native desktop app:

```sh
cargo run --features native                  # opens a window
cargo run --features native -- --frames 60 --dump frame.rgba   # verify: render 60 frames, dump raw RGBA, exit
```

`DREND_ASSETS` overrides the asset directory (default `./public`). Controls are the same; the numeric output matches the browser build pixel-for-pixel.

## Test

`cargo test` runs the native unit tests (matrix math pinned to values
verified against the previous renderers, OBJ/MTL parsing, world structure).

## Layout

- `src/math.rs` — column-major 4x4 matrix math (exact port of the engine's camera math)
- `src/obj.rs` — OBJ + MTL parser and asset types
- `src/world.rs` — scene assembly: ground, terrain, buildings, downloaded models
- `src/renderer.rs` — wgpu renderer: pipeline, per-texture draw groups, readback (platform-agnostic)
- `src/app.rs` — shared camera + input state (used by both frontends)
- `src/lib.rs` — wasm-bindgen glue and `App` (shared app state)
- `src/main.rs` — winit desktop frontend (`--features native`)
- `public/` — static web root: page, JS glue, models, textures, `pkg/` (build output)
