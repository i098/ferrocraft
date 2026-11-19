use super::super::{Kind, Particles};
use super::*;

#[test]
fn leaves_shards_are_green_and_grass_blocks_stay_grey() {
    let maps = Colormaps::plains();
    let leaves = 18u16 << 4;
    let site = Site::new([0, 64, 0], |_, _| 1);
    let m = multiplier(&maps, leaves, Some(&site));
    assert!(m[1] > m[0] && m[1] > m[2], "{m:?}");
    assert_eq!(multiplier(&maps, 2 << 4, Some(&site)), [1.0; 3]);
    assert_eq!(multiplier(&maps, 1 << 4, Some(&site)), [1.0; 3]);
    // A packet shard uses the render colour, 0x48b518 for leaves.
    assert_eq!(multiplier(&maps, leaves, None), unpack(0x48_b518));
}

#[test]
fn a_broken_leaves_block_throws_shards_that_remember_the_site() {
    let mut ps = Particles::new();
    ps.break_block([3, 70, 4], 18 << 4, |_, _| 1);
    assert_eq!(ps.len(), 64);
    assert!(ps
        .list
        .iter()
        .all(|p| matches!(p.sprite, Sprite::Block { site: Some(_), .. })));
    ps.spawn(Kind::BlockCrack, [0.0; 3], [0.0; 3], 18 << 4);
    assert!(matches!(
        ps.list.last().unwrap().sprite,
        Sprite::Block { site: None, .. }
    ));
}
