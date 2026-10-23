//! A `state` assigned in an event handler is the state its render
//! declared.
//!
//! Regression: `VM::call_closure`, the path the widget tree runs handlers
//! through, did not put the closure's captured call-stack path back on the
//! runtime the way `OpCode::Call` does. A handler's `count = count + 1`
//! therefore wrote a fresh root-level `:count`, and the next render read
//! the component's own `main@1:count` — still 0. Every click counted to 1
//! and no further.

use masque::runtime::config::RuntimeConfig;
use masque::runtime::value::Value;
use masque::widget::{event::Event, point::Point};
use masque::Masque;

fn click(masque: &mut Masque, x: f32, y: f32) {
    masque
        .get_ui_mut()
        .call_event(&Event::with_point("mouse_down".into(), Point::new(x, y)));
    masque.update().expect("rerender");
    masque.get_ui_mut().layout(200.0, 100.0);
}

/// What the counter's label says, read back through a host event the
/// render raises with the value it read.
fn counting(source: &str) -> Vec<i32> {
    let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let sink = seen.clone();
    let config = RuntimeConfig::new().with_event_handler("rendered", move |args| {
        if let Some(Value::Integer(n)) = args.first() {
            sink.lock().unwrap().push(*n);
        }
        Ok(Value::Void)
    });
    let mut masque = Masque::from_source(source, config).expect("builds");
    masque.update().expect("first update");
    masque.get_ui_mut().layout(200.0, 100.0);
    for _ in 0..3 {
        click(&mut masque, 50.0, 50.0);
    }
    let seen = seen.lock().unwrap().clone();
    seen
}

#[test]
fn a_handler_counts_state_declared_in_main() {
    let source = r#"
let main = fn () {
    state count = 0;
    event("rendered", count);
    Flex {
        style: { width: "grow", height: "grow" },
        mouse_down: fn () { count = count + 1; },
    }
};
"#;
    assert_eq!(counting(source).last(), Some(&3));
}

#[test]
fn a_handler_counts_state_declared_in_a_component() {
    let source = r#"
let button = fn () {
    state count = 0;
    event("rendered", count);
    Flex {
        style: { width: "grow", height: "grow" },
        mouse_down: fn () { count = count + 1; },
    }
};
let main = fn () {
    Flex { style: { width: "grow", height: "grow" }, children: [ button() ] }
};
"#;
    assert_eq!(counting(source).last(), Some(&3));
}

#[test]
fn sibling_components_keep_their_own_counts() {
    let source = r#"
let button = fn (id: int) {
    state count = 0;
    event("rendered", id * 100 + count);
    Flex {
        style: { width: 100, height: "grow" },
        mouse_down: fn () { count = count + 1; },
    }
};
let main = fn () {
    Flex {
        style: { width: "grow", height: "grow", direction: "row" },
        children: [ button(1), button(2) ],
    }
};
"#;
    let seen = counting(source);
    // Three clicks on the first button: it reads 3, the second still 0.
    let tail = &seen[seen.len() - 2..];
    assert_eq!(tail, &[103, 200]);
}
