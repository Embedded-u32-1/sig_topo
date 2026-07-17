// M39: guard coordination scenario — "payment success -> inventory decrement"
//
// Teaching point: a single top-level `guard <id> { <expr> }` template is shared
// by *multiple* reactions. The compiler inlines the guard's expression into every
// reaction that references it, so the reactions behave identically: when the guard
// is true both fire, when false both skip. This is the M38/M39 "shared guard =>
// consistent behavior" guarantee, demonstrated here across three signals. The
// guard gates inventory reservation *and* audit clearing together, so they can
// never diverge.
//
// Business narrative: an order is paid. On a small payment (the `canreserve`
// guard is true) the system reserves inventory *and* clears the audit trail.
// On a large payment (guard false) the shared guard skips *both* reactions, so
// inventory is left untouched and the audit stays flagged for manual review. The
// `reset` re-flags the audit (so the next payment starts from a flagged audit);
// `cancel` fails its verification and the engine rolls the order back.

guard canreserve {
    payload.amount <= 100
}

signal order {
    states: [pending, paid, cancelled]
    initial: pending
    on pay from pending -> paid
    on reset from paid -> pending
    on cancel from paid -> cancelled {
        on_transition: verify_cancel
    }
}

signal inventory {
    states: [ok, reserved, low]
    initial: ok
    on reserve from ok -> reserved
    on release from reserved -> ok
    on deduct from reserved -> low
}

signal audit {
    states: [clean, flagged]
    initial: flagged
    on mark from clean -> flagged
    on clear from flagged -> clean
}

// Both reactions reference the SAME guard id `canreserve`. They therefore share
// the guard `payload.amount <= 100` and are consistent: both fire on a small
// payment, both skip on a large one. This is the core M39 demonstration.
reaction {
    when order enters paid -> inventory reserve when canreserve
}
reaction {
    when order enters paid -> audit clear when canreserve
}

// Unguarded reset reaction: whenever an order returns to pending (via reset) the
// audit is re-flagged, setting up the large-payment attempt to start from a
// flagged audit. It is deliberately NOT gated by `canreserve` — it always runs.
reaction {
    when order enters pending -> audit mark
}
