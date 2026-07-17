// Gate-flow signal, adapted from `examples/gate_flow.json`.
//
// A physical gate/door. It is deliberately different from the order scenario:
// it resets from any source state to `closed` via the wildcard `from *` and it
// guards the fault event. Together they show the engine blocking a guarded
// transition and funneling any source state to a single target.
//
// The DDL compiler lowers `on reset from * -> closed` to one `reset`
// transition per source state (see `doc/ddl.md` — `from` may be `*`). The
// observable behavior is identical — including the `closed -> closed`
// self-loop that proves the wildcard matches the *current* state rather than
// acting as a no-op.
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
        on_transition: engage_brake, engage_backup_brake
        on_enter: log_fault
    }

    // Wildcard reset: one transition per source state, lowering to the same
    // behavior the v0.12 DDL wrote as three explicit `reset` arms. The
    // `closed -> closed` arm is the proof that `*` matches the current state.
    on reset from * -> closed {
        on_transition: clear_fault_safely
        on_enter: log_reset
    }

    on repair from fault -> closed {
        on_transition: run_diagnostics
        on_enter: log_repair
    }
}
