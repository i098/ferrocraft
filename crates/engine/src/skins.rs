//! Player skins: downloads `textures` skins in the background and hands the decoded images to the renderer.

use ferrocraft_net_client::{fetch_skin, parse_textures};
use render::{decode_png, Image, Renderer};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};

pub struct PlayerSkins {
    cache: PathBuf,
    requested: HashSet<u128>,
    /// Decoded skins with their arm model; kept so the renderer can get one back after it replaced it.
    ready: HashMap<u128, (Image, bool)>,
    tx: Sender<(u128, bool, Vec<u8>)>,
    rx: Receiver<(u128, bool, Vec<u8>)>,
}

impl PlayerSkins {
    /// `game_dir/assets/skins` holds the cache, as in the vanilla launcher.
    pub fn new(game_dir: &std::path::Path) -> PlayerSkins {
        let (tx, rx) = channel();
        PlayerSkins {
            cache: game_dir.join("assets").join("skins"),
            requested: HashSet::new(),
            ready: HashMap::new(),
            tx,
            rx,
        }
    }

    /// Starts the download of `textures` once per player.
    fn request(&mut self, uuid: u128, textures: &str) {
        if !self.requested.insert(uuid) {
            return;
        }
        let (tx, cache, textures) = (self.tx.clone(), self.cache.clone(), textures.to_string());
        std::thread::spawn(move || {
            if let Some(info) = parse_textures(&textures) {
                if let Some(bytes) = fetch_skin(&info.url, &cache) {
                    let _ = tx.send((uuid, info.slim, bytes));
                }
            }
        });
    }

    /// The skin of a drawn player: `Some` once downloaded. Puts it into the renderer when it is
    /// new there or when the renderer replaced it for lack of room.
    pub fn show(&mut self, r: &mut Renderer, uuid: u128, textures: Option<&str>) -> Option<u128> {
        while let Ok((uuid, slim, bytes)) = self.rx.try_recv() {
            if let Some(img) = decode_png(&bytes) {
                self.ready.insert(uuid, (img, slim));
            }
        }
        if let Some(t) = textures {
            self.request(uuid, t);
        }
        let (img, _) = self.ready.get(&uuid)?;
        if !r.has_player_skin(uuid) {
            r.set_player_skin(uuid, img);
        }
        Some(uuid)
    }

    /// Arm model of a downloaded skin, `None` until it is ready.
    pub fn slim(&self, uuid: u128) -> Option<bool> {
        self.ready.get(&uuid).map(|r| r.1)
    }
}
