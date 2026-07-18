// Guard-compensation scenario (M47).
//
// Teaching point: a reaction's `on_fail: <action_id>` hook is a *compensation*
// that runs when the cascade it triggers fails. When the derived transition in
// the target signal fails (its lifecycle action is injected to fail, so the
// target rolls back and the cascade error propagates), the engine runs the
// named compensation action **before** propagating that error upward — best
// effort, with the failure message carried in ActionContext.failure. The
// original cascade error is still returned, so the compensation can never mask
// the failure; it is a cross-signal rollback hook. The reaction is *guarded*,
// so the (compensatable) cascade is only attempted at all when the guard is
// true — the guard gates whether the side effect can even be attempted, and the
// compensation handles the case where that attempt fails.
//
// Business narrative: paying an order triggers an inventory allocation, but
// only for auto-orders (the guard). If the allocation cannot complete (its
// `commit_stock` action fails), the cascade fails and the engine runs
// `release_holding` to undo the reservation bookkeeping — a compensation that
// mirrors the business "undo the side effect when the downstream step does not
// commit." A second auto-payment where the allocation succeeds completes
// normally, proving the compensation only fires on real failures.

signal order {
    states: [pending, paid]
    initial: pending
    on pay from pending -> paid
    on reset from paid -> pending
}

signal inventory {
    states: [ok, allocated]
    initial: ok
    on allocate from ok -> allocated {
        on_transition: commit_stock
    }
}

// Guarded allocation reaction with a compensation hook. When `auto` is false
// the reaction is skipped (no cascade, no compensation). When `auto` is true
// the cascade runs: if `commit_stock` succeeds the allocation commits; if it
// fails the cascade fails and `release_holding` runs as compensation before the
// error propagates.
reaction {
    when order enters paid -> inventory allocate
        when payload.auto == true
        on_fail: release_holding
}
