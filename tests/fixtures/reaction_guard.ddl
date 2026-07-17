// M34: reaction guard that passes + static payload → cascade fires with a
// payload.
//
// Reaction guard `payload.auto == true` is evaluated against the source event's
// payload (here `{"auto": true}` sent on `approve`). The reaction's static
// payload `{ "auto": true }` (the `with { ... }` block) is delivered as the
// derived event's payload to the target signal. See `engine::send_event_internal`
// M32/M34 for how the two payloads differ.

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
