//! Text of the F3 lines that `Minecraft`, `RenderGlobal`, `DifficultyInstance` and `GuiOverlayDebug.getDebugInfoRight` build.

/// `BiomeGenBase.biomeName` of 1.8.9; unknown ids read as plains, like `BiomeGenBase.getBiome`.
pub fn biome_name(id: u8) -> &'static str {
    match id {
        0 => "Ocean",
        2 => "Desert",
        3 => "Extreme Hills",
        4 => "Forest",
        5 => "Taiga",
        6 => "Swampland",
        7 => "River",
        8 => "Hell",
        9 => "The End",
        10 => "FrozenOcean",
        11 => "FrozenRiver",
        12 => "Ice Plains",
        13 => "Ice Mountains",
        14 => "MushroomIsland",
        15 => "MushroomIslandShore",
        16 => "Beach",
        17 => "DesertHills",
        18 => "ForestHills",
        19 => "TaigaHills",
        20 => "Extreme Hills Edge",
        21 => "Jungle",
        22 => "JungleHills",
        23 => "JungleEdge",
        24 => "Deep Ocean",
        25 => "Stone Beach",
        26 => "Cold Beach",
        27 => "Birch Forest",
        28 => "Birch Forest Hills",
        29 => "Roofed Forest",
        30 => "Cold Taiga",
        31 => "Cold Taiga Hills",
        32 => "Mega Taiga",
        33 => "Mega Taiga Hills",
        34 => "Extreme Hills+",
        35 => "Savanna",
        36 => "Savanna Plateau",
        37 => "Mesa",
        38 => "Mesa Plateau F",
        39 => "Mesa Plateau",
        129 => "Sunflower Plains",
        130 => "Desert M",
        131 => "Extreme Hills M",
        132 => "Flower Forest",
        133 => "Taiga M",
        134 => "Swampland M",
        140 => "Ice Plains Spikes",
        149 => "Jungle M",
        151 => "JungleEdge M",
        155 => "Birch Forest M",
        156 => "Birch Forest Hills M",
        157 => "Roofed Forest M",
        158 => "Cold Taiga M",
        160 => "Mega Spruce Taiga",
        161 => "Redwood Taiga Hills M",
        162 => "Extreme Hills+ M",
        163 => "Savanna M",
        164 => "Savanna Plateau M",
        165 => "Mesa (Bryce)",
        166 => "Mesa Plateau F M",
        167 => "Mesa Plateau M",
        _ => "Plains",
    }
}

/// `DifficultyInstance.calculateAdditionalDifficulty` for a client world: the chunk's inhabited time is 0,
/// `difficulty` is the `EnumDifficulty` id (0 peaceful to 3 hard).
pub fn additional_difficulty(difficulty: u8, world_time: i64, moon_factor: f32) -> f32 {
    if difficulty == 0 {
        return 0.0;
    }
    let clamp = |v: f32, lo: f32, hi: f32| v.max(lo).min(hi);
    let time = clamp((world_time as f32 + -72000.0) / 1_440_000.0, 0.0, 1.0) * 0.25;
    let base = 0.75 + time;
    let mut inhabited =
        clamp(0.0 / 3_600_000.0, 0.0, 1.0) * if difficulty == 3 { 1.0 } else { 0.75 };
    inhabited += clamp(moon_factor * 0.25, 0.0, time);
    if difficulty == 1 {
        inhabited *= 0.5;
    }
    f32::from(difficulty) * (base + inhabited)
}

/// `WorldProvider.getMoonPhase` then `moonPhaseFactors`.
pub fn moon_factor(world_time: i64) -> f32 {
    const FACTORS: [f32; 8] = [1.0, 0.75, 0.5, 0.25, 0.0, 0.25, 0.5, 0.75];
    FACTORS[((world_time / 24000 % 8 + 8) % 8) as usize]
}

/// `Local Difficulty: %.2f (Day %d)`; Java rounds a tie of the exact binary value half up.
pub fn local_difficulty_line(difficulty: u8, world_time: i64) -> String {
    let v = f64::from(additional_difficulty(
        difficulty,
        world_time,
        moon_factor(world_time),
    ));
    let tie = (v * 8.0).fract() == 0.0 && (v * 1000.0).round() as i64 % 10 == 5;
    let shown = if tie {
        (v * 100.0 + 0.5).floor() / 100.0
    } else {
        v
    };
    format!("Local Difficulty: {shown:.2} (Day {})", world_time / 24000)
}

/// `Light: %d (%d sky, %d block)`: `getLightSubtracted(pos, 0)` is the larger of the two.
pub fn light_line(sky: u8, block: u8) -> String {
    format!("Light: {} ({sky} sky, {block} block)", sky.max(block))
}

/// Settings the `Minecraft.debug` line reads.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FpsInfo {
    pub fps: u32,
    pub chunk_updates: u32,
    pub max_fps: i32,
    pub vsync: bool,
    pub fancy: bool,
    /// `GameSettings.clouds`: 0 off, 1 fast, 2 fancy.
    pub clouds: i32,
}

/// `Minecraft.debug`; 260 is `FRAMERATE_LIMIT`'s maximum, shown as `inf`.
pub fn fps_line(i: &FpsInfo) -> String {
    let limit = if i.max_fps == 260 {
        "inf".to_owned()
    } else {
        i.max_fps.to_string()
    };
    format!(
        "{} fps ({} chunk update{}) T: {limit}{}{}{}",
        i.fps,
        i.chunk_updates,
        if i.chunk_updates == 1 { "" } else { "s" },
        if i.vsync { " vsync" } else { "" },
        if i.fancy { "" } else { " fast" },
        match i.clouds {
            0 => "",
            1 => " fast-clouds",
            _ => " fancy-clouds",
        },
    )
}

/// `RenderGlobal.getDebugInfoRenders`: `C: %d/%d %sD: %d, pC: %03d, pU: %1d, aB: %1d` with `(s) ` for
/// `renderChunksMany`. Ferrocraft meshes on workers, so `pC` and `pU` read 0 and `aB` is the worker count.
pub fn renders_line(rendered: usize, total: usize, distance: i32, builders: usize) -> String {
    format!("C: {rendered}/{total} (s) D: {distance}, pC: 000, pU: 0, aB: {builders}")
}

/// `RenderGlobal.getDebugInfoEntities`: `E: rendered/total, B: hidden, I: invisible`.
pub fn entities_line(rendered: usize, total: usize, hidden: usize) -> String {
    format!(
        "E: {rendered}/{total}, B: {hidden}, I: {}",
        total.saturating_sub(hidden + rendered)
    )
}

/// What the right column reports, `GuiOverlayDebug.getDebugInfoRight`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RightInfo {
    pub java: String,
    pub bits: u32,
    /// Bytes: `maxMemory`, `totalMemory`, `totalMemory - freeMemory`.
    pub max: u64,
    pub total: u64,
    pub used: u64,
    pub cpu: String,
    pub display: (u32, u32),
    pub vendor: String,
    pub renderer: String,
    pub version: String,
}

/// Java's `% 2d` prints a space before a non-negative number.
fn spaced(v: u64) -> String {
    format!(" {v}")
}

/// `getDebugInfoRight` without the block state lines.
pub fn right_lines(r: &RightInfo) -> Vec<String> {
    let mb = |b: u64| b / 1024 / 1024;
    let max = r.max.max(1);
    vec![
        format!("Java: {} {}bit", r.java, r.bits),
        format!(
            "Mem: {}% {:03}/{:03}MB",
            spaced(r.used * 100 / max),
            mb(r.used),
            mb(r.max)
        ),
        format!(
            "Allocated: {}% {:03}MB",
            spaced(r.total * 100 / max),
            mb(r.total)
        ),
        String::new(),
        format!("CPU: {}", r.cpu),
        String::new(),
        format!("Display: {}x{} ({})", r.display.0, r.display.1, r.vendor),
        r.renderer.clone(),
        r.version.clone(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fps_line_reads_settings() {
        let i = FpsInfo {
            fps: 144,
            chunk_updates: 1,
            max_fps: 260,
            vsync: true,
            fancy: false,
            clouds: 1,
        };
        assert_eq!(
            fps_line(&i),
            "144 fps (1 chunk update) T: inf vsync fast fast-clouds"
        );
        let j = FpsInfo {
            chunk_updates: 3,
            max_fps: 120,
            fancy: true,
            clouds: 2,
            ..i
        };
        assert_eq!(
            fps_line(&FpsInfo { vsync: false, ..j }),
            "144 fps (3 chunk updates) T: 120 fancy-clouds"
        );
    }

    #[test]
    fn difficulty_follows_vanilla_formula() {
        // Normal, day 0: 2 * (0.75 + 0 + clamp(1.0 * 0.25, 0, 0)) = 1.5.
        assert_eq!(
            local_difficulty_line(2, 6000),
            "Local Difficulty: 1.50 (Day 0)"
        );
        // Peaceful is 0 whatever the time.
        assert_eq!(
            local_difficulty_line(0, 9_000_000),
            "Local Difficulty: 0.00 (Day 375)"
        );
        // Hard, long played, new moon (day 68): 3 * (0.75 + 0.25).
        assert_eq!(
            local_difficulty_line(3, 68 * 24000),
            "Local Difficulty: 3.00 (Day 68)"
        );
        // Easy halves the moon share: 1 * (1.0 + 0.5 * 0.1875) at the 7th phase (day 63).
        assert_eq!(
            local_difficulty_line(1, 63 * 24000),
            "Local Difficulty: 1.09 (Day 63)"
        );
    }

    #[test]
    fn lines_use_vanilla_formats() {
        assert_eq!(light_line(15, 3), "Light: 15 (15 sky, 3 block)");
        assert_eq!(entities_line(3, 20, 4), "E: 3/20, B: 4, I: 13");
        assert_eq!(
            renders_line(130, 625 * 16, 12, 4),
            "C: 130/10000 (s) D: 12, pC: 000, pU: 0, aB: 4"
        );
        assert_eq!(biome_name(1), "Plains");
        assert_eq!(biome_name(132), "Flower Forest");
        let r = RightInfo {
            java: "Rust".into(),
            bits: 64,
            max: 4096 << 20,
            total: 512 << 20,
            used: 205 << 20,
            cpu: "8x CPU".into(),
            display: (854, 480),
            vendor: "V".into(),
            renderer: "R".into(),
            version: "1".into(),
        };
        assert_eq!(
            right_lines(&r),
            [
                "Java: Rust 64bit",
                "Mem:  5% 205/4096MB",
                "Allocated:  12% 512MB",
                "",
                "CPU: 8x CPU",
                "",
                "Display: 854x480 (V)",
                "R",
                "1"
            ]
        );
    }
}
