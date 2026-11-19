//! `EntityRenderer.drawDebugAxes`: F3 draws the red x, green y and blue z axes 0.1 in front of the camera.
//! The frame with the axes differs from the frame without them only in a few pixels around the screen centre.
//! `AXES_PROOF_DIR` keeps both frames as PNG; `FERROCRAFT_CLIENT_JAR` gives real assets.

use render::{encode_png, Camera, Renderer};

const SIZE: (u32, u32) = (854, 480);

/// The pixels that the axes changed: `(x, y, rgb)`.
fn changed(off: &[u8], on: &[u8]) -> Vec<(u32, u32, [u8; 3])> {
    off.as_chunks::<4>()
        .0
        .iter()
        .zip(on.as_chunks::<4>().0)
        .enumerate()
        .filter(|(_, (a, b))| a != b)
        .map(|(i, (_, b))| (i as u32 % SIZE.0, i as u32 / SIZE.0, [b[0], b[1], b[2]]))
        .collect()
}

#[test]
fn f3_draws_the_three_axes_at_the_screen_centre() {
    let jar = std::env::var_os("FERROCRAFT_CLIENT_JAR").map(std::path::PathBuf::from);
    let mut r = Renderer::new_offscreen(SIZE.0, SIZE.1, jar.as_deref());
    // Pitched down, so the z axis (the view direction when level) shows as a line too.
    let cam = Camera {
        yaw: 0.0,
        pitch: 45.0,
        ..Camera::default()
    };
    let off = r.render_to_rgba(&cam, &[]).2;
    r.set_debug_axes(true);
    let on = r.render_to_rgba(&cam, &[]).2;
    let hit = changed(&off, &on);
    let (cx, cy) = (SIZE.0 / 2, SIZE.1 / 2);
    assert!(
        hit.iter()
            .all(|&(x, y, _)| x.abs_diff(cx) < 40 && y.abs_diff(cy) < 40),
        "the axes stay near the centre"
    );
    let has = |want: [u8; 3]| hit.iter().any(|&(_, _, c)| c == want);
    assert!(has([255, 0, 0]), "red x axis");
    assert!(has([0, 255, 0]), "green y axis");
    assert!(has([0, 0, 255]), "blue z axis");
    // Red leaves the centre to the left (world +x is screen left at yaw 0), green goes up.
    assert!(hit.iter().any(|&(x, _, c)| c == [255, 0, 0] && x < cx));
    assert!(hit.iter().any(|&(_, y, c)| c == [0, 255, 0] && y < cy));

    r.set_debug_axes(false);
    assert_eq!(r.render_to_rgba(&cam, &[]).2, off, "no axes with F3 closed");

    if let Some(dir) = std::env::var_os("AXES_PROOF_DIR") {
        let dir = std::path::Path::new(&dir);
        std::fs::create_dir_all(dir).unwrap();
        encode_png(&dir.join("axes_off.png"), SIZE.0, SIZE.1, &off).unwrap();
        encode_png(&dir.join("axes_on.png"), SIZE.0, SIZE.1, &on).unwrap();
    }
}
