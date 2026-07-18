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
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ComponentDef {
    /// Parameter names expected to be bound on instantiation.
    pub params: Vec<String>,
    /// The component's exposed ports (reaction interfaces a parent can wire to).
    /// Empty (the default) preserves the pre-M45 component behavior.
    #[serde(default)]
    pub ports: Vec<PortDef>,
    /// The component's signals.
    pub signals: Vec<SignalDef>,
    /// The component's transitions.
    pub transitions: Vec<TransitionDef>,
    /// The component's reactions. Defaults to empty when absent.
    #[serde(default)]
    pub reactions: Vec<ReactionDef>,
}

/// The direction of a component port, describing how the parent topology may
/// interact with the exposed signal.
///
/// - `Out`: the component drives this signal; the parent may react to its state
///   changes (the connected signal's reactions become visible to the parent).
/// - `In`: the parent may trigger this signal.
/// - `InOut`: both directions.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub enum PortDirection {
    /// The parent can trigger this signal.
    In,
    /// This signal's state changes are visible to the parent.
    Out,
    /// Both directions.
    InOut,
}

/// An exposed reaction interface on a component.
///
/// A port names a `(signal, state)` pair inside the component that the parent
/// can wire to a parent-level signal via an `InstanceDef` connection. The
/// optional `alias` is the stable name the parent uses to refer to this port
/// (defaults to `<signal>.<state>` when absent).
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct PortDef {
    /// How the parent may interact with this port.
    pub direction: PortDirection,
    /// The component-internal signal the port is exposed on (may contain
    /// `${param}` placeholders).
    pub signal: String,
    /// The state on `signal` that the port exposes (may contain `${param}`).
    pub state: String,
    /// Stable name the parent uses in `ConnectionDef::port`. When `None`, the
    /// port is addressed as `<signal>.<state>` (after param substitution).
    pub alias: Option<String>,
}

/// A wire from a component port to a parent-level signal.
///
/// During expansion the component-internal signal named by the port is
/// renamed to `target_signal`, so that the parent's reactions (and the
/// component's own reactions) reference the parent-level signal directly.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct ConnectionDef {
    /// The port to wire, identified by its `alias` or by `<signal>.<state>`.
    pub port: String,
    /// The parent-level signal the port's signal is renamed to.
    pub target_signal: String,
}

/// A concrete instantiation of a `ComponentDef`, with `params` bound to values.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct InstanceDef {
    /// The component name (must match a key in `TopologySchema::components`).
    pub component: String,
    /// Maps each parameter name to its concrete value.
    pub bindings: HashMap<String, String>,
    /// Wires from this instance's component ports to parent-level signals.
    /// Empty (the default) preserves the pre-M16 instantiation behavior.
    #[serde(default)]
    pub connections: Vec<ConnectionDef>,
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
    /// The fork group this reaction belongs to, if any. M44: reactions sharing
    /// a `join_group` form a parallel group — they fire (each with its own
    /// cascade) and the group is marked complete only once all its members have
    /// fired. `None` means the reaction is not part of a fork group.
    #[serde(default)]
    pub join_group: Option<String>,
    /// The fork groups this reaction waits on before it may fire. M44: a
    /// reaction with a non-empty `requires` is a "join" — it is held back
    /// until every group named here has completed, then unblocked and fired.
    /// Empty (the default) means no dependency: the reaction fires as soon as
    /// it is reached, same as the pre-M44 serial behavior.
    #[serde(default)]
    pub requires: Vec<String>,
    /// M47: the compensation action to fire when this reaction's cascade fails,
    /// if any. When `fire_one_reaction` returns an `Err` and this is
    /// `Some(action_id)`, the engine runs `action_id` (with the failure message
    /// carried in the `ActionContext.failure` field) before propagating the
    /// error upward. `None` (the default) preserves the pre-M47 behavior — the
    /// cascade error propagates untouched.
    #[serde(default)]
    pub on_fail: Option<String>,
}
