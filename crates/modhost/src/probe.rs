//! Render probes: the values that a mod changes inside a vanilla render method, and the render events of the entities.
//! Rust draws the entities and the items, so the patch of the mod never runs there. Once per frame Rust asks Java to run
//! the real vanilla method on a stand-in object and to send the result. Wire message `'Q'`.
//!
//! Rust to Java: `'Q'`, u8 `want` (bit 0 the hurt tint, bit 1 the item glint color, bit 2 the render events), u8 hurt time
//! of the entity that is drawn (the tint can fade with it), u32 entity count, then per entity i32 id and three f64 position,
//! relative to the camera.
//! Java to Rust: u8 `valid` (bit 0 hurt tint, bit 1 glint color, bit 2 a handler listens for `RenderLivingEvent`), four f32
//! red, green, blue and alpha of the hurt tint, i32 ARGB of the glint, then one byte per entity of the request: 1 when a
//! handler cancelled `RenderLivingEvent.Pre`, so the entity is not drawn.

use std::io::{self, Read, Write};

use crate::hooks::Hooks;
use crate::wire::{f32_, u8_};

const WANT_HURT: u8 = 1;
const WANT_GLINT: u8 = 2;
const WANT_EVENTS: u8 = 4;
const VALID_HURT: u8 = 1;
const VALID_GLINT: u8 = 2;
const VALID_LISTENING: u8 = 4;

/// A request holds at most this many entities; a longer list is a corrupt stream, or more than the renderer draws.
const MAX_TARGETS: usize = 1 << 12;

/// While no handler listens, the render events are asked again after this many frames.
const RECHECK_FRAMES: u32 = 120;

const RENDERER: &str = "net/minecraft/client/renderer/entity/RendererLivingEntity";
const RENDER_ITEM: &str = "net/minecraft/client/renderer/entity/RenderItem";
const MODEL: &str = "Lnet/minecraft/client/resources/model/IBakedModel;";

/// An entity of the frame: its id and its position relative to the camera, as `RenderLivingEvent` gives it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Target {
    pub id: i32,
    pub pos: [f64; 3],
}

/// What the frame draws, which tells what is worth asking.
#[derive(Clone, Copy, Debug)]
pub struct Frame<'a> {
    /// `hurtTime` of a drawn entity that flashes, if any does.
    pub hurt_time: Option<u8>,
    /// An enchanted item is drawn.
    pub glint: bool,
    /// The living entities that are drawn.
    pub targets: &'a [Target],
}

#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    want: u8,
    hurt_time: u8,
    targets: Vec<Target>,
}

/// What the mods leave of each value; `None` is the vanilla value.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Reply {
    /// Red, green, blue and alpha of `RendererLivingEntity.setBrightness` for a hit entity.
    pub hurt: Option<[f32; 4]>,
    /// Red, green, blue and alpha of the color that `RenderItem.renderEffect` gives to `renderModel`.
    pub glint: Option<[f32; 4]>,
    /// The entities that a handler of `RenderLivingEvent.Pre` cancelled.
    pub cancelled: Vec<i32>,
}

/// Decides each frame what to ask. A probe costs a round trip, so a method that no mod changed is not asked.
#[derive(Debug, Default)]
pub struct Gate {
    listening: bool,
    quiet_frames: u32,
}

impl Gate {
    /// The request for this frame, or `None` when nothing is worth asking.
    pub fn request(&mut self, hooks: &Hooks, frame: &Frame) -> Option<Request> {
        let hurt = frame.hurt_time.is_some()
            && hooks.changed(
                RENDERER,
                "func_177092_a",
                "(Lnet/minecraft/entity/EntityLivingBase;FZ)Z",
            );
        let glint = frame.glint
            && (hooks.changed(RENDER_ITEM, "func_180451_a", &format!("({MODEL})V"))
                || hooks.changed(RENDER_ITEM, "func_175035_a", &format!("({MODEL}I)V")));
        let events = !frame.targets.is_empty() && (self.listening || self.recheck());
        let want = u8::from(hurt) * WANT_HURT
            | u8::from(glint) * WANT_GLINT
            | u8::from(events) * WANT_EVENTS;
        (want != 0).then(|| Request {
            want,
            hurt_time: frame.hurt_time.unwrap_or(0),
            targets: if events {
                frame.targets.iter().take(MAX_TARGETS).copied().collect()
            } else {
                Vec::new()
            },
        })
    }

    fn recheck(&mut self) -> bool {
        self.quiet_frames += 1;
        self.quiet_frames > RECHECK_FRAMES && {
            self.quiet_frames = 0;
            true
        }
    }

    /// Remember whether a handler listens, from a reply to a request that asked for the events.
    pub fn learn(&mut self, listening: bool) {
        self.listening = listening;
    }
}

impl Request {
    pub fn asks_events(&self) -> bool {
        self.want & WANT_EVENTS != 0
    }

    pub fn targets(&self) -> usize {
        self.targets.len()
    }
}

/// The reply and whether a handler listens; `listening` is `None` when the request did not ask for the events.
pub struct Answer {
    pub reply: Reply,
    pub listening: Option<bool>,
}

pub fn write_request(w: &mut impl Write, r: &Request) -> io::Result<()> {
    let mut b = vec![b'Q', r.want, r.hurt_time];
    b.extend((r.targets.len() as u32).to_be_bytes());
    for t in &r.targets {
        b.extend(t.id.to_be_bytes());
        for v in t.pos {
            b.extend(v.to_be_bytes());
        }
    }
    w.write_all(&b)?;
    w.flush()
}

pub fn read_reply(r: &mut impl Read, request: &Request) -> io::Result<Answer> {
    let valid = u8_(r)?;
    let mut hurt = [0.0; 4];
    for v in &mut hurt {
        *v = f32_(r)?;
    }
    let mut glint = [0; 4];
    r.read_exact(&mut glint)?;
    let glint = i32::from_be_bytes(glint) as u32;
    let mut flags = vec![0; request.targets.len()];
    r.read_exact(&mut flags)?;
    let has = |bit: u8| valid & bit != 0;
    let channel = |shift: u32| ((glint >> shift) & 255) as f32 / 255.0;
    Ok(Answer {
        reply: Reply {
            hurt: has(VALID_HURT).then_some(hurt),
            glint: has(VALID_GLINT).then(|| [channel(16), channel(8), channel(0), channel(24)]),
            cancelled: request
                .targets
                .iter()
                .zip(flags)
                .filter_map(|(t, f)| (f != 0).then_some(t.id))
                .collect(),
        },
        listening: request.asks_events().then_some(has(VALID_LISTENING)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hooks(list: &[(&str, &str, &str)]) -> Hooks {
        let mut b = (list.len() as u32).to_be_bytes().to_vec();
        for (owner, name, desc) in list {
            for s in [owner, name, desc] {
                b.extend((s.len() as u16).to_be_bytes());
                b.extend(s.as_bytes());
            }
        }
        Hooks::read(&mut &b[..]).unwrap()
    }

    const SET_BRIGHTNESS: (&str, &str, &str) = (
        RENDERER,
        "func_177092_a",
        "(Lnet/minecraft/entity/EntityLivingBase;FZ)Z",
    );

    #[test]
    fn asks_only_what_a_mod_changed_and_the_frame_draws() {
        let target = [Target {
            id: 5,
            pos: [1.0, 2.0, 3.0],
        }];
        let frame = Frame {
            hurt_time: Some(7),
            glint: true,
            targets: &target,
        };
        let mut gate = Gate::default();
        assert_eq!(
            gate.request(
                &hooks(&[]),
                &Frame {
                    targets: &[],
                    ..frame
                }
            ),
            None
        );
        let r = gate
            .request(
                &hooks(&[SET_BRIGHTNESS]),
                &Frame {
                    targets: &[],
                    ..frame
                },
            )
            .unwrap();
        assert_eq!((r.want, r.hurt_time), (WANT_HURT, 7));
        let no_hit = Frame {
            hurt_time: None,
            targets: &[],
            ..frame
        };
        assert_eq!(gate.request(&hooks(&[SET_BRIGHTNESS]), &no_hit), None);
    }

    #[test]
    fn render_events_are_rechecked_until_a_handler_listens() {
        let target = [Target {
            id: 5,
            pos: [0.0; 3],
        }];
        let frame = Frame {
            hurt_time: None,
            glint: false,
            targets: &target,
        };
        let none = hooks(&[]);
        let mut gate = Gate::default();
        let asked = (0..=RECHECK_FRAMES)
            .filter(|_| gate.request(&none, &frame).is_some())
            .count();
        assert_eq!(asked, 1);
        gate.learn(true);
        assert!(gate.request(&none, &frame).is_some());
        gate.learn(false);
        assert!(gate.request(&none, &frame).is_none());
    }

    #[test]
    fn writes_the_request_java_reads() {
        let r = Request {
            want: WANT_HURT | WANT_EVENTS,
            hurt_time: 9,
            targets: vec![Target {
                id: -2,
                pos: [0.5, 0.0, -1.0],
            }],
        };
        let mut out = Vec::new();
        write_request(&mut out, &r).unwrap();
        let mut want = vec![b'Q', 5, 9, 0, 0, 0, 1, 0xff, 0xff, 0xff, 0xfe];
        for v in [0.5f64, 0.0, -1.0] {
            want.extend(v.to_be_bytes());
        }
        assert_eq!(out, want);
    }

    #[test]
    fn reads_the_groups_java_marked_valid_and_the_cancelled_entities() {
        let request = Request {
            want: WANT_GLINT | WANT_EVENTS,
            hurt_time: 0,
            targets: vec![
                Target {
                    id: 4,
                    pos: [0.0; 3],
                },
                Target {
                    id: 8,
                    pos: [0.0; 3],
                },
            ],
        };
        // A blue glint (0xFF0000FF); the hurt group is not valid, so its floats do not count.
        let mut b = vec![VALID_GLINT | VALID_LISTENING];
        for v in [9.0f32, 9.0, 9.0, 9.0] {
            b.extend(v.to_be_bytes());
        }
        b.extend(0xFF00_00FFu32.to_be_bytes());
        b.extend([0, 1]);
        let a = read_reply(&mut b.as_slice(), &request).unwrap();
        assert_eq!(a.reply.hurt, None);
        assert_eq!(a.reply.glint, Some([0.0, 0.0, 1.0, 1.0]));
        assert_eq!(a.reply.cancelled, vec![8]);
        assert_eq!(a.listening, Some(true));
        assert!(read_reply(&mut &b[..b.len() - 1], &request).is_err());
    }
}
