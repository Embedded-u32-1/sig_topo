use crate::error::EngineError;
use crate::guard::eval_guard;
use crate::schema::{ReactionDef, SignalDef, TopologySchema, TransitionDef};
use crate::trace::{now_ms, TraceEvent, TraceLog};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

/// The context passed to every action callback during a transition.
///
/// Lifecycle actions at all three phases (`on_exit`, `on_transition`, `on_enter`)
/// receive the same `ActionContext` describing the transition being attempted.
#[derive(Debug, Clone)]
pub struct ActionContext {
    /// The signal being transitioned.
    pub signal_id: String,
    /// The source state the signal is leaving.
    pub from_state: String,
    /// The target state the signal is entering.
    pub to_state: String,
    /// The event name that triggered the transition.
    pub event: String,
    /// The event payload, if any. `None` for events sent without a payload.
    pub payload: Option<Value>,
}

/// The outcome of a successful transition, returned by `send_event`.
#[derive(Debug)]
pub struct TransitionResult {
    /// The signal that transitioned.
    pub signal_id: String,
    /// The source state the signal left.
    pub from: String,
    /// The target state the signal entered.
    pub to: String,
    /// The action ids that ran, in lifecycle order (`on_exit` → `on_transition`
    /// → `on_enter`). Includes only the actions that were reached before any
    /// failure (a failing action short-circuits the rest).
    pub executed_actions: Vec<String>,
}

type ActionFn = Box<dyn FnMut(ActionContext) -> Result<(), EngineError> + 'static>;

/// A validated, runnable signal-topology engine.
///
/// Build one with `from_json` / `from_schema` (or `TopologyEngine`-free
/// `load_topology` / `from_path` in `crate::compose`), register action
/// implementations with `register_action`, then drive signals through transitions
/// with `send_event`. Every run appends to an internal trace log readable via
/// `traces` / `traces_for`.
pub struct TopologyEngine {
    pub(crate) signals: HashMap<String, SignalState>,
    pub(crate) transitions: Vec<TransitionDef>,
    pub(crate) reactions: Vec<ReactionDef>,
    actions: HashMap<String, ActionFn>,
    trace: TraceLog,
    max_cascade_depth: usize,
}

pub(crate) struct SignalState {
    pub(crate) current: String,
    pub(crate) states: Vec<String>,
    // Captured from the schema at construction so a runtime snapshot can
    // recover the static initial-state highlight without re-holding the full
    // schema. Populated in `from_schema` / `reload_topology`.
    pub(crate) initial_state: String,
}

impl TopologyEngine {
    /// Parse a JSON topology and build a ready-to-run engine.
    ///
    /// The JSON is deserialized into a `TopologySchema`, expanded (components /
    /// instances / includes) and validated before the engine is constructed.
    /// Returns `EngineError::ParseError` on malformed JSON or a validation error
    /// on an invalid topology.
    pub fn from_json(json_str: &str) -> Result<Self, EngineError> {
        let schema: TopologySchema =
            serde_json::from_str(json_str).map_err(|e| EngineError::ParseError(e.to_string()))?;
        Self::from_schema(schema)
    }

    /// Build an engine from an already-deserialized `TopologySchema`.
    ///
    /// Like `from_json` but takes the schema directly, useful when the topology
    /// is constructed programmatically. The schema is expanded and validated.
    pub fn from_schema(schema: TopologySchema) -> Result<Self, EngineError> {
        // Expand parameterized components/instances into a flat schema before
        // validation. When `instances` is empty this is a no-op pass-through,
        // so legacy flat JSON keeps working unchanged.
        let schema = crate::compose::expand(schema)?;

        Self::validate(&schema)?;

        let mut signals = HashMap::new();
        for sig in &schema.signals {
            signals.insert(
                sig.id.clone(),
                SignalState {
                    current: sig.initial_state.clone(),
                    states: sig.states.clone(),
                    initial_state: sig.initial_state.clone(),
                },
            );
        }

        Ok(TopologyEngine {
            signals,
            transitions: schema.transitions,
            reactions: schema.reactions,
            actions: HashMap::new(),
            trace: TraceLog::default(),
            max_cascade_depth: 8,
        })
    }

    /// Validate a `TopologySchema` without building an engine.
    ///
    /// Checks for duplicate signal ids, unknown signal references in transitions
    /// and reactions, and `from`/`to`/`initial_state` values that are not
    /// members of the signal's `states` list. `from` may be the wildcard `*`.
    pub fn validate(schema: &TopologySchema) -> Result<(), EngineError> {
        let mut signal_ids = HashSet::new();
        for sig in &schema.signals {
            if !signal_ids.insert(&sig.id) {
                return Err(EngineError::ValidationError(format!(
                    "Duplicate signal id: {}",
                    sig.id
                )));
            }

            if !sig.states.contains(&sig.initial_state) {
                return Err(EngineError::ValidationError(format!(
                    "Invalid initial_state '{}' for signal '{}'",
                    sig.initial_state, sig.id
                )));
            }
        }

        for (i, trans) in schema.transitions.iter().enumerate() {
            if !signal_ids.contains(&trans.signal_id) {
                return Err(EngineError::ValidationError(format!(
                    "Transition {} references unknown signal '{}'",
                    i, trans.signal_id
                )));
            }

            let signal = schema
                .signals
                .iter()
                .find(|s| s.id == trans.signal_id)
                .unwrap();
            if trans.from != "*" && !signal.states.contains(&trans.from) {
                return Err(EngineError::ValidationError(format!(
                    "Transition {} references invalid 'from' state '{}' for signal '{}'",
                    i, trans.from, trans.signal_id
                )));
            }
            if !signal.states.contains(&trans.to) {
                return Err(EngineError::ValidationError(format!(
                    "Transition {} references invalid 'to' state '{}' for signal '{}'",
                    i, trans.to, trans.signal_id
                )));
            }
        }

        for reaction in &schema.reactions {
            if !signal_ids.contains(&reaction.from_signal) {
                return Err(EngineError::ReactionSignalNotFound(
                    reaction.from_signal.clone(),
                ));
            }
            if !signal_ids.contains(&reaction.to_signal) {
                return Err(EngineError::ReactionSignalNotFound(
                    reaction.to_signal.clone(),
                ));
            }
        }

        Ok(())
    }

    /// Register an action callback under `action_id`.
    ///
    /// Each action id referenced by a transition's lifecycle phases must be
    /// registered before `send_event` runs that transition, otherwise the
    /// transition fails with `EngineError::ActionNotFound`. Registering the same
    /// id twice overwrites the previous callback.
    pub fn register_action<F>(&mut self, action_id: &str, f: F)
    where
        F: FnMut(ActionContext) -> Result<(), EngineError> + 'static,
    {
        self.actions
            .insert(action_id.to_string(), Box::new(f));
    }

    /// Set the maximum cascade depth (default 8).
    ///
    /// When a transition commits, matching reactions fire derived events
    /// recursively; this limit bounds that recursion. Exceeding it returns
    /// `EngineError::CascadeDepthExceeded`.
    pub fn set_max_cascade_depth(&mut self, depth: usize) {
        self.max_cascade_depth = depth;
    }

    fn run_action(
        trace: &mut TraceLog,
        actions: &mut HashMap<String, ActionFn>,
        signal_id: &str,
        action_id: &str,
        ctx: ActionContext,
    ) -> Result<(), EngineError> {
        trace.push(TraceEvent::ActionStarted {
            signal_id: signal_id.to_string(),
            action_id: action_id.to_string(),
            timestamp_ms: now_ms(),
        });
        let action = actions
            .get_mut(action_id)
            .ok_or_else(|| EngineError::ActionNotFound(action_id.to_string()))?;
        action(ctx).map_err(|e| {
            let msg = match &e {
                EngineError::ActionExecutionError(m) => m.clone(),
                _ => e.to_string(),
            };
            trace.push(TraceEvent::ActionFailed {
                signal_id: signal_id.to_string(),
                action_id: action_id.to_string(),
                timestamp_ms: now_ms(),
                error: msg.clone(),
            });
            EngineError::ActionExecutionError(msg)
        })?;
        trace.push(TraceEvent::ActionSucceeded {
            signal_id: signal_id.to_string(),
            action_id: action_id.to_string(),
            timestamp_ms: now_ms(),
        });
        Ok(())
    }

    /// Send an event to a signal, attempting the matching transition.
    ///
    /// The transition is chosen by `(signal_id, event)` matching the signal's
    /// current state (or the wildcard `*`). A `when` guard, if present, is
    /// evaluated against the payload and blocks the transition on `false`
    /// (`EngineError::GuardBlocked`). On a match, the three lifecycle phases run
    /// in order; any action failure rolls the signal back to its source state and
    /// returns the error. On success, matching reactions are fired recursively
    /// (bounded by `max_cascade_depth`) and a `TransitionResult` is returned.
    pub fn send_event(
        &mut self,
        signal_id: &str,
        event: &str,
        payload: Option<Value>,
    ) -> Result<TransitionResult, EngineError> {
        // The top-level event is its own "parent" — a reaction reacting to this
        // transition evaluates its guard against this same payload (see
        // send_event_internal).
        self.send_event_internal(signal_id, event, payload.clone(), payload, 0)
    }

    fn send_event_internal(
        &mut self,
        signal_id: &str,
        event: &str,
        payload: Option<Value>,
        // The payload of the event that caused *this* transition. A reaction
        // reacting to this transition evaluates its guard against
        // `parent_payload` (the source event's payload), not the reaction's
        // static payload — mirroring how a transition guard reads its own
        // event's payload (M32).
        parent_payload: Option<Value>,
        depth: usize,
    ) -> Result<TransitionResult, EngineError> {
        if depth > self.max_cascade_depth {
            return Err(EngineError::CascadeDepthExceeded);
        }

        self.trace.push(TraceEvent::EventReceived {
            signal_id: signal_id.to_string(),
            event: event.to_string(),
            timestamp_ms: now_ms(),
            payload: payload.as_ref().map(|v| v.to_string()),
        });

        let signal = self
            .signals
            .get_mut(signal_id)
            .ok_or_else(|| EngineError::SignalNotFound(signal_id.to_string()))?;

        let transition = self
            .transitions
            .iter()
            .find(|t| {
                t.signal_id == signal_id
                    && t.event == event
                    && (t.from == signal.current || t.from == "*")
            })
            .ok_or_else(|| EngineError::TransitionNotFound {
                signal: signal_id.to_string(),
                event: event.to_string(),
            })?;

        let from_state = signal.current.clone();
        let to_state = transition.to.clone();

        let ctx = ActionContext {
            signal_id: signal_id.to_string(),
            from_state: from_state.clone(),
            to_state: to_state.clone(),
            event: event.to_string(),
            payload: payload.clone(),
        };

        if let Some(guard) = &transition.guard {
            match eval_guard(guard, &ctx) {
                Ok(true) => {}
                Ok(false) => {
                    return Err(EngineError::GuardBlocked {
                        signal: signal_id.to_string(),
                        event: event.to_string(),
                        guard: guard.clone(),
                    });
                }
                Err(msg) => return Err(EngineError::GuardEvaluationError(msg)),
            }
        }

        let mut executed_actions = Vec::new();

        for action_id in &transition.actions.on_exit {
            Self::run_action(
                &mut self.trace,
                &mut self.actions,
                signal_id,
                action_id,
                ctx.clone(),
            )?;
            executed_actions.push(action_id.clone());
        }

        // Tentatively commit the target state so that lifecycle actions read a
        // consistent `signal.current`, but keep the source state so we can roll
        // back if any on_transition / on_enter action fails. The trace
        // `StateChanged` and `Rollbacked` events are the durable record of which
        // path was taken.
        let old_state = signal.current.clone();
        signal.current = to_state.clone();

        // Run on_transition / on_enter, capturing the first failure. The action
        // lifecycle inside `run_action` already pushes `ActionFailed` to the
        // trace before we return here, so the failure is observable regardless.
        let mut transition_error = None;

        for action_id in &transition.actions.on_transition {
            if let Err(e) = Self::run_action(
                &mut self.trace,
                &mut self.actions,
                signal_id,
                action_id,
                ctx.clone(),
            ) {
                transition_error = Some(e);
                break;
            }
            executed_actions.push(action_id.clone());
        }

        if transition_error.is_none() {
            for action_id in &transition.actions.on_enter {
                if let Err(e) = Self::run_action(
                    &mut self.trace,
                    &mut self.actions,
                    signal_id,
                    action_id,
                    ctx.clone(),
                ) {
                    transition_error = Some(e);
                    break;
                }
                executed_actions.push(action_id.clone());
            }
        }

        if let Some(e) = transition_error {
            // Roll back to the source state and record the rollback. External
            // action side effects (IO, logging) are irreversible — this is an
            // inherent limitation of business actions; the trace keeps
            // `ActionFailed` + this `Rollbacked` for debugging.
            signal.current = old_state.clone();
            self.trace.push(TraceEvent::Rollbacked {
                signal_id: signal_id.to_string(),
                from: to_state,
                to: old_state,
                timestamp_ms: now_ms(),
            });
            return Err(e);
        }

        // All lifecycle actions succeeded: emit the durable state-change record.
        self.trace.push(TraceEvent::StateChanged {
            signal_id: signal_id.to_string(),
            from: from_state.clone(),
            to: to_state.clone(),
            timestamp_ms: now_ms(),
        });

        let result = TransitionResult {
            signal_id: signal_id.to_string(),
            from: from_state,
            to: to_state,
            executed_actions,
        };

        let matching_reactions: Vec<ReactionDef> = self
            .reactions
            .iter()
            .filter(|r| {
                r.from_signal == signal_id
                    && (r.from_state == result.to || r.from_state == "*")
            })
            .cloned()
            .collect();

        for reaction in matching_reactions {
            // A reaction guard gates the cascade. It is evaluated against the
            // *source* event's payload (`parent_payload`) — the payload of the
            // event that triggered the transition this reaction reacts to — so
            // a reaction can be gated on the data that caused it. This mirrors
            // how a transition guard reads its own event's payload. A guard
            // that is false, or that fails to evaluate, skips *this* reaction
            // only — it never rolls back the already-committed transition and
            // never aborts the remaining reactions. This keeps a single bad
            // guard from breaking the whole cascade chain (see M32).
            if let Some(guard_str) = &reaction.guard {
                let guard_ctx = ActionContext {
                    signal_id: reaction.to_signal.clone(),
                    from_state: reaction.from_state.clone(),
                    to_state: self
                        .signals
                        .get(&reaction.to_signal)
                        .map(|s| s.current.clone())
                        .unwrap_or_default(),
                    event: reaction.event.clone(),
                    payload: parent_payload.clone(),
                };
                let guard_result = eval_guard(guard_str, &guard_ctx);
                // M38: record the guard outcome (true / false / error) so the
                // trace shows why this reaction fired or was skipped. The
                // `result` string is "true", "false", or "error: <msg>".
                let result_str = match &guard_result {
                    Ok(true) => "true".to_string(),
                    Ok(false) => "false".to_string(),
                    Err(msg) => format!("error: {}", msg),
                };
                self.trace.push(TraceEvent::ReactionGuardEvaluated {
                    signal_id: signal_id.to_string(),
                    reaction_from_signal: reaction.from_signal.clone(),
                    reaction_from_state: reaction.from_state.clone(),
                    reaction_to_signal: reaction.to_signal.clone(),
                    reaction_event: reaction.event.clone(),
                    guard: guard_str.clone(),
                    result: result_str,
                    timestamp_ms: now_ms(),
                });
                match guard_result {
                    Ok(true) => {}
                    Ok(false) => continue,
                    Err(_) => continue,
                }
            }

            // The reaction's static payload becomes the derived event's payload
            // (`payload`), and — being the event that drives the next level —
            // also the `parent_payload` any deeper reaction reacts against.
            self.send_event_internal(
                &reaction.to_signal,
                &reaction.event,
                reaction.payload.clone(),
                reaction.payload.clone(),
                depth + 1,
            )?;
        }

        Ok(result)
    }

    /// Return the current state of `signal_id`.
    ///
    /// Returns `EngineError::SignalNotFound` if the signal does not exist.
    pub fn get_state(&self, signal_id: &str) -> Result<&str, EngineError> {
        let signal = self
            .signals
            .get(signal_id)
            .ok_or_else(|| EngineError::SignalNotFound(signal_id.to_string()))?;
        Ok(&signal.current)
    }

    /// Return every signal id in the engine, in arbitrary order.
    pub fn signal_ids(&self) -> Vec<&str> {
        self.signals.keys().map(|s| s.as_str()).collect()
    }

    /// Render the topology as Graphviz DOT with every signal's *current*
    /// state highlighted (see `crate::export::to_dot_with_state`).
    ///
    /// Reconstructs a minimal `TopologySchema` from the engine's runtime
    /// state — the engine does not hold the original schema, only the
    /// flattened transitions/reactions and per-signal `SignalState` (which
    /// carries `states` + `initial_state`). The current states are collected
    /// into the `states` map the renderer expects, so the resulting diagram
    /// shows lightgreen "live" nodes that follow the engine as it transitions.
    pub fn snapshot_dot(&self) -> String {
        let states: HashMap<String, String> = self
            .signals
            .iter()
            .map(|(id, sig)| (id.clone(), sig.current.clone()))
            .collect();

        let signals: Vec<SignalDef> = self
            .signals
            .iter()
            .map(|(id, sig)| SignalDef {
                id: id.clone(),
                initial_state: sig.initial_state.clone(),
                states: sig.states.clone(),
            })
            .collect();

        let schema = TopologySchema {
            // The engine never stored the source version; the renderer does
            // not use it, so a sentinel keeps the field honest.
            version: "snapshot".to_string(),
            signals,
            transitions: self.transitions.clone(),
            reactions: self.reactions.clone(),
            components: None,
            instances: Vec::new(),
            includes: Vec::new(),
        };

        crate::export::to_dot_with_state(&schema, &states)
    }

    /// Replace the engine's topology while preserving current signal states.
    ///
    /// Existing signals keep their current state; new signals start at their
    /// `initial_state`; dropped signals' states are discarded. The new topology
    /// is validated, and on any parse or validation error the engine is left
    /// unchanged and an `EngineError::ReloadError` is returned.
    pub fn reload_topology(&mut self, json_str: &str) -> Result<(), EngineError> {
        let schema: TopologySchema =
            serde_json::from_str(json_str).map_err(|e| EngineError::ReloadError(e.to_string()))?;
        Self::validate(&schema).map_err(|e| EngineError::ReloadError(e.to_string()))?;

        let mut new_signals = HashMap::new();
        for sig in &schema.signals {
            let current = self
                .signals
                .get(&sig.id)
                .map(|s| s.current.clone())
                .unwrap_or_else(|| sig.initial_state.clone());
            new_signals.insert(
                sig.id.clone(),
                SignalState {
                    current,
                    states: sig.states.clone(),
                    initial_state: sig.initial_state.clone(),
                },
            );
        }

        self.signals = new_signals;
        self.transitions = schema.transitions;
        self.reactions = schema.reactions;
        Ok(())
    }

    /// Return every recorded trace event, in order.
    pub fn traces(&self) -> &[TraceEvent] {
        self.trace.events()
    }

    /// Return the trace events involving `signal_id`, in order.
    pub fn traces_for(&self, signal_id: &str) -> Vec<&TraceEvent> {
        self.trace.for_signal(signal_id)
    }

    /// Clear the trace log.
    pub fn clear_traces(&mut self) {
        self.trace.clear();
    }

    /// Collect every action id referenced by the engine's transitions,
    /// deduplicated and sorted for deterministic registration.
    ///
    /// `transitions` is `pub(crate)`, so language-binding crates (C-ABI, WASM)
    /// that live outside this crate cannot scan them directly. This helper
    /// lets those bindings pre-register every action as a no-op, exactly as
    /// `crate::ffi::engine_new` does, without exposing the internals.
    pub fn action_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self
            .transitions
            .iter()
            .flat_map(|t| t.actions.all_actions().into_iter().cloned())
            .collect();
        ids.sort();
        ids.dedup();
        ids
    }

    /// Serialize the entire trace log to a JSON array string.
    ///
    /// `TraceEvent` deliberately has no `serde::Serialize` derive, and the WASM
    /// surface cannot hand-roll it atop `traces()` alone (the trace lives behind
    /// the engine). This helper produces the same representation as
    /// `crate::ffi::trace_to_value`: each element carries `signal_id`,
    /// `timestamp_ms`, a `kind` discriminator, and kind-specific fields. It is
    /// also covered by the unit test `unit_traces_json_well_formed` in this file.
    pub fn traces_json(&self) -> String {
        let events: Vec<Value> = self.trace.events().iter().map(trace_event_to_value).collect();
        Value::from(events).to_string()
    }
}

/// Hand-write a `TraceEvent` to a JSON value, mirroring the representation
/// produced by `crate::ffi::trace_to_value`. Kept here as a private helper so
/// both the FFI surface and the WASM surface agree on the JSON shape without
/// touching the `TraceEvent` type.
fn trace_event_to_value(e: &TraceEvent) -> Value {
    let mut v = serde_json::json!({
        "signal_id": e.signal_id(),
        "timestamp_ms": e.timestamp_ms(),
    });
    let obj = v.as_object_mut().expect("json! object is always an Object");
    match e {
        TraceEvent::EventReceived {
            event, payload, ..
        } => {
            obj.insert("kind".into(), Value::from("EventReceived"));
            obj.insert("event".into(), Value::from(event.clone()));
            obj.insert(
                "payload".into(),
                payload.clone().map(Value::from).unwrap_or(Value::Null),
            );
        }
        TraceEvent::ActionStarted { action_id, .. } => {
            obj.insert("kind".into(), Value::from("ActionStarted"));
            obj.insert("action_id".into(), Value::from(action_id.clone()));
        }
        TraceEvent::ActionSucceeded { action_id, .. } => {
            obj.insert("kind".into(), Value::from("ActionSucceeded"));
            obj.insert("action_id".into(), Value::from(action_id.clone()));
        }
        TraceEvent::ActionFailed {
            action_id, error, ..
        } => {
            obj.insert("kind".into(), Value::from("ActionFailed"));
            obj.insert("action_id".into(), Value::from(action_id.clone()));
            obj.insert("error".into(), Value::from(error.clone()));
        }
        TraceEvent::StateChanged { from, to, .. } => {
            obj.insert("kind".into(), Value::from("StateChanged"));
            obj.insert("from".into(), Value::from(from.clone()));
            obj.insert("to".into(), Value::from(to.clone()));
        }
        TraceEvent::Rollbacked { from, to, .. } => {
            obj.insert("kind".into(), Value::from("Rollbacked"));
            obj.insert("from".into(), Value::from(from.clone()));
            obj.insert("to".into(), Value::from(to.clone()));
        }
        TraceEvent::ReactionGuardEvaluated {
            reaction_from_signal,
            reaction_from_state,
            reaction_to_signal,
            reaction_event,
            guard,
            result,
            ..
        } => {
            obj.insert("kind".into(), Value::from("ReactionGuardEvaluated"));
            obj.insert("from_signal".into(), Value::from(reaction_from_signal.clone()));
            obj.insert("from_state".into(), Value::from(reaction_from_state.clone()));
            obj.insert("to_signal".into(), Value::from(reaction_to_signal.clone()));
            obj.insert("event".into(), Value::from(reaction_event.clone()));
            obj.insert("guard".into(), Value::from(guard.clone()));
            obj.insert("result".into(), Value::from(result.clone()));
        }
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    const ORDER_APPROVAL: &str = include_str!("../examples/order_approval.json");

    fn engine_with_noops() -> TopologyEngine {
        let mut engine = TopologyEngine::from_json(ORDER_APPROVAL).expect("fixture parses");
        for id in engine.action_ids() {
            let id = id.clone();
            engine.register_action(&id, |_| Ok(()));
        }
        engine
    }

    #[test]
    fn unit_action_ids_deduped_and_sorted() {
        let engine = TopologyEngine::from_json(ORDER_APPROVAL).expect("fixture parses");
        let ids = engine.action_ids();
        // order_approval references 9 unique action ids across its transitions.
        assert_eq!(ids.len(), 9, "expected 9 unique action ids, got {:?}", ids);
        let mut sorted = ids.clone();
        sorted.sort();
        assert_eq!(ids, sorted, "action_ids should be returned sorted");
        let deduped: std::collections::HashSet<_> = ids.iter().collect();
        assert_eq!(deduped.len(), ids.len(), "action_ids should be deduplicated");
    }

    #[test]
    fn unit_traces_json_well_formed() {
        let mut engine = engine_with_noops();
        assert!(
            engine.send_event("order", "submit", None).is_ok(),
            "submit should succeed with no-op actions"
        );

        let json = engine.traces_json();
        let parsed: Value =
            serde_json::from_str(&json).expect("traces_json should produce valid JSON");
        assert!(parsed.is_array(), "traces should serialize to a JSON array");
        let arr = parsed.as_array().unwrap();
        assert!(!arr.is_empty(), "submit should emit at least one trace event");

        // Every event carries the common fields and a kind discriminator.
        for e in arr {
            assert!(e.get("signal_id").is_some(), "event should carry signal_id");
            assert!(
                e.get("timestamp_ms").is_some(),
                "event should carry timestamp_ms"
            );
            assert!(e.get("kind").is_some(), "event should carry a kind field");
        }

        // The first event is the EventReceived for submit.
        let first = &arr[0];
        assert_eq!(first["kind"], "EventReceived");
        assert_eq!(first["event"], "submit");
    }

    #[test]
    fn unit_traces_json_round_trip_reaches_shipped() {
        let mut engine = engine_with_noops();
        assert!(engine.send_event("order", "submit", None).is_ok());
        assert!(engine
            .send_event("order", "approve", Some(serde_json::json!({"amount": 5000})))
            .is_ok());
        assert!(engine.send_event("order", "ship", None).is_ok());
        assert_eq!(engine.get_state("order").unwrap(), "shipped");
    }
}
