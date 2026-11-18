use snowhush_core::config::{Config, Effect, Patch};
use snowhush_core::draw::Batch;
use snowhush_core::geom::Rect;
use snowhush_core::sim::*;

fn params(effect: Effect) -> Params {
    Params { effect, intensity: 1.0, melt: false, interact: true, wind: 0.0, ground: true, windows: true, layers: true, pixel: false, pixel_size: 0.0 }
}

fn scene(cursor: Option<[f32; 2]>) -> Scene {
    Scene {
        w: 1920.0,
        h: 1080.0,
        surfs: vec![
            Surf { id: 0, rect: Rect::new(0.0, 1050.0, 1920.0, 2000.0), z: -1, kind: SurfKind::Ground, inset: 0.0, shown: true },
            Surf { id: 1, rect: Rect::new(400.0, 300.0, 600.0, 400.0), z: 0, kind: SurfKind::Window, inset: 8.0, shown: true },
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
    // Tiny flowers: even stretched, well under 40px above the surface at y=300.
    assert!(min_y > 300.0 - 40.0, "a flower reached y={min_y}");
}

mod shape {
    use snowhush_core::shape::*;

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
