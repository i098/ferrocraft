//! Draws the HUD over an empty world offscreen and writes a PNG (`HUD_PNG` overrides the path).
//! `FERROCRAFT_CLIENT_JAR` gives the 1.8.9 client jar for real textures and the font.

use ferrocraft_net_client::styled;
use gui::inventory::icons::Icons;
use gui::scoreboard::TeamInfo;
use gui::tablist::TabEntry;
use gui::{debug, Frame, Gui, GuiScale, HotbarItems, Hud, Metrics, Status};
use render::{encode_png, Camera, Renderer};
use world::inventory::{ItemStack, Slot};

#[test]
fn draws_hud_to_png() {
    let jar = std::env::var_os("FERROCRAFT_CLIENT_JAR").map(std::path::PathBuf::from);
    let mut r = Renderer::new_offscreen(854, 480, jar.as_deref());
    let mut hud = Hud::default();
    for json in [
        r#"{"translate":"chat.type.text","with":["Steve","hello world"]}"#,
        r#"{"text":"Alex joined the game","color":"yellow"}"#,
    ] {
        hud.chat.print(styled(json), 0, 0, false, Metrics(r.font()));
    }
    hud.selected_slot = 2;
    hud.set_action_bar("Action bar".into());
    let names = ["Steve", "Alex", "Notch"];
    for (i, n) in names.iter().enumerate() {
        hud.tablist.add(TabEntry {
            uuid: i as u128,
            name: (*n).into(),
            ping: 100 * i as i32,
            gamemode: if i == 2 { 3 } else { 0 },
            ..TabEntry::default()
        });
    }
    hud.tablist.header = "\u{a7}6Header".into();
    hud.tablist.footer = "Footer".into();
    let sb = &mut hud.scoreboard;
    sb.set_objective("kills", "\u{a7}eKills", 0, false);
    sb.set_display(1, "kills");
    let red = TeamInfo {
        prefix: "\u{a7}c".into(),
        ..TeamInfo::default()
    };
    sb.set_team("red", 0, &red, &["Steve".into()]);
    for (i, n) in names.iter().enumerate() {
        sb.set_score(n, "kills", i as i32 * 3, false);
    }
    hud.title.set_title(0, Some("\u{a7}bTitle".into()), 0, 0, 0);
    hud.title.set_title(1, Some("Subtitle".into()), 0, 0, 0);
    hud.title.tick();
    hud.set_boss("Wither".into(), 0.6);
    let held: Slot = Some(ItemStack {
        id: 276,
        count: 1,
        damage: 0,
        nbt: None,
    });
    hud.update_held(&held);
    let read = |p: &str| -> Option<Vec<u8>> {
        let jar = std::fs::File::open(jar.as_ref()?).ok()?;
        let mut z = zip::ZipArchive::new(jar).ok()?;
        let mut out = Vec::new();
        std::io::Read::read_to_end(
            &mut z.by_name(&format!("assets/minecraft/{p}")).ok()?,
            &mut out,
        )
        .ok()?;
        Some(out)
    };
    let icons = Icons::new(read);
    let stone: Slot = Some(ItemStack {
        id: 1,
        count: 64,
        damage: 0,
        nbt: None,
    });
    let apple: Slot = Some(ItemStack {
        id: 260,
        count: 5,
        damage: 0,
        nbt: None,
    });
    let empty: Slot = None;
    let slots = [
        &stone, &apple, &held, &empty, &empty, &empty, &empty, &empty, &stone,
    ];
    let status = Status {
        health: 13.0,
        food: 15,
        armor: 7,
        air: 200,
        in_water: true,
        xp_bar: 0.4,
        xp_level: 12,
        absorption: 4.0,
        ..Default::default()
    };
    let left = debug::left_lines(&debug::DebugInfo {
        pos: [8.0, 68.0, -4.0],
        ..Default::default()
    });
    let mut g = Gui::new(GuiScale::new(854, 480, 0, false), r.font(), 0);
    hud.draw(
        &mut g,
        &Frame {
            chat_input: Some("/say hi"),
            debug: Some((&left, &[])),
            fancy: false,
            brightness: 1.0,
            player_view: true,
            status: Some(&status),
            hotbar: Some(HotbarItems {
                slots,
                icons: &icons,
                bob: [0, 3, 0, 0, 0, 0, 0, 0, 0],
            }),
            player_list: true,
            held_item_tooltips: true,
            ..Default::default()
        },
    );
    let quads = g.into_quads();
    let (w, h, rgba) = r.render_to_rgba(&Camera::default(), &quads);
    // `GuiIngame.renderHotbar`: the 182x22 bar sits at (scaledWidth / 2 - 91, scaledHeight - 22);
    // 854x480 scales by 2 to 427x240, so the bar is at GUI (122, 218) and 364x44 pixels.
    let bar = quads.iter().any(|q| {
        let xs = q.pos.map(|p| p[0]);
        let ys = q.pos.map(|p| p[1]);
        q.tex == "textures/gui/widgets.png"
            && xs.iter().copied().fold(f32::MAX, f32::min) == 244.0
            && xs.iter().copied().fold(f32::MIN, f32::max) == 608.0
            && ys.iter().copied().fold(f32::MAX, f32::min) == 436.0
            && ys.iter().copied().fold(f32::MIN, f32::max) == 480.0
    });
    assert!(bar, "hotbar background quad at the vanilla position");
    let at = |x: u32, y: u32| -> [u8; 4] {
        rgba[((y * w + x) * 4) as usize..][..4]
            .try_into()
            .expect("four bytes")
    };
    let sky = at(w - 5, 5);
    assert_eq!(sky, [121, 167, 255, 255], "top-right is plain sky");
    // `GuiOverlayDebug.renderDebugInfoLeft` fills from GUI (1, 1) with 0x90505050; the scale is 2.
    assert_eq!(
        at(1, 1),
        sky,
        "nothing is drawn left of or above GUI (1, 1)"
    );
    let alpha = f32::from(0x90_u8) / 255.0;
    for (i, s) in sky.iter().take(3).enumerate() {
        let want = f32::from(*s) * (1.0 - alpha) + f32::from(0x50_u8) * alpha;
        assert!(
            (f32::from(at(3, 3)[i]) - want).abs() <= 2.0,
            "debug box channel {i}"
        );
    }
    let path = std::env::var("HUD_PNG").unwrap_or_else(|_| {
        std::env::temp_dir()
            .join("ferrocraft-hud.png")
            .to_string_lossy()
            .into()
    });
    encode_png(path.as_ref(), w, h, &rgba).expect("write png");
}
