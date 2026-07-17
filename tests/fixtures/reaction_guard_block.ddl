// M34: reaction guard that fails → cascade skipped, main transition commits.
//
// Same topology as reaction_guard.ddl. The reaction guard `payload.auto ==
// true` is evaluated against the source event's payload; here `approve` is sent
// with `{"auto": false}`, so the guard is false and the cascade is skipped
// while the main `order -> approved` transition still commits. The reaction's
// static payload `{ "auto": true }` (the `with { ... }` block) is delivered to
// the target only when the cascade fires, so it is irrelevant here.

signal order {
    states: [submitted, approved]
    initial: submitted

    on approve from submitted -> approved
}

signal inventory {
    states: [idle, allocating]
    initial: idle

    on allocate from idle -> allocating
}

reaction {
    when order enters approved -> inventory allocate
        when payload.auto == true
        with { "auto": true }
}
