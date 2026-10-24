# Masque — The Application Model

> **Status: Design record — agreed direction, being built.**
> 2026-08-20. Phases 0 and 1 have landed: Carte
> exists as the `carte` crate (routing's table, walk and outbox,
> with §3.2's tiers and areas and §3.4's guard form as table data),
> and §2's dependency edge holds in the direction that matters —
> `carte` depends on nothing. The store (§5), the selection
> contract (§4) and the binding (§6) are not built. Build status,
> including the one amended acceptance criterion, is
> `APPLICATION_BUILD.md` §0.5. This header flips to "built" when the
> last phase gate passes.
>
> This document states what Masque is becoming: **two composed
> frameworks and a contract between them** — a Carte
> (routing fused with a scoped, schema'd state store) and a surface
> framework (the DSL, layout, and reactivity), joined by a thin
> binding. It was workshopped against untold_lore's 2026-08-20
> architecture audit and records the patched holes as axioms.
>
> When building starts, this document amends: the routing plan
> (`lorekeeper/docs/ROUTING.md`, deleted 2026-10-17 once every phase
> but its editor-host one had landed; the rules it still stated live
> are §3.5–3.9, now in `carte/docs/APPLICATION.md`, and §6.6–6.7 here), untold_lore's crossing record (now `ul-client/src/crossing.rs`)
> axiom 4 / §6 (the route tier *does* learn areas; the presentation
> machinery survives) and §3 (the "world root needs no parking"
> rule is repealed: instance roots gain teardown/mount lifetimes,
> §5.6), and `untold_lore/docs/ROOMS.md` (a room's `Presentation`
> becomes a reading of its route node, and its `Requires`/`Refusal`
> columns move with it, §3.4). Those amendments are written in
> Phase 7, at the end of the build; until they are, those documents
> stand as written and this one is the direction. One of them is
> already half-true in the code: the route tier has left the surface
> framework's crate (WP-1.1), but nothing has been rewritten to say
> so yet.
>
> Amended 2026-08-20 after a four-repo consumer audit (untold_lore,
> regency, stargazer-celia, and lorekeeper's driver tier). The
> implementation plan is `APPLICATION_BUILD.md`.

---

## 1. Why: the history this resolves

Masque began as a React replacement for Rust applications. The parts
of that thesis that hold up are load-bearing today: retained
reactive trees (the crossing's ghost works *because* an unprojected
tree keeps drawing), CSS-like layout, a hot-reloading DSL whose
moddability story is the WoW-addon model done with a real language.

What was never re-examined after the pivot away from "React
replacement" is the framework's **boundary**. Routing entered the
language in August 2026 (`src/route/`) on the argument that the
document already had `screen` blocks and scoped keys, so the
language should be able to decide a path. That argument was right
about the gap and wrong about the home, and untold_lore proved it
empirically within a week:

- **Most readers of the path are not widgets.** Which music plays,
  which backdrop stands, when a session boots or tears down, when
  input is gated, whether the body drives — all are functions of
  the path. The path is the *application's* central derived fact;
  the UI is one consumer among several.
- **The `host_state {}` block has the contract inverted.** The
  consumer's document declares what the provider supplies. No
  serious contract system works that way, and the failure mode is
  sharpest exactly where Masque is strongest: a modder edits the
  declaration and manufactures the false impression that data
  should exist.

So: the view layer stays a view layer, and the things that crept in
— routing, and now state — become their own framework, *below* it.

## 2. The three pieces

One sentence each, per the single-responsibility rule:

- **Carte** (routing + store): what exists, what
  is true about it, and who is told when it changes.
- **The surface framework** (the DSL: documents, widgets, layout,
  reactivity, hot reload): how named surfaces look, given validated
  selections of state.
- **The binding / driver**: mounts instances, delivers frames,
  performs the crossing presentation. In a game this is the
  engine's driver tier (`lorekeeper/front` + `app`); a standalone
  application brings a minimal host trait implementation (§8.1,
  resolved).

The engineering guarantee is a dependency edge: **Carte
depends on nothing of the surface framework, and vice
versa; only the binding depends on both.** Cargo enforces it. The
surface framework's route-facing constructs (`screen`, `outlet()`,
selections) take *names* — strings and validated field paths —
never Carte's types. A single-document tool must
be able to use the surface framework with no router at all; if it
cannot, the split did not happen.

## 3. Axioms — Carte

Moved to Carte's repository, at `carte/docs/APPLICATION.md` §3, with
its numbering kept: §3.1–§3.9 are cited there as they were here.

## 4. Axioms — the contract (provider-owned, GraphQL-shaped)

Moved to `carte/docs/APPLICATION.md` §4, numbering kept (§4.1–§4.7).
The provider owns the contract, and Carte is the provider. The
consumer's half is the language: `select`, `events {}` and fragments
are in `LANGUAGE.md`.

## 5. Axioms — the store

Moved to `carte/docs/APPLICATION.md` §5, numbering kept (§5.1–§5.7).

## 6. Axioms — the binding

The §2 sentence — mounts instances, delivers frames, performs the
crossing — unpacked, because the consumer audit found every one of
these being hand-rolled somewhere:

### 6.1 The binding mounts what the table names

An instance root
names its document; the binding mounts, projects, and tears down.
No host hand-mounts a second document (`mounted_ui`, `own_ui`,
`brings_own_document` delete — untold_lore's workspaces held as
`Option<EditorClient>` fields were the drift indicator realized).

### 6.2 One draw slot per instance root

What paints *under* an
instance's document is declared at the mount: an optional backdrop
painter fed by store frame parameters (read verb) and world data —
its signature does not pass the store's write surface, which is
§5.2 drawn as an API instead of a discipline. Whether the instance
is opaque or composites over its painter is mount data too
(retiring per-route `occludes()`), and a painter that animates
continuously says so, because a widget tree only repaints when
dirtied.

### 6.3 Side channels are named and validated

Anchors (a host
point positioning document content) and painter payloads
(per-frame paint positioned by document layout) are binding-owned,
per-instance channels — per-frame geometry that must *not* be
store fields (§5.2) but must not be stringly either. Their names
validate at document load exactly as selections do; a typo'd
anchor is a refusal, not a silently empty portal.

### 6.4 The crossing belongs to the binding

The binding detects
an instance-root boundary in the path delta itself (no game diffs
`side_of` by hand), mounts the arriving instance *before* the
sweep begins, runs a parameterized default passage (direction,
duration, easing), and lets a game override per edge (`Departure`
— regency's page bleed). Presses are swallowed during a crossing;
releases flow. The passage's side effects — whoosh, music,
backdrop — are host reactions to the path change (§3.3), never
crossing code.

### 6.5 Caches are shared across instances

Images, fonts, and
their decode products live at the binding, not per instance — a
crossing must never re-decode the arriving document's art on the
crossing frame.

### 6.6 Occlusion is a ladder of pixels; modality is a claim

`Occlusion` is ordered — `None`, `View`, `Surface` — and `Surface`
implies `View`: ancestors draw and their screens render behind (a
prompt over the workspace it is leaving); ancestors draw and their
screens do not (every ordinary child); neither (a node that brings its
own world). One ladder rather than two bits is an empirical claim about
every consumer — nothing wants an ancestor's 3D hidden while its HUD
still draws — and is the line to attack first if that stops being true.
The ladder says what draws and nothing about who reads the keyboard:
`View` leaves an ancestor's `event` on the dispatch list, and a route
that declines a key hands it down. A modal node therefore claims the
whole input stream and says so; raises are already addressed and stay
untouched. Whether a body is being driven is read off the path, never
off a flag a route holds.
*Drift indicator:* a fourth occlusion value; a pause that declines keys
it means to swallow; `driving` as a field.

### 6.7 Two visible views stack

`visible_views` may return more than one id — an `Occlusion::None`
prompt over the workspace it is leaving — and the outlet renders them
as consecutive children of one `Presence` with `stack` set: each takes
the whole content box, the deeper one paints over the shallower.
Stacking composes with a child's own transform rather than setting one,
so entry animations survive; the absolutely-positioned layer tried first
set the child's paint transform and snapped every animated screen to its
final place. Rendering two views and routing input to two views are one
feature: a flow never has to decide who gets a press because its
children do not overlap, stacked children overlap exactly, so the
pointer walk is reversed when stacking and the top child is offered the
press first.
*Drift indicator:* a consumer wrapping a screen; a second outlet for
prompts.

## 7. What this dissolves in the consumers

Stated so the payoff is checkable, not vibes:

- untold_lore's `chrome::State` (78 projected keys over 77 fields,
  one namespace) becomes per-scope schemas with route-node
  lifetimes; on the main menu the gameplay scope does not exist,
  and vice versa.
- `project` (535 lines of mechanical transcription) deletes into
  derived reflection; `reproject` (313 lines/frame) splits — its
  per-frame diff and its scope-arm skeleton are the store's, its
  joins relocate into producers (§5.7).
- `every_declared_key_is_projected` and
  `every_declared_raise_reaches_a_handler` delete — their
  properties become unrepresentable.
- `UlGame::project_root` deletes — the router mounts and projects
  every instance, menu and world alike.
- `crossing::side_of` moves into the route table as the instance-
  root tier of the nodes.
- The rooms table's `Presentation` becomes a reading of the route
  node; "room" and "stance" retire as vocabulary (a room is a
  routed view; a stance is an overlay — `Overlay { keys_reach_body
  }` vs `Page` collapses the enum, `Stance` folding in as
  `keys_reach_body: false`; the table's other columns follow §3.4).
- The ul-editor family joins the ledger: `EditorShell::host_state`
  (176 lines), `map_chrome.rs` (647 lines), and the per-frame
  reinjection loops are a second hand-rolled driver whose
  documents declare no contract at all; its three `Page` instances
  become instance roots and its injections become schemas.

The same audit's ledgers for the other consumers:

- regency: the ~254-line record block and `host_state` delete; the
  schema-conformance and raise/screen guard tests delete; sixteen
  hand-rolled equality guards and the `panels_dirty` idiom die
  into the store; half of `teardown`'s manual remembering becomes
  unrepresentable (scope state dies with its node); Wheel and
  Epilogue promote from store facts to views, making stage ↔ path
  a bijection and deleting the page-turn edge detector; the page
  bleed becomes the crossing's marked passage, fed by the ghost —
  which also fixes the inbound capture that bled the backdrop but
  popped the invitation card.
- celia: `HostCx::project_root` deletes outright (the root
  headings become per-document literals); the raise mpsc plumbing
  and ~150 lines of guard tests delete; the pause-claim
  choreography, duplicated across both routes with its own
  regression test, dies into node-lifetime scope state; the
  lobby → arena crossing gains the deployment ceremony the old
  machinery never allowed.
- lorekeeper: the `shell` crate deletes (the routing plan's phase 8b, at
  last); `Chrome`'s per-key diff map *is* the proto-store and
  moves down into it; `drive_menu_music`'s memo bool becomes a
  path subscription.

## 8. Open questions

1. **The standalone driver.** Resolved by the audit: yes, and it
   already exists in embryo. The minimal host trait is `HostCx`
   minus its game-flavored methods — render context ready, window
   controls, tick producers, gesture stream — with the engine's
   `app` as one implementation and `masque_preview` (which already
   hand-drives a single instance) as the free second one that
   proves the trait. What it must not include: anything naming a
   session, a save, or a capability — those are a game
   framework's opinions.
2. **Engine stock screens.** Resolved: the engine publishes, per
   stock screen, the scope schema (a Rust type whose derived
   reflection documents validate against), the intent vocabulary,
   and the producer. Title, connect, settings, and the save
   browser ship as an engine **front-of-house instance root**
   games compose into their tables; pause, settings, and saveload
   additionally splice as views under game-owned roots (pause can
   never be a root — the world must keep drawing behind it). The
   stock document ships and is restyled per `screen` block — or
   replaced wholesale, the engine contribution that survives
   replacement being the schemas, intents, and producers;
   surfaces stay the game's, which is what lets celia's paperwork
   dress and regency's engraving survive.
3. **Mod-facing stability policy** (§4.5): stated the day
   third-party documents exist, not after.
4. **Naming — resolved.** The surface framework is **Masque** (it was
   Ogham, retired for its pronunciation) and the structure framework is
   **Carte** (it was the working name `structure`). The criteria were: name the pair as a
   family; unambiguous English pronunciation on sight; one–two
   syllables; crates.io/GitHub availability; no collision with
   established Rust projects. The pair should read as *structure*
   vs *surface*: a map and a mask.

## 9. Precedents consulted

GraphQL (provider schema / consumer selection / introspection /
subscriptions-vs-queries), the wasm component model's WIT (host
publishes, guest imports, instantiation-time refusal — the modern
answer to the modding contract), protobuf (explicit evolution
rules; contract as compiled artifact), SQL grants/views (per-
consumer-class schema projection), the signals family and re-frame
(per-field invalidation; subscriptions as derived signals), Elm
(single store, actions as the only writes) — and React context as
the anti-example, being exactly what a stringly, contract-free
`host_state` map is. The WoW addon ecosystem is the cautionary
tale on the modding side: a huge consumer surface with no static
contract, where every patch breaks addons at runtime, mysteriously.
