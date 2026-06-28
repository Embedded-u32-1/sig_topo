use signal_topology::{EngineError, TopologyEngine, TraceEvent};

#[test]
fn test_single_cascade() {
    let json = r#"{
        "version": "0.1",
        "signals": [
            {"id": "A", "initial_state": "a0", "states": ["a0", "a1"]},
            {"id": "B", "initial_state": "b0", "states": ["b0", "b1"]}
        ],
        "transitions": [
            {"signal_id": "A", "from": "a0", "event": "go", "to": "a1"},
            {"signal_id": "B", "from": "b0", "event": "react", "to": "b1"}
        ],
        "reactions": [
            {"from_signal": "A", "from_state": "a1", "to_signal": "B", "event": "react"}
        ]
    }"#;

    let mut engine = TopologyEngine::from_json(json).expect("Should load topology");
    let result = engine.send_event("A", "go", None).expect("Should transition");

    assert_eq!(result.signal_id, "A");
    assert_eq!(result.from, "a0");
    assert_eq!(result.to, "a1");
    assert_eq!(engine.get_state("A").unwrap(), "a1");
    assert_eq!(engine.get_state("B").unwrap(), "b1");
}

#[test]
fn test_multi_level_cascade() {
    let json = r#"{
        "version": "0.1",
        "signals": [
            {"id": "A", "initial_state": "a0", "states": ["a0", "a1"]},
            {"id": "B", "initial_state": "b0", "states": ["b0", "b1"]},
            {"id": "C", "initial_state": "c0", "states": ["c0", "c1"]}
        ],
        "transitions": [
            {"signal_id": "A", "from": "a0", "event": "go", "to": "a1"},
            {"signal_id": "B", "from": "b0", "event": "react", "to": "b1"},
            {"signal_id": "C", "from": "c0", "event": "react", "to": "c1"}
        ],
        "reactions": [
            {"from_signal": "A", "from_state": "a1", "to_signal": "B", "event": "react"},
            {"from_signal": "B", "from_state": "b1", "to_signal": "C", "event": "react"}
        ]
    }"#;

    let mut engine = TopologyEngine::from_json(json).expect("Should load topology");
    let result = engine.send_event("A", "go", None).expect("Should transition");

    assert_eq!(result.to, "a1");
    assert_eq!(engine.get_state("A").unwrap(), "a1");
    assert_eq!(engine.get_state("B").unwrap(), "b1");
    assert_eq!(engine.get_state("C").unwrap(), "c1");
}

#[test]
fn test_cascade_depth_limit() {
    let json = r#"{
        "version": "0.1",
        "signals": [
            {"id": "A", "initial_state": "a0", "states": ["a0", "a1"]}
        ],
        "transitions": [
            {"signal_id": "A", "from": "a0", "event": "tick", "to": "a1"},
            {"signal_id": "A", "from": "a1", "event": "tick", "to": "a1"}
        ],
        "reactions": [
            {"from_signal": "A", "from_state": "a1", "to_signal": "A", "event": "tick"}
        ]
    }"#;

    let mut engine = TopologyEngine::from_json(json).expect("Should load topology");
    engine.set_max_cascade_depth(2);

    let result = engine.send_event("A", "tick", None);
    assert!(matches!(result, Err(EngineError::CascadeDepthExceeded)));
    assert_eq!(engine.get_state("A").unwrap(), "a1");
}

#[test]
fn test_cascade_failure_does_not_revert_original() {
    let json = r#"{
        "version": "0.1",
        "signals": [
            {"id": "A", "initial_state": "a0", "states": ["a0", "a1"]},
            {"id": "B", "initial_state": "b0", "states": ["b0", "b1"]}
        ],
        "transitions": [
            {"signal_id": "A", "from": "a0", "event": "go", "to": "a1"},
            {"signal_id": "B", "from": "b0", "event": "known", "to": "b1"}
        ],
        "reactions": [
            {"from_signal": "A", "from_state": "a1", "to_signal": "B", "event": "missing"}
        ]
    }"#;

    let mut engine = TopologyEngine::from_json(json).expect("Should load topology");
    let result = engine.send_event("A", "go", None);

    assert!(matches!(
        result,
        Err(EngineError::TransitionNotFound { signal, event })
            if signal == "B" && event == "missing"
    ));
    assert_eq!(engine.get_state("A").unwrap(), "a1");
    assert_eq!(engine.get_state("B").unwrap(), "b0");
}

#[test]
fn test_cascade_uses_reaction_payload() {
    let json = r#"{
        "version": "0.1",
        "signals": [
            {"id": "A", "initial_state": "a0", "states": ["a0", "a1"]},
            {"id": "B", "initial_state": "b0", "states": ["b0", "b1"]}
        ],
        "transitions": [
            {"signal_id": "A", "from": "a0", "event": "go", "to": "a1"},
            {
                "signal_id": "B",
                "from": "b0",
                "event": "process",
                "to": "b1",
                "guard": "payload.amount > 50"
            }
        ],
        "reactions": [
            {
                "from_signal": "A",
                "from_state": "a1",
                "to_signal": "B",
                "event": "process",
                "payload": {"amount": 100}
            }
        ]
    }"#;

    let mut engine = TopologyEngine::from_json(json).expect("Should load topology");
    let result = engine.send_event("A", "go", None).expect("Should transition");

    assert_eq!(result.to, "a1");
    assert_eq!(engine.get_state("A").unwrap(), "a1");
    assert_eq!(engine.get_state("B").unwrap(), "b1");
}

#[test]
fn test_cascade_wildcard_from_state() {
    let json = r#"{
        "version": "0.1",
        "signals": [
            {"id": "A", "initial_state": "a0", "states": ["a0", "a1", "a2"]},
            {"id": "B", "initial_state": "b0", "states": ["b0", "b1", "b2"]}
        ],
        "transitions": [
            {"signal_id": "A", "from": "a0", "event": "go", "to": "a1"},
            {"signal_id": "A", "from": "a1", "event": "go2", "to": "a2"},
            {"signal_id": "B", "from": "b0", "event": "react", "to": "b1"},
            {"signal_id": "B", "from": "b1", "event": "react", "to": "b2"}
        ],
        "reactions": [
            {"from_signal": "A", "from_state": "*", "to_signal": "B", "event": "react"}
        ]
    }"#;

    let mut engine = TopologyEngine::from_json(json).expect("Should load topology");
    engine.send_event("A", "go", None).expect("Should transition");

    assert_eq!(engine.get_state("A").unwrap(), "a1");
    assert_eq!(engine.get_state("B").unwrap(), "b1");

    engine.send_event("A", "go2", None).expect("Should transition");
    assert_eq!(engine.get_state("A").unwrap(), "a2");
    assert_eq!(engine.get_state("B").unwrap(), "b2");
}

#[test]
fn test_cascade_records_event_received_trace() {
    let json = r#"{
        "version": "0.1",
        "signals": [
            {"id": "A", "initial_state": "a0", "states": ["a0", "a1"]},
            {"id": "B", "initial_state": "b0", "states": ["b0", "b1"]}
        ],
        "transitions": [
            {"signal_id": "A", "from": "a0", "event": "go", "to": "a1"},
            {"signal_id": "B", "from": "b0", "event": "react", "to": "b1"}
        ],
        "reactions": [
            {"from_signal": "A", "from_state": "a1", "to_signal": "B", "event": "react"}
        ]
    }"#;

    let mut engine = TopologyEngine::from_json(json).expect("Should load topology");
    engine.send_event("A", "go", None).expect("Should transition");

    let b_events: Vec<_> = engine
        .traces_for("B")
        .into_iter()
        .filter(|e| matches!(e, TraceEvent::EventReceived { .. }))
        .collect();
    assert_eq!(b_events.len(), 1);
    assert_eq!(b_events[0].signal_id(), "B");
}

#[test]
fn test_reaction_validation_fails_for_unknown_signal() {
    let json = r#"{
        "version": "0.1",
        "signals": [
            {"id": "A", "initial_state": "a0", "states": ["a0", "a1"]}
        ],
        "transitions": [],
        "reactions": [
            {"from_signal": "A", "from_state": "a1", "to_signal": "B", "event": "react"}
        ]
    }"#;

    let result = TopologyEngine::from_json(json);
    assert!(matches!(
        result,
        Err(EngineError::ReactionSignalNotFound(signal)) if signal == "B"
    ));
}
