//! The browser stage: one Masque document drawn into a CPU raster surface
//! whose pixels a page copies into a `<canvas>`.
//!
//! [`Stage`] is the whole host, written against plain Rust types so it
//! runs and is tested natively. `main.rs` wraps it in the `extern "C"`
//! functions the emscripten build exports. The frame mirrors the native
//! hosts' (`AGENTS.md`, *Rendering pipeline*): update, tick, lay out, draw
//! on damage. Design record: `docs/internal/WEB_STAGE.md`.

use std::sync::{Arc, Mutex};

use masque::runtime::config::RuntimeConfig;
use masque::runtime::value::Value;
use masque::skia::SkiaEnv;
use masque::skia_safe::{surfaces, AlphaType, Color, ColorType, ImageInfo};
use masque::widget::event::{Event, KeyModifiers};
use masque::widget::point::Point;
use masque::widget::style::CursorRole;
use masque::widget::Surface;
use masque::Masque;

/// The family the stage's text draws in unless a document names another.
pub const FONT_FAMILY: &str = "Geist";
const FONTS: &[&[u8]] = &[
    include_bytes!("../fonts/Geist-400.ttf"),
    include_bytes!("../fonts/Geist-700.ttf"),
];

/// [`Stage::frame`] drew new pixels.
pub const FRAME_REPAINTED: u32 = 1;
/// The pointer is over something clickable.
pub const FRAME_CURSOR_POINTER: u32 = 2;

/// A pointer event, as the page reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pointer {
    Move,
    Down,
    Up,
    /// The pointer left the canvas: a move to nowhere, so every hovered
    /// widget hears `mouse_leave`.
    Leave,
}

/// A key event, as the page reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Down,
    Up,
}

/// Modifier bits shared with the page: ctrl 1, alt 2, shift 4, meta 8.
pub fn modifiers(bits: u32) -> KeyModifiers {
    KeyModifiers {
        ctrl: bits & 1 != 0,
        alt: bits & 2 != 0,
        shift: bits & 4 != 0,
        meta: bits & 8 != 0,
    }
}

pub struct Stage {
    masque: Option<Masque>,
    env: SkiaEnv,
    /// Logical size and device pixel ratio.
    width: f32,
    height: f32,
    dpr: f32,
    /// The last frame, RGBA8 unpremultiplied, `width·dpr × height·dpr`:
    /// what `putImageData` takes.
    pixels: Vec<u8>,
    /// Set by anything that changes what is drawn outside the tree's own
    /// damage flags: a new document, a resize.
    dirty: bool,
    event_names: Vec<String>,
    /// Raises since the page last took them, each a JSON array
    /// `[name, ...args]`. Handlers must be `Send + Sync` and a `Value`
    /// holds `Rc`s, so they are serialised where they are raised.
    events: Arc<Mutex<Vec<String>>>,
    error: Option<String>,
}

impl Stage {
    /// A stage that answers `event(name, …)` for each of `event_names`.
    /// It draws nothing until [`Self::set_source`] succeeds.
    pub fn new(event_names: Vec<String>) -> Self {
        let mut stage = Self {
            masque: None,
            env: SkiaEnv::new(raster(1, 1)),
            width: 1.0,
            height: 1.0,
            dpr: 1.0,
            pixels: Vec::new(),
            dirty: true,
            event_names,
            events: Arc::new(Mutex::new(Vec::new())),
            error: None,
        };
        stage.resize(1.0, 1.0, 1.0);
        stage
    }

    /// Build the document from `source`. On an error the previous
    /// document, if any, stays up, and the error is kept for
    /// [`Self::error`]. Every rebuild starts `state` afresh.
    pub fn set_source(&mut self, source: &str) -> Result<(), String> {
        let mut config = RuntimeConfig::new().with_strict_vocabulary();
        for name in &self.event_names {
            let events = self.events.clone();
            let raised = name.clone();
            config = config.with_event_handler(name.clone(), move |args| {
                let mut json = String::from("[");
                push_json_string(&mut json, &raised);
                for arg in args {
                    json.push(',');
                    push_json(&mut json, arg);
                }
                json.push(']');
                events.lock().expect("event queue lock poisoned").push(json);
                Ok(Value::Void)
            });
        }
        match Masque::from_source(source, config) {
            Ok(mut masque) => {
                masque.register_font_data(FONT_FAMILY, FONTS);
                masque.set_default_font(FONT_FAMILY);
                self.masque = Some(masque);
                self.error = None;
                self.dirty = true;
                Ok(())
            }
            Err(e) => {
                let message = e.to_string();
                self.error = Some(message.clone());
                Err(message)
            }
        }
    }

    /// The last [`Self::set_source`] or [`Self::frame`] error, until a
    /// build succeeds.
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// What the strict vocabulary found in the current document: keys
    /// and values the builder ignores, one per line. Not errors; the
    /// document draws regardless.
    pub fn notes(&self) -> String {
        let Some(masque) = &self.masque else {
            return String::new();
        };
        masque
            .vocabulary_violations()
            .iter()
            .map(|v| v.to_string())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// The canvas's logical size and device pixel ratio.
    pub fn resize(&mut self, width: f32, height: f32, dpr: f32) {
        self.width = width.max(1.0);
        self.height = height.max(1.0);
        self.dpr = if dpr > 0.0 { dpr } else { 1.0 };
        let (w, h) = self.pixel_size();
        self.env.surface = raster(w, h);
        self.env.set_dpi_scale(self.dpr);
        self.pixels = vec![0; (w * h * 4) as usize];
        self.dirty = true;
    }

    /// The pixel buffer's size in device pixels.
    pub fn pixel_size(&self) -> (i32, i32) {
        (
            (self.width * self.dpr).round().max(1.0) as i32,
            (self.height * self.dpr).round().max(1.0) as i32,
        )
    }

    pub fn pointer(&mut self, kind: Pointer, x: f32, y: f32, mods: KeyModifiers) {
        let name = match kind {
            Pointer::Move => "mouse_move",
            Pointer::Down => "mouse_down",
            Pointer::Up => "mouse_up",
            Pointer::Leave => "mouse_move",
        };
        let point = match kind {
            Pointer::Leave => Point::new(-1.0e6, -1.0e6),
            _ => Point::new(x, y),
        };
        self.call(&Event::with_point(name.to_string(), point).with_modifiers(mods));
    }

    /// True when the document took the wheel (a scroll container moved);
    /// otherwise the page should scroll instead.
    pub fn wheel(&mut self, x: f32, y: f32, dx: f32, dy: f32) -> bool {
        self.call(&Event::scroll(Point::new(x, y), dx, dy))
    }

    /// `key_code` is the DOM's `keyCode`, which is the code
    /// `widget::keys` resolves chords from. `character` is the DOM's
    /// `key` when it is one character. The sequence is the native
    /// hosts': keydown, then keypress for a character (or Space).
    pub fn key(&mut self, kind: Key, key_code: u32, character: Option<char>, mods: KeyModifiers) {
        match kind {
            Key::Down => {
                self.call(&Event::keydown(key_code, None, mods.clone()));
                let character = character.or((key_code == 32).then_some(' '));
                if let Some(c) = character {
                    self.call(&Event::keypress(key_code, Some(c), mods));
                }
            }
            Key::Up => {
                self.call(&Event::keyup(key_code, character, mods));
            }
        }
    }

    fn call(&mut self, event: &Event) -> bool {
        match &mut self.masque {
            Some(masque) => masque.get_ui_mut().call_event(event),
            None => false,
        }
    }

    /// Advance `dt` seconds and draw if anything changed. Returns
    /// `FRAME_*` bits. A frame without [`FRAME_REPAINTED`] means the
    /// page can stop its animation loop until the next input.
    pub fn frame(&mut self, dt: f32) -> Result<u32, String> {
        let Some(masque) = &mut self.masque else {
            return Ok(0);
        };
        let (width, height) = (self.width, self.height);
        masque.set_screen_size(width, height);
        if let Err(e) = masque.update() {
            let message = e.to_string();
            self.error = Some(message.clone());
            return Err(message);
        }
        let ui = masque.get_ui_mut();
        ui.tick_animations(dt.min(1.0 / 30.0));
        // Read the damage before `layout`, which clears the repaint flag
        // when it runs a full pass.
        let damaged = self.dirty || ui.needs_repaint() || ui.needs_layout();
        ui.layout(width, height);

        let mut bits = 0;
        if ui.hovered_cursor() == CursorRole::Pointer {
            bits |= FRAME_CURSOR_POINTER;
        }
        if damaged {
            self.env.surface.canvas().clear(Color::TRANSPARENT);
            self.env.draw(ui);
            ui.clear_needs_repaint();
            let (w, h) = self.pixel_size();
            let info = ImageInfo::new((w, h), ColorType::RGBA8888, AlphaType::Unpremul, None);
            let read =
                self.env
                    .surface
                    .read_pixels(&info, &mut self.pixels, (w * 4) as usize, (0, 0));
            debug_assert!(read, "raster read_pixels failed");
            self.dirty = false;
            bits |= FRAME_REPAINTED;
        }
        Ok(bits)
    }

    /// The last frame's pixels; see [`Self::pixel_size`].
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// The raises since the last call, as a JSON array of arrays.
    pub fn take_events(&mut self) -> String {
        let mut events = self.events.lock().expect("event queue lock poisoned");
        let json = format!("[{}]", events.join(","));
        events.clear();
        json
    }

    /// The live document, for tests.
    pub fn masque(&self) -> Option<&Masque> {
        self.masque.as_ref()
    }
}

fn raster(w: i32, h: i32) -> masque::skia_safe::Surface {
    surfaces::raster_n32_premul((w, h)).expect("raster surface")
}

fn push_json(out: &mut String, value: &Value) {
    match value {
        Value::Integer(i) => out.push_str(&i.to_string()),
        Value::Float(f) if f.is_finite() => out.push_str(&f.to_string()),
        Value::Boolean(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::String(s) => push_json_string(out, s),
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                push_json(out, item);
            }
            out.push(']');
        }
        Value::Map(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            out.push('{');
            for (i, key) in keys.into_iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                push_json_string(out, key);
                out.push(':');
                push_json(out, &map[key]);
            }
            out.push('}');
        }
        // Closures, widgets, handles and non-finite floats have no JSON
        // form; the log shows where they were.
        _ => out.push_str("null"),
    }
}

fn push_json_string(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;

    const HERO: &str = include_str!("../tests/hero.masque");

    fn hero() -> Stage {
        let mut stage = Stage::new(vec!["clicked".to_string()]);
        stage.resize(600.0, 400.0, 2.0);
        stage.set_source(HERO).expect("hero builds");
        stage.frame(0.016).expect("first frame");
        stage
    }

    fn click(stage: &mut Stage, x: f32, y: f32) {
        let none = KeyModifiers::default();
        stage.pointer(Pointer::Move, x, y, none.clone());
        stage.pointer(Pointer::Down, x, y, none.clone());
        stage.pointer(Pointer::Up, x, y, none);
        stage.frame(0.016).expect("frame");
    }

    /// The pixel at logical `(x, y)`.
    fn pixel(stage: &Stage, x: f32, y: f32) -> [u8; 4] {
        let (w, _) = stage.pixel_size();
        let (px, py) = ((x * stage.dpr) as i32, (y * stage.dpr) as i32);
        let i = ((py * w + px) * 4) as usize;
        stage.pixels()[i..i + 4].try_into().unwrap()
    }

    /// Sum of channel differences over a logical rect, between two frames.
    fn changed(before: &[u8], after: &[u8], stage: &Stage, x: f32, y: f32, w: f32, h: f32) -> u64 {
        let (pw, _) = stage.pixel_size();
        let mut sum = 0u64;
        for py in (y * stage.dpr) as i32..((y + h) * stage.dpr) as i32 {
            for px in (x * stage.dpr) as i32..((x + w) * stage.dpr) as i32 {
                let i = ((py * pw + px) * 4) as usize;
                for c in 0..4 {
                    sum += (before[i + c] as i64 - after[i + c] as i64).unsigned_abs();
                }
            }
        }
        sum
    }

    #[test]
    fn the_hero_draws_its_stage_and_buttons() {
        let stage = hero();
        // The golden the wasm build is compared against (`web/test.sh`).
        if let Ok(path) = std::env::var("MASQUE_STAGE_GOLDEN") {
            std::fs::write(path, stage.pixels()).expect("golden written");
        }
        assert_eq!(stage.pixel_size(), (1200, 800));
        // The stage background, in a corner.
        assert_eq!(pixel(&stage, 4.0, 4.0), [30, 30, 30, 255]);
        // The middle button's face: paper over the stage, not the stage.
        assert_ne!(pixel(&stage, 200.0, 200.0), [30, 30, 30, 255]);
        assert!(stage.notes().is_empty(), "{}", stage.notes());
    }

    #[test]
    fn a_click_counts_on_its_own_button_only() {
        let mut stage = hero();
        let before = stage.pixels().to_vec();
        // The three 44px buttons are centred with 12px gaps: 400/2 ± 56.
        click(&mut stage, 300.0, 144.0);
        click(&mut stage, 300.0, 144.0);
        let after = stage.pixels().to_vec();
        assert!(changed(&before, &after, &stage, 190.0, 122.0, 220.0, 44.0) > 0);
        assert_eq!(
            changed(&before, &after, &stage, 190.0, 178.0, 220.0, 44.0),
            0
        );
        assert_eq!(
            changed(&before, &after, &stage, 190.0, 234.0, 220.0, 44.0),
            0
        );
    }

    #[test]
    fn an_idle_frame_draws_nothing() {
        let mut stage = hero();
        assert_eq!(stage.frame(0.016).unwrap() & FRAME_REPAINTED, 0);
        stage.resize(500.0, 400.0, 1.0);
        assert_ne!(stage.frame(0.016).unwrap() & FRAME_REPAINTED, 0);
    }

    #[test]
    fn a_bad_edit_keeps_the_last_good_document() {
        let mut stage = hero();
        let before = stage.pixels().to_vec();
        assert!(stage.set_source("let main = fn () { Flex {").is_err());
        assert!(stage.error().is_some());
        assert!(stage.masque().is_some());
        stage.frame(0.016).unwrap();
        assert_eq!(stage.pixels(), &before[..]);
        stage.set_source(HERO).unwrap();
        assert!(stage.error().is_none());
    }

    #[test]
    fn hovering_a_button_asks_for_the_pointer_cursor() {
        let mut stage = Stage::new(vec![]);
        stage.resize(200.0, 100.0, 1.0);
        stage
            .set_source(
                r#"let main = fn () {
                    Flex {
                        style: { width: "grow", height: "grow", cursor: "pointer" },
                        mouse_down: fn () { },
                    }
                };"#,
            )
            .unwrap();
        stage.frame(0.016).unwrap();
        stage.pointer(Pointer::Move, 50.0, 50.0, KeyModifiers::default());
        assert_ne!(stage.frame(0.016).unwrap() & FRAME_CURSOR_POINTER, 0);
        stage.pointer(Pointer::Leave, 0.0, 0.0, KeyModifiers::default());
        assert_eq!(stage.frame(0.016).unwrap() & FRAME_CURSOR_POINTER, 0);
    }

    #[test]
    fn raised_events_come_out_as_json() {
        let mut stage = Stage::new(vec!["picked".to_string()]);
        stage.resize(200.0, 100.0, 1.0);
        stage
            .set_source(
                r#"let main = fn () {
                    Flex {
                        style: { width: "grow", height: "grow" },
                        mouse_down: fn () { event("picked", 7, "a\"b", { k: true }); },
                    }
                };"#,
            )
            .unwrap();
        stage.frame(0.016).unwrap();
        stage.pointer(Pointer::Down, 50.0, 50.0, KeyModifiers::default());
        assert_eq!(stage.take_events(), r#"[["picked",7,"a\"b",{"k":true}]]"#);
        assert_eq!(stage.take_events(), "[]");
    }
}
