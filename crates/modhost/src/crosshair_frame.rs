//! A mod crosshair (PolyCrosshair) reaches the screen as the frame `GlState.java` builds: the picture it draws with a
//! `DynamicTexture` as a define and an image command, and the crosshair element as owned by Java. Rust then draws the
//! mod picture and no vanilla crosshair. `CROSSHAIR_PROOF_DIR` and `FERROCRAFT_CLIENT_JAR` make the test write the
//! frames before and after as PNG files.

use super::*;
use gui::{Frame, Gui, GuiScale, Hud};
use render::{Camera, Quad, Renderer};

const NAME: &str = "tex:3f0c";

/// An 11 x 11 picture: a green plus with a one pixel gap in the middle.
fn plus_png() -> Vec<u8> {
    let mut px = vec![0u8; 11 * 11 * 4];
    for i in 0..11 {
        for (x, y) in [(i, 5), (5, i)] {
            if i != 5 {
                px[(y * 11 + x) * 4..][..4].copy_from_slice(&[0x20, 0xff, 0x40, 0xff]);
            }
        }
    }
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, 11, 11);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(&px).unwrap();
    out
}

/// What `Sink.take` writes for a quad at the screen center with the source alpha blend, and the crosshair element owned.
fn java_frame() -> Vec<u8> {
    let png = plus_png();
    let mut b = vec![0];
    b.extend(3u32.to_be_bytes());
    b.push(b'D');
    b.extend((NAME.len() as u32).to_be_bytes());
    b.extend(NAME.as_bytes());
    b.extend((png.len() as u32).to_be_bytes());
    b.extend(&png);
    b.push(b'I');
    b.extend((NAME.len() as u32).to_be_bytes());
    b.extend(NAME.as_bytes());
    for v in [202.0f32, 109.0, 224.0, 131.0] {
        b.extend(v.to_be_bytes());
    }
    b.extend(0xffff_ffffu32.to_be_bytes());
    b.extend([0, 0]);
    b.push(b'H');
    b.extend((1u32 << hud::Element::Crosshair as u32).to_be_bytes());
    b
}

fn quads(r: &Renderer, mods: Option<&ModHost>, textures: &mut Vec<(String, Vec<u8>)>) -> Vec<Quad> {
    let mut hud = Hud::default();
    let mut frame = Frame::default();
    if let Some(m) = mods {
        m.hud_owned().leave_out(&mut frame);
    }
    let mut g = Gui::new(GuiScale::new(427, 240, 0, false), r.font(), 0);
    hud.draw(&mut g, &frame);
    if let Some(m) = mods {
        m.draw(&mut g, textures);
    }
    g.into_quads()
}

#[test]
fn a_mod_crosshair_is_drawn_and_the_vanilla_one_is_not() {
    let jar = std::env::var_os("FERROCRAFT_CLIENT_JAR").map(std::path::PathBuf::from);
    let mut r = Renderer::new_offscreen(427, 240, jar.as_deref());
    let host = ModHost {
        child: Arc::new(Mutex::new(
            std::process::Command::new("sleep")
                .arg("10")
                .spawn()
                .unwrap(),
        )),
        staging: None,
        link: tests::link(),
    };
    let mut none = Vec::new();
    let before = quads(&r, None, &mut none);
    let (screen, cmds) = wire::read_frame(&mut java_frame().as_slice()).unwrap();
    apply_frame(&mut host.link.state.lock().unwrap(), screen, cmds);
    let mut textures = Vec::new();
    let after = quads(&r, Some(&host), &mut textures);

    let invert = |q: &[Quad]| {
        q.iter()
            .filter(|q| q.blend == render::Blend::Invert)
            .count()
    };
    assert_eq!(
        (invert(&before), invert(&after)),
        (1, 0),
        "only vanilla draws an inverting crosshair"
    );
    let mod_pictures = after
        .iter()
        .filter(|q| q.tex.starts_with("modhost/tex:"))
        .count();
    assert_eq!(mod_pictures, 1, "the mod picture is in the frame");
    let vanilla = |q: &[Quad]| {
        q.iter()
            .filter(|q| q.tex == "textures/gui/icons.png" && q.blend == render::Blend::Invert)
            .count()
    };
    assert_eq!((vanilla(&before), vanilla(&after)), (1, 0));
    assert_eq!(textures.len(), 1);

    if let (Some(dir), true) = (std::env::var_os("CROSSHAIR_PROOF_DIR"), jar.is_some()) {
        for (name, quads) in [("before", &before), ("after", &after)] {
            for (n, png) in &textures {
                r.register_texture(n, png);
            }
            let (w, h, rgba) = r.render_to_rgba(&Camera::default(), quads);
            render::encode_png(
                &std::path::Path::new(&dir).join(format!("{name}.png")),
                w,
                h,
                &rgba,
            )
            .unwrap();
        }
    }
}
