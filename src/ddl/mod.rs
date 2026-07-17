// M28: Domain Description Language (DDL) compiler.
//
// A small, domain-facing language for describing signal topologies. Users
// write `.ddl` files with business semantics (`signal`, `states`, `on`, `when`,
// `reaction`); the compiler lowers them to the engine's `TopologySchema` JSON
// model. The engine itself is untouched — DDL is purely a front-end.
//
// Pipeline: source -> lexer (tokens) -> parser (AST `DdlDoc`) -> codegen
// (`TopologySchema`).

/// Lower a `DdlDoc` AST into a `TopologySchema`.
pub mod codegen;
/// Lex DDL source into `TokenKind`s.
pub mod lexer;
/// Recursive-descent parse tokens into a `DdlDoc` AST.
pub mod parser;

// Re-export the AST types so both `compile_full` (in this module) and the
// AST-level checker (`crate::check`) can refer to them without reaching into
// `parser` directly.
pub use parser::DdlDoc;
pub use parser::GuardDecl;
pub use parser::ReactionDecl;

use crate::error::EngineError;
use crate::schema::TopologySchema;

use lexer::tokenize;
use parser::parse;

/// Token kinds produced by `lexer.rs`. The set is a superset of the DDL
/// structure keywords/symbols and the guard expression operators, so a single
/// lex pass covers both `signal`/`reaction` blocks and `when <guard>` clauses.
#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // Literals.
    Identifier(String),
    String(String),
    Integer(i64),
    Float(f64),
    Bool(bool),

    // DDL structure keywords.
    Signal,
    States,
    Initial,
    On,
    From,
    To,
    When,
    Reaction,
    Enters,
    With,
    OnExit,
    OnTransition,
    OnEnter,
    Guard,

    // Guard expression keywords.
    And,
    Or,
    Not,

    // Structure symbols.
    LBracket,
    RBracket,
    LBrace,
    RBrace,
    LParen,
    RParen,
    Arrow,
    Colon,
    Comma,
    Dot,

    // Guard expression operators.
    Plus,
    Minus,
    Mul,
    Div,
    Eq,
    Neq,
    Lt,
    Lte,
    Gt,
    Gte,

    Eof,
}

/// Compile DDL source into a `TopologySchema`.
///
/// This is the single public entry point used by the `stc` binary and the
/// integration tests. Any failure — lexical, syntactic, or semantic — is
/// reported as an `EngineError` with a line/column location where useful.
pub fn compile(src: &str) -> Result<TopologySchema, EngineError> {
    let tokens = tokenize(src).map_err(EngineError::ParseError)?;
    let doc = parse(&tokens, src).map_err(EngineError::ParseError)?;
    let schema = codegen::emit(doc)?;
    Ok(schema)
}

/// Compile DDL source into both a `TopologySchema` and the parsed `DdlDoc` AST.
///
/// Like `compile`, but keeps the AST around so AST-level checks (`stc --int-)
/// can inspect the top-level guard declarations and per-reaction guard
/// references that are lost once codegen lowers the document to a schema. The
/// schema is produced from a clone of the document; the returned AST is the
/// original. Any failure is reported as an `EngineError`.
pub fn compile_full(src: &str) -> Result<(TopologySchema, DdlDoc), EngineError> {
    let tokens = tokenize(src).map_err(EngineError::ParseError)?;
    let doc = parse(&tokens, src).map_err(EngineError::ParseError)?;
    let schema = codegen::emit(doc.clone())?;
    Ok((schema, doc))
}
