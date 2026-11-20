//! Enchanted item icons with the glint, offscreen with the real 1.8.9 assets.
//! `GLINT_PROOF_DIR` keeps `glint_icons.png`.

use gui::inventory::icons::Icons;
use gui::inventory::Canvas;
use gui::{Gui, GuiCanvas, GuiScale};
use render::{encode_png, Camera, Renderer};
use world::inventory::{ItemStack, Nbt, Tag};

fn enchanted(id: i16, damage: i16) -> ItemStack {
    ItemStack {
        id,
        count: 1,
        damage,
        nbt: Some(Nbt {
            name: String::new(),
            root: vec![(
                "ench".into(),
                Tag::List(vec![Tag::Compound(vec![
                    ("id".into(), Tag::Short(16)),
                    ("lvl".into(), Tag::Short(1)),
                ])]),
            )],
        }),
    }
}

#[test]
#[ignore = "requires the vanilla 1.8.9 client assets and a GPU or software adapter"]
fn enchanted_icons_are_drawn_with_glint() {
    let jar =
        std::path::PathBuf::from(std::env::var_os("FERROCRAFT_CLIENT_JAR").expect("client jar"));
    let icons = Icons::new(|p: &str| -> Option<Vec<u8>> {
        let mut z = zip::ZipArchive::new(std::fs::File::open(&jar).ok()?).ok()?;
        let mut out = Vec::new();
        std::io::Read::read_to_end(
            &mut z.by_name(&format!("assets/minecraft/{p}")).ok()?,
            &mut out,
        )
        .ok()?;
        Some(out)
    });
    let mut r = Renderer::new_offscreen(854, 480, Some(&jar));
    // diamond sword, bow, diamond chestplate, leather boots (two layers), stone, golden apple
    let items = [
        enchanted(276, 0),
        enchanted(261, 0),
        enchanted(311, 0),
        enchanted(301, 0),
        enchanted(1, 0),
        ItemStack {
            id: 322,
            count: 1,
            damage: 1,
            nbt: None,
        },
    ];
    let draw = |r: &mut Renderer, glint: bool| {
        let mut g = Gui::new(GuiScale::new(854, 480, 0, false), r.font(), 0);
        {
            let mut c = GuiCanvas::new(&mut g);
            c.fill(0, 0, 427, 240, 0xFF30_3030);
            for (i, s) in items.iter().enumerate() {
                let mut s = s.clone();
                if !glint {
                    s.nbt = None;
                    if s.id == 322 {
                        s.damage = 0;
                    }
                }
                icons.draw(&mut c, &s, 20 + 24 * i as i32, 20);
            }
        }
        let quads = g.into_quads();
        r.render_to_rgba(&Camera::default(), &quads)
    };
    let plain = draw(&mut r, false);
    let glowing = draw(&mut r, true);
    let n = plain
        .2
        .chunks(4)
        .zip(glowing.2.chunks(4))
        .filter(|(a, b)| a != b)
        .count();
    assert!(n > 300, "only {n} pixels changed with glint");
    if let Some(dir) = std::env::var_os("GLINT_PROOF_DIR") {
        let dir = std::path::Path::new(&dir);
        encode_png(
            &dir.join("glint_icons.png"),
            glowing.0,
            glowing.1,
            &glowing.2,
        )
        .unwrap();
        encode_png(&dir.join("plain_icons.png"), plain.0, plain.1, &plain.2).unwrap();
    }
}
