use crate::error::EngineError;
use crate::schema::{TopologySchema, TransitionDef};
use crate::trace::{now_ms, TraceEvent, TraceLog};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

#[derive(Debug)]
pub struct ActionContext {
    pub signal_id: String,
    pub from_state: String,
    pub to_state: String,
    pub event: String,
    pub payload: Option<Value>,
}

#[derive(Debug)]
pub struct TransitionResult {
    pub signal_id: String,
    pub from: String,
    pub to: String,
    pub executed_actions: Vec<String>,
}

type ActionFn = Box<dyn FnMut(ActionContext) -> Result<(), EngineError> + 'static>;

pub struct TopologyEngine {
    signals: HashMap<String, SignalState>,
    transitions: Vec<TransitionDef>,
    actions: HashMap<String, ActionFn>,
    trace: TraceLog,
}

struct SignalState {
    current: String,
}

impl TopologyEngine {
    pub fn from_json(json_str: &str) -> Result<Self, EngineError> {
        let schema: TopologySchema =
            serde_json::from_str(json_str).map_err(|e| EngineError::ParseError(e.to_string()))?;

        Self::validate(&schema)?;

        let mut signals = HashMap::new();
        for sig in &schema.signals {
            signals.insert(
                sig.id.clone(),
                SignalState {
                    current: sig.initial_state.clone(),
                },
            );
        }

        Ok(TopologyEngine {
            signals,
            transitions: schema.transitions,
            actions: HashMap::new(),
            trace: TraceLog::default(),
        })
    }

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

        Ok(())
    }

    pub fn register_action<F>(&mut self, action_id: &str, mut f: F)
    where
        F: FnMut(ActionContext) -> Result<(), EngineError> + 'static,
    {
        self.actions
            .insert(action_id.to_string(), Box::new(move |ctx| f(ctx)));
    }

    pub fn send_event(
        &mut self,
        signal_id: &str,
        event: &str,
        payload: Option<Value>,
    ) -> Result<TransitionResult, EngineError> {
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
        let mut executed_actions = Vec::new();

        for action_id in &transition.actions.on_exit {
            self.trace.push(TraceEvent::ActionStarted {
                signal_id: signal_id.to_string(),
                action_id: action_id.clone(),
                timestamp_ms: now_ms(),
            });
            let action = self
                .actions
                .get_mut(action_id)
                .ok_or_else(|| EngineError::ActionNotFound(action_id.clone()))?;
            let ctx = ActionContext {
                signal_id: signal_id.to_string(),
                from_state: from_state.clone(),
                to_state: to_state.clone(),
                event: event.to_string(),
                payload: payload.clone(),
            };
            action(ctx).map_err(|e| {
                let msg = match &e {
                    EngineError::ActionExecutionError(m) => m.clone(),
                    _ => e.to_string(),
                };
                self.trace.push(TraceEvent::ActionFailed {
                    signal_id: signal_id.to_string(),
                    action_id: action_id.clone(),
                    timestamp_ms: now_ms(),
                    error: msg.clone(),
                });
                EngineError::ActionExecutionError(msg)
            })?;
            self.trace.push(TraceEvent::ActionSucceeded {
                signal_id: signal_id.to_string(),
                action_id: action_id.clone(),
                timestamp_ms: now_ms(),
            });
            executed_actions.push(action_id.clone());
        }

        signal.current = to_state.clone();

        self.trace.push(TraceEvent::StateChanged {
            signal_id: signal_id.to_string(),
            from: from_state.clone(),
            to: to_state.clone(),
            timestamp_ms: now_ms(),
        });

        for action_id in &transition.actions.on_transition {
            self.trace.push(TraceEvent::ActionStarted {
                signal_id: signal_id.to_string(),
                action_id: action_id.clone(),
                timestamp_ms: now_ms(),
            });
            let action = self
                .actions
                .get_mut(action_id)
                .ok_or_else(|| EngineError::ActionNotFound(action_id.clone()))?;
            let ctx = ActionContext {
                signal_id: signal_id.to_string(),
                from_state: from_state.clone(),
                to_state: to_state.clone(),
                event: event.to_string(),
                payload: payload.clone(),
            };
            action(ctx).map_err(|e| {
                let msg = match &e {
                    EngineError::ActionExecutionError(m) => m.clone(),
                    _ => e.to_string(),
                };
                self.trace.push(TraceEvent::ActionFailed {
                    signal_id: signal_id.to_string(),
                    action_id: action_id.clone(),
                    timestamp_ms: now_ms(),
                    error: msg.clone(),
                });
                EngineError::ActionExecutionError(msg)
            })?;
            self.trace.push(TraceEvent::ActionSucceeded {
                signal_id: signal_id.to_string(),
                action_id: action_id.clone(),
                timestamp_ms: now_ms(),
            });
            executed_actions.push(action_id.clone());
        }

        for action_id in &transition.actions.on_enter {
            self.trace.push(TraceEvent::ActionStarted {
                signal_id: signal_id.to_string(),
                action_id: action_id.clone(),
                timestamp_ms: now_ms(),
            });
            let action = self
                .actions
                .get_mut(action_id)
                .ok_or_else(|| EngineError::ActionNotFound(action_id.clone()))?;
            let ctx = ActionContext {
                signal_id: signal_id.to_string(),
                from_state: from_state.clone(),
                to_state: to_state.clone(),
                event: event.to_string(),
                payload: payload.clone(),
            };
            action(ctx).map_err(|e| {
                let msg = match &e {
                    EngineError::ActionExecutionError(m) => m.clone(),
                    _ => e.to_string(),
                };
                self.trace.push(TraceEvent::ActionFailed {
                    signal_id: signal_id.to_string(),
                    action_id: action_id.clone(),
                    timestamp_ms: now_ms(),
                    error: msg.clone(),
                });
                EngineError::ActionExecutionError(msg)
            })?;
            self.trace.push(TraceEvent::ActionSucceeded {
                signal_id: signal_id.to_string(),
                action_id: action_id.clone(),
                timestamp_ms: now_ms(),
            });
            executed_actions.push(action_id.clone());
        }

        Ok(TransitionResult {
            signal_id: signal_id.to_string(),
            from: from_state,
            to: to_state,
            executed_actions,
        })
    }

    pub fn get_state(&self, signal_id: &str) -> Result<&str, EngineError> {
        let signal = self
            .signals
            .get(signal_id)
            .ok_or_else(|| EngineError::SignalNotFound(signal_id.to_string()))?;
        Ok(&signal.current)
    }

    pub fn traces(&self) -> &[TraceEvent] {
        self.trace.events()
    }

    pub fn traces_for(&self, signal_id: &str) -> Vec<&TraceEvent> {
        self.trace.for_signal(signal_id)
    }

    pub fn clear_traces(&mut self) {
        self.trace.clear();
    }
}
