use serde::Deserialize;
use std::collections::HashMap;

/// A fully-described topology: signals, transitions, reactions and (optionally)
/// parameterized components, instances and cross-file includes.
///
/// This is the top-level type parsed from a JSON topology file. After expansion
/// (`crate::compose::expand`) the `components` / `instances` / `includes` fields
/// are gone and the schema is flat.
#[derive(Debug, Clone, Deserialize)]
pub struct TopologySchema {
    /// Schema version string (informational).
    pub version: String,
    /// The signals in the topology.
    pub signals: Vec<SignalDef>,
    /// The transitions between states.
    pub transitions: Vec<TransitionDef>,
    /// Cross-signal cascade rules. Defaults to empty when absent.
    #[serde(default)]
    pub reactions: Vec<ReactionDef>,
    /// Named, parameterized sub-topologies, keyed by component name.
    #[serde(default)]
    pub components: Option<HashMap<String, ComponentDef>>,
    /// Concrete instantiations of components with bound parameter values.
    #[serde(default)]
    pub instances: Vec<InstanceDef>,
    // M17: cross-file import (field added in M16; parsing implemented in M17).
    /// Other topology files to merge in, resolved relative to this file.
    #[serde(default)]
    pub includes: Vec<String>,
}

/// A named, parameterized sub-topology reusable via `InstanceDef`.
///
/// Every string field may contain `${param}` placeholders that are substituted
/// when an instance supplies bindings.
#[derive(Debug, Clone, Deserialize)]
pub struct ComponentDef {
    /// Parameter names expected to be bound on instantiation.
    pub params: Vec<String>,
    /// The component's signals.
    pub signals: Vec<SignalDef>,
    /// The component's transitions.
    pub transitions: Vec<TransitionDef>,
    /// The component's reactions. Defaults to empty when absent.
    #[serde(default)]
    pub reactions: Vec<ReactionDef>,
}

/// A concrete instantiation of a `ComponentDef`, with `params` bound to values.
#[derive(Debug, Clone, Deserialize)]
pub struct InstanceDef {
    /// The component name (must match a key in `TopologySchema::components`).
    pub component: String,
    /// Maps each parameter name to its concrete value.
    pub bindings: HashMap<String, String>,
}

/// A signal: a named state machine with a fixed set of states.
#[derive(Debug, Clone, Deserialize)]
pub struct SignalDef {
    /// The signal's unique id.
    pub id: String,
    /// The state the signal starts in (must be a member of `states`).
    pub initial_state: String,
    /// The full set of states the signal may occupy.
    pub states: Vec<String>,
}

/// A transition: a named event that moves a signal from `from` to `to`.
#[derive(Debug, Clone, Deserialize)]
pub struct TransitionDef {
    /// The signal this transition belongs to.
    pub signal_id: String,
    /// The source state, or `*` to match any state.
    pub from: String,
    /// The event name that triggers this transition.
    pub event: String,
    /// The target state.
    pub to: String,
    /// The lifecycle actions bound to this transition. Defaults to empty.
    #[serde(default)]
    pub actions: ActionBinding,
    /// An optional guard expression; the transition is blocked when it evaluates
    /// to `false`. Defaults to `None`.
    #[serde(default)]
    pub guard: Option<String>,
}

/// The lifecycle actions bound to a transition, in the order the engine runs
/// them: `on_exit` → `on_transition` → `on_enter`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ActionBinding {
    /// Actions run before leaving the source state.
    #[serde(default)]
    pub on_exit: Vec<String>,
    /// Actions run after tentatively entering the target state.
    #[serde(default)]
    pub on_transition: Vec<String>,
    /// Actions run after the transition has committed.
    #[serde(default)]
    pub on_enter: Vec<String>,
}

impl ActionBinding {
    /// Return all bound action ids in lifecycle order.
    pub fn all_actions(&self) -> Vec<&String> {
        let mut actions = Vec::new();
        actions.extend(self.on_exit.iter());
        actions.extend(self.on_transition.iter());
        actions.extend(self.on_enter.iter());
        actions
    }
}

/// A cross-signal cascade rule: when `from_signal` enters `from_state`,
/// deliver `event` to `to_signal`.
///
/// `from_state` may be `*` to match any state. The optional `payload` is the
/// static event payload delivered to the target; when `None`, the target
/// receives the event with no payload.
#[derive(Debug, Clone, Deserialize)]
pub struct ReactionDef {
    /// The signal whose state change triggers the cascade.
    pub from_signal: String,
    /// The state that triggers the cascade, or `*` for any.
    pub from_state: String,
    /// The signal that receives the derived event.
    pub to_signal: String,
    /// The event delivered to the target signal.
    pub event: String,
    /// The static payload for the derived event, if any.
    pub payload: Option<serde_json::Value>,
    /// An optional guard expression; the reaction is skipped when it evaluates
    /// to `false`. Defaults to `None` (unconditional cascade).
    ///
    /// The guard is evaluated against the reaction's static `payload` (see
    /// `engine::send_event_internal`), so a reaction guard can gate the cascade
    /// on the payload carried by the derived event.
    #[serde(default)]
    pub guard: Option<String>,
}
