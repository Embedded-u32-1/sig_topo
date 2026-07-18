// Fork-join basic scenario (M44).
//
// Teaching point: a single transition on `order` fans out to *two* parallel
// reactions via a `fork { }` block, and a `join fork0 { }` reaction waits until
// both fork members have completed before it fires. This is the canonical
// "payment and inventory happen in parallel; shipping waits for both" workflow
// pattern — the minimal fork/join the linear cascade could not express.
//
// Business narrative: paying an order kicks off inventory reservation and audit
// clearing *in parallel*. Only once BOTH have finished does the order become
// shippable: `shipment` leaves `pending`. The fork guarantees the two branches
// run as independent cascades (each per-signal atomic); the join guarantees
// `shipment` is held until the whole group is done.

signal order {
    states: [pending, paid]
    initial: pending
    on pay from pending -> paid
}

signal inventory {
    states: [ok, reserved]
    initial: ok
    on reserve from ok -> reserved
}

signal audit {
    states: [flagged, clean]
    initial: flagged
    on clear from flagged -> clean
}

signal shipment {
    states: [pending, ready]
    initial: pending
    on dispatch from pending -> ready
}

// Both fire when `order` enters `paid`, each as its own cascade. The engine
// assigns them the auto-named group `fork0`.
fork {
    when order enters paid -> inventory reserve
    when order enters paid -> audit clear
}

// Held until every `fork0` member has fired, then fires. `shipment` therefore
// leaves `pending` only after BOTH `inventory` and `audit` have changed.
join fork0 {
    when order enters paid -> shipment dispatch
}
