use super::token::Token;

pub fn tokenize(input: &str) -> Result<Vec<Token>, String> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];

        if c.is_whitespace() {
            i += 1;
            continue;
        }

        if c == '\'' {
            let mut s = String::new();
            i += 1;
            while i < chars.len() && chars[i] != '\'' {
                s.push(chars[i]);
                i += 1;
            }
            if i >= chars.len() {
                return Err("Unterminated string literal".to_string());
            }
            i += 1;
            tokens.push(Token::String(s));
            continue;
        }

        if c.is_ascii_digit() || (c == '.' && i + 1 < chars.len() && chars[i + 1].is_ascii_digit()) {
            let mut s = String::new();
            let mut is_float = false;
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                if chars[i] == '.' {
                    if is_float {
                        return Err("Invalid number with multiple decimal points".to_string());
                    }
                    is_float = true;
                }
                s.push(chars[i]);
                i += 1;
            }
            if is_float {
                let val = s.parse::<f64>().map_err(|e| format!("Invalid float: {}", e))?;
                tokens.push(Token::Float(val));
            } else {
                let val = s.parse::<i64>().map_err(|e| format!("Invalid integer: {}", e))?;
                tokens.push(Token::Integer(val));
            }
            continue;
        }

        if c.is_alphabetic() || c == '_' {
            let mut s = String::new();
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                s.push(chars[i]);
                i += 1;
            }
            let token = match s.as_str() {
                "and" => Token::And,
                "or" => Token::Or,
                "not" => Token::Not,
                "true" => Token::Bool(true),
                "false" => Token::Bool(false),
                _ => Token::Identifier(s),
            };
            tokens.push(token);
            continue;
        }

        match c {
            '(' => tokens.push(Token::LParen),
            ')' => tokens.push(Token::RParen),
            '.' => tokens.push(Token::Dot),
            '+' => tokens.push(Token::Plus),
            '-' => tokens.push(Token::Minus),
            '*' => tokens.push(Token::Mul),
            '/' => tokens.push(Token::Div),
            '<' => {
                if i + 1 < chars.len() && chars[i + 1] == '=' {
                    tokens.push(Token::Lte);
                    i += 1;
                } else {
                    tokens.push(Token::Lt);
                }
            }
            '>' => {
                if i + 1 < chars.len() && chars[i + 1] == '=' {
                    tokens.push(Token::Gte);
                    i += 1;
                } else {
                    tokens.push(Token::Gt);
                }
            }
            '!' => {
                if i + 1 < chars.len() && chars[i + 1] == '=' {
                    tokens.push(Token::Neq);
                    i += 1;
                } else {
                    return Err(format!("Unexpected character: {}", c));
                }
            }
            '=' if i + 1 < chars.len() && chars[i + 1] == '=' => {
                tokens.push(Token::Eq);
                i += 1;
            }
            '=' => return Err(format!("Unexpected character: {}", c)),
            _ => return Err(format!("Unexpected character: {}", c)),
        }
        i += 1;
    }

    tokens.push(Token::Eof);
    Ok(tokens)
}
