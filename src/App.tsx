import React, { useCallback, useEffect } from 'react';
import './App.css';

const dtr = (deg: number): number => (deg * Math.PI) / 180.0;
const fix_degrees = (deg: number): number => {
  while (deg > 360) {
    deg = deg - 360;
  }
  while (deg < 0) {
    deg = deg + 360;
  }
  return deg;
}

const intersect = (x1: number, y1: number, x2: number, y2: number, x3: number, y3: number, x4: number, y4: number): Coords | undefined => {

  // Check if none of the lines are of length 0
  if ((x1 === x2 && y1 === y2) || (x3 === x4 && y3 === y4)) {
    return
  }

  const denominator = ((y4 - y3) * (x2 - x1) - (x4 - x3) * (y2 - y1))

  // Lines are parallel
  if (denominator === 0) {
    return
  }

  const ua = ((x4 - x3) * (y1 - y3) - (y4 - y3) * (x1 - x3)) / denominator
  const ub = ((x2 - x1) * (y1 - y3) - (y2 - y1) * (x1 - x3)) / denominator

  // is the intersection along the segments
  if (ua < 0 || ua > 1 || ub < 0 || ub > 1) {
    return
  }

  // Return a object with the x and y coordinates of the intersection
  const x = x1 + ua * (x2 - x1)
  const y = y1 + ua * (y2 - y1)

  return [x, y]
}



type Coords = [number, number]; // x, y
type Wall = [Coords, Coords, string] // start, end, texture url


const in_frustum = (angle: number, from: number, to: number): boolean => {
  if (from <= to) {
    return angle >= from && angle <= to;
  }
  return angle >= from || angle <= to;
}

const distance = (p1: Coords, p2: Coords): number => {
  const a = p1[0] - p2[0];
  const b = p1[1] - p2[1];
  return Math.sqrt(a * a + b * b);
}

const wall_param = (p: Coords, a: Coords, b: Coords): number => {
  const dx = b[0] - a[0];
  const dy = b[1] - a[1];
  const len2 = dx * dx + dy * dy;
  if (len2 === 0) {
    return 0;
  }
  return ((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / len2;
}

const angle_to_x = (angle: number, from: number, to: number, width: number): number => {
  const total = from <= to ? to - from : to - from + 360;
  const span = angle >= from ? angle - from : angle - from + 360;
  return (span / total) * width;
}

const half_height = (dist: number, height: number, max_dist: number): number => {
  return Math.max(0, height / 2 - (dist / max_dist) * (height / 2));
}

const dist_to_segment = (p: Coords, a: Coords, b: Coords): number => {
  const dx = b[0] - a[0];
  const dy = b[1] - a[1];
  const t = Math.max(0, Math.min(1, ((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / (dx * dx + dy * dy)));
  return Math.hypot(p[0] - (a[0] + t * dx), p[1] - (a[1] + t * dy));
}

const walls: Wall[] = [
  [[10, 10], [300, 150], "/textures/wall1.png"],
  [[300, 150], [300, 200], "/textures/wall2.png"]
];

const wall_textures: Record<string, HTMLImageElement> = {};
for (const wall of walls) {
  const img = new Image();
  img.src = wall[2];
  wall_textures[wall[2]] = img;
}

interface Camera {
  coords: Coords;
  direction: number;
  angle: number;
}

const camera: Camera = {
  coords: [200, 200],
  direction: 270,
  angle: 60
}

const movement_size = 5;

function App() {

  const redraw = useCallback(() => {
    const canvas = document.getElementById("canvas2D") as HTMLCanvasElement;
    canvas.width = 400;
    canvas.height = 400;
    const ctx = canvas.getContext("2d");
    if (ctx === null) {
      return;
    }
    ctx.clearRect(0, 0, canvas.width, canvas.height);

    const canvas3D = document.getElementById("canvas3D") as HTMLCanvasElement;
    canvas3D.width = 400;
    canvas3D.height = 400;
    const ctx3D = canvas3D.getContext("2d");
    if (ctx3D === null) {
      return;
    }
    ctx3D.clearRect(0, 0, canvas3D.width, canvas3D.height);

    for (const wall of walls) {
      ctx.beginPath();
      ctx.moveTo(wall[0][0], wall[0][1]);
      ctx.lineTo(wall[1][0], wall[1][1]);
      ctx.stroke();
    }

    const cam_angle = fix_degrees(camera.direction);
    const cam_from = fix_degrees(cam_angle - (camera.angle / 2));
    const cam_to = fix_degrees(cam_angle + (camera.angle / 2));

    ctx.beginPath();
    ctx.arc(camera.coords[0], camera.coords[1], 15, dtr(cam_from), dtr(cam_to));
    ctx.stroke();


    const line_length = 400;
    const x1_add = camera.coords[0] + Math.sin(dtr(90 - cam_from)) * line_length;
    const y1_add = camera.coords[1] + Math.cos(dtr(90 - cam_from)) * line_length;
    const x2_add = camera.coords[0] + Math.sin(dtr(90 - cam_to)) * line_length;
    const y2_add = camera.coords[1] + Math.cos(dtr(90 - cam_to)) * line_length;
    ctx.beginPath();
    ctx.moveTo(camera.coords[0], camera.coords[1]);
    ctx.lineTo(x1_add, y1_add);
    ctx.stroke();
    ctx.beginPath();
    ctx.moveTo(camera.coords[0], camera.coords[1]);
    ctx.lineTo(x2_add, y2_add);
    ctx.stroke();

    // Draw far walls first so nearer walls paint over them (painter's algorithm).
    const walls_by_depth = [...walls].sort(
      (a, b) => dist_to_segment(camera.coords, b[0], b[1]) - dist_to_segment(camera.coords, a[0], a[1])
    );
    for (const wall of walls_by_depth) {

      // 2D top-down view: mark where the frustum boundary rays meet the wall.
      const hit_from = intersect(camera.coords[0], camera.coords[1], x1_add, y1_add, wall[0][0], wall[0][1], wall[1][0], wall[1][1]);
      if (hit_from !== undefined) {
        ctx.beginPath();
        ctx.arc(hit_from[0], hit_from[1], 5, 0, 2 * Math.PI);
        ctx.stroke();
      }
      const hit_to = intersect(camera.coords[0], camera.coords[1], x2_add, y2_add, wall[0][0], wall[0][1], wall[1][0], wall[1][1]);
      if (hit_to !== undefined) {
        ctx.beginPath();
        ctx.arc(hit_to[0], hit_to[1], 5, 0, 2 * Math.PI);
        ctx.stroke();
      }

      // 3D view: fill the wall's screen quad with its texture, matching the
      // wireframe geometry exactly. Anchors = in-frustum wall endpoints plus
      // frustum-boundary hits; the height interpolates linearly between the
      // two extreme anchors, so the textured fill has the same straight edges
      // as the wireframe quad.
      const [wall_a, wall_b] = wall;
      const anchors: { x: number; hh: number; u: number }[] = [];

      for (const edge_coord of [wall_a, wall_b]) {
        const angle = fix_degrees((Math.atan2(edge_coord[1] - camera.coords[1], edge_coord[0] - camera.coords[0]) * 180) / Math.PI);
        if (in_frustum(angle, cam_from, cam_to)) {
          anchors.push({
            x: angle_to_x(angle, cam_from, cam_to, canvas3D.width),
            hh: half_height(distance(camera.coords, edge_coord), canvas3D.height, line_length),
            u: wall_param(edge_coord, wall_a, wall_b),
          });
        }
      }
      if (hit_from !== undefined) {
        anchors.push({
          x: 0,
          hh: half_height(distance(camera.coords, hit_from), canvas3D.height, line_length),
          u: wall_param(hit_from, wall_a, wall_b),
        });
      }
      if (hit_to !== undefined) {
        anchors.push({
          x: canvas3D.width,
          hh: half_height(distance(camera.coords, hit_to), canvas3D.height, line_length),
          u: wall_param(hit_to, wall_a, wall_b),
        });
      }

      if (anchors.length >= 2) {
        anchors.sort((a, b) => a.x - b.x);
        const left = anchors[0];
        const right = anchors[anchors.length - 1];
        const img = wall_textures[wall[2]];
        const ready = img.complete && img.naturalWidth > 0;
        for (let x = Math.ceil(left.x); x <= Math.floor(right.x); x++) {
          const f = (x - left.x) / (right.x - left.x);
          const hh = left.hh + (right.hh - left.hh) * f;
          const top = Math.round(canvas3D.height / 2 - hh);
          const bottom = Math.round(canvas3D.height / 2 + hh);
          if (bottom <= top) {
            continue;
          }
          if (ready) {
            const u = Math.floor((left.u + (right.u - left.u) * f) * img.width) % img.width;
            ctx3D.drawImage(img, u, 0, 1, img.height, x, top, 1, bottom - top);
          } else {
            ctx3D.fillStyle = "#6a6a6a";
            ctx3D.fillRect(x, top, 1, bottom - top);
          }
        }
      }
    }

  }, []);

  useEffect(() => {
    for (const img of Object.values(wall_textures)) {
      if (!img.complete) {
        img.onload = () => { redraw(); };
      }
    }
    redraw();
    document.onkeydown = (e: KeyboardEvent): void => {

      if (e.keyCode === 38) {
        // up arrow
        const new_x = camera.coords[0] + Math.cos(dtr(camera.direction)) * movement_size
        const new_y = camera.coords[1] + Math.sin(dtr(camera.direction)) * movement_size
        camera.coords = [new_x, new_y]
        redraw();
      }
      else if (e.keyCode === 40) {
        // down arrow
        const new_x = camera.coords[0] - Math.cos(dtr(camera.direction)) * movement_size
        const new_y = camera.coords[1] - Math.sin(dtr(camera.direction)) * movement_size
        camera.coords = [new_x, new_y]
        redraw();
      }
      else if (e.keyCode === 37) {
        // left arrow
        camera.direction = camera.direction - 5;
        redraw();
      }
      else if (e.keyCode === 39) {
        // right arrow
        camera.direction = camera.direction + 5;
        redraw();
      }

    }
  });

  return (
    <div className="App">
      <canvas id="canvas2D" />
      <canvas id="canvas3D" />
    </div>
  );
}

export default App;
