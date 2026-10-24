# Masque

**A UI language for Rust applications.**

Masque documents are compiled to bytecode, run on a small VM, and turned into
a Flexbox widget tree. That tree is reconciled frame to frame, with spring
transitions and exit animations built in. Skia paints it out of the box, and
any other renderer can implement the `Surface` trait. Your Rust code pushes
state in and receives events out, so the UI never holds data the host doesn't
know about. Saving the file updates the running application without
restarting it.

> **Status:** pre-1.0 and not yet on crates.io. The API changes without
> deprecation cycles.

## Where Masque fits

- **Game UI over a live scene.** Menus, HUDs, inventories and tooltips
  drawn on top of a 3D renderer you already own. A transparent root lets the
  scene show through. Portals anchor chrome to a point you project from world
  space, and `UI::blocks_point` tells your picking code whether the UI is in
  the way.
- **Tool and editor chrome in a Rust application.** Panels, inspectors,
  palettes and dialogs around a viewport that Rust draws. A `Canvas` widget
  gives your own Skia code a flex-laid-out box when a dial or graph needs
  pixels that widgets can't express.
- **Iterating on UI without a rebuild.** Layout, styling and animation live in
  the document, so you can tune them while the application runs and keep its
  state.
- **Hosts with their own renderer.** The core is renderer-agnostic. Implement
  `Surface` and `RenderContext` and the same document draws wherever you need
  it.

Masque is a projection of host state. It is not a standalone application
platform. If you want the UI layer to own the application's data and logic,
a framework built around that model will suit you better.

## Getting started

Add the crate:

```toml
[dependencies]
masque = { git = "https://github.com/fian-team/masque" }
```

Masque links Skia through [`skia-safe`](https://github.com/rust-skia/rust-skia),
which downloads prebuilt binaries for common targets. If your host also
draws with Skia, use the re-exported `masque::skia_safe` so that only one
copy ends up in your binary.

Write a document, `ui/counter.masque`:

```masque
host_state { count: int };
events { increment() };

let main = fn () {
  Flex {
    style: {
      width: "grow",
      height: "grow",
      main_alignment: "center",
      cross_alignment: "center",
    },
    children: [
      Flex {
        mouse_down: fn () { event("increment"); },
        style: {
          padding: 12,
          corner_radius: 8,
          background_color: { r: 60, g: 90, b: 120, a: 255 },
        },
        children: [
          Text {
            text: "Clicked " + count + " times",
            style: { size: 18, color: { r: 255, g: 255, b: 255, a: 255 } },
          },
        ],
      },
    ],
  }
};
```

Then mount it from Rust and drive it once per frame:

```rust
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use masque::runtime::config::RuntimeConfig;
use masque::runtime::value::Value;
use masque::skia::SkiaEnv;
use masque::widget::{event::Event, point::Point, Surface};
use masque::Masque;

let clicks = Arc::new(Mutex::new(0));

let config = RuntimeConfig::new()
    .with_host_state(HashMap::from([("count".to_string(), Value::Integer(0))]))
    .with_event_handler("increment", {
        let clicks = clicks.clone();
        move |_args| {
            *clicks.lock().unwrap() += 1;
            Ok(Value::Void)
        }
    });

// `watch` reloads the document (and everything it imports) when saved.
let mut masque = Masque::watch("ui/counter.masque".to_string(), config)?;

// Each frame:
masque.with_runtime_mut(|rt| rt.set_host_state("count", *clicks.lock().unwrap()));
masque.frame(width, height, dt)?;          // reload, rerender, animate, lay out
skia_env.draw(masque.get_ui_mut());        // or your own `Surface`

// On input:
masque
    .get_ui_mut()
    .call_event(&Event::with_point("mouse_down".to_string(), Point::new(x, y)));
```

`examples/` has runnable documents. To open one in a window with hot reload,
use the previewer in [lorekeeper](https://github.com/fian-team/lorekeeper):

```sh
cd ../lorekeeper && cargo run -p masque_preview -- ../masque/examples/counter.masque
```

For editor support, build `masque-lsp` (`cargo build --bin masque-lsp`) and
point the VS Code extension in `editors/vscode/` at it.

## Language tour

### Values, functions and components

Masque is an expression language. A block's last expression is its value, as
in Rust. A component is just a function that returns a widget.

```masque
let accent = { r: 90, g: 120, b: 150, a: 255 };

let label = fn (text: string, size: int) {
  Text { text: text, style: { size: size, color: accent } }
};

let main = fn () {
  Flex {
    style: { direction: "column", gap: 8, padding: 24 },
    children: [label("Title", 24), label("Subtitle", 14)],
  }
};
```

A `.masque` document has ten built-in widgets. `Flex` and `Grid` lay things
out. `Text`, `TextInput` and `Image` (PNG, at a fixed pixel size) show
content. `Slider` and `Pad` take a value from the pointer. `Canvas` is painted
by host code. `Presence` and `Portal` cover lifecycle and layering. Sizes are
`"grow"`, `"shrink"`, a pixel count, or a weighted grow like `{ grow: 2 }`.

### Control flow

`match` and `for` are expressions, so they can sit directly where a widget or
a list of children is expected. `if` is a statement, so inside a widget use
`match` instead. `for` runs over integer ranges. A `{` after `=>` opens a
block, not a map, so a map-valued arm is written as a name bound with `let`.

```masque
let selected_bg = { r: 60, g: 90, b: 120, a: 255 };
let idle_bg = { r: 30, g: 30, b: 30, a: 255 };

let row = fn (item: map, selected: bool) {
  Flex {
    style: {
      padding: 8,
      background_color: match selected { true => selected_bg, false => idle_bg },
    },
    children: [Text { text: item.name }],
  }
};

let main = fn () {
  Flex {
    style: { direction: "column" },
    children: for (i in 0..items.length()) { row(items[i], i == selected_index) },
  }
};
```

### Host state and events

The host pushes data in by name. A document can declare what it reads and
what it raises, and a declared document is strict: an unknown name is a
compile error, and so is an undeclared event.

```masque
host_state { player_name: string, volume: float };
events { set_volume(float), close() };

let main = fn () {
  Flex {
    style: { direction: "column", gap: 12 },
    children: [
      Text { text: "Settings for " + player_name },
      Slider {
        value: volume, min: 0, max: 1,
        on_change: fn (v: float) { event("set_volume", v); },
        style: { width: 240, height: 16 },
      },
      Flex {
        mouse_down: fn () { event("close"); },
        children: [Text { text: "Done" }],
      },
    ],
  }
};
```

`state` declares document-local state that survives rerenders. It suits
purely presentational things like whether a disclosure is open.

```masque
let disclosure = fn (title: string, body: string) {
  state open = false;
  Flex {
    mouse_down: fn () { open = !open; },
    style: { direction: "column" },
    children: match open {
      true => [Text { text: title }, Text { text: body }],
      false => [Text { text: title }],
    },
  }
};
```

### Animation

When a style value changes, the properties listed in `transition` spring to
it. `initial` is the style a widget is born at. `exit` is the style it leaves
with: it stays on screen until that animation settles. A `key` gives the
widget a stable identity across rerenders.

```masque
Flex {
  key: "toast",
  initial: { opacity: 0, transform: { translate_y: -12 } },
  exit:    { opacity: 0, transform: { translate_y: -8 } },
  style: {
    padding: 12,
    background_color: { r: 40, g: 44, b: 52, a: 255 },
    transition: { opacity: "spring", transform: "spring", background_color: "spring" },
  },
  hover_style: { background_color: { r: 60, g: 66, b: 78, a: 255 } },
  children: [Text { text: "Saved" }],
}
```

`Presence` animates a swap between whole subtrees, such as pages. When its
`key` changes, the old content plays its exit while the new content enters.

```masque
Presence { key: current_page, children: [render_page(current_page)] }
```

### Layers

`Portal` lifts its children out of the parent's clip and paint order into a
named layer: `overlay-modal`, `popover`, `tooltip`, `toast` or
`cursor-attached`. Each layer has its own backdrop and input policy.

```masque
Flex {
  children: [
    Text { text: "Hover me" },
    Portal {
      open: "hover",
      layer: "tooltip",
      anchor: "parent",
      children: [Flex { style: { padding: 6 }, children: [Text { text: "A tooltip" }] }],
    },
  ],
}
```

### Imports

```masque
import "./theme.masque";                       // everything it defines
import { button, panel } from "./widgets.masque";  // just these names
```

The full grammar is in [`docs/internal/LANGUAGE.md`](docs/internal/LANGUAGE.md),
and the integration guide (input, drag and drop, key chords, custom
backends) is in [`AGENTS.md`](AGENTS.md).

## Related projects

- **[Carte](https://github.com/fian-team/carte)** is a router and scoped
  store for Rust applications. It and Masque are deliberately independent:
  neither depends on the other.
- **[Lorekeeper](https://github.com/fian-team/lorekeeper)** is the engine
  that joins them. Its `driver` crate mounts Masque documents under Carte
  routes, and its `contract` crate checks a document's `select` and
  `events {}` against the store's schemas under `cargo test`. It is also
  home to `masque_preview`.
- **Untold Lore** is a 3D game that draws all of its UI with Masque, and is
  the project that shapes it.

## License

MIT. See [`LICENSE`](LICENSE).
