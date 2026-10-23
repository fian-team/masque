//! `Pad`: a `Slider` in two axes.
//!
//! A press puts the value under the cursor and takes capture; the moves
//! that follow keep reporting, at a tenth of the speed while Ctrl is
//! held; the release commits; a double-click returns to `rest`. Every
//! report is a `{ x, y }` map and every value is held inside the shape.

use std::sync::{Arc, Mutex};

use masque::runtime::config::RuntimeConfig;
use masque::runtime::value::Value;
use masque::widget::event::{Event, KeyModifiers};
use masque::widget::pad_widget::PadWidget;
use masque::widget::point::Point;
use masque::Masque;

const W: f32 = 800.0;
const H: f32 = 600.0;
const DT: f32 = 1.0 / 60.0;

type Log = Arc<Mutex<Vec<(String, f64, f64)>>>;

/// A 200 × 200 pad at the origin over `-1..1` on both axes, with
/// `extra` spliced in.
fn document(extra: &str) -> String {
    format!(
        r#"
let main = fn () {{
  Flex {{
    style: {{ width: "grow", height: "grow" }},
    children: [
      Pad {{
        x: 0,
        y: 0,
        {extra}
        on_change: fn (v: map) {{ event("changed", v); }},
        on_commit: fn (v: map) {{ event("committed", v); }},
        style: {{ width: 200, height: 200 }},
      }}
    ],
  }}
}};
"#
    )
}

fn mounted(src: &str) -> (Masque, Log) {
    let log: Log = Arc::new(Mutex::new(Vec::new()));
    let mut config = RuntimeConfig::new();
    for name in ["changed", "committed"] {
        let log = log.clone();
        config = config.with_event_handler(name, move |args| {
            let Some(Value::Map(m)) = args.first() else {
                panic!("a pad reports a map, got {:?}", args.first());
            };
            let axis = |k: &str| match m.get(k) {
                Some(Value::Float(f)) => (*f * 1000.0).round() / 1000.0,
                other => panic!("`{k}` is a float, got {other:?}"),
            };
            log.lock().unwrap().push((name.to_string(), axis("x"), axis("y")));
            Ok(Value::Void)
        });
    }
    let mut o = Masque::from_source(src, config).expect("the document mounts");
    o.frame(W, H, DT).expect("frame");
    (o, log)
}

fn send(o: &mut Masque, name: &str, x: f32, y: f32) -> bool {
    o.get_ui_mut()
        .call_event(&Event::with_point(name.to_string(), Point::new(x, y)))
}

fn send_ctrl(o: &mut Masque, name: &str, x: f32, y: f32) -> bool {
    let ctrl = KeyModifiers { ctrl: true, ..KeyModifiers::default() };
    o.get_ui_mut()
        .call_event(&Event::with_point(name.to_string(), Point::new(x, y)).with_modifiers(ctrl))
}

fn row(name: &str, x: f64, y: f64) -> (String, f64, f64) {
    (name.to_string(), x, y)
}

fn pad_value(o: &Masque) -> (f32, f32) {
    let child = o.get_ui().root.lock().unwrap().get_children()[0].clone();
    let g = child.lock().unwrap();
    let p = g.downcast_ref::<PadWidget>().expect("a Pad");
    (p.x, p.y)
}

#[test]
fn a_press_reports_the_point_under_it_with_y_up() {
    let (mut o, log) = mounted(&document(""));
    assert!(send(&mut o, "mouse_down", 150.0, 50.0));
    assert_eq!(log.lock().unwrap().as_slice(), [row("changed", 0.5, 0.5)]);
    assert!(o.get_ui().captured().is_some(), "the press took capture");
}

#[test]
fn a_drag_keeps_reporting_off_the_pad_and_the_release_commits() {
    let (mut o, log) = mounted(&document(""));
    send(&mut o, "mouse_down", 100.0, 100.0);
    send(&mut o, "mouse_move", 150.0, 150.0);
    send(&mut o, "mouse_move", 900.0, 100.0);
    send(&mut o, "mouse_up", 900.0, 100.0);
    assert_eq!(
        log.lock().unwrap().as_slice(),
        [
            row("changed", 0.5, -0.5),
            row("changed", 1.0, 0.0),
            row("committed", 1.0, 0.0),
        ],
        "the centre press is no change; off the pad it clamps"
    );
    assert!(o.get_ui().captured().is_none(), "the release ended the capture");
}

/// The fixture that could fail: a point off the rim but inside the box,
/// where a rect would report its corner.
#[test]
fn a_disc_holds_the_value_on_its_rim() {
    let (mut o, log) = mounted(&document(r#"shape: "disc","#));
    send(&mut o, "mouse_down", 100.0, 100.0);
    send(&mut o, "mouse_move", 200.0, 0.0);
    let log = log.lock().unwrap();
    let (_, x, y) = log[0].clone();
    assert!((x * x + y * y - 1.0).abs() < 1e-3, "on the rim: {x}, {y}");
    assert!((x - y).abs() < 1e-3, "toward the corner it was dragged to");
}

#[test]
fn a_press_outside_the_disc_is_not_the_pads() {
    let (mut o, log) = mounted(&document(r#"shape: "disc","#));
    send(&mut o, "mouse_down", 4.0, 4.0);
    assert!(log.lock().unwrap().is_empty());
    assert!(o.get_ui().captured().is_none());
}

#[test]
fn ctrl_drags_at_a_tenth_of_the_speed_and_leaves_the_dot_where_it_is() {
    let (mut o, log) = mounted(&document(""));
    send_ctrl(&mut o, "mouse_down", 150.0, 150.0);
    assert!(log.lock().unwrap().is_empty(), "a fine press does not jump");
    send_ctrl(&mut o, "mouse_move", 200.0, 150.0);
    send_ctrl(&mut o, "mouse_up", 200.0, 150.0);
    assert_eq!(
        log.lock().unwrap().as_slice(),
        [row("changed", 0.05, 0.0), row("committed", 0.05, 0.0)]
    );
}

#[test]
fn a_double_click_returns_to_rest_and_commits() {
    let (mut o, log) = mounted(&document("rest: { x: 0.25, y: -0.25 },"));
    send(&mut o, "mouse_down", 150.0, 50.0);
    send(&mut o, "mouse_up", 150.0, 50.0);
    send(&mut o, "mouse_down", 150.0, 50.0);
    assert_eq!(
        log.lock().unwrap().as_slice(),
        [
            row("changed", 0.5, 0.5),
            row("committed", 0.5, 0.5),
            row("changed", 0.25, -0.25),
            row("committed", 0.25, -0.25),
        ]
    );
    assert!(o.get_ui().captured().is_none(), "a reset is not a drag");
}

#[test]
fn without_rest_a_second_press_is_a_press() {
    let (mut o, log) = mounted(&document(""));
    send(&mut o, "mouse_down", 150.0, 50.0);
    send(&mut o, "mouse_up", 150.0, 50.0);
    send(&mut o, "mouse_down", 150.0, 50.0);
    assert!(o.get_ui().captured().is_some());
    assert_eq!(log.lock().unwrap().len(), 2, "no change the second time: {:?}", log.lock().unwrap());
}

/// The document supplies the value: never written back, the next render
/// puts the dot where the document said.
#[test]
fn the_value_is_the_documents() {
    let (mut o, _log) = mounted(&document(""));
    send(&mut o, "mouse_down", 150.0, 50.0);
    assert_eq!(pad_value(&o), (0.5, 0.5));
    o.with_runtime_mut(|rt| rt.request_rerender());
    o.frame(W, H, DT).expect("frame");
    assert_eq!(pad_value(&o), (0.0, 0.0));
}

#[test]
fn arrow_keys_step_a_hundredth_and_commit_while_focused() {
    let (mut o, log) = mounted(&document(""));
    send(&mut o, "mouse_down", 100.0, 100.0);
    send(&mut o, "mouse_up", 100.0, 100.0);
    log.lock().unwrap().clear();
    o.get_ui_mut().call_event(&Event::keydown(38, None, KeyModifiers::default()));
    o.get_ui_mut().call_event(&Event::keydown(39, None, KeyModifiers::default()));
    assert_eq!(
        log.lock().unwrap().as_slice(),
        [
            row("changed", 0.0, 0.02),
            row("committed", 0.0, 0.02),
            row("changed", 0.02, 0.02),
            row("committed", 0.02, 0.02),
        ]
    );
}

#[test]
fn a_missing_axis_or_a_bad_shape_is_refused() {
    let err = |src: &str| {
        format!(
            "{:?}",
            Masque::from_source(src, RuntimeConfig::new()).err().expect("refused")
        )
    };
    let missing = err(r#"let main = fn () { Pad { x: 0 } };"#);
    assert!(missing.contains('y'), "{missing}");
    let shape = err(&document(r#"shape: "circle","#));
    assert!(shape.contains("shape"), "{shape}");
}
