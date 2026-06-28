use super::ast::{BinaryOp, Expr, Literal, UnaryOp};
use super::token::Token;

pub fn parse(tokens: &[Token]) -> Result<Expr, String> {
    let mut parser = Parser::new(tokens);
    let expr = parser.parse_or()?;
    if !parser.is_at(Token::Eof) {
        return Err(format!("Unexpected token after expression: {:?}", parser.peek()));
    }
    Ok(expr)
}

struct Parser<'a> {
    tokens: &'a [Token],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(tokens: &'a [Token]) -> Self {
        Parser { tokens, pos: 0 }
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.pos]
    }

    fn advance(&mut self) -> &Token {
        let tok = &self.tokens[self.pos];
        if self.pos + 1 < self.tokens.len() {
            self.pos += 1;
        }
        tok
    }

    fn is_at(&self, token: Token) -> bool {
        self.peek() == &token
    }

    fn expect(&mut self, token: Token) -> Result<(), String> {
        if self.is_at(token.clone()) {
            self.advance();
            Ok(())
        } else {
            Err(format!("Expected {:?}, found {:?}", token, self.peek()))
        }
    }

    fn parse_or(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_and()?;
        while self.is_at(Token::Or) {
            self.advance();
            let right = self.parse_and()?;
            left = Expr::Binary {
                left: Box::new(left),
                op: BinaryOp::Or,
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_comparison()?;
        while self.is_at(Token::And) {
            self.advance();
            let right = self.parse_comparison()?;
            left = Expr::Binary {
                left: Box::new(left),
                op: BinaryOp::And,
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_comparison(&mut self) -> Result<Expr, String> {
        let left = self.parse_addition()?;
        let op = match self.peek() {
            Token::Eq => BinaryOp::Eq,
            Token::Neq => BinaryOp::Neq,
            Token::Lt => BinaryOp::Lt,
            Token::Lte => BinaryOp::Lte,
            Token::Gt => BinaryOp::Gt,
            Token::Gte => BinaryOp::Gte,
            _ => return Ok(left),
        };
        self.advance();
        let right = self.parse_addition()?;
        Ok(Expr::Binary {
            left: Box::new(left),
            op,
            right: Box::new(right),
        })
    }

    fn parse_addition(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_multiplication()?;
        loop {
            let op = match self.peek() {
                Token::Plus => BinaryOp::Add,
                Token::Minus => BinaryOp::Sub,
                _ => break,
            };
            self.advance();
            let right = self.parse_multiplication()?;
            left = Expr::Binary {
                left: Box::new(left),
                op,
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_multiplication(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_unary()?;
        loop {
            let op = match self.peek() {
                Token::Mul => BinaryOp::Mul,
                Token::Div => BinaryOp::Div,
                _ => break,
            };
            self.advance();
            let right = self.parse_unary()?;
            left = Expr::Binary {
                left: Box::new(left),
                op,
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<Expr, String> {
        match self.peek() {
            Token::Not => {
                self.advance();
                Ok(Expr::Unary {
                    op: UnaryOp::Not,
                    expr: Box::new(self.parse_unary()?),
                })
            }
            Token::Minus => {
                self.advance();
                Ok(Expr::Unary {
                    op: UnaryOp::Neg,
                    expr: Box::new(self.parse_unary()?),
                })
            }
            _ => self.parse_primary(),
        }
    }

    fn parse_primary(&mut self) -> Result<Expr, String> {
        match self.peek().clone() {
            Token::Integer(v) => {
                self.advance();
                Ok(Expr::Literal(Literal::Integer(v)))
            }
            Token::Float(v) => {
                self.advance();
                Ok(Expr::Literal(Literal::Float(v)))
            }
            Token::String(ref s) => {
                let s = s.clone();
                self.advance();
                Ok(Expr::Literal(Literal::String(s)))
            }
            Token::Bool(v) => {
                self.advance();
                Ok(Expr::Literal(Literal::Bool(v)))
            }
            Token::LParen => {
                self.advance();
                let expr = self.parse_or()?;
                self.expect(Token::RParen)?;
                Ok(expr)
            }
            Token::Identifier(ref name) => {
                let name = name.clone();
                self.advance();
                if self.is_at(Token::Dot) {
                    let mut path = vec![name];
                    while self.is_at(Token::Dot) {
                        self.advance();
                        if let Token::Identifier(part) = self.peek().clone() {
                            self.advance();
                            path.push(part);
                        } else {
                            return Err(format!(
                                "Expected identifier after '.', found {:?}",
                                self.peek()
                            ));
                        }
                    }
                    if path[0] == "payload" {
                        Ok(Expr::PayloadAccess(path[1..].to_vec()))
                    } else {
                        Ok(Expr::Identifier(path.join(".")))
                    }
                } else {
                    Ok(Expr::Identifier(name))
                }
            }
            ref other => Err(format!("Unexpected token: {:?}", other)),
        }
    }
}
