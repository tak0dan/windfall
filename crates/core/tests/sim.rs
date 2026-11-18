use snowhush_core::config::{Config, Effect, Patch};
use snowhush_core::draw::Batch;
use snowhush_core::geom::Rect;
use snowhush_core::sim::*;

fn params(effect: Effect) -> Params {
    Params { effect, intensity: 1.0, melt: false, interact: true, wind: 0.0, ground: true, windows: true, layers: true, pixel: false }
}

fn scene(cursor: Option<[f32; 2]>) -> Scene {
    Scene {
        w: 1920.0,
        h: 1080.0,
        surfs: vec![
            Surf { id: 0, rect: Rect::new(0.0, 1050.0, 1920.0, 2000.0), z: -1, kind: SurfKind::Ground, inset: 0.0 },
            Surf { id: 1, rect: Rect::new(400.0, 300.0, 600.0, 400.0), z: 0, kind: SurfKind::Window, inset: 8.0 },
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
fn moving_window_sheds_its_snow() {
    let mut w = World::new(2);
    w.set_scene(scene(None));
    let p = params(Effect::Snow);
    run(&mut w, &p, 120.0);
    let mut s = scene(None);
    s.surfs[1].rect.x += 200.0;
    w.set_scene(s);
    assert_eq!(w.pile_height(1, 900.0), 0.0);
    assert!(w.particle_count() > 20);
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
    assert!(b.vertex_count() > 300, "{}", b.vertex_count());
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
