// M28: DDL lexer.
//
// Produces a flat token stream for the DDL parser. The token set is a
// superset of the DDL structure keywords/symbols and the guard expression
// operators, so a single pass covers both the `signal`/`reaction` blocks and
// the `when <guard>` expressions. Each token carries its byte span (start/len)
// plus line/column so the parser can (a) locate errors and (b) slice the
// original source of a guard expression to pass through verbatim.

use super::TokenKind;

/// A single lexical token, carrying its kind plus the location spans the parser
/// needs to report errors (`line`/`col`) and slice guard expressions verbatim
/// from the source (`start`/`len`, in bytes).
#[derive(Debug, Clone)]
pub struct Token {
    /// The token's kind (identifier, keyword, symbol, literal).
    pub kind: TokenKind,
    /// The 1-based source line where the token begins.
    pub line: usize,
    /// The 1-based source column where the token begins.
    pub col: usize,
    /// The byte offset of the token's first character in the source.
    pub start: usize,
    /// The token's length in bytes.
    pub len: usize,
}

/// Lex `src` into tokens. Returns `Err(msg)` with a line/column location on
/// the first lexical error (unexpected char, unterminated string).
pub fn tokenize(src: &str) -> Result<Vec<Token>, String> {
    let chars: Vec<char> = src.chars().collect();
    // Map each char index to its byte offset in `src`, so a Token's `start`
    // and `len` are byte-based and can safely slice `&str` (multi-byte chars
    // make char indices differ from byte offsets — `parse_guard` relies on
    // this being correct).
    let byte_pos: Vec<usize> = {
        let mut v = Vec::with_capacity(chars.len() + 1);
        let mut b = 0;
        for c in &chars {
            v.push(b);
            b += c.len_utf8();
        }
        v.push(b);
        v
    };
    let mut i = 0;
    let mut line: usize = 1;
    let mut col: usize = 1;
    let mut tokens = Vec::new();

    while i < chars.len() {
        // Skip `//` line comments.
        if chars[i] == '/' && i + 1 < chars.len() && chars[i + 1] == '/' {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }

        let c = chars[i];

        // Whitespace (not newline): just advance.
        if c == ' ' || c == '\t' || c == '\r' {
            i += 1;
            col += 1;
            continue;
        }

        // Newline: advance line counter.
        if c == '\n' {
            i += 1;
            line += 1;
            col = 1;
            continue;
        }

        let start = byte_pos[i]; // byte offset, see `byte_pos` above
        let start_line = line;
        let start_col = col;

        // String literal: single-quoted, no escape handling (matches guard).
        // Also accepts double-quoted strings so a reaction's `with { ... }`
        // static payload (JSON) can contain `"..."` string values without
        // tripping the lexer. Both quote styles desugar to `TokenKind::String`.
        if c == '\'' || c == '"' {
            let quote = c;
            i += 1;
            col += 1;
            let mut s = String::new();
            while i < chars.len() && chars[i] != quote {
                if chars[i] == '\n' {
                    return Err(format!(
                        "line {} col {}: unterminated string literal",
                        start_line, start_col
                    ));
                }
                s.push(chars[i]);
                i += 1;
                col += 1;
            }
            if i >= chars.len() {
                return Err(format!(
                    "line {} col {}: unterminated string literal",
                    start_line, start_col
                ));
            }
            i += 1;
            col += 1; // closing quote
            tokens.push(Token {
                kind: TokenKind::String(s),
                line: start_line,
                col: start_col,
                start,
                len: byte_pos[i] - start,
            });
            continue;
        }

        // Number literal (integer or float).
        if c.is_ascii_digit() {
            let mut s = String::new();
            let mut is_float = false;
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                if chars[i] == '.' {
                    if is_float {
                        return Err(format!(
                            "line {} col {}: number with multiple decimal points",
                            start_line, start_col
                        ));
                    }
                    is_float = true;
                }
                s.push(chars[i]);
                i += 1;
                col += 1;
            }
            let kind = if is_float {
                match s.parse::<f64>() {
                    Ok(v) => TokenKind::Float(v),
                    Err(_) => {
                        return Err(format!(
                            "line {} col {}: invalid float literal",
                            start_line, start_col
                        ))
                    }
                }
            } else {
                match s.parse::<i64>() {
                    Ok(v) => TokenKind::Integer(v),
                    Err(_) => {
                        return Err(format!(
                            "line {} col {}: invalid integer literal",
                            start_line, start_col
                        ))
                    }
                }
            };
            tokens.push(Token {
                kind,
                line: start_line,
                col: start_col,
                start,
                len: byte_pos[i] - start,
            });
            continue;
        }

        // Identifier or keyword.
        if c.is_alphabetic() || c == '_' {
            let mut s = String::new();
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                s.push(chars[i]);
                i += 1;
                col += 1;
            }
            let kind = match s.as_str() {
                "signal" => TokenKind::Signal,
                "states" => TokenKind::States,
                "initial" => TokenKind::Initial,
                "on" => TokenKind::On,
                "from" => TokenKind::From,
                "to" => TokenKind::To,
                "when" => TokenKind::When,
                "reaction" => TokenKind::Reaction,
                "enters" => TokenKind::Enters,
                "with" => TokenKind::With,
                "on_exit" => TokenKind::OnExit,
                "on_transition" => TokenKind::OnTransition,
                "on_enter" => TokenKind::OnEnter,
                "guard" => TokenKind::Guard,
                "and" => TokenKind::And,
                "or" => TokenKind::Or,
                "not" => TokenKind::Not,
                "true" => TokenKind::Bool(true),
                "false" => TokenKind::Bool(false),
                _ => TokenKind::Identifier(s),
            };
            tokens.push(Token {
                kind,
                line: start_line,
                col: start_col,
                start,
                len: byte_pos[i] - start,
            });
            continue;
        }

        // Symbols and operators.
        let kind = match c {
            '[' => TokenKind::LBracket,
            ']' => TokenKind::RBracket,
            '{' => TokenKind::LBrace,
            '}' => TokenKind::RBrace,
            '(' => TokenKind::LParen,
            ')' => TokenKind::RParen,
            ':' => TokenKind::Colon,
            ',' => TokenKind::Comma,
            '.' => TokenKind::Dot,
            '+' => TokenKind::Plus,
            '*' => TokenKind::Mul,
            '/' => TokenKind::Div,
            '-' => {
                // `->` is the arrow; bare `-` is the minus operator (guard).
                if i + 1 < chars.len() && chars[i + 1] == '>' {
                    i += 1;
                    col += 1;
                    TokenKind::Arrow
                } else {
                    TokenKind::Minus
                }
            }
            '>' => {
                if i + 1 < chars.len() && chars[i + 1] == '=' {
                    i += 1;
                    col += 1;
                    TokenKind::Gte
                } else {
                    TokenKind::Gt
                }
            }
            '<' => {
                if i + 1 < chars.len() && chars[i + 1] == '=' {
                    i += 1;
                    col += 1;
                    TokenKind::Lte
                } else {
                    TokenKind::Lt
                }
            }
            '!' => {
                if i + 1 < chars.len() && chars[i + 1] == '=' {
                    i += 1;
                    col += 1;
                    TokenKind::Neq
                } else {
                    return Err(format!(
                        "line {} col {}: unexpected character '{}'",
                        start_line, start_col, c
                    ));
                }
            }
            '=' => {
                if i + 1 < chars.len() && chars[i + 1] == '=' {
                    i += 1;
                    col += 1;
                    TokenKind::Eq
                } else {
                    return Err(format!(
                        "line {} col {}: unexpected character '=' (did you mean '=='?)",
                        start_line, start_col
                    ));
                }
            }
            _ => {
                return Err(format!(
                    "line {} col {}: unexpected character '{}'",
                    start_line, start_col, c
                ));
            }
        };
        i += 1;
        col += 1;
        tokens.push(Token {
            kind,
            line: start_line,
            col: start_col,
            start,
            len: byte_pos[i] - start,
        });
    }

    tokens.push(Token {
        kind: TokenKind::Eof,
        line,
        col,
        start: i,
        len: 0,
    });
    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keywords_are_recognized() {
        let toks = tokenize("signal states initial on from to when reaction enters on_exit on_transition on_enter guard and or not true false").unwrap();
        let kinds: Vec<_> = toks.iter().map(|t| &t.kind).collect();
        assert!(matches!(kinds[0], TokenKind::Signal));
        assert!(matches!(kinds[1], TokenKind::States));
        assert!(matches!(kinds[2], TokenKind::Initial));
        assert!(matches!(kinds[3], TokenKind::On));
        assert!(matches!(kinds[4], TokenKind::From));
        assert!(matches!(kinds[5], TokenKind::To));
        assert!(matches!(kinds[6], TokenKind::When));
        assert!(matches!(kinds[7], TokenKind::Reaction));
        assert!(matches!(kinds[8], TokenKind::Enters));
        assert!(matches!(kinds[9], TokenKind::OnExit));
        assert!(matches!(kinds[10], TokenKind::OnTransition));
        assert!(matches!(kinds[11], TokenKind::OnEnter));
        assert!(matches!(kinds[12], TokenKind::Guard));
        assert!(matches!(kinds[13], TokenKind::And));
        assert!(matches!(kinds[14], TokenKind::Or));
        assert!(matches!(kinds[15], TokenKind::Not));
        assert!(matches!(kinds[16], TokenKind::Bool(true)));
        assert!(matches!(kinds[17], TokenKind::Bool(false)));
        assert!(matches!(kinds[18], TokenKind::Eof));
    }

    #[test]
    fn symbols_and_operators() {
        let toks = tokenize("[ ] { } ( ) -> : , . + - * / == != < <= > >=").unwrap();
        let kinds: Vec<_> = toks.iter().map(|t| &t.kind).collect();
        assert!(matches!(kinds[0], TokenKind::LBracket));
        assert!(matches!(kinds[1], TokenKind::RBracket));
        assert!(matches!(kinds[2], TokenKind::LBrace));
        assert!(matches!(kinds[3], TokenKind::RBrace));
        assert!(matches!(kinds[4], TokenKind::LParen));
        assert!(matches!(kinds[5], TokenKind::RParen));
        assert!(matches!(kinds[6], TokenKind::Arrow));
        assert!(matches!(kinds[7], TokenKind::Colon));
        assert!(matches!(kinds[8], TokenKind::Comma));
        assert!(matches!(kinds[9], TokenKind::Dot));
        assert!(matches!(kinds[10], TokenKind::Plus));
        assert!(matches!(kinds[11], TokenKind::Minus));
        assert!(matches!(kinds[12], TokenKind::Mul));
        assert!(matches!(kinds[13], TokenKind::Div));
        assert!(matches!(kinds[14], TokenKind::Eq));
        assert!(matches!(kinds[15], TokenKind::Neq));
        assert!(matches!(kinds[16], TokenKind::Lt));
        assert!(matches!(kinds[17], TokenKind::Lte));
        assert!(matches!(kinds[18], TokenKind::Gt));
        assert!(matches!(kinds[19], TokenKind::Gte));
    }

    #[test]
    fn strings_and_numbers() {
        let toks = tokenize("'USD' 42 3.14").unwrap();
        assert!(matches!(&toks[0].kind, TokenKind::String(s) if s == "USD"));
        assert!(matches!(toks[1].kind, TokenKind::Integer(42)));
        assert!(matches!(&toks[2].kind, TokenKind::Float(f) if *f == "3.14".parse::<f64>().unwrap()));
    }

    #[test]
    fn comments_are_skipped() {
        let toks = tokenize("signal // this is a comment\norder").unwrap();
        assert!(matches!(toks[0].kind, TokenKind::Signal));
        assert!(matches!(&toks[1].kind, TokenKind::Identifier(s) if s == "order"));
        assert!(matches!(toks[2].kind, TokenKind::Eof));
    }

    #[test]
    fn line_col_advance_across_newlines() {
        let toks = tokenize("signal\n  order").unwrap();
        assert_eq!(toks[0].line, 1);
        assert_eq!(toks[0].col, 1);
        // `order` is on line 2, indented by 2 spaces.
        assert_eq!(toks[1].line, 2);
        assert_eq!(toks[1].col, 3);
    }

    #[test]
    fn unterminated_string_errors_with_location() {
        let err = tokenize("signal 'oops").unwrap_err();
        assert!(err.contains("line 1 col 8"), "got: {}", err);
    }

    #[test]
    fn unexpected_character_reports_location() {
        let err = tokenize("signal $").unwrap_err();
        assert!(err.contains("line 1 col 8"), "got: {}", err);
    }
}
