//! The loopback protocol with `net.ferrocraft.host.Host`. All numbers are big-endian.
//!
//! Rust to Java, one frame: `'F'`, i32 width, i32 height, i32 gui scale, u8 force unicode,
//! i32 mouse x, i32 mouse y, u8 mouse button mask, u16 key count, i32 LWJGL key codes.
//! Java to Rust, per frame: u32 command count, then the commands of `Sink.java`.

use std::io::{self, Read, Write};

/// What Rust tells the JVM each frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Input {
    pub width: i32,
    pub height: i32,
    pub gui_scale: i32,
    pub unicode: bool,
    pub mouse: (i32, i32),
    /// Bit 0 left, bit 1 right, bit 2 middle.
    pub buttons: u8,
    /// LWJGL 2 key codes that are down.
    pub keys: Vec<i32>,
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
}

/// Text above this many bytes means a corrupt stream, not a long string.
const MAX_TEXT: usize = 1 << 16;
/// A frame above this many commands means a corrupt stream.
const MAX_COMMANDS: u32 = 1 << 16;

pub fn write_input(w: &mut impl Write, i: &Input) -> io::Result<()> {
    let mut b = Vec::with_capacity(32 + i.keys.len() * 4);
    b.push(b'F');
    for v in [i.width, i.height, i.gui_scale] {
        b.extend(v.to_be_bytes());
    }
    b.push(u8::from(i.unicode));
    b.extend(i.mouse.0.to_be_bytes());
    b.extend(i.mouse.1.to_be_bytes());
    b.push(i.buttons);
    let n = u16::try_from(i.keys.len()).map_err(|_| io::Error::other("too many keys"))?;
    b.extend(n.to_be_bytes());
    for k in &i.keys {
        b.extend(k.to_be_bytes());
    }
    w.write_all(&b)?;
    w.flush()
}

pub fn read_commands(r: &mut impl Read) -> io::Result<Vec<DrawCmd>> {
    let n = u32_(r)?;
    if n > MAX_COMMANDS {
        return Err(io::Error::other(format!("{n} commands in one frame")));
    }
    (0..n).map(|_| read_command(r)).collect()
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
            let len = u32_(r)? as usize;
            if len > MAX_TEXT {
                return Err(io::Error::other(format!("{len} byte text")));
            }
            let mut s = vec![0; len];
            r.read_exact(&mut s)?;
            Ok(DrawCmd::Text {
                text: String::from_utf8_lossy(&s).into_owned(),
                pos,
                argb,
                shadow,
                translate,
                scale,
            })
        }
        t => Err(io::Error::other(format!("unknown command tag {t}"))),
    }
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

    /// Bytes as `Sink.take` writes them: one rect and one text with a scaled matrix.
    fn frame() -> Vec<u8> {
        let mut b = 2u32.to_be_bytes().to_vec();
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
        b
    }

    #[test]
    fn reads_the_java_frame_layout() {
        let cmds = read_commands(&mut frame().as_slice()).unwrap();
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
            ]
        );
    }

    #[test]
    fn rejects_a_cut_frame_and_an_unknown_tag() {
        let f = frame();
        assert!(read_commands(&mut &f[..f.len() - 1]).is_err());
        let mut bad = 1u32.to_be_bytes().to_vec();
        bad.push(b'?');
        assert!(read_commands(&mut bad.as_slice()).is_err());
    }

    #[test]
    fn writes_the_frame_header_java_reads() {
        let mut out = Vec::new();
        let input = Input {
            width: 854,
            height: 480,
            gui_scale: 0,
            unicode: false,
            mouse: (5, 6),
            buttons: 1,
            keys: vec![17],
        };
        write_input(&mut out, &input).unwrap();
        assert_eq!(out[0], b'F');
        assert_eq!(out.len(), 1 + 12 + 1 + 8 + 1 + 2 + 4);
        assert_eq!(&out[out.len() - 6..out.len() - 4], &[0, 1]);
        assert_eq!(&out[out.len() - 4..], &17i32.to_be_bytes());
    }
}
