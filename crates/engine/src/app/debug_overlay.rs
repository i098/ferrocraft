//! The F3 screen: `GuiOverlayDebug.call()` and `getDebugInfoRight()` filled from the world and the renderer.

use super::App;
use gui::debug::{left_lines, DebugInfo};
use gui::debug_data::{
    biome_name, entities_line, fps_line, light_line, local_difficulty_line, renders_line,
    right_lines, FpsInfo, RightInfo,
};
use render::{cull, cull::Frustum, Camera, EntityDraw};
use std::collections::HashSet;

/// Mesh workers of the renderer, vanilla's free chunk builders (`aB`).
const MESH_WORKERS: usize = 2;

/// Both F3 columns.
pub(super) type Columns = (Vec<String>, Vec<String>);

impl App {
    /// The F3 columns, `None` while F3 is off or there is no player. `draws` are the entity draws of the frame.
    pub(super) fn debug_lines(
        &self,
        frac: f64,
        cam: &Camera,
        draws: &[EntityDraw],
    ) -> Option<Columns> {
        if !self.show_debug {
            return None;
        }
        let p = self.game.player.as_ref()?;
        let [x, y, z] = p.pos.map(|v| v.floor() as i32);
        let total = self.game.entities.len() + 1;
        let (sections, rendered) = self.visible_view(cam, (x >> 4, z >> 4), draws);
        let left = left_lines(&DebugInfo {
            fps_line: self.debug_fps(),
            renders: self.debug_renders(&sections),
            entities: entities_line(rendered.min(total), total, 0),
            particles: format!("P: {}. T: All: {total}", self.particles.len()),
            provider: format!(
                "MultiplayerChunkCache: {}, {}",
                self.game.store.len(),
                self.game.store.len()
            ),
            pos: p.pos,
            yaw: p.yaw,
            pitch: self.game.pitch,
            loaded_extra: self.debug_location(x, y, z),
            looking_at: self.debug_looking_at(frac),
            reduced: self.reduced_debug(),
        });
        Some((left, self.debug_right()))
    }

    fn debug_fps(&self) -> String {
        let s = &self.ui.settings;
        fps_line(&FpsInfo {
            fps: self.fps.1,
            chunk_updates: self.chunk_updates.1,
            max_fps: s.max_fps,
            vsync: s.vsync,
            fancy: s.fancy_graphics,
            clouds: s.clouds,
        })
    }

    fn debug_looking_at(&self, frac: f64) -> Option<[i32; 3]> {
        let (eye, yaw, pitch, id, vehicle) = self.view_ray(frac)?;
        let (hit, _) = self.game.block_aim(eye, yaw, pitch, id, vehicle)?;
        Some(hit.pos)
    }

    /// `setupTerrain` and `renderEntities`: the sections that survive frustum and cave culling, and how many
    /// entities `RenderManager.shouldRender` accepts inside them.
    fn visible_view(
        &self,
        cam: &Camera,
        feet: (i32, i32),
        draws: &[EntityDraw],
    ) -> (HashSet<(i32, i32, i32)>, usize) {
        let Some(r) = self.renderer.as_ref() else {
            return (HashSet::new(), 0);
        };
        let d = self.ui.settings.render_distance;
        let (w, h) = r.size();
        let far = f64::from(d) * 16.0 * std::f64::consts::SQRT_2;
        let frustum = Frustum::new(cam, w as f32 / h.max(1) as f32, far);
        let store = &self.game.store;
        let facings = |cx: i32, sy: i32, cz: i32| {
            let at = cam.pos.map(|v| v.floor() as i32);
            let opaque = |x: usize, y: usize, z: usize| {
                world::dig::opaque_cube(store.get_block(
                    cx * 16 + x as i32,
                    sy * 16 + y as i32,
                    cz * 16 + z as i32,
                ))
            };
            cull::visible_facings(
                opaque,
                [at[0] & 15, at[1] & 15, at[2] & 15].map(|v| v as usize),
            )
        };
        let sections = cull::visible_sections(cam, &frustum, feet, d, facings, |cx, sy, cz| {
            r.section_visibility(cx, sy, cz)
        });
        let eye = cam.pos;
        let rendered = self
            .game
            .entities
            .iter()
            .zip(draws)
            .filter(|(_, d)| entity_in_view(d, eye, &sections, &frustum))
            .count();
        (sections, rendered)
    }

    /// `getDebugInfoRenders`: the visible sections that hold blocks over all sections of the view.
    fn debug_renders(&self, visible: &HashSet<(i32, i32, i32)>) -> String {
        let d = self.ui.settings.render_distance;
        let rendered = visible
            .iter()
            .filter(|&&(cx, sy, cz)| {
                self.game
                    .store
                    .get(cx, cz)
                    .is_some_and(|c| c.section(sy as usize).is_some())
            })
            .count();
        let side = (2 * d + 1) as usize;
        renders_line(rendered, side * side * 16, d, MESH_WORKERS)
    }

    /// `Biome`, `Light` and `Local Difficulty` when the feet block is in Y 0..256 and its chunk is loaded.
    fn debug_location(&self, x: i32, y: i32, z: i32) -> Vec<String> {
        let chunk = self.game.store.get(x >> 4, z >> 4);
        let (true, Some(chunk)) = ((0..256).contains(&y), chunk) else {
            return Vec::new();
        };
        let [sky, block] = self.renderer.as_ref().map_or([0.0; 2], |r| {
            r.light_at([f64::from(x), f64::from(y), f64::from(z)])
        });
        let level = |v: f32| (v * 15.0).round() as u8;
        vec![
            format!(
                "Biome: {}",
                biome_name(chunk.biome((x & 15) as usize, (z & 15) as usize))
            ),
            light_line(level(sky), level(block)),
            local_difficulty_line(self.ui.difficulty.unwrap_or(0), self.game.world_time),
        ]
    }

    /// `getDebugInfoRight` without the block state lines of the block under the crosshair.
    fn debug_right(&self) -> Vec<String> {
        let (max, total, used) = process_memory();
        let gpu = self.renderer.as_ref().map(|r| r.gpu_info().clone());
        let gpu = gpu.unwrap_or_default();
        right_lines(&RightInfo {
            java: "Ferrocraft (Rust)".to_owned(),
            bits: usize::BITS,
            max,
            total,
            used,
            cpu: CPU.clone(),
            display: self.renderer.as_ref().map_or((0, 0), |r| r.size()),
            vendor: gpu.vendor,
            renderer: gpu.renderer,
            version: gpu.version,
        })
    }
}

/// `OpenGlHelper.getCpu`: `"<cores>x <model>"` with runs of spaces collapsed.
static CPU: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
    let info = std::fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
    let model = info
        .lines()
        .find_map(|l| l.strip_prefix("model name"))
        .and_then(|l| l.split_once(':'))
        .map(|(_, m)| m.split_whitespace().collect::<Vec<_>>().join(" "));
    match model {
        Some(m) => format!(
            "{}x {m}",
            std::thread::available_parallelism().map_or(1, usize::from)
        ),
        None => "<unknown>".to_owned(),
    }
});

/// `(maxMemory, totalMemory, used)` in bytes: the machine memory, the process data size and its resident size.
/// Zero where `/proc` is missing. shortcut: 4 KiB pages assumed, read the page size when a platform needs it.
fn process_memory() -> (u64, u64, u64) {
    const PAGE: u64 = 4096;
    let kb = |text: &str, key: &str| {
        text.lines()
            .find_map(|l| l.strip_prefix(key))
            .and_then(|v| v.split_whitespace().next()?.parse::<u64>().ok())
            .unwrap_or(0)
            * 1024
    };
    let max = kb(
        &std::fs::read_to_string("/proc/meminfo").unwrap_or_default(),
        "MemTotal:",
    );
    let statm = std::fs::read_to_string("/proc/self/statm").unwrap_or_default();
    let pages: Vec<u64> = statm
        .split_whitespace()
        .filter_map(|v| v.parse().ok())
        .collect();
    let at = |i: usize| pages.get(i).copied().unwrap_or(0) * PAGE;
    let used = at(1);
    (max, at(5).max(used).min(max.max(used)), used)
}

/// `RenderManager.shouldRender` for one entity: in range, in a visible section and inside the frustum.
fn entity_in_view(
    d: &EntityDraw,
    eye: [f64; 3],
    sections: &HashSet<(i32, i32, i32)>,
    frustum: &Frustum,
) -> bool {
    let (w, h) = (f64::from(d.size.0), f64::from(d.size.1));
    let reach = (2.0 * w + h) / 3.0 * 64.0;
    let dist: f64 = (0..3).map(|a| (d.pos[a] - eye[a]).powi(2)).sum();
    let min = [
        d.pos[0] - w / 2.0 - 0.5,
        d.pos[1] - 0.5,
        d.pos[2] - w / 2.0 - 0.5,
    ];
    let max = [
        d.pos[0] + w / 2.0 + 0.5,
        d.pos[1] + h + 0.5,
        d.pos[2] + w / 2.0 + 0.5,
    ];
    let section = (
        (d.pos[0].floor() as i32) >> 4,
        ((d.pos[1].floor() as i32) >> 4).clamp(0, 15),
        (d.pos[2].floor() as i32) >> 4,
    );
    dist < reach * reach && sections.contains(&section) && frustum.contains(min, max)
}
