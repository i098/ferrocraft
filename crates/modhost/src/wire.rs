//! The loopback protocol with `net.ferrocraft.host.Host`. All numbers are big-endian.
//!
//! Java to Rust at connect: `'H'`, then the 32 ASCII bytes of the token that Rust gave the JVM in its
//! environment (`FERROCRAFT_BRIDGE_TOKEN`); a connection with another token is closed.
//!
//! Rust to Java, one frame: `'F'`, i32 width in pixels, i32 height in pixels, i32 GUI scale setting,
//! u8 force unicode, u8 screen open, i32 mouse x, i32 mouse y (pixels, origin top left), u16 event
//! count, then per event: u8 kind, i32 argument, u8 state, u16 character. Kinds: `'K'` key (LWJGL code; state
//! bit 0 down, bit 1 repeat; character is the UTF-16 unit the key typed, 0 for none), `'B'` mouse button (LWJGL
//! number 0 left, 1 right, 2 middle; state bit 0 down), `'W'` wheel (argument is the LWJGL delta, 120 per
//! notch). The character is 0 for buttons and wheel. State bit 2 on any kind: a screen was open when the event
//! arrived.
//! Java to Rust, per frame: u8 mod screen open (a screen a mod opened is current), u32 command count,
//! then the commands of `Sink.java`: `'R'` rect, `'G'` gradient, `'T'` text, `'D'` define (a resource
//! by name: u32 length and UTF-8 bytes, then u32 length and bytes of a PNG or SVG file; sent once),
//! `'I'` image (name, four f32 corners, i32 ARGB tint, u8 recolor: an SVG takes the tint's color),
//! `'C'` clip (u8 on, then four f32 corners when on).

use std::io::{self, Read, Write};

/// A key or mouse transition, in the order it happened.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    Key {
        code: i32,
        pressed: bool,
        repeat: bool,
        /// What the key typed, as LWJGL's `getEventCharacter`: 0 when it typed nothing.
        ch: u16,
    },
    Button {
        button: i32,
        pressed: bool,
    },
    Wheel(i32),
}

/// What Rust tells the JVM each frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Input {
    pub width: i32,
    pub height: i32,
    pub gui_scale: i32,
    pub unicode: bool,
    /// A menu, the container or chat is open.
    pub screen: bool,
    pub mouse: (i32, i32),
    /// Transitions since the previous frame, each with whether a screen was open when it arrived.
    pub events: Vec<(Event, bool)>,
}

/// One draw call of a mod, in scaled GUI pixels.
#[derive(Clone, Debug, PartialEq)]
pub enum DrawCmd {
    Rect {
        rect: [f32; 4],
        argb: u32,
    },
    Gradient {
        rect: [f32; 4],
        top: u32,
        bottom: u32,
    },
    Text {
        text: String,
        pos: [f32; 2],
        argb: u32,
        shadow: bool,
        /// GL matrix at the time of the call: translation, then uniform scale.
        translate: [f32; 2],
        scale: f32,
    },
    /// A PNG or SVG file that later `Image` commands name.
    Define {
        name: String,
        data: Vec<u8>,
    },
    Image {
        name: String,
        rect: [f32; 4],
        argb: u32,
        /// An SVG is painted in the color of `argb` instead of its own.
        recolor: bool,
    },
    /// Cut the commands after this one to the rectangle; `None` lifts the cut.
    Clip(Option<[f32; 4]>),
}

/// A resource file above this many bytes means a corrupt stream.
const MAX_DATA: usize = 1 << 24;

/// Text above this many bytes means a corrupt stream, not a long string.
const MAX_TEXT: usize = 1 << 16;
/// A frame above this many commands means a corrupt stream.
const MAX_COMMANDS: u32 = 1 << 16;

pub fn write_input(w: &mut impl Write, i: &Input) -> io::Result<()> {
    let mut b = Vec::with_capacity(32 + i.events.len() * 8);
    b.push(b'F');
    for v in [i.width, i.height, i.gui_scale] {
        b.extend(v.to_be_bytes());
    }
    b.push(u8::from(i.unicode));
    b.push(u8::from(i.screen));
    b.extend(i.mouse.0.to_be_bytes());
    b.extend(i.mouse.1.to_be_bytes());
    let n = u16::try_from(i.events.len()).map_err(|_| io::Error::other("too many events"))?;
    b.extend(n.to_be_bytes());
    for &(e, screen) in &i.events {
        let (kind, arg, state, ch) = match e {
            Event::Key {
                code,
                pressed,
                repeat,
                ch,
            } => (b'K', code, u8::from(pressed) | u8::from(repeat) << 1, ch),
            Event::Button { button, pressed } => (b'B', button, u8::from(pressed), 0),
            Event::Wheel(delta) => (b'W', delta, 0, 0),
        };
        b.push(kind);
        b.extend(arg.to_be_bytes());
        b.push(state | u8::from(screen) << 2);
        b.extend(ch.to_be_bytes());
    }
    w.write_all(&b)?;
    w.flush()
}

/// What `EntityPlayerSP.onLivingUpdate` reads from the world, sent once per game tick.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PlayerState {
    pub on_ground: bool,
    pub collided_horizontally: bool,
    pub food: i32,
    pub allow_flying: bool,
    pub flying: bool,
    pub using_item: bool,
}

/// What the mods leave of the player's input after `onLivingUpdate`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayerInput {
    pub sprinting: bool,
    pub sneaking: bool,
}

pub fn write_player(w: &mut impl Write, p: &PlayerState) -> io::Result<()> {
    let mut b = vec![
        b'P',
        u8::from(p.on_ground),
        u8::from(p.collided_horizontally),
    ];
    b.extend(p.food.to_be_bytes());
    b.push(u8::from(p.allow_flying) | u8::from(p.flying) << 1 | u8::from(p.using_item) << 2);
    w.write_all(&b)?;
    w.flush()
}

pub fn read_player(r: &mut impl Read) -> io::Result<PlayerInput> {
    Ok(PlayerInput {
        sprinting: u8_(r)? != 0,
        sneaking: u8_(r)? != 0,
    })
}

/// Read one frame from Java: whether a mod screen is open, and the draw commands.
pub fn read_frame(r: &mut impl Read) -> io::Result<(bool, Vec<DrawCmd>)> {
    let screen = u8_(r)? != 0;
    let n = u32_(r)?;
    if n > MAX_COMMANDS {
        return Err(io::Error::other(format!("{n} commands in one frame")));
    }
    let cmds = (0..n).map(|_| read_command(r)).collect::<io::Result<_>>()?;
    Ok((screen, cmds))
}

fn read_command(r: &mut impl Read) -> io::Result<DrawCmd> {
    match u8_(r)? {
        b'R' => Ok(DrawCmd::Rect {
            rect: rect(r)?,
            argb: u32_(r)?,
        }),
        b'G' => Ok(DrawCmd::Gradient {
            rect: rect(r)?,
            top: u32_(r)?,
            bottom: u32_(r)?,
        }),
        b'T' => {
            let pos = [f32_(r)?, f32_(r)?];
            let argb = u32_(r)?;
            let shadow = u8_(r)? != 0;
            let translate = [f32_(r)?, f32_(r)?];
            let scale = f32_(r)?;
            let text = string(r, MAX_TEXT)?;
            Ok(DrawCmd::Text {
                text,
                pos,
                argb,
                shadow,
                translate,
                scale,
            })
        }
        b'D' => Ok(DrawCmd::Define {
            name: string(r, MAX_TEXT)?,
            data: bytes(r, MAX_DATA)?,
        }),
        b'I' => Ok(DrawCmd::Image {
            name: string(r, MAX_TEXT)?,
            rect: rect(r)?,
            argb: u32_(r)?,
            recolor: u8_(r)? != 0,
        }),
        b'C' => Ok(DrawCmd::Clip(if u8_(r)? != 0 {
            Some(rect(r)?)
        } else {
            None
        })),
        t => Err(io::Error::other(format!("unknown command tag {t}"))),
    }
}

fn string(r: &mut impl Read, max: usize) -> io::Result<String> {
    Ok(String::from_utf8_lossy(&bytes(r, max)?).into_owned())
}

fn bytes(r: &mut impl Read, max: usize) -> io::Result<Vec<u8>> {
    let len = u32_(r)? as usize;
    if len > max {
        return Err(io::Error::other(format!("{len} byte string")));
    }
    let mut s = vec![0; len];
    r.read_exact(&mut s)?;
    Ok(s)
}

fn rect(r: &mut impl Read) -> io::Result<[f32; 4]> {
    Ok([f32_(r)?, f32_(r)?, f32_(r)?, f32_(r)?])
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

    /// Bytes as `Sink.take(true)` writes them: the screen flag, a rect, a text with a scaled matrix, a
    /// resource, an image and the two forms of clip.
    fn frame() -> Vec<u8> {
        let mut b = vec![1];
        b.extend(6u32.to_be_bytes());
        b.push(b'R');
        for v in [1.0f32, 2.0, 21.0, 22.0] {
            b.extend(v.to_be_bytes());
        }
        b.extend(0x80ff0000u32.to_be_bytes());
        b.push(b'T');
        for v in [3.0f32, 4.0] {
            b.extend(v.to_be_bytes());
        }
        b.extend(0xffffffffu32.to_be_bytes());
        b.push(1);
        for v in [10.0f32, 20.0, 2.0] {
            b.extend(v.to_be_bytes());
        }
        b.extend(1u32.to_be_bytes());
        b.push(b'W');
        b.push(b'D');
        b.extend(3u32.to_be_bytes());
        b.extend(b"a.b");
        b.extend(2u32.to_be_bytes());
        b.extend([7, 8]);
        b.push(b'I');
        b.extend(3u32.to_be_bytes());
        b.extend(b"a.b");
        for v in [5.0f32, 6.0, 7.0, 8.0] {
            b.extend(v.to_be_bytes());
        }
        b.extend(0xff00ff00u32.to_be_bytes());
        b.push(1);
        b.extend([b'C', 0]);
        b.extend([b'C', 1]);
        for v in [1.0f32, 2.0, 3.0, 4.0] {
            b.extend(v.to_be_bytes());
        }
        b
    }

    #[test]
    fn reads_the_java_frame_layout() {
        let (screen, cmds) = read_frame(&mut frame().as_slice()).unwrap();
        assert!(screen);
        assert_eq!(
            cmds,
            vec![
                DrawCmd::Rect {
                    rect: [1.0, 2.0, 21.0, 22.0],
                    argb: 0x80ff0000
                },
                DrawCmd::Text {
                    text: "W".into(),
                    pos: [3.0, 4.0],
                    argb: 0xffffffff,
                    shadow: true,
                    translate: [10.0, 20.0],
                    scale: 2.0
                },
                DrawCmd::Define {
                    name: "a.b".into(),
                    data: vec![7, 8]
                },
                DrawCmd::Image {
                    name: "a.b".into(),
                    rect: [5.0, 6.0, 7.0, 8.0],
                    argb: 0xff00ff00,
                    recolor: true
                },
                DrawCmd::Clip(None),
                DrawCmd::Clip(Some([1.0, 2.0, 3.0, 4.0])),
            ]
        );
    }

    #[test]
    fn rejects_a_cut_frame_and_an_unknown_tag() {
        let f = frame();
        assert!(read_frame(&mut &f[..f.len() - 1]).is_err());
        let mut bad = vec![0];
        bad.extend(1u32.to_be_bytes());
        bad.push(b'?');
        assert!(read_frame(&mut bad.as_slice()).is_err());
    }

    #[test]
    fn writes_the_frame_java_reads() {
        let mut out = Vec::new();
        let input = Input {
            width: 854,
            height: 480,
            gui_scale: 2,
            unicode: true,
            screen: true,
            mouse: (5, 6),
            events: vec![
                (
                    Event::Key {
                        code: 17,
                        pressed: true,
                        repeat: true,
                        ch: 0x00e9,
                    },
                    false,
                ),
                (
                    Event::Button {
                        button: 1,
                        pressed: false,
                    },
                    true,
                ),
                (Event::Wheel(-120), true),
            ],
        };
        write_input(&mut out, &input).unwrap();
        let mut want = vec![b'F'];
        for v in [854i32, 480, 2] {
            want.extend(v.to_be_bytes());
        }
        want.extend([1, 1]);
        want.extend(5i32.to_be_bytes());
        want.extend(6i32.to_be_bytes());
        want.extend([0, 3]);
        want.push(b'K');
        want.extend(17i32.to_be_bytes());
        want.push(0b011);
        want.extend(0x00e9u16.to_be_bytes());
        want.push(b'B');
        want.extend(1i32.to_be_bytes());
        want.push(0b100);
        want.extend([0, 0]);
        want.push(b'W');
        want.extend((-120i32).to_be_bytes());
        want.push(0b100);
        want.extend([0, 0]);
        assert_eq!(out, want);
    }
}
