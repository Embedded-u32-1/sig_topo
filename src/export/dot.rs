use crate::schema::{TopologySchema, TransitionDef};
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
