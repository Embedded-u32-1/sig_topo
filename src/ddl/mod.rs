// M28: Domain Description Language (DDL) compiler.
//
// A small, domain-facing language for describing signal topologies. Users
// write `.ddl` files with business semantics (`signal`, `states`, `on`, `when`,
// `reaction`); the compiler lowers them to the engine's `TopologySchema` JSON
// model. The engine itself is untouched — DDL is purely a front-end.
//
// Pipeline: source -> lexer (tokens) -> parser (AST `DdlDoc`) -> codegen
// (`TopologySchema`).

pub mod codegen;
pub mod lexer;
pub mod parser;

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
    OnExit,
    OnTransition,
    OnEnter,

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
