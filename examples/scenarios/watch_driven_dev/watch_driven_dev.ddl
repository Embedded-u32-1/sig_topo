// Watch-driven development scenario (M51).
//
// Teaching point: `stc watch <file.ddl> [--scenario <file.json>]` is the M51
// tight edit / inspect loop. The watcher polls the `.ddl` for changes and
// recompiles on every save; with `--scenario` it also *replays the scenario*
// against a fresh engine after each successful compile, so an edit that breaks
// the scenario is caught the instant it is saved. The `.ddl` here is the kind
// of topology you would iterate on under the watch — a guarded, multi-action
// workflow whose guard threshold you tune as the business rule evolves. The
// scenario is the regression contract the watcher guards.
//
// Business narrative: a withdrawal request is `submit`ted, then `approve`d only
// when the amount is within the per-request limit (a guard you tune under the
// watch). Approval cascades a `debit` to the ledger. As the compliance rule
// tightens (`amount <= 1000` one week, `<= 500` the next), you edit the guard
// in place and the watcher tells you immediately whether the scenario still
// passes — a broken scenario is caught on save, not in production.

signal withdrawal {
    states: [draft, submitted, approved]
    initial: draft

    on submit from draft -> submitted {
        on_transition: validate_request
        on_enter: log_submitted
    }

    // The guard threshold is the knob you tune under `stc watch`: tightening it
    // (`<= 1000` -> `<= 500`) regresses the "large withdrawal" scenario arm;
    // loosening it makes the large withdrawal pass. The watcher re-evaluates
    // the scenario on every save.
    on approve from submitted -> approved
        when payload.amount > 0 and payload.amount <= 1000 {
        on_transition: reserve_funds
        on_enter: log_approved
    }
}

signal ledger {
    states: [balanced, debited]
    initial: balanced
    on debit from balanced -> debited
}

reaction {
    when withdrawal enters approved -> ledger debit
        with { "origin": "withdrawal" }
}
