// M28: `stc` — signal-topology-compiler.
//
// Compiles a DDL (Domain Description Language) source file into the engine's
// JSON topology schema. The compiled schema feeds the unmodified engine and
// the rest of the tool chain (`sts` / `stt` / `stp` / `stv`).
//
// Usage: stc [--check] <input.ddl> [output.json]
//   With no output path, the JSON is printed to stdout.
//   With `--check`, semantic warnings are printed to stderr (non-blocking)
//   before the JSON is written: self-loops and unreachable states.

use serde_json::{Map, Value};
use signal_topology::check::{check_ddl, check_schema};
use signal_topology::ddl::compile_full;
use signal_topology::schema::{ActionBinding, TopologySchema};
use std::env;
use std::fs;
use std::path::Path;
use std::process::exit;

fn main() {
    let args: Vec<String> = env::args().collect();

    // Pull the optional `--check` flag out of the arg list; everything else is
    // positional. This keeps `stc --check <ddl> [out]` and `stc <ddl> [out]`
    // without committing to a specific flag position.
    let mut check = false;
    let positional: Vec<&str> = args[1..]
        .iter()
        .filter_map(|a| {
            if a == "--check" {
                check = true;
                None
            } else {
                Some(a.as_str())
            }
        })
        .collect();

    if positional.is_empty() || positional.len() > 2 {
        eprintln!("Usage: stc [--check] <input.ddl> [output.json]");
        exit(1);
    }

    let input_path = Path::new(positional[0]);

    let src = fs::read_to_string(input_path).unwrap_or_else(|e| {
        eprintln!("Failed to read '{}': {}", input_path.display(), e);
        exit(1);
    });

    // M39: `compile_full` keeps the parsed `DdlDoc` AST alongside the lowered
    // schema so `--check` can run AST-level guard lints (which need the
    // top-level guard declarations and per-reaction guard references that
    // codegen erases) in addition to the schema-level checks.
    let (schema, ddl_doc) = compile_full(&src).unwrap_or_else(|e| {
        eprintln!("Failed to compile '{}': {}", input_path.display(), e);
        exit(1);
    });

    // M36/M39: `--check` runs the semantic checkers and prints any warnings to
    // stderr. They are non-blocking — warnings never abort the run or change
    // the exit code — so the JSON below is still produced. When an output
    // path is also supplied the user gets both: warnings now, topology later.
    // M39 adds AST-level guard lints (unused template / duplicate condition)
    // on top of the schema-level self-loop and unreachable-state checks.
    if check {
        let mut warnings = check_schema(&schema);
        warnings.extend(check_ddl(&ddl_doc, &schema));
        for w in &warnings {
            eprintln!("Warning: {}: {}", w.kind, w.message);
        }
        match warnings.len() {
            0 => eprintln!("No warnings found."),
            n => eprintln!("{} warning(s) found.", n),
        }
    }

    // `TopologySchema` is Deserialize-only (engine layer, untouched), so we
    // serialize manually from its public fields.
    let json = serde_json::to_string_pretty(&schema_to_value(&schema)).unwrap_or_else(|e| {
        eprintln!("Failed to serialize schema: {}", e);
        exit(1);
    });

    if positional.len() == 2 {
        let output_path = Path::new(positional[1]);
        fs::write(output_path, &json).unwrap_or_else(|e| {
            eprintln!("Failed to write '{}': {}", output_path.display(), e);
            exit(1);
        });
        println!("Compiled {} -> {}", input_path.display(), output_path.display());
    } else {
        println!("{}", json);
    }
}

/// Serialize a `TopologySchema` to a JSON `Value`. The engine type is
/// Deserialize-only (and we must not alter `schema.rs`), so we map its public
/// fields by hand. Empty `reactions` are omitted to match the canonical
/// examples; `components` / `instances` / `includes` are never produced by the
/// DDL compiler.
fn schema_to_value(schema: &TopologySchema) -> Value {
    let mut root = Map::new();
    root.insert("version".to_string(), Value::String(schema.version.clone()));

    let signals = schema.signals.iter().map(signal_to_value).collect::<Vec<_>>();
    root.insert("signals".to_string(), Value::Array(signals));

    let transitions = schema
        .transitions
        .iter()
        .map(transition_to_value)
        .collect::<Vec<_>>();
    root.insert("transitions".to_string(), Value::Array(transitions));

    if !schema.reactions.is_empty() {
        let reactions = schema
            .reactions
            .iter()
            .map(|r| {
                let mut m = Map::new();
                m.insert(
                    "from_signal".to_string(),
                    Value::String(r.from_signal.clone()),
                );
                m.insert(
                    "from_state".to_string(),
                    Value::String(r.from_state.clone()),
                );
                m.insert("to_signal".to_string(), Value::String(r.to_signal.clone()));
                m.insert("event".to_string(), Value::String(r.event.clone()));
                // M38: a reaction may carry a guard (either written literally or
                // expanded from a `guard <id>` template). Mirror the transition
                // serializer and emit it only when present.
                if let Some(guard) = &r.guard {
                    m.insert("guard".to_string(), Value::String(guard.clone()));
                }
                // M44: fork/join fields. Emit only when set, mirroring the
                // `#[serde(default)]` schema so legacy JSON stays unchanged.
                if let Some(group) = &r.join_group {
                    m.insert("join_group".to_string(), Value::String(group.clone()));
                }
                if !r.requires.is_empty() {
                    m.insert(
                        "requires".to_string(),
                        Value::Array(
                            r.requires.iter().map(|x| Value::String(x.clone())).collect(),
                        ),
                    );
                }
                Value::Object(m)
            })
            .collect::<Vec<_>>();
        root.insert("reactions".to_string(), Value::Array(reactions));
    }

    Value::Object(root)
}

fn signal_to_value(s: &signal_topology::schema::SignalDef) -> Value {
    let mut m = Map::new();
    m.insert("id".to_string(), Value::String(s.id.clone()));
    m.insert(
        "initial_state".to_string(),
        Value::String(s.initial_state.clone()),
    );
    m.insert(
        "states".to_string(),
        Value::Array(s.states.iter().map(|x| Value::String(x.clone())).collect()),
    );
    Value::Object(m)
}

fn transition_to_value(t: &signal_topology::schema::TransitionDef) -> Value {
    let mut m = Map::new();
    m.insert("signal_id".to_string(), Value::String(t.signal_id.clone()));
    m.insert("from".to_string(), Value::String(t.from.clone()));
    m.insert("event".to_string(), Value::String(t.event.clone()));
    m.insert("to".to_string(), Value::String(t.to.clone()));
    m.insert("actions".to_string(), action_binding_to_value(&t.actions));
    if let Some(guard) = &t.guard {
        m.insert("guard".to_string(), Value::String(guard.clone()));
    }
    Value::Object(m)
}

fn action_binding_to_value(a: &ActionBinding) -> Value {
    let mut m = Map::new();
    if !a.on_exit.is_empty() {
        m.insert(
            "on_exit".to_string(),
            Value::Array(a.on_exit.iter().map(|x| Value::String(x.clone())).collect()),
        );
    }
    if !a.on_transition.is_empty() {
        m.insert(
            "on_transition".to_string(),
            Value::Array(
                a.on_transition
                    .iter()
                    .map(|x| Value::String(x.clone()))
                    .collect(),
            ),
        );
    }
    if !a.on_enter.is_empty() {
        m.insert(
            "on_enter".to_string(),
            Value::Array(a.on_enter.iter().map(|x| Value::String(x.clone())).collect()),
        );
    }
    Value::Object(m)
}
