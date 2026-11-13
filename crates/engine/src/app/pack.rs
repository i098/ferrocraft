//! `NetHandlerPlayClient.handleResourcePack`: a `level://` pack is taken at once; any other pack
//! follows the server's resource mode, and the player is asked while that mode is "prompt".

use super::{boxed, App};
use ferrocraft_net_client::{load_servers, save_servers, split_address, PackStatus};
use gui::screens::PackPrompt;

impl App {
    /// The `servers.dat` entry for the connected server, with its `acceptTextures` choice.
    fn server_entry(&self) -> Option<(usize, Vec<ferrocraft_net_client::ServerEntry>)> {
        let path = self.args.game_dir.join("servers.dat");
        let list = load_servers(&path);
        let i = list.iter().position(|e| {
            let (host, port) = split_address(&e.ip);
            let addr = if host.contains(':') {
                format!("[{host}]:{port}")
            } else {
                format!("{host}:{port}")
            };
            addr == self.server
        })?;
        Some((i, list))
    }

    pub(super) fn saved_pack_mode(&self) -> Option<bool> {
        let (i, list) = self.server_entry()?;
        list[i].accept_textures
    }

    pub(super) fn offer_server_pack(&mut self, url: &str, hash: &str) {
        let level = url.starts_with("level://");
        let r = match (level, self.pack_mode) {
            (true, _) | (_, Some(true)) => self.accept_server_pack(url, hash),
            (_, Some(false)) => {
                self.session_send(|s| s.send_pack_status(hash, PackStatus::Declined))
            }
            (_, None) if self.awaiting_terrain => {
                self.pending_pack = Some((url.to_owned(), hash.to_owned()));
                Ok(())
            }
            (_, None) => {
                let m = metrics!(self);
                let q = self
                    .ui
                    .open(&m, boxed(PackPrompt::new(url.to_owned(), hash.to_owned())));
                self.run_effects(q);
                Ok(())
            }
        };
        self.send_result(r);
    }

    /// The Yes or No of the prompt: keep it for the server, then answer the offer.
    pub(super) fn answer_server_pack(&mut self, url: &str, hash: &str, accepted: bool) {
        self.pack_mode = Some(accepted);
        if let Some((i, mut list)) = self.server_entry() {
            list[i].accept_textures = Some(accepted);
            let _ = save_servers(&self.args.game_dir.join("servers.dat"), &list);
        }
        let r = if accepted {
            self.accept_server_pack(url, hash)
        } else {
            self.session_send(|s| s.send_pack_status(hash, PackStatus::Declined))
        };
        self.send_result(r);
    }

    fn accept_server_pack(&mut self, url: &str, hash: &str) -> std::io::Result<()> {
        let dir = self.args.game_dir.clone();
        self.session_send(|s| s.accept_resource_pack(url, hash, &dir))
    }
}
