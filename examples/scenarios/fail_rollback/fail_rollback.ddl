// Failure-rollback scenario (teaching): an action is injected to fail, the
// engine rolls the transition back, then a re-run without the injection
// succeeds. EXPECTED.md walks the ActionFailed + Rollbacked trace.
//
// The teaching point is the engine's M19 transaction semantics: when any
// lifecycle action returns `Err`, the engine reverts `signal.current` to the
// source state, records an `ActionFailed` and a `Rollbacked` trace event, and
// returns `ActionExecutionError`. The signal is left in the source state, so
// a later re-run of the same event (without the injection) commits normally.
//
// The failure itself is injected at *replay* time via `fail_actions` in the
// scenario — the topology and engine are unchanged, so the exact same
// transition that once rolled back can commit on retry.
signal order {
    states: [draft, submitted, approved, shipped]
    initial: draft

    on submit from draft -> submitted {
        on_transition: validate
        on_enter: notify_submitted
    }

    on approve from submitted -> approved
        when payload.amount > 0 {
        on_transition: reserve_inventory
        on_enter: notify_approved
    }

    on ship from approved -> shipped {
        on_transition: dispatch
        on_enter: notify_shipped
    }
}
