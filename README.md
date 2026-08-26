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

## Run

```sh
node serve.mjs             # static server on http://localhost:3000 (public/)
```

Open http://localhost:3000. Controls: WASD move, arrow keys turn/pitch.

## Test

`cargo test` runs the native unit tests (matrix math pinned to values
verified against the previous renderers, OBJ/MTL parsing, world structure).

## Layout

- `src/math.rs` — column-major 4x4 matrix math (exact port of the engine's camera math)
- `src/obj.rs` — OBJ + MTL parser and asset types
- `src/world.rs` — scene assembly: ground, terrain, buildings, downloaded models
- `src/renderer.rs` — wgpu renderer: pipeline, per-texture draw groups, readback
- `src/lib.rs` — wasm entry: app state, camera, input, frame loop
- `public/` — static web root: page, JS glue, models, textures, `pkg/` (build output)
