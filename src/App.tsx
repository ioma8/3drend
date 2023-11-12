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

  let ua = ((x4 - x3) * (y1 - y3) - (y4 - y3) * (x1 - x3)) / denominator
  let ub = ((x2 - x1) * (y1 - y3) - (y2 - y1) * (x1 - x3)) / denominator

  // is the intersection along the segments
  if (ua < 0 || ua > 1 || ub < 0 || ub > 1) {
    return
  }

  // Return a object with the x and y coordinates of the intersection
  let x = x1 + ua * (x2 - x1)
  let y = y1 + ua * (y2 - y1)

  return [x, y]
}



type Coords = [number, number]; // x, y
type Wall = [Coords, Coords] // start, end


const distance = (p1: Coords, p2: Coords): number => {
  const a = p1[0] - p2[0];
  const b = p1[1] - p2[1];

  const c = Math.sqrt(a * a + b * b);
  return c;
}

const walls: Wall[] = [
  [[10, 10], [300, 150]]
];

type Camera = {
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
    const ctx = canvas.getContext("2d")!;
    ctx.clearRect(0, 0, canvas.width, canvas.height);

    const canvas3D = document.getElementById("canvas3D") as HTMLCanvasElement;
    canvas3D.width = 400;
    canvas3D.height = 400;
    const ctx3D = canvas3D.getContext("2d")!;
    ctx3D.clearRect(0, 0, canvas3D.width, canvas3D.height);

    for (const wall of walls) {
      ctx.beginPath();
      ctx.moveTo(wall[0][0], wall[0][1]);
      ctx.lineTo(wall[1][0], wall[1][1]);
      ctx.stroke();
    }

    const cam_angle = fix_degrees(camera.direction);
    const cam_from = cam_angle - (camera.angle / 2);
    const cam_to = cam_angle + (camera.angle / 2);

    ctx.beginPath();
    ctx.arc(camera.coords[0], camera.coords[1], 15, dtr(cam_to), dtr(cam_from));
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

    for (const wall of walls) {
      const visible_wall_edges: Coords[] = []

      const intersect1 = intersect(camera.coords[0], camera.coords[1], x1_add, y1_add, wall[0][0], wall[0][1], wall[1][0], wall[1][1])
      if (intersect1 !== undefined) {
        ctx.beginPath();
        ctx.arc(intersect1[0], intersect1[1], 5, 0, 2 * Math.PI);
        ctx.stroke();
        visible_wall_edges.push(intersect1);
      }
      const intersect2 = intersect(camera.coords[0], camera.coords[1], x2_add, y2_add, wall[0][0], wall[0][1], wall[1][0], wall[1][1])
      if (intersect2 !== undefined) {
        ctx.beginPath();
        ctx.arc(intersect2[0], intersect2[1], 5, 0, 2 * Math.PI);
        ctx.stroke();
        visible_wall_edges.push(intersect2);
      }

      if (visible_wall_edges.length === 2) {
        const dist1 = distance(camera.coords, visible_wall_edges[0]);
        const dist2 = distance(camera.coords, visible_wall_edges[1]);
        const p1_hh = canvas3D.height / 2 - (dist1 / line_length) * canvas3D.height / 2
        const p2_hh = canvas3D.height / 2 - (dist2 / line_length) * canvas3D.height / 2

        ctx3D.beginPath();
        ctx3D.moveTo(0, canvas3D.height / 2 + p1_hh);
        ctx3D.lineTo(canvas3D.width, canvas3D.height / 2 + p2_hh);
        ctx3D.lineTo(canvas3D.width, canvas3D.height / 2 - p2_hh);
        ctx3D.lineTo(0, canvas3D.height / 2 - p1_hh);
        ctx3D.lineTo(0, canvas3D.height / 2 + p1_hh);
        ctx3D.stroke();
      }
      else if (visible_wall_edges.length === 1) {
        for (const edge_coord of wall) {
          const dist_camera_edge = distance(camera.coords, edge_coord);
          const dist_y = camera.coords[1] - edge_coord[1];
          const alpha = 2 * Math.PI - Math.asin(dist_y / dist_camera_edge);
          const camera_angles = [dtr(cam_from), dtr(cam_to)].sort()
          if (alpha > camera_angles[0] && alpha < camera_angles[1]) {
            const angle_diff = Math.abs(camera_angles[0] - camera_angles[1])
            const point_diff = Math.abs(camera_angles[0] - alpha)
            const left_dist = (point_diff / angle_diff) * canvas3D.width;
            console.error(left_dist)
            const dist1 = distance(camera.coords, visible_wall_edges[0]);
            const dist_inside = distance(camera.coords, edge_coord);
            const p1_hh = canvas3D.height / 2 - (dist1 / line_length) * canvas3D.height / 2
            const p2_hh = canvas3D.height / 2 - (dist_inside / line_length) * canvas3D.height / 2
            ctx3D.beginPath();
            ctx3D.moveTo(0, canvas3D.height / 2 + p1_hh);
            ctx3D.lineTo(left_dist, canvas3D.height / 2 + p2_hh);
            ctx3D.lineTo(left_dist, canvas3D.height / 2 - p2_hh);
            ctx3D.lineTo(0, canvas3D.height / 2 - p1_hh);
            ctx3D.lineTo(0, canvas3D.height / 2 + p1_hh);
            ctx3D.stroke();
          }
        }
        console.error("=====")
      }

    }

  }, []);

  useEffect(() => {
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
