# C-ABI (FFI) Shared Library

`signal-topology` exposes a small C-ABI surface so C / C++ / Python / Node (via
FFI) can drive the engine without writing Rust: load a JSON topology, send
events, read state and traces. The engine layer is untouched -- `src/ffi.rs` is a
thin, unsafe pointer-and-string wrapper over the existing safe engine API. Zero
new Rust dependencies (just `std::ffi` + `#[no_mangle] extern "C"`).

The API and memory rules are defined once in the hand-written header
[`include/signal_topology.h`](include/signal_topology.h); this document explains
how to use them.

## Building the library

```bash
cargo build
```

Produces (in `target/debug/`):

| File                | Type       | Consumer                          |
| ------------------- | ---------- | --------------------------------- |
| `libsignal_topology.so` | `cdynamic` | `dlopen` / implicit dynamic link  |
| `libsignal_topology.a`  | `static`   | static link                       |

(`cargo test` continues to work because `crate-type` also includes `rlib`.)

## Function signatures

```c
/* Create an engine from a JSON topology string. Returns an opaque handle, or
 * NULL on parse/validation error (caller must check). */
extern engine_t *engine_new(const char *topology_json);

/* Send an event. Returns a JSON result string (caller frees with
 * engine_free_str): {"ok": {...}} on success, {"error": "..."} on failure. */
extern char *engine_send_event(engine_t *engine, const char *event_json);

/* Query a signal's current state: {"state": "..."} or {"error": "..."}. */
extern char *engine_get_state(engine_t *engine, const char *signal_id);

/* Return all recorded trace events as a JSON array string. */
extern char *engine_get_traces(engine_t *engine);

/* Free an engine / a returned string. Both accept NULL as a no-op. */
extern void engine_free(engine_t *engine);
extern void engine_free_str(char *s);
```

`engine_t` is an opaque type; never inspect or free it directly.

## Parameters and return JSON

### `event_json` (input)

A JSON object with `signal_id` (string) and `event` (string) and an optional
`payload` (any JSON value, defaulting to absent):

```json
{"signal_id": "order", "event": "submit"}
{"signal_id": "order", "event": "approve", "payload": {"amount": 5000}}
```

### `engine_send_event` result

```json
{"ok": {"signal_id": "order", "from": "draft", "to": "submitted",
        "executed_actions": ["log_draft_exit", "validate_order_payload",
                            "notify_submitted"]}}
```

or, on any failure (unknown signal / event, guard block, action error):

```json
{"error": "Transition not found: signal=order, event=defenestrate"}
```

### `engine_get_state` result

```json
{"state": "shipped"}
```

### `engine_get_traces` result

A JSON array; each element carries `signal_id`, `timestamp_ms`, a `kind`
discriminator, and kind-specific fields:

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

## Memory ownership (read carefully)

1. **Returned strings are Rust-allocated.** Every `char *` returned by
   `engine_send_event` / `engine_get_state` / `engine_get_traces` **must** be
   released with `engine_free_str`. Do NOT pass them to `free()` -- they were
   allocated by Rust's allocator.
2. **The engine handle must be freed.** Release the pointer from `engine_new`
   with `engine_free` when done. Using it after `engine_free` is undefined
   behaviour.
3. **Inputs are borrowed only.** `topology_json` / `event_json` / `signal_id`
   are read only for the duration of the call; the library never takes ownership.

## Auto-registered actions

The FFI surface has no way to register Rust action closures, so `engine_new`
automatically registers a no-op for **every** action id referenced by the loaded
topology. This makes any action-bearing topology fully runnable across the
language boundary with no host-language action code. (Because actions always
succeed, the FFI path cannot demonstrate rollback -- see "Limitations".)

## Language demos

Both demos load the same order-approval topology, drive
`submit -> approve -> ship`, and assert the final state is `shipped`. The
topology is embedded inline so neither demo needs to read files.

### C demo

```bash
cargo build
gcc -I include examples/ffi/test.c -L target/debug -lsignal_topology \
    -Wl,-rpath,target/debug -o /tmp/test_ffi_c && /tmp/test_ffi_c
```

The C demo links implicitly against `libsignal_topology.so` (located at run
time via the embedded `-Wl,-rpath`).

### Python demo

```bash
cargo build
LD_LIBRARY_PATH=target/debug python3 examples/ffi/test.py
```

Uses `ctypes` to load the `.so`, declares `argtypes`/`restype`, and converts
each returned `char *` to a Python string with `ctypes.string_at` before freeing
it. The demo also exercises `engine_get_traces` and asserts the trace array is
well-formed.

### One-shot verification

```bash
bash examples/ffi/run.sh
```

Builds, then compiles+runs the C demo and runs the Python demo, exiting
non-zero if any step fails.

## Limitations

- **No host-language action callbacks.** Actions execute as no-ops; the FFI
  path cannot run meaningful business logic or demonstrate action failure /
  rollback. Need that? Use the Rust engine directly, or extend the FFI with a
  registered-callback scheme (a future milestone).
- **Single-threaded per engine.** The functions hold no global state, but a
  single `engine_t` is not safe to share across threads without external
  synchronisation. Give each thread its own engine (or serialise access).
- **Reaction cascades follow engine defaults.** `send_event` runs matching
  reactions recursively up to the default depth bound (`max_cascade_depth = 8`);
  there is no per-call override from the FFI.

## Testing

The FFI is covered at two levels:

- `src/ffi.rs` -- unit tests calling the `extern "C"` functions with valid,
  invalid, and null inputs.
- `tests/ffi_test.rs` -- integration tests driving the full `order_approval`
  scenario plus error paths (bad JSON, unknown event, guard block, unknown
  signal, null engine pointers) through the public FFI only.
