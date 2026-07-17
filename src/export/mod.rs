//! Graphviz DOT rendering of a topology.
//!
//! The single backend is the `dot` module, which offers both a structural
//! skeleton ([`to_dot`]) and a runtime-highlighted view ([`to_dot_with_state`]);
//! both are re-exported here at the `export` level.

/// Render a topology to Graphviz DOT.
pub mod dot;

/// Render a topology's structural skeleton (initial states highlighted
/// lightblue). Re-exported from [`dot::to_dot`].
pub use dot::to_dot;
/// Render a topology with each signal's current state highlighted lightgreen.
/// Re-exported from [`dot::to_dot_with_state`].
pub use dot::to_dot_with_state;
