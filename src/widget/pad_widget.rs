//! A native two-axis control — [`super::slider_widget`] in two
//! dimensions.
//!
//! `Pad { x, y, x_min, x_max, y_min, y_max, shape, rest, on_change,
//! on_commit, style, dot_color, ring_color }`. The value is
//! **controlled**, as a `Slider`'s is: the document supplies `x` and `y`
//! every render and the widget reports what the pointer asked for, as a
//! map `{ x, y }`, through `on_change` while the button is held and
//! `on_commit` on the release. A document that writes back something
//! else — a value clipped by a rule only the host knows — sees the dot
//! drawn where it said, and the next report starts from the pointer
//! again.
//!
//! # The plane
//!
//! The content box (the style's box less margin and padding) is the
//! plane. `x` runs left to right from `x_min` to `x_max`; `y` runs
//! **bottom to top** from `y_min` to `y_max`, the way a chart's axis
//! does, because the hosts this was built for draw planes from maths
//! (a colour plane's `b` axis points up).
//!
//! `shape: "rect"` (the default) is the whole box, each axis clamped on
//! its own. `shape: "disc"` is the circle inscribed in the box, centred,
//! with the box's shorter side as its diameter; the value is held inside
//! the ellipse that circle maps to, and a press outside the circle is
//! not the pad's.
//!
//! # The gestures
//!
//! - **A press puts the dot under the cursor**, and a drag moves it as
//!   far as the cursor moves, wherever the cursor goes: the press takes
//!   pointer capture ([`crate::widget::UI`]'s) until the release. A
//!   Ctrl-held press leaves the dot where it is and only drags it.
//! - **Ctrl held moves the dot at a tenth of the cursor's speed.**
//!   Pressing or letting go of it mid-drag rebases the drag where it
//!   stands, so neither end of a fine stretch jumps.
//! - **A double-click returns the value to `rest`** and commits it. A
//!   pad with no `rest` treats the second press as a press.
//! - **Arrow keys, while focused, move one hundredth of an axis's range**
//!   and commit at once — a key is not a drag.
//!
//! Every reported value is clamped into the shape. Nothing here knows
//! what the plane means: a picture behind it is a sibling `Canvas`
//! under an absolutely positioned `Pad`, the composition `AGENTS.md`
//! prescribes for chrome over painted content.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use super::event::{Event, EventContext};
use super::point::Point;
use super::rect::Rect;
use super::style::{Color, Corners, CursorRole, Direction, FlexStyle, Position, Size};
use super::{LayoutContext, RenderContext, UpdateResult, Widget, WidgetRef};
use crate::runtime::value::Value;
use crate::widget::image::ImageCache;

/// The box a `shrink`-sized pad takes.
const DEFAULT_SIDE: f32 = 160.0;
/// The dot's radius, and the ring's width around it.
const DOT_RADIUS: f32 = 5.0;
const RING_WIDTH: f32 = 1.5;
/// How much slower a Ctrl-held drag moves the value than the cursor.
const FINE: f32 = 0.1;
/// Two presses at most this far apart in time and space are a
/// double-click.
const DOUBLE_CLICK: Duration = Duration::from_millis(400);
const DOUBLE_CLICK_SLOP: f32 = 4.0;

/// Which part of the box is the plane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PadShape {
    Rect,
    Disc,
}

/// A drag in progress: where it was last rebased, and whether it is
/// the fine kind.
#[derive(Debug, Clone, Copy)]
struct Grab {
    pointer: (f32, f32),
    value: (f32, f32),
    fine: bool,
}

pub struct PadWidget {
    pub x: f32,
    pub y: f32,
    pub x_min: f32,
    pub x_max: f32,
    pub y_min: f32,
    pub y_max: f32,
    pub shape: PadShape,
    /// Where a double-click returns the value. `None` has no reset.
    pub rest: Option<(f32, f32)>,
    pub style: FlexStyle,
    pub dot_color: Color,
    pub ring_color: Color,
    /// `on_change` / `on_commit`, fired with `{ x, y }` on
    /// `Event::payload`.
    pub event_listeners: HashMap<String, Vec<Box<dyn Fn(&Event)>>>,
    pub hovered: bool,
    pub layout: Option<Rect>,
    grab: Option<Grab>,
    /// The last press, for telling a double-click.
    last_press: Option<(Instant, f32, f32)>,
}

impl PadWidget {
    pub fn new() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            x_min: -1.0,
            x_max: 1.0,
            y_min: -1.0,
            y_max: 1.0,
            shape: PadShape::Rect,
            rest: None,
            style: FlexStyle::default(),
            dot_color: Color::new(245, 245, 245, 255),
            ring_color: Color::new(20, 20, 24, 255),
            event_listeners: HashMap::new(),
            hovered: false,
            layout: None,
            grab: None,
            last_press: None,
        }
    }

    /// The plane in parent-relative space: `(left, top, width, height)`.
    /// For a disc, the square holding the inscribed circle.
    fn plane(&self) -> Option<(f32, f32, f32, f32)> {
        let layout = self.layout.as_ref()?;
        let s = &self.style;
        let x = layout.x + s.margin.get_left() + s.padding.get_left();
        let y = layout.y + s.margin.get_top() + s.padding.get_top();
        let w = (layout.width - s.horizontal_inset()).max(0.0);
        let h = (layout.height - s.vertical_inset()).max(0.0);
        Some(match self.shape {
            PadShape::Rect => (x, y, w, h),
            PadShape::Disc => {
                let side = w.min(h);
                (x + (w - side) / 2.0, y + (h - side) / 2.0, side, side)
            }
        })
    }

    /// `(x, y)` as the plane's unit coordinates: `-1..1` on each axis,
    /// `+v` up.
    fn to_unit(&self, (x, y): (f32, f32)) -> (f32, f32) {
        let unit = |v: f32, lo: f32, hi: f32| {
            let span = hi - lo;
            if span.abs() <= f32::EPSILON {
                0.0
            } else {
                (v - lo) / span * 2.0 - 1.0
            }
        };
        (unit(x, self.x_min, self.x_max), unit(y, self.y_min, self.y_max))
    }

    fn from_unit(&self, (u, v): (f32, f32)) -> (f32, f32) {
        let value = |t: f32, lo: f32, hi: f32| lo + (t + 1.0) / 2.0 * (hi - lo);
        (value(u, self.x_min, self.x_max), value(v, self.y_min, self.y_max))
    }

    /// `(x, y)` held inside the shape.
    pub fn clamp(&self, value: (f32, f32)) -> (f32, f32) {
        let (u, v) = self.to_unit(value);
        let (u, v) = match self.shape {
            PadShape::Rect => (u.clamp(-1.0, 1.0), v.clamp(-1.0, 1.0)),
            PadShape::Disc => {
                let len = (u * u + v * v).sqrt();
                if len > 1.0 {
                    (u / len, v / len)
                } else {
                    (u, v)
                }
            }
        };
        self.from_unit((u, v))
    }

    /// The value a pointer at parent-relative `(px, py)` asks for.
    pub fn value_at(&self, px: f32, py: f32) -> (f32, f32) {
        let Some((x, y, w, h)) = self.plane() else {
            return (self.x, self.y);
        };
        let u = if w > 0.0 { (px - x) / w * 2.0 - 1.0 } else { 0.0 };
        let v = if h > 0.0 { 1.0 - (py - y) / h * 2.0 } else { 0.0 };
        self.clamp(self.from_unit((u, v)))
    }

    /// The value a drag from `grab` asks for with the pointer at
    /// `(px, py)`: the grab's value moved by the cursor's travel, at a
    /// tenth of it when fine. Until the first rebase that is exactly the
    /// value under the cursor, since a plain press puts it there.
    fn drag_value_at(&self, grab: &Grab, px: f32, py: f32) -> (f32, f32) {
        let Some((_, _, w, h)) = self.plane() else {
            return (self.x, self.y);
        };
        let gain = if grab.fine { FINE } else { 1.0 };
        let dx = if w > 0.0 { (px - grab.pointer.0) / w * (self.x_max - self.x_min) } else { 0.0 };
        let dy = if h > 0.0 { -(py - grab.pointer.1) / h * (self.y_max - self.y_min) } else { 0.0 };
        self.clamp((grab.value.0 + dx * gain, grab.value.1 + dy * gain))
    }

    /// The dot's centre in parent-relative space.
    fn dot_at(&self) -> Option<(f32, f32)> {
        let (x, y, w, h) = self.plane()?;
        let (u, v) = self.to_unit(self.clamp((self.x, self.y)));
        Some((x + (u + 1.0) / 2.0 * w, y + (1.0 - v) / 2.0 * h))
    }

    fn fire(&self, name: &str) {
        if let Some(listeners) = self.event_listeners.get(name) {
            let mut event = Event::new(name.to_string());
            event.payload = Some(Value::Map(std::rc::Rc::new(HashMap::from([
                ("x".to_string(), Value::Float(self.x as f64)),
                ("y".to_string(), Value::Float(self.y as f64)),
            ]))));
            for listener in listeners {
                listener(&event);
            }
        }
    }

    /// Move to `next`, reporting `on_change` if it differs.
    fn set_value(&mut self, next: (f32, f32)) -> bool {
        if (next.0 - self.x).abs() <= f32::EPSILON && (next.1 - self.y).abs() <= f32::EPSILON {
            return false;
        }
        self.x = next.0;
        self.y = next.1;
        self.fire("on_change");
        true
    }

    /// Where a drag with the pointer at `(px, py)` puts the value, with
    /// Ctrl held or not — rebasing the drag if that changed.
    fn drag_to(&mut self, px: f32, py: f32, fine: bool) {
        let Some(mut grab) = self.grab else { return };
        if grab.fine != fine {
            grab = Grab { pointer: (px, py), value: (self.x, self.y), fine };
            self.grab = Some(grab);
        }
        let next = self.drag_value_at(&grab, px, py);
        self.set_value(next);
    }

    /// Whether a press at `(px, py)` now is the second of a double-click,
    /// recording it as the first of the next one otherwise.
    fn second_press(&mut self, px: f32, py: f32) -> bool {
        let now = Instant::now();
        let double = self.last_press.is_some_and(|(at, x, y)| {
            now.duration_since(at) <= DOUBLE_CLICK
                && (px - x).abs() <= DOUBLE_CLICK_SLOP
                && (py - y).abs() <= DOUBLE_CLICK_SLOP
        });
        self.last_press = if double { None } else { Some((now, px, py)) };
        double
    }

    fn has_listener(&self, name: &str) -> bool {
        self.event_listeners.get(name).is_some_and(|l| !l.is_empty())
    }
}

impl Default for PadWidget {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for PadWidget {
    fn get_type(&self) -> &str {
        "pad"
    }

    fn is_focusable(&self) -> bool {
        true
    }

    fn update(&mut self, new_widget: WidgetRef) -> UpdateResult {
        let mut new_widget = new_widget.lock().expect("widget lock poisoned");
        let Some(new) = new_widget.downcast_mut::<PadWidget>() else {
            return UpdateResult::replace();
        };
        let layout_changed = !self.style.layout_equal(&new.style);
        let paint_changed = !self.style.paint_equal(&new.style)
            || self.x != new.x
            || self.y != new.y
            || self.x_min != new.x_min
            || self.x_max != new.x_max
            || self.y_min != new.y_min
            || self.y_max != new.y_max
            || self.shape != new.shape
            || self.dot_color != new.dot_color
            || self.ring_color != new.ring_color;
        // Controlled: the document's value is the value, drag or no drag.
        // The drag and the last press are the widget's own and survive.
        self.x = new.x;
        self.y = new.y;
        self.x_min = new.x_min;
        self.x_max = new.x_max;
        self.y_min = new.y_min;
        self.y_max = new.y_max;
        self.shape = new.shape;
        self.rest = new.rest;
        self.style = new.style.clone();
        self.dot_color = new.dot_color;
        self.ring_color = new.ring_color;
        std::mem::swap(&mut self.event_listeners, &mut new.event_listeners);
        UpdateResult {
            absorbed: true,
            needs_boundary_layout: false,
            needs_layout: layout_changed,
            needs_repaint: layout_changed || paint_changed,
            cancelled_unmount_prefixes: Vec::new(),
            drained_path_prefixes: Vec::new(),
        }
    }

    fn get_dimensions(
        &self,
        ctx: &LayoutContext,
        parent_direction: &Direction,
        parent_width: f32,
        parent_available_width: f32,
        parent_height: f32,
        parent_available_height: f32,
        sibling_basis: f32,
    ) -> (f32, f32) {
        // `SliderWidget`'s resolution, with a square intrinsic size.
        let width = match ctx.effective_width(self.style.width) {
            Size::Fixed(w) => w,
            Size::Shrink => {
                let want = DEFAULT_SIDE + self.style.horizontal_inset();
                let max_width =
                    if parent_direction.is_row() { parent_available_width } else { parent_width };
                if max_width > 0.0 { want.min(max_width) } else { want }
            }
            Size::Grow(basis) => {
                if parent_direction.is_row() {
                    parent_direction.get_grow_size(basis, sibling_basis, parent_available_width)
                } else {
                    parent_width
                }
            }
            Size::Percent(_) => 0.0,
        };
        let height = match ctx.effective_height(self.style.height) {
            Size::Fixed(h) => h,
            Size::Shrink => {
                let want = DEFAULT_SIDE + self.style.vertical_inset();
                let max_height =
                    if parent_direction.is_row() { parent_height } else { parent_available_height };
                if max_height > 0.0 { want.min(max_height) } else { want }
            }
            Size::Grow(basis) => {
                if parent_direction.is_row() {
                    parent_height
                } else {
                    parent_direction.get_grow_size(basis, sibling_basis, parent_available_height)
                }
            }
            Size::Percent(_) => 0.0,
        };
        (width, height)
    }

    fn get_children(&self) -> Vec<WidgetRef> {
        Vec::new()
    }

    fn get_basis(&self, direction: &Direction) -> f32 {
        if matches!(self.style.position, Position::Absolute(_, _)) {
            return 0.0;
        }
        if direction.is_row() {
            self.style.width.grow_basis()
        } else {
            self.style.height.grow_basis()
        }
    }

    fn get_children_basis(&self) -> f32 {
        0.0
    }

    fn get_fixed_width(&self) -> Option<f32> {
        self.style.width.as_fixed()
    }

    fn get_fixed_height(&self) -> Option<f32> {
        self.style.height.as_fixed()
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut EventContext, self_ref: &WidgetRef) -> bool {
        if let Some(point) = &event.point {
            let captured = ctx.is_captured(self_ref);
            let (px, py) = (point.x(), point.y());
            let fine = event.modifiers.ctrl;
            return match event.name.as_str() {
                "mouse_down" if captured || self.contains_point(point) => {
                    ctx.request_focus(self_ref.clone());
                    ctx.listener_fired = true;
                    if let Some(rest) = self.rest.filter(|_| self.second_press(px, py)) {
                        self.grab = None;
                        self.set_value(self.clamp(rest));
                        self.fire("on_commit");
                        return true;
                    }
                    ctx.request_capture(self_ref.clone(), point.clone());
                    if !fine {
                        let next = self.value_at(px, py);
                        self.set_value(next);
                    }
                    self.grab = Some(Grab { pointer: (px, py), value: (self.x, self.y), fine });
                    true
                }
                "mouse_move" if captured => {
                    self.drag_to(px, py, fine);
                    true
                }
                "mouse_up" if captured => {
                    self.drag_to(px, py, fine);
                    self.grab = None;
                    self.fire("on_commit");
                    true
                }
                _ => false,
            };
        }
        // Keyboard, only when focused: a hundredth of an axis per arrow,
        // committed at once.
        if event.name == "keydown" && ctx.is_focused(self_ref) {
            if let Some(code) = event.keyboard_data.as_ref().and_then(|k| k.key_code) {
                let dx = (self.x_max - self.x_min) / 100.0;
                let dy = (self.y_max - self.y_min) / 100.0;
                let (sx, sy) = match code {
                    37 => (-dx, 0.0),
                    39 => (dx, 0.0),
                    38 => (0.0, dy),
                    40 => (0.0, -dy),
                    _ => return false,
                };
                let next = self.clamp((self.x + sx, self.y + sy));
                if self.set_value(next) {
                    self.fire("on_commit");
                }
                return true;
            }
        }
        false
    }

    fn layout(
        &mut self,
        ctx: &LayoutContext,
        cursor_x: f32,
        cursor_y: f32,
        parent_direction: &Direction,
        parent_width: f32,
        parent_available_width: f32,
        parent_height: f32,
        parent_available_height: f32,
        sibling_basis: f32,
    ) {
        let (width, height) = self.get_dimensions(
            ctx,
            parent_direction,
            parent_width,
            parent_available_width,
            parent_height,
            parent_available_height,
            sibling_basis,
        );
        self.layout = Some(Rect::new(cursor_x, cursor_y, width, height));
    }

    fn contains_point(&self, point: &Point) -> bool {
        let Some((x, y, w, h)) = self.plane() else {
            return false;
        };
        match self.shape {
            // Margin-aware, like `FlexWidget::contains_point`: the padding
            // is the pad's, so a press there is a press at the edge.
            PadShape::Rect => {
                let Some(layout) = self.layout.as_ref() else { return false };
                let m = &self.style.margin;
                let (bx, by) = (layout.x + m.get_left(), layout.y + m.get_top());
                let bw = layout.width - m.get_left() - m.get_right();
                let bh = layout.height - m.get_top() - m.get_bottom();
                point.x() >= bx && point.x() <= bx + bw && point.y() >= by && point.y() <= by + bh
            }
            // The circle, and the dot where it overhangs the rim.
            PadShape::Disc => {
                let r = w.min(h) / 2.0 + DOT_RADIUS;
                let (dx, dy) = (point.x() - (x + w / 2.0), point.y() - (y + h / 2.0));
                dx * dx + dy * dy <= r * r
            }
        }
    }

    fn blocks_point(&self, point: &Point) -> bool {
        self.contains_point(point)
    }

    fn blocks_interactions(&self) -> bool {
        true
    }

    fn declared_cursor(&self) -> CursorRole {
        self.style.cursor
    }

    fn set_hovered(&mut self, hovered: bool) {
        self.hovered = hovered;
    }

    fn is_hovered(&self) -> bool {
        self.hovered
    }

    fn fire_listeners(&self, event_name: &str, event: &Event) {
        if let Some(listeners) = self.event_listeners.get(event_name) {
            for listener in listeners {
                listener(event);
            }
        }
    }

    fn fire_event_listener(&self, event: &Event) -> bool {
        if self.has_listener(&event.name) {
            self.fire_listeners(&event.name, event);
            true
        } else {
            false
        }
    }

    fn is_absolute_positioned(&self) -> bool {
        matches!(self.style.position, Position::Absolute(_, _))
    }

    fn get_absolute_offset(&self) -> Option<(f32, f32)> {
        match self.style.position {
            Position::Absolute(x, y) => Some((x, y)),
            _ => None,
        }
    }

    fn get_layout_rect(&self) -> Option<&Rect> {
        self.layout.as_ref()
    }

    fn render(&self, ctx: &mut dyn RenderContext, _focused: bool, _image_cache: &mut ImageCache) {
        let Some(layout) = self.layout.as_ref() else {
            return;
        };
        let s = &self.style;
        let box_x = layout.x + s.margin.get_left();
        let box_y = layout.y + s.margin.get_top();
        let box_w = layout.width - s.margin.get_left() - s.margin.get_right();
        let box_h = layout.height - s.margin.get_top() - s.margin.get_bottom();
        if let Some(bg) = s.background_color {
            if s.corners.is_all_sharp() {
                ctx.fill_rect(box_x, box_y, box_w, box_h, &bg);
            } else {
                ctx.fill_corners_rect(box_x, box_y, box_w, box_h, &s.corners, &bg);
            }
        }
        ctx.draw_border(&s.border, box_x, box_y, box_w, box_h, &s.corners);
        let Some((cx, cy)) = self.dot_at() else {
            return;
        };
        // A ring under the dot, so it reads over any picture behind it.
        let outer = DOT_RADIUS + RING_WIDTH;
        ctx.fill_corners_rect(
            cx - outer,
            cy - outer,
            outer * 2.0,
            outer * 2.0,
            &Corners::all_round(outer),
            &self.ring_color,
        );
        ctx.fill_corners_rect(
            cx - DOT_RADIUS,
            cy - DOT_RADIUS,
            DOT_RADIUS * 2.0,
            DOT_RADIUS * 2.0,
            &Corners::all_round(DOT_RADIUS),
            &self.dot_color,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pad(shape: PadShape) -> PadWidget {
        let mut p = PadWidget::new();
        p.shape = shape;
        p.layout = Some(Rect::new(0.0, 0.0, 200.0, 100.0));
        p
    }

    #[test]
    fn y_runs_bottom_to_top() {
        let p = pad(PadShape::Rect);
        assert_eq!(p.value_at(0.0, 0.0), (-1.0, 1.0));
        assert_eq!(p.value_at(200.0, 100.0), (1.0, -1.0));
        assert_eq!(p.value_at(100.0, 50.0), (0.0, 0.0));
    }

    #[test]
    fn a_disc_is_the_inscribed_circle_and_holds_the_value_inside_it() {
        let p = pad(PadShape::Disc);
        // A 100-wide square centred in the 200 × 100 box: x from 50 to 150.
        assert_eq!(p.value_at(150.0, 50.0), (1.0, 0.0));
        let (x, y) = p.value_at(150.0, 0.0);
        assert!((x * x + y * y - 1.0).abs() < 1e-5, "held on the rim: {x}, {y}");
        assert!(!p.contains_point(&Point::new(10.0, 50.0)), "the box outside the circle");
        assert!(p.contains_point(&Point::new(100.0, 50.0)));
    }

    #[test]
    fn a_fine_drag_moves_a_tenth_as_far() {
        let mut p = pad(PadShape::Rect);
        p.grab = Some(Grab { pointer: (100.0, 50.0), value: (0.0, 0.0), fine: true });
        p.drag_to(150.0, 50.0, true);
        assert!((p.x - 0.05).abs() < 1e-6, "{}", p.x);
        // Letting go of Ctrl rebases where the dot stands: no jump back to
        // the cursor, and the next move travels at the cursor's speed.
        p.drag_to(150.0, 50.0, false);
        assert!((p.x - 0.05).abs() < 1e-6, "{}", p.x);
        p.drag_to(160.0, 50.0, false);
        assert!((p.x - 0.15).abs() < 1e-6, "{}", p.x);
    }
}
