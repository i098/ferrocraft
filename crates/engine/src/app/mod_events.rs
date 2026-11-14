//! World and entity events for the mods: what the client does when a world loads, an entity joins it, the player
//! attacks, or the connection ends, told to the mod host so that Forge handlers see the events the real client posts.
//! Rust acts first; the host answers nothing. See `crates/modhost/src/events.rs`.

use super::App;
use ferrocraft_modhost::{ModEvent, Spawn};
use ferrocraft_net_client::NetEvent;
use world::entities::EntityEvent;

/// A spawn packet position in 1/32 blocks.
fn blocks(pos: [i32; 3]) -> [f64; 3] {
    pos.map(|v| f64::from(v) / 32.0)
}

impl App {
    /// A server event, before the game applies it: the join and a dimension change load a world, and each spawn packet
    /// puts an entity in it.
    pub(super) fn mod_net_event(&self, ev: &NetEvent) {
        let Some(mods) = &self.mods else { return };
        let own = |id| ModEvent::EntityJoin {
            id,
            kind: Spawn::Own,
            pos: self.game.player.as_ref().map_or([0.0; 3], |p| p.pos),
        };
        let join = |id, kind, pos| ModEvent::EntityJoin { id, kind, pos };
        match ev {
            NetEvent::JoinGame { entity_id, .. } => {
                for e in [ModEvent::Connect, ModEvent::WorldLoad, own(*entity_id)] {
                    mods.post_event(e);
                }
            }
            NetEvent::Respawn {
                dimension_changed: true,
            } => {
                for e in [
                    ModEvent::WorldUnload,
                    ModEvent::WorldLoad,
                    own(self.game.entity_id),
                ] {
                    mods.post_event(e);
                }
            }
            NetEvent::Entity(ev) => {
                let e = match ev {
                    EntityEvent::SpawnPlayer { id, uuid, pos, .. } => {
                        join(*id, Spawn::Player(*uuid), blocks(*pos))
                    }
                    EntityEvent::SpawnMob { id, kind, pos, .. } => {
                        join(*id, Spawn::Mob(*kind), blocks(*pos))
                    }
                    EntityEvent::SpawnObject { id, kind, pos, .. } => {
                        join(*id, Spawn::Object(*kind), blocks(*pos))
                    }
                    EntityEvent::SpawnExperienceOrb { id, pos } => {
                        join(*id, Spawn::Orb, blocks(*pos))
                    }
                    EntityEvent::SpawnPainting { id, pos, .. } => {
                        join(*id, Spawn::Painting, pos.map(f64::from))
                    }
                    _ => return,
                };
                mods.post_event(e);
            }
            _ => {}
        }
    }

    /// The client leaves its world, by the server's disconnect, a lost connection or the menu.
    pub(super) fn mod_leave_world(&self) {
        let Some(mods) = self.mods.as_ref().filter(|_| self.ui.in_world) else {
            return;
        };
        mods.post_event(ModEvent::WorldUnload);
        mods.post_event(ModEvent::Disconnect);
    }

    /// The local player attacks an entity (`EntityPlayer.attackTargetEntityWithCurrentItem`).
    pub(super) fn mod_attack(&self, id: i32) {
        if let Some(mods) = &self.mods {
            mods.post_event(ModEvent::Attack(id));
        }
    }
}
