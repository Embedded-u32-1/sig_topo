//! Graphviz DOT rendering of a topology.
//!
//! The single backend is the `dot` module, which offers three views: a
//! structural skeleton ([`to_dot`]), a runtime-highlighted view
//! ([`to_dot_with_state`]), and an extended view ([`to_dot_extended`]) that
//! additionally draws cross-signal reaction edges colored by their
//! guard-evaluation result. All three are re-exported here at the `export`
//! level.

/// Render a topology to Graphviz DOT.
pub mod dot;

/// M49: render a DOT source to SVG through the system Graphviz `dot` (the
/// shared SVG-rendering helper the visualization binaries funnel through, so
/// the "pipe DOT to `dot -Tsvg`, write the SVG" step does not drift between
/// them). Re-exported from [`render::render_dot_to_svg`].
pub mod render;
/// Render a topology to SVG through the system Graphviz `dot`. Re-exported from
/// [`render::render_dot_to_svg`].
pub use render::render_dot_to_svg;
/// Outcome of rendering a DOT source to SVG; re-exported from
/// [`render::SvgOutcome`].
pub use render::SvgOutcome;
/// Render a topology's structural skeleton (initial states highlighted
/// lightblue). Re-exported from [`dot::to_dot`].
pub use dot::to_dot;
/// Render a topology with each signal's current state highlighted lightgreen.
/// Re-exported from [`dot::to_dot_with_state`].
pub use dot::to_dot_with_state;
/// Render a topology to DOT with live-state highlighting *plus* cross-signal
/// reaction edges colored by their guard-evaluation result (fired = solid
/// green, blocked = dashed gray, guard error = dashed red, never evaluated =
/// dashed black). Re-exported from [`dot::to_dot_extended`].
pub use dot::to_dot_extended;
