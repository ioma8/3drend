// Thin glue between the page and the Rust core: asset I/O (fetch, image
// decode), input events, the frame loop, and the minimap overlay.
// All geometry, camera, and rendering live in Rust (wasm-bindgen + wgpu).
// The ?v= query busts the browser's module cache across rebuilds.
const { default: init, App } = await import('../pkg/drend.js?v=' + Date.now());

// World textures, in the order Rust assigns their indices (0..5).
const WORLD_TEX = [
  ['worldmap', 'jpg'],
  ['grasslight', 'jpg'],
  ['wall1', 'png'],
  ['wall2', 'png'],
  ['roof', 'png'],
  ['tree', 'png'],
];
const MODELS = ['tree.obj', 'spider.obj', 'WusonOBJ.obj', 'backpack.obj'];

async function loadRgba(url) {
  const img = new Image();
  img.src = url;
  await img.decode();
  const c = document.createElement('canvas');
  c.width = img.naturalWidth;
  c.height = img.naturalHeight;
  const g = c.getContext('2d');
  g.drawImage(img, 0, 0);
  const id = g.getImageData(0, 0, c.width, c.height);
  return { w: c.width, h: c.height, data: new Uint8Array(id.data.buffer) };
}

async function fetchText(url) {
  const r = await fetch(url);
  if (!r.ok) throw new Error('failed to fetch ' + url);
  return r.text();
}

async function fetchTextMaybe(url) {
  try {
    return await fetchText(url);
  } catch {
    return null;
  }
}

async function loadModel(name) {
  const objText = await fetchText('models/' + name);
  const mtlFile = (objText.match(/^mtllib (.+)$/m) || [])[1];
  const mtlText = mtlFile ? await fetchTextMaybe('models/' + mtlFile) : null;
  const images = [];
  if (mtlText) {
    for (const m of mtlText.matchAll(/^map_Kd (.+)$/gm)) {
      const file = m[1].trim();
      images.push({ file, ...(await loadRgba('models/' + file)) });
    }
  }
  return { obj: objText, mtl: mtlText, images, fallback: -1 };
}

async function main() {
  await init();
  const canvas = document.getElementById('view');
  const mini = document.getElementById('mini');
  const miniCtx = mini.getContext('2d');

  const [worldmap, grasslight, wall1, wall2, roof, tree] = await Promise.all(
    WORLD_TEX.map(([n, ext]) => loadRgba('textures/' + n + '.' + ext)),
  );
  const [treeM, spiderM, wusonM, backpackM] = await Promise.all(MODELS.map(loadModel));
  treeM.fallback = 5; // tree texture index (world textures above)

  const assets = {
    world: [worldmap, grasslight, wall1, wall2, roof, tree],
    tree: treeM,
    spider: spiderM,
    wuson: wusonM,
    backpack: backpackM,
  };
  const app = await App.create(canvas, assets);
  const resize = () => {
    const rect = canvas.getBoundingClientRect();
    if (rect.width === 0 || rect.height === 0) return;
    const dpr = window.devicePixelRatio || 1;
    const width = Math.max(1, Math.round(rect.width * dpr));
    const height = Math.max(1, Math.round(rect.height * dpr));
    if (canvas.width !== width || canvas.height !== height) {
      canvas.width = width;
      canvas.height = height;
      app.resize(width, height);
    }
  };
  const resizeObserver = new ResizeObserver(resize);
  window.addEventListener('resize', resize);
  resizeObserver.observe(canvas);
  resize();
  window.__rust = {
    app,
    readFrame: () => app.readFrame(),
    resizeObserver,
  };

  const footprints = app.footprints();
  const markers = app.markers();

  document.addEventListener('keydown', (e) => {
    app.key(e.key, true);
    if (e.key.startsWith('Arrow')) e.preventDefault();
  });
  document.addEventListener('keyup', (e) => app.key(e.key, false));

  let last = performance.now();
  const loop = (now) => {
    const dt = Math.min(0.05, (now - last) / 1000);
    last = now;
    app.tick(dt);
    drawMinimap(miniCtx, app.cam(), footprints, markers);
    requestAnimationFrame(loop);
  };
  requestAnimationFrame(loop);
}

function drawMinimap(ctx, cam, footprints, markers) {
  const scale = 0.9; // 200 px / ~220 world units
  const ox = 100;
  const oy = 100;
  const sx = (x) => ox + x * scale;
  const sz = (z) => oy + z * scale;
  ctx.fillStyle = '#14181d';
  ctx.fillRect(0, 0, 200, 200);
  ctx.strokeStyle = '#2a3a2a';
  ctx.strokeRect(sx(-100), sz(-100), 200 * scale, 200 * scale);
  ctx.fillStyle = '#5a6a7a';
  for (const f of footprints) ctx.fillRect(sx(f.x), sz(f.z), f.w * scale, f.d * scale);
  ctx.fillStyle = '#3a8a4a';
  for (const m of markers) ctx.fillRect(sx(m.x) - 2, sz(m.z) - 2, 5, 5);
  const camx = sx(cam.x);
  const camz = sz(cam.z);
  ctx.fillStyle = '#e8e020';
  ctx.beginPath();
  ctx.arc(camx, camz, 3, 0, 2 * Math.PI);
  ctx.fill();
  const halfFov = 35;
  ctx.strokeStyle = '#e8e020';
  ctx.beginPath();
  ctx.moveTo(camx, camz);
  ctx.lineTo(sx(cam.x + 30 * Math.sin(cam.yaw + halfFov * Math.PI / 180)), sz(cam.z + 30 * Math.cos(cam.yaw + halfFov * Math.PI / 180)));
  ctx.moveTo(camx, camz);
  ctx.lineTo(sx(cam.x + 30 * Math.sin(cam.yaw - halfFov * Math.PI / 180)), sz(cam.z + 30 * Math.cos(cam.yaw - halfFov * Math.PI / 180)));
  ctx.stroke();
}

main().catch((err) => console.error('failed to start 3drend', err));
