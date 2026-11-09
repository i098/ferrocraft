//! `GuiOverlayDebug`: the F3 screen.

use crate::toolkit::{Gui, FONT_HEIGHT};

/// What the left column reports; the engine fills what it tracks.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DebugInfo {
    /// `mc.debug`, such as `"60 fps, 3 chunk updates"`.
    pub fps_line: String,
    /// `renderGlobal.getDebugInfoRenders()`.
    pub renders: String,
    /// `renderGlobal.getDebugInfoEntities()`.
    pub entities: String,
    /// `"P: <particles>. T: <entities>"`.
    pub particles: String,
    /// `getProviderName()`, such as `"MultiplayerChunkCache: 81, 81"`.
    pub provider: String,
    /// Feet position (`posX`, bounding box `minY`, `posZ`).
    pub pos: [f64; 3],
    pub yaw: f32,
    pub pitch: f32,
    /// `Biome`, `Light` and `Local Difficulty` lines when the block at the feet is loaded.
    pub loaded_extra: Vec<String>,
    /// The block the crosshair hits.
    pub looking_at: Option<[i32; 3]>,
    /// F3 with `reducedDebugInfo`: no position lines.
    pub reduced: bool,
}

/// `wrapAngleTo180_float`.
fn wrap_180(a: f32) -> f32 {
    let mut a = a % 360.0;
    if a >= 180.0 {
        a -= 360.0;
    }
    if a < -180.0 {
        a += 360.0;
    }
    a
}

/// `GuiOverlayDebug.call()`.
pub fn left_lines(d: &DebugInfo) -> Vec<String> {
    let mut l = vec![
        "Minecraft 1.8.9 (1.8.9/vanilla)".to_owned(),
        d.fps_line.clone(),
        d.renders.clone(),
        d.entities.clone(),
        d.particles.clone(),
        d.provider.clone(),
        String::new(),
    ];
    let (x, y, z) = (
        d.pos[0].floor() as i32,
        d.pos[1].floor() as i32,
        d.pos[2].floor() as i32,
    );
    if d.reduced {
        l.push(format!("Chunk-relative: {} {} {}", x & 15, y & 15, z & 15));
        return l;
    }
    let facing = ((d.yaw * 4.0 / 360.0 + 0.5).floor() as i32) & 3;
    let (name, toward) = [
        ("south", "Towards positive Z"),
        ("west", "Towards negative X"),
        ("north", "Towards negative Z"),
        ("east", "Towards positive X"),
    ][facing as usize];
    l.push(format!(
        "XYZ: {:.3} / {:.5} / {:.3}",
        d.pos[0], d.pos[1], d.pos[2]
    ));
    l.push(format!("Block: {x} {y} {z}"));
    l.push(format!(
        "Chunk: {} {} {} in {} {} {}",
        x & 15,
        y & 15,
        z & 15,
        x >> 4,
        y >> 4,
        z >> 4
    ));
    l.push(format!(
        "Facing: {name} ({toward}) ({:.1} / {:.1})",
        wrap_180(d.yaw),
        wrap_180(d.pitch)
    ));
    l.extend(d.loaded_extra.iter().cloned());
    if let Some([bx, by, bz]) = d.looking_at {
        l.push(format!("Looking at: {bx} {by} {bz}"));
    }
    l
}

/// `renderDebugInfoLeft` and `renderDebugInfoRight`: gray boxes behind each non-empty line.
pub fn draw(gui: &mut Gui, left: &[String], right: &[String]) {
    const BOX: u32 = 0x9050_5050;
    const TEXT: u32 = 0xE0E0E0;
    for (i, s) in left.iter().enumerate() {
        if s.is_empty() {
            continue;
        }
        let (w, y) = (gui.width(s), 2 + FONT_HEIGHT * i as i32);
        gui.fill(1, y - 1, 2 + w + 1, y + FONT_HEIGHT - 1, BOX);
        gui.text_plain(s, 2.0, y as f32, TEXT);
    }
    for (i, s) in right.iter().enumerate() {
        if s.is_empty() {
            continue;
        }
        let w = gui.width(s);
        let (x, y) = (gui.scale.width - 2 - w, 2 + FONT_HEIGHT * i as i32);
        gui.fill(x - 1, y - 1, x + w + 1, y + FONT_HEIGHT - 1, BOX);
        gui.text_plain(s, x as f32, y as f32, TEXT);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn position_lines_use_vanilla_formats() {
        let d = DebugInfo {
            pos: [-0.5, 64.0, 17.25],
            yaw: 450.0,
            pitch: 12.34,
            ..Default::default()
        };
        let l = left_lines(&d);
        assert_eq!(l[7], "XYZ: -0.500 / 64.00000 / 17.250");
        assert_eq!(l[8], "Block: -1 64 17");
        assert_eq!(l[9], "Chunk: 15 0 1 in -1 4 1");
        assert_eq!(l[10], "Facing: west (Towards negative X) (90.0 / 12.3)");
    }
}
