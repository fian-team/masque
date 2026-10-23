//! The drag gesture, driven the way a host drives it: pointer events into
//! `UI::call_event` and nothing else (`masque::widget::drag_gesture`).
//!
//! Every test builds a real document, lays it out, and presses on it. No
//! test calls `dispatch_drag_*` — the point of the gesture is that no host
//! has to.

use std::sync::{Arc, Mutex};

use masque::runtime::config::RuntimeConfig;
use masque::runtime::value::Value;
use masque::widget::event::{Event, KeyModifiers};
use masque::widget::point::Point;
use masque::Masque;

const DT: f32 = 1.0 / 60.0;
const W: f32 = 800.0;
const H: f32 = 600.0;

/// A source row at the top (0..200 × 0..40), a box that takes nothing below
/// it (0..200 × 40..100), and a target keyed `canvas` at (0, 100), 400 ×
/// 200, whose `drag_end` raises the payload and the point it was handed.
/// `pressed` counts presses on the row, so a test can see the press still
/// reached the tree.
const DOC: &str = r#"
let main = fn () {
  Flex {
    style: { width: 800, height: 600, direction: "column" },
    children: [
      Flex {
        key: "source",
        drag_payload: "workbench",
        mouse_down: fn () { event("pressed"); },
        style: { width: 200, height: 40 },
        children: [ Text { text: "Workbench" } ],
      },
      Flex { style: { width: 200, height: 60 }, children: [] },
      Flex {
        key: "canvas",
        accepts_drop: fn (payload: string): bool { return true; },
        drag_end: fn (payload: string, at: map) { event("dropped", payload, at.x, at.y); },
        style: { width: 400, height: 200 },
        children: [],
      },
    ],
  }
};
"#;

/// Everything the document raised, in order, as `name(args…)`.
type Log = Arc<Mutex<Vec<String>>>;

fn settled(src: &str) -> (Masque, Log) {
    let mut o = Masque::from_source(src, RuntimeConfig::new()).expect("from_source");
    let log: Log = Arc::new(Mutex::new(Vec::new()));
    for name in ["pressed", "dropped"] {
        let log = log.clone();
        o.with_runtime_mut(|rt| {
            rt.register_event_handler(name, move |args: &[Value]| {
                let args: Vec<String> = args.iter().map(|a| format!("{a:?}")).collect();
                log.lock().unwrap().push(format!("{name}({})", args.join(", ")));
                Ok(Value::Void)
            })
        });
    }
    for _ in 0..4 {
        o.frame(W, H, DT).expect("frame");
    }
    (o, log)
}

fn at(name: &str, x: f32, y: f32) -> Event {
    Event::with_point(name.to_string(), Point::new(x, y))
}

fn send(o: &mut Masque, e: Event) -> bool {
    o.get_ui_mut().call_event(&e)
}

fn raised(log: &Log) -> Vec<String> {
    log.lock().unwrap().clone()
}

#[test]
fn a_press_past_the_dead_zone_becomes_a_drag_and_drops_with_the_targets_own_point() {
    let (mut o, log) = settled(DOC);
    send(&mut o, at("mouse_down", 20.0, 20.0));
    assert!(o.get_ui().drag().is_none(), "a press alone is not a drag");
    // Inside the 4 px default: still a press.
    send(&mut o, at("mouse_move", 22.0, 21.0));
    assert!(o.get_ui().drag().is_none());
    // Out of it: a drag, carrying the row's payload.
    assert!(send(&mut o, at("mouse_move", 30.0, 30.0)), "a live drag owns the move");
    let view = o.get_ui().drag().expect("the press became a drag");
    assert!(matches!(&view.payload, Value::String(s) if s == "workbench"));
    assert!(view.over.is_none(), "over the source, which takes nothing");
    // Over the canvas, which says where on itself the pointer is.
    send(&mut o, at("mouse_move", 50.0, 150.0));
    let over = o.get_ui().drag().unwrap().over.expect("over the canvas");
    assert_eq!(over.key.as_deref(), Some("canvas"));
    assert_eq!((over.at.x(), over.at.y()), (50.0, 50.0));
    assert!(o.get_ui().pointer_is_ours());

    assert!(send(&mut o, at("mouse_up", 60.0, 170.0)));
    assert!(o.get_ui().drag().is_none());
    assert!(!o.get_ui().pointer_is_ours());
    o.frame(W, H, DT).unwrap();
    let log = raised(&log);
    assert_eq!(log.first().map(String::as_str), Some("pressed()"), "the press still reached the row");
    assert!(
        log.iter().any(|l| l.contains("dropped") && l.contains("workbench") && l.contains("60") && l.contains("70")),
        "drag_end carried the canvas-local point: {log:?}"
    );
}

#[test]
fn a_press_on_a_sources_label_drags_the_source() {
    let (mut o, _) = settled(DOC);
    // The Text is the deepest widget here and declares no payload.
    send(&mut o, at("mouse_down", 5.0, 5.0));
    send(&mut o, at("mouse_move", 25.0, 25.0));
    assert!(o.get_ui().drag().is_some());
}

#[test]
fn a_release_inside_the_dead_zone_is_a_click_and_drops_nothing() {
    let (mut o, log) = settled(DOC);
    send(&mut o, at("mouse_down", 20.0, 20.0));
    send(&mut o, at("mouse_move", 21.0, 21.0));
    send(&mut o, at("mouse_up", 21.0, 21.0));
    // The next move is a plain move, not the start of a drag.
    send(&mut o, at("mouse_move", 50.0, 150.0));
    assert!(o.get_ui().drag().is_none());
    o.frame(W, H, DT).unwrap();
    assert!(!raised(&log).iter().any(|l| l.starts_with("dropped")));
}

#[test]
fn a_press_away_from_any_source_never_drags() {
    let (mut o, _) = settled(DOC);
    send(&mut o, at("mouse_down", 300.0, 500.0));
    send(&mut o, at("mouse_move", 50.0, 150.0));
    assert!(o.get_ui().drag().is_none());
}

#[test]
fn escape_ends_a_drag_and_fires_nothing() {
    let (mut o, log) = settled(DOC);
    send(&mut o, at("mouse_down", 20.0, 20.0));
    send(&mut o, at("mouse_move", 50.0, 150.0));
    assert!(o.get_ui().drag().is_some());
    assert!(send(&mut o, Event::keydown(27, None, KeyModifiers::default())));
    assert!(o.get_ui().drag().is_none());
    assert!(o.get_ui().active_drag_preview().is_none());
    send(&mut o, at("mouse_up", 50.0, 150.0));
    o.frame(W, H, DT).unwrap();
    assert!(!raised(&log).iter().any(|l| l.starts_with("dropped")));
}

#[test]
fn the_wheel_turns_what_is_carried_and_nothing_else() {
    let (mut o, _) = settled(DOC);
    send(&mut o, at("mouse_down", 20.0, 20.0));
    send(&mut o, at("mouse_move", 50.0, 150.0));
    assert!(send(&mut o, Event::scroll(Point::new(50.0, 150.0), 0.0, 3.0)));
    assert!(send(&mut o, Event::scroll(Point::new(50.0, 150.0), 0.0, 1.0)));
    assert!(send(&mut o, Event::scroll(Point::new(50.0, 150.0), 0.0, -2.0)));
    assert_eq!(o.get_ui().drag().unwrap().turn, 1.0, "one per notch, signed");
}

#[test]
fn a_source_that_leaves_the_tree_ends_its_drag() {
    // The row is drawn only while `shown` is true.
    let src = r#"
host_state { shown: bool };
let main = fn () {
  Flex {
    style: { width: 800, height: 600, direction: "column" },
    children: match shown {
      true => [ Flex { drag_payload: "x", style: { width: 200, height: 40 }, children: [] } ],
      false => [],
    },
  }
};
"#;
    let config = RuntimeConfig::new().with_host_state(std::collections::HashMap::from([(
        "shown".to_string(),
        Value::Boolean(true),
    )]));
    let mut o = Masque::from_source(src, config).expect("from_source");
    for _ in 0..4 {
        o.frame(W, H, DT).unwrap();
    }
    send(&mut o, at("mouse_down", 20.0, 20.0));
    send(&mut o, at("mouse_move", 50.0, 150.0));
    assert!(o.get_ui().drag().is_some());
    o.with_runtime_mut(|rt| rt.set_host_state("shown", false));
    o.update().expect("update");
    for _ in 0..4 {
        o.frame(W, H, DT).unwrap();
    }
    // The next event finds the source gone.
    send(&mut o, at("mouse_move", 60.0, 160.0));
    assert!(o.get_ui().drag().is_none());
    assert!(o.get_ui().active_drag_preview().is_none());
}

#[test]
fn a_one_parameter_drag_end_still_gets_just_the_payload() {
    let src = r#"
let main = fn () {
  Flex {
    style: { width: 800, height: 600, direction: "column" },
    children: [
      Flex { drag_payload: "x", style: { width: 200, height: 40 }, children: [] },
      Flex {
        accepts_drop: fn (payload: string): bool { return true; },
        drag_end: fn (payload: string) { event("dropped", payload); },
        style: { width: 400, height: 200 },
        children: [],
      },
    ],
  }
};
"#;
    let (mut o, log) = settled(src);
    send(&mut o, at("mouse_down", 20.0, 20.0));
    send(&mut o, at("mouse_move", 50.0, 100.0));
    send(&mut o, at("mouse_up", 50.0, 100.0));
    o.frame(W, H, DT).unwrap();
    assert!(raised(&log).iter().any(|l| l.starts_with("dropped(") && l.contains('x')), "{:?}", raised(&log));
}

#[test]
fn an_empty_payload_is_no_source() {
    let src = r#"
let main = fn () {
  Flex {
    style: { width: 800, height: 600, direction: "column" },
    children: [ Flex { drag_payload: "", style: { width: 200, height: 40 }, children: [] } ],
  }
};
"#;
    let (mut o, _) = settled(src);
    send(&mut o, at("mouse_down", 20.0, 20.0));
    send(&mut o, at("mouse_move", 50.0, 150.0));
    assert!(o.get_ui().drag().is_none());
}

/// **A scrolled source and a scrolled target are found where they are
/// drawn.** Both walks map the pointer into a container's scrolled content
/// the way a press is dispatched; without that, a press on a row in a
/// scrolled list found the row the list had scrolled past (or nothing), and
/// a palette at the foot of a long sidebar could not be dragged at all.
#[test]
fn a_drag_from_a_scrolled_list_onto_a_scrolled_target_finds_both_where_they_are_drawn() {
    let src = r#"
let row = fn (id: string) {
  Flex { drag_payload: id, style: { width: 200, height: 40 }, children: [] }
};
let main = fn () {
  Flex {
    style: { width: 800, height: 600, direction: "row" },
    children: [
      Flex {
        style: { width: 200, height: 100, direction: "column", overflow: "scroll" },
        children: [ row("row0"), row("row1"), row("row2"), row("row3"), row("row4") ],
      },
      Flex {
        style: { width: 400, height: 100, direction: "column", overflow: "scroll" },
        children: [
          Flex { style: { width: 400, height: 100 }, children: [] },
          Flex {
            key: "deep",
            accepts_drop: fn (payload: string): bool { return true; },
            drag_end: fn (payload: string, at: map) { event("dropped", payload, at.x, at.y); },
            style: { width: 400, height: 100 },
            children: [],
          },
        ],
      },
    ],
  }
};
"#;
    let (mut o, log) = settled(src);
    // Both to the end: 100 px each, whatever a notch is worth.
    for x in [100.0, 400.0] {
        for _ in 0..20 {
            send(&mut o, Event::scroll(Point::new(x, 50.0), 0.0, -50.0));
            o.frame(W, H, DT).unwrap();
        }
    }
    for _ in 0..120 {
        o.frame(W, H, DT).unwrap();
    }

    // 20 px down the list, scrolled 100: the fourth row.
    send(&mut o, at("mouse_down", 100.0, 20.0));
    send(&mut o, at("mouse_move", 120.0, 40.0));
    let view = o.get_ui().drag().expect("the scrolled row is a source");
    assert!(matches!(&view.payload, Value::String(s) if s == "row3"), "{:?}", view.payload);
    // 50 px down the right pane, scrolled 100: inside the target, 50 into it.
    send(&mut o, at("mouse_move", 300.0, 50.0));
    let over = o.get_ui().drag().unwrap().over.expect("over the scrolled target");
    assert_eq!(over.key.as_deref(), Some("deep"));
    assert_eq!((over.at.x(), over.at.y()), (100.0, 50.0));
    send(&mut o, at("mouse_up", 300.0, 50.0));
    o.frame(W, H, DT).unwrap();
    assert!(
        raised(&log).iter().any(|l| l.starts_with("dropped(") && l.contains("row3")),
        "{:?}",
        raised(&log)
    );
}
