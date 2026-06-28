#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Integer(i64),
    Float(f64),
    String(String),
    Bool(bool),
    Identifier(String),

    And,
    Or,
    Not,

    Eq,
    Neq,
    Lt,
    Lte,
    Gt,
    Gte,

    Plus,
    Minus,
    Mul,
    Div,

    LParen,
    RParen,
    Dot,

    Eof,
}
