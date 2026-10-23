# Masque — Architecture at a Glance

> **Status: Live contract.**
>
> A one-page orientation. Each diagram answers a question someone
> new to the codebase tends to ask in their first hour. The detail
> lives in the documents this one points at; this page is for the
> picture-shaped answers.
>
> If a diagram contradicts code, the code is the more reliable
> source — but flag the discrepancy. The same drift discipline
> that governs `INTENT.md` applies here.

## 1. The pipeline

```mermaid
flowchart LR
    src[".masque source"]
    tok["Token stream<br/>(scanner/)"]
    ast["AST<br/>(parser/)"]
    bc["Bytecode<br/>FunctionProto<br/>(runtime/compiler.rs)"]
    val["Values<br/>incl. WidgetDescriptor<br/>(runtime/vm.rs)"]
    tree["Widget tree<br/>WidgetRef<br/>(widget/builder.rs)"]
    ui["UI tick<br/>layout · events · animation<br/>(widget/mod.rs)"]
    px["Pixels<br/>(skia.rs / custom Surface)"]

    src --> tok --> ast --> bc --> val --> tree --> ui --> px
```

Each arrow is a one-way dependency. The compiler doesn't see
runtime state; the VM doesn't construct widget trees; the builder
doesn't see AST; the surface doesn't mutate the tree. This is
load-bearing — see [INTENT §1](INTENT.md#1-the-pipeline-is-four-distinct-passes-not-three).

**Read next:** [`LANGUAGE.md`](LANGUAGE.md) for scanner+parser,
[`VM.md`](VM.md) for compiler+VM, [`WIDGET_TREE.md`](WIDGET_TREE.md)
for builder+reconciliation.

## 2. Embedding

```mermaid
flowchart TB
    subgraph host["Host Rust application"]
        rust["application code"]
        cfg["RuntimeConfig<br/>· host_state<br/>· event_handlers<br/>· custom_widgets<br/>· fonts"]
    end

    subgraph masque["Masque instance (lib.rs)"]
        rt["Runtime<br/>(runtime/mod.rs)"]
        uitree["UI<br/>(widget/mod.rs)"]
        fw["FileWatcher<br/>(file_watcher.rs)"]
    end

    surface["Surface impl<br/>(skia.rs or custom)"]

    rust -- "RuntimeConfig::new()<br/>.with_host_state()<br/>.with_event_handler()" --> cfg
    cfg -- "Masque::watch / from_source" --> rt
    rt -- "VM produces Value::Widget" --> uitree
    fw -. "file changed → reload" .-> rt
    rust -- "tick(inject)<br/>set_screen_size()<br/>call_event()" --> masque
    rust -- "event(name, ...)<br/>(via handler)" --- rt
    uitree -- "UI::layout(w, h)" --> uitree
    surface -- "draw(&mut UI)" --> uitree
```

Host writes; Masque reads. From the `.masque` side, host state is
observable but immutable; mutations leave as `event(...)` calls
the host's registered handlers receive. See
[INTENT §2](INTENT.md#2-host-state-flows-in-events-flow-out) and
[`RUNTIME.md`](RUNTIME.md).

The `tick(inject)` callback is the canonical per-frame seam: the
host pushes the current frame's state into the runtime via the
`inject` closure; `tick` then checks for file changes, runs the
injection, and reconciles the tree if a rerender is pending. Returns
`true` if the tree was reconciled.

## 3. Per-frame loop

```mermaid
sequenceDiagram
    autonumber
    participant Host
    participant Masque
    participant RT as Runtime
    participant UI
    participant SF as Surface

    Host->>Masque: tick(inject)
    Masque->>Masque: file_watcher.check_for_changes()
    alt watcher reports change
        Masque->>RT: reload (fresh Runtime from disk)
    end
    Masque->>RT: inject(&mut runtime) — host state push
    Note over RT: set_host_state diffs;<br/>request_rerender if changed
    Masque->>RT: needs_rerender()?
    alt yes
        RT->>RT: rerender() → Value::Widget
        Masque->>UI: reconcile(new_root)
        UI-->>Masque: UpdateResult { needs_layout, needs_boundary_layout, needs_repaint }
    end
    Host->>Masque: tick_animations(dt)
    Masque->>UI: tick_animations(dt) — springs + smooth scroll
    Host->>Masque: layout(w, h)
    Masque->>UI: UI::layout(w, h) — full pass, boundary pass, or nothing
    Host->>SF: draw(&mut ui)
    SF->>UI: walk tree, calling Widget::render
```

Three independent dirty signals reach the UI: rerenders
(`mark_needs_layout`), event handling (`mark_dirty`), and
animation ticks. Each can request a layout pass. A rerender or a
tick whose change stayed inside a box fixed on both axes requests
only a **boundary pass**, which lays out the marked boxes against
their stored constraints (`FLEX.md` *Relayout boundaries*).
`UI::layout_counts` counts both kinds; debug builds attribute
full passes per source, so a chronic over-marking regression
shows up as a per-second warning.

After draw the host calls `Masque::process_drain_queues()` to
flush any `on_unmount` / `cleanup` hooks owed by widgets that
settled their exit animation in this tick — the drain machinery
crosses the UI/Runtime seam and is the reason `tick_animations`
threads a `TickContext` (carrying `drained_path_prefixes` and
`cancelled_unmount_prefixes`) rather than a bare `dt`. Drag and
context-menu dispatch take their own paths
(`Masque::dispatch_drag_*` / `dispatch_contextmenu`) — see
[EVENTS.md](EVENTS.md).

**Read next:** [`WIDGET_TREE.md`](WIDGET_TREE.md) for what
`reconcile` actually does, [`STYLE_AND_ANIMATION.md`](STYLE_AND_ANIMATION.md)
for `tick_animations`, [`ANIMATION_LIFECYCLE.md`](ANIMATION_LIFECYCLE.md)
for entry/exit + Presence sequencing, [`SURFACE.md`](SURFACE.md)
for the draw seam, [`EVENTS.md`](EVENTS.md) for drag and
context-menu dispatch, [`RUNTIME.md`](RUNTIME.md) for drain
queues and host-state injection.

## 4. Module layout

```mermaid
flowchart TB
    subgraph src["src/"]
        lib["lib.rs<br/>Masque (top-level facade)"]

        subgraph scanner["scanner/"]
            sc["mod.rs · token.rs · token_type.rs"]
        end

        subgraph parser["parser/"]
            pa["mod.rs (Parser)<br/>statement.rs · expression.rs<br/>literal.rs · widget.rs · function.rs<br/>operator.rs · span.rs · syntax_error.rs<br/>array.rs · map.rs · call.rs · block.rs · identifier.rs · node.rs"]
        end

        subgraph runtime["runtime/"]
            rt["mod.rs (Runtime, StateManager, ImportResolver)<br/>config.rs · host_state.rs · environment.rs<br/>compiler.rs · vm.rs · opcode.rs<br/>value.rs · ops.rs · descriptor.rs · error.rs"]
        end

        subgraph widget["widget/"]
            w["mod.rs (UI, Widget trait, Surface trait, RenderEffects)<br/>builder.rs (Value → tree)<br/>flex_widget.rs · presence_widget.rs<br/>portal_widget.rs · portal_layer.rs<br/>grid_widget.rs · text_widget.rs<br/>text_input_widget.rs · svg_widget.rs<br/>image_widget.rs<br/>style.rs · animation.rs · event.rs<br/>point.rs · rect.rs · image.rs"]
        end

        subgraph diag["diagnostics/"]
            dg["check.rs · diagnostic.rs · manifest.rs<br/>(schema-diagnostic engine)"]
        end

        subgraph cli["cli/ (binary: masque)"]
            cli_files["check.rs · render.rs · main.rs"]
        end

        subgraph lsp["lsp/ (binary: masque-lsp)"]
            l["server.rs · document.rs · hover.rs<br/>goto_definition.rs · semantic_tokens.rs<br/>document_symbols.rs · main.rs"]
        end

        subgraph client["client/ (binary: masque browser)"]
            c["standalone .masque viewer"]
        end

        typed["typed.rs<br/>(TypedMasque&lt;S, M&gt;)"]
        skia_mod["skia.rs<br/>(SkiaEnv : Surface)"]
        fw["file_watcher.rs"]
    end

    lib --> scanner
    lib --> parser
    lib --> runtime
    lib --> widget
    lib --> skia_mod
    lib --> fw
    lib --> typed
    runtime --> parser
    runtime --> scanner
    widget --> runtime
    skia_mod --> widget
    typed --> lib
    cli --> diag
    cli --> lib
    lsp --> scanner
    lsp --> parser
    client --> lib
```

The LSP depends on `scanner` and `parser` only — never on
`runtime` or `widget`. That's the seam that keeps it cheap and
side-effect-free; see [LSP.md](LSP.md) and
[INTENT §1](INTENT.md#1-the-pipeline-is-four-distinct-passes-not-three).

## 5. Where state lives

```mermaid
flowchart LR
    subgraph runtime["Runtime (one per Masque instance)"]
        hs["host_state<br/>HashMap&lt;String, Value&gt;"]
        cs["state.component_state<br/>HashMap&lt;path:name, Value&gt;"]
        lh["state.lifecycle hooks<br/>(mount / unmount / effect / cleanup<br/>per call-stack path)"]
        eh["event_handlers"]
        ctx["context_stack<br/>(transient per render)"]
        env["environment<br/>(transient per render)"]
        comp["compiled_module<br/>(cached bytecode)"]
    end

    subgraph ui["UI (widget tree)"]
        sp["per-widget springs<br/>(AnimationState)"]
        hov["per-widget hover"]
        sc["per-Flex scroll_y"]
        focus["focused widget"]
        layers["portal_layers<br/>(per-frame, rebuilt by reconcile)"]
        drag["active_drag_preview<br/>(while drag in flight)"]
        pend["pending_drained_prefixes<br/>pending_cancelled_prefixes<br/>(flushed by process_drain_queues)"]
    end

    rust["Host Rust app"] -- write --> hs
    runtime -- "VM reads via GetState (with fallback)" --- env
    runtime -- "VM reads via GetHostState" --- hs
    runtime -- "DeclareState/SetState writes" --- cs
    runtime -- "RegisterMountHook / Unmount / Effect writes" --- lh
    ui -- "preserved across reconcile" --- sp
    ui -- "preserved across reconcile" --- hov
    ui -- "preserved across reconcile" --- sc
    ui -- "rebuilt per reconcile (entries) /<br/>persistent (registry)" --- layers

    classDef volatile fill:#ffeebb,stroke:#aa8833;
    class env,ctx,comp,layers volatile;
```

Yellow boxes are derived/transient and rebuilt every render (the
environment and context stack are cleared at the start of every
module execution; the compiled module is cached but invalidated
when the source changes). Everything else persists across
rerenders — that persistence is what makes spring animations and
state-driven UI work.

**Watch out:** `Masque::reload` and `recompile_from_source` build
a *new* `Runtime`, so component state and the lifecycle-hook
registry are dropped on hot reload (the reload path also calls
`UI::clear_lifecycle_state` to scrub any pending drain queues
on the UI side). Widget-tree animation/hover/scroll state
survives because it lives on the `UI`, which reconciles against
the new descriptors. See
[INTENT §7](INTENT.md#7-hot-reload-preserves-what-it-can-drops-what-it-cant).
