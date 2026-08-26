# 3drend

A small software 3D engine that renders a textured world in the browser using **vector polygon rendering** — no raycasting, no raytracing.

## How it renders

The same method the project has used from the start, extended to full 3D:

1. Every object is a mesh of textured triangles (polygons).
2. Triangle vertices are transformed into camera space and perspective-projected.
3. Faces are backface-culled and clipped against the near plane.
4. Polygons are depth-ordered (painter's algorithm) and rasterized scanline-by-scanline into a per-pixel z-buffer with perspective-correct textures.

No rays are cast, no lights are traced — geometry is projected and filled, exactly like the original wall renderer, just for arbitrary 3D polygons.

## Controls

| Key | Action |
| --- | --- |
| `W` / `S` | move forward / back |
| `A` / `D` | strafe |
| `←` / `→` | turn |
| `↑` / `↓` | look up / down |

## Run

```sh
npm install
npm start        # http://localhost:3000
```

Other scripts: `npm run build` (production build), `npm run lint`, `npm test`.

## The world

A 200×200 map with a downloaded world-map texture as the ground, rolling grass hills, five buildings (pyramid and flat roofs), and downloaded OBJ models placed around town: a low-poly tree, a spider, a character statue, and a 68k-face backpack on a pedestal in the square.

## Structure

- `src/engine3d.ts` — the engine: camera transform, projection, near-plane clipping, painter sort, z-buffered scanline rasterizer, OBJ/MTL loader.
- `src/world.ts` — world assembly: ground, hills, buildings, model placement.
- `src/App.tsx` — canvas, input, render loop, minimap.
- `public/models/` — downloaded OBJ models and their textures.
- `public/textures/` — ground, wall, roof, and tree textures.

## Assets

Models: `tree.obj` (three.js repo), `spider.obj` + textures (assimp test suite), `WusonOBJ.obj` (assimp test suite), `backpack.obj` + `diffuse.jpg` (LearnOpenGL). World map: NASA-style `land_ocean_ice_cloud_2048.jpg` (three.js examples); `grasslight-big.jpg` (three.js examples).
