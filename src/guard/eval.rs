use super::ast::{BinaryOp, Expr, Literal, UnaryOp};
use crate::engine::ActionContext;
use serde_json::Value as JsonValue;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Integer(i64),
    Float(f64),
    String(String),
    Bool(bool),
    Null,
}

pub fn eval_guard(guard: &str, ctx: &ActionContext) -> Result<bool, String> {
    use super::lexer::tokenize;
    use super::parser::parse;

    let tokens = tokenize(guard)?;
    let expr = parse(&tokens)?;
    let value = eval(&expr, ctx)?;
    match value {
        Value::Bool(b) => Ok(b),
        Value::Null => Ok(false),
        Value::Integer(i) => Ok(i != 0),
        Value::Float(f) => Ok(f != 0.0),
        Value::String(s) => Ok(!s.is_empty()),
    }
}

pub fn eval(expr: &Expr, ctx: &ActionContext) -> Result<Value, String> {
    match expr {
        Expr::Literal(lit) => match lit {
            Literal::Integer(i) => Ok(Value::Integer(*i)),
            Literal::Float(f) => Ok(Value::Float(*f)),
            Literal::String(s) => Ok(Value::String(s.clone())),
            Literal::Bool(b) => Ok(Value::Bool(*b)),
        },
        Expr::PayloadAccess(path) => {
            let payload = ctx.payload.as_ref().ok_or("No payload available")?;
            access_payload(payload, path)
        }
        Expr::Identifier(name) => Err(format!("Unknown identifier: {}", name)),
        Expr::Unary { op, expr } => {
            let val = eval(expr, ctx)?;
            match op {
                UnaryOp::Not => Ok(Value::Bool(!as_bool(&val))),
                UnaryOp::Neg => match val {
                    Value::Integer(i) => Ok(Value::Integer(-i)),
                    Value::Float(f) => Ok(Value::Float(-f)),
                    _ => Err(format!("Cannot negate {:?}", val)),
                },
            }
        }
        Expr::Binary { left, op, right } => {
            let left_val = eval(left, ctx)?;
            let right_val = eval(right, ctx)?;
            match op {
                BinaryOp::Add => arithmetic(&left_val, &right_val, |a, b| a + b, |a, b| a + b),
                BinaryOp::Sub => arithmetic(&left_val, &right_val, |a, b| a - b, |a, b| a - b),
                BinaryOp::Mul => arithmetic(&left_val, &right_val, |a, b| a * b, |a, b| a * b),
                BinaryOp::Div => arithmetic(&left_val, &right_val, |a, b| a / b, |a, b| {
                    if b == 0.0 {
                        0.0
                    } else {
                        a / b
                    }
                }),
                BinaryOp::Eq => Ok(Value::Bool(values_equal(&left_val, &right_val))),
                BinaryOp::Neq => Ok(Value::Bool(!values_equal(&left_val, &right_val))),
                BinaryOp::Lt => compare(&left_val, &right_val, |ord| ord == std::cmp::Ordering::Less),
                BinaryOp::Lte => compare(&left_val, &right_val, |ord| ord != std::cmp::Ordering::Greater),
                BinaryOp::Gt => compare(
                    &left_val,
                    &right_val,
                    |ord| ord == std::cmp::Ordering::Greater,
                ),
                BinaryOp::Gte => compare(
                    &left_val,
                    &right_val,
                    |ord| ord != std::cmp::Ordering::Less,
                ),
                BinaryOp::And => Ok(Value::Bool(as_bool(&left_val) && as_bool(&right_val))),
                BinaryOp::Or => Ok(Value::Bool(as_bool(&left_val) || as_bool(&right_val))),
            }
        }
    }
}

fn access_payload(payload: &JsonValue, path: &[String]) -> Result<Value, String> {
    let mut current = payload;
    for part in path {
        match current {
            JsonValue::Object(map) => {
                current = map.get(part).unwrap_or(&JsonValue::Null);
            }
            _ => return Ok(Value::Null),
        }
    }
    json_to_value(current)
}

fn json_to_value(v: &JsonValue) -> Result<Value, String> {
    match v {
        JsonValue::Null => Ok(Value::Null),
        JsonValue::Bool(b) => Ok(Value::Bool(*b)),
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(Value::Integer(i))
            } else if let Some(f) = n.as_f64() {
                Ok(Value::Float(f))
            } else {
                Err(format!("Unsupported number: {}", n))
            }
        }
        JsonValue::String(s) => Ok(Value::String(s.clone())),
        _ => Err("Unsupported JSON value type".to_string()),
    }
}

fn as_bool(v: &Value) -> bool {
    match v {
        Value::Bool(b) => *b,
        Value::Null => false,
        Value::Integer(i) => *i != 0,
        Value::Float(f) => *f != 0.0,
        Value::String(s) => !s.is_empty(),
    }
}

fn values_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Null, Value::Null) => true,
        (Value::Bool(a), Value::Bool(b)) => a == b,
        (Value::Integer(a), Value::Integer(b)) => a == b,
        (Value::Integer(a), Value::Float(b)) => *a as f64 == *b,
        (Value::Float(a), Value::Integer(b)) => *a == *b as f64,
        (Value::Float(a), Value::Float(b)) => a == b,
        (Value::String(a), Value::String(b)) => a == b,
        _ => false,
    }
}

fn compare(
    left_val: &Value,
    right_val: &Value,
    pred: fn(std::cmp::Ordering) -> bool,
) -> Result<Value, String> {
    if matches!(left_val, Value::Null) || matches!(right_val, Value::Null) {
        return Ok(Value::Bool(false));
    }
    let ord = value_ordering(left_val, right_val)?;
    Ok(Value::Bool(pred(ord)))
}

fn value_ordering(a: &Value, b: &Value) -> Result<std::cmp::Ordering, String> {
    match (a, b) {
        (Value::Integer(a), Value::Integer(b)) => Ok(a.cmp(b)),
        (Value::Integer(a), Value::Float(b)) => {
            (*a as f64).partial_cmp(b).ok_or_else(|| "Cannot compare floats".to_string())
        }
        (Value::Float(a), Value::Integer(b)) => {
            a.partial_cmp(&(*b as f64)).ok_or_else(|| "Cannot compare floats".to_string())
        }
        (Value::Float(a), Value::Float(b)) => {
            a.partial_cmp(b).ok_or_else(|| "Cannot compare floats".to_string())
        }
        (Value::String(a), Value::String(b)) => Ok(a.cmp(b)),
        _ => Err(format!("Cannot compare {:?} and {:?}", a, b)),
    }
}

fn arithmetic(
    left: &Value,
    right: &Value,
    int_op: fn(i64, i64) -> i64,
    float_op: fn(f64, f64) -> f64,
) -> Result<Value, String> {
    match (left, right) {
        (Value::Integer(a), Value::Integer(b)) => Ok(Value::Integer(int_op(*a, *b))),
        (Value::Integer(a), Value::Float(b)) => Ok(Value::Float(float_op(*a as f64, *b))),
        (Value::Float(a), Value::Integer(b)) => Ok(Value::Float(float_op(*a, *b as f64))),
        (Value::Float(a), Value::Float(b)) => Ok(Value::Float(float_op(*a, *b))),
        _ => Err(format!("Cannot perform arithmetic on {:?} and {:?}", left, right)),
    }
}
