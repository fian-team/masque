//! `max_width` on a Flex style — CSS `max-width`: a ceiling on whatever
//! width the sizing rules resolve, the twin of `max_height`. The fixtures
//! pair a box that stops at its ceiling with one that stays below it, so a
//! ceiling that clamped everything, or nothing, fails one of them; and the
//! centred column is the shape the key exists for.

use masque::runtime::config::RuntimeConfig;
use masque::widget::vocabulary::scan_source;
use masque::widget::WidgetRef;
use masque::Masque;

fn find(node: &WidgetRef, key: &str) -> Option<WidgetRef> {
    let children = {
        let g = node.lock().unwrap();
        if g.key() == Some(key) {
            return Some(node.clone());
        }
        g.get_children()
    };
    children.iter().find_map(|c| find(c, key))
}

fn rect_of(o: &Masque, key: &str) -> (f32, f32) {
    let node = find(&o.get_ui().root, key).unwrap_or_else(|| panic!("no widget keyed {key}"));
    let g = node.lock().unwrap();
    let r = g.get_layout_rect().expect("laid out");
    (r.x, r.width)
}

fn settled(src: &str) -> Masque {
    let mut o = Masque::from_source(src, RuntimeConfig::default()).expect("from_source");
    for _ in 0..4 {
        o.frame(800.0, 600.0, 1.0 / 60.0).expect("frame");
    }
    o
}

/// A grow box in an 800-px centred row: capped at 300 it stops there and
/// stands in the middle; capped at 2000 it is the whole row.
#[test]
fn a_grow_box_stops_at_its_ceiling_and_the_row_centres_it() {
    let src = r#"
let main = fn () {
  Flex { style: { width: "grow", height: "grow", direction: "column" }, children: [
    Flex { style: { width: "grow", height: 50, direction: "row", main_alignment: "center" }, children: [
      Flex { key: "capped", style: { width: "grow", height: 50, max_width: 300 } },
    ] },
    Flex { style: { width: "grow", height: 50, direction: "row", main_alignment: "center" }, children: [
      Flex { key: "roomy", style: { width: "grow", height: 50, max_width: 2000 } },
    ] },
  ] }
};"#;
    let o = settled(src);
    assert_eq!(rect_of(&o, "capped"), (250.0, 300.0), "stopped at its ceiling, centred");
    assert_eq!(rect_of(&o, "roomy"), (0.0, 800.0), "below its ceiling it is its share");
}

/// In a column the ceiling is on the cross axis, and `cross_alignment`
/// places the clamped box.
#[test]
fn a_column_centres_a_clamped_child_on_its_cross_axis() {
    let src = r#"
let main = fn () {
  Flex { style: { width: "grow", height: "grow", direction: "column", cross_alignment: "center" }, children: [
    Flex { key: "sheet", style: { width: "grow", height: 40, max_width: 200 } },
  ] }
};"#;
    let o = settled(src);
    assert_eq!(rect_of(&o, "sheet"), (300.0, 200.0));
}

#[test]
fn the_vocabulary_knows_the_key() {
    let found = scan_source(
        "fixture.masque",
        r#"let main = fn () { Flex { style: { max_width: 120 } } };"#,
    );
    assert!(found.is_empty(), "{:?}", found.iter().map(|v| &v.path).collect::<Vec<_>>());
}
