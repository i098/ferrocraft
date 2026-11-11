//! Player state, window, tab list, title, scoreboard and world-effect packets as [`NetEvent`] values.

use crate::{chat, NetEvent};
use ferrocraft_protocol::play::clientbound as cb;
use ferrocraft_protocol::play::{PlayerListAction, TitleAction};

/// One `PlayerListItem` (S38) change.
#[derive(Debug, Clone, PartialEq)]
pub enum PlayerListEvent {
    Add(Vec<PlayerInfo>),
    Gamemode(u128, u8),
    Latency(u128, i32),
    /// The custom tab list name as formatted text (§ codes), `None` to clear it.
    DisplayName(u128, Option<String>),
    Remove(Vec<u128>),
}

/// A player added to the tab list.
#[derive(Debug, Clone, PartialEq)]
pub struct PlayerInfo {
    pub uuid: u128,
    pub name: String,
    pub gamemode: u8,
    pub ping: i32,
    /// Formatted text (§ codes) of the custom tab list name.
    pub display: Option<String>,
}

/// `IChatComponent.getFormattedText()` of a chat JSON string.
fn formatted(json: &str) -> String {
    chat::formatted_text(&chat::styled(json))
}

/// The events of one packet; empty for packets handled elsewhere.
pub(crate) fn events(packet: &cb::Packet) -> Vec<NetEvent> {
    if let cb::Packet::PlayerListItem(p) = packet {
        return player_list(&p.action);
    }
    window_event(packet)
        .or_else(|| score_event(packet))
        .or_else(|| world_event(packet))
        .into_iter()
        .collect()
}

fn window_event(packet: &cb::Packet) -> Option<NetEvent> {
    use cb::Packet as P;
    Some(match packet {
        P::UpdateHealth(p) => NetEvent::Health {
            health: p.health,
            food: p.food.0,
            saturation: p.saturation,
        },
        P::EntityEffect(p) => NetEvent::EntityEffect {
            entity_id: p.entity_id.0,
            effect: world::effects::Effect {
                id: p.effect_id,
                amplifier: p.amplifier,
                duration: p.duration.0,
                permanent: p.duration.0 == world::effects::PERMANENT_TICKS,
                ambient: false,
                particles: !p.hide_particles,
            },
        },
        P::RemoveEntityEffect(p) => NetEvent::RemoveEntityEffect {
            entity_id: p.entity_id.0,
            effect_id: p.effect_id,
        },
        P::EntityProperties(p) => movement_speed(p)?,
        P::SetExperience(p) => NetEvent::Experience {
            bar: p.bar,
            level: p.level.0,
            total: p.total.0,
        },
        P::HeldItemChange(p) => NetEvent::HeldItem(p.slot as u8),
        P::SetSlot(p) => NetEvent::SetSlot {
            window: p.window_id,
            slot: p.slot,
            item: p.item.clone(),
        },
        P::WindowItems(p) => NetEvent::WindowItems {
            window: p.window_id,
            items: p.slots.0.clone(),
        },
        P::OpenWindow(p) => NetEvent::OpenWindow {
            id: p.window_id,
            kind: p.window_type.clone(),
            title: chat::plain_text(&p.title),
            slots: p.slots,
        },
        P::CloseWindow(_) => NetEvent::CloseWindow,
        P::WindowProperty(p) => NetEvent::WindowProperty {
            window: p.window_id,
            property: p.property,
            value: p.value,
        },
        P::ConfirmTransaction(p) => NetEvent::Transaction {
            window: p.window_id,
            action: p.action,
            accepted: p.accepted,
        },
        _ => return None,
    })
}

/// The `generic.movementSpeed` instance of an S20 packet.
fn movement_speed(p: &cb::EntityProperties) -> Option<NetEvent> {
    let a = p
        .attributes
        .0
        .iter()
        .find(|a| a.key == "generic.movementSpeed")?;
    Some(NetEvent::MovementSpeed {
        entity_id: p.entity_id.0,
        speed: world::effects::Speed {
            base: a.value,
            modifiers: a
                .modifiers
                .iter()
                .map(|m| world::effects::Modifier {
                    uuid: m.uuid.0,
                    amount: m.amount,
                    operation: m.operation,
                })
                .collect(),
        },
    })
}

fn score_event(packet: &cb::Packet) -> Option<NetEvent> {
    use cb::Packet as P;
    Some(match packet {
        P::PlayerListHeaderFooter(p) => NetEvent::TabHeaderFooter {
            header: formatted(&p.header),
            footer: formatted(&p.footer),
        },
        P::Title(p) => title(&p.action),
        P::ScoreboardObjective(p) => NetEvent::ScoreboardObjective {
            name: p.name.clone(),
            display: p
                .data
                .as_ref()
                .map_or_else(String::new, |d| d.value.clone()),
            mode: p.mode as u8,
        },
        P::UpdateScore(p) => NetEvent::ScoreUpdate {
            name: p.name.clone(),
            objective: p.objective.clone(),
            value: p.value.map_or(0, |v| v.0),
            remove: p.action == 1,
        },
        P::DisplayScoreboard(p) => NetEvent::DisplayScoreboard {
            position: p.position as u8,
            name: p.name.clone(),
        },
        P::Teams(p) => NetEvent::Team {
            name: p.name.clone(),
            mode: p.mode as u8,
            prefix: p
                .info
                .as_ref()
                .map_or_else(String::new, |i| i.prefix.clone()),
            suffix: p
                .info
                .as_ref()
                .map_or_else(String::new, |i| i.suffix.clone()),
            members: p.players.clone().unwrap_or_default(),
        },
        _ => return None,
    })
}

fn world_event(packet: &cb::Packet) -> Option<NetEvent> {
    use cb::Packet as P;
    Some(match packet {
        P::SoundEffect(p) => NetEvent::Sound {
            name: p.name.clone(),
            pos: [p.x, p.y, p.z].map(|v| f64::from(v) / 8.0),
            volume: p.volume,
            pitch: p.pitch,
        },
        P::Particle(p) => NetEvent::Particle {
            id: p.id,
            long_distance: p.long_distance,
            pos: [p.x, p.y, p.z],
            offset: [p.offset_x, p.offset_y, p.offset_z],
            speed: p.speed,
            count: p.count,
            data: p.data.iter().map(|v| v.0).collect(),
        },
        P::Effect(p) => NetEvent::Effect {
            id: p.effect_id,
            pos: [p.location.x, p.location.y, p.location.z],
            data: p.data,
            global: p.disable_relative_volume,
        },
        P::Explosion(p) => NetEvent::Explosion {
            pos: [p.x, p.y, p.z],
            radius: p.radius,
            records: p.records.0.iter().map(|o| [o.x, o.y, o.z]).collect(),
            motion: [p.velocity_x, p.velocity_y, p.velocity_z],
        },
        P::PlayerAbilities(p) => NetEvent::Abilities {
            flags: p.flags,
            fly_speed: p.flying_speed,
            walk_speed: p.walking_speed,
        },
        P::ServerDifficulty(p) => NetEvent::Difficulty(p.difficulty),
        P::BlockBreakAnimation(p) => NetEvent::BlockBreakAnim {
            id: p.entity_id.0,
            pos: [p.location.x, p.location.y, p.location.z],
            stage: p.destroy_stage,
        },
        _ => return None,
    })
}

fn title(action: &TitleAction) -> NetEvent {
    let (action, text, [fade_in, stay, fade_out]) = match action {
        TitleAction::SetTitle(t) => (0, Some(formatted(t)), [0; 3]),
        TitleAction::SetSubtitle(t) => (1, Some(formatted(t)), [0; 3]),
        TitleAction::SetTimes {
            fade_in,
            stay,
            fade_out,
        } => (2, None, [*fade_in, *stay, *fade_out]),
        TitleAction::Hide => (3, None, [0; 3]),
        TitleAction::Reset => (4, None, [0; 3]),
    };
    NetEvent::Title {
        action,
        text,
        fade_in,
        stay,
        fade_out,
    }
}

fn player_list(action: &PlayerListAction) -> Vec<NetEvent> {
    let wrap = |action| NetEvent::PlayerList { action };
    match action {
        PlayerListAction::Add(v) => vec![wrap(PlayerListEvent::Add(
            v.iter()
                .map(|(u, a)| PlayerInfo {
                    uuid: u.0,
                    name: a.name.clone(),
                    gamemode: a.gamemode.0 as u8,
                    ping: a.ping.0,
                    display: a.display_name.0.as_deref().map(formatted),
                })
                .collect(),
        ))],
        PlayerListAction::Gamemode(v) => v
            .iter()
            .map(|(u, m)| wrap(PlayerListEvent::Gamemode(u.0, m.0 as u8)))
            .collect(),
        PlayerListAction::Latency(v) => v
            .iter()
            .map(|(u, p)| wrap(PlayerListEvent::Latency(u.0, p.0)))
            .collect(),
        PlayerListAction::DisplayName(v) => v
            .iter()
            .map(|(u, d)| {
                wrap(PlayerListEvent::DisplayName(
                    u.0,
                    d.0.as_deref().map(formatted),
                ))
            })
            .collect(),
        PlayerListAction::Remove(v) => vec![wrap(PlayerListEvent::Remove(
            v.iter().map(|u| u.0).collect(),
        ))],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrocraft_protocol::codec::{ItemStack, Opt, Uuid, VarInt};
    use ferrocraft_protocol::nbt::{Nbt, Tag};
    use ferrocraft_protocol::play::{ObjectiveData, PlayerListAdd, TeamInfo};
    use ferrocraft_protocol::PacketSet;

    /// Encodes then decodes, so each test goes through the wire format.
    fn wire(p: impl Into<cb::Packet>) -> Vec<NetEvent> {
        events(&cb::Packet::decode(&p.into().encode()).unwrap())
    }

    #[test]
    fn player_list_add_and_display_name() {
        let add = PlayerListAdd {
            name: "Steve".into(),
            properties: vec![],
            gamemode: VarInt(1),
            ping: VarInt(42),
            display_name: Opt(Some(r#"{"text":"Boss","color":"red"}"#.into())),
        };
        let ev = wire(cb::PlayerListItem {
            action: PlayerListAction::Add(vec![(Uuid(7), add)]),
        });
        assert_eq!(
            ev,
            [NetEvent::PlayerList {
                action: PlayerListEvent::Add(vec![PlayerInfo {
                    uuid: 7,
                    name: "Steve".into(),
                    gamemode: 1,
                    ping: 42,
                    display: Some("§cBoss§r".into()),
                }])
            }]
        );
        let ev = wire(cb::PlayerListItem {
            action: PlayerListAction::Latency(vec![(Uuid(7), VarInt(5)), (Uuid(8), VarInt(9))]),
        });
        assert_eq!(
            ev,
            [
                NetEvent::PlayerList {
                    action: PlayerListEvent::Latency(7, 5)
                },
                NetEvent::PlayerList {
                    action: PlayerListEvent::Latency(8, 9)
                },
            ]
        );
        let ev = wire(cb::PlayerListItem {
            action: PlayerListAction::DisplayName(vec![(Uuid(7), Opt(None))]),
        });
        assert_eq!(
            ev,
            [NetEvent::PlayerList {
                action: PlayerListEvent::DisplayName(7, None)
            }]
        );
    }

    #[test]
    fn title_actions() {
        let ev = wire(cb::Title {
            action: TitleAction::SetSubtitle(r#"{"text":"hi","bold":true}"#.into()),
        });
        assert_eq!(
            ev,
            [NetEvent::Title {
                action: 1,
                text: Some("§lhi§r".into()),
                fade_in: 0,
                stay: 0,
                fade_out: 0,
            }]
        );
        let ev = wire(cb::Title {
            action: TitleAction::SetTimes {
                fade_in: 10,
                stay: 70,
                fade_out: 20,
            },
        });
        assert_eq!(
            ev,
            [NetEvent::Title {
                action: 2,
                text: None,
                fade_in: 10,
                stay: 70,
                fade_out: 20,
            }]
        );
        let ev = wire(cb::Title {
            action: TitleAction::Reset,
        });
        assert!(matches!(ev[..], [NetEvent::Title { action: 4, .. }]));
    }

    #[test]
    fn scoreboard_packets() {
        let ev = wire(cb::ScoreboardObjective {
            name: "obj".into(),
            mode: 0,
            data: Some(ObjectiveData {
                value: "Kills".into(),
                kind: "integer".into(),
            }),
        });
        assert_eq!(
            ev,
            [NetEvent::ScoreboardObjective {
                name: "obj".into(),
                display: "Kills".into(),
                mode: 0,
            }]
        );
        let ev = wire(cb::UpdateScore {
            name: "Steve".into(),
            action: 1,
            objective: "obj".into(),
            value: None,
        });
        assert_eq!(
            ev,
            [NetEvent::ScoreUpdate {
                name: "Steve".into(),
                objective: "obj".into(),
                value: 0,
                remove: true,
            }]
        );
        let ev = wire(cb::UpdateScore {
            name: "Steve".into(),
            action: 0,
            objective: "obj".into(),
            value: Some(VarInt(3)),
        });
        assert!(matches!(
            ev[..],
            [NetEvent::ScoreUpdate {
                value: 3,
                remove: false,
                ..
            }]
        ));
        let ev = wire(cb::DisplayScoreboard {
            position: 1,
            name: "obj".into(),
        });
        assert_eq!(
            ev,
            [NetEvent::DisplayScoreboard {
                position: 1,
                name: "obj".into(),
            }]
        );
        let ev = wire(cb::Teams {
            name: "red".into(),
            mode: 0,
            info: Some(TeamInfo {
                display_name: "Red".into(),
                prefix: "§c".into(),
                suffix: "§r".into(),
                friendly_fire: 0,
                name_tag_visibility: "always".into(),
                color: 12,
            }),
            players: Some(vec!["Steve".into()]),
        });
        assert_eq!(
            ev,
            [NetEvent::Team {
                name: "red".into(),
                mode: 0,
                prefix: "§c".into(),
                suffix: "§r".into(),
                members: vec!["Steve".into()],
            }]
        );
        // Mode 3 (add players) carries no team info.
        let ev = wire(cb::Teams {
            name: "red".into(),
            mode: 3,
            info: None,
            players: Some(vec!["Alex".into()]),
        });
        assert!(
            matches!(&ev[..], [NetEvent::Team { mode: 3, prefix, members, .. }]
            if prefix.is_empty() && members == &["Alex"])
        );
    }

    /// `world::inventory::Slot` is the protocol slot, so NBT goes through `SetSlot` unchanged.
    #[test]
    fn set_slot_keeps_nbt() {
        let ench = Tag::Compound(vec![
            ("id".into(), Tag::Short(16)),
            ("lvl".into(), Tag::Short(5)),
        ]);
        let display = Tag::Compound(vec![
            ("Name".into(), Tag::String("§bExcalibur".into())),
            ("Lore".into(), Tag::List(vec![Tag::String("old".into())])),
        ]);
        let nbt = Nbt {
            name: String::new(),
            root: vec![
                ("ench".into(), Tag::List(vec![ench])),
                ("display".into(), display),
            ],
        };
        let item: world::inventory::Slot = Some(ItemStack {
            id: 276,
            count: 1,
            damage: 12,
            nbt: Some(nbt),
        });
        let ev = wire(cb::SetSlot {
            window_id: -1,
            slot: -1,
            item: item.clone(),
        });
        assert_eq!(
            ev,
            [NetEvent::SetSlot {
                window: -1,
                slot: -1,
                item
            }]
        );
    }
}
