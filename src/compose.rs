use crate::error::EngineError;
use crate::schema::{
    ActionBinding, ComponentDef, ReactionDef, SignalDef, TopologySchema, TransitionDef,
};
use std::collections::{HashMap, HashSet};

/// Expand parameterized components and instances into a flat `TopologySchema`.
///
/// Each instance refers to a component by name and supplies `bindings` for the
/// component's `params`. Every string field of the component's signals,
/// transitions and reactions has `${param}` replaced by its bound value. The
/// expanded signals/transitions/reactions are appended to the schema's own, and
/// the returned schema no longer carries `components`/`instances`/`includes`
/// (it is fully flattened).
///
/// If `instances` is empty, the schema is returned unchanged (preserving all
/// fields, including any `components`/`includes`).
pub fn expand(schema: TopologySchema) -> Result<TopologySchema, EngineError> {
    // Fast path: nothing to expand. Preserve the schema exactly as-is.
    if schema.instances.is_empty() {
        return Ok(schema);
    }

    let components = schema.components.unwrap_or_default();

    let mut out_signals = schema.signals;
    let mut out_transitions = schema.transitions;
    let mut out_reactions = schema.reactions;

    for instance in &schema.instances {
        let component = components
            .get(&instance.component)
            .ok_or_else(|| EngineError::ComponentNotFound(instance.component.clone()))?;

        // Validate bindings: each declared param must be supplied.
        let bound = check_bindings(component, &instance.bindings)?;

        for sig in &component.signals {
            out_signals.push(expand_signal(sig, &instance.component, component, &bound)?);
        }
        for trans in &component.transitions {
            out_transitions.push(expand_transition(trans, &instance.component, component, &bound)?);
        }
        for reaction in &component.reactions {
            out_reactions.push(expand_reaction(reaction, &instance.component, component, &bound)?);
        }
    }

    enforce_unique_signal_ids(&out_signals)?;

    Ok(TopologySchema {
        version: schema.version,
        signals: out_signals,
        transitions: out_transitions,
        reactions: out_reactions,
        components: None,
        instances: Vec::new(),
        includes: Vec::new(),
    })
}

/// Ensure every declared `param` is supplied in `bindings`.
fn check_bindings(
    component: &ComponentDef,
    bindings: &HashMap<String, String>,
) -> Result<HashMap<String, String>, EngineError> {
    let mut bound = HashMap::new();
    for param in &component.params {
        let value = bindings.get(param).ok_or_else(|| EngineError::MissingBinding {
            component: component_identity(),
            param: param.clone(),
        })?;
        bound.insert(param.clone(), value.clone());
    }
    Ok(bound)
}

/// Expand a signal's string fields, validating leftover `${...}` refs.
fn expand_signal(
    sig: &SignalDef,
    name: &str,
    component: &ComponentDef,
    bound: &HashMap<String, String>,
) -> Result<SignalDef, EngineError> {
    Ok(SignalDef {
        id: resolve(&sig.id, name, component, bound)?,
        initial_state: resolve(&sig.initial_state, name, component, bound)?,
        states: resolve_vec(&sig.states, name, component, bound)?,
    })
}

/// Expand a transition's string fields, validating leftover `${...}` refs.
fn expand_transition(
    trans: &TransitionDef,
    name: &str,
    component: &ComponentDef,
    bound: &HashMap<String, String>,
) -> Result<TransitionDef, EngineError> {
    Ok(TransitionDef {
        signal_id: resolve(&trans.signal_id, name, component, bound)?,
        from: resolve(&trans.from, name, component, bound)?,
        event: resolve(&trans.event, name, component, bound)?,
        to: resolve(&trans.to, name, component, bound)?,
        actions: ActionBinding {
            on_exit: resolve_vec(&trans.actions.on_exit, name, component, bound)?,
            on_transition: resolve_vec(&trans.actions.on_transition, name, component, bound)?,
            on_enter: resolve_vec(&trans.actions.on_enter, name, component, bound)?,
        },
        guard: match &trans.guard {
            Some(g) => Some(resolve(g, name, component, bound)?),
            None => None,
        },
    })
}

/// Expand a reaction's string fields, validating leftover `${...}` refs.
fn expand_reaction(
    reaction: &ReactionDef,
    name: &str,
    component: &ComponentDef,
    bound: &HashMap<String, String>,
) -> Result<ReactionDef, EngineError> {
    Ok(ReactionDef {
        from_signal: resolve(&reaction.from_signal, name, component, bound)?,
        from_state: resolve(&reaction.from_state, name, component, bound)?,
        to_signal: resolve(&reaction.to_signal, name, component, bound)?,
        event: resolve(&reaction.event, name, component, bound)?,
        payload: reaction.payload.clone(),
    })
}

/// Replace `${param}` occurrences with their bound value (literal single-pass).
fn subst(s: &str, bound: &HashMap<String, String>) -> String {
    let mut result = s.to_string();
    for (param, value) in bound {
        result = result.replace(&format!("${{{}}}", param), value);
    }
    result
}

/// Like `subst`, but after substitution an error is raised if a `${xxx}`
/// remains where `xxx` is not one of the component's declared params.
fn resolve(
    s: &str,
    name: &str,
    component: &ComponentDef,
    bound: &HashMap<String, String>,
) -> Result<String, EngineError> {
    let result = subst(s, bound);
    if let Some(rem) = find_unresolved(&result) {
        if !component.params.iter().any(|p| p == &rem) {
            return Err(EngineError::InvalidParamRef {
                component: name.to_string(),
                param: rem,
            });
        }
    }
    Ok(result)
}

fn resolve_vec(
    items: &[String],
    name: &str,
    component: &ComponentDef,
    bound: &HashMap<String, String>,
) -> Result<Vec<String>, EngineError> {
    items
        .iter()
        .map(|s| resolve(s, name, component, bound))
        .collect()
}

/// Detect a `${...}` pattern and return its inner contents. Only the first one
/// is reported, which is sufficient for error reporting.
fn find_unresolved(s: &str) -> Option<String> {
    let open = s.find("${")?;
    let start = open + 2;
    let end = s[start..].find('}').map(|i| start + i)?;
    Some(s[start..end].to_string())
}

fn enforce_unique_signal_ids(signals: &[SignalDef]) -> Result<(), EngineError> {
    let mut seen = HashSet::new();
    for sig in signals {
        if !seen.insert(&sig.id) {
            return Err(EngineError::DuplicateSignalAfterExpand(sig.id.clone()));
        }
    }
    Ok(())
}

/// Stable placeholder for error context when we don't carry a component name.
fn component_identity() -> String {
    "<component>".to_string()
}
