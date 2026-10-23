//! `Masque::frame` — the single-call frame drive — and the
//! state-preserving hot reload it depends on. A reload rebuilds the
//! runtime from the config's *initial* host state; `frame`'s contract
//! is that hosts push state only when it changes, so the reload must
//! carry the live host-state map forward or the first reloaded frame
//! would render placeholders.

use std::collections::HashMap;

use masque::runtime::config::RuntimeConfig;
use masque::runtime::value::Value;
use masque::widget::text_widget::TextWidget;
use masque::Masque;

/// A strict-mode module whose whole tree is one host-state-driven Text,
/// so the root widget downcasts straight to `TextWidget` for assertions.
const LABELED_TREE: &str = r##"
host_state { label: string };
let main = fn () {
  Text { text: label, style: { size: 16 } }
};
"##;

fn build_with(label: &str) -> Masque {
    let config = RuntimeConfig::new().with_host_state(HashMap::from([(
        "label".to_string(),
        Value::String(label.to_string()),
    )]));
    Masque::from_source(LABELED_TREE, config).expect("Masque::from_source")
}

fn root_text(masque: &Masque) -> String {
    let root = masque.get_ui().root.clone();
    let g = root.lock().expect("widget lock poisoned");
    g.downcast_ref::<TextWidget>()
        .expect("root widget is Text")
        .text
        .clone()
}

#[test]
fn reload_carries_live_host_state_into_the_new_runtime() {
    let mut masque = build_with("placeholder");
    assert_eq!(root_text(&masque), "placeholder");

    masque.with_runtime_mut(|rt| rt.set_host_state("label", "live"));
    masque.update().expect("update");
    assert_eq!(root_text(&masque), "live");

    masque
        .recompile_from_source(LABELED_TREE)
        .expect("recompile");

    // The reloaded tree shows live state before any further update() —
    // the reload's own module execution already saw the carried map.
    assert_eq!(root_text(&masque), "live");
    assert_eq!(
        masque.with_runtime_mut(|rt| rt.get_host_state("label")),
        Some(Value::String("live".to_string()))
    );
}

#[test]
fn frame_reexecutes_only_when_host_state_changed() {
    let mut masque = build_with("first");

    // Settle whatever construction left pending.
    masque.frame(800.0, 600.0, 1.0 / 60.0).expect("frame");

    // Idle frame: nothing changed, no re-execute, and no watched file
    // means no frame can ever report a reload.
    let report = masque.frame(800.0, 600.0, 1.0 / 60.0).expect("frame");
    assert!(!report.rerendered);
    assert!(!report.reloaded);

    // A changed value re-executes and the tree reflects it...
    masque.with_runtime_mut(|rt| rt.set_host_state("label", "second"));
    assert!(
        masque
            .frame(800.0, 600.0, 1.0 / 60.0)
            .expect("frame")
            .rerendered
    );
    assert_eq!(root_text(&masque), "second");

    // ...an unchanged re-inject does not.
    masque.with_runtime_mut(|rt| rt.set_host_state("label", "second"));
    assert!(
        !masque
            .frame(800.0, 600.0, 1.0 / 60.0)
            .expect("frame")
            .rerendered
    );
}
