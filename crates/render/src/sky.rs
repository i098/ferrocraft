//! Day/night sky, fog, sun, moon, stars and clouds, ported from vanilla 1.8.9
//! (`World.getSkyColor`, `RenderGlobal.renderSky`, `RenderGlobal.renderClouds`).

use crate::assets::{decode_png, AssetSource, Image};
use crate::mesh::Vertex;
use std::f32::consts::PI;

/// What the world tells the renderer about time and weather.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Environment {
    /// Time of day in ticks (Time Update packet, `world_time % 24000` is used).
    pub world_time: i64,
    /// Ticks since the world loaded, for cloud drift.
    pub ticks: u64,
    /// Fraction of the current tick, `0..1`.
    pub partial: f32,
    pub rain: f32,
    pub thunder: f32,
    pub render_distance_chunks: u32,
}

impl Default for Environment {
    /// Noon, clear weather, 8 chunks.
    fn default() -> Self {
        Environment {
            world_time: 6000,
            ticks: 0,
            partial: 0.0,
            rain: 0.0,
            thunder: 0.0,
            render_distance_chunks: 8,
        }
    }
}

/// Everything derived from `Environment` for one frame.
#[derive(Clone, Copy, Debug)]
pub struct SkyState {
    pub angle: f32,
    pub sky: [f32; 3],
    pub fog: [f32; 3],
    pub cloud: [f32; 3],
    pub sun_brightness: f32,
    pub star_alpha: f32,
    pub fog_start: f32,
    pub fog_end: f32,
}

/// `WorldProvider.calculateCelestialAngle`: 0 at noon, 0.5 at midnight.
pub fn celestial_angle(time: i64, partial: f32) -> f32 {
    let t = time.rem_euclid(24000) as f32;
    let mut f = (t + partial) / 24000.0 - 0.25;
    if f < 0.0 {
        f += 1.0;
    }
    if f > 1.0 {
        f -= 1.0;
    }
    let f1 = f;
    let f = 1.0 - ((f * PI).cos() + 1.0) / 2.0;
    f1 + (f - f1) / 3.0
}

fn hsb_to_rgb(h: f32, s: f32, b: f32) -> [f32; 3] {
    let h6 = (h - h.floor()) * 6.0;
    let f = h6 - h6.floor();
    let (p, q, t) = (b * (1.0 - s), b * (1.0 - s * f), b * (1.0 - s * (1.0 - f)));
    match h6 as i32 {
        0 => [b, t, p],
        1 => [q, b, p],
        2 => [p, b, t],
        3 => [p, q, b],
        4 => [t, p, b],
        _ => [b, p, q],
    }
}

fn gray(c: [f32; 3]) -> f32 {
    c[0] * 0.3 + c[1] * 0.59 + c[2] * 0.11
}

/// Blend `c` toward its gray value by `1 - keep`, as the vanilla weather code does.
fn desaturate(c: [f32; 3], gray_scale: f32, keep: f32) -> [f32; 3] {
    let g = gray(c) * gray_scale;
    c.map(|v| v * keep + g * (1.0 - keep))
}

impl Environment {
    pub fn sky_state(&self) -> SkyState {
        let angle = celestial_angle(self.world_time, self.partial);
        let day = ((angle * PI * 2.0).cos() * 2.0 + 0.5).clamp(0.0, 1.0);

        // Plains temperature 0.8 (`BiomeGenBase.getSkyColorByTemp`).
        let t = (0.8f32 / 3.0).clamp(-1.0, 1.0);
        let mut sky = hsb_to_rgb(0.622_222_24 - t * 0.05, 0.5 + t * 0.1, 1.0).map(|c| c * day);
        if self.rain > 0.0 {
            sky = desaturate(sky, 0.6, 1.0 - self.rain * 0.75);
        }
        if self.thunder > 0.0 {
            sky = desaturate(sky, 0.2, 1.0 - self.thunder * 0.75);
        }

        let mut fog = [
            0.752_941_2 * (day * 0.94 + 0.06),
            0.847_058_83 * (day * 0.94 + 0.06),
            day * 0.91 + 0.09,
        ];
        let f = 1.0 - (0.25 + 0.75 * self.render_distance_chunks as f32 / 32.0).powf(0.25);
        fog = [0, 1, 2].map(|i| fog[i] + (sky[i] - fog[i]) * f);
        if self.rain > 0.0 {
            let (a, b) = (1.0 - self.rain * 0.5, 1.0 - self.rain * 0.4);
            fog = [fog[0] * a, fog[1] * a, fog[2] * b];
        }
        if self.thunder > 0.0 {
            fog = fog.map(|c| c * (1.0 - self.thunder * 0.5));
        }

        let mut cloud = [day * 0.9 + 0.1, day * 0.9 + 0.1, day * 0.85 + 0.15];
        if self.rain > 0.0 {
            cloud = desaturate(cloud, 0.6, 1.0 - self.rain * 0.95);
        }
        if self.thunder > 0.0 {
            cloud = desaturate(cloud, 0.2, 1.0 - self.thunder * 0.95);
        }

        // `World.getSunBrightness`
        let mut sun = 1.0 - (1.0 - ((angle * PI * 2.0).cos() * 2.0 + 0.2)).clamp(0.0, 1.0);
        sun *= 1.0 - self.rain * 5.0 / 16.0;
        sun *= 1.0 - self.thunder * 5.0 / 16.0;

        // `World.getStarBrightness`
        let s = (1.0 - ((angle * PI * 2.0).cos() * 2.0 + 0.25)).clamp(0.0, 1.0);
        let star_alpha = s * s * 0.5 * (1.0 - self.rain);

        let far = (self.render_distance_chunks * 16) as f32;
        SkyState {
            angle,
            sky,
            fog,
            cloud,
            sun_brightness: sun * 0.8 + 0.2,
            star_alpha,
            fog_start: far * 0.75,
            fog_end: far,
        }
    }

    pub fn moon_phase(&self) -> u32 {
        (self.world_time.div_euclid(24000).rem_euclid(8)) as u32
    }
}

/// Environment texture: clouds on top, then sun, moon phases and one white tile.
pub const ENV_W: u32 = 256;
pub const ENV_H: u32 = 360;
const SUN_Y: f32 = 256.0;
const MOON_Y: f32 = 288.0;
const WHITE_Y: f32 = 352.0;

pub struct EnvTexture {
    pub rgba: Vec<u8>,
    /// Whether `clouds.png` was found (clouds are not drawn otherwise).
    pub has_clouds: bool,
    /// `true` where a cloud texel is opaque, row-major 256x256.
    pub clouds: Vec<bool>,
}

fn paste(dst: &mut [u8], x0: usize, y0: usize, img: &Image) {
    for y in 0..img.height.min(ENV_H as usize - y0) {
        for x in 0..img.width.min(ENV_W as usize - x0) {
            let s = (y * img.width + x) * 4;
            let d = ((y0 + y) * ENV_W as usize + x0 + x) * 4;
            dst[d..d + 4].copy_from_slice(&img.rgba[s..s + 4]);
        }
    }
}

impl EnvTexture {
    pub fn load(source: Option<&mut AssetSource>) -> EnvTexture {
        let mut rgba = vec![255u8; (ENV_W * ENV_H * 4) as usize];
        // Everything but the white tile starts transparent so missing textures draw nothing.
        for px in rgba[..(WHITE_Y as usize * ENV_W as usize * 4)]
            .as_chunks_mut::<4>()
            .0
        {
            px.copy_from_slice(&[255, 255, 255, 0]);
        }
        let mut clouds = vec![false; 256 * 256];
        let mut has_clouds = false;
        if let Some(src) = source {
            let mut load = |name: &str| {
                src.read(&format!("assets/minecraft/textures/environment/{name}.png"))
                    .and_then(|b| decode_png(&b))
            };
            if let Some(img) = load("clouds").filter(|i| i.width == 256 && i.height == 256) {
                for (c, px) in clouds.iter_mut().zip(img.rgba.as_chunks::<4>().0) {
                    *c = px[3] > 0;
                }
                has_clouds = true;
            }
            if let Some(img) = load("sun") {
                paste(&mut rgba, 0, SUN_Y as usize, &img);
            }
            if let Some(img) = load("moon_phases") {
                paste(&mut rgba, 0, MOON_Y as usize, &img);
            }
        }
        EnvTexture {
            rgba,
            has_clouds,
            clouds,
        }
    }
}

fn env_uv(x: f32, y: f32) -> [f32; 2] {
    [x / ENV_W as f32, y / ENV_H as f32]
}

fn white_uv() -> [f32; 2] {
    env_uv(4.0, WHITE_Y + 4.0)
}

#[derive(Default)]
pub struct SkyGeometry {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

impl SkyGeometry {
    fn quad(&mut self, v: [([f32; 3], [f32; 2], [f32; 4]); 4]) {
        let base = self.vertices.len() as u32;
        for (pos, uv, color) in v {
            self.vertices.push(Vertex {
                pos,
                uv,
                color,
                light: [1.0, 1.0],
            });
        }
        self.indices.extend([0, 1, 2, 0, 2, 3].map(|i| base + i));
    }

    fn tri_fan(&mut self, center: ([f32; 3], [f32; 4]), rim: &[([f32; 3], [f32; 4])]) {
        let uv = white_uv();
        let base = self.vertices.len() as u32;
        let mk = |p: ([f32; 3], [f32; 4])| Vertex {
            pos: p.0,
            uv,
            color: p.1,
            light: [1.0, 1.0],
        };
        self.vertices.push(mk(center));
        self.vertices.extend(rim.iter().map(|p| mk(*p)));
        for i in 1..rim.len() as u32 {
            self.indices.extend([base, base + i, base + i + 1]);
        }
    }

    fn solid_quad(&mut self, corners: [[f32; 3]; 4], color: [f32; 4]) {
        let uv = white_uv();
        self.quad(corners.map(|p| (p, uv, color)));
    }
}

/// `glRotatef(deg, axis)` applied to a vertex; `axis` is 0 (x), 1 (y) or 2 (z).
fn rot(v: [f32; 3], deg: f32, axis: usize) -> [f32; 3] {
    let (s, c) = deg.to_radians().sin_cos();
    match axis {
        0 => [v[0], v[1] * c - v[2] * s, v[1] * s + v[2] * c],
        1 => [v[0] * c + v[2] * s, v[1], -v[0] * s + v[2] * c],
        _ => [v[0] * c - v[1] * s, v[0] * s + v[1] * c, v[2]],
    }
}

fn add(a: [f32; 3], b: [f64; 3]) -> [f32; 3] {
    [a[0] + b[0] as f32, a[1] + b[1] as f32, a[2] + b[2] as f32]
}

/// Sky disc, below-horizon disc, sunrise glow and stars, with the camera at `eye`.
/// `blend` geometry is alpha blended, `solid` geometry replaces the clear color.
pub struct SkyLayers {
    pub solid: SkyGeometry,
    pub blend: SkyGeometry,
    pub additive: SkyGeometry,
}

pub fn build_sky(env: &Environment, s: &SkyState, eye: [f64; 3]) -> SkyLayers {
    let mut solid = SkyGeometry::default();
    let mut blend = SkyGeometry::default();
    let mut additive = SkyGeometry::default();
    let sky = [s.sky[0], s.sky[1], s.sky[2], 1.0];
    let r = 512.0;
    solid.solid_quad(
        [[-r, 16.0, -r], [-r, 16.0, r], [r, 16.0, r], [r, 16.0, -r]].map(|p| add(p, eye)),
        sky,
    );
    if eye[1] < 63.0 {
        solid.solid_quad(
            [
                [-r, -16.0, -r],
                [r, -16.0, -r],
                [r, -16.0, r],
                [-r, -16.0, r],
            ]
            .map(|p| add(p, eye)),
            [0.0, 0.0, 0.0, 1.0],
        );
    }

    // Sunrise and sunset glow (`WorldProvider.calcSunriseSunsetColors`).
    let c = (s.angle * PI * 2.0).cos();
    if (-0.4..=0.4).contains(&c) {
        let f3 = c / 0.4 * 0.5 + 0.5;
        let mut a = 1.0 - (1.0 - (f3 * PI).sin()) * 0.99;
        a *= a;
        let col = [f3 * 0.3 + 0.7, f3 * f3 * 0.7 + 0.2, 0.2, a];
        let flip = (s.angle * PI * 2.0).sin() < 0.0;
        let place = |v: [f32; 3]| {
            let mut v = rot(v, 90.0, 0);
            if flip {
                v = rot(v, 180.0, 2);
            }
            add(rot(v, 90.0, 2), eye)
        };
        let rim: Vec<_> = (0..=16)
            .map(|l| {
                let f6 = l as f32 * PI * 2.0 / 16.0;
                let (f7, f8) = (f6.sin(), f6.cos());
                (
                    place([f7 * 120.0, f8 * 120.0, -f8 * 40.0 * a]),
                    [col[0], col[1], col[2], 0.0],
                )
            })
            .collect();
        blend.tri_fan((place([0.0, 100.0, 0.0]), col), &rim);
    }

    let sky_rot = |v: [f32; 3]| add(rot(rot(v, s.angle * 360.0, 0), -90.0, 1), eye);
    if s.star_alpha > 0.0 {
        stars(&mut blend, s.star_alpha, sky_rot);
    }

    // Sun and moon, additive like vanilla's `SRC_ALPHA, ONE`.
    let white = [1.0, 1.0, 1.0, 1.0 - env.rain];
    let sun = 30.0;
    additive.quad([
        (sky_rot([-sun, 100.0, -sun]), env_uv(0.0, SUN_Y), white),
        (sky_rot([sun, 100.0, -sun]), env_uv(32.0, SUN_Y), white),
        (
            sky_rot([sun, 100.0, sun]),
            env_uv(32.0, SUN_Y + 32.0),
            white,
        ),
        (
            sky_rot([-sun, 100.0, sun]),
            env_uv(0.0, SUN_Y + 32.0),
            white,
        ),
    ]);
    let k = env.moon_phase();
    let (col, row) = ((k % 4) as f32, (k / 4 % 2) as f32);
    let (u0, u1) = (col * 32.0, (col + 1.0) * 32.0);
    let (v0, v1) = (MOON_Y + row * 32.0, MOON_Y + (row + 1.0) * 32.0);
    let moon = 20.0;
    additive.quad([
        (sky_rot([-moon, -100.0, moon]), env_uv(u1, v1), white),
        (sky_rot([moon, -100.0, moon]), env_uv(u0, v1), white),
        (sky_rot([moon, -100.0, -moon]), env_uv(u0, v0), white),
        (sky_rot([-moon, -100.0, -moon]), env_uv(u1, v0), white),
    ]);
    SkyLayers {
        solid,
        blend,
        additive,
    }
}

/// `java.util.Random` (48-bit LCG), enough for vanilla's star field.
struct JavaRandom(i64);

impl JavaRandom {
    fn new(seed: i64) -> Self {
        JavaRandom((seed ^ 0x5DEECE66D) & ((1 << 48) - 1))
    }
    fn next(&mut self, bits: u32) -> i32 {
        self.0 = (self.0.wrapping_mul(0x5DEECE66D).wrapping_add(0xB)) & ((1 << 48) - 1);
        (self.0 >> (48 - bits)) as i32
    }
    fn next_float(&mut self) -> f32 {
        self.next(24) as f32 / (1 << 24) as f32
    }
    fn next_double(&mut self) -> f64 {
        (((self.next(26) as i64) << 27) + self.next(27) as i64) as f64 / (1u64 << 53) as f64
    }
}

/// `RenderGlobal.renderStars`: 1500 quads on a sphere of radius 100, seeded 10842.
fn stars(g: &mut SkyGeometry, alpha: f32, place: impl Fn([f32; 3]) -> [f32; 3]) {
    let mut rng = JavaRandom::new(10842);
    let uv = white_uv();
    let color = [1.0, 1.0, 1.0, alpha];
    for _ in 0..1500 {
        let mut d0 = (rng.next_float() * 2.0 - 1.0) as f64;
        let mut d1 = (rng.next_float() * 2.0 - 1.0) as f64;
        let mut d2 = (rng.next_float() * 2.0 - 1.0) as f64;
        let d3 = (0.15 + rng.next_float() * 0.1) as f64;
        let mut d4 = d0 * d0 + d1 * d1 + d2 * d2;
        if d4 >= 1.0 || d4 <= 0.01 {
            continue;
        }
        d4 = 1.0 / d4.sqrt();
        (d0, d1, d2) = (d0 * d4, d1 * d4, d2 * d4);
        let (d5, d6, d7) = (d0 * 100.0, d1 * 100.0, d2 * 100.0);
        let d8 = d0.atan2(d2);
        let (d9, d10) = (d8.sin(), d8.cos());
        let d11 = (d0 * d0 + d2 * d2).sqrt().atan2(d1);
        let (d12, d13) = (d11.sin(), d11.cos());
        let d14 = rng.next_double() * std::f64::consts::PI * 2.0;
        let (d15, d16) = (d14.sin(), d14.cos());
        let mut quad = [(([0.0; 3]), uv, color); 4];
        for (j, q) in quad.iter_mut().enumerate() {
            let d18 = (((j & 2) as f64) - 1.0) * d3;
            let d19 = ((((j + 1) & 2) as f64) - 1.0) * d3;
            let d20 = d18 * d16 - d19 * d15;
            let d21 = d19 * d16 + d18 * d15;
            let d22 = d20 * d12;
            let d23 = -d20 * d13;
            let d24 = d23 * d9 - d21 * d10;
            let d25 = d21 * d9 + d23 * d10;
            q.0 = place([(d5 + d24) as f32, (d6 + d22) as f32, (d7 + d25) as f32]);
        }
        g.quad(quad);
    }
}

/// Fancy clouds: a 12x4x12 block box per opaque `clouds.png` texel around the camera.
pub fn build_clouds(
    env: &Environment,
    s: &SkyState,
    eye: [f64; 3],
    texels: &[bool],
) -> SkyGeometry {
    let mut g = SkyGeometry::default();
    let shift = (env.ticks as f64 + env.partial as f64) * 0.03;
    let (tx0, tz0) = (
        ((eye[0] + shift) / 12.0).floor() as i64,
        (eye[2] / 12.0 + 0.33).floor() as i64,
    );
    let at = |tx: i64, tz: i64| texels[((tz.rem_euclid(256)) * 256 + tx.rem_euclid(256)) as usize];
    let (y0, y1) = (128.0f32 + 0.33, 128.0f32 + 0.33 + 4.0);
    let shade = [1.0, 0.7, 0.8, 0.8, 0.9, 0.9];
    let range = 12i64;
    for tz in tz0 - range..=tz0 + range {
        for tx in tx0 - range..=tx0 + range {
            if !at(tx, tz) {
                continue;
            }
            let x0 = (tx as f64 * 12.0 - shift) as f32;
            let z0 = ((tz as f64 - 0.33) * 12.0) as f32;
            let (x1, z1) = (x0 + 12.0, z0 + 12.0);
            // Faces in `UP, DOWN, NORTH, SOUTH, WEST, EAST` order; a face is dropped next to another cloud texel.
            let faces: [(bool, [[f32; 3]; 4]); 6] = [
                (
                    true,
                    [[x0, y1, z0], [x0, y1, z1], [x1, y1, z1], [x1, y1, z0]],
                ),
                (
                    true,
                    [[x0, y0, z0], [x1, y0, z0], [x1, y0, z1], [x0, y0, z1]],
                ),
                (
                    !at(tx, tz - 1),
                    [[x1, y0, z0], [x0, y0, z0], [x0, y1, z0], [x1, y1, z0]],
                ),
                (
                    !at(tx, tz + 1),
                    [[x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1]],
                ),
                (
                    !at(tx - 1, tz),
                    [[x0, y0, z0], [x0, y0, z1], [x0, y1, z1], [x0, y1, z0]],
                ),
                (
                    !at(tx + 1, tz),
                    [[x1, y0, z1], [x1, y0, z0], [x1, y1, z0], [x1, y1, z1]],
                ),
            ];
            for (f, (draw, corners)) in faces.iter().enumerate() {
                if *draw {
                    let k = shade[f];
                    let color = [s.cloud[0] * k, s.cloud[1] * k, s.cloud[2] * k, 0.8];
                    g.solid_quad(*corners, color);
                }
            }
        }
    }
    g
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn celestial_angle_matches_vanilla_at_key_times() {
        assert!(celestial_angle(6000, 0.0).abs() < 1e-6, "noon");
        assert!((celestial_angle(18000, 0.0) - 0.5).abs() < 1e-6, "midnight");
        // Sunrise (tick 0) is a little before 0.75 on the eased curve.
        let sunrise = celestial_angle(0, 0.0);
        assert!(
            (0.75..0.8).contains(&sunrise) || (0.95..1.0).contains(&sunrise),
            "{sunrise}"
        );
    }

    #[test]
    fn midday_sky_is_blue_and_midnight_is_dark() {
        let day = Environment::default().sky_state();
        assert!(day.sky[2] > day.sky[0] && day.sky[2] > 0.9);
        let night = Environment {
            world_time: 18000,
            ..Environment::default()
        }
        .sky_state();
        assert!(night.sky.iter().all(|c| *c < 0.05));
        assert!(night.sun_brightness < 0.3 && day.sun_brightness > 0.99);
        assert!(night.star_alpha > 0.4 && day.star_alpha == 0.0);
    }

    #[test]
    fn java_random_matches_the_jdk() {
        // new Random(42).nextInt(100) == 0 (first value), a well-known JDK result: next(31) % 100 style; check nextDouble sequence start.
        let mut r = JavaRandom::new(42);
        let d = r.next_double();
        assert!((d - 0.7275636800328681).abs() < 1e-12, "{d}");
    }
}
