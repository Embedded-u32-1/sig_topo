use signal_topology::{EngineError, TopologyEngine};
use serde_json::json;

fn make_guarded_topology(guard: &str) -> String {
    format!(
        r#"{{
      "version": "0.1",
      "signals": [
        {{"id": "payment", "initial_state": "pending", "states": ["pending", "processed", "rejected"]}}
      ],
      "transitions": [
        {{
          "signal_id": "payment",
          "from": "pending",
          "event": "process",
          "to": "processed",
          "guard": "{}",
          "actions": {{"on_enter": ["mark_processed"]}}
        }}
      ]
    }}"#,
        guard
    )
}

#[test]
fn test_guard_passes_allows_transition() {
    let json = make_guarded_topology("payload.amount > 0");
    let mut engine = TopologyEngine::from_json(&json).expect("Should load topology");
    engine.register_action("mark_processed", |_| Ok(()));

    let result = engine
        .send_event("payment", "process", Some(json!({"amount": 100})))
        .expect("Transition should succeed");

    assert_eq!(result.from, "pending");
    assert_eq!(result.to, "processed");
    assert_eq!(engine.get_state("payment").unwrap(), "processed");
}

#[test]
fn test_guard_blocks_transition_state_unchanged() {
    let json = make_guarded_topology("payload.amount > 0");
    let mut engine = TopologyEngine::from_json(&json).expect("Should load topology");
    engine.register_action("mark_processed", |_| Ok(()));

    let result = engine.send_event("payment", "process", Some(json!({"amount": 0})));

    assert!(matches!(
        result,
        Err(EngineError::GuardBlocked {
            signal,
            event,
            guard,
        }) if signal == "payment" && event == "process" && guard == "payload.amount > 0"
    ));
    assert_eq!(engine.get_state("payment").unwrap(), "pending");
}

#[test]
fn test_guard_evaluates_string_equality_and_logical() {
    let json = make_guarded_topology("payload.currency == 'USD' and payload.amount > 0");
    let mut engine = TopologyEngine::from_json(&json).expect("Should load topology");
    engine.register_action("mark_processed", |_| Ok(()));

    let blocked = engine
        .send_event(
            "payment",
            "process",
            Some(json!({"currency": "EUR", "amount": 100})),
        )
        .expect_err("Should be blocked");
    assert!(matches!(blocked, EngineError::GuardBlocked { .. }));
    assert_eq!(engine.get_state("payment").unwrap(), "pending");

    let result = engine
        .send_event(
            "payment",
            "process",
            Some(json!({"currency": "USD", "amount": 100})),
        )
        .expect("Transition should succeed");
    assert_eq!(result.to, "processed");
}

#[test]
fn test_guard_missing_field_treats_as_null() {
    let json = make_guarded_topology("payload.amount > 0");
    let mut engine = TopologyEngine::from_json(&json).expect("Should load topology");
    engine.register_action("mark_processed", |_| Ok(()));

    let result = engine.send_event("payment", "process", Some(json!({})));

    assert!(matches!(result, Err(EngineError::GuardBlocked { .. })));
    assert_eq!(engine.get_state("payment").unwrap(), "pending");
}

#[test]
fn test_guard_syntax_error_returns_evaluation_error() {
    let json = make_guarded_topology("payload.amount @ 0");
    let mut engine = TopologyEngine::from_json(&json).expect("Should load topology");

    let result = engine.send_event("payment", "process", Some(json!({"amount": 100})));

    assert!(matches!(result, Err(EngineError::GuardEvaluationError(_))));
    assert_eq!(engine.get_state("payment").unwrap(), "pending");
}

#[test]
fn test_no_guard_behavior_unchanged() {
    let json = r#"{
      "version": "0.1",
      "signals": [
        {"id": "task", "initial_state": "idle", "states": ["idle", "running"]}
      ],
      "transitions": [
        {
          "signal_id": "task",
          "from": "idle",
          "event": "start",
          "to": "running",
          "actions": {"on_enter": ["log_start"]}
        }
      ]
    }"#;
    let mut engine = TopologyEngine::from_json(json).expect("Should load topology");
    engine.register_action("log_start", |_| Ok(()));

    let result = engine.send_event("task", "start", None).expect("Should transition");
    assert_eq!(result.to, "running");
    assert_eq!(engine.get_state("task").unwrap(), "running");
}
