// M32: reaction guard that fails → cascade skipped, main transition commits.
//
// Same topology as reaction_guard.ddl, but the guard is `false`. The engine
// must skip the cascade while still committing the order -> approved transition.

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
    when order enters approved -> inventory allocate when false
}
