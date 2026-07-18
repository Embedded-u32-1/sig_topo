use signal_topology::schema::{
    ActionBinding, ComponentDef, ConnectionDef, InstanceDef, PortDef, PortDirection, ReactionDef,
    SignalDef, TopologySchema, TransitionDef,
};
use signal_topology::{expand, from_path, load_topology, EngineError, TopologyEngine};

// ---------------------------------------------------------------------------
// Helpers to build schemas/components/instances by hand for fine-grained tests.
// ---------------------------------------------------------------------------

fn signal(id: &str, initial: &str, states: &[&str]) -> SignalDef {
    SignalDef {
        id: id.to_string(),
        initial_state: initial.to_string(),
        states: states.iter().map(|s| s.to_string()).collect(),
    }
}

/// A reusable "lockable" component: a signal `${name}` that flips between
/// `locked`/`unlocked`. Useful for exercising `${name}` in id + state names.
fn lockable_component() -> ComponentDef {
    ComponentDef {
        ports: vec![],
        params: vec!["name".to_string()],
        signals: vec![signal("${name}", "unlocked", &["locked", "unlocked"])],
        transitions: vec![
            TransitionDef {
                signal_id: "${name}".to_string(),
                from: "unlocked".to_string(),
                event: "lock".to_string(),
                to: "locked".to_string(),
                actions: ActionBinding::default(),
                guard: None,
            },
            TransitionDef {
                signal_id: "${name}".to_string(),
                from: "locked".to_string(),
                event: "unlock".to_string(),
                to: "unlocked".to_string(),
                actions: ActionBinding::default(),
                guard: None,
            },
        ],
        reactions: vec![],
    }
}

/// A component carrying a reaction driven by `${kind}`.
fn notify_component() -> ComponentDef {
    ComponentDef {
        ports: vec![],
        params: vec!["src".to_string(), "kind".to_string()],
        signals: vec![signal("${kind}", "idle", &["idle", "done"])],
        transitions: vec![TransitionDef {
            signal_id: "${kind}".to_string(),
            from: "idle".to_string(),
            event: "complete".to_string(),
            to: "done".to_string(),
            actions: ActionBinding::default(),
            guard: None,
        }],
        reactions: vec![ReactionDef {
            from_signal: "${src}".to_string(),
            from_state: "go".to_string(),
            to_signal: "${kind}".to_string(),
            event: "complete".to_string(),
            payload: None,
            guard: None,
            join_group: None,
            requires: Vec::new(),
        }],
    }
}

use std::collections::HashMap;

fn base_schema() -> TopologySchema {
    TopologySchema {
        version: "0.7".to_string(),
        signals: vec![],
        transitions: vec![],
        reactions: vec![],
        components: None,
        instances: vec![],
        includes: vec![],
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn test_expand_single_instance_flattens_signals_and_transitions() {
    let mut components = HashMap::new();
    components.insert("lockable".to_string(), lockable_component());

    let schema = TopologySchema {
        components: Some(components),
        instances: vec![InstanceDef {
            connections: vec![],
            component: "lockable".to_string(),
            bindings: HashMap::from([("name".to_string(), "door".to_string())]),
        }],
        ..base_schema()
    };

    let flat = expand(schema).expect("expand should succeed");

    // components/instances/includes are stripped; signals expanded.
    assert!(flat.components.is_none());
    assert!(flat.instances.is_empty());
    assert_eq!(flat.signals.len(), 1);
    assert_eq!(flat.signals[0].id, "door");
    assert_eq!(flat.signals[0].states, vec!["locked", "unlocked"]);
    assert_eq!(flat.transitions.len(), 2);
    assert_eq!(flat.transitions[0].to, "locked");
    assert_eq!(flat.transitions[1].to, "unlocked");
}

#[test]
fn test_expand_same_component_twice_with_different_bindings() {
    let mut components = HashMap::new();
    components.insert("lockable".to_string(), lockable_component());

    let schema = TopologySchema {
        components: Some(components),
        instances: vec![
            InstanceDef {
                connections: vec![],
                component: "lockable".to_string(),
                bindings: HashMap::from([("name".to_string(), "door".to_string())]),
            },
            InstanceDef {
                connections: vec![],
                component: "lockable".to_string(),
                bindings: HashMap::from([("name".to_string(), "window".to_string())]),
            },
        ],
        ..base_schema()
    };

    let flat = expand(schema).expect("expand should succeed");

    let ids: Vec<&str> = flat.signals.iter().map(|s| s.id.as_str()).collect();
    assert!(ids.contains(&"door"));
    assert!(ids.contains(&"window"));
    assert_eq!(flat.signals.len(), 2);
    // Each signal contributes two transitions (lock/unlock).
    assert_eq!(flat.transitions.len(), 4);
    assert!(flat.transitions.iter().any(|t| t.signal_id == "door" && t.to == "locked"));
    assert!(flat.transitions.iter().any(|t| t.signal_id == "window" && t.to == "locked"));
}

#[test]
fn test_expand_param_injected_into_state_names() {
    // Component where the param `${kind}` drives the generated state + id.
    let mut components = HashMap::new();
    components.insert("notify".to_string(), notify_component());

    let schema = TopologySchema {
        components: Some(components),
        instances: vec![InstanceDef {
            connections: vec![],
            component: "notify".to_string(),
            bindings: HashMap::from([
                ("src".to_string(), "master".to_string()),
                ("kind".to_string(), "email".to_string()),
            ]),
        }],
        ..base_schema()
    };

    let flat = expand(schema).expect("expand should succeed");

    assert_eq!(flat.signals[0].id, "email");
    assert_eq!(flat.signals[0].states, vec!["idle", "done"]);
    assert_eq!(flat.reactions[0].to_signal, "email");
    assert_eq!(flat.reactions[0].from_signal, "master");
}

#[test]
fn test_expand_missing_binding_returns_error() {
    let mut components = HashMap::new();
    components.insert("lockable".to_string(), lockable_component());

    // Missing the required `name` binding.
    let schema = TopologySchema {
        components: Some(components),
        instances: vec![InstanceDef {
            connections: vec![],
            component: "lockable".to_string(),
            bindings: HashMap::new(),
        }],
        ..base_schema()
    };

    let err = expand(schema).expect_err("should fail with missing binding");
    assert!(
        matches!(err, EngineError::MissingBinding { param, .. } if param == "name")
    );
}

#[test]
fn test_expand_unknown_component_returns_error() {
    let schema = TopologySchema {
        components: Some(HashMap::new()),
        instances: vec![InstanceDef {
            connections: vec![],
            component: "does_not_exist".to_string(),
            bindings: HashMap::new(),
        }],
        ..base_schema()
    };

    let err = expand(schema).expect_err("should fail with component not found");
    assert!(
        matches!(err, EngineError::ComponentNotFound(name) if name == "does_not_exist")
    );
}

#[test]
fn test_expand_duplicate_signal_after_expand_returns_error() {
    let mut components = HashMap::new();
    components.insert("lockable".to_string(), lockable_component());

    // Two instances resolve to the same signal id "door".
    let schema = TopologySchema {
        components: Some(components),
        instances: vec![
            InstanceDef {
                connections: vec![],
                component: "lockable".to_string(),
                bindings: HashMap::from([("name".to_string(), "door".to_string())]),
            },
            InstanceDef {
                connections: vec![],
                component: "lockable".to_string(),
                bindings: HashMap::from([("name".to_string(), "door".to_string())]),
            },
        ],
        ..base_schema()
    };

    let err = expand(schema).expect_err("should fail with duplicate signal");
    assert!(
        matches!(err, EngineError::DuplicateSignalAfterExpand(id) if id == "door")
    );
}

#[test]
fn test_expand_empty_instances_preserves_schema() {
    let mut schema = base_schema();
    schema.signals.push(signal("s1", "a", &["a", "b"]));
    // include components + includes to verify they are preserved on no-op path.
    schema.components = Some(HashMap::new());
    schema.includes.push("other.json".to_string());

    let out = expand(schema).expect("no-op expand should succeed");
    assert_eq!(out.signals.len(), 1);
    assert_eq!(out.signals[0].id, "s1");
    // No-op path keeps the original fields intact.
    assert!(out.components.is_some());
    assert_eq!(out.includes, vec!["other.json".to_string()]);
}

#[test]
fn test_expand_invalid_param_ref_returns_error() {
    // Component references `${extra}` which is not declared in `params`.
    let bad = ComponentDef {
        ports: vec![],
        params: vec!["name".to_string()],
        signals: vec![signal("${extra}", "a", &["a"])],
        transitions: vec![],
        reactions: vec![],
    };

    let mut components = HashMap::new();
    components.insert("bad".to_string(), bad);

    let schema = TopologySchema {
        components: Some(components),
        instances: vec![InstanceDef {
            connections: vec![],
            component: "bad".to_string(),
            bindings: HashMap::from([("name".to_string(), "x".to_string())]),
        }],
        ..base_schema()
    };

    let err = expand(schema).expect_err("should fail with invalid param ref");
    assert!(
        matches!(err, EngineError::InvalidParamRef { param, .. } if param == "extra")
    );
}

// ---------------------------------------------------------------------------
// End-to-end: expand then drive the engine through the generated topology.
// ---------------------------------------------------------------------------

#[test]
fn test_end_to_end_expanded_component_runs_in_engine() {
    let mut components = HashMap::new();
    components.insert("lockable".to_string(), lockable_component());

    let schema = TopologySchema {
        components: Some(components),
        instances: vec![
            InstanceDef {
                connections: vec![],
                component: "lockable".to_string(),
                bindings: HashMap::from([("name".to_string(), "door".to_string())]),
            },
            InstanceDef {
                connections: vec![],
                component: "lockable".to_string(),
                bindings: HashMap::from([("name".to_string(), "window".to_string())]),
            },
        ],
        ..base_schema()
    };

    let mut engine = TopologyEngine::from_schema(schema).expect("engine should load expanded schema");

    assert_eq!(engine.get_state("door").unwrap(), "unlocked");
    assert_eq!(engine.get_state("window").unwrap(), "unlocked");

    let r = engine.send_event("door", "lock", None).expect("lock door");
    assert_eq!(r.to, "locked");
    assert_eq!(engine.get_state("door").unwrap(), "locked");
    // window unaffected by door's transition
    assert_eq!(engine.get_state("window").unwrap(), "unlocked");

    let r = engine.send_event("window", "lock", None).expect("lock window");
    assert_eq!(r.to, "locked");

    let r = engine.send_event("door", "unlock", None).expect("unlock door");
    assert_eq!(r.to, "unlocked");
}

// ---------------------------------------------------------------------------
// M45 — sub-topology composition with ports + wired connections.
// ---------------------------------------------------------------------------

/// A "lockable" component whose internal signal is `lock` (not param-driven).
/// It exposes an `out` port on `lock.locked` aliased `locked`, so a parent can
/// wire that exposed signal to any parent-level signal.
fn lockable_port_component() -> ComponentDef {
    ComponentDef {
        params: vec![],
        ports: vec![PortDef {
            direction: PortDirection::Out,
            signal: "lock".to_string(),
            state: "locked".to_string(),
            alias: Some("locked".to_string()),
        }],
        signals: vec![signal("lock", "unlocked", &["locked", "unlocked"])],
        transitions: vec![
            TransitionDef {
                signal_id: "lock".to_string(),
                from: "unlocked".to_string(),
                event: "lock".to_string(),
                to: "locked".to_string(),
                actions: ActionBinding::default(),
                guard: None,
            },
            TransitionDef {
                signal_id: "lock".to_string(),
                from: "locked".to_string(),
                event: "unlock".to_string(),
                to: "unlocked".to_string(),
                actions: ActionBinding::default(),
                guard: None,
            },
        ],
        reactions: vec![],
    }
}

/// A component whose internal reaction targets an exposed port signal. After
/// wiring, that reaction must fire against the parent signal it was wired to.
fn notify_port_component() -> ComponentDef {
    ComponentDef {
        params: vec![],
        ports: vec![PortDef {
            direction: PortDirection::Out,
            signal: "flag".to_string(),
            state: "set".to_string(),
            alias: Some("flag_set".to_string()),
        }],
        signals: vec![signal("flag", "clear", &["clear", "set"])],
        transitions: vec![TransitionDef {
            signal_id: "flag".to_string(),
            from: "clear".to_string(),
            event: "raise".to_string(),
            to: "set".to_string(),
            actions: ActionBinding::default(),
            guard: None,
        }],
        reactions: vec![ReactionDef {
            from_signal: "flag".to_string(),
            from_state: "set".to_string(),
            to_signal: "audit".to_string(),
            event: "note".to_string(),
            payload: None,
            guard: None,
            join_group: None,
            requires: Vec::new(),
        }],
    }
}

#[test]
fn test_expand_connection_remaps_port_signal_to_parent() {
    let mut components = HashMap::new();
    components.insert("lockable".to_string(), lockable_port_component());

    let schema = TopologySchema {
        components: Some(components),
        instances: vec![InstanceDef {
            component: "lockable".to_string(),
            bindings: HashMap::new(),
            connections: vec![ConnectionDef {
                port: "locked".to_string(),
                target_signal: "door".to_string(),
            }],
        }],
        ..base_schema()
    };

    let flat = expand(schema).expect("expand should succeed");

    // The internal signal `lock` is renamed to the wired parent signal `door`.
    let ids: Vec<&str> = flat.signals.iter().map(|s| s.id.as_str()).collect();
    assert!(ids.contains(&"door"), "expected 'door' in signals, got {:?}", ids);
    assert!(!ids.contains(&"lock"), "internal 'lock' should be renamed away");
    assert_eq!(flat.signals.len(), 1);

    // The transition that was on `door` (wire target) now references `door`.
    assert!(flat
        .transitions
        .iter()
        .all(|t| t.signal_id == "door"), "all transitions should target 'door'");
    assert_eq!(flat.transitions.len(), 2);
}

#[test]
fn test_expand_connection_remaps_internal_reaction_to_parent_signal() {
    let mut components = HashMap::new();
    components.insert("notify".to_string(), notify_port_component());

    let schema = TopologySchema {
        components: Some(components),
        instances: vec![InstanceDef {
            component: "notify".to_string(),
            bindings: HashMap::new(),
            connections: vec![ConnectionDef {
                port: "flag_set".to_string(),
                target_signal: "alarm".to_string(),
            }],
        }],
        ..base_schema()
    };

    let flat = expand(schema).expect("expand should succeed");

    // Internal signal `flag` renamed to the wired parent signal `alarm`.
    let ids: Vec<&str> = flat.signals.iter().map(|s| s.id.as_str()).collect();
    assert!(ids.contains(&"alarm"), "expected 'alarm', got {:?}", ids);
    assert!(!ids.contains(&"flag"));

    // The component's internal reaction `flag.set -> audit note` must now read
    // `alarm.set -> audit note`, i.e. its from_signal became the parent signal.
    let r = flat
        .reactions
        .iter()
        .find(|r| r.event == "note")
        .expect("reaction should survive expansion");
    assert_eq!(r.from_signal, "alarm");
    assert_eq!(r.from_state, "set");
    assert_eq!(r.to_signal, "audit");
}

#[test]
fn test_expand_connection_port_not_found_is_error() {
    let mut components = HashMap::new();
    components.insert("lockable".to_string(), lockable_port_component());

    let schema = TopologySchema {
        components: Some(components),
        instances: vec![InstanceDef {
            component: "lockable".to_string(),
            bindings: HashMap::new(),
            connections: vec![ConnectionDef {
                port: "does_not_exist".to_string(),
                target_signal: "door".to_string(),
            }],
        }],
        ..base_schema()
    };

    let err = expand(schema).expect_err("unknown port should error");
    assert!(
        matches!(err, EngineError::UnknownPort { port, .. } if port == "does_not_exist")
    );
}

#[test]
fn test_expand_connection_port_by_signal_state() {
    // A port without an alias is addressed as `<signal>.<state>`.
    let mut components = HashMap::new();
    components.insert(
        "c".to_string(),
        ComponentDef {
            params: vec![],
            ports: vec![PortDef {
                direction: PortDirection::Out,
                signal: "inner".to_string(),
                state: "go".to_string(),
                alias: None,
            }],
            signals: vec![signal("inner", "a", &["a", "go"])],
            transitions: vec![TransitionDef {
                signal_id: "inner".to_string(),
                from: "a".to_string(),
                event: "step".to_string(),
                to: "go".to_string(),
                actions: ActionBinding::default(),
                guard: None,
            }],
            reactions: vec![],
        },
    );

    let schema = TopologySchema {
        components: Some(components),
        instances: vec![InstanceDef {
            component: "c".to_string(),
            bindings: HashMap::new(),
            connections: vec![ConnectionDef {
                port: "inner.go".to_string(),
                target_signal: "outer".to_string(),
            }],
        }],
        ..base_schema()
    };

    let flat = expand(schema).expect("expand should succeed");
    let ids: Vec<&str> = flat.signals.iter().map(|s| s.id.as_str()).collect();
    assert!(ids.contains(&"outer"));
    assert!(flat.transitions.iter().all(|t| t.signal_id == "outer"));
}

// ---------------------------------------------------------------------------
// M17 — cross-file imports.
// ---------------------------------------------------------------------------

/// Absolute path to a fixture file, resolved against the crate root so the test
/// passes regardless of the process's working directory.
fn fixture(name: &str) -> std::path::PathBuf {
    let mut p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("tests/fixtures");
    p.push(name);
    p
}

#[test]
fn test_load_topology_flat_file() {
    let schema = load_topology(&fixture("lockable.json")).expect("load flat file");
    assert_eq!(schema.signals.len(), 1);
    assert_eq!(schema.signals[0].id, "lock");
    // Flat file: includes should be empty post-load.
    assert!(schema.includes.is_empty());
}

// Aliases matching the spec's exact test names; each delegates to the concrete
// assertions above/below so behavior is covered regardless of naming.
#[test]
fn test_load_topology_plain_file() {
    test_load_topology_flat_file();
}

#[test]
fn test_load_topology_with_includes() {
    let schema = load_topology(&fixture("breaker.json")).expect("load with includes");

    let ids: Vec<&str> = schema.signals.iter().map(|s| s.id.as_str()).collect();
    // Main file contributes "breaker"; included lockable.json contributes "lock".
    assert!(ids.contains(&"breaker"), "breaker missing from merged signals");
    assert!(ids.contains(&"lock"), "lock missing from merged signals");
    assert_eq!(schema.signals.len(), 2);

    // Top-level `includes` is cleared after being resolved.
    assert!(schema.includes.is_empty());
    // Main file's version wins.
    assert_eq!(schema.version, "0.7");
}

#[test]
fn test_load_topology_merges_includes() {
    let schema = load_topology(&fixture("main_ok.json")).expect("load with includes");

    let ids: Vec<&str> = schema.signals.iter().map(|s| s.id.as_str()).collect();
    assert!(ids.contains(&"lock"), "lock missing from merged signals");
    assert!(ids.contains(&"notifier"), "notifier missing from merged signals");

    // The notify.json reaction ("lock" locked -> notifier fire) survives the merge.
    assert!(
        schema
            .reactions
            .iter()
            .any(|r| r.from_signal == "lock" && r.to_signal == "notifier" && r.event == "fire"),
        "reaction missing after merge"
    );

    // Top-level `includes` is cleared after being resolved.
    assert!(schema.includes.is_empty());
    // main file's version wins.
    assert_eq!(schema.version, "0.7");
}

#[test]
fn test_load_topology_transitive_include() {
    // trans_a includes trans_b includes trans_c; all three signals must merge.
    let schema = load_topology(&fixture("trans_a.json")).expect("load transitive chain");

    let ids: Vec<&str> = schema.signals.iter().map(|s| s.id.as_str()).collect();
    assert!(ids.contains(&"a"), "a missing (root file)");
    assert!(ids.contains(&"b"), "b missing (first include)");
    assert!(ids.contains(&"c"), "c missing (second include)");
    assert_eq!(schema.signals.len(), 3);
    assert!(schema.includes.is_empty());
}

#[test]
fn test_load_topology_cross_file_duplicate_signal() {
    // dup_main and dup_inc both define "same_id".
    let err = load_topology(&fixture("dup_main.json")).expect_err("dup must error");
    assert!(
        matches!(err, EngineError::DuplicateSignalAfterExpand(ref id) if id == "same_id"),
        "expected DuplicateSignalAfterExpand(same_id), got {:?}",
        err
    );
}

#[test]
fn test_from_path_builds_engine() {
    // from_path == load_topology + TopologyEngine::from_schema.
    let engine = from_path(&fixture("breaker.json")).expect("from_path should build engine");
    assert_eq!(engine.get_state("breaker").unwrap(), "on");
    assert_eq!(engine.get_state("lock").unwrap(), "unlocked");

    // Also verify a transitive chain builds a working engine.
    let engine2 = from_path(&fixture("trans_a.json")).expect("transitive from_path builds");
    let ids: Vec<&str> = engine2.signal_ids();
    assert!(ids.contains(&"a"));
    assert!(ids.contains(&"b"));
    assert!(ids.contains(&"c"));
}

#[test]
fn test_load_topology_cycle_detection() {
    let err = load_topology(&fixture("cycle_a.json")).expect_err("cycle must be detected");
    assert!(
        matches!(err, EngineError::IncludeCycle(_)),
        "expected IncludeCycle, got {:?}",
        err
    );
}

#[test]
fn test_load_topology_include_not_found() {
    let err = load_topology(&fixture("include_missing.json"))
        .expect_err("missing include must error");
    assert!(
        matches!(err, EngineError::IncludeNotFound(_)),
        "expected IncludeNotFound, got {:?}",
        err
    );
}

#[test]
fn test_load_topology_then_engine_end_to_end() {
    // Drive the merged topology through the engine: lock "lock", which should
    // fire the reaction and drive "notifier" to "done".
    let mut engine = TopologyEngine::from_schema(
        load_topology(&fixture("main_ok.json")).expect("load merged"),
    )
    .expect("engine builds from merged schema");

    assert_eq!(engine.get_state("lock").unwrap(), "unlocked");
    assert_eq!(engine.get_state("notifier").unwrap(), "idle");

    engine.send_event("lock", "lock", None).expect("lock fires");
    assert_eq!(engine.get_state("lock").unwrap(), "locked");
    // Reaction: lock=locked -> notifier fire -> notifier done.
    assert_eq!(engine.get_state("notifier").unwrap(), "done");
}

// ---------------------------------------------------------------------------
// Regression tests for the M1 review finding: substitution must be deterministic
// (no dependence on HashMap iteration order) and must not rescan substituted
// text (no double-interpretation of `${...}` that appears inside a value).
// ---------------------------------------------------------------------------

/// A component whose binding value itself contains a `${other}` placeholder.
/// The literal `${kind}` inside the value must survive verbatim rather than
/// being re-interpreted as another param.
fn value_with_nested_ref_component() -> ComponentDef {
    ComponentDef {
        ports: vec![],
        params: vec!["name".to_string(), "kind".to_string()],
        signals: vec![SignalDef {
            // value of `name` is "${kind}" — after substitution the signal id
            // must be literally "${kind}", not the value of `kind`.
            id: "${name}".to_string(),
            initial_state: "a".to_string(),
            states: vec!["a".to_string()],
        }],
        transitions: vec![],
        reactions: vec![],
    }
}

#[test]
fn test_expand_does_not_rescan_substituted_text() {
    let mut components = HashMap::new();
    components.insert("v".to_string(), value_with_nested_ref_component());

    let schema = TopologySchema {
        components: Some(components),
        instances: vec![InstanceDef {
            connections: vec![],
            component: "v".to_string(),
            bindings: HashMap::from([
                ("name".to_string(), "${kind}".to_string()),
                ("kind".to_string(), "real".to_string()),
            ]),
        }],
        ..base_schema()
    };

    let flat = expand(schema).expect("expand should succeed");
    // The substituted text "${kind}" is written verbatim and never re-scanned.
    assert_eq!(flat.signals[0].id, "${kind}");
}

#[test]
fn test_expand_substitution_is_deterministic() {
    // Run expand twenty times over the same pathological schema (value contains
    // `${other}`). With a HashMap-iterating subst the result would vary between
    // runs; it must be stable here.
    let build = || -> TopologySchema {
        let mut components = HashMap::new();
        components.insert("v".to_string(), value_with_nested_ref_component());
        TopologySchema {
            components: Some(components),
            instances: vec![InstanceDef {
                connections: vec![],
                component: "v".to_string(),
                bindings: HashMap::from([
                    ("name".to_string(), "${kind}".to_string()),
                    ("kind".to_string(), "real".to_string()),
                ]),
            }],
            ..base_schema()
        }
    };

    let first = expand(build()).expect("expand should succeed").signals[0].id.clone();
    for _ in 0..20 {
        let id = &expand(build()).expect("expand should succeed").signals[0].id;
        assert_eq!(id, &first, "substitution must be deterministic across runs");
    }
    assert_eq!(first, "${kind}");
}

#[test]
fn test_expand_missing_binding_reports_component_name() {
    // M2 regression: MissingBinding error must carry the real component name.
    let mut components = HashMap::new();
    components.insert("lockable".to_string(), lockable_component());

    let schema = TopologySchema {
        components: Some(components),
        instances: vec![InstanceDef {
            connections: vec![],
            component: "lockable".to_string(),
            bindings: HashMap::new(), // missing `name`
        }],
        ..base_schema()
    };

    let err = expand(schema).expect_err("should fail with missing binding");
    assert!(
        matches!(&err, EngineError::MissingBinding { component, param, .. }
            if component == "lockable" && param == "name"),
        "MissingBinding should report real component name, got {:?}",
        err
    );
}
