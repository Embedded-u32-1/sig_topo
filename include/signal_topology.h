/*
 * M31: C-ABI (FFI) shared-library surface for signal-topology.
 *
 * A thin C API that lets C / C++ / Python / Node (via FFI) drive the engine
 * without writing Rust: load a JSON topology, send events, read state and
 * traces. The engine itself is untouched -- these functions are a pointer and
 * string translation over the existing Rust engine.
 *
 * Build the library with `cargo build`:
 *     target/debug/libsignal_topology.so   (cdynamic)
 *     target/debug/libsignal_topology.a    (static)
 *
 * Compile a consumer against it:
 *     gcc -I include examples/ffi/test.c -L target/debug -lsignal_topology \
 *         -Wl,-rpath,target/debug -o /tmp/test_ffi_c
 *
 * ---------------------------------------------------------------------------
 * MEMORY OWNERSHIP  -- read carefully
 * ---------------------------------------------------------------------------
 *
 * 1. Every `char *` returned by engine_send_event / engine_get_state /
 *    engine_get_traces is heap-allocated by Rust. The caller MUST release each
 *    one with engine_free_str(). Failing to do so leaks memory. Do NOT pass
 *    these pointers to free() -- they were allocated by Rust's allocator.
 *
 * 2. The `void *` returned by engine_new is an opaque engine handle. The caller
 *    MUST release it with engine_free() when done. Passing it to any function
 *    after engine_free() is undefined behaviour.
 *
 * 3. Input strings (topology_json, event_json, signal_id) are borrowed only
 *    for the duration of the call; the library never takes ownership of them.
 *
 * ---------------------------------------------------------------------------
 * THREAD SAFETY
 * ---------------------------------------------------------------------------
 *
 * The functions themselves hold no global mutable state, but a single engine
 * handle is NOT safe to share across threads without external synchronisation.
 * Give each thread its own engine (or serialise access).
 */

#ifndef SIGNAL_TOPOLOGY_H
#define SIGNAL_TOPOLOGY_H

#ifdef __cplusplus
extern "C" {
#endif

#include <stddef.h>

/* Opaque engine handle. Never inspect or free this directly. */
typedef void engine_t;

/*
 * Create an engine from a JSON topology string.
 * Returns an opaque handle on success, or NULL on a parse / validation error
 * (the caller must check for NULL).
 *
 * Every action referenced by the topology is automatically registered as a
 * no-op, so the engine is fully runnable without supplying action
 * implementations from the host language.
 */
extern engine_t *engine_new(const char *topology_json);

/*
 * Send an event to a signal.
 *
 * event_json must be a JSON object with "signal_id" (string) and "event"
 * (string) fields and an optional "payload" (any JSON value), e.g.:
 *     {"signal_id": "order", "event": "submit"}
 *     {"signal_id": "order", "event": "approve", "payload": {"amount": 5000}}
 *
 * Returns a JSON string the caller must free with engine_free_str():
 *   success -> {"ok": {"signal_id":..., "from":..., "to":...,
 *                     "executed_actions":[...]}}
 *   failure -> {"error": "..."}
 *     (unknown signal / event, guard block, action execution error, ...)
 */
extern char *engine_send_event(engine_t *engine, const char *event_json);

/*
 * Query the current state of a signal.
 * Returns a JSON string the caller must free with engine_free_str():
 *   success -> {"state": "..."}
 *   failure -> {"error": "..."}
 */
extern char *engine_get_state(engine_t *engine, const char *signal_id);

/*
 * Return every recorded trace event as a JSON array string the caller must free
 * with engine_free_str(). Each element carries "signal_id", "timestamp_ms", a
 * "kind" discriminator, and kind-specific fields. Example element:
 *   {"signal_id":"order","timestamp_ms":1737,"kind":"StateChanged",
 *    "from":"draft","to":"submitted"}
 */
extern char *engine_get_traces(engine_t *engine);

/* Free an engine created by engine_new(). NULL is a no-op. */
extern void engine_free(engine_t *engine);

/*
 * Free a string returned by engine_send_event / engine_get_state /
 * engine_get_traces. NULL is a no-op.
 */
extern void engine_free_str(char *s);

#ifdef __cplusplus
}
#endif

#endif /* SIGNAL_TOPOLOGY_H */
