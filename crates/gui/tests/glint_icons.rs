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

#[test]
#[ignore = "requires the vanilla 1.8.9 client assets and a GPU or software adapter"]
fn dyed_leather_tunic_icon_is_blue() {
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
    let tunic = |dye: Option<i32>| ItemStack {
        id: 299,
        count: 1,
        damage: 0,
        nbt: dye.map(|c| Nbt {
            name: String::new(),
            root: vec![(
                "display".into(),
                Tag::Compound(vec![("color".into(), Tag::Int(c))]),
            )],
        }),
    };
    let mut draw = |s: &ItemStack| {
        let mut g = Gui::new(GuiScale::new(854, 480, 0, false), r.font(), 0);
        {
            let mut c = GuiCanvas::new(&mut g);
            c.fill(0, 0, 427, 240, 0xFF30_3030);
            icons.draw(&mut c, s, 20, 20);
        }
        let quads = g.into_quads();
        r.render_to_rgba(&Camera::default(), &quads)
    };
    let plain = draw(&tunic(None));
    let blue = draw(&tunic(Some(0x0000FF)));
    // Sum channels over the image: blue dye must raise blue and cut red against the brown default.
    let sum = |img: &(u32, u32, Vec<u8>), ch: usize| {
        img.2.chunks(4).map(|p| u64::from(p[ch])).sum::<u64>()
    };
    assert!(sum(&blue, 2) > sum(&plain, 2));
    assert!(sum(&blue, 0) < sum(&plain, 0));
    if let Some(dir) = std::env::var_os("GLINT_PROOF_DIR") {
        let dir = std::path::Path::new(&dir);
        encode_png(&dir.join("tunic_blue.png"), blue.0, blue.1, &blue.2).unwrap();
        encode_png(&dir.join("tunic_brown.png"), plain.0, plain.1, &plain.2).unwrap();
    }
}
