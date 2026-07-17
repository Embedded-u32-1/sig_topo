// M36: semantic checks on a compiled `TopologySchema`.
//
// Run by `stc --check` over the schema the DDL compiler produced, *before* the
// JSON is emitted. The checks look for suspicious patterns (self-loops,
// unreachable states) and return them as warnings — non-blocking, never an
// error. The warning list is printed to stderr by the `stc` binary; the JSON
// is still written normally.
//
// All of the logic here is a pure function of the schema (no IO, no parser
// dependency), so it unit-tests cleanly and can later be reused from an LSP
// or a `--watch` mode. The schema alone does not carry line/column info, so
// `line`/`col` stay `None`; enrich them later if check ever takes the AST.

use crate::ddl::parser::{DdlDoc, ReactionDecl};
use crate::schema::TopologySchema;
use std::collections::{HashMap, HashSet};
use std::fmt;

/// A suspicious pattern found by `check_schema`. Warnings are non-blocking:
/// they are reported (by the `stc` binary) but never abort compilation or
/// change the exit code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckWarning {
    /// The kind of warning. Rendered as the `self-loop` / `unreachable-state`
    /// label in the CLI output.
    pub kind: WarningKind,
    /// Human-readable detail, e.g. `gate: closed -> closed` for a self-loop or
    /// `inventory: obsolete` for an unreachable state.
    pub message: String,
    /// Source line of the offending construct, when known. The schema-only
    /// entry point has no source location, so this is `None`.
    pub line: Option<usize>,
    /// Source column of the offending construct, when known.
    pub col: Option<usize>,
}

/// The kinds of suspicious patterns `check_schema` reports. Each maps to one
/// branch of the checker; extend the enum and the match in `check_schema` to
/// add a new lint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WarningKind {
    /// A transition whose `from` and `to` are the same state
    /// (`from == to`). This includes the self-loop a wildcard `from *` lowers
    /// into — `gate_flow`'s `closed -> closed` is the canonical case. It is
    /// usually harmless, but surfacing it lets the user confirm (or remove) it.
    SelfLoop,
    /// A state that is never entered: it is not the signal's initial state and
    /// it is not the `to` target of any *other* state's transition. Such a
    /// state is dead — the engine can never occupy it.
    UnreachableState,
    /// A top-level `guard <id> { ... }` declaration that no reaction
    /// references via `when <id>`. The template is dead code — it is never
    /// evaluated — so the user can delete it (or wire it up).
    UnusedGuardTemplate,
    /// Two or more top-level guard declarations with identical expression
    /// text. They likely express the same condition expressed twice and could
    /// be merged into one template referenced from both reactions.
    DuplicateGuardCondition,
}

impl fmt::Display for WarningKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WarningKind::SelfLoop => write!(f, "self-loop"),
            WarningKind::UnreachableState => write!(f, "unreachable-state"),
            WarningKind::UnusedGuardTemplate => write!(f, "unused-guard-template"),
            WarningKind::DuplicateGuardCondition => write!(f, "duplicate-guard-condition"),
        }
    }
}

/// Scan a compiled `TopologySchema` for suspicious patterns and return the
/// resulting warnings.
///
/// Two checks run today:
///
/// 1. **Self-loop** — any transition with `from == to`. Because the compiler
///    lowers a wildcard `from *` to one concrete transition per source state,
///    this catches both literal self-loops (`on ev from a -> a`) *and* the
///    wildcard-produced self-loop (`closed -> closed` in `gate_flow`). They
///    are indistinguishable from the schema alone, and both are worth
///    surfacing, so we report every `from == to` pair.
/// 2. **Unreachable state** — for each signal, the reachable set is the
///    initial state plus the `to` of every non-self-loop transition; a signal
///    state outside that set is dead.
///
/// The function is pure: no IO, no globals, total w.r.t. its inputs.
pub fn check_schema(schema: &TopologySchema) -> Vec<CheckWarning> {
    let mut warnings = Vec::new();

    // 1. Self-loops: every transition whose source equals its target.
    for t in &schema.transitions {
        if t.from == t.to {
            warnings.push(CheckWarning {
                kind: WarningKind::SelfLoop,
                message: format!("{}: {} -> {}", t.signal_id, t.from, t.to),
                line: None,
                col: None,
            });
        }
    }

    // 2. Unreachable states: a state is reachable iff it is the initial state
    //    or the `to` of some non-self-loop transition belonging to the signal.
    for sig in &schema.signals {
        let mut reachable = HashSet::new();
        reachable.insert(&sig.initial_state);

        for t in &schema.transitions {
            if t.signal_id != sig.id {
                continue;
            }
            // A self-loop's `to` contributes nothing: it can only be taken
            // once you are already in the state, so it never *reaches* the
            // state from elsewhere.
            if t.from == t.to {
                continue;
            }
            reachable.insert(&t.to);
        }

        for state in &sig.states {
            if !reachable.contains(state) {
                warnings.push(CheckWarning {
                    kind: WarningKind::UnreachableState,
                    message: format!("{}: {}", sig.id, state),
                    line: None,
                    col: None,
                });
            }
        }
    }

    warnings
}

/// AST-level checks on the parsed `DdlDoc`, run by `stc --check` alongside the
/// schema-level `check_schema`.
///
/// `check_schema` sees only the lowered `TopologySchema`, which has already
/// expanded every `when <id>` guard reference into literal expression text — so
/// it cannot tell whether a top-level guard template is used, or whether two
/// templates repeat the same condition. This companion function runs over the
/// AST (where `ReactionDecl.guard_ref` records the id each reaction references
/// and `DdlDoc.guards` lists the templates) and surfaces the two lints that
/// require that information. `schema` is accepted for symmetry with
/// `check_schema` and to leave room for cross-level checks; it is not needed by
/// the lints implemented today.
///
/// Two checks run today:
///
/// 1. **Unused guard template** — a top-level `guard <id> { ... }` declaration
///    that no reaction references via `when <id>`. With no reference its
///    expression is never evaluated, so it is dead code.
/// 2. **Duplicate guard condition** — two or more top-level guard declarations
///    with identical expression text. They likely express the same condition
///    twice and could be merged into one template shared by the referencing
///    reactions.
pub fn check_ddl(doc: &DdlDoc, _schema: &TopologySchema) -> Vec<CheckWarning> {
    let mut warnings = Vec::new();

    // Collect the set of guard ids actually referenced by reactions. A
    // reaction's `guard_ref` is `Some(id)` precisely when it was written as a
    // bare `when <id>`; literal guards and unguarded reactions contribute
    // nothing. A template id not present in this set is unused.
    let referenced: HashSet<&str> = doc
        .reactions
        .iter()
        .filter_map(|r: &ReactionDecl| r.guard_ref.as_deref())
        .collect();

    for g in &doc.guards {
        if !referenced.contains(g.id.as_str()) {
            warnings.push(CheckWarning {
                kind: WarningKind::UnusedGuardTemplate,
                message: format!("guard '{}' is declared but never referenced by a reaction", g.id),
                line: None,
                col: None,
            });
        }
    }

    // Group templates by their expression text. Any bucket with more than one
    // id is a duplicate condition (regardless of whether the templates are
    // referenced — declaring two identical unused templates is still a smell).
    let mut by_expr: HashMap<&str, Vec<&str>> = HashMap::new();
    for g in &doc.guards {
        by_expr.entry(g.expr.as_str()).or_default().push(g.id.as_str());
    }
    for (expr, ids) in &by_expr {
        if ids.len() > 1 {
            warnings.push(CheckWarning {
                kind: WarningKind::DuplicateGuardCondition,
                message: format!(
                    "guards [{}] share the same condition `{}`",
                    ids.join(", "),
                    expr
                ),
                line: None,
                col: None,
            });
        }
    }

    warnings
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{ActionBinding, SignalDef, TransitionDef};

    fn sig(id: &str, states: &[&str], initial: &str) -> SignalDef {
        SignalDef {
            id: id.to_string(),
            initial_state: initial.to_string(),
            states: states.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn tr(signal_id: &str, from: &str, to: &str) -> TransitionDef {
        TransitionDef {
            signal_id: signal_id.to_string(),
            from: from.to_string(),
            event: "go".to_string(),
            to: to.to_string(),
            actions: ActionBinding::default(),
            guard: None,
        }
    }

    #[test]
    fn self_loop_detected() {
        // order: submitted -> submitted is a literal self-loop.
        let schema = TopologySchema {
            version: "0.1".to_string(),
            signals: vec![sig("order", &["submitted", "approved"], "submitted")],
            transitions: vec![tr("order", "submitted", "submitted")],
            reactions: Vec::new(),
            components: None,
            instances: Vec::new(),
            includes: Vec::new(),
        };

        let warnings = check_schema(&schema);
        assert!(
            warnings.iter().any(|w| w.kind == WarningKind::SelfLoop),
            "expected a SelfLoop warning, got: {:?}",
            warnings
        );
        assert!(
            warnings
                .iter()
                .any(|w| w.message.contains("submitted -> submitted")),
            "expected the self-loop to be described, got: {:?}",
            warnings
        );
    }

    #[test]
    fn unreachable_state_detected() {
        // task: idle -> running, but `done` is never targeted and is not the
        // initial state — it is dead.
        let schema = TopologySchema {
            version: "0.1".to_string(),
            signals: vec![sig("task", &["idle", "running", "done"], "idle")],
            transitions: vec![tr("task", "idle", "running")],
            reactions: Vec::new(),
            components: None,
            instances: Vec::new(),
            includes: Vec::new(),
        };

        let warnings = check_schema(&schema);
        let unreachable: Vec<_> = warnings
            .iter()
            .filter(|w| w.kind == WarningKind::UnreachableState)
            .collect();
        assert_eq!(
            unreachable.len(),
            1,
            "expected exactly one unreachable warning, got: {:?}",
            warnings
        );
        assert_eq!(unreachable[0].message, "task: done");
    }

    #[test]
    fn no_warnings_on_clean_linear_schema() {
        // a -> b -> c, initial `a`: every state reachable, no self-loops.
        let schema = TopologySchema {
            version: "0.1".to_string(),
            signals: vec![sig("task", &["a", "b", "c"], "a")],
            transitions: vec![tr("task", "a", "b"), tr("task", "b", "c")],
            reactions: Vec::new(),
            components: None,
            instances: Vec::new(),
            includes: Vec::new(),
        };

        let warnings = check_schema(&schema);
        assert!(
            warnings.is_empty(),
            "clean linear schema should produce no warnings, got: {:?}",
            warnings
        );
    }

    // --- check_ddl (M39) ---

    fn guard(id: &str, expr: &str) -> crate::ddl::GuardDecl {
        crate::ddl::GuardDecl {
            id: id.to_string(),
            expr: expr.to_string(),
        }
    }

    fn reaction_with_ref(from_signal: &str, id: Option<&str>) -> ReactionDecl {
        ReactionDecl {
            from_signal: from_signal.to_string(),
            from_state: "paid".to_string(),
            to_signal: "inv".to_string(),
            event: "reserve".to_string(),
            guard: Some("payload.amount <= 100".to_string()),
            guard_ref: id.map(String::from),
            payload: None,
            join_group: None,
            requires: vec![],
        }
    }

    fn empty_schema() -> TopologySchema {
        TopologySchema {
            version: "0.1".to_string(),
            signals: vec![sig("s", &["a", "b"], "a")],
            transitions: vec![],
            reactions: Vec::new(),
            components: None,
            instances: Vec::new(),
            includes: Vec::new(),
        }
    }

    #[test]
    fn unused_guard_template_detected() {
        // `lonely` is declared but never referenced; `used` is referenced.
        let doc = DdlDoc {
            signals: vec![],
            reactions: vec![reaction_with_ref("order", Some("used"))],
            guards: vec![guard("used", "payload.amount <= 100"), guard("lonely", "payload.x == 1")],
        };

        let warnings = check_ddl(&doc, &empty_schema());
        let unused: Vec<_> = warnings
            .iter()
            .filter(|w| w.kind == WarningKind::UnusedGuardTemplate)
            .collect();
        assert_eq!(
            unused.len(),
            1,
            "expected exactly one unused-guard warning, got: {:?}",
            warnings
        );
        assert!(unused[0].message.contains("lonely"));
    }

    #[test]
    fn duplicate_guard_condition_detected() {
        // `g1` and `g2` declare identical expressions.
        let doc = DdlDoc {
            signals: vec![],
            reactions: vec![reaction_with_ref("order", Some("g1"))],
            guards: vec![guard("g1", "payload.amount <= 100"), guard("g2", "payload.amount <= 100")],
        };

        let warnings = check_ddl(&doc, &empty_schema());
        let dup: Vec<_> = warnings
            .iter()
            .filter(|w| w.kind == WarningKind::DuplicateGuardCondition)
            .collect();
        assert_eq!(
            dup.len(),
            1,
            "expected exactly one duplicate-guard warning, got: {:?}",
            warnings
        );
        assert!(dup[0].message.contains("g1"));
        assert!(dup[0].message.contains("g2"));
        // An undeclared duplicate is also unused, but it must still be flagged
        // as a duplicate — both lints are independent.
        assert!(warnings
            .iter()
            .any(|w| w.kind == WarningKind::UnusedGuardTemplate && w.message.contains("g2")));
    }

    #[test]
    fn no_guard_warnings_when_all_used_and_unique() {
        let doc = DdlDoc {
            signals: vec![],
            reactions: vec![
                reaction_with_ref("order", Some("a")),
                reaction_with_ref("order", Some("b")),
            ],
            guards: vec![guard("a", "payload.amount <= 100"), guard("b", "payload.amount > 0")],
        };

        let warnings = check_ddl(&doc, &empty_schema());
        assert!(
            warnings.is_empty(),
            "all-used unique guards should produce no warnings, got: {:?}",
            warnings
        );
    }
}
