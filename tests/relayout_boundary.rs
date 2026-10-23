//! Relayout boundaries: a Flex whose width and height are both fixed
//! reports the same size whatever its children do, so a change inside it
//! is laid out by itself rather than costing the whole tree a pass.
//!
//! Every test that claims containment has a twin fixture that must *not*
//! be contained — the `loose` box, which shrinks to its content and so
//! moves its neighbour when its content changes. Without that twin, a rule
//! that contained everything would pass every test here.

use std::collections::{BTreeMap, HashMap};

use masque::runtime::config::RuntimeConfig;
use masque::runtime::value::Value;
use masque::widget::flex_widget::FlexWidget;
use masque::widget::style::Size;
use masque::widget::{LayoutCounts, WidgetRef};
use masque::Masque;

const W: f32 = 800.0;
const H: f32 = 600.0;
const DT: f32 = 1.0 / 60.0;

/// A fixed 200×20 track holding a fill whose width is a host value and a
/// marker placed after it; beside it, a shrink box holding a fill of its
/// own, then a neighbour whose x is that box's width.
const DOC: &str = r#"
let main = fn () {
  Flex {
    style: { width: "grow", height: "grow", direction: "column" },
    children: [
      Flex {
        key: "track",
        style: { width: track_w, height: 20, direction: "row" },
        children: [
          Flex { key: "fill", style: { width: 200.0 * fill, height: "grow" } },
          Flex { key: "after", style: { width: 10, height: "grow" } },
          match swap {
            true => Text { text: "now a label", style: { width: "shrink", height: "shrink" } },
            false => Flex { style: { width: 4, height: 4 } },
          },
          Flex {
            key: "padded",
            style: {
              width: "shrink", height: "grow",
              padding: { left: pad },
              transition: { padding: { stiffness: 170, damping: 26 } },
            },
            children: [ Flex { key: "pad_inner", style: { width: 6, height: 6 } } ],
          },
        ],
      },
      Flex {
        style: { width: "grow", height: "shrink", direction: "row" },
        children: [
          Flex {
            key: "loose",
            style: { width: "shrink", height: "shrink", direction: "row" },
            children: [ Flex { key: "loose_fill", style: { width: 100.0 * loose, height: 10 } } ],
          },
          Flex { key: "neighbour", style: { width: 20, height: 10 } },
        ],
      },
    ],
  }
};
"#;

fn initial() -> Vec<(&'static str, Value)> {
    vec![
        ("fill", Value::Float(0.5)),
        ("loose", Value::Float(0.5)),
        ("track_w", Value::Float(200.0)),
        ("pad", Value::Float(0.0)),
        ("swap", Value::Boolean(false)),
    ]
}

fn mounted(src: &str) -> Masque {
    let config = RuntimeConfig::new().with_host_state(
        initial()
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect::<HashMap<_, _>>(),
    );
    let mut o = Masque::from_source(src, config).expect("the document mounts");
    for _ in 0..3 {
        o.frame(W, H, DT).expect("frame");
    }
    o
}

fn set(o: &mut Masque, name: &str, value: Value) {
    o.with_runtime_mut(|rt| rt.set_host_state(name, value));
}

fn counts(o: &Masque) -> LayoutCounts {
    o.get_ui().layout_counts()
}

/// Every keyed widget's parent-relative rect.
fn rects(o: &Masque) -> BTreeMap<String, (f32, f32, f32, f32)> {
    fn walk(node: &WidgetRef, out: &mut BTreeMap<String, (f32, f32, f32, f32)>) {
        let children = {
            let g = node.lock().unwrap();
            if let (Some(k), Some(r)) = (g.key(), g.get_layout_rect()) {
                out.insert(k.to_string(), (r.x, r.y, r.width, r.height));
            }
            g.get_children()
        };
        for c in &children {
            walk(c, out);
        }
    }
    let mut out = BTreeMap::new();
    walk(&o.get_ui().root, &mut out);
    out
}

fn rect(o: &Masque, key: &str) -> (f32, f32, f32, f32) {
    rects(o)[key]
}

/// Lay the whole tree out from scratch and return what it produced.
fn full_pass_rects(o: &mut Masque) -> BTreeMap<String, (f32, f32, f32, f32)> {
    o.get_ui_mut().mark_needs_layout();
    o.get_ui_mut().layout(W, H);
    rects(o)
}

#[test]
fn a_change_inside_a_fixed_box_lays_out_only_the_box() {
    let mut o = mounted(DOC);
    let before = counts(&o);
    set(&mut o, "fill", Value::Float(0.25));
    assert!(o.update().unwrap());
    assert!(!o.get_ui().needs_layout(), "the track absorbed it");
    assert!(o.get_ui().needs_boundary_layout());
    o.get_ui_mut().layout(W, H);

    let after = counts(&o);
    assert_eq!(after.full, before.full, "no full pass");
    assert_eq!(after.boundary, before.boundary + 1, "one boundary pass");
    assert_eq!(rect(&o, "fill").2, 50.0, "the fill was laid out at its new width");
    assert_eq!(rect(&o, "after").0, 50.0, "and what follows it moved with it");
}

#[test]
fn a_boundary_pass_lays_out_what_a_full_pass_would() {
    let mut o = mounted(DOC);
    set(&mut o, "fill", Value::Float(0.8));
    o.update().unwrap();
    o.get_ui_mut().layout(W, H);
    let partial = rects(&o);
    assert_eq!(full_pass_rects(&mut o), partial);
}

#[test]
fn a_change_inside_a_shrink_box_still_lays_out_the_tree() {
    let mut o = mounted(DOC);
    let before = counts(&o);
    let neighbour_x = rect(&o, "neighbour").0;
    set(&mut o, "loose", Value::Float(0.9));
    o.update().unwrap();
    assert!(o.get_ui().needs_layout(), "a shrink box is its content's size");
    o.get_ui_mut().layout(W, H);

    assert_eq!(counts(&o).full, before.full + 1);
    assert_eq!(
        rect(&o, "neighbour").0,
        neighbour_x + 40.0,
        "the neighbour moved — which a contained change could not have done"
    );
}

#[test]
fn a_fixed_box_changing_its_own_size_lays_out_the_tree() {
    let mut o = mounted(DOC);
    let before = counts(&o);
    set(&mut o, "track_w", Value::Float(300.0));
    o.update().unwrap();
    assert!(o.get_ui().needs_layout());
    o.get_ui_mut().layout(W, H);
    assert_eq!(counts(&o).full, before.full + 1);
    assert_eq!(rect(&o, "track").2, 300.0);
}

#[test]
fn a_layout_spring_inside_a_fixed_box_animates_by_boundary_passes() {
    let mut o = mounted(DOC);
    let before = counts(&o);
    set(&mut o, "pad", Value::Float(30.0));
    for _ in 0..10 {
        o.frame(W, H, DT).unwrap();
    }
    let after = counts(&o);
    assert_eq!(after.full, before.full, "ten frames of a padding spring, no full pass");
    assert!(
        after.boundary >= 10,
        "every frame the spring moved was a boundary pass, got {}",
        after.boundary - before.boundary
    );
    let inner_x = rect(&o, "pad_inner").0;
    assert!(inner_x > 0.0 && inner_x < 30.0, "mid-spring, got {inner_x}");
    assert_eq!(full_pass_rects(&mut o)["pad_inner"].0, inner_x);
}

#[test]
fn a_replaced_child_inside_a_fixed_box_is_laid_out_by_the_boundary() {
    let mut o = mounted(DOC);
    let before = counts(&o);
    let padded_x = rect(&o, "padded").0;
    set(&mut o, "swap", Value::Boolean(true));
    o.update().unwrap();
    assert!(!o.get_ui().needs_layout());
    o.get_ui_mut().layout(W, H);
    assert_eq!(counts(&o).full, before.full);
    assert!(
        rect(&o, "padded").0 > padded_x,
        "the label that replaced the 4px box was measured, and pushed its sibling along"
    );
    let partial = rects(&o);
    assert_eq!(full_pass_rects(&mut o), partial);
}

#[test]
fn a_full_pass_owed_beside_a_boundary_pays_for_both() {
    let mut o = mounted(DOC);
    let before = counts(&o);
    set(&mut o, "fill", Value::Float(0.1));
    set(&mut o, "loose", Value::Float(0.1));
    o.update().unwrap();
    o.get_ui_mut().layout(W, H);
    let after = counts(&o);
    assert_eq!((after.full, after.boundary), (before.full + 1, before.boundary));
    assert!(!o.get_ui().needs_boundary_layout(), "the full pass cleared the mark");
    assert_eq!(rect(&o, "fill").2, 20.0, "and laid the boundary's inside out too");

    o.get_ui_mut().layout(W, H);
    assert_eq!(counts(&o), after, "nothing is left owed");
}

/// Every document the route tier mounts is rooted in a Presence, so a
/// boundary under one is the case a game actually has.
#[test]
fn a_boundary_under_a_presence_is_contained() {
    let src = DOC.replacen(
        r#"  Flex {
    style: { width: "grow", height: "grow", direction: "column" },"#,
        r#"  Presence {
    key: "screen",
    style: { width: "grow", height: "grow", direction: "column" },"#,
        1,
    );
    assert_ne!(src, DOC, "the fixture was rewritten");
    let mut o = mounted(&src);
    let before = counts(&o);
    set(&mut o, "fill", Value::Float(0.3));
    o.update().unwrap();
    assert!(!o.get_ui().needs_layout());
    o.get_ui_mut().layout(W, H);
    assert_eq!(counts(&o).full, before.full);
    assert_eq!(rect(&o, "fill").2, 60.0);
}

/// The Presence's own box can be the boundary: it wraps a Flex and
/// reconciles that Flex's children itself, so it has to contain too.
#[test]
fn a_fixed_presence_is_itself_a_boundary() {
    let src = r#"
let main = fn () {
  Flex {
    style: { width: "grow", height: "grow" },
    children: [
      Presence {
        key: "card",
        style: { width: 300, height: 100, direction: "row" },
        children: [ Flex { key: "fill", style: { width: 300.0 * fill, height: 10 } } ],
      },
    ],
  }
};
"#;
    let mut o = mounted(src);
    let before = counts(&o);
    set(&mut o, "fill", Value::Float(0.2));
    o.update().unwrap();
    assert!(!o.get_ui().needs_layout(), "the Presence's box absorbed it");
    o.get_ui_mut().layout(W, H);
    assert_eq!(counts(&o).full, before.full);
    assert_eq!(rect(&o, "fill").2, 60.0);
}

/// A box with no constraints to repeat cannot lay itself out alone.
#[test]
fn a_box_never_laid_out_is_not_a_boundary() {
    let mut w = FlexWidget::new();
    w.style.width = Size::Fixed(100.0);
    w.style.height = Size::Fixed(10.0);
    assert!(!w.is_layout_boundary());
    assert_eq!(w.contain_layout(false, true, false), (true, false));
}
