//! Vanilla 1.8.9 `SoundHandler` port: `sounds.json` events, category volumes, distance
//! attenuation, local block sounds and the music ticker, played through rodio.
//!
//! Skipped versus vanilla: stereo panning (sounds are centered) and streaming of `.ogg` files
//! from the jar (assets are read from the launcher's `assets/objects`).
pub mod blocks;
pub mod index;
pub mod music;

use blocks::{effect_sound, wire_pitch, BlockSound};
use index::{Category, SoundIndex};
use music::MusicTicker;
use rodio::{Decoder, DeviceSinkBuilder, MixerDeviceSink, Player, Source};
use std::collections::HashMap;
use std::fs::File;
use std::path::Path;

/// Vanilla caps concurrent non-stream sounds at 28.
const MAX_SOUNDS: usize = 28;

pub struct Sound {
    index: SoundIndex,
    sink: MixerDeviceSink,
    players: Vec<Player>,
    music: Option<Player>,
    ticker: MusicTicker,
    volumes: HashMap<Category, f32>,
    listener: [f64; 3],
    rng: u32,
}

impl Sound {
    /// `minecraft_dir` is the launcher dir holding `assets/`. Fails when assets or an output device are missing.
    pub fn new(minecraft_dir: &Path, version: &str) -> Result<Self, String> {
        let index = SoundIndex::load(minecraft_dir, version)?;
        let mut sink = DeviceSinkBuilder::open_default_sink().map_err(|e| e.to_string())?;
        sink.log_on_drop(false);
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(1, |d| d.subsec_nanos() | 1);
        let mut s = Self {
            index,
            sink,
            players: Vec::new(),
            music: None,
            ticker: MusicTicker::new(seed),
            volumes: Category::ALL.iter().map(|c| (*c, 1.0)).collect(),
            listener: [0.0; 3],
            rng: seed,
        };
        // Vanilla default: music slider at 100% is shipped as 1.0; keep it.
        s.volumes.insert(Category::Music, 1.0);
        Ok(s)
    }

    fn rand(&mut self) -> u32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        self.rng
    }

    pub fn set_listener(&mut self, pos: [f64; 3]) {
        self.listener = pos;
    }

    /// Slider value 0.0..=1.0, as `options.txt` `soundCategory_*`.
    pub fn set_volume(&mut self, cat: Category, v: f32) {
        self.volumes.insert(cat, v.clamp(0.0, 1.0));
        if cat == Category::Music || cat == Category::Master {
            let gain = self.gain(Category::Music);
            match &self.music {
                Some(p) if gain > 0.0 => p.set_volume(gain),
                Some(p) => {
                    p.stop();
                    self.music = None;
                }
                None => {}
            }
        }
    }

    pub fn volume(&self, cat: Category) -> f32 {
        self.volumes[&cat]
    }

    fn gain(&self, cat: Category) -> f32 {
        let master = self.volumes[&Category::Master];
        if cat == Category::Master { master } else { master * self.volumes[&cat] }
    }

    /// Plays `event`; `at` is a world position, `None` is a non-positional (UI) sound.
    pub fn play(&mut self, event: &str, at: Option<[f64; 3]>, volume: f32, pitch: f32) {
        let roll = self.rand();
        let Some(ev) = self.index.event(event) else { return };
        let Some(v) = ev.pick(roll) else { return };
        let mut gain = (volume * v.volume).clamp(0.0, 1.0) * self.gain(ev.category);
        if let Some(p) = at {
            // Vanilla: linear falloff over 16 blocks, scaled by volume when above 1.
            let range = 16.0 * f64::from(volume.max(1.0));
            let d = ((p[0] - self.listener[0]).powi(2)
                + (p[1] - self.listener[1]).powi(2)
                + (p[2] - self.listener[2]).powi(2))
            .sqrt();
            if d >= range {
                return;
            }
            gain *= (1.0 - d / range) as f32;
        }
        if gain <= 0.0 {
            return;
        }
        self.players.retain(|p| !p.empty());
        if self.players.len() >= MAX_SOUNDS {
            return;
        }
        let Ok(file) = File::open(&v.file) else { return };
        let Ok(dec) = Decoder::try_from(file) else { return };
        let player = Player::connect_new(self.sink.mixer());
        player.set_volume(gain);
        player.append(dec.speed((pitch * v.pitch).clamp(0.5, 2.0)));
        self.players.push(player);
    }

    /// Server `SoundEffect` (0x29): `x/y/z` are fixed-point (eighths of a block).
    pub fn play_packet(&mut self, name: &str, x: i32, y: i32, z: i32, volume: f32, pitch: u8) {
        let p = [f64::from(x) / 8.0, f64::from(y) / 8.0, f64::from(z) / 8.0];
        self.play(name, Some(p), volume, wire_pitch(pitch));
    }

    /// UI click and other non-positional sounds.
    pub fn play_ui(&mut self, event: &str) {
        self.play(event, None, 1.0, 1.0);
    }

    pub fn step(&mut self, block_id: u16, at: [f64; 3]) {
        let (e, v, p) = BlockSound::of(block_id).step();
        self.play(&e, Some(at), v, p);
    }

    /// Local break (aux effect 2001) and place share this sound.
    pub fn dig(&mut self, block_id: u16, at: [f64; 3]) {
        let (e, v, p) = BlockSound::of(block_id).dig();
        self.play(&e, Some(at), v, p);
    }

    /// Server `Effect` (0x28) with its block position.
    pub fn play_effect(&mut self, id: i32, data: i32, at: [f64; 3]) {
        if id == 2001 {
            return self.dig((data & 0xfff) as u16, at);
        }
        let r = (self.rand() >> 8) as f32 / (1u32 << 24) as f32;
        if let Some((e, v, p)) = effect_sound(id, r) {
            self.play(e, Some(at), v, p);
        }
    }

    /// Call once per game tick (20 Hz).
    pub fn tick(&mut self, in_world: bool) {
        if self.music.as_ref().is_some_and(Player::empty) {
            self.music = None;
        }
        if self.gain(Category::Music) <= 0.0 {
            return;
        }
        let roll = self.rand();
        let Some(event) = self.ticker.tick(in_world, self.music.is_some(), roll) else { return };
        let pick = self.rand();
        let Some(v) = self.index.event(event).and_then(|e| e.pick(pick)) else { return };
        let Ok(file) = File::open(&v.file) else { return };
        let Ok(dec) = Decoder::try_from(file) else { return };
        let player = Player::connect_new(self.sink.mixer());
        player.set_volume(self.gain(Category::Music) * v.volume);
        player.append(dec);
        self.music = Some(player);
    }
}
