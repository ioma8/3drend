//! Basic Doom-style game: a grid maze with sliding doors, 3D-model enemies,
//! and distinct hitscan guns. Produces the meshes the renderer draws each
//! frame; owns all runtime state (player, enemies, doors, weapons).

use crate::app::Camera;
use crate::obj::{load_obj, Mesh, Texture};
use crate::renderer::Hud;
use crate::world::{box_mesh, quad, shade_mesh, transform_mesh, vert, ModelAssets};

pub const CELL: f32 = 4.0;
const WALL_H: f32 = 4.0;
const EYE: f32 = 1.6;
const PLAYER_R: f32 = 0.35;
const MAX_HEALTH: i32 = 100;
const DOOR_SPEED: f32 = 1.5; // seconds to fully open
const DOOR_HOLD: f32 = 3.0;

pub const TEX_FLOOR: u32 = 0;
pub const TEX_WALL: u32 = 1;
pub const TEX_DOOR: u32 = 2;
pub const TEX_CEIL: u32 = 3;

// 15x15 grid. # = wall, . = floor, D = door, P = player spawn,
// E = spider enemy, W = wuson enemy.
const MAP: &str = "\
###############
#P....#..#...E#
#.##..#..#.##.#
#.#D#.#..#.#..#
#.#..#D#..#.#.#
#D#..#..#.#D#.#
#..##..##..#..#
#.#..E..#..##D#
#.#..#D#..#...#
#..####..##..D#
#D..#..#.#..W.#
#..#.#..#..##.#
#.#.#..#.#..#.#
#..E..#....#..#
###############";

#[derive(Clone, Copy, PartialEq)]
pub enum Weapon {
    Pistol,
    Smg,
    Shotgun,
}

impl Weapon {
    fn all() -> [Weapon; 3] {
        [Weapon::Pistol, Weapon::Smg, Weapon::Shotgun]
    }
    fn index(&self) -> u32 {
        match self {
            Weapon::Pistol => 0,
            Weapon::Smg => 1,
            Weapon::Shotgun => 2,
        }
    }
    fn cooldown(&self) -> f32 {
        match self {
            Weapon::Pistol => 0.28,
            Weapon::Smg => 0.09,
            Weapon::Shotgun => 0.85,
        }
    }
    fn pellets(&self) -> i32 {
        match self {
            Weapon::Pistol => 1,
            Weapon::Smg => 1,
            Weapon::Shotgun => 6,
        }
    }
    fn damage(&self) -> i32 {
        match self {
            Weapon::Pistol => 25,
            Weapon::Smg => 9,
            Weapon::Shotgun => 15,
        }
    }
    fn spread(&self) -> f32 {
        match self {
            Weapon::Pistol => 0.0,
            Weapon::Smg => 0.02,
            Weapon::Shotgun => 0.09,
        }
    }
    fn box_dims(&self) -> (f32, f32, f32) {
        // (width, height, length) of the primitive view model
        match self {
            Weapon::Pistol => (0.16, 0.20, 0.45),
            Weapon::Smg => (0.18, 0.24, 0.65),
            Weapon::Shotgun => (0.24, 0.20, 0.85),
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum EnemyKind {
    Spider,
    Wuson,
}

struct Enemy {
    pos: [f32; 2],
    health: i32,
    kind: EnemyKind,
    cooldown: f32,
    flash: f32,
}

#[derive(Clone, Copy, PartialEq)]
enum DoorState {
    Closed,
    Opening,
    Open,
    Closing,
}

struct Door {
    x: usize,
    z: usize,
    open: f32, // 0 closed .. 1 open
    state: DoorState,
    timer: f32,
}

/// Per-frame player intent, assembled by the frontend.
pub struct Input {
    pub forward: f32,    // -1..1
    pub strafe: f32,     // -1..1
    pub turn: f32,       // radians (arrow keys)
    pub look_yaw: f32,   // radians (mouse)
    pub look_pitch: f32, // radians (mouse)
    pub fire: bool,
    pub use_door: bool,
    pub switch: i32, // -1 = none, else weapon index
}

pub struct Game {
    w: usize,
    h: usize,
    grid: Vec<u8>,
    static_meshes: Vec<Mesh>,
    door_base: Mesh,
    enemy_spider: Mesh,
    enemy_wuson: Mesh,
    guns: Vec<Mesh>,
    camera: Camera,
    health: i32,
    weapon: Weapon,
    fire_cooldown: f32,
    muzzle: f32,
    enemies: Vec<Enemy>,
    doors: Vec<Door>,
    time: f32,
    rng: u32,
    textures: Vec<Texture>,
}

impl Game {
    pub fn new(textures: Vec<Texture>, spider: ModelAssets, wuson: ModelAssets) -> Game {
        let mut textures = textures;
        let enemy_spider = load_obj(&spider.obj_text, spider.mtl_text.as_deref(), spider.images, &mut textures, -1);
        let enemy_wuson = load_obj(&wuson.obj_text, wuson.mtl_text.as_deref(), wuson.images, &mut textures, -1);

        // Parse the map.
        let mut grid = Vec::new();
        let mut w = 0;
        let mut h = 0;
        let mut spawn = (1.0, 1.0);
        let mut enemies = Vec::new();
        let mut doors = Vec::new();
        for (row, line) in MAP.lines().enumerate() {
            w = line.len();
            h = row + 1;
            for (col, ch) in line.chars().enumerate() {
                let cell = match ch {
                    '#' | 'D' => {
                        if ch == 'D' {
                            doors.push(Door { x: col, z: row, open: 0.0, state: DoorState::Closed, timer: 0.0 });
                            2
                        } else {
                            1
                        }
                    }
                    'P' => {
                        spawn = (col as f32 * CELL + CELL / 2.0, row as f32 * CELL + CELL / 2.0);
                        0
                    }
                    'E' | 'W' => {
                        let kind = if ch == 'E' { EnemyKind::Spider } else { EnemyKind::Wuson };
                        enemies.push(Enemy {
                            pos: [col as f32 * CELL + CELL / 2.0, row as f32 * CELL + CELL / 2.0],
                            health: if kind == EnemyKind::Spider { 40 } else { 90 },
                            kind,
                            cooldown: 0.0,
                            flash: 0.0,
                        });
                        0
                    }
                    _ => 0,
                };
                grid.push(cell);
            }
        }

        // Static geometry: floor, ceiling, and all non-door wall cells.
        let mut static_meshes = Vec::new();
        let size = (w as f32 * CELL, h as f32 * CELL);
        let mut floor = Mesh::default();
        quad(&mut floor, TEX_FLOOR,
            vert(-1.0, 0.0, -1.0, 0.0, 0.0),
            vert(size.0 + 1.0, 0.0, -1.0, size.0 / CELL, 0.0),
            vert(size.0 + 1.0, 0.0, size.1 + 1.0, size.0 / CELL, size.1 / CELL),
            vert(-1.0, 0.0, size.1 + 1.0, 0.0, size.1 / CELL));
        static_meshes.push(floor);
        let mut ceil = Mesh::default();
        quad(&mut ceil, TEX_CEIL,
            vert(-1.0, WALL_H, -1.0, 0.0, 0.0),
            vert(size.0 + 1.0, WALL_H, -1.0, size.0 / CELL, 0.0),
            vert(size.0 + 1.0, WALL_H, size.1 + 1.0, size.0 / CELL, size.1 / CELL),
            vert(-1.0, WALL_H, size.1 + 1.0, 0.0, size.1 / CELL));
        static_meshes.push(ceil);
        let mut walls = Mesh::default();
        for z in 0..h {
            for x in 0..w {
                if grid[z * w + x] == 1 {
                    box_mesh(&mut walls, x as f32 * CELL + CELL / 2.0, z as f32 * CELL + CELL / 2.0, CELL, WALL_H, CELL, TEX_WALL, TEX_WALL);
                }
            }
        }
        shade_mesh(&mut walls, -0.4, 0.8, 0.35);
        static_meshes.push(walls);

        let mut door_base = Mesh::default();
        box_mesh(&mut door_base, CELL / 2.0, CELL / 2.0, CELL, WALL_H, CELL, TEX_DOOR, TEX_DOOR);
        shade_mesh(&mut door_base, -0.4, 0.8, 0.35);

        // Primitive gun view models (one box each, distinct per weapon).
        let mut guns = Vec::new();
        for w in Weapon::all() {
            let (wd, ht, ln) = w.box_dims();
            let mut m = Mesh::default();
            box_mesh(&mut m, 0.0, 0.0, wd, ht, ln, TEX_WALL, TEX_WALL);
            shade_mesh(&mut m, -0.2, 0.9, 0.3);
            guns.push(m);
        }

        let camera = Camera::new(spawn.0, EYE, spawn.1, 0.0, 0.0);

        Game {
            w,
            h,
            grid,
            static_meshes,
            door_base,
            enemy_spider,
            enemy_wuson,
            guns,
            camera,
            health: MAX_HEALTH,
            weapon: Weapon::Pistol,
            fire_cooldown: 0.0,
            muzzle: 0.0,
            enemies,
            doors,
            time: 0.0,
            rng: 0x1234_5678,
            textures,
        }
    }

    pub fn textures(&self) -> &[Texture] {
        &self.textures
    }

    pub fn camera(&self) -> Camera {
        self.camera
    }

    pub fn hud(&self) -> Hud {
        Hud {
            health: (self.health as f32 / MAX_HEALTH as f32).max(0.0),
            ammo: 1.0,
            weapon: self.weapon.index(),
            muzzle: self.muzzle,
        }
    }

    /// Advance the simulation.
    pub fn update(&mut self, dt: f32, input: &Input) {
        self.time += dt;
        if let Some(w) = Weapon::all().get(input.switch.max(0) as usize) {
            if input.switch >= 0 {
                self.weapon = *w;
            }
        }
        self.fire_cooldown = (self.fire_cooldown - dt).max(0.0);
        self.muzzle = (self.muzzle - dt * 6.0).max(0.0);

        self.camera.yaw += input.turn + input.look_yaw;
        self.camera.pitch = (self.camera.pitch + input.look_pitch).clamp(-1.4, 1.4);

        // Movement with per-axis collision so the player slides along walls.
        let yaw = self.camera.yaw;
        let (fx, fz) = (yaw.sin(), yaw.cos());
        let (rx, rz) = (yaw.cos(), -yaw.sin());
        let mx = input.forward * fx + input.strafe * rx;
        let mz = input.forward * fz + input.strafe * rz;
        let speed = 5.0;
        let nx = self.camera.x + mx * speed * dt;
        if !collides(&self.grid, self.w, self.h, &self.doors, nx, self.camera.z) {
            self.camera.x = nx;
        }
        let nz = self.camera.z + mz * speed * dt;
        if !collides(&self.grid, self.w, self.h, &self.doors, self.camera.x, nz) {
            self.camera.z = nz;
        }

        // Doors.
        for door in &mut self.doors {
            match door.state {
                DoorState::Opening => {
                    door.open = (door.open + dt / DOOR_SPEED).min(1.0);
                    if door.open >= 1.0 {
                        door.state = DoorState::Open;
                        door.timer = DOOR_HOLD;
                    }
                }
                DoorState::Open => {
                    door.timer -= dt;
                    if door.timer <= 0.0 {
                        door.state = DoorState::Closing;
                    }
                }
                DoorState::Closing => {
                    door.open = (door.open - dt / DOOR_SPEED).max(0.0);
                    if door.open <= 0.0 {
                        door.state = DoorState::Closed;
                    }
                }
                DoorState::Closed => {}
            }
        }

        // Use door: toggle the door cell directly ahead of the player.
        if input.use_door {
            self.toggle_door_ahead();
        }

        // Enemies: home in on the player, attack when close.
        let (px, pz) = (self.camera.x, self.camera.z);
        for e in &mut self.enemies {
            if e.health <= 0 {
                continue;
            }
            e.flash = (e.flash - dt).max(0.0);
            e.cooldown = (e.cooldown - dt).max(0.0);
            let (dx, dz) = (px - e.pos[0], pz - e.pos[1]);
            let dist = (dx * dx + dz * dz).sqrt();
            let speed = if e.kind == EnemyKind::Spider { 3.0 } else { 1.4 };
            if dist > 1.2 && dist < 25.0 {
                let step = speed * dt;
                let nx = e.pos[0] + dx / dist * step;
                let nz = e.pos[1] + dz / dist * step;
                if !solid_at(&self.grid, self.w, self.h, &self.doors, nx, nz) {
                    e.pos = [nx, nz];
                }
            }
            if dist < 1.6 && e.cooldown <= 0.0 {
                self.health -= if e.kind == EnemyKind::Spider { 8 } else { 15 };
                e.cooldown = 1.0;
            }
        }

        // Firing.
        if input.fire && self.fire_cooldown <= 0.0 {
            self.fire_cooldown = self.weapon.cooldown();
            self.muzzle = 1.0;
            for _ in 0..self.weapon.pellets() {
                let dir = self.aim_dir(self.weapon.spread());
                self.hitscan([self.camera.x, EYE, self.camera.z], dir);
            }
        }
    }

    /// All meshes for this frame: static maze + doors + enemies + gun.
    pub fn meshes(&self) -> Vec<Mesh> {
        let mut out = self.static_meshes.clone();

        for door in &self.doors {
            let mut m = self.door_base.clone();
            let y = -door.open * WALL_H;
            transform_mesh(&mut m, 1.0, 1.0, 1.0, 0.0, door.x as f32 * CELL, y, door.z as f32 * CELL);
            out.push(m);
        }

        for e in &self.enemies {
            if e.health <= 0 {
                continue;
            }
            let base = match e.kind {
                EnemyKind::Spider => &self.enemy_spider,
                EnemyKind::Wuson => &self.enemy_wuson,
            };
            let mut m = base.clone();
            let (sx, sy, sz) = match e.kind {
                EnemyKind::Spider => (0.03, 0.03, 0.03),
                EnemyKind::Wuson => (2.0, 2.0, 2.0),
            };
            let bob = (self.time * 3.0 + e.pos[0] + e.pos[1]).sin() * 0.1;
            transform_mesh(&mut m, sx, sy, sz, 0.0, e.pos[0], bob, e.pos[1]);
            if e.flash > 0.0 {
                brighten(&mut m);
            }
            out.push(m);
        }

        out.push(self.gun_mesh());
        out
    }

    fn gun_mesh(&self) -> Mesh {
        let mut m = self.guns[self.weapon.index() as usize].clone();
        let yaw = self.camera.yaw;
        let (fx, fz) = (yaw.sin(), yaw.cos());
        let (rx, rz) = (yaw.cos(), -yaw.sin());
        let tx = self.camera.x + fx * 0.55 + rx * 0.18;
        let ty = EYE - 0.35;
        let tz = self.camera.z + fz * 0.55 + rz * 0.18;
        transform_mesh(&mut m, 1.0, 1.0, 1.0, yaw, tx, ty, tz);
        m
    }


    fn toggle_door_ahead(&mut self) {
        let (px, pz) = (self.camera.x, self.camera.z);
        let (dx, dz) = (self.camera.yaw.sin(), self.camera.yaw.cos());
        // Scan the cells a short distance ahead.
        for step in 1..=2 {
            let cx = ((px + dx * step as f32 * CELL * 0.8) / CELL).floor() as i32;
            let cz = ((pz + dz * step as f32 * CELL * 0.8) / CELL).floor() as i32;
            if cx >= 0 && cz >= 0 && (cx as usize) < self.w && (cz as usize) < self.h {
                if cell(&self.grid, self.w, self.h, cx, cz) == 2 {
                    let idx = self.doors.iter().position(|d| d.x == cx as usize && d.z == cz as usize);
                    if let Some(idx) = idx {
                        let door = &mut self.doors[idx];
                        if door.state == DoorState::Closed || door.state == DoorState::Open {
                            door.state = if door.state == DoorState::Closed { DoorState::Opening } else { DoorState::Closing };
                        }
                    }
                    return;
                }
            }
        }
    }

    fn aim_dir(&mut self, spread: f32) -> [f32; 3] {
        let jy = (self.rand01() - 0.5) * 2.0 * spread;
        let jp = (self.rand01() - 0.5) * 2.0 * spread;
        let yaw = self.camera.yaw + jy;
        let pitch = self.camera.pitch + jp;
        let (cp, sp) = (pitch.cos(), pitch.sin());
        [yaw.sin() * cp, sp, yaw.cos() * cp]
    }

    fn hitscan(&mut self, origin: [f32; 3], dir: [f32; 3]) {
        let max_dist = 40.0;
        let step = 0.15;
        let mut wall_dist: Option<f32> = None;
        let mut t = 0.0;
        while t < max_dist {
            let x = origin[0] + dir[0] * t;
            let z = origin[2] + dir[2] * t;
            if solid_at(&self.grid, self.w, self.h, &self.doors, x, z) {
                wall_dist = Some(t);
                break;
            }
            t += step;
        }

        let mut best: Option<(usize, f32)> = None;
        for (i, e) in self.enemies.iter().enumerate() {
            if e.health <= 0 {
                continue;
            }
            if let Some(d) = ray_circle(origin[0], origin[2], dir[0], dir[2], e.pos[0], e.pos[1], 0.8) {
                if d < max_dist && wall_dist.map_or(true, |w| d < w) {
                    if best.map_or(true, |(_, bd)| d < bd) {
                        best = Some((i, d));
                    }
                }
            }
        }
        if let Some((i, _)) = best {
            let e = &mut self.enemies[i];
            e.health -= self.weapon.damage();
            e.flash = 0.15;
        }
    }

    fn rand01(&mut self) -> f32 {
        self.rng = self.rng.wrapping_mul(1103515245).wrapping_add(12345);
        (self.rng >> 8) as f32 / 16_777_216.0
    }
}

/// Grid cell at integer cell coordinates; out of bounds is solid.
fn cell(grid: &[u8], w: usize, h: usize, x: i32, z: i32) -> u8 {
    if x < 0 || z < 0 || x >= w as i32 || z >= h as i32 {
        return 1;
    }
    grid[z as usize * w + x as usize]
}

fn door_open(doors: &[Door], x: usize, z: usize) -> f32 {
    doors.iter().find(|d| d.x == x && d.z == z).map(|d| d.open).unwrap_or(0.0)
}

fn solid_at(grid: &[u8], w: usize, h: usize, doors: &[Door], x: f32, z: f32) -> bool {
    let cx = (x / CELL).floor() as i32;
    let cz = (z / CELL).floor() as i32;
    let c = cell(grid, w, h, cx, cz);
    if c == 0 {
        return false;
    }
    if c == 1 {
        return true;
    }
    door_open(doors, cx as usize, cz as usize) < 0.8
}

fn collides(grid: &[u8], w: usize, h: usize, doors: &[Door], x: f32, z: f32) -> bool {
    let min_x = ((x - PLAYER_R) / CELL).floor() as i32;
    let max_x = ((x + PLAYER_R) / CELL).floor() as i32;
    let min_z = ((z - PLAYER_R) / CELL).floor() as i32;
    let max_z = ((z + PLAYER_R) / CELL).floor() as i32;
    for gz in min_z..=max_z {
        for gx in min_x..=max_x {
            let c = cell(grid, w, h, gx, gz);
            if c == 1 || (c == 2 && door_open(doors, gx as usize, gz as usize) < 0.8) {
                return true;
            }
        }
    }
    false
}

/// Ray (2D, in the XZ plane) vs a circle; returns the nearest hit distance.
fn ray_circle(ox: f32, oz: f32, dx: f32, dz: f32, cx: f32, cz: f32, r: f32) -> Option<f32> {
    let (fx, fz) = (ox - cx, oz - cz);
    let a = dx * dx + dz * dz;
    let b = 2.0 * (fx * dx + fz * dz);
    let c = fx * fx + fz * fz - r * r;
    let disc = b * b - 4.0 * a * c;
    if disc < 0.0 {
        return None;
    }
    let sq = disc.sqrt();
    let t0 = (-b - sq) / (2.0 * a);
    let t1 = (-b + sq) / (2.0 * a);
    let t = if t0 >= 0.0 { t0 } else { t1 };
    if t >= 0.0 {
        Some(t)
    } else {
        None
    }
}

/// Set every face to full brightness (hit flash).
fn brighten(mesh: &mut Mesh) {
    for t in &mut mesh.tris {
        t.shade = 1.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::ModelAssets;

    fn empty_assets() -> ModelAssets {
        ModelAssets { obj_text: String::new(), mtl_text: None, images: vec![], fallback: -1 }
    }

    fn dummy_textures() -> Vec<Texture> {
        (0..4).map(|_| Texture { w: 1, h: 1, data: vec![0; 4] }).collect()
    }

    #[test]
    fn maze_parses_walls_doors_and_enemies() {
        let g = Game::new(dummy_textures(), empty_assets(), empty_assets());
        let walls = g.grid.iter().filter(|&&c| c == 1).count();
        let doors = g.grid.iter().filter(|&&c| c == 2).count();
        assert!(walls > 40, "walls = {walls}");
        assert_eq!(g.doors.len(), doors);
        assert_eq!(g.enemies.len(), 4, "3 spiders + 1 wuson");
        assert_eq!(g.w, 15);
        assert_eq!(g.h, 15);
    }

    #[test]
    fn solid_at_respects_walls_and_open_doors() {
        let g = Game::new(dummy_textures(), empty_assets(), empty_assets());
        // (0,0) is the outer border wall cell.
        assert!(solid_at(&g.grid, g.w, g.h, &g.doors, 1.0, 1.0));
        // Player spawn cell (1,1) is a floor.
        assert!(!solid_at(&g.grid, g.w, g.h, &g.doors, g.camera.x, g.camera.z));
    }

    #[test]
    fn ray_circle_hits_target() {
        // Ray along +z from origin hits a circle at (0, 5) radius 1.
        let d = ray_circle(0.0, 0.0, 0.0, 1.0, 0.0, 5.0, 1.0);
        assert!(d.is_some());
        assert!((d.unwrap() - 4.0).abs() < 0.01);
        // Ray pointing away misses.
        assert!(ray_circle(0.0, 0.0, 0.0, -1.0, 0.0, 5.0, 1.0).is_none());
    }
}
