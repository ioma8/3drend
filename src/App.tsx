import React, { useEffect, useRef } from 'react';
import './App.css';
import { Engine, loadImage, makeTexture } from './engine3d';
import { buildWorld } from './world';

const VIEW_W = 640;
const VIEW_H = 360;
const MOVE_SPEED = 40; // world units / second
const TURN_SPEED = 1.6; // radians / second

const dtr = (deg: number): number => (deg * Math.PI) / 180.0;

function App() {
  const viewRef = useRef<HTMLCanvasElement>(null);
  const miniRef = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const view = viewRef.current;
    const mini = miniRef.current;
    if (!view || !mini) return;
    view.width = VIEW_W;
    view.height = VIEW_H;
    const ctx = view.getContext('2d');
    const miniCtx = mini.getContext('2d');
    if (!ctx || !miniCtx) return;

    let disposed = false;
    let raf = 0;

    (async () => {
      const engine = new Engine(VIEW_W, VIEW_H);
      const cam = engine.camera;
      cam.x = 0;
      cam.y = 30;
      cam.z = -70;
      cam.yaw = 0;
      cam.pitch = -0.4;

      // base textures
      const texNames = ['worldmap', 'grasslight', 'wall1', 'wall2', 'roof', 'tree'] as const;
      const tex: Record<string, number> = {};
      for (const name of texNames) {
        const ext = name === 'worldmap' || name === 'grasslight' ? 'jpg' : 'png';
        engine.textures.push(makeTexture(await loadImage(`/textures/${name}.${ext}`)));
        tex[name] = engine.textures.length - 1;
      }
      const world = await buildWorld(engine, {
        worldmap: tex.worldmap,
        grasslight: tex.grasslight,
        wall1: tex.wall1,
        wall2: tex.wall2,
        roof: tex.roof,
        tree: tex.tree,
      });
      // test seam: numeric validation of the running engine
      (window as unknown as { __engine3d: unknown }).__engine3d = { engine, world };

      const keys = new Set<string>();
      const onKeyDown = (e: KeyboardEvent): void => {
        keys.add(e.key);
        if (e.key.startsWith('Arrow')) e.preventDefault();
      };
      const onKeyUp = (e: KeyboardEvent): void => {
        keys.delete(e.key);
      };
      document.addEventListener('keydown', onKeyDown);
      document.addEventListener('keyup', onKeyUp);

      let last = performance.now();
      const loop = (now: number): void => {
        if (disposed) return;
        const dt = Math.min(0.05, (now - last) / 1000);
        last = now;

        // movement: forward = (sin yaw, 0, cos yaw)
        const fx = Math.sin(cam.yaw);
        const fz = Math.cos(cam.yaw);
        const rx = Math.cos(cam.yaw);
        const rz = -Math.sin(cam.yaw);
        let mx = 0;
        let mz = 0;
        if (keys.has('w')) {
          mx += fx;
          mz += fz;
        }
        if (keys.has('s')) {
          mx -= fx;
          mz -= fz;
        }
        if (keys.has('a')) {
          mx -= rx;
          mz -= rz;
        }
        if (keys.has('d')) {
          mx += rx;
          mz += rz;
        }
        const mlen = Math.hypot(mx, mz);
        if (mlen > 0) {
          cam.x += (mx / mlen) * MOVE_SPEED * dt;
          cam.z += (mz / mlen) * MOVE_SPEED * dt;
        }
        if (keys.has('ArrowLeft')) cam.yaw -= TURN_SPEED * dt;
        if (keys.has('ArrowRight')) cam.yaw += TURN_SPEED * dt;
        if (keys.has('ArrowUp')) cam.pitch = Math.min(1.5, cam.pitch + TURN_SPEED * 0.5 * dt);
        if (keys.has('ArrowDown')) cam.pitch = Math.max(-1.5, cam.pitch - TURN_SPEED * 0.5 * dt);

        engine.render(world.meshes);
        engine.present(ctx);
        drawMinimap(miniCtx, world, cam);
        raf = requestAnimationFrame(loop);
      };
      raf = requestAnimationFrame(loop);

      return () => {
        disposed = true;
        cancelAnimationFrame(raf);
        document.removeEventListener('keydown', onKeyDown);
        document.removeEventListener('keyup', onKeyUp);
      };
    })().catch((err) => {
      console.error('failed to start engine', err);
    });

    return () => {
      disposed = true;
      cancelAnimationFrame(raf);
    };
  }, []);

  return (
    <div className="App">
      <canvas ref={viewRef} />
      <canvas ref={miniRef} width={400} height={400} />
      <p className="hint">WASD move · arrows turn / look · drag nothing, just keys</p>
    </div>
  );
}

function drawMinimap(ctx: CanvasRenderingContext2D, world: { footprints: { x: number; z: number; w: number; d: number }[]; markers: { x: number; z: number; label: string }[] }, cam: { x: number; y: number; z: number; yaw: number; pitch: number }): void {
  ctx.clearRect(0, 0, 400, 400);
  ctx.fillStyle = '#14181d';
  ctx.fillRect(0, 0, 400, 400);
  const scale = 1.8;
  const ox = 200;
  const oy = 200;
  const sx = (x: number): number => ox + x * scale;
  const sz = (z: number): number => oy + z * scale;
  ctx.strokeStyle = '#2a3a2a';
  ctx.strokeRect(sx(-100), sz(-100), 200 * scale, 200 * scale);
  ctx.fillStyle = '#5a6a7a';
  for (const f of world.footprints) {
    ctx.fillRect(sx(f.x), sz(f.z), f.w * scale, f.d * scale);
  }
  ctx.fillStyle = '#3a8a4a';
  for (const m of world.markers) {
    if (m.label === 'tree') ctx.fillRect(sx(m.x) - 2, sz(m.z) - 2, 5, 5);
    else ctx.fillRect(sx(m.x) - 2, sz(m.z) - 2, 5, 5);
  }
  // camera: position + view cone (yaw)
  const camx = sx(cam.x);
  const camz = sz(cam.z);
  ctx.fillStyle = '#e8e020';
  ctx.beginPath();
  ctx.arc(camx, camz, 4, 0, 2 * Math.PI);
  ctx.fill();
  const halfFov = 35;
  ctx.strokeStyle = '#e8e020';
  ctx.beginPath();
  ctx.moveTo(camx, camz);
  ctx.lineTo(sx(cam.x + 30 * Math.sin(cam.yaw + dtr(halfFov))), sz(cam.z + 30 * Math.cos(cam.yaw + dtr(halfFov))));
  ctx.moveTo(camx, camz);
  ctx.lineTo(sx(cam.x + 30 * Math.sin(cam.yaw - dtr(halfFov))), sz(cam.z + 30 * Math.cos(cam.yaw - dtr(halfFov))));
  ctx.stroke();
}

export default App;
