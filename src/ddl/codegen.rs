// M28: DDL codegen.
//
// Lowers a `DdlDoc` AST into the engine's `TopologySchema`. This is the only
// place that touches `schema.rs` types, so the lexer/parser stay decoupled
// from serde. The mapping is 1:1 and total — every AST node becomes exactly
// one schema node, and guards pass through verbatim as `Option<String>`.

use crate::error::EngineError;
use crate::schema::{ActionBinding, ReactionDef, SignalDef, TopologySchema, TransitionDef};

use super::parser::{DdlDoc, TransDecl};

/// Emit a `TopologySchema` from a parsed DDL document.
///
/// Reaction guards are rejected here: the engine's `ReactionDef` carries no
/// guard field and cascade matching does not evaluate one, so a reaction guard
/// cannot be enforced. Rather than silently dropping it (a footgun — the user
/// wrote it expecting it to mean something), we surface a clear error pointing
/// the user at transition guards / payload conditions instead.
pub fn emit(doc: DdlDoc) -> Result<TopologySchema, EngineError> {
    for r in &doc.reactions {
        if r.guard.is_some() {
            return Err(EngineError::ParseError(
                "reaction guards are not supported by the engine; guard a \
                 transition instead, or gate the source event"
                    .to_string(),
            ));
        }
    }

    let mut signals = Vec::with_capacity(doc.signals.len());
    let mut transitions = Vec::new();

    for sig in doc.signals {
        signals.push(SignalDef {
            id: sig.id.clone(),
            initial_state: sig.initial,
            states: sig.states,
        });

        for tr in sig.transitions {
            transitions.push(emit_transition(&sig.id, tr));
        }
    }

    let reactions = doc.reactions.into_iter().map(emit_reaction).collect();

    Ok(TopologySchema {
        version: "0.1".to_string(),
        signals,
        transitions,
        reactions,
        components: None,
        instances: Vec::new(),
        includes: Vec::new(),
    })
}

fn emit_transition(signal_id: &str, tr: TransDecl) -> TransitionDef {
    TransitionDef {
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
    }
}

fn emit_reaction(r: super::parser::ReactionDecl) -> ReactionDef {
    ReactionDef {
        from_signal: r.from_signal,
        from_state: r.from_state,
        to_signal: r.to_signal,
        event: r.event,
        payload: None,
    }
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
            }],
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
    fn codegen_reaction_guard_is_rejected() {
        let err = emit(DdlDoc {
            signals: vec![],
            reactions: vec![crate::ddl::parser::ReactionDecl {
                from_signal: "order".to_string(),
                from_state: "approved".to_string(),
                to_signal: "fulfill".to_string(),
                event: "begin".to_string(),
                guard: Some("payload.auto".to_string()),
            }],
        })
        .unwrap_err();

        let msg = err.to_string();
        assert!(
            msg.contains("reaction guards are not supported"),
            "got: {}",
            msg
        );
    }
}
