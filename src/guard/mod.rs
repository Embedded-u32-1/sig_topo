//! Guard expression evaluation against an [`ActionContext`](crate::engine::ActionContext).
//!
//! A guard is a boolean expression over the event payload (`payload.field`,
//! literals, comparisons, arithmetic and logic). Parse and evaluate one with
//! [`eval_guard`]; the AST, evaluator and lexer are also exposed for downstream
//! reuse (notably the [`ddl`](crate::ddl) compiler lexes guard expressions with
//! its own token set).

/// Guard expression AST.
pub mod ast;
/// Evaluator: an AST + an `ActionContext` → a [`Value`].
pub mod eval;
/// Lex guard source into tokens.
pub mod lexer;
/// Recursive-descent parse guard tokens into an AST.
pub mod parser;
/// Token type produced by the guard lexer.
pub mod token;

/// Evaluate a guard expression string against `ctx`, returning `true` to allow
/// the transition.
pub use eval::eval_guard;
/// Evaluate a guard AST against `ctx`, producing a [`Value`].
pub use eval::eval;
/// The scalar value type produced by guard evaluation (`Integer`, `Float`,
/// `String`, `Bool`, `Null`).
pub use eval::Value;
