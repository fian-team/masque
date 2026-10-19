# Web stage: Ogham in a browser

Status: **complete 2026-10-19.** Phases 0–4 merged to main from the `web` worktree (since removed); Phases 5–6 are in `masque-website` (`apps/masque`).

## Progress

| Phase | State | Result |
|---|---|---|
| 0 | done | emsdk **4.0.7** (Skia 0.91's pin, `skia/bin/activate-emsdk`), Rust 1.95, `wasm32-unknown-emscripten`. Clippy baseline on main: 256 diagnostics. `masque-website` git-initialised. |
| 1 | **go** | Skia alone draws text into a PNG under node. Raw 5.4 MB, brotli 1.7 MB (with `pdf`). |
| 2 | done | Target tables; native `cargo tree` identical to main's; `register_font_data`. |
| 3 | done | `cargo build --lib --target wasm32-unknown-emscripten` builds. The fallback font collection needed no change: layout and paint both use the UI's collection once fonts are registered. |
| 4 | done | `web/` host, `build.sh`, `test.sh`. The wasm render of the hero is **pixel-identical** to the native one (0 channels differ). Stage: 6.4 MB, **2.0 MB brotli**. |
| 5 | done | `apps/masque`: `LiveStage` / `LiveDemo` / `stage-module.ts`, `pnpm stage:sync`. Checked in headless Firefox over WebDriver BiDi: draws, counts per button, phone width at 3×, no-JS fallback in the server HTML. 2.5 MB over gzip (`next start`); a brotli host serves 2.0 MB. |
| 6 | done | `SourceEditor`: textarea over Shiki, re-highlighted in the browser by `shiki/core` + the JavaScript engine (tokens identical to the server's on every example), 250 ms rebuild, error line, Reset. |

What Phase 1 found that the plan did not expect:

- **rust-skia's prebuilt emscripten archive is unusable with current
  Rust.** It was compiled with emscripten's JS setjmp/longjmp; Rust's
  emscripten target now links with native wasm exceptions
  (`-fwasm-exceptions`), and emcc refuses the mix (`SUPPORT_LONGJMP=emscripten
  is not compatible with -fwasm-exceptions`). An aborting
  `emscripten_longjmp` stub does not get past it either: the JS-SJLJ
  objects import `invoke_*` wrappers emcc will not generate. Skia is
  built from source with `extra_cflags+=["-fwasm-exceptions",
  "-sSUPPORT_LONGJMP=wasm"]`. With all cores that is about 1½ minutes.
- **bindgen needs emsdk's compat headers** (`xlocale.h`, under
  `system/include/compat`).
- **Skia's dependency sync re-fetches `gn` on every build script run**
  and failed twice on a flaky connection. Once synced, `SKIA_SOURCE_DIR`
  makes the build offline.

All three live in `web/env.sh`.

What Phase 4 found:

- **A runtime bug on main, unrelated to the web.** `VM::call_closure`,
  the path event handlers run through, did not restore the closure's
  captured call-stack path. A handler's `count = count + 1` wrote a
  root-level `:count`, and the next render read the component's own,
  still 0, so every counter stopped at 1. It had been broken since the
  bytecode VM replaced the tree walk (`be0e945`). Fixed on `web` with
  `tests/handler_state.rs`.
- `UI::layout` clears the repaint flag when it runs a full pass, so a
  redraw-on-demand host must read `needs_repaint()` / `needs_layout()`
  *before* `layout`. `Stage::frame` does.
- The size budget (decision 7) is still open. 2.0 MB brotli is the
  number to set it against. Dropping `pdf` changed nothing (the linker
  already discards it); the module is 4.8 MB code and 1.6 MB data, most
  of the data ICU's tables, which SkParagraph needs.

What Phases 5–6 found:

- **A scanner bug on main.** A string literal spanning a newline made
  its token after the newline had moved `last_newline` past the token's
  start, so the column underflowed: an overflow panic in a native debug
  build (the LSP included), column 4294967294 in the stage's error line.
  Fixed in `60389cf`; the scanner takes a token's position when it
  starts.
- The shared theme sets `code { font-size: 0.875em }`, so the text in a
  Shiki `<pre>` is smaller than the pre and the pre's strut sets the
  line pitch. The editor's textarea matches both
  (`masque.css`, *The editable source*).
- Firefox does not dispatch `focus` to a headless window that does not
  have it; the editor also starts its highlighter on the first edit, so
  no path depends on the focus event.

Refreshing the site's module: `web/build.sh && web/test.sh` here, then
`pnpm stage:sync` in `apps/masque`, and commit `public/stage/`.

Goal: the stage on the website's home page (`masque-website`,
`src/components/StageDemo.tsx`) runs the real scanner, compiler, VM,
widget tree, layout and Skia renderer in the browser. The source pane
becomes editable. Today the right pane is `StagePreview.tsx`, an HTML
imitation of three counter buttons.

Naming: the site says Masque and this repository says Ogham. Nothing is
renamed by this plan. New code in this repo uses `ogham`, and the website
keeps calling it Masque.

---

## 0. What is already true

Measured on 2026-10-18, at `756e5f6`.

- **The hero script is real.** The source string in `StageDemo.tsx`
  goes through `Ogham::from_source` with
  `RuntimeConfig::with_strict_vocabulary()`, then `update()` and
  `layout(600, 400)`, with no error and no vocabulary violations. The
  stage can run exactly what it shows.
- **Skia is in layout, not only in paint.** `widget/text_layout.rs`
  measures text with SkParagraph, and Text and TextInput call it for
  wrapping, intrinsic width and caret placement. A browser build must
  therefore carry Skia. It can't replace Skia with Canvas2D.
- **skia-bindings 0.91 has an emscripten platform**
  (`build_support/platform/emscripten.rs`). It requires the `EMSDK`
  environment variable. It embeds FreeType, and it builds Skia's *empty*
  font manager, so there are no system fonts but typefaces can still be
  created from bytes. It has no `wasm32-unknown-unknown` platform.
- **The host hooks mostly exist already:**
  - Fonts: `Ogham::register_font` / `RuntimeConfig::with_font`, which
    feed a `TypefaceFontProvider`, plus `with_default_font`. They read
    file paths.
  - Imports without a filesystem: `RuntimeConfig::with_embedded_sources`.
  - Keys: `Event::keydown(code, char, mods)` takes **JS `keyCode`
    values** (`widget/keys.rs`), which is what a browser sends.
  - Pointer: `Event::with_point("mouse_move" | "mouse_down" |
    "mouse_up", p)`. Scrolling is the `"scroll"` event.
  - Cursor: `UI::hovered_cursor()` returns `CursorRole::{Default,
    Pointer}`.
  - Idle detection: `UI::needs_repaint()`, plus
    `TickResult::still_animating`.
- **Native-only dependencies are unconditional library dependencies:**
  `tokio` (`rt-multi-thread`) and `tower-lsp`, used only by `src/lsp/`;
  `notify`, used only by `file_watcher.rs`; and skia-safe's `vulkan`
  feature, which ogham's own code doesn't use (`SkiaEnv` wraps any
  `skia_safe::Surface`, and the Vulkan surface is lorekeeper's).
- **No threads.** Nothing in `src/` spawns one. `Arc<Mutex<Runtime>>`
  is fine on single-threaded wasm.
- **`std::fs` and `Instant` work under emscripten.** `std::fs` maps to
  emscripten's in-memory filesystem (MEMFS), and `Instant` maps to
  `performance.now()`. `Pad`'s double-click timer and the layout-window
  timer need no changes.
- **Every consumer depends on `path = "../../ogham"`**: lorekeeper's
  app, front, driver, contract, editable-ogham and ogham_preview, plus
  untold_lore's ul-editor. A worktree at another path is invisible to
  all of them, and that is what keeps them compiling.
- **`masque-website` is not a git repository.** It needs `git init`
  before Phase 5.
- **Toolchain on this machine:** only the `x86_64-unknown-linux-gnu`
  Rust target is installed, and there is no `emcc`.

## 1. Approach

**Target `wasm32-unknown-emscripten`, with the CPU raster backend.**

The deciding constraint is the one in §0: Skia is the text engine inside
layout. The only build of Skia for the web is emscripten's. That is how
CanvasKit is built, and it is the platform skia-bindings supports.

Consequences that shape the rest of the plan:

- **No `wasm-bindgen`.** It does not support the emscripten target. The
  host exports plain `extern "C"` functions, and JavaScript calls them
  through emscripten's `cwrap` / `HEAPU8`. The export surface is small
  (§5), so writing the glue by hand is cheap.
- **Raster first, WebGL later.** `SkiaEnv::draw` paints into
  `skia_safe::surfaces::raster_n32_premul`. JavaScript copies the pixel
  buffer into a `<canvas>` with `putImageData`. At 600×400 logical on a
  2× display that is about 2 MB per frame, which is fine for a demo
  stage that idles when nothing changes. Skia's `gl` feature against
  WebGL is a later step (§9) and changes only the host.
- **The same pixels as native.** A CPU raster with the same Skia, the
  same font and the same script should render identically on both
  sides, give or take SIMD rounding. That gives the plan a strong gate
  (Phase 3): render natively, render in wasm, compare.

Rejected (see the earlier discussion): compiling only the core to plain
`wasm32-unknown-unknown` and drawing with Canvas2D or HTML. It means
pulling SkParagraph out of layout behind a text-measuring trait across
Text, TextInput and the caret code, and the stage would then show a
renderer that isn't the shipped one.

## 2. Worktree and gates

```sh
git worktree add ../ogham-web -b web     # from /home/rowan/Projects/ogham
```

This is a deliberate exception to the single-branch rule. The rule
allows isolation for a spike that might break things, and Phase 1 is
exactly that. Main and every consumer stay on `../ogham`. The `web`
branch is rebased onto main before each phase. It merges back as a fast
forward only after Phase 4, when the changes inside the `ogham` crate
are proven. Phases 5–6 live in the website repository, and the stage
artefact is copied there.

Every phase in this repository has the same base gate:

- `cargo build` and `cargo test` at the worktree root, with default
  features, are green. That includes `tests/independence.rs`.
- `cargo fmt --check` passes on the touched files.
- Clippy is measured as a **delta against main's baseline**, not
  against zero, because CI is red on main. Record the baseline count in
  Phase 0.

## 3. Phases

### Phase 0: toolchain (½ day)

1. Install emsdk, pinned. Pick the version in Phase 1: it must satisfy
   both rustc's emscripten target and Skia's build. Write it down here
   and in `web/README.md`.
2. `rustup target add wasm32-unknown-emscripten`.
3. Export `EMSDK`, which skia-bindings' bindgen step panics without.
4. Create the worktree (§2) and record the clippy baseline.
5. `git init` `masque-website`, and commit its current state as the
   baseline.

**Gate:** `emcc --version` works, the target is listed, and the
worktree's `cargo test` is green.

### Phase 1: Skia alone in the browser, the go/no-go spike (1–3 days)

This phase is the risk. Do it before touching ogham.

A throwaway crate in the scratchpad depends on `skia-safe = "0.91"` with
`textlayout` and without `vulkan`. Built for
`wasm32-unknown-emscripten`, it:

- creates a raster surface;
- loads a font from bytes through `FontMgr::new().new_from_data` (the
  empty font manager);
- lays out and draws a paragraph and a rounded rect;
- writes a PNG.

Run it under `node`.

Find out and record:

- **Prebuilt or source build.** Does rust-skia's binary cache have an
  emscripten archive for 0.91 with these features, or does Skia build
  from source? A source build needs python, ninja and emsdk, and takes
  tens of minutes once, then is cached.
- **The working emsdk version** (Phase 0, item 1).
- **Size**: the `.wasm` file raw, after `wasm-opt -Oz`, and
  brotli-compressed. This sets the budget for Phase 4.
- **Whether `embed-icudtl` and the `pdf` default feature carry into
  the wasm build.** `pdf` is dead weight for the stage, and whether it
  can be dropped feeds Phase 2.

**Go:** a PNG with readable text comes out of node.
**No-go:** Skia can't be made to build for emscripten in three days. In
that case stop and come back to this document. The fallback is the
rejected approach in §1, and that is a different and larger plan.

### Phase 2: native-only dependencies leave the browser build (1 day)

These changes are in the worktree, inside the `ogham` crate. Consumers
must see **no difference**.

Gate by target, not by feature:

```toml
[dependencies]
downcast-rs = "2.0.1"
lodepng = "3.12.1"
nalgebra-glm = "0.19.0"

[target.'cfg(not(target_os = "emscripten"))'.dependencies]
skia-safe = { version = "0.91.0", features = ["vulkan", "textlayout"] }
notify = "6.1"
tower-lsp = "0.20"
tokio = { version = "1", features = ["io-std", "macros", "rt-multi-thread"] }

[target.'cfg(target_os = "emscripten")'.dependencies]
skia-safe = { version = "0.91.0", features = ["textlayout"] }  # ± default-features per Phase 1
```

- `mod file_watcher` and the `watch` / `check_for_changes` / watched
  `reload` paths in `lib.rs` get
  `#[cfg(not(target_os = "emscripten"))]`. `from_source` is the
  browser's constructor.
- `ogham-lsp` and `ogham-vocab` are binaries, and a browser host builds
  only the library, so they need nothing.
- Add `Ogham::register_font_data(family, &[&[u8]])` beside
  `register_font`. `register_font` becomes a wrapper that reads the
  files and calls it. This is additive. A browser carries its fonts with
  `include_bytes!`, and could otherwise write them into MEMFS first, but
  bytes are the honest form.

**Decision 2a (resolved: target tables).** Target tables were chosen;
the user had no preference. They describe a property of the platform, they need no
`default-features` handling in seven consumer manifests, and they leave
the separate open item of splitting the LSP and CLI out of the package
untouched. A `watch` / `lsp` feature split is the alternative if you
want a native host without tokio too. That is that other item, and I
wouldn't bundle it in here.

**Gate:** the base gate, plus `cargo tree -e normal` at the worktree
root is unchanged compared with main. For consumers, temporarily point
lorekeeper's app at the worktree with a local, uncommitted path edit,
run `cargo check`, then revert the edit.

### Phase 3: ogham compiles and renders for emscripten (1–2 days)

```sh
cargo build --lib --target wasm32-unknown-emscripten
```

Expected issues:

- **The fallback font collection.**
  `text_layout::DEFAULT_FONT_COLLECTION` is built from `FontMgr::new()`
  and is used before the first layout caches the UI's collection.
  Under the empty font manager it measures with no fonts, and the
  result disagrees with what is drawn. Fix: `rebuild_font_collection`
  also installs its provider into the thread-local fallback, through a
  `pub(crate)` setter. Natively this should be a no-op in practice, and
  it removes a latent mismatch there too.
- Anything else the compiler finds. List it here as found.

**Golden gate.** A test binary, `web/tests/golden` or a `node` script,
renders the hero script with a bundled font at 600×400 @2×:

1. natively, with `cargo run`, to `native.png`;
2. as wasm, under node, to `wasm.png`.

The two match exactly, or within a stated per-channel tolerance if SIMD
rounding differs. Record which.

### Phase 4: the host crate `web/` (2–3 days)

`web/` is a package in this repository: `ogham-web`, a `bin` target,
which is how Rust's emscripten target emits `.js` + `.wasm`. It is a
workspace member with `default-members = ["."]`, so a bare `cargo
build` or `cargo test` at the root doesn't change.

**Decision 4a (resolved: this repository).** The host lives here. It is a Rust host for Ogham, the browser counterpart of
lorekeeper's `ogham_preview`. Its native unit tests run with the rest
of the repository, and the website only consumes two files. The
independence test is unaffected because it reads only the root
manifest.

#### Export surface

All functions are `extern "C"`. Strings are passed as pointer + length
into the wasm heap.

| Export | Does |
|---|---|
| `stage_new(src, len) -> i32` | `from_source` with the bundled font as default, with strict vocabulary turned on. 0 = ok. |
| `stage_set_source(src, len) -> i32` | Rebuild from new source. On error, **keep the last good tree** and hold the error. |
| `stage_error(out_len) -> *const u8` | The held `RuntimeError` / bridge error / vocabulary findings as UTF-8, or null. |
| `stage_resize(w, h, dpr)` | Logical size + DPI. Reallocates the raster surface at `w·dpr × h·dpr`; `SkiaEnv::set_dpi_scale`. |
| `stage_pointer(kind, x, y, mods)` | kind ∈ move / down / up / leave → `Event::with_point`. Leave = a `mouse_move` outside the bounds, so `mouse_leave` fires. |
| `stage_wheel(x, y, dx, dy)` | The `"scroll"` event. |
| `stage_key(kind, key_code, char, mods)` | `Event::keydown` / `keypress` / `keyup`, with the same sequence as lorekeeper's `app/src/lib.rs` (~line 600). |
| `stage_frame(dt_ms) -> u32` | `update` → `tick_animations(min(dt, 33ms))` → `layout` → `draw` (only if it needs a repaint) → `process_drain_queues`. Bit flags: repainted, still animating, cursor = pointer. |
| `stage_pixels() -> *const u8` | RGBA8 of the last frame, `w·dpr × h·dpr`. |
| `stage_events(out_len) -> *const u8` | `event(...)` raises since the last call, as a JSON array, then cleared. |

**Decision 4b (resolved: (i), the event bridge is in v1).** Handlers are registered by name in
`RuntimeConfig`, and the hero script raises none. The host registers
names supplied by JavaScript at `stage_new` (`event_names: string[]`)
and queues the raises. An editable pane will grow `event(...)` calls,
and the site shows them under the stage as a small log, which shows the
embedding model rather than hiding it.

Pixel format: Skia's N32 premultiplied is BGRA on little-endian.
`putImageData` wants unpremultiplied RGBA. Raster into an
`ImageInfo::new(…, ColorType::RGBA8888, AlphaType::Unpremul)` surface,
or the stage's opaque background makes premultiplication moot. Choose
whichever the golden test proves.

#### Build

`web/build.sh` pins the flags:

```
-sMODULARIZE=1 -sEXPORT_ES6=1 -sENVIRONMENT=web,node
-sALLOW_MEMORY_GROWTH=1 -sEXPORTED_FUNCTIONS=[_stage_new,…,_malloc,_free]
-sEXPORTED_RUNTIME_METHODS=[cwrap,HEAPU8]
-O3 (then wasm-opt -Oz), panic=abort
```

It writes `web/dist/ogham_stage.{js,wasm}` and a `manifest.json` with
the commit hash, the build date and the sizes.

#### Tests

- **Native unit tests** for the host logic, `cargo test -p ogham-web`:
  - pointer down/up at counter 1's centre twice → its text reads
    `Counter 1 2` and the others read 0;
  - a bad `set_source` leaves the old tree up and the error readable;
  - an idle frame reports nothing repainted.
- **A node smoke test**, `web/tests/smoke.mjs`: load the module, run
  the hero script, click, and check that the pixel region under counter
  1 changed and the region under counter 2 did not.

**Gate:** the base gate, the native host tests, the node smoke test,
and a size within the budget set in Phase 1.

**Merge back.** After this phase, rebase `web` onto main and run the
base gate. Build lorekeeper and ul-editor against the result: they
follow main, so this is the real check that consumers are unchanged.
Then fast-forward main. Phases 5–6 don't touch this crate's code. Remove
the worktree, or keep it for Phase 6's possible
`Ogham::reload_source` (decision 6b).

### Phase 5: the website stage (2 days, in `masque-website`)

- `scripts/sync-stage.mjs` copies `ogham-web/web/dist/*` into
  `public/stage/`.
- **Decision 5a (resolved: committed).** The built artefacts are
  committed to the site, with `manifest.json`. The
  site then builds with no emsdk, the bundle is reproducible, and the
  manifest says which Ogham commit it is.
- `src/components/LiveStage.tsx` (`"use client"`):
  - **Server-rendered placeholder.** `StagePreview` stays as the
    server-rendered content, so there is something to see with no
    JavaScript, during the download, and on failure.
  - **Lazy load.** An `IntersectionObserver` on the stage starts the
    download when the stage is near view, or `requestIdleCallback` on
    the home page, because the module is megabytes. Swap to the canvas
    only after `stage_new` succeeds.
  - **Sizing.** A `ResizeObserver` plus `devicePixelRatio` drive
    `stage_resize`. Match the `matchMedia` resolution query for DPR
    changes when a window moves between monitors.
  - **Input.** Pointer events, using `setPointerCapture` on down and
    `lostpointercapture` → up, so a drag that leaves the canvas still
    ends. Wheel events with `{ passive: false }` only if the document
    scrolls, otherwise let the page scroll. Keyboard events while the
    canvas has focus (`tabindex=0`), using `e.keyCode` and `e.key` when
    it is a single character.
  - **Cursor.** The `cursor = pointer` bit sets `canvas.style.cursor`.
  - **Loop.** `requestAnimationFrame` while animating or after input,
    and stopped when `stage_frame` reports idle, so a parked tab costs
    nothing.
  - **Reduced motion.** Under `prefers-reduced-motion` the springs
    still run; that is the document's business. Don't lie about it.
- Rewrite the caption in `StageDemo.tsx` that says "Preview approximated
  in HTML until the WASM stage lands", and remove the README's "Next
  steps" item.

**Gate:** `pnpm build` and `pnpm typecheck` are green. Then check by
hand in **Firefox**: clicks count per counter, hover and the cursor
behave, resize and a 1× and 2× display look right, the no-JavaScript
fallback shows the HTML stage, and the network panel shows the module
loaded lazily.

### Phase 6: editable source (1–2 days, in `masque-website`)

- **Editor: a transparent `<textarea>` over the Shiki-highlighted
  `<pre>`.** It re-highlights on input using the TextMate grammar the
  site already has (`src/grammars/masque.tmLanguage.json`). One grammar
  and no editor dependency. CodeMirror would need a second grammar in
  Lezer. Tab inserts two spaces, and Escape leaves the field so
  keyboard users aren't trapped.
- After a 250 ms pause in typing → `stage_set_source`. On error, an
  error strip under the source shows `stage_error()`, and the canvas
  keeps the last good render. A Reset button restores the original
  script.
- **Decision 6b (resolved: rebuild and reset).** `from_source` documents have no
  path, so `Ogham::reload` is a no-op for them, and v1 rebuilds, which
  resets every `state` (counters go back to 0 on each edit). That is
  accepted. A small additive `Ogham::reload_source(&str)`, doing for a
  source string what `reload_file` does for a path, stays available as
  later work if keeping state across edits starts to matter.

**Gate:** as Phase 5, plus by hand: a typo shows an error and leaves the
stage live, fixing the typo recovers, and Reset works.

## 4. Docs to update when it lands

- `AGENTS.md`: a `web/` row in the module table, and the browser host
  as a third mode beside library and standalone. The table also still
  lists `crates/ogham-derive/` and `src/cli/`, which no longer exist;
  fix them in passing or separately.
- `docs/internal/SURFACE.md`: the raster-in-wasm host as a second
  reference consumer of `SkiaEnv`.
- The site: no docs page. Browser embedding is not a user-facing
  feature for now (§8, item 3).
- This document: phase results, the pinned emsdk version, and the
  measured sizes.

## 5. Risks

| Risk | Where it shows | Response |
|---|---|---|
| Skia won't build for emscripten, or takes days | Phase 1 | No-go criterion; stop and replan. |
| emsdk version that satisfies both rustc and Skia | Phase 0–1 | Pin once and record it; CI uses the pin. |
| Size: Skia + SkParagraph + ICU + HarfBuzz is several MB | Phase 1, 4 | Budget from Phase 1's measurement; drop `pdf`; `-Oz`; brotli; lazy load. CanvasKit is the reference point. |
| Text renders differently from native | Phase 3 | The golden gate exists to catch it; the fallback-collection fix is the known cause. |
| Rust's emscripten target is less used than `-unknown-unknown` | throughout | Keep the host small and the exports C-shaped; avoid crates that assume wasm-bindgen. |
| A consumer breaks after merge | Phase 4 merge | Lorekeeper and ul-editor build against main before pushing; Phase 2's target-table approach changes nothing for them by construction. |
| Text input fidelity (IME, mobile keyboards, paste) | Phase 5–6 | Out of scope: the stage's documents use buttons. A hidden textarea feeding `keypress` is the later fix if a demo needs `TextInput`. |

## 6. Estimate

| Phase | Days |
|---|---|
| 0 Toolchain | ½ |
| 1 Skia spike (go/no-go) | 1–3 |
| 2 Dependency gates | 1 |
| 3 Ogham on emscripten + golden | 1–2 |
| 4 Host crate + merge back | 2–3 |
| 5 Website stage | 2 |
| 6 Editable source | 1–2 |
| **Total** | **8½–13½** |

Most of the spread is in Phase 1.

## 7. Later, not in this plan

- Skia's `gl` feature against WebGL, which replaces the
  `putImageData` copy.
- Rendered screenshots on `/examples`, using the same host headless in
  node. The golden harness from Phase 3 is most of it.
- `Canvas` painters in the browser. They are Rust closures, so the web
  host would register its own. The JavaScript-side painters a site
  author might want are a different design.
- Running the examples from `src/examples/` on the stage behind a tab
  switcher, using `with_embedded_sources` for any imports.

## 8. Decisions

Resolved on 2026-10-18:

1. **2a**: target tables. The user had no preference, so the
   recommendation stands.
2. **4a**: the host crate lives in this repository, as `web/`.
3. **Scope**: the site's stage only. Browser embedding is not a
   user-facing feature and gets no docs page.
4. **4b**: the JavaScript event bridge is in v1.
5. **5a**: built artefacts are committed to the site, with
   `manifest.json`.
6. **6b**: rebuild on edit; resetting `state` is acceptable.

7. **Size budget**: set from Phase 1's measurement, with no ceiling
   named in advance. The lazy load and the HTML fallback mean size costs
   download time and data, not page speed.
