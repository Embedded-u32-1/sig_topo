// Gate-flow signal, adapted from `examples/gate_flow.json`.
//
// A physical gate/door. It is deliberately different from the order scenario:
// it resets from any source state to `closed` (the JSON form writes this as the
// single wildcard transition `on reset from * -> closed`) and it guards the
// fault event. Together they show the engine blocking a guarded transition and
// funneling any source state to a single target.
//
// Adaptation note: the JSON fixture expresses the reset as one `from *`
// wildcard, but the DDL compiler (v0.12) binds exactly one source state per
// transition, so the wildcard is expanded here to one `reset` transition per
// source state. The observable behavior is identical — including the
// `closed -> closed` self-loop that proves the wildcard matches the *current*
// state rather than acting as a no-op. (See `doc/ddl.md` — `from` may be `*`,
// which the compiler lowers to per-state transitions the same way the JSON path
// does at runtime.)
signal gate {
    states: [closed, open, fault]
    initial: closed

    on open from closed -> open {
        on_transition: activate_motor
        on_enter: log_gate_open
    }

    on close from open -> closed {
        on_transition: deactivate_motor
        on_enter: log_gate_closed
    }

    on fault from open -> fault
        when payload.emergency == true {
        on_transition: engage_brake
        on_enter: log_fault
    }

    // Expanded form of the wildcard `on reset from * -> closed`: one transition
    // per source state. The `closed -> closed` arm is the proof that the
    // wildcard matches the current state.
    on reset from closed -> closed {
        on_transition: clear_fault_safely
        on_enter: log_reset
    }

    on reset from open -> closed {
        on_transition: clear_fault_safely
        on_enter: log_reset
    }

    on reset from fault -> closed {
        on_transition: clear_fault_safely
        on_enter: log_reset
    }

    on repair from fault -> closed {
        on_transition: run_diagnostics
        on_enter: log_repair
    }
}
