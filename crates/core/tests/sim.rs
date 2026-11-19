use windfall_core::config::{Config, Effect, Patch};
use windfall_core::draw::Batch;
use windfall_core::geom::Rect;
use windfall_core::sim::*;

fn params(effect: Effect) -> Params {
    Params { effect, intensity: 1.0, melt: false, interact: true, wind: 0.0, ground: true, windows: true, layers: true, pixel: false, pixel_size: 0.0, glow: [1.0, 0.85, 0.3], element: windfall_core::config::Element::Natural, lights: false }
}

fn scene(cursor: Option<[f32; 2]>) -> Scene {
    Scene {
        w: 1920.0,
        h: 1080.0,
        surfs: vec![
            Surf { id: 0, rect: Rect::new(0.0, 1050.0, 1920.0, 2000.0), z: -1, kind: SurfKind::Ground, inset: 0.0, group: 0, shown: true },
            Surf { id: 1, rect: Rect::new(400.0, 300.0, 600.0, 400.0), z: 0, kind: SurfKind::Window, inset: 8.0, group: 0, shown: true },
        ],
        cursor,
    }
}

fn run(w: &mut World, p: &Params, secs: f32) {
    for _ in 0..(secs * 60.0) as usize {
        w.step(1.0 / 60.0, p);
    }
}

#[test]
fn snow_settles_on_window_and_ground() {
    let mut w = World::new(1);
    w.set_scene(scene(None));
    let p = params(Effect::Snow);
    run(&mut w, &p, 120.0);
    assert!(w.pile_height(1, 700.0) > 1.0, "window pile {}", w.pile_height(1, 700.0));
    assert!(w.pile_height(0, 100.0) > 1.0);
    assert_eq!(w.pile_height(1, 100.0), 0.0); // outside the window
}

#[test]
fn unchanged_scene_keeps_its_snow() {
    // Regression: a snapshot that merely repeats the geometry (focus change, cursor movement) used to shake
    // every window's snow off.
    let mut w = World::new(2);
    w.set_scene(scene(None));
    let p = params(Effect::Snow);
    run(&mut w, &p, 90.0);
    let before = w.pile_height(1, 700.0);
    assert!(before > 1.0);
    for _ in 0..50 {
        w.set_scene(scene(Some([10.0, 10.0])));
        w.step(1.0 / 60.0, &p);
    }
    assert!(w.pile_height(1, 700.0) >= before * 0.9);
}

#[test]
fn moving_window_sheds_gradually() {
    let mut w = World::new(2);
    w.set_scene(scene(None));
    let p = Params { intensity: 0.0, ..params(Effect::Snow) };
    let q = params(Effect::Snow);
    run(&mut w, &q, 120.0);
    let mut s = scene(None);
    s.surfs[1].rect.x += 200.0;
    w.set_scene(s);
    let at_once = w.particle_count();
    run(&mut w, &p, 0.3);
    let soon = w.particle_count();
    run(&mut w, &p, 4.0);
    assert!(soon > at_once, "nothing shed yet");
    assert_eq!(w.pile_height(1, 450.0), 0.0); // the old spot is bare
}

#[test]
fn closed_window_snow_drops() {
    let mut w = World::new(3);
    w.set_scene(scene(None));
    run(&mut w, &params(Effect::Snow), 120.0);
    let mut s = scene(None);
    s.surfs.remove(1);
    w.set_scene(s);
    run(&mut w, &Params { intensity: 0.0, ..params(Effect::Snow) }, 1.0);
    assert!(w.particle_count() > 20);
}

#[test]
fn off_screen_window_keeps_snow() {
    let mut w = World::new(4);
    w.set_scene(scene(None));
    let p = params(Effect::Snow);
    run(&mut w, &p, 90.0);
    let before = w.pile_height(1, 700.0);
    let mut hidden = scene(None);
    hidden.surfs[1].shown = false;
    w.set_scene(hidden);
    run(&mut w, &p, 5.0);
    w.set_scene(scene(None));
    assert!(w.pile_height(1, 700.0) >= before * 0.95);
}

#[test]
fn rain_never_piles() {
    let mut w = World::new(3);
    w.set_scene(scene(None));
    run(&mut w, &params(Effect::Rain), 20.0);
    assert_eq!(w.pile_height(1, 700.0), 0.0);
}

#[test]
fn flowers_grow_and_draw() {
    let mut w = World::new(4);
    w.set_scene(scene(None));
    let p = params(Effect::Flowers);
    run(&mut w, &p, 30.0);
    let mut b = Batch::default();
    w.draw(&mut b, &p);
    assert!(b.vertex_count() > 1500, "{}", b.vertex_count());
    // pointer parked over the window top makes verts differ (swaying) without panicking
    w.set_scene(scene(Some([700.0, 290.0])));
    run(&mut w, &p, 5.0);
}

#[test]
fn config_patch_clamps() {
    let mut c = Config::default();
    assert!(c.apply(&Patch { intensity: Some(7.0), effect: Some(Effect::Rain), ..Default::default() }));
    assert_eq!(c.intensity, 1.0);
    assert!(!c.apply(&Patch::default()));
}

#[test]
fn touched_flowers_stop_growing_at_a_limit() {
    let mut w = World::new(5);
    w.set_scene(scene(None));
    let p = params(Effect::Flowers);
    run(&mut w, &p, 20.0);
    // Hold the pointer over the window top for a long time.
    w.set_scene(scene(Some([700.0, 285.0])));
    run(&mut w, &p, 60.0);
    let mut b = Batch::default();
    w.draw(&mut b, &p);
    let min_y = b.data.chunks(8).map(|v| v[1]).fold(f32::MAX, f32::min);
    // Even stretched, a flower (and the glow around it) stays within ~90px of the surface at y=300.
    assert!(min_y > 300.0 - 90.0, "a flower reached y={min_y}");
}

mod shape {
    use windfall_core::shape::*;

    /// A transparent bar: a gradient "wallpaper" with framed modules (a light 1px frame, darker inside).
    fn bar(w: usize, h: usize, modules: &[(usize, usize, usize)]) -> Vec<u8> {
        let mut d = vec![0u8; w * h * 4];
        for y in 0..h {
            for x in 0..w {
                let wall = (30 + (x / 40) % 6 * 2) as u8;
                let mut c = [wall, wall + 5, wall + 10];
                for &(x0, x1, top) in modules {
                    if x >= x0 && x < x1 && y >= top {
                        c = if y == top || x == x0 || x == x1 - 1 { [200, 180, 150] } else { [12, 12, 14] };
                    }
                }
                let i = (y * w + x) * 4;
                d[i..i + 4].copy_from_slice(&[c[0], c[1], c[2], 255]);
            }
        }
        d
    }

    #[test]
    fn framed_modules_on_transparent_bar() {
        let (w, h) = (600, 30);
        let d = bar(w, h, &[(20, 140, 4), (160, 300, 4), (420, 580, 4)]);
        let segs = segments(&Strip { data: &d, w, h, stride: w * 4 });
        assert_eq!(segs.len(), 3, "{segs:?}");
        assert!(segs[0].x0 >= 20.0 && segs[0].x1 <= 140.0 && (segs[0].top - 4.0).abs() <= 1.0, "{:?}", segs[0]);
        // Nothing grows in the gap between modules.
        assert!(segs.iter().all(|s| s.x1 <= 300.0 || s.x0 >= 420.0));
    }

    #[test]
    fn solid_bar_is_painted_end_to_end() {
        let (w, h) = (400, 30);
        let mut d = vec![0u8; w * h * 4];
        for p in d.chunks_mut(4) {
            p.copy_from_slice(&[40, 30, 30, 255]);
        }
        let segs = segments(&Strip { data: &d, w, h, stride: w * 4 });
        assert_eq!(segs, vec![Segment { x0: 0.0, x1: 400.0, top: 0.0 }]);
    }

    #[test]
    fn plain_strip_counts_as_solid_and_busy_wallpaper_as_nothing() {
        let (w, h) = (300, 30);
        let d = bar(w, h, &[]);
        assert_eq!(segments(&Strip { data: &d, w, h, stride: w * 4 }).len(), 1);
        // Noisy wallpaper without any modules: no long level edge, and not flat either.
        let mut n = vec![0u8; w * h * 4];
        let mut seed = 7u32;
        for p in n.chunks_mut(4) {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let v = (seed >> 24) as u8;
            p.copy_from_slice(&[v, v, v, 255]);
        }
        assert!(segments(&Strip { data: &n, w, h, stride: w * 4 }).is_empty());
    }
}

#[test]
fn bar_that_shrinks_drops_what_lay_on_the_lost_part() {
    let bar = |x0: f32, w: f32| Scene {
        w: 1920.0,
        h: 1080.0,
        surfs: vec![Surf { id: 9, rect: Rect::new(x0, 1000.0, w, 30.0), z: 1000, kind: SurfKind::Layer, inset: 0.0, group: 0, shown: true }],
        cursor: None,
    };
    let mut w = World::new(11);
    w.set_scene(bar(300.0, 800.0));
    let p = params(Effect::Snow);
    run(&mut w, &p, 90.0);
    assert!(w.pile_height(9, 1000.0) > 1.0);
    let before = w.pile_height(9, 400.0);
    // The painted part loses its right half.
    w.set_scene(bar(300.0, 400.0));
    assert_eq!(w.pile_height(9, 1000.0), 0.0, "the lost part must no longer hold snow");
    assert!(w.pile_height(9, 400.0) >= before * 0.9, "the part that is still painted keeps its snow");
    let still = Params { intensity: 0.0, ..params(Effect::Snow) };
    run(&mut w, &still, 0.3);
    assert!(w.particle_count() > 5, "it falls");
}

#[test]
fn flowers_on_a_shrinking_bar_part_squeeze_together_and_only_a_vanished_part_puffs() {
    let bar = |w: f32| Scene {
        w: 1920.0,
        h: 1080.0,
        surfs: vec![Surf { id: 9, rect: Rect::new(300.0, 1000.0, w, 30.0), z: 1000, kind: SurfKind::Layer, inset: 0.0, group: 0, shown: true }],
        cursor: None,
    };
    let blue = |b: &Batch| b.data.chunks(8).filter(|v| (v[4] - 0.35).abs() < 1e-3 && (v[5] - 0.62).abs() < 1e-3 && (v[6] - 1.0).abs() < 1e-3).count();
    let mut w = World::new(12);
    w.set_scene(bar(800.0));
    let p = params(Effect::Flowers);
    run(&mut w, &p, 30.0);
    // Shrinking: nothing puffs.
    w.set_scene(bar(300.0));
    let mut b = Batch::default();
    for _ in 0..(3 * 60) {
        w.step(1.0 / 60.0, &p);
        b.clear();
        w.draw(&mut b, &p);
        assert_eq!(blue(&b), 0, "a shrinking part must not puff its flowers away");
    }
    // Vanishing altogether: they puff, starting at once.
    w.set_scene(Scene { w: 1920.0, h: 1080.0, surfs: vec![], cursor: None });
    let mut seen = 0;
    for _ in 0..(2 * 60) {
        w.step(1.0 / 60.0, &p);
        b.clear();
        w.draw(&mut b, &p);
        seen = seen.max(blue(&b));
    }
    assert!(seen > 0, "no puffs");
}

#[test]
fn a_hidden_window_does_not_hide_flowers_below_it() {
    // A floating window on another workspace (a scratchpad terminal, say) sits over the window top but is
    // not on screen, so it must not cover anything.
    let mut sc = scene(None);
    sc.surfs.push(Surf { id: 5, rect: Rect::new(300.0, 250.0, 900.0, 700.0), z: 480, kind: SurfKind::Window, inset: 8.0, group: 0, shown: false });
    let mut w = World::new(21);
    w.set_scene(sc);
    let p = params(Effect::Flowers);
    run(&mut w, &p, 25.0);
    let mut b = Batch::default();
    w.draw(&mut b, &p);
    // Green stems and grass over the window's top edge (x 400..1000, y just above 300) must be drawn.
    let over_window = b.data.chunks(8).filter(|v| v[0] > 420.0 && v[0] < 980.0 && v[1] > 285.0 && v[1] < 300.0).count();
    assert!(over_window > 200, "the window's lawn is hidden: {over_window}");
}

#[test]
fn snow_on_a_reshaped_bar_stays_where_another_part_now_covers_it() {
    let part = |id: u64, x: f32, w: f32| Surf { id, rect: Rect::new(x, 1000.0, w, 30.0), z: 1000, kind: SurfKind::Layer, inset: 0.0, group: 7, shown: true };
    let scene = |a: (f32, f32), b: (f32, f32)| Scene { w: 1920.0, h: 1080.0, surfs: vec![part(1, a.0, a.1), part(2, b.0, b.1)], cursor: None };
    let mut w = World::new(31);
    w.set_scene(scene((100.0, 300.0), (420.0, 300.0)));
    run(&mut w, &params(Effect::Snow), 100.0);
    let at_350 = w.pile_height(1, 350.0);
    assert!(at_350 > 1.0);
    // Part one shrinks to [100,300]; part two grows to start at 300, so it now covers x=350.
    w.set_scene(scene((100.0, 200.0), (300.0, 420.0)));
    assert!(w.pile_height(2, 350.0) >= at_350 * 0.8, "the snow at 350 was lost though ground is still there");
}

#[test]
fn a_closed_window_takes_its_lawn_at_once_with_only_a_few_quiet_puffs() {
    let mut w = World::new(41);
    w.set_scene(scene(None));
    let p = params(Effect::Flowers);
    run(&mut w, &p, 30.0);
    let mut s = scene(None);
    s.surfs.remove(1); // the window closes
    w.set_scene(s);
    let mut b = Batch::default();
    w.step(1.0 / 60.0, &p);
    w.draw(&mut b, &p);
    // Nothing of the window's lawn is left on the very next frame: no stems or grass over its top edge.
    let green = b.data.chunks(8).filter(|v| v[0] > 420.0 && v[0] < 980.0 && v[1] > 280.0 && v[1] < 302.0 && v[5] > v[4] && v[5] > v[6] && v[7] > 0.9).count();
    assert_eq!(green, 0, "the lawn is still there");
    let blue = b.data.chunks(8).filter(|v| (v[4] - 0.35).abs() < 1e-3 && (v[5] - 0.62).abs() < 1e-3 && (v[6] - 1.0).abs() < 1e-3).count() / 6;
    assert!(blue > 0 && blue < 130, "puffs should be there but few: {blue}");
}

fn count_color(b: &Batch, c: [f32; 3]) -> usize {
    b.data.chunks(8).filter(|v| (v[4] - c[0]).abs() < 0.02 && (v[5] - c[1]).abs() < 0.02 && (v[6] - c[2]).abs() < 0.02).count()
}

#[test]
fn comets_burst_into_blue_sparks_on_contact_and_do_not_pile() {
    let mut w = World::new(51);
    w.set_scene(scene(None));
    let p = Params { intensity: 1.0, ..params(Effect::Comets) };
    let mut b = Batch::default();
    let mut blue = 0;
    for _ in 0..(20 * 60) {
        w.step(1.0 / 60.0, &p);
        b.clear();
        w.draw(&mut b, &p);
        blue = blue.max(count_color(&b, [0.4, 0.66, 1.0]));
    }
    assert!(blue > 6, "no blue burst when a comet hit something: {blue}");
    assert_eq!(w.pile_height(1, 700.0), 0.0);
}

#[test]
fn hearts_fall_and_burst_pink() {
    let mut w = World::new(52);
    w.set_scene(scene(None));
    let p = Params { intensity: 1.0, ..params(Effect::Hearts) };
    let mut b = Batch::default();
    let mut pink = 0;
    for _ in 0..(25 * 60) {
        w.step(1.0 / 60.0, &p);
        b.clear();
        w.draw(&mut b, &p);
        pink = pink.max(count_color(&b, [1.0, 0.45, 0.65]));
    }
    assert!(pink > 6, "{pink}");
}

#[test]
fn warm_lights_drift_and_stay_in_the_air() {
    let mut w = World::new(53);
    w.set_scene(scene(None));
    let p = Params { intensity: 1.0, ..params(Effect::Lights) };
    for _ in 0..(30 * 60) {
        w.step(1.0 / 60.0, &p);
    }
    let mut b = Batch::default();
    w.draw(&mut b, &p);
    assert!(b.vertex_count() > 60, "no lights drawn: {}", b.vertex_count());
    assert_eq!(w.pile_height(1, 700.0), 0.0);
}

#[test]
fn every_element_grows_its_own_flowers_and_things_in_the_air() {
    use windfall_core::config::Element;
    for el in [Element::Lightning, Element::Fire, Element::Ice, Element::Matter, Element::Void] {
        let mut w = World::new(60);
        w.set_scene(scene(None));
        let p = Params { element: el, ..params(Effect::Flowers) };
        for _ in 0..(25 * 60) {
            w.step(1.0 / 60.0, &p);
        }
        let mut b = Batch::default();
        w.draw(&mut b, &p);
        assert!(b.vertex_count() > 1500, "{el:?} drew too little");
    }
}

#[test]
fn floating_lights_overlay_any_effect_in_its_colours_and_are_yellow_alone() {
    let blueish = |b: &Batch| b.data.chunks(8).filter(|v| v[7] > 0.0 && v[7] < 0.5 && v[6] > v[4] + 0.1).count();
    let yellowish = |b: &Batch| b.data.chunks(8).filter(|v| v[7] > 0.0 && v[7] < 0.5 && v[4] > v[6] + 0.3 && v[5] > 0.6).count();
    let run_with = |effect: Effect, lights: bool| {
        let mut w = World::new(70);
        w.set_scene(scene(None));
        let p = Params { lights, ..params(effect) };
        for _ in 0..(40 * 60) {
            w.step(1.0 / 60.0, &p);
        }
        let mut b = Batch::default();
        w.draw(&mut b, &p);
        b
    };
    let snow_off = run_with(Effect::Snow, false);
    let snow_on = run_with(Effect::Snow, true);
    assert!(snow_on.vertex_count() > snow_off.vertex_count() + 60, "no lights over the snow");
    assert!(blueish(&snow_on) > blueish(&snow_off), "lights over snow should be icy");
    let alone = run_with(Effect::Lights, false);
    assert!(yellowish(&alone) > 20, "lights on their own are yellow");
    let leaves = run_with(Effect::Leaves, true);
    assert!(yellowish(&leaves) > 20, "lights over leaves should be amber");
}
