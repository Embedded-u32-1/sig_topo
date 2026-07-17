// M32: reaction guard that passes → cascade fires.
//
// The guard `true` is payload-independent (DDL does not yet emit reaction
// payloads; see M28). It exercises the full path: DDL source → ReactionDef.guard
// → engine guard eval → cascade fires.

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
    when order enters approved -> inventory allocate when true
}
