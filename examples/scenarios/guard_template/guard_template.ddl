// Guard-template scenario (M38).
//
// A single top-level `guard allow_alloc { payload.auto == true }` declaration
// is referenced by *two* reactions via `when allow_alloc`. This is the M38
// "guard template / reuse" feature: the guard expression is written once and
// inlined into every reaction that references it. The two reactions must
// therefore behave identically — both fire when the guard is true, both skip
// when it is false — and the engine records a `ReactionGuardEvaluated` trace
// event for each.
//
// The scenario drives `order` through `pending -> approved` twice: once with
// `auto: true` (both reactions fire) and once with `auto: false` (both skip,
// but the main transition still commits).

guard allow_alloc {
    payload.auto == true
}

signal order {
    states: [pending, approved]
    initial: pending
    on approve from pending -> approved
    on reset from approved -> pending
}

signal inventory {
    states: [idle, allocated]
    initial: idle
    on allocate from idle -> allocated
}

signal audit {
    states: [idle, noted]
    initial: idle
    on note from idle -> noted
}

// Two reactions share the same guard id.
reaction {
    when order enters approved -> inventory allocate when allow_alloc
}
reaction {
    when order enters approved -> audit note when allow_alloc
}
