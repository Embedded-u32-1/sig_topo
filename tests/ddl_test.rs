//! M28: DDL end-to-end integration tests.
//!
//! Compiles `.ddl` fixtures through `ddl::compile`, feeds the resulting
//! `TopologySchema` to the unmodified `TopologyEngine`, and asserts the same
//! behaviour as the equivalent JSON fixtures. This pins down the "DDL is a
//! front-end to the engine, engine layer untouched" contract.

use signal_topology::{EngineError, TopologyEngine, TraceEvent};
use std::fs;

/// Compile a DDL source string straight into a runnable engine.
fn engine_from_ddl(src: &str) -> TopologyEngine {
    let schema = signal_topology::ddl::compile(src).expect("DDL should compile");
    TopologyEngine::from_schema(schema).expect("schema should build an engine")
}

#[test]
fn order_approval_ddl_end_to_end_reaches_shipped() {
    let src = fs::read_to_string("examples/order_approval.ddl")
        .expect("order_approval.ddl should exist");
    let mut engine = engine_from_ddl(&src);

    for action in [
        "log_draft_exit",
        "validate_order_payload",
        "notify_submitted",
        "reserve_inventory",
        "notify_customer_approved",
        "dispatch_order",
        "notify_shipped",
    ] {
        engine.register_action(action, |_| Ok(()));
    }

    engine
        .send_event("order", "submit", None)
        .expect("submit should succeed");
    assert_eq!(engine.get_state("order").unwrap(), "submitted");

    engine
        .send_event(
            "order",
            "approve",
            Some(serde_json::json!({"amount": 5000})),
        )
        .expect("approve with amount=5000 should pass the guard");
    assert_eq!(engine.get_state("order").unwrap(), "approved");

    engine
        .send_event("order", "ship", None)
        .expect("ship should succeed");
    assert_eq!(engine.get_state("order").unwrap(), "shipped");
}

#[test]
fn order_approval_ddl_guard_blocks_invalid_amount() {
    let src = fs::read_to_string("examples/order_approval.ddl")
        .expect("order_approval.ddl should exist");
    let mut engine = engine_from_ddl(&src);
    // Register every action `submit` runs, so the transition completes and the
    // only thing under test is the `approve` guard.
    for action in [
        "log_draft_exit",
        "validate_order_payload",
        "notify_submitted",
        "reserve_inventory",
    ] {
        engine.register_action(action, |_| Ok(()));
    }

    engine
        .send_event("order", "submit", None)
        .expect("submit should succeed");

    // amount == 0 violates `payload.amount > 0` -> guard blocks, state unchanged.
    let result = engine.send_event(
        "order",
        "approve",
        Some(serde_json::json!({"amount": 0})),
    );
    assert!(
        matches!(result, Err(EngineError::GuardBlocked { .. })),
        "approve(amount=0) should be guard-blocked, got {:?}",
        result
    );
    assert_eq!(
        engine.get_state("order").unwrap(),
        "submitted",
        "state must not change on guard block"
    );

    // amount above the cap also violates the guard.
    let result = engine.send_event(
        "order",
        "approve",
        Some(serde_json::json!({"amount": 100_001})),
    );
    assert!(matches!(result, Err(EngineError::GuardBlocked { .. })));
}

#[test]
fn reaction_cascade_end_to_end() {
    // When `order` enters `approved`, a reaction kicks `inventory` into
    // motion. Verifies the DDL `reaction { ... }` maps to a working cascade.
    let src = r#"
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
}
"#;

    let mut engine = engine_from_ddl(src);
    engine.register_action("noop", |_| Ok(()));

    engine
        .send_event("order", "approve", None)
        .expect("approve should succeed and trigger cascade");

    assert_eq!(engine.get_state("order").unwrap(), "approved");
    assert_eq!(
        engine.get_state("inventory").unwrap(),
        "allocating",
        "reaction should have cascaded into inventory"
    );
}

#[test]
fn reaction_guard_passes_through_to_schema() {
    // M32: a reaction guard is no longer rejected. The DDL compiler passes it
    // through verbatim into `ReactionDef.guard`, and the engine evaluates it
    // at cascade time (see tests/reaction_guard_test.rs).
    let src = r#"
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
    when order enters approved -> inventory allocate when payload.auto
}
"#;

    let schema = signal_topology::ddl::compile(src).expect("reaction guard must now compile");
    assert_eq!(schema.reactions.len(), 1);
    assert_eq!(
        schema.reactions[0].guard,
        Some("payload.auto".to_string()),
        "the guard must reach ReactionDef.guard verbatim"
    );
}

#[test]
fn reaction_guard_end_to_end_via_ddl() {
    // End-to-end via the DDL fixture: a reaction carries a guard that
    // evaluates to true with the payload the reaction delivers. This proves
    // the full path — DDL source → `ReactionDef.guard` → engine guard eval →
    // cascade fires — and that the main transition commits either way.
    //
    // Note: DDL does not yet emit reaction *payloads* (deferred in M28), so
    // this fixture's guard is written to be true without reading `payload.*`.
    // Payload-based gating through DDL will follow once reaction payload
    // templates land; the payload case is covered in
    // tests/reaction_guard_test.rs via JSON (which can set reaction payload).
    let ddl = std::fs::read_to_string("tests/fixtures/reaction_guard.ddl")
        .expect("reaction_guard.ddl fixture should exist");
    let mut engine = engine_from_ddl(&ddl);

    // The fixture's reaction guard is `payload.auto == true`, evaluated
    // against the source event's payload (M32). Send `approve` carrying the
    // matching payload so the guard passes and the cascade fires.
    engine
        .send_event(
            "order",
            "approve",
            Some(serde_json::json!({"auto": true})),
        )
        .expect("main transition commits");

    assert_eq!(engine.get_state("order").unwrap(), "approved");
    assert_eq!(
        engine.get_state("inventory").unwrap(),
        "allocating",
        "guard=true must let the cascade fire"
    );
}

#[test]
fn reaction_guard_end_to_end_ddl_blocks_when_false() {
    // The same DDL fixture topology, but `approve` is sent with
    // `{"auto": false}`. The fixture's reaction guard `payload.auto == true`
    // then evaluates to false, so the cascade must be skipped while the main
    // transition still commits — the headline M32 contract.
    let ddl = std::fs::read_to_string("tests/fixtures/reaction_guard_block.ddl")
        .expect("reaction_guard_block.ddl fixture should exist");
    let mut engine = engine_from_ddl(&ddl);

    engine
        .send_event(
            "order",
            "approve",
            Some(serde_json::json!({"auto": false})),
        )
        .expect("main transition commits");

    assert_eq!(engine.get_state("order").unwrap(), "approved");
    assert_eq!(
        engine.get_state("inventory").unwrap(),
        "idle",
        "guard=false must skip the cascade"
    );
}

#[test]
fn reaction_with_payload_reaches_reaction_def() {
    // M34: a reaction's `with { ... }` static payload is parsed and lands in
    // `ReactionDef.payload` as a valid JSON value. The engine would deliver it
    // as the derived event's payload to the target signal when the cascade
    // fires.
    let src = r#"
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
        with { "auto": true, "count": 1 }
}
"#;

    let schema = signal_topology::ddl::compile(src).expect("reaction with payload must compile");
    assert_eq!(schema.reactions.len(), 1);
    assert_eq!(
        schema.reactions[0].guard,
        Some("payload.auto == true".to_string())
    );
    assert_eq!(
        schema.reactions[0].payload,
        Some(serde_json::json!({"auto": true, "count": 1 })),
        "the static payload must reach ReactionDef.payload"
    );
}

#[test]
fn gate_flow_wildcard_from_end_to_end() {
    // M34: the gate_flow.ddl writes the reset as a single `on reset from * ->
    // closed`. After compiling, that must lower to three `reset` transitions
    // (one per source state, including the `closed -> closed` self-loop), and
    // the full scenario must replay with the expected guard block and final
    // state `closed`.
    let src = std::fs::read_to_string("examples/scenarios/gate_flow/gate_flow.ddl")
        .expect("gate_flow.ddl should exist");
    let schema = signal_topology::ddl::compile(&src).unwrap();

    // The wildcard `reset` transition (from == "*") expanded to exactly three:
    // closed/open/fault -> closed.
    let reset_transitions: Vec<_> = schema
        .transitions
        .iter()
        .filter(|t| t.event == "reset" && t.from != "*")
        .collect();
    assert_eq!(
        reset_transitions.len(),
        3,
        "from * should expand to 3 reset transitions (with self-loop)"
    );
    let reset_froms: std::collections::HashSet<_> =
        reset_transitions.iter().map(|t| t.from.as_str()).collect();
    assert_eq!(
        reset_froms,
        ["closed", "open", "fault"].into_iter().collect()
    );

    // Every expanded reset arm shares the same actions/guard (identity check).
    for t in &reset_transitions {
        assert_eq!(t.actions.on_transition, vec!["clear_fault_safely"]);
        assert_eq!(t.actions.on_enter, vec!["log_reset"]);
        assert_eq!(t.to, "closed");
    }

    // Replay the scenario through the engine end-to-end.
    let action_ids = signal_topology::run::collect_action_ids(&schema);
    let mut engine = TopologyEngine::from_schema(schema).unwrap();
    for id in &action_ids {
        engine.register_action(id, |_| Ok(()));
    }

    // open
    let res = engine.send_event("gate", "open", None).unwrap();
    assert_eq!(res.executed_actions, vec!["activate_motor", "log_gate_open"]);
    assert_eq!(engine.get_state("gate").unwrap(), "open");

    // fault({emergency:false}) -> guard blocked
    let err = engine
        .send_event("gate", "fault", Some(serde_json::json!({"emergency": false})))
        .unwrap_err();
    assert!(matches!(err, signal_topology::EngineError::GuardBlocked { .. }));
    assert_eq!(engine.get_state("gate").unwrap(), "open");

    // fault({emergency:true}) -> open -> fault, multi on_transition actions
    let res = engine
        .send_event("gate", "fault", Some(serde_json::json!({"emergency": true})))
        .unwrap();
    assert_eq!(res.executed_actions, vec!["engage_brake", "engage_backup_brake", "log_fault"]);
    assert_eq!(engine.get_state("gate").unwrap(), "fault");

    // reset from fault -> closed (wildcard arm)
    engine.send_event("gate", "reset", None).unwrap();
    assert_eq!(engine.get_state("gate").unwrap(), "closed");

    // open -> close (sanity)
    engine.send_event("gate", "open", None).unwrap();
    assert_eq!(engine.get_state("gate").unwrap(), "open");
    engine.send_event("gate", "close", None).unwrap();
    assert_eq!(engine.get_state("gate").unwrap(), "closed");

    // reset from closed -> closed (self-loop proves * is live)
    let res = engine.send_event("gate", "reset", None).unwrap();
    assert_eq!(res.from, "closed");
    assert_eq!(res.to, "closed");
    assert_eq!(engine.get_state("gate").unwrap(), "closed");
}

#[test]
fn syntax_error_reports_line_number() {
    let src = r#"
signal s {
    states: [a, b]
    initial: a

    on go from a b
}
"#;
    let err = signal_topology::ddl::compile(src).unwrap_err();
    let msg = err.to_string();
    // The missing `->` is on line 6 of the source.
    assert!(
        msg.contains("line 6"),
        "error should point at line 6, got: {}",
        msg
    );
}

#[test]
fn duplicate_signal_reports_error() {
    let src = r#"
signal dup {
    states: [a]
    initial: a
}
signal dup {
    states: [b]
    initial: b
}
"#;
    let err = signal_topology::ddl::compile(src).unwrap_err();
    assert!(
        err.to_string().contains("duplicate signal"),
        "got: {}",
        err
    );
}

// ---------------------------------------------------------------------------
// M44: fork/join DDL syntax.
// ---------------------------------------------------------------------------

/// Collect the `StateChanged` events in trace order, as `(signal, to)`.
fn state_changed(events: &[TraceEvent]) -> Vec<(String, String)> {
    events
        .iter()
        .filter_map(|e| match e {
            TraceEvent::StateChanged { signal_id, to, .. } => {
                Some((signal_id.clone(), to.clone()))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn fork_block_assigns_join_group_to_members() {
    let ddl = r#"
signal A {
    states: [a0, a1]
    initial: a0
    on go from a0 -> a1
}
signal B {
    states: [b0, b1]
    initial: b0
    on react from b0 -> b1
}
signal C {
    states: [c0, c1]
    initial: c0
    on react from c0 -> c1
}

fork {
    when A enters a1 -> B react
    when A enters a1 -> C react
}
"#;

    let schema = signal_topology::ddl::compile(ddl).expect("fork should compile");
    assert_eq!(schema.reactions.len(), 2);
    for r in &schema.reactions {
        assert_eq!(
            r.join_group,
            Some("fork0".to_string()),
            "fork members must share the auto-named group"
        );
        assert!(r.requires.is_empty(), "fork members have no requires");
    }
}

#[test]
fn join_block_sets_requires_on_members() {
    let ddl = r#"
signal A {
    states: [a0, a1]
    initial: a0
    on go from a0 -> a1
}
signal B {
    states: [b0, b1]
    initial: b0
    on react from b0 -> b1
}
signal C {
    states: [c0, c1]
    initial: c0
    on react from c0 -> c1
}
signal D {
    states: [d0, d1]
    initial: d0
    on react from d0 -> d1
}

fork {
    when A enters a1 -> B react
    when A enters a1 -> C react
}
join fork0 {
    when A enters a1 -> D react
}
"#;

    let schema = signal_topology::ddl::compile(ddl).expect("fork+join should compile");
    assert_eq!(schema.reactions.len(), 3);
    let d = schema
        .reactions
        .iter()
        .find(|r| r.to_signal == "D")
        .expect("D reaction must exist");
    assert_eq!(d.requires, vec!["fork0".to_string()]);
    assert!(d.join_group.is_none(), "join member belongs to no fork group");
}

#[test]
fn join_references_undefined_fork_group_is_error() {
    let ddl = r#"
signal A {
    states: [a0, a1]
    initial: a0
    on go from a0 -> a1
}
signal D {
    states: [d0, d1]
    initial: d0
    on react from d0 -> d1
}

join no_such_group {
    when A enters a1 -> D react
}
"#;

    let err = signal_topology::ddl::compile(ddl).unwrap_err();
    assert!(
        err.to_string().contains("undefined fork group 'no_such_group'"),
        "got: {}",
        err
    );
}

#[test]
fn fork_join_end_to_end_join_waits_for_group() {
    let ddl = r#"
signal A {
    states: [a0, a1]
    initial: a0
    on go from a0 -> a1
}
signal B {
    states: [b0, b1]
    initial: b0
    on react from b0 -> b1
}
signal C {
    states: [c0, c1]
    initial: c0
    on react from c0 -> c1
}
signal D {
    states: [d0, d1]
    initial: d0
    on react from d0 -> d1
}

fork {
    when A enters a1 -> B react
    when A enters a1 -> C react
}
join fork0 {
    when A enters a1 -> D react
}
"#;

    let mut engine = engine_from_ddl(ddl);
    engine.send_event("A", "go", None).expect("main transition commits");

    assert_eq!(engine.get_state("A").unwrap(), "a1");
    assert_eq!(engine.get_state("B").unwrap(), "b1");
    assert_eq!(engine.get_state("C").unwrap(), "c1");
    assert_eq!(engine.get_state("D").unwrap(), "d1");

    // D fires only after both B and C have changed.
    let changed = state_changed(engine.traces());
    let pos = |sig: &str| changed.iter().position(|(s, _)| s == sig).unwrap();
    assert!(pos("D") > pos("B"));
    assert!(pos("D") > pos("C"));
}

#[test]
fn fork_with_end_to_end_fires_members() {
    let ddl = r#"
signal A {
    states: [a0, a1]
    initial: a0
    on go from a0 -> a1
}
signal B {
    states: [b0, b1]
    initial: b0
    on react from b0 -> b1
}
signal C {
    states: [c0, c1]
    initial: c0
    on react from c0 -> c1
}

fork {
    when A enters a1 -> B react
    when A enters a1 -> C react
}
"#;

    let mut engine = engine_from_ddl(ddl);
    engine.send_event("A", "go", None).expect("main transition commits");
    assert_eq!(engine.get_state("B").unwrap(), "b1");
    assert_eq!(engine.get_state("C").unwrap(), "c1");
}

// ---------------------------------------------------------------------------
// M45: sub-topology component with ports + wired instantiation, end-to-end.
// ---------------------------------------------------------------------------

#[test]
fn sub_topology_component_via_ports_end_to_end() {
    // A "lockable" sub-topology exposes its internal `lock.locked` state via an
    // `out` port aliased `locked`. The parent (`house`) instantiates it and
    // wires that port to the parent-level signal `door`. After compilation +
    // expansion, the parent's own reaction may react to `door` directly — the
    // component's exposed signal has been renamed into the parent namespace.
    let ddl = r#"
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

reaction {
    when door enters locked -> controller notify
}

instantiate lockable as door with {} connect { locked -> door }
"#;

    let schema = signal_topology::ddl::compile(ddl).expect("DDL should compile");
    assert!(
        schema.components.is_none(),
        "compile() flattens components"
    );
    assert!(schema.instances.is_empty(), "compile() flattens instances");

    let mut engine = TopologyEngine::from_schema(schema).expect("engine builds");

    assert_eq!(engine.get_state("controller").unwrap(), "idle");
    assert_eq!(engine.get_state("door").unwrap(), "unlocked");

    // Lock the (wired) `door` signal via the component's `lock` transition.
    engine
        .send_event("door", "lock", None)
        .expect("lock should succeed");
    assert_eq!(engine.get_state("door").unwrap(), "locked");

    // Parent reaction fires: door=locked -> controller notify -> alerted.
    assert_eq!(
        engine.get_state("controller").unwrap(),
        "alerted",
        "component's exposed signal should drive the parent reaction"
    );

    // Reset the controller independently; back to idle.
    engine.send_event("controller", "reset", None).expect("reset");
    assert_eq!(engine.get_state("controller").unwrap(), "idle");
}

#[test]
fn sub_topology_internal_reaction_wired_to_parent_end_to_end() {
    // A component's *internal* reaction targets an exposed port signal. After
    // wiring, that reaction must fire against the parent signal it was wired
    // to — the renamed-from identity.
    let ddl = r#"
signal audit {
    states: [idle, noted]
    initial: idle
    on note from idle -> noted
}

component flag {
    port out flag.set as flag_set
    signal flag {
        states: [clear, set]
        initial: clear
        on raise from clear -> set
    }
    reaction {
        when flag enters set -> audit note
    }
}

instantiate flag as alarm with {} connect { flag_set -> alarm }
"#;

    let mut engine = engine_from_ddl(ddl);

    // Raising the wired `alarm` signal (the component's internal `flag`) sends
    // `set` to `alarm`; the component's internal reaction reacts to
    // `alarm.set` and cascades into `audit`.
    engine
        .send_event("alarm", "raise", None)
        .expect("raise should succeed");
    assert_eq!(engine.get_state("alarm").unwrap(), "set");
    assert_eq!(
        engine.get_state("audit").unwrap(),
        "noted",
        "wired internal reaction should cascade into the parent"
    );
}

