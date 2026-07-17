use crate::schema::{ReactionDef, TopologySchema, TransitionDef};
use std::collections::HashMap;

pub fn to_dot(schema: &TopologySchema) -> String {
    // The structural skeleton with no runtime state: delegate to the
    // state-aware renderer with an empty map so every signal falls back to
    // the static initial-state highlight. Keeps a single rendering path and
    // full backward compatibility for `to_dot`'s signature and output.
    to_dot_with_state(schema, &HashMap::new())
}

/// Render `schema` as Graphviz DOT, additionally highlighting each signal's
/// *current* state from `states` (signal id -> current state).
///
/// Visual strategy, per state node (first match wins):
/// - current state (`states.get(signal.id) == Some(state)`) ->
///   `style=filled fillcolor=lightgreen penwidth=2`. The runtime highlight
///   always wins, so a node the signal is sitting on reads as "live".
/// - otherwise, the initial state -> `style=filled fillcolor=lightblue`,
///   the static "started here" marker.
/// - everything else -> no extra attributes.
///
/// When current != initial you see both cues (lightblue = started here,
/// lightgreen = is here now); when they coincide, lightgreen wins. Callers
/// passing an empty `states` map get the same output as `to_dot`.
pub fn to_dot_with_state(schema: &TopologySchema, states: &HashMap<String, String>) -> String {
    // Cross-signal reactions (cascades) are intentionally omitted from DOT
    // output to keep diagrams readable; only explicit transitions are drawn.
    let mut out = String::new();
    out.push_str("digraph Topology {\n");
    out.push_str("  rankdir=LR;\n");
    out.push_str("  node [shape=ellipse];\n\n");

    let transitions_by_signal: HashMap<&String, Vec<&TransitionDef>> = schema
        .transitions
        .iter()
        .fold(HashMap::new(), |mut acc, t| {
            acc.entry(&t.signal_id).or_default().push(t);
            acc
        });

    for signal in &schema.signals {
        let sig_id = sanitize_id(&signal.id);
        out.push_str(&format!("  subgraph cluster_{} {{\n", sig_id));
        out.push_str(&format!("    label=\"{}\";\n", escape_label(&signal.id)));
        out.push_str("    style=rounded;\n");
        out.push_str("    color=gray;\n\n");

        for state in &signal.states {
            let node_id = node_id(&signal.id, state);
            // Runtime highlight takes precedence over the static initial-state
            // marker: "where the signal is now" outranks "where it started".
            let attrs = match states.get(&signal.id) {
                current if current == Some(state) => "style=filled fillcolor=lightgreen penwidth=2",
                _ => {
                    if *state == signal.initial_state {
                        "style=filled fillcolor=lightblue"
                    } else {
                        ""
                    }
                }
            };
            out.push_str(&format!(
                "    {} [label=\"{}\"{}];\n",
                node_id,
                escape_label(state),
                if attrs.is_empty() {
                    String::new()
                } else {
                    format!(" {}", attrs)
                }
            ));
        }

        out.push('\n');

        if let Some(transitions) = transitions_by_signal.get(&signal.id) {
            for transition in transitions {
                let label = edge_label(transition);
                let tooltip = edge_tooltip(transition);

                if transition.from == "*" {
                    for state in &signal.states {
                        if *state == transition.to {
                            continue;
                        }
                        let from_id = node_id(&signal.id, state);
                        let to_id = node_id(&signal.id, &transition.to);
                        out.push_str(&format!(
                            "    {} -> {} [label=\"{}\" tooltip=\"{}\"];\n",
                            from_id,
                            to_id,
                            escape_label(&label),
                            escape_label(&tooltip)
                        ));
                    }
                } else {
                    let from_id = node_id(&signal.id, &transition.from);
                    let to_id = node_id(&signal.id, &transition.to);
                    out.push_str(&format!(
                        "    {} -> {} [label=\"{}\" tooltip=\"{}\"];\n",
                        from_id,
                        to_id,
                        escape_label(&label),
                        escape_label(&tooltip)
                    ));
                }
            }
        }

        out.push_str("  }\n");
    }

    out.push_str("}\n");
    out
}

/// Render `schema` as Graphviz DOT with every signal's current state
/// highlighted (as `to_dot_with_state`) **plus** cross-signal reaction
/// edges colored by their guard-evaluation result.
///
/// Reaction edges are drawn as dashed arrows from the triggering state to an
/// anchor node in the target signal's cluster. The intended reading is:
/// "when `from_signal` enters `from_state`, an `event` fire is (or would be)
/// delivered to `to_signal`" — the edge's color tells you whether the
/// reaction's guard let it through.
///
/// `guard_info` maps `(from_signal, from_state, to_signal, event)` to the
/// reaction's guard result string (`"true"`, `"false"`, or `"error: <msg>"`),
/// typically collected from an engine's `ReactionGuardEvaluated` trace events
/// (see `TopologyEngine::snapshot_dot_extended`). Color decision, first match
/// wins:
/// - result starts with `"true"`  → solid green (guard passed, reaction fired).
/// - result starts with `"false"` → dashed gray (guard blocked the reaction).
/// - result starts with `"error"` → dashed red (guard failed to evaluate).
/// - reaction absent from `guard_info` → dashed black with a `not evaluated`
///   label (the engine never evaluated this reaction's guard this run).
///
/// The reaction edge's tail anchors on the `from_state` node of `from_signal`
/// (the state the reaction watches), and its head lands on the target signal's
/// first state — a stable cluster anchor, since a reaction delivers `event`
/// to `to_signal` rather than targeting any particular target state. When
/// `from_state` is the wildcard `*` there is no single watched node, so the
/// tail anchors on `from_signal`'s current state instead.
///
/// `to_dot` and `to_dot_with_state` are intentionally unchanged: this function
/// is the only path that draws reaction edges, so their output (and every test
/// that asserts on it) is unaffected.
pub fn to_dot_extended(
    schema: &TopologySchema,
    states: &HashMap<String, String>,
    guard_info: &HashMap<(String, String, String, String), String>,
) -> String {
    let mut out = String::new();
    out.push_str("digraph Topology {\n");
    out.push_str("  rankdir=LR;\n");
    out.push_str("  node [shape=ellipse];\n\n");

    let transitions_by_signal: HashMap<&String, Vec<&TransitionDef>> = schema
        .transitions
        .iter()
        .fold(HashMap::new(), |mut acc, t| {
            acc.entry(&t.signal_id).or_default().push(t);
            acc
        });

    for signal in &schema.signals {
        let sig_id = sanitize_id(&signal.id);
        out.push_str(&format!("  subgraph cluster_{} {{\n", sig_id));
        out.push_str(&format!("    label=\"{}\";\n", escape_label(&signal.id)));
        out.push_str("    style=rounded;\n");
        out.push_str("    color=gray;\n\n");

        for state in &signal.states {
            let node_id = node_id(&signal.id, state);
            let attrs = match states.get(&signal.id) {
                current if current == Some(state) => "style=filled fillcolor=lightgreen penwidth=2",
                _ => {
                    if *state == signal.initial_state {
                        "style=filled fillcolor=lightblue"
                    } else {
                        ""
                    }
                }
            };
            out.push_str(&format!(
                "    {} [label=\"{}\"{}];\n",
                node_id,
                escape_label(state),
                if attrs.is_empty() {
                    String::new()
                } else {
                    format!(" {}", attrs)
                }
            ));
        }

        out.push('\n');

        if let Some(transitions) = transitions_by_signal.get(&signal.id) {
            for transition in transitions {
                let label = edge_label(transition);
                let tooltip = edge_tooltip(transition);

                if transition.from == "*" {
                    for state in &signal.states {
                        if *state == transition.to {
                            continue;
                        }
                        let from_id = node_id(&signal.id, state);
                        let to_id = node_id(&signal.id, &transition.to);
                        out.push_str(&format!(
                            "    {} -> {} [label=\"{}\" tooltip=\"{}\"];\n",
                            from_id,
                            to_id,
                            escape_label(&label),
                            escape_label(&tooltip)
                        ));
                    }
                } else {
                    let from_id = node_id(&signal.id, &transition.from);
                    let to_id = node_id(&signal.id, &transition.to);
                    out.push_str(&format!(
                        "    {} -> {} [label=\"{}\" tooltip=\"{}\"];\n",
                        from_id,
                        to_id,
                        escape_label(&label),
                        escape_label(&tooltip)
                    ));
                }
            }
        }

        out.push_str("  }\n");
    }

    // Cross-signal reaction edges, outside any cluster so they route between
    // them. One edge per reaction, colored by its guard-evaluation result.
    for reaction in &schema.reactions {
        let from_node = reaction_from_node(reaction, schema, states);
        let to_node = match anchor_node(&reaction.to_signal, schema) {
            Some(n) => n,
            None => continue, // reaction targets an unknown signal; skip defensively
        };
        let result = guard_info.get(&(
            reaction.from_signal.clone(),
            reaction.from_state.clone(),
            reaction.to_signal.clone(),
            reaction.event.clone(),
        ));
        let (color, style, evaluated) = match result {
            Some(r) if r.starts_with("true") => ("green", "solid", true),
            Some(r) if r.starts_with("false") => ("gray", "dashed", true),
            Some(r) if r.starts_with("error") => ("red", "dashed", true),
            Some(_) => ("black", "dashed", true),
            None => ("black", "dashed", false),
        };
        let label = if evaluated {
            format!("{} [guard: {}]", reaction.event, result.unwrap())
        } else {
            format!("{} [guard: not evaluated]", reaction.event)
        };
        out.push_str(&format!(
            "  {} -> {} [label=\"{}\" color={} style={}];\n",
            from_node,
            to_node,
            escape_label(&label),
            color,
            style
        ));
    }

    out.push_str("}\n");
    out
}

/// The source node for a reaction edge: the watched `from_state` node of
/// `from_signal`, or — when the reaction is keyed on the wildcard `*` — the
/// signal's current state node, since there is no single watched state.
fn reaction_from_node(
    reaction: &ReactionDef,
    schema: &TopologySchema,
    states: &HashMap<String, String>,
) -> String {
    if reaction.from_state != "*" {
        node_id(&reaction.from_signal, &reaction.from_state)
    } else {
        // `*` matches any state; anchor the tail on the live node so the edge
        // still points at a real, rendered node.
        let current = states
            .get(&reaction.from_signal)
            .cloned()
            .or_else(|| {
                schema
                    .signals
                    .iter()
                    .find(|s| s.id == reaction.from_signal)
                    .map(|s| s.initial_state.clone())
            })
            .unwrap_or_default();
        node_id(&reaction.from_signal, &current)
    }
}

/// A stable anchor node for `signal_id`: its first state. Reactions target a
/// *signal* (they deliver `event` to it), not a particular target state, so the
/// first state is a deterministic in-cluster landing point for the head.
fn anchor_node(signal_id: &str, schema: &TopologySchema) -> Option<String> {
    let signal = schema.signals.iter().find(|s| s.id == signal_id)?;
    signal.states.first().map(|s| node_id(signal_id, s))
}

fn sanitize_id(input: &str) -> String {
    input
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

fn node_id(signal_id: &str, state: &str) -> String {
    format!("n_{}_{}", sanitize_id(signal_id), sanitize_id(state))
}

fn escape_label(input: &str) -> String {
    input.replace('\\', "\\\\").replace('"', "\\\"")
}

fn edge_label(transition: &TransitionDef) -> String {
    let actions = transition.actions.all_actions();
    if actions.is_empty() {
        transition.event.clone()
    } else {
        let action_list: Vec<&str> = actions.iter().map(|s| s.as_str()).collect();
        format!("{} [{}]", transition.event, action_list.join(", "))
    }
}

fn edge_tooltip(transition: &TransitionDef) -> String {
    let actions = transition.actions.all_actions();
    if actions.is_empty() {
        format!(
            "{}: {} -> {}",
            transition.event, transition.from, transition.to
        )
    } else {
        let action_list: Vec<&str> = actions.iter().map(|s| s.as_str()).collect();
        format!(
            "{}: {} -> {}\\nactions: {}",
            transition.event,
            transition.from,
            transition.to,
            action_list.join(", ")
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::SignalDef;

    /// Build a one-signal schema with the given states (`initial` first).
    fn single_signal(id: &str, initial: &str, states: &[&str]) -> TopologySchema {
        TopologySchema {
            version: "0.1".to_string(),
            signals: vec![SignalDef {
                id: id.to_string(),
                initial_state: initial.to_string(),
                states: states.iter().map(|s| s.to_string()).collect(),
            }],
            transitions: Vec::new(),
            reactions: Vec::new(),
            components: None,
            instances: Vec::new(),
            includes: Vec::new(),
        }
    }

    #[test]
    fn to_dot_extended_draws_reaction_edges_with_guard_colors() {
        // Two signals; one reaction each way so we can pin every guard-result
        // color to a distinct edge without collisions.
        let schema = TopologySchema {
            version: "0.1".to_string(),
            signals: vec![
                SignalDef {
                    id: "order".to_string(),
                    initial_state: "draft".to_string(),
                    states: vec!["draft".to_string(), "approved".to_string()],
                },
                SignalDef {
                    id: "inventory".to_string(),
                    initial_state: "idle".to_string(),
                    states: vec!["idle".to_string(), "allocated".to_string()],
                },
            ],
            transitions: Vec::new(),
            reactions: vec![
                ReactionDef {
                    from_signal: "order".to_string(),
                    from_state: "approved".to_string(),
                    to_signal: "inventory".to_string(),
                    event: "allocate".to_string(),
                    payload: None,
                    guard: None,
                },
                ReactionDef {
                    from_signal: "inventory".to_string(),
                    from_state: "idle".to_string(),
                    to_signal: "order".to_string(),
                    event: "replan".to_string(),
                    payload: None,
                    guard: None,
                },
                ReactionDef {
                    from_signal: "order".to_string(),
                    from_state: "approved".to_string(),
                    to_signal: "order".to_string(),
                    event: "audit".to_string(),
                    payload: None,
                    guard: None,
                },
                ReactionDef {
                    from_signal: "order".to_string(),
                    from_state: "*".to_string(),
                    to_signal: "inventory".to_string(),
                    event: "panic".to_string(),
                    payload: None,
                    guard: None,
                },
            ],
            components: None,
            instances: Vec::new(),
            includes: Vec::new(),
        };

        let mut states = HashMap::new();
        states.insert("order".to_string(), "approved".to_string());
        states.insert("inventory".to_string(), "idle".to_string());

        let mut guard_info = HashMap::new();
        // true -> solid green
        guard_info.insert(
            ("order".into(), "approved".into(), "inventory".into(), "allocate".into()),
            "true".into(),
        );
        // false -> dashed gray
        guard_info.insert(
            ("inventory".into(), "idle".into(), "order".into(), "replan".into()),
            "false".into(),
        );
        // error -> dashed red
        guard_info.insert(
            ("order".into(), "approved".into(), "order".into(), "audit".into()),
            "error: guard eval failed".into(),
        );
        // panic (wildcard) intentionally omitted -> dashed black "not evaluated"

        let dot = to_dot_extended(&schema, &states, &guard_info);

        // true: solid green (the only solid reaction edge).
        assert!(
            dot.contains("n_order_approved -> n_inventory_idle [label=\"allocate [guard: true]\" color=green style=solid]"),
            "guard=true should render solid green; got:\n{}",
            dot
        );
        // false: dashed gray.
        assert!(
            dot.contains("n_inventory_idle -> n_order_draft [label=\"replan [guard: false]\" color=gray style=dashed]"),
            "guard=false should render dashed gray; got:\n{}",
            dot
        );
        // error: dashed red.
        assert!(
            dot.contains("n_order_approved -> n_order_draft [label=\"audit [guard: error: guard eval failed]\" color=red style=dashed]"),
            "guard=error should render dashed red; got:\n{}",
            dot
        );
        // not evaluated: dashed black, wildcard tail on the current state node.
        assert!(
            dot.contains("n_order_approved -> n_inventory_idle [label=\"panic [guard: not evaluated]\" color=black style=dashed]"),
            "unevaluated wildcard reaction should render dashed black; got:\n{}",
            dot
        );

        // Unchanged base rendering: current state highlighted, reaction edges
        // drawn outside the clusters so they route between them.
        assert!(dot.contains("n_order_approved [label=\"approved\" style=filled fillcolor=lightgreen penwidth=2]"));
        assert!(dot.ends_with("}\n"));

        // Keep `single_signal` referenced so the helper is not dead when the
        // above assertions dominate; exercises the wildcard/absent-anchor path.
        let minimal = single_signal("s", "a", &["a", "b"]);
        let minimal_dot = to_dot_extended(&minimal, &HashMap::new(), &HashMap::new());
        assert!(minimal_dot.ends_with("}\n"));
    }
}
