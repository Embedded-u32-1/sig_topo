// M28: Domain Description Language compiler. Compiles `.ddl` source into the
// engine's `TopologySchema` (see `src/ddl/`); the engine layer is untouched.
pub mod ddl;

pub mod compose;
pub mod engine;
pub mod error;
pub mod export;
pub mod guard;
pub mod persist;
/// Shared scaffolding for the `sts` / `stt` / `stp` binaries: turns a topology
/// file + a shared fail-set into a runnable engine. Not part of the stable
/// library surface -- treat it as build support for the binaries; binary usage
/// and docs take precedence over these APIs.
pub mod run;
pub mod schema;
pub mod trace;

pub use compose::{expand, from_path, load_topology};
pub use engine::{ActionContext, TopologyEngine, TransitionResult};
pub use error::EngineError;
pub use export::to_dot;
pub use guard::{eval_guard, Value};
pub use persist::StateSnapshot;
pub use trace::{TraceEvent, TraceLog};
