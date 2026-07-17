// Order-approval signal, adapted from `examples/order_approval.ddl`.
//
// A single order moves through a review pipeline. The `approve` transition is
// guarded by `payload.amount > 0`: a non-positive amount is blocked by the
// guard and the order stays `submitted`, so a later well-formed `approve` can
// still push it through. `reserve_inventory` stands in for an action that a
// real system would swap for a fallible inventory call (the M21 rollback seam).
signal order {
    states: [draft, submitted, approved, rejected, shipped]
    initial: draft

    on submit from draft -> submitted {
        on_exit: log_draft_exit
        on_transition: validate_order_payload
        on_enter: notify_submitted
    }

    on approve from submitted -> approved
        when payload.amount > 0 {
        on_transition: reserve_inventory
        on_enter: notify_customer_approved
    }

    on reject from submitted -> rejected {
        on_transition: release_hold
        on_enter: notify_customer_rejected
    }

    on ship from approved -> shipped {
        on_transition: dispatch_order
        on_enter: notify_shipped
    }
}
