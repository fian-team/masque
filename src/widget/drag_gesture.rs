//! **The drag gesture** — the decision that a press has become a drag, and
//! everything that follows from it, made inside [`UI::call_event`].
//!
//! Before 2026-10-11 ogham shipped the dispatch (`dispatch_drag_start`,
//! `_move`, `_end`), the drop hit test and the cursor-attached preview, and
//! nothing that called them: `DragState`'s doc credited the dead-zone state
//! machine to a host input pump that did not exist. Each host that wanted a
//! drag wrote its own, and the one that did (`untold_lore`'s inventory) had
//! to re-find the drag source, the drop point and the drop target's
//! rectangle by walking the tree itself. This module is that machine, once,
//! fed by the pointer events every host already hands `call_event`.
//!
//! # The rules
//!
//! 1. **A press arms; distance starts.** A `mouse_down` that no widget
//!    captured arms the deepest widget under it that declares a
//!    `drag_payload` (portal layers first, then the base tree — the press's
//!    own order). A payload of `""` declares no source, which is how one
//!    widget says *not this one* without a null. The press is still a press: it reaches the tree exactly as
//!    it would have, so a row that selects on press still selects. The drag
//!    begins when the pointer leaves the source's `drag_dead_zone`
//!    ([`DEFAULT_DEAD_ZONE`] when it declares none); a release before that is
//!    an ordinary click.
//! 2. **A live drag owns the pointer.** Every move, the release, the wheel
//!    and Escape are the drag's and are reported handled, so neither hover
//!    nor a host's world layer reacts underneath it. A host gates its own
//!    pointer work on [`UI::pointer_is_ours`].
//! 3. **The wheel turns what is carried.** Each notch adds one, signed, to
//!    [`DragView::turn`]; what a notch means is the host's. This is the one
//!    wheel rule for every drag until a drag needs to scroll a list it is
//!    over.
//! 4. **The drop says where.** `drag_end` fires on the deepest widget under
//!    the pointer whose `accepts_drop` answers true, as before. A listener
//!    declared with two parameters is also handed `at`, the drop point in
//!    that widget's own pixels as `{ x, y }` — which is what lets a widget
//!    stand for something the host draws (a map, a world view) and the host
//!    turn the point into its own coordinates.
//! 5. **A drag ends four ways**: a release (rule 4), Escape, the source
//!    leaving the tree or starting its exit, and [`UI::cancel_drag`] from a
//!    host. Only the release fires anything.
//!
//! [`UI::drag`] is the in-flight drag as the host reads it each frame — the
//! payload, the pointer, the turn, and the target it is over with that
//! target's local point — so a host can draw a preview in its own world
//! without waiting for an event.

use super::event::{DragState, Event};
use super::point::Point;
use super::{PortalEntry, UI, WidgetRef};
use crate::runtime::value::Value;

/// How far, in pixels, the pointer must travel from the press before the
/// press becomes a drag, for a source that declares no `drag_dead_zone`.
pub const DEFAULT_DEAD_ZONE: f32 = 4.0;

/// The gesture's state. Owned by [`UI`].
#[derive(Default)]
pub(super) enum Gesture {
    #[default]
    Idle,
    /// A press on a drag source that has not yet travelled its dead zone.
    Armed(Armed),
    /// A drag in flight.
    Live(Live),
}

pub(super) struct Armed {
    origin: WidgetRef,
    payload: Value,
    at: Point,
    dead_zone: f32,
}

pub(super) struct Live {
    origin: WidgetRef,
    state: DragState,
    turn: f32,
    over: Option<(WidgetRef, Point)>,
}

/// The in-flight drag, as a host reads it ([`UI::drag`]).
#[derive(Clone, Debug)]
pub struct DragView {
    /// The source's `drag_payload`.
    pub payload: Value,
    /// The pointer, in window pixels.
    pub at: Point,
    /// Wheel notches since the drag began, signed (rule 3).
    pub turn: f32,
    /// The drop target under the pointer, if any widget there accepts the
    /// payload.
    pub over: Option<DropOver>,
}

/// The widget a drag is over and would drop onto.
#[derive(Clone, Debug)]
pub struct DropOver {
    /// The target's `key`, which is how a host tells its targets apart.
    pub key: Option<String>,
    /// The pointer in the target's own pixels — the point `drag_end`'s `at`
    /// will carry if the drag is released here.
    pub at: Point,
}

impl UI {
    /// The drag in flight, or `None` — including while a press is armed but
    /// has not yet travelled its dead zone.
    pub fn drag(&self) -> Option<DragView> {
        let Gesture::Live(live) = &self.gesture else {
            return None;
        };
        Some(DragView {
            payload: live.state.payload.clone().unwrap_or(Value::Void),
            at: live.state.current_position.clone(),
            turn: live.turn,
            over: live.over.as_ref().map(|(widget, at)| DropOver {
                key: widget
                    .lock()
                    .expect("widget lock poisoned")
                    .key()
                    .map(str::to_string),
                at: at.clone(),
            }),
        })
    }

    /// Whether the chrome owns the pointer right now: a widget holds pointer
    /// capture, or a drag is in flight. A host that runs its own pointer
    /// work under the chrome (picking, hover, a camera on the wheel) stands
    /// down while this is true, as it does over a blocking widget.
    pub fn pointer_is_ours(&self) -> bool {
        self.captured.is_some() || matches!(self.gesture, Gesture::Live(_))
    }

    /// End any armed press or drag without firing anything (rule 5). For a
    /// host whose surface went away under the gesture. Idempotent.
    pub fn cancel_drag(&mut self) {
        if let Gesture::Live(_) = std::mem::take(&mut self.gesture) {
            self.clear_active_drag();
        }
    }

    /// Run the gesture for `event`. `Some(handled)` when the gesture took the
    /// event; `None` when it goes on to ordinary dispatch.
    pub(super) fn pump_drag_gesture(&mut self, event: &Event) -> Option<bool> {
        if !matches!(self.gesture, Gesture::Idle) && !self.drag_origin_mounted() {
            self.cancel_drag();
            return None;
        }
        match std::mem::take(&mut self.gesture) {
            Gesture::Idle => None,
            Gesture::Armed(armed) => self.pump_armed(armed, event),
            Gesture::Live(live) => Some(self.pump_live(live, event)),
        }
    }

    fn pump_armed(&mut self, armed: Armed, event: &Event) -> Option<bool> {
        match (event.name.as_str(), &event.point) {
            ("mouse_move", Some(point)) => {
                let (dx, dy) = (point.x() - armed.at.x(), point.y() - armed.at.y());
                if dx * dx + dy * dy < armed.dead_zone * armed.dead_zone {
                    self.gesture = Gesture::Armed(armed);
                    return None;
                }
                self.last_mouse = Some(point.clone());
                let state = self.dispatch_drag_start(
                    armed.origin.clone(),
                    armed.payload.clone(),
                    point.clone(),
                );
                let over = self.drop_target_at(&armed.payload, point);
                self.gesture = Gesture::Live(Live {
                    origin: armed.origin,
                    state,
                    turn: 0.0,
                    over,
                });
                Some(true)
            }
            // A release inside the dead zone was a click; anything else
            // leaves the press armed.
            ("mouse_up", _) => None,
            _ => {
                self.gesture = Gesture::Armed(armed);
                None
            }
        }
    }

    fn pump_live(&mut self, mut live: Live, event: &Event) -> bool {
        match (event.name.as_str(), &event.point) {
            ("mouse_move", Some(point)) => {
                self.last_mouse = Some(point.clone());
                self.dispatch_drag_move(&mut live.state, point.clone());
                let payload = live.state.payload.clone().unwrap_or(Value::Void);
                live.over = self.drop_target_at(&payload, point);
                self.gesture = Gesture::Live(live);
                true
            }
            ("mouse_up", Some(point)) => {
                self.dispatch_drag_end(&mut live.state, point.clone());
                true
            }
            ("scroll", _) => {
                let dy = event.scroll_delta.map_or(0.0, |(_, dy)| dy);
                if dy != 0.0 {
                    live.turn += dy.signum();
                }
                self.gesture = Gesture::Live(live);
                true
            }
            ("keydown", _)
                if event.keyboard_data.as_ref().and_then(|k| k.key_code) == Some(27) =>
            {
                self.clear_active_drag();
                true
            }
            // Every other pointer event is the drag's too; a key that is not
            // Escape goes on to the tree.
            (_, Some(_)) => {
                self.gesture = Gesture::Live(live);
                true
            }
            _ => {
                self.gesture = Gesture::Live(live);
                false
            }
        }
    }

    /// Arm a press at `point` on the deepest drag source under it (rule 1).
    /// Called by `call_event` after an uncaptured `mouse_down` has been
    /// dispatched.
    pub(super) fn arm_drag(&mut self, point: &Point) {
        self.gesture = match self.drag_source_at(point) {
            Some((origin, payload, dead_zone)) => Gesture::Armed(Armed {
                origin,
                payload,
                at: point.clone(),
                dead_zone,
            }),
            None => Gesture::Idle,
        };
    }

    /// Whether the gesture's source is still in the tree and not leaving it.
    fn drag_origin_mounted(&self) -> bool {
        let origin = match &self.gesture {
            Gesture::Idle => return true,
            Gesture::Armed(armed) => &armed.origin,
            Gesture::Live(live) => &live.origin,
        };
        if origin.lock().expect("widget lock poisoned").is_exiting() {
            return false;
        }
        let mut path = Vec::new();
        super::path_to(&self.root, origin, &mut path)
    }

    /// The deepest drag source at `point`: portal layers in hit-test order,
    /// then the base tree, stopping at a layer whose backdrop consumes an
    /// outside press — the order a press is dispatched in.
    fn drag_source_at(&self, point: &Point) -> Option<(WidgetRef, Value, f32)> {
        let entries: Vec<PortalEntry> = self.portal_layers.iter_hit_test_order().cloned().collect();
        for entry in &entries {
            let local = Point::new(point.x() - entry.viewport_rect.x, point.y() - entry.viewport_rect.y);
            let children = entry.widget.lock().expect("widget lock poisoned").get_children();
            for child in &children {
                let contains = child.lock().expect("widget lock poisoned").contains_point(&local);
                if contains {
                    let mut found = None;
                    deepest_source(child, &local, &mut found);
                    return found;
                }
            }
            if Self::entry_policy(entry).consumes_outside_press() {
                return None;
            }
        }
        if !self.root.lock().expect("widget lock poisoned").contains_point(point) {
            return None;
        }
        let mut found = None;
        deepest_source(&self.root, point, &mut found);
        found
    }

    /// The drop target at `point` for `payload`, with the point in its own
    /// pixels.
    fn drop_target_at(&self, payload: &Value, point: &Point) -> Option<(WidgetRef, Point)> {
        self.hit_test_drop_target_local(payload, point)
    }
}

/// Walk down the hit path from `node` (whose parent's space `at` is in),
/// keeping the deepest node that declares a `drag_payload`. `found` holds the
/// nearest source *above* a hit that has none of its own, so a press on a
/// row's label drags the row.
fn deepest_source(node: &WidgetRef, at: &Point, found: &mut Option<(WidgetRef, Value, f32)>) {
    let (origin, scroll, children, source) = {
        let g = node.lock().expect("widget lock poisoned");
        let origin = g.get_layout_rect().map(|r| (r.x, r.y)).unwrap_or((0.0, 0.0));
        let source = g
            .drag_payload()
            .cloned()
            .map(|p| (p, g.drag_dead_zone().unwrap_or(DEFAULT_DEAD_ZONE)));
        (origin, g.scroll_offset(), g.get_children(), source)
    };
    if let Some((payload, dead_zone)) = source {
        *found = Some((node.clone(), payload, dead_zone));
    }
    // Into the node's scrolled content, as a press is dispatched.
    let local = Point::new(at.x() - origin.0 + scroll.0, at.y() - origin.1 + scroll.1);
    for child in children.iter().rev() {
        let hit = {
            let g = child.lock().expect("widget lock poisoned");
            !g.is_exiting() && g.contains_point(&local)
        };
        if hit {
            deepest_source(child, &local, found);
            return;
        }
    }
}

/// `{ x, y }` — how a point reaches a document.
pub(crate) fn point_value(point: &Point) -> Value {
    Value::map([
        ("x".to_string(), Value::Float(point.x() as f64)),
        ("y".to_string(), Value::Float(point.y() as f64)),
    ])
}
