// DDL expressiveness scenario (M34) — the three DDL-side language features
// that make a `.ddl` file read like a domain document rather than JSON.
//
// Teaching point: M34 added (1) the wildcard `on ev from * -> state` that the
// compiler lowers to one concrete transition per source state, (2) multi-action
// lifecycle hooks (`on_transition: a, b, c`) evaluated in declaration order, and
// (3) the reaction static payload (`with { ... }`) riding on the derived event.
// A single scenario exercises all three together so a reader sees the .ddl
// source as a self-contained domain story.
//
// Business narrative: a machine that starts up, runs, and faults. The `start`
// transition carries a *multi-action* hook (warm up AND calibrate, in order,
// before logging the entry). `fault` only happens in an emergency (a *transition
// guard*). There is an emergency `reset` from *any* state back to `idle` (the
// `*` wildcard) that records a static `reset` payload on the audit cascade.

signal machine {
    states: [idle, running, fault]
    initial: idle

    on start from idle -> running {
        // Multi-action hook: warm_up runs, then calibrate, THEN on_enter log_run.
        on_transition: warm_up, calibrate
        on_enter: log_run
    }

    on stop from running -> idle {
        on_enter: log_stop
    }

    // Transition guard: fault only commits in a real emergency. Sending `fault`
    // without emergency is rejected with GuardBlocked and the state is unchanged.
    on fault from running -> fault
        when payload.emergency == true {
        on_transition: engage_brake
        on_enter: log_fault
    }

    // Wildcard: one transition PER source state. The compiler lowers this to
    // {idle,running,fault} -> idle, including the idle -> idle self-loop that
    // proves `*` matches the current state. On reset, run fault handling, then
    // log the entry.
    on reset from * -> idle {
        on_transition: clear_fault_safely
        on_enter: log_reset
    }
}

signal audit {
    states: [quiet, noted]
    initial: quiet
    on note from quiet -> noted
}

// Static payload: the derived `note` event rides the payload { "origin":
// "reset" } to `audit`, showing the reaction `with { ... }` block. Combined with
// the wildcard reset, any machine state -> idle produces an audited note.
reaction {
    when machine enters idle -> audit note
        with { "origin": "reset" }
}
