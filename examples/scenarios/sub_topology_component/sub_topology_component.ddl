// Sub-topology component scenario (M45).
//
// Teaching point: a reusable `component` bundles a signal topology and exposes
// one of its states through a `port`. An `instantiate ... connect { }` block
// creates a concrete copy and *wires* the port to a parent-level signal. During
// expansion the component's internal signal is renamed into the parent
// namespace, so a parent reaction can react to the wired parent signal
// directly. The sub-topology's exposed state change feeds the parent cascade —
// composition without the parent knowing the component's internals.
//
// Business narrative: a `lockable` component models a generic lockable thing.
// The house wires it to a `door` signal and reacts when the door locks by
// alerting a `controller`. The parent never mentions the component's internal
// `lock` signal — only the aliased port `locked`, wired to `door`.

signal controller {
    states: [idle, alerted]
    initial: idle
    on notify from idle -> alerted
    on reset from alerted -> idle
}

component lockable {
    port out lock.locked as locked
    signal lock {
        states: [locked, unlocked]
        initial: unlocked
        on lock from unlocked -> locked
        on unlock from locked -> unlocked
    }
}

// Parent-level reaction against the WIRED signal name `door` (the port's
// target). After expansion the component's `lock` signal is renamed to `door`,
// so this reaction fires when `door` enters the state the port exposes.
reaction {
    when door enters locked -> controller notify
}

instantiate lockable as door with {} connect { locked -> door }
