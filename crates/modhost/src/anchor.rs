//! Draws in the world: name tags and the TNT countdown. A mod changes these in vanilla methods that Rust ports
//! (`Render.renderLivingLabel`, `RenderTNTPrimed.doRender`). Once per frame Rust sends the entities in view, and Java
//! runs those vanilla methods on stand-in entities at the origin and records the text and plates they draw. Rust
//! projects them and draws them over the world.
//!
//! Wire, a round trip like `overrides.rs`. Rust to Java: `'A'`, f32 partial tick, f32 view yaw, f32 view pitch, three f64
//! camera position, u16 subject count, then per subject: i32 entity id, u8 class (0 player, 1 other, 2 primed TNT),
//! u8 roles (1 label, 2 countdown, 4 hitbox), three f64 position of the feet, f32 width, f32 height, f32 yaw, f32 label
//! offset, i32 fuse, and for the label role u32 length and the UTF-8 text.
//! Java to Rust: u32 item count, then per item `'L'` (i32 entity id, u16 op count, then per op `'Q'`: four points of
//! two f32 and i32 ARGB; or `'T'`: f32 right, f32 up, f32 blocks per text pixel, i32 ARGB, u8 shadow, u32 length and
//! UTF-8); or `'H'` (i32 entity id, u16 segment count, then per segment two points of three f32, blocks from the
//! feet, and i32 ARGB: the hitbox lines). Label points are blocks to the right of the feet and up from them, facing the camera.

use std::io::{self, Read, Write};

use super::{ModHost, Phase, PLAYER_TICK_WAIT};

/// Subjects above this count are not sent; the nearest come first.
pub const MAX_SUBJECTS: usize = 256;
/// Counts above these mean a corrupt stream.
const MAX_ITEMS: u32 = 1 << 14;
const MAX_OPS: u16 = 256;
const MAX_SEGMENTS: u16 = 256;
const MAX_TEXT: usize = 1 << 12;

pub const ROLE_LABEL: u8 = 1;
pub const ROLE_COUNTDOWN: u8 = 2;
/// `RenderManager.renderDebugBoundingBox`; asked only while F3+B shows hitboxes.
pub const ROLE_HITBOX: u8 = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    Player = 0,
    Other = 1,
    PrimedTnt = 2,
}

/// One entity of this frame and what the mods may draw for it.
#[derive(Clone, Debug, PartialEq)]
pub struct Subject {
    pub id: i32,
    pub class: Class,
    /// `ROLE_*` bits.
    pub roles: u8,
    pub pos: [f64; 3],
    pub size: (f32, f32),
    pub yaw: f32,
    /// What is added to the height of the feet for the label: vanilla's lift of a sneaking player or a child.
    pub label_dy: f32,
    /// `EntityTNTPrimed.fuse`; 0 when the entity is no TNT.
    pub fuse: i32,
    pub label: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Request {
    pub partial: f32,
    /// `RenderManager.playerViewY` and `playerViewX`.
    pub view: [f32; 2],
    pub camera: [f64; 3],
    pub subjects: Vec<Subject>,
}

/// One piece of a label, in blocks from the camera.
#[derive(Clone, Debug, PartialEq)]
pub enum Op {
    Quad {
        pts: [[f32; 2]; 4],
        argb: u32,
    },
    Text {
        /// The top left of the text, to the right of the feet and up from them, in blocks.
        right: f32,
        up: f32,
        /// Blocks per text pixel.
        scale: f32,
        argb: u32,
        shadow: bool,
        text: String,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Label {
    pub id: i32,
    pub ops: Vec<Op>,
}

/// One hitbox line, in blocks from the feet of its entity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Segment {
    pub from: [f32; 3],
    pub to: [f32; 3],
    pub argb: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Lines {
    pub id: i32,
    pub segments: Vec<Segment>,
}

/// What the mods drew for one frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct World {
    pub labels: Vec<Label>,
    pub lines: Vec<Lines>,
}

/// The request the pump has not sent yet and the answer it has not handed out.
#[derive(Default)]
pub(super) struct Slot {
    pub request: Option<Request>,
    pub answer: Option<World>,
}

impl ModHost {
    /// What the mods draw in the world for these entities, once per frame. `None` while the JVM is not live or when it
    /// does not answer in time: the engine then draws the vanilla tags. A late answer stalls like one of `overrides`.
    pub fn anchor(&self, request: Request) -> Option<World> {
        let mut s = self.link.state.lock().unwrap();
        if s.phase != Phase::Live || s.stalled {
            return None;
        }
        s.anchor.request = Some(request);
        s.anchor.answer = None;
        self.link.view.notify_one();
        let (mut s, _) = self
            .link
            .answered
            .wait_timeout_while(s, PLAYER_TICK_WAIT, |s| {
                s.anchor.answer.is_none() && s.phase == Phase::Live
            })
            .unwrap();
        s.anchor.request = None;
        let answer = s.anchor.answer.take();
        s.stalled = answer.is_none() && s.phase == Phase::Live;
        answer
    }
}

pub fn write_request(w: &mut impl Write, r: &Request) -> io::Result<()> {
    let n = r.subjects.len().min(MAX_SUBJECTS);
    let mut b = vec![b'A'];
    for v in [r.partial, r.view[0], r.view[1]] {
        b.extend(v.to_be_bytes());
    }
    for v in r.camera {
        b.extend(v.to_be_bytes());
    }
    b.extend((n as u16).to_be_bytes());
    for s in &r.subjects[..n] {
        b.extend(s.id.to_be_bytes());
        b.push(s.class as u8);
        b.push(s.roles);
        for v in s.pos {
            b.extend(v.to_be_bytes());
        }
        for v in [s.size.0, s.size.1, s.yaw, s.label_dy] {
            b.extend(v.to_be_bytes());
        }
        b.extend(s.fuse.to_be_bytes());
        if s.roles & ROLE_LABEL != 0 {
            let text = s.label.as_bytes();
            let len = text.len().min(MAX_TEXT);
            b.extend((len as u32).to_be_bytes());
            b.extend(&text[..len]);
        }
    }
    w.write_all(&b)?;
    w.flush()
}

pub fn read_world(r: &mut impl Read) -> io::Result<World> {
    let n = u32_(r)?;
    if n > MAX_ITEMS {
        return Err(io::Error::other(format!("{n} world items")));
    }
    let mut world = World::default();
    for _ in 0..n {
        match u8_(r)? {
            b'L' => world.labels.push(read_label(r)?),
            b'H' => world.lines.push(read_lines(r)?),
            t => return Err(io::Error::other(format!("unknown world item {t}"))),
        }
    }
    Ok(world)
}

fn read_label(r: &mut impl Read) -> io::Result<Label> {
    let id = u32_(r)? as i32;
    let mut b = [0; 2];
    r.read_exact(&mut b)?;
    let n = u16::from_be_bytes(b);
    if n > MAX_OPS {
        return Err(io::Error::other(format!("{n} ops in one label")));
    }
    let ops = (0..n).map(|_| read_op(r)).collect::<io::Result<_>>()?;
    Ok(Label { id, ops })
}

fn read_lines(r: &mut impl Read) -> io::Result<Lines> {
    let id = u32_(r)? as i32;
    let mut b = [0; 2];
    r.read_exact(&mut b)?;
    let n = u16::from_be_bytes(b);
    if n > MAX_SEGMENTS {
        return Err(io::Error::other(format!("{n} hitbox segments")));
    }
    let point = |r: &mut dyn Read| -> io::Result<[f32; 3]> {
        let mut p = [0.0; 3];
        for v in &mut p {
            let mut b = [0; 4];
            r.read_exact(&mut b)?;
            *v = f32::from_be_bytes(b);
        }
        Ok(p)
    };
    let segments = (0..n)
        .map(|_| {
            Ok(Segment {
                from: point(r)?,
                to: point(r)?,
                argb: u32_(r)?,
            })
        })
        .collect::<io::Result<_>>()?;
    Ok(Lines { id, segments })
}

fn read_op(r: &mut impl Read) -> io::Result<Op> {
    match u8_(r)? {
        b'Q' => Ok(Op::Quad {
            pts: [point2(r)?, point2(r)?, point2(r)?, point2(r)?],
            argb: u32_(r)?,
        }),
        b'T' => Ok(Op::Text {
            right: f32_(r)?,
            up: f32_(r)?,
            scale: f32_(r)?,
            argb: u32_(r)?,
            shadow: u8_(r)? != 0,
            text: text(r)?,
        }),
        t => Err(io::Error::other(format!("unknown label op {t}"))),
    }
}

fn text(r: &mut impl Read) -> io::Result<String> {
    let len = u32_(r)? as usize;
    if len > MAX_TEXT {
        return Err(io::Error::other(format!("{len} byte text")));
    }
    let mut s = vec![0; len];
    r.read_exact(&mut s)?;
    Ok(String::from_utf8_lossy(&s).into_owned())
}

fn point2(r: &mut impl Read) -> io::Result<[f32; 2]> {
    Ok([f32_(r)?, f32_(r)?])
}

fn u8_(r: &mut impl Read) -> io::Result<u8> {
    let mut b = [0; 1];
    r.read_exact(&mut b)?;
    Ok(b[0])
}

fn u32_(r: &mut impl Read) -> io::Result<u32> {
    let mut b = [0; 4];
    r.read_exact(&mut b)?;
    Ok(u32::from_be_bytes(b))
}

fn f32_(r: &mut impl Read) -> io::Result<f32> {
    u32_(r).map(f32::from_bits)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_the_request_java_reads() {
        let r = Request {
            partial: 0.5,
            view: [90.0, 10.0],
            camera: [1.0, 2.0, 3.0],
            subjects: vec![Subject {
                id: 7,
                class: Class::PrimedTnt,
                roles: ROLE_LABEL | ROLE_COUNTDOWN,
                pos: [4.0, 5.0, 6.0],
                size: (0.98, 0.98),
                yaw: 0.0,
                label_dy: -0.25,
                fuse: 40,
                label: "Ab".into(),
            }],
        };
        let mut out = Vec::new();
        write_request(&mut out, &r).unwrap();
        let mut want = vec![b'A'];
        for v in [0.5f32, 90.0, 10.0] {
            want.extend(v.to_be_bytes());
        }
        for v in [1.0f64, 2.0, 3.0] {
            want.extend(v.to_be_bytes());
        }
        want.extend([0, 1]);
        want.extend(7i32.to_be_bytes());
        want.extend([2, 3]);
        for v in [4.0f64, 5.0, 6.0] {
            want.extend(v.to_be_bytes());
        }
        for v in [0.98f32, 0.98, 0.0, -0.25] {
            want.extend(v.to_be_bytes());
        }
        want.extend(40i32.to_be_bytes());
        want.extend(2u32.to_be_bytes());
        want.extend(b"Ab");
        assert_eq!(out, want);
    }

    #[test]
    fn reads_labels() {
        let mut b = 1u32.to_be_bytes().to_vec();
        b.push(b'L');
        b.extend((-3i32).to_be_bytes());
        b.extend(2u16.to_be_bytes());
        b.push(b'Q');
        let quad: [[f32; 2]; 4] = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        for v in quad.iter().flatten() {
            b.extend(v.to_be_bytes());
        }
        b.extend(0x4000_0000u32.to_be_bytes());
        b.push(b'T');
        for v in [0.0f32, 0.5, 0.02] {
            b.extend(v.to_be_bytes());
        }
        b.extend(0xffff_ffffu32.to_be_bytes());
        b.push(1);
        b.extend(3u32.to_be_bytes());
        b.extend(b"TNT");
        let w = read_world(&mut b.as_slice()).unwrap();
        assert_eq!(w.labels.len(), 1);
        assert_eq!(w.labels[0].id, -3);
        assert_eq!(
            w.labels[0].ops[0],
            Op::Quad {
                pts: quad,
                argb: 0x4000_0000
            }
        );
        assert!(
            matches!(&w.labels[0].ops[1], Op::Text { text, shadow: true, .. } if text == "TNT")
        );
    }

    #[test]
    fn reads_hitbox_lines() {
        let mut b = 1u32.to_be_bytes().to_vec();
        b.push(b'H');
        b.extend(9i32.to_be_bytes());
        b.extend(1u16.to_be_bytes());
        for v in [0.0f32, 0.0, 0.0, 1.0, 2.0, 3.0] {
            b.extend(v.to_be_bytes());
        }
        b.extend(0xff00_00ffu32.to_be_bytes());
        let w = read_world(&mut b.as_slice()).unwrap();
        assert_eq!(w.lines[0].id, 9);
        assert_eq!(
            w.lines[0].segments[0],
            Segment {
                from: [0.0; 3],
                to: [1.0, 2.0, 3.0],
                argb: 0xff00_00ff
            }
        );
    }

    #[test]
    fn reads_an_empty_hitbox_record() {
        let mut b = 1u32.to_be_bytes().to_vec();
        b.push(b'H');
        b.extend(9i32.to_be_bytes());
        b.extend(0u16.to_be_bytes());
        let w = read_world(&mut b.as_slice()).unwrap();
        assert_eq!(
            w.lines,
            vec![Lines {
                id: 9,
                segments: Vec::new()
            }]
        );
    }

    #[test]
    fn rejects_a_huge_count_and_an_unknown_item() {
        assert!(read_world(&mut u32::MAX.to_be_bytes().as_slice()).is_err());
        let mut b = 1u32.to_be_bytes().to_vec();
        b.push(b'?');
        assert!(read_world(&mut b.as_slice()).is_err());
    }
}
