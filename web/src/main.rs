//! The stage's `extern "C"` surface, for emscripten's `cwrap`.
//!
//! One stage per module instance. Strings cross as pointer + length in
//! the wasm heap: the page `_malloc`s and writes UTF-8 going in, and reads
//! `stage_out_ptr()` for the length a call returned coming out. The
//! buffer behind `stage_out_ptr` and `stage_pixels_ptr` is valid until the
//! next call into the stage.
//!
//! `wasm-bindgen` does not support the emscripten target, which is why
//! the glue is by hand (`docs/internal/WEB_STAGE.md` §1).

use std::cell::RefCell;

use ogham_web::{modifiers, Key, Pointer, Stage};

thread_local! {
    static STAGE: RefCell<Option<Stage>> = const { RefCell::new(None) };
    static OUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

fn with_stage<R>(f: impl FnOnce(&mut Stage) -> R) -> Option<R> {
    STAGE.with(|s| s.borrow_mut().as_mut().map(f))
}

/// Put `text` in the out buffer and return its length.
fn out(text: &str) -> u32 {
    OUT.with(|o| {
        let mut o = o.borrow_mut();
        o.clear();
        o.extend_from_slice(text.as_bytes());
        o.len() as u32
    })
}

/// # Safety
/// `ptr` must point at `len` readable bytes, or `len` must be 0.
unsafe fn text<'a>(ptr: *const u8, len: u32) -> &'a str {
    if len == 0 {
        return "";
    }
    std::str::from_utf8(std::slice::from_raw_parts(ptr, len as usize)).unwrap_or("")
}

/// Create the stage. `names` is the event names the document may raise,
/// newline-separated. Replaces any stage already made.
///
/// # Safety
/// `names` must point at `len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn stage_new(names: *const u8, len: u32) {
    let names = text(names, len)
        .lines()
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .map(String::from)
        .collect();
    STAGE.with(|s| *s.borrow_mut() = Some(Stage::new(names)));
}

/// Build the document. 0 on success; 1 on an error, whose text
/// `stage_error` returns, with the previous document left up.
///
/// # Safety
/// `src` must point at `len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn stage_set_source(src: *const u8, len: u32) -> u32 {
    let source = text(src, len);
    with_stage(|s| s.set_source(source).is_err() as u32).unwrap_or(1)
}

/// The last build or frame error into the out buffer; 0 when there is none.
#[no_mangle]
pub extern "C" fn stage_error() -> u32 {
    with_stage(|s| s.error().map(out).unwrap_or(0)).unwrap_or(0)
}

/// The strict vocabulary's findings into the out buffer, one per line.
#[no_mangle]
pub extern "C" fn stage_notes() -> u32 {
    with_stage(|s| out(&s.notes())).unwrap_or(0)
}

/// Raises since the last call into the out buffer, as JSON.
#[no_mangle]
pub extern "C" fn stage_take_events() -> u32 {
    with_stage(|s| out(&s.take_events())).unwrap_or(0)
}

#[no_mangle]
pub extern "C" fn stage_out_ptr() -> *const u8 {
    OUT.with(|o| o.borrow().as_ptr())
}

#[no_mangle]
pub extern "C" fn stage_resize(width: f32, height: f32, dpr: f32) {
    with_stage(|s| s.resize(width, height, dpr));
}

/// `kind`: 0 move, 1 down, 2 up, 3 leave. `mods`: ctrl 1, alt 2,
/// shift 4, meta 8.
#[no_mangle]
pub extern "C" fn stage_pointer(kind: u32, x: f32, y: f32, mods: u32) {
    let kind = match kind {
        1 => Pointer::Down,
        2 => Pointer::Up,
        3 => Pointer::Leave,
        _ => Pointer::Move,
    };
    with_stage(|s| s.pointer(kind, x, y, modifiers(mods)));
}

#[no_mangle]
pub extern "C" fn stage_wheel(x: f32, y: f32, dx: f32, dy: f32) {
    with_stage(|s| s.wheel(x, y, dx, dy));
}

/// `kind`: 0 down, 1 up. `character`: a Unicode scalar, 0 for none.
#[no_mangle]
pub extern "C" fn stage_key(kind: u32, key_code: u32, character: u32, mods: u32) {
    let kind = if kind == 1 { Key::Up } else { Key::Down };
    let character = char::from_u32(character).filter(|c| *c != '\0');
    with_stage(|s| s.key(kind, key_code, character, modifiers(mods)));
}

/// Advance `dt_ms` and draw on damage. Returns the `FRAME_*` bits, or
/// `u32::MAX` on a runtime error (read it with `stage_error`).
#[no_mangle]
pub extern "C" fn stage_frame(dt_ms: f32) -> u32 {
    with_stage(|s| s.frame(dt_ms / 1000.0).unwrap_or(u32::MAX)).unwrap_or(0)
}

#[no_mangle]
pub extern "C" fn stage_pixels_ptr() -> *const u8 {
    with_stage(|s| s.pixels().as_ptr()).unwrap_or(std::ptr::null())
}

#[no_mangle]
pub extern "C" fn stage_pixel_width() -> u32 {
    with_stage(|s| s.pixel_size().0 as u32).unwrap_or(0)
}

#[no_mangle]
pub extern "C" fn stage_pixel_height() -> u32 {
    with_stage(|s| s.pixel_size().1 as u32).unwrap_or(0)
}

fn main() {}
