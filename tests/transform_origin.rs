//! `transform_origin` — where a transform's scale and rotation pivot.
//!
//! The case it exists for is a bar that drains: a full-width fill scaled
//! on `x` about its left edge is a paint change, where the same bar
//! written as `width: track * fill` is a layout change on every step.
//! A transform that could only pivot around the center shrank such a fill
//! from both ends.

use std::collections::HashMap;

use masque::runtime::config::RuntimeConfig;
use masque::runtime::value::Value;
use masque::widget::vocabulary::scan_source;
use masque::widget::WidgetRef;
use masque::Masque;

const W: f32 = 800.0;
const H: f32 = 600.0;
const DT: f32 = 1.0 / 60.0;

fn find_keyed(node: &WidgetRef, key: &str) -> Option<WidgetRef> {
    let children = {
        let g = node.lock().unwrap();
        if g.key() == Some(key) {
            return Some(node.clone());
        }
        g.get_children()
    };
    children.iter().find_map(|c| find_keyed(c, key))
}

fn mounted(src: &str, state: &[(&str, Value)]) -> Masque {
    let config = RuntimeConfig::new().with_host_state(
        state
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect::<HashMap<_, _>>(),
    );
    let mut o = Masque::from_source(src, config).expect("the document mounts");
    for _ in 0..3 {
        o.frame(W, H, DT).expect("frame");
    }
    o
}

/// The pivot `render_effects` hands the surface, and the fill's rect.
fn pivot(o: &Masque, key: &str) -> ((f32, f32), (f32, f32, f32, f32)) {
    let node = find_keyed(&o.get_ui().root, key).expect("the keyed widget");
    let g = node.lock().unwrap();
    let fx = g.render_effects().expect("a transformed widget has effects");
    let r = g.get_layout_rect().expect("laid out");
    ((fx.pivot_x, fx.pivot_y), (r.x, r.y, r.width, r.height))
}

fn bar(origin: &str) -> String {
    format!(
        r#"
let main = fn () {{
  Flex {{
    style: {{ width: "grow", height: "grow", padding: {{ left: 40, top: 30 }} }},
    children: [
      Flex {{
        style: {{ width: 200, height: 10, overflow: "hidden" }},
        children: [
          Flex {{ key: "fill", style: {{
            width: "grow", height: "grow",
            transform: {{ scale_x: fill }},
            {origin}
          }} }},
        ],
      }},
    ],
  }}
}};
"#
    )
}

#[test]
fn a_left_origin_pivots_on_the_left_edge() {
    let o = mounted(&bar(r#"transform_origin: "left","#), &[("fill", Value::Float(0.5))]);
    let ((px, py), (x, y, w, h)) = pivot(&o, "fill");
    assert_eq!(w, 200.0, "the fill is the track's whole width; the scale is paint");
    assert_eq!((px, py), (x, y + h / 2.0));
}

#[test]
fn a_corner_and_a_map_say_the_same_thing() {
    let keyword = mounted(&bar(r#"transform_origin: "bottom_right","#), &[("fill", Value::Float(0.5))]);
    let map = mounted(&bar(r#"transform_origin: { x: 1, y: 1 },"#), &[("fill", Value::Float(0.5))]);
    let ((kx, ky), (x, y, w, h)) = pivot(&keyword, "fill");
    assert_eq!((kx, ky), (x + w, y + h));
    assert_eq!(pivot(&map, "fill").0, (kx, ky));
}

/// The default is what every transform pivoted on before the key existed;
/// a scale written without an origin must not move.
#[test]
fn no_origin_is_the_center() {
    let o = mounted(&bar(""), &[("fill", Value::Float(0.5))]);
    let ((px, py), (x, y, w, h)) = pivot(&o, "fill");
    assert_eq!((px, py), (x + w / 2.0, y + h / 2.0));
}

/// The point of the key: a bar draining through `scale_x` re-runs the
/// document and repaints, and owes no layout pass of any kind.
#[test]
fn a_scaled_fill_draining_is_paint() {
    let mut o = mounted(&bar(r#"transform_origin: "left","#), &[("fill", Value::Float(0.5))]);
    o.with_runtime_mut(|rt| rt.set_host_state("fill", 0.49));
    assert!(o.update().expect("reconciles"), "the fill changed, so the document re-ran");
    let ui = o.get_ui();
    assert!(!ui.needs_layout(), "a scale is paint");
    assert!(!ui.needs_boundary_layout(), "not even a boundary's");
    assert!(ui.needs_repaint());
}

#[test]
fn the_vocabulary_knows_the_key_and_its_words() {
    let clean = scan_source("t.masque", &bar(r#"transform_origin: "top_left","#));
    assert!(clean.is_empty(), "{clean:?}");
    let map = scan_source("t.masque", &bar(r#"transform_origin: { x: 0, y: 0.5 },"#));
    assert!(map.is_empty(), "{map:?}");

    let wrong_word = scan_source("t.masque", &bar(r#"transform_origin: "start","#));
    assert_eq!(wrong_word.len(), 1, "{wrong_word:?}");
    let wrong_key = scan_source("t.masque", &bar(r#"transform_origin: { left: 0 },"#));
    assert_eq!(wrong_key.len(), 1, "{wrong_key:?}");
}
