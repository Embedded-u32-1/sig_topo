use crate::error::EngineError;
use crate::guard::eval_guard;
use crate::schema::{TopologySchema, TransitionDef};
use crate::trace::{now_ms, TraceEvent, TraceLog};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone)]
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
    pub(crate) signals: HashMap<String, SignalState>,
    pub(crate) transitions: Vec<TransitionDef>,
    actions: HashMap<String, ActionFn>,
    trace: TraceLog,
}

pub(crate) struct SignalState {
    pub(crate) current: String,
    pub(crate) states: Vec<String>,
}

impl TopologyEngine {
    pub fn from_json(json_str: &str) -> Result<Self, EngineError> {
        let schema: TopologySchema =
            serde_json::from_str(json_str).map_err(|e| EngineError::ParseError(e.to_string()))?;
        Self::from_schema(schema)
    }

    pub fn from_schema(schema: TopologySchema) -> Result<Self, EngineError> {
        Self::validate(&schema)?;

        let mut signals = HashMap::new();
        for sig in &schema.signals {
            signals.insert(
                sig.id.clone(),
                SignalState {
                    current: sig.initial_state.clone(),
                    states: sig.states.clone(),
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

    pub fn register_action<F>(&mut self, action_id: &str, f: F)
    where
        F: FnMut(ActionContext) -> Result<(), EngineError> + 'static,
    {
        self.actions
            .insert(action_id.to_string(), Box::new(f));
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

        signal.current = to_state.clone();

        self.trace.push(TraceEvent::StateChanged {
            signal_id: signal_id.to_string(),
            from: from_state.clone(),
            to: to_state.clone(),
            timestamp_ms: now_ms(),
        });

        for action_id in &transition.actions.on_transition {
            Self::run_action(
                &mut self.trace,
                &mut self.actions,
                signal_id,
                action_id,
                ctx.clone(),
            )?;
            executed_actions.push(action_id.clone());
        }

        for action_id in &transition.actions.on_enter {
            Self::run_action(
                &mut self.trace,
                &mut self.actions,
                signal_id,
                action_id,
                ctx.clone(),
            )?;
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

    pub fn signal_ids(&self) -> Vec<&str> {
        self.signals.keys().map(|s| s.as_str()).collect()
    }

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
                },
            );
        }

        self.signals = new_signals;
        self.transitions = schema.transitions;
        Ok(())
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
