use super::*;

fn moved(id: i16, from: [f32; 3]) -> [f32; 3] {
    crate::hand::point(&pose(id), from).map(|v| (v * 1e4).round() / 1e4)
}

/// Oracle: 1.8.9 `Item.isWielded` registry. Swords and tools lift to 0.0625 and scale 0.625; the rod and
/// carrot on a stick turn half a turn first; a bow, a potion and an unlisted item are not wielded.
#[test]
fn wielded_items_take_the_full_3d_pose() {
    assert_eq!(moved(276, [1.0; 3]), [0.6875, -0.5625, 0.625]);
    assert_eq!(moved(276, [0.0; 3]), [0.0625, 0.0625, 0.0]);
    assert_eq!(moved(346, [0.0; 3]), [-0.0625, 0.0, 0.0]);
    assert_eq!(moved(398, [0.0; 3]), [-0.0625, 0.0, 0.0]);
    assert!([280, 352, 369].into_iter().all(is_wielded));
    assert!(![259, 261, 287, 373].into_iter().any(is_wielded));
}
