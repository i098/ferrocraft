//! A player head item icon draws the owner's skin, offscreen with the real 1.8.9 assets.
//! `HEAD_PROOF_DIR` keeps `head_icon.png`.

use gui::inventory::icons::{head_texture, Icons};
use gui::{Gui, GuiCanvas, GuiScale};
use render::{encode_png, Camera, Image, Renderer};
use world::inventory::{ItemStack, Nbt, Tag};

const OWNER: u128 = 0x069a79f4_44e9_4726_a5be_fca90e38aaf5;

fn head() -> ItemStack {
    let s = |v: &str| Tag::String(v.into());
    ItemStack {
        id: 397,
        count: 1,
        damage: 3,
        nbt: Some(Nbt {
            name: String::new(),
            root: vec![(
                "SkullOwner".into(),
                Tag::Compound(vec![
                    ("Id".into(), s("069a79f4-44e9-4726-a5be-fca90e38aaf5")),
                    ("Name".into(), s("Notch")),
                ]),
            )],
        }),
    }
}

/// A skin with a green front face, red and yellow sides and blue elsewhere.
fn skin() -> Image {
    let mut rgba = vec![0; 64 * 64 * 4];
    for y in 0..64 {
        for x in 0..64 {
            let i = (y * 64 + x) * 4;
            let c: [u8; 4] = match (x / 8, y / 8) {
                (1, 1) => [0, 255, 0, 255],
                (2, 1) => [255, 0, 0, 255],
                (0, 1) => [255, 255, 0, 255],
                (4..=7, 0..=1) => [0, 0, 0, 0],
                _ => [0, 0, 255, 255],
            };
            rgba[i..i + 4].copy_from_slice(&c);
        }
    }
    Image {
        width: 64,
        height: 64,
        rgba,
    }
}

#[test]
#[ignore = "requires the vanilla 1.8.9 client assets and a GPU or software adapter"]
fn player_head_icon_draws_the_owner_skin() {
    let jar =
        std::path::PathBuf::from(std::env::var_os("FERROCRAFT_CLIENT_JAR").expect("client jar"));
    let mut icons = Icons::new(|p: &str| -> Option<Vec<u8>> {
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
    let draw = |r: &mut Renderer, icons: &Icons| {
        let mut g = Gui::new(GuiScale::new(854, 480, 0, false), r.font(), 0);
        {
            let mut c = GuiCanvas::new(&mut g);
            gui::inventory::Canvas::fill(&mut c, 0, 0, 427, 240, 0xFF30_3030);
            icons.draw(&mut c, &head(), 100, 100);
        }
        let quads = g.into_quads();
        r.render_to_rgba(&Camera::default(), &quads)
    };
    let before = draw(&mut r, &icons);
    r.register_image(&head_texture(OWNER), &skin());
    icons.heads.insert(OWNER);
    let after = draw(&mut r, &icons);
    let count =
        |img: &(u32, u32, Vec<u8>), f: fn(&[u8]) -> bool| img.2.chunks(4).filter(|p| f(p)).count();
    let green = |p: &[u8]| p[1] > 150 && p[0] < 80 && p[2] < 80;
    let blue = |p: &[u8]| p[2] > 150 && p[0] < 80 && p[1] < 80;
    if let Some(dir) = std::env::var_os("HEAD_PROOF_DIR") {
        let dir = std::path::Path::new(&dir);
        encode_png(
            &dir.join("head_icon_before.png"),
            before.0,
            before.1,
            &before.2,
        )
        .unwrap();
        encode_png(&dir.join("head_icon.png"), after.0, after.1, &after.2).unwrap();
    }
    assert_eq!(
        count(&before, green) + count(&before, blue),
        0,
        "default skin leaked skin colors"
    );
    let red = |p: &[u8]| p[0] > 120 && p[1] < 60 && p[2] < 60;
    let yellow = |p: &[u8]| p[0] > 120 && p[1] > 120 && p[2] < 60;
    let face = count(&after, green) + count(&after, red) + count(&after, yellow);
    assert!(face > 30, "owner face not drawn: {face}");
    assert!(count(&after, blue) > 50, "owner skin not drawn");
}
