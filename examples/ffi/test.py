"""
M31: Python (ctypes) end-to-end demo of the signal-topology C-ABI.

Loads the order_approval topology, drives it through submit -> approve -> ship,
and asserts the final state is "shipped". The topology JSON is embedded inline
so the demo has no file-reading dependency.

Run (from the repo root):
    LD_LIBRARY_PATH=target/debug python3 examples/ffi/test.py
"""

import ctypes
import json
import os
import sys

# Locate the shared library relative to this script's directory.
HERE = os.path.dirname(os.path.abspath(__file__))
LIB_PATH = os.path.join(HERE, "..", "..", "target", "debug", "libsignal_topology.so")

# Embedded copy of examples/order_approval.json (single-signal: order).
ORDER_APPROVAL = {
    "version": "0.1",
    "signals": [
        {
            "id": "order",
            "initial_state": "draft",
            "states": ["draft", "submitted", "approved", "rejected", "shipped"],
        }
    ],
    "transitions": [
        {
            "signal_id": "order", "from": "draft", "event": "submit",
            "to": "submitted",
            "actions": {
                "on_exit": ["log_draft_exit"],
                "on_transition": ["validate_order_payload"],
                "on_enter": ["notify_submitted"],
            },
        },
        {
            "signal_id": "order", "from": "submitted", "event": "approve",
            "to": "approved",
            "guard": "payload.amount > 0 and payload.amount <= 100000",
            "actions": {
                "on_transition": ["reserve_inventory"],
                "on_enter": ["notify_customer_approved"],
            },
        },
        {
            "signal_id": "order", "from": "submitted", "event": "reject",
            "to": "rejected",
            "actions": {
                "on_transition": ["release_hold"],
                "on_enter": ["notify_customer_rejected"],
            },
        },
        {
            "signal_id": "order", "from": "approved", "event": "ship",
            "to": "shipped",
            "actions": {
                "on_transition": ["dispatch_order"],
                "on_enter": ["notify_shipped"],
            },
        },
    ],
}


def load_library(path):
    lib = ctypes.CDLL(path)

    # engine_new(topology_json) -> void*
    lib.engine_new.argtypes = [ctypes.c_char_p]
    lib.engine_new.restype = ctypes.c_void_p

    # engine_send_event(engine, event_json) -> char*
    lib.engine_send_event.argtypes = [ctypes.c_void_p, ctypes.c_char_p]
    lib.engine_send_event.restype = ctypes.POINTER(ctypes.c_char)

    # engine_get_state(engine, signal_id) -> char*
    lib.engine_get_state.argtypes = [ctypes.c_void_p, ctypes.c_char_p]
    lib.engine_get_state.restype = ctypes.POINTER(ctypes.c_char)

    # engine_get_traces(engine) -> char*
    lib.engine_get_traces.argtypes = [ctypes.c_void_p]
    lib.engine_get_traces.restype = ctypes.POINTER(ctypes.c_char)

    # engine_free(engine)
    lib.engine_free.argtypes = [ctypes.c_void_p]
    lib.engine_free.restype = None

    # engine_free_str(s)
    lib.engine_free_str.argtypes = [ctypes.POINTER(ctypes.c_char)]
    lib.engine_free_str.restype = None

    return lib


def read_str(ptr):
    """Convert a returned char* to a Python string and free it."""
    if not ptr:
        raise RuntimeError("got null pointer from engine")
    try:
        return ctypes.string_at(ptr).decode("utf-8")
    finally:
        lib.engine_free_str(ptr)


def event_json(signal_id, event, payload=None):
    obj = {"signal_id": signal_id, "event": event}
    if payload is not None:
        obj["payload"] = payload
    return json.dumps(obj).encode("utf-8")


lib = load_library(LIB_PATH)

engine = lib.engine_new(json.dumps(ORDER_APPROVAL).encode("utf-8"))
if not engine:
    print("FAIL: engine_new returned NULL", file=sys.stderr)
    sys.exit(1)

result = json.loads(read_str(lib.engine_send_event(engine, event_json("order", "submit"))))
print("submit  ->", result)
assert result["ok"]["to"] == "submitted", f"submit did not reach submitted: {result}"

result = json.loads(read_str(lib.engine_send_event(
    engine, event_json("order", "approve", {"amount": 5000}))))
print("approve ->", result)
assert result["ok"]["to"] == "approved", f"approve did not reach approved: {result}"

result = json.loads(read_str(lib.engine_send_event(engine, event_json("order", "ship"))))
print("ship    ->", result)
assert result["ok"]["to"] == "shipped", f"ship did not reach shipped: {result}"

state = json.loads(read_str(lib.engine_get_state(engine, b"order")))
print("state   ->", state)
assert state["state"] == "shipped", f"final state is not shipped: {state}"

# Bonus: traces come back as a JSON array.
traces = json.loads(read_str(lib.engine_get_traces(engine)))
print("traces  ->", len(traces), "event(s)")
assert isinstance(traces, list) and traces, "traces should be a non-empty array"
assert all("kind" in e for e in traces), "trace events should carry a kind field"

lib.engine_free(engine)

print("PASS: order reached shipped via Python ctypes")
