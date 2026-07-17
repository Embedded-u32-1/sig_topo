# WASM Binding

`signal-topology` exposes a small `wasm-bindgen` surface so the browser / Node /
extension host can drive the engine without writing Rust: load a JSON topology,
send events, read state and traces, render a live DOT. The engine layer is
untouched -- `wasm-topology/` is a thin translation over the existing safe API,
mirroring the C-ABI layer (`src/ffi.rs`).

The API and usage are defined once in the wrapper crate
[`wasm-topology/`](wasm-topology/); this document explains how to build and use
it.

## Building

WASM lives in its own workspace sub-crate so the main `signal-topology` crate
keeps **zero new Rust dependencies**. The only brand-new dependency for M35 is
`wasm-bindgen` (plus `wasm-bindgen-test` as a dev-dependency); `serde` /
`serde_json` / `thiserror` are pulled transitively through the engine.

### Path A -- wasm-pack (recommended)

[`wasm-pack`](https://rustwasm.github.io/wasm-pack/) produces a ready-to-ship
`pkg/` with a friendly ES-module API and a smaller binary:

```bash
wasm-pack build --target web --out-dir pkg --release -p wasm-topology
# produces: pkg/wasm_topology.js + pkg/wasm_topology_bg.wasm
```

`--target web` emits an ES module you can load with `<script type="module">` or
bundler. (Other useful targets: `--target bundler` for webpack/vite, `--target
nodejs` for Node.)

### Path B -- cargo + wasm-bindgen-cli (fallback)

If `wasm-pack` is unavailable:

```bash
cargo build --target wasm32-unknown-unknown --release -p wasm-topology
wasm-bindgen --target web --out-dir pkg \
    target/wasm32-unknown-unknown/release/wasm_topology.wasm
```

Both paths are wrapped by [`wasm-topology/build.sh`](wasm-topology/build.sh),
which picks whichever toolchain is present.

### Verifying the build

The headless-build acceptance check is just:

```bash
cargo build --target wasm32-unknown-unknown -p wasm-topology
```

which emits `target/wasm32-unknown-unknown/debug/wasm_topology.wasm`. (The
friendly JS glue is only produced by the `wasm-bindgen` CLI step above.)

## JavaScript API

Load the ES module and `await init()` to instantiate the wasm, then construct an
engine:

```js
import init, { WasmEngine } from './pkg/wasm_topology.js';

await init();
const engine = new WasmEngine(topologyJson);   // throws on parse/validation error
```

`WasmEngine` is not constructable without `new` and becomes invalid after being
garbage-collected / passed to no further calls.

### `new(topology_json)` --> `WasmEngine`

Parse a JSON topology and return a ready-to-run engine. On a parse or validation
error, **throws** a JS exception carrying the engine message. Every action
referenced by the topology is auto-registered as a no-op, so action-bearing
topologies run with no further setup (exactly like `ffi::engine_new`).

### `send_event(event_json)` --> `JSON string`

`event_json` is a JSON object with `signal_id` (string) and `event` (string) and
an optional `payload` (any JSON value):

```js
engine.send_event(JSON.stringify({ signal_id: 'order', event: 'submit' }));
engine.send_event(JSON.stringify({ signal_id: 'order', event: 'approve',
                                   payload: { amount: 5000 } }));
```

Returns a JSON string on success, **throws** on any failure (unknown signal /
event, guard block, action error):

```json
{"ok": {"signal_id":"order","from":"draft","to":"submitted",
        "executed_actions":["log_draft_exit","validate_order_payload",
                            "notify_submitted"]}}
```

On error the thrown message is the engine error string
(e.g. `Signal not found: nope`, `Transition blocked by guard '...'`). It is
**not** a `{"error": ...}` object (unlike the C-ABI surface) -- the wasm binding
uses exceptions for errors.

### `get_state(signal_id)` --> `string | null`

Returns the signal's current state, or `null` if the signal is unknown.

### `get_traces()` --> `JSON string`

Returns every recorded trace event as a JSON array. Each element carries
`signal_id`, `timestamp_ms`, a `kind` discriminator, and kind-specific fields --
the same shape as `ffi::engine_get_traces`:

```json
[
  {"signal_id":"order","timestamp_ms":1737,"kind":"EventReceived",
   "event":"submit","payload":null},
  {"signal_id":"order","timestamp_ms":1737,"kind":"StateChanged",
   "from":"draft","to":"submitted"}
]
```

`kind` is one of `EventReceived`, `ActionStarted`, `ActionSucceeded`,
`ActionFailed` (+ `error`), `StateChanged` (+ `from`/`to`), `Rollbacked` (+
`from`/`to`).

### `snapshot_dot()` --> `string`

Renders the topology as Graphviz DOT with each signal's *current* state
highlighted `lightgreen` (mirrors `TopologyEngine::snapshot_dot`). Render the
string with Graphviz / viz.js / `@hpcc-js/wasm` to get an SVG.

## Browser / Node demo

A minimal online demo lives in [`demo/index.html`](demo/index.html):
a `<textarea>` pre-filled with the order-approval topology, a
**Compile & Load** button (`new WasmEngine(json)`), per-signal **send_event**
+ quick submit/approve/ship buttons, and **Snapshot DOT** / **Traces** panes
that update as you drive the engine.

Run it:

```bash
wasm-pack build --target web --out-dir pkg --release -p wasm-topology
python3 -m http.server 8080 -d .        # serve the repo root over http
# open http://localhost:8080/demo/index.html
```

(`demo/index.html` imports `./pkg/wasm_topology.js`, so the glue must live in
`pkg/` next to the page; `build.sh` explains the layout.)

Step through `submit` (draft -> submitted) -> `approve` (with payload
`{"amount":5000}`, guard `payload.amount > 0 and payload.amount <= 100000`) ->
`ship` (approved -> shipped). The **State** pill follows along, the DOT's
`submitted` -> `approved` -> `shipped` nodes light up `lightgreen` in turn, and
the traces pane accumulates `EventReceived` / `StateChanged` entries. The approve
transition is guard-gated: drop the `amount` or set it to `0` and the call
throws `Transition blocked by guard ...`.

> WASM must be fetched over http(s) -- the `file://` scheme is blocked by
> browsers. Graphviz rendering in the demo pulls viz.js from a CDN; if the CDN
> is unreachable, the raw DOT source is always shown as a fallback.

## Auto-registered actions

The wasm surface has no way to register Rust action closures, so `new(...)`
automatically registers a no-op for **every** action id referenced by the loaded
topology. This makes any action-bearing topology fully runnable across the
language boundary with no host-language action code. (Because actions always
succeed, the WASM path cannot demonstrate rollback -- see "Limitations".)

## Limitations

- **No host-language action callbacks.** Actions execute as no-ops; the WASM
  path cannot run meaningful business logic or demonstrate action failure /
  rollback. Need that? Use the Rust engine directly, or extend the surface with
  a registered-callback scheme (a future milestone).
- **Single-threaded per engine.** The functions hold no global state, but a
  single `WasmEngine` is `!Send` and must be used on one thread. Give each
  worker / component its own engine.
- **Reaction cascades follow engine defaults.** `send_event` runs matching
  reactions recursively up to the default depth bound (`max_cascade_depth = 8`);
  there is no per-call override from WASM.
- **No engine reload.** The C-ABI surface exposes `engine_free`; the wasm
  `WasmEngine` has no teardown API beyond JS GC. To "reload", drop the old
  engine and build a new one.

## Testing

Two layers, mirroring the FFI tests:

- **Host unit tests** (`wasm-topology/src/lib.rs`, `#[cfg(test)] mod tests`):
  exercise the JSON-handling logic behind every wasm method (loading
  order_approval, `send_event` success + error paths, `get_state`, `get_traces`,
  `snapshot_dot`) with plain Rust `#[test]`s. These run with the normal
  `cargo test -p wasm-topology` -- no browser needed:
  ```bash
  cargo test -p wasm-topology        # 6 tests: reach shipped / highlight / traces / errors
  ```
- **Wasm-targeted tests** (`#[wasm_bindgen_test]`, behind
  `#[cfg(target_arch = "wasm32")]`): mirror the host tests against the real
  wasm glue. They only compile on wasm32 and are run with:
  ```bash
  wasm-pack test --headless --chrome -p wasm-topology   # or --firefox / --node
  ```

The host logic helpers are deliberately factored out of the `#[wasm_bindgen]`
impl so the same behaviour is covered by ordinary `cargo test` even where a
browser is unavailable; the wasm-targeted tests additionally exercise the real
generated glue.

The host-side engine helpers that power this crate are themselves tested in
`src/engine.rs` (`TopologyEngine::action_ids`, `TopologyEngine::traces_json`).
