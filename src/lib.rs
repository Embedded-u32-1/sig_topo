//! A file-driven Rust state-machine engine.
//!
//! Describe a system as a topology of **signals**, each holding a current state
//! among a fixed set of **states**. **Transitions** move a signal from one state
//! to another when an **event** arrives, optionally guarded by a **guard**
//! expression over the event payload. Each transition binds three phases of
//! lifecycle **actions** (`on_exit` → `on_transition` → `on_engine`); a failure
//! in any phase rolls the signal back to its source state. When a transition
//! commits, matching **reactions** fire derived events on other signals,
//! bounded by a cascade depth limit.
//!
//! The topology can be authored directly as JSON (`schema`) or written in the
//! domain description language (`ddl`) and compiled to the same model. A
//! fully-expanded topology is built into a `TopologyEngine` and driven through a
//! small set of entry points:
//!
//! ```
//! # use signal_topology::{TopologyEngine, TransitionResult};
//! # let json = include_str!("../tests/topology.json");
//! let mut engine = TopologyEngine::from_json(json).unwrap();
//! // Register every action referenced by the topology so transitions run.
//! for id in ["log_idle_leave", "init_task_resource", "start_task_execution"] {
//!     engine.register_action(id, |_| Ok(()));
//! }
//! let result: TransitionResult = engine.send_event("task_status", "start", None).unwrap();
//! assert_eq!(result.to, "running");
//! assert_eq!(engine.get_state("task_status").unwrap(), "running");
//! ```
//!
//! Beyond runtime, the engine renders its structure to DOT (`export`), persists
//! state to JSON (`persist`), and records an ordered trace log of every event
//! and action lifecycle step (`trace`).

/// M28: Domain Description Language (DDL) compiler. Compiles `.ddl` source into
/// the engine's `TopologySchema` (see `src/ddl/`); the engine layer is
/// untouched.
pub mod ddl;

/// Parameterized component expansion (`expand`) and cross-file import with
/// cycle detection (`load_topology`, `from_path`).
pub mod compose;

/// The `TopologyEngine` runtime plus its public types (`ActionContext`,
/// `TransitionResult`).
pub mod engine;

/// The `EngineError` enum covering every failure path in the crate.
pub mod error;

/// DOT export (`to_dot`, `to_dot_with_state`).
pub mod export;

/// Guard expression evaluation (`eval_guard`, `Value`).
pub mod guard;

/// State snapshot and JSON persistence (`StateSnapshot`, `save_state`,
/// `load_state`).
pub mod persist;

/// Shared scaffolding for the `sts` / `stt` / `stp` binaries: turns a topology
/// file + a shared fail-set into a runnable engine. Not part of the stable
/// library surface -- treat it as build support for the binaries; binary usage
/// and docs take precedence over these APIs.
pub mod run;

/// Topology JSON model (`TopologySchema`, `SignalDef`, `TransitionDef`,
/// `ReactionDef`, `ActionBinding`).
pub mod schema;

/// Ordered trace log of runtime events (`TraceEvent`, `TraceLog`).
pub mod trace;

/// Expand parameterized components and instances into a flat `TopologySchema`.
pub use compose::expand;
/// Convenience: `load_topology` + build a ready-to-use `TopologyEngine`.
pub use compose::from_path;
/// Load a topology file, recursively merging everything it `includes`, then
/// expand instances into a flat `TopologySchema`.
pub use compose::load_topology;
/// The context passed to every action callback: the signal, source/target
/// states, event name and optional payload.
pub use engine::ActionContext;
/// Validate a topology and drive signals through transitions with action
/// callbacks and cascade depth limiting.
pub use engine::TopologyEngine;
/// The outcome of a successful transition: the signal, source/target states and
/// the action ids that ran in lifecycle order.
pub use engine::TransitionResult;
/// The error type for every failure path in the crate.
pub use error::EngineError;
/// Render a topology's structural skeleton to Graphviz DOT (initial states
/// highlighted lightblue).
pub use export::to_dot;
/// Render a topology to DOT with each signal's *current* state highlighted
/// lightgreen.
pub use export::to_dot_with_state;
/// Evaluate a guard expression against an `ActionContext`, returning `true` to
/// allow the transition.
pub use guard::eval_guard;
/// The value type produced by guard expression evaluation (`Integer`, `Float`,
/// `String`, `Bool`, `Null`).
pub use guard::Value;
/// A point-in-time snapshot of every signal's current state, serde-serializable.
pub use persist::StateSnapshot;
/// One entry in the ordered trace log.
pub use trace::TraceEvent;
/// An append-only log of `TraceEvent`s with query helpers (`for_signal`,
/// `since`).
pub use trace::TraceLog;
