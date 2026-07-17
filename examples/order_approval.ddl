// Order-approval signal. Semantically equivalent to `order_approval.json`.
signal order {
    states: [draft, submitted, approved, rejected, shipped]
    initial: draft

    on submit from draft -> submitted {
        on_exit: log_draft_exit
        on_transition: validate_order_payload
        on_enter: notify_submitted
    }

    on approve from submitted -> approved
        when payload.amount > 0 and payload.amount <= 100000 {
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
