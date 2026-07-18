// M28: DDL codegen.
//
// Lowers a `DdlDoc` AST into the engine's `TopologySchema`. This is the only
// place that touches `schema.rs` types, so the lexer/parser stay decoupled
// from serde. The mapping is 1:1 and total — every AST node becomes exactly
// one schema node, and guards pass through verbatim as `Option<String>`.

use crate::error::EngineError;
use crate::schema::{
    ActionBinding, ComponentDef, InstanceDef, ReactionDef, SignalDef, TopologySchema,
    TransitionDef,
};

use super::parser::{DdlDoc, InstantiateDecl, SignalDecl, TransDecl};

use std::collections::HashMap;

/// Emit a `TopologySchema` from a parsed DDL document.
///
/// Reaction guards pass through verbatim into `ReactionDef.guard`; the engine
/// evaluates them at cascade time and skips any reaction whose guard is false
/// (see `engine::send_event_internal`, M32).
pub fn emit(doc: DdlDoc) -> Result<TopologySchema, EngineError> {
    // Lower the top-level signals and their transitions.
    let (signals, transitions) = emit_signals(doc.signals)?;

    let reactions = doc
        .reactions
        .into_iter()
        .map(emit_reaction)
        .collect::<Result<Vec<_>, _>>()?;

    // M45: lower each component declaration into a `ComponentDef`. Its signals,
    // transitions and reactions are `${param}`-parameterized and expanded at
    // instantiation time.
    let mut components = HashMap::with_capacity(doc.components.len());
    for comp in doc.components {
        let (signals, transitions) = emit_signals(comp.signals)?;
        let reactions = comp
            .reactions
            .into_iter()
            .map(emit_reaction)
            .collect::<Result<Vec<_>, _>>()?;
        let component = ComponentDef {
            params: comp.params,
            ports: comp.ports,
            signals,
            transitions,
            reactions,
        };
        components.insert(comp.id, component);
    }

    // M45: lower each instantiation into an `InstanceDef` (bindings + wiring).
    let instances = doc
        .instantiates
        .into_iter()
        .map(emit_instantiate)
        .collect::<Result<Vec<_>, _>>()?;

    Ok(TopologySchema {
        version: "0.1".to_string(),
        signals,
        transitions,
        reactions,
        components: if components.is_empty() {
            None
        } else {
            Some(components)
        },
        instances,
        includes: Vec::new(),
    })
}

/// Lower a set of `SignalDecl`s (each carrying its transitions) into flat
/// `SignalDef`s and `TransitionDef`s. Used for both the top-level body and the
/// body of a component.
fn emit_signals(
    decls: Vec<SignalDecl>,
) -> Result<(Vec<SignalDef>, Vec<TransitionDef>), EngineError> {
    let mut signals = Vec::with_capacity(decls.len());
    let mut transitions = Vec::new();

    for sig in decls {
        signals.push(SignalDef {
            id: sig.id.clone(),
            initial_state: sig.initial,
            states: sig.states.clone(),
        });

        for tr in sig.transitions {
            // M34: a wildcard `from *` lowers to one transition per source
            // state (including the `to -> to` self-loop the engine matches via
            // `t.from == signal.current`, so the self-loop is harmless). All
            // expanded arms share the same event/to/actions/guard.
            transitions.extend(emit_transition(&sig.id, &sig.states, tr)?);
        }
    }

    Ok((signals, transitions))
}

/// Lower an `InstantiateDecl` into an `InstanceDef`.
fn emit_instantiate(inst: InstantiateDecl) -> Result<InstanceDef, EngineError> {
    Ok(InstanceDef {
        component: inst.component,
        bindings: inst.bindings,
        connections: inst.connections,
    })
}

/// Lower a single transition. When `tr.from == "*"`, the caller supplies the
/// signal's `states` so we can expand into one `TransitionDef` per source state
/// (see `emit`); otherwise this returns exactly one transition with `from`
/// untouched.
fn emit_transition(
    signal_id: &str,
    states: &[String],
    tr: TransDecl,
) -> Result<Vec<TransitionDef>, EngineError> {
    if tr.from == "*" {
        let mut out = Vec::with_capacity(states.len());
        for from in states {
            out.push(TransitionDef {
                signal_id: signal_id.to_string(),
                from: from.clone(),
                event: tr.event.clone(),
                to: tr.to.clone(),
                actions: ActionBinding {
                    on_exit: tr.actions.on_exit.clone(),
                    on_transition: tr.actions.on_transition.clone(),
                    on_enter: tr.actions.on_enter.clone(),
                },
                guard: tr.guard.clone(),
            });
        }
        return Ok(out);
    }
    Ok(vec![TransitionDef {
        signal_id: signal_id.to_string(),
        from: tr.from,
        event: tr.event,
        to: tr.to,
        actions: ActionBinding {
            on_exit: tr.actions.on_exit,
            on_transition: tr.actions.on_transition,
            on_enter: tr.actions.on_enter,
        },
        guard: tr.guard,
    }])
}

fn emit_reaction(r: super::parser::ReactionDecl) -> Result<ReactionDef, EngineError> {
    let payload = match r.payload {
        Some(raw) => Some(
            serde_json::from_str::<serde_json::Value>(&raw).map_err(|e| {
                EngineError::ParseError(format!(
                    "reaction payload is not valid JSON: {}",
                    e
                ))
            })?,
        ),
        None => None,
    };
    Ok(ReactionDef {
        from_signal: r.from_signal,
        from_state: r.from_state,
        to_signal: r.to_signal,
        event: r.event,
        payload,
        guard: r.guard,
        join_group: r.join_group,
        requires: r.requires,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ddl::parser::{DdlActionBinding, DdlDoc, SignalDecl};

    fn doc() -> DdlDoc {
        DdlDoc {
            signals: vec![SignalDecl {
                id: "task".to_string(),
                states: vec!["idle".to_string(), "running".to_string(), "done".to_string()],
                initial: "idle".to_string(),
                transitions: vec![
                    TransDecl {
                        event: "start".to_string(),
                        from: "idle".to_string(),
                        to: "running".to_string(),
                        guard: None,
                        actions: DdlActionBinding::default(),
                    },
                    TransDecl {
                        event: "finish".to_string(),
                        from: "running".to_string(),
                        to: "done".to_string(),
                        guard: Some("payload.ok == true".to_string()),
                        actions: DdlActionBinding {
                            on_exit: vec!["a".to_string()],
                            on_transition: vec!["b".to_string()],
                            on_enter: vec!["c".to_string()],
                        },
                    },
                ],
            }],
            reactions: vec![],
            guards: vec![],
            components: vec![],
            instantiates: vec![]
        }
    }

    #[test]
    fn codegen_single_signal_no_actions() {
        let schema = emit(doc()).unwrap();

        assert_eq!(schema.version, "0.1");
        assert_eq!(schema.signals.len(), 1);
        assert_eq!(schema.signals[0].id, "task");
        assert_eq!(schema.signals[0].initial_state, "idle");
        assert_eq!(schema.transitions.len(), 2);

        let t0 = &schema.transitions[0];
        assert_eq!(t0.signal_id, "task");
        assert_eq!(t0.from, "idle");
        assert_eq!(t0.event, "start");
        assert_eq!(t0.to, "running");
        assert!(t0.guard.is_none());
        assert!(t0.actions.all_actions().is_empty());
    }

    #[test]
    fn codegen_guard_passes_through() {
        let schema = emit(doc()).unwrap();
        let t1 = &schema.transitions[1];
        assert_eq!(t1.guard, Some("payload.ok == true".to_string()));
    }

    #[test]
    fn codegen_three_lifecycle_actions_preserve_order() {
        let schema = emit(doc()).unwrap();
        let t1 = &schema.transitions[1];
        assert_eq!(t1.actions.on_exit, vec!["a"]);
        assert_eq!(t1.actions.on_transition, vec!["b"]);
        assert_eq!(t1.actions.on_enter, vec!["c"]);
        assert_eq!(t1.actions.all_actions(), vec!["a", "b", "c"]);
    }

    #[test]
    fn codegen_reaction_mapping() {
        let schema = emit(DdlDoc {
            signals: vec![],
            reactions: vec![crate::ddl::parser::ReactionDecl {
                from_signal: "order".to_string(),
                from_state: "approved".to_string(),
                to_signal: "fulfill".to_string(),
                event: "begin".to_string(),
                guard: None,
                guard_ref: None,
                payload: None,
                join_group: None,
                requires: vec![],
            }],
            guards: vec![],
            components: vec![],
            instantiates: vec![]
        })
        .unwrap();

        assert_eq!(schema.reactions.len(), 1);
        let r = &schema.reactions[0];
        assert_eq!(r.from_signal, "order");
        assert_eq!(r.from_state, "approved");
        assert_eq!(r.to_signal, "fulfill");
        assert_eq!(r.event, "begin");
        assert_eq!(r.payload, None);
    }

    #[test]
    fn codegen_reaction_guard_passes_through() {
        // M32: reaction guards are now supported by the engine and pass
        // through verbatim into `ReactionDef.guard`.
        let schema = emit(DdlDoc {
            signals: vec![],
            reactions: vec![crate::ddl::parser::ReactionDecl {
                from_signal: "order".to_string(),
                from_state: "approved".to_string(),
                to_signal: "fulfill".to_string(),
                event: "begin".to_string(),
                guard: Some("payload.auto".to_string()),
                guard_ref: None,
                payload: None,
                join_group: None,
                requires: vec![],
            }],
            guards: vec![],
            components: vec![],
            instantiates: vec![]
        })
        .unwrap();

        assert_eq!(schema.reactions.len(), 1);
        assert_eq!(schema.reactions[0].guard, Some("payload.auto".to_string()));
    }

    #[test]
    fn codegen_reaction_with_payload_passes_through() {
        // M34: a reaction's `with { ... }` static payload (captured as raw text
        // by the parser) is parsed to JSON and lands in `ReactionDef.payload`.
        let schema = emit(DdlDoc {
            signals: vec![],
            reactions: vec![crate::ddl::parser::ReactionDecl {
                from_signal: "order".to_string(),
                from_state: "approved".to_string(),
                to_signal: "fulfill".to_string(),
                event: "begin".to_string(),
                guard: None,
                guard_ref: None,
                payload: Some(r#"{ "auto": true, "count": 1 }"#.to_string()),
                join_group: None,
                requires: vec![],
            }],
            guards: vec![],
            components: vec![],
            instantiates: vec![]
        })
        .unwrap();

        assert_eq!(schema.reactions.len(), 1);
        assert_eq!(
            schema.reactions[0].payload,
            Some(serde_json::json!({"auto": true, "count": 1 }))
        );
    }

    #[test]
    fn codegen_reaction_payload_invalid_json_is_error() {
        // A malformed `with { ... }` block must surface as a ParseError from
        // codegen, not a panic.
        let err = emit(DdlDoc {
            signals: vec![],
            reactions: vec![crate::ddl::parser::ReactionDecl {
                from_signal: "order".to_string(),
                from_state: "approved".to_string(),
                to_signal: "fulfill".to_string(),
                event: "begin".to_string(),
                guard: None,
                guard_ref: None,
                payload: Some(r#"{ not json }"#.to_string()),
                join_group: None,
                requires: vec![],
            }],
            guards: vec![],
            components: vec![],
            instantiates: vec![]
        })
        .unwrap_err();
        assert!(err.to_string().contains("not valid JSON"), "got: {}", err);
    }
}
