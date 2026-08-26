// Thin glue between the page and the Rust core: asset I/O (fetch, image
// decode), input (pointer lock + keyboard), and the frame loop.
// All game logic, camera, and rendering live in Rust (wasm-bindgen + wgpu).
const { default: init, App } = await import('../pkg/drend.js?v=' + Date.now());

// World textures, in the order Rust assigns their indices: floor, wall,
// door, ceiling.
const WORLD_TEX = [
  ['grasslight', 'jpg'],
  ['wall1', 'png'],
  ['wall2', 'png'],
  ['roof', 'png'],
];
const MODELS = ['spider.obj', 'WusonOBJ.obj'];

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

  const [floor, wall, door, ceil] = await Promise.all(
    WORLD_TEX.map(([n, ext]) => loadRgba('textures/' + n + '.' + ext)),
  );
  const [spiderM, wusonM] = await Promise.all(MODELS.map(loadModel));

  const assets = {
    world: [floor, wall, door, ceil],
    spider: spiderM,
    wuson: wusonM,
  };
  const app = await App.create(canvas, assets);
  window.__rust = { app, readFrame: () => app.readFrame() };

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
  resizeObserver.observe(canvas);
  window.addEventListener('resize', resize);
  resize();

  // Keyboard: map the space key's " " to "Space" so it matches the native
  // frontend's key codes.
  const key = (e, down) => {
    const code = e.key === ' ' ? 'Space' : e.key;
    app.key(code, down);
    if (e.key.startsWith('Arrow') || e.key === ' ') e.preventDefault();
  };
  document.addEventListener('keydown', (e) => key(e, true));
  document.addEventListener('keyup', (e) => key(e, false));

  // Mouse look (pointer lock) and firing.
  canvas.addEventListener('mousemove', (e) => {
    if (document.pointerLockElement === canvas) app.look(e.movementX, e.movementY);
  });
  canvas.addEventListener('mousedown', (e) => {
    if (e.button === 0) app.key('Space', true);
    else if (e.button === 2) app.key('e', true);
  });
  canvas.addEventListener('mouseup', (e) => {
    if (e.button === 0) app.key('Space', false);
    else if (e.button === 2) app.key('e', false);
  });
  canvas.addEventListener('click', () => {
    if (!document.pointerLockElement) canvas.requestPointerLock();
  });
  canvas.addEventListener('contextmenu', (e) => e.preventDefault());

  let last = performance.now();
  const loop = (now) => {
    const dt = Math.min(0.05, (now - last) / 1000);
    last = now;
    app.tick(dt);
    requestAnimationFrame(loop);
  };
  requestAnimationFrame(loop);
}

main().catch((err) => console.error('failed to start 3drend', err));
