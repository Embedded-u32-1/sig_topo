//! Topology composition: expand parameterized components/instances and resolve
//! cross-file imports into a single flat `TopologySchema` ready for the engine.

use crate::error::EngineError;
use crate::schema::{
    ActionBinding, ComponentDef, ReactionDef, SignalDef, TopologySchema, TransitionDef,
};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

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
        let bound = check_bindings(component, &instance.bindings, &instance.component)?;

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

// ---------------------------------------------------------------------------
// M17 — cross-file import (`includes`).
// ---------------------------------------------------------------------------

/// Public entry point: load a topology file, recursively merging everything it
/// `includes`, then expand instances into a flat `TopologySchema`.
///
/// The returned schema is fully flattened: `includes` is empty, `components`/
/// `instances` have been expanded away by `expand`. Relative include paths are
/// resolved against the including file's parent directory. Canonicalized
/// absolute paths drive cycle detection: re-visiting any file yields
/// `IncludeCycle`.
pub fn load_topology(path: &Path) -> Result<TopologySchema, EngineError> {
    let mut seen = HashSet::new();
    let merged = load_topology_inner(path, &mut seen)?;
    // A single top-level expand turns any parameterized instances into flat
    // signals/transitions/reactions. `expand` is a no-op (pass-through) when
    // there are no instances, so schemas without components still round-trip.
    expand(merged)
}

/// Convenience: `load_topology` + build a ready-to-use `TopologyEngine`.
///
/// `TopologyEngine::from_schema` internally calls `expand` again, which is a
/// no-op on the already-expanded schema returned by `load_topology`.
pub fn from_path(path: &Path) -> Result<crate::engine::TopologyEngine, EngineError> {
    let schema = load_topology(path)?;
    crate::engine::TopologyEngine::from_schema(schema)
}

/// Recursive helper that tracks which files have already been visited (by their
/// canonical absolute path) to detect cycles.
fn load_topology_inner(
    path: &Path,
    seen: &mut HashSet<PathBuf>,
) -> Result<TopologySchema, EngineError> {
    // Read + parse. Any I/O or JSON error surfaces as `IncludeNotFound` so the
    // caller gets a single, actionable error type at the top level.
    let text = std::fs::read_to_string(path)
        .map_err(|_| EngineError::IncludeNotFound(path.display().to_string()))?;
    let schema: TopologySchema = serde_json::from_str(&text).map_err(|e| {
        EngineError::IncludeNotFound(format!("{}: {}", path.display(), e))
    })?;

    // Canonicalize after a successful read so a missing file reports
    // `IncludeNotFound` (canonicalize would obscure that distinction).
    let canonical = std::fs::canonicalize(path)
        .map_err(|_| EngineError::IncludeNotFound(path.display().to_string()))?;

    if !seen.insert(canonical) {
        return Err(EngineError::IncludeCycle(path.display().to_string()));
    }

    // Snapshot includes, then detach them from self before merging so the
    // merged result carries an empty `includes`.
    let includes = schema.includes.clone();
    let mut merged = TopologySchema {
        includes: Vec::new(),
        ..schema
    };

    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    for inc in &includes {
        let sub_path = parent.join(inc);
        let sub = load_topology_inner(&sub_path, seen)?;
        merge_in(&mut merged, sub)?;
    }

    Ok(merged)
}

/// Merge `sub` into `acc`. Signals/transitions/reactions are appended; duplicate
/// signal ids across files raise `DuplicateSignalAfterExpand`. Components with
/// the same name are overwritten by `sub` (last-wins), no error.
fn merge_in(acc: &mut TopologySchema, sub: TopologySchema) -> Result<(), EngineError> {
    // Duplicate check on signals uses the same error expand uses, so load and
    // expand agree on what "duplicate" means.
    for sig in &sub.signals {
        if acc.signals.iter().any(|s| s.id == sig.id) {
            return Err(EngineError::DuplicateSignalAfterExpand(sig.id.clone()));
        }
    }
    acc.signals.extend(sub.signals);
    acc.transitions.extend(sub.transitions);
    acc.reactions.extend(sub.reactions);

    if let Some(sub_components) = sub.components {
        let acc_components = acc.components.get_or_insert_with(HashMap::new);
        for (name, comp) in sub_components {
            acc_components.insert(name, comp);
        }
    }

    Ok(())
}

/// Ensure every declared `param` is supplied in `bindings`.
///
/// `owner` is the concrete component name (from the instance) surfaced in the
/// error so the caller knows *which* component is missing a binding.
fn check_bindings(
    component: &ComponentDef,
    bindings: &HashMap<String, String>,
    owner: &str,
) -> Result<HashMap<String, String>, EngineError> {
    let mut bound = HashMap::new();
    for param in &component.params {
        let value = bindings.get(param).ok_or_else(|| EngineError::MissingBinding {
            component: owner.to_string(),
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
        guard: reaction.guard.clone(),
    })
}

/// Replace `${param}` occurrences with their bound value in a single left-to-right
/// pass. The text of a substituted value is written straight to the output and
/// scanned no further, so:
/// - substitution is deterministic (no dependence on `HashMap` iteration order),
/// - a value containing `${other}` is never re-interpreted (no double-scan),
/// - leftover `${xxx}` (unbound name) simply falls through to the output for
///   `resolve` to flag.
fn subst(s: &str, bound: &HashMap<String, String>) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(open) = rest.find("${") {
        // Everything before `${` is literal.
        out.push_str(&rest[..open]);
        let after_open = &rest[open + 2..];
        if let Some(close) = after_open.find('}') {
            let name = &after_open[..close];
            if let Some(value) = bound.get(name) {
                // Substitute; the value's text goes straight to `out` and will
                // not be rescanned because we advance `rest` past the `}`.
                out.push_str(value);
            } else {
                // Unknown name: keep the literal `${...}` verbatim.
                out.push_str(&rest[open..open + 2 + close + 1]);
            }
            rest = &after_open[close + 1..];
        } else {
            // No closing `}`: remainder is literal.
            out.push_str(rest);
            rest = "";
        }
    }
    out.push_str(rest);
    out
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

