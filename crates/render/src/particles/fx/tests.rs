use super::*;

fn tick_alone(ps: &mut Particles, kind: Kind, wet: bool) -> Vec<Particle> {
    ps.spawn(kind, [0.5, 70.5, 0.5], [0.0; 3], 0);
    ps.tick(|_, _, _| if wet { 9 << 4 } else { 0 });
    ps.list.clone()
}

#[test]
fn flame_lives_14_to_44_ticks_and_starts_up_to_twice_the_size() {
    let mut ps = Particles::new();
    for _ in 0..2000 {
        ps.spawn(Kind::Flame, [0.0; 3], [0.0; 3], 0);
    }
    let (min, max) = ps.list.iter().fold((u32::MAX, 0), |(lo, hi), p| {
        (lo.min(p.max_age), hi.max(p.max_age))
    });
    assert!(min >= 12 && max <= 44, "{min}..{max}");
    assert!(ps.list.iter().all(|p| (1.0..=2.0).contains(&p.init_scale)));
    // `scale = start * (1 - (age / max_age)^2 * 0.5)`: full at birth, half at the end.
    let p = ps.list[0];
    assert_eq!(super::super::render_scale(&p, 0.0), p.init_scale);
    let end = Particle {
        age: p.max_age,
        ..p
    };
    assert!((super::super::render_scale(&end, 0.0) - p.init_scale * 0.5).abs() < 1e-6);
}

#[test]
fn flame_starts_within_a_twentieth_of_a_block_and_ignores_blocks() {
    let mut ps = Particles::new();
    for _ in 0..200 {
        ps.spawn(Kind::Flame, [5.0, 5.0, 5.0], [0.0; 3], 0);
    }
    assert!(ps
        .list
        .iter()
        .all(|p| (0..3).all(|i| (p.pos[i] - 5.0).abs() <= 0.05)));
    let mut p = ps.list[0];
    p.vel = [0.0, -0.5, 0.0];
    let at = p.pos;
    super::move_fx(&mut p, &|_, _, _| true);
    assert!((p.pos[1] - (at[1] - 0.5)).abs() < 1e-12);
}

#[test]
fn lava_pop_shrinks_to_nothing_and_leaves_smoke() {
    let mut ps = Particles::new();
    ps.spawn(Kind::Lava, [0.5, 70.5, 0.5], [0.0; 3], 0);
    let pop = ps.list[0];
    assert!((pop.vel[1] - 0.05).abs() <= 0.4 && pop.vel[1] >= 0.05);
    let end = Particle {
        age: pop.max_age,
        ..pop
    };
    assert!(super::super::render_scale(&end, 0.0) <= 0.0);
    let mut smoke = 0;
    for _ in 0..=pop.max_age {
        smoke += ps
            .list
            .iter()
            .filter(|p| matches!(p.behavior, Behavior::Smoke))
            .count();
        ps.tick(|_, _, _| 0);
    }
    // The chance of a puff each tick is `1 - age / max_age`, about half the ticks on average.
    assert!(smoke >= 1, "no smoke");
    assert!(ps.list.iter().all(|p| p.behavior != Behavior::Lava));
}

#[test]
fn note_color_follows_the_pitch() {
    let close = |a: [f32; 3], b: [f32; 3]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-3);
    assert!(close(note_color(0.0), [0.35, 0.9129, 0.0]));
    assert!(close(note_color(0.5), [0.35, 0.0, 0.9129]));
}

#[test]
fn note_and_hearts_grow_in_at_vanilla_size_and_the_angry_villager_sits_half_a_block_up() {
    let mut ps = Particles::new();
    ps.spawn(Kind::Note, [0.0, 10.0, 0.0], [0.5, 0.0, 0.0], 0);
    ps.spawn(Kind::Heart, [0.0, 10.0, 0.0], [0.0; 3], 0);
    ps.spawn(Kind::AngryVillager, [0.0, 10.0, 0.0], [0.0; 3], 0);
    let [note, heart, angry] = [0, 1, 2].map(|i| ps.list[i]);
    assert_eq!(note.color, note_color(0.5));
    assert!((1.5..=3.0).contains(&note.init_scale));
    assert_eq!(super::super::render_scale(&note, 0.0), 0.0);
    let grown = Particle { age: 1, ..heart };
    assert_eq!(super::super::render_scale(&grown, 0.0), heart.init_scale);
    assert_eq!(heart.pos[1], 10.0);
    assert_eq!(angry.pos[1], 10.5);
}

#[test]
fn bubble_with_no_packet_motion_drifts_by_at_most_0_02_and_dies_outside_water() {
    let mut ps = Particles::new();
    for _ in 0..100 {
        ps.spawn(Kind::Bubble, [0.5, 70.5, 0.5], [0.0; 3], 0);
    }
    assert!(ps
        .list
        .iter()
        .all(|p| p.vel.iter().all(|v| v.abs() <= 0.02 + 1e-9)));
    assert_eq!(
        tick_alone(&mut Particles::new(), Kind::Bubble, true).len(),
        1
    );
    assert!(tick_alone(&mut Particles::new(), Kind::Bubble, false).is_empty());
    // Packet motion adds a fifth of itself.
    let mut ps = Particles::new();
    ps.spawn(Kind::Bubble, [0.0; 3], [1.0, 0.0, 0.0], 0);
    assert!((ps.list[0].vel[0] - 0.2).abs() <= 0.02 + 1e-9);
}

#[test]
fn id_6_is_a_wake_that_follows_the_packet_motion_without_gravity() {
    assert_eq!(Kind::from_id(6), Some(Kind::Wake));
    assert_eq!(Kind::from_id(5), Some(Kind::Splash));
    let mut ps = Particles::new();
    ps.spawn(Kind::Wake, [0.5, 70.5, 0.5], [0.1, 0.0, -0.1], 0);
    let first = ps.list[0];
    assert_eq!(first.vel, [0.1, 0.0, -0.1]);
    assert_eq!(first.gravity, 0.0);
    ps.tick(|_, _, _| 0);
    let p = ps.list[0];
    assert!((p.pos[0] - 0.6).abs() < 1e-9 && (p.pos[1] - 70.5).abs() < 1e-9);
    assert!((p.vel[0] - 0.1 * f64::from(0.98_f32)).abs() < 1e-9);
    // The tile cycles with the time left: `19 + (60 - left) % 4`.
    let Sprite::Sheet { base, .. } = p.sprite else {
        panic!("sheet sprite");
    };
    assert_eq!(
        base,
        19 + (60 - (first.max_age as i32)).rem_euclid(4) as u32
    );
}
