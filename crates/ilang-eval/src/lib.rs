use ilang_ast::{BinOp, Expr, UnOp};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Value {
    Int(i64),
    Float(f64),
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Int(n) => write!(f, "{n}"),
            Value::Float(x) => {
                if x.is_finite() && x.fract() == 0.0 {
                    write!(f, "{x:.1}")
                } else {
                    write!(f, "{x}")
                }
            }
        }
    }
}

#[derive(Debug, Error, PartialEq)]
pub enum RuntimeError {
    #[error("integer division by zero")]
    DivisionByZero,
    #[error("integer overflow")]
    Overflow,
}

pub fn eval(expr: &Expr) -> Result<Value, RuntimeError> {
    match expr {
        Expr::Int(n) => Ok(Value::Int(*n)),
        Expr::Float(f) => Ok(Value::Float(*f)),
        Expr::Unary { op, expr } => {
            let v = eval(expr)?;
            match (op, v) {
                (UnOp::Pos, v) => Ok(v),
                (UnOp::Neg, Value::Int(n)) => n.checked_neg().map(Value::Int).ok_or(RuntimeError::Overflow),
                (UnOp::Neg, Value::Float(f)) => Ok(Value::Float(-f)),
            }
        }
        Expr::Binary { op, lhs, rhs } => {
            let l = eval(lhs)?;
            let r = eval(rhs)?;
            apply_binary(*op, l, r)
        }
    }
}

fn apply_binary(op: BinOp, l: Value, r: Value) -> Result<Value, RuntimeError> {
    match (l, r) {
        (Value::Int(a), Value::Int(b)) => int_op(op, a, b),
        (a, b) => {
            let a = to_f64(a);
            let b = to_f64(b);
            Ok(Value::Float(float_op(op, a, b)))
        }
    }
}

fn to_f64(v: Value) -> f64 {
    match v {
        Value::Int(n) => n as f64,
        Value::Float(f) => f,
    }
}

fn int_op(op: BinOp, a: i64, b: i64) -> Result<Value, RuntimeError> {
    let r = match op {
        BinOp::Add => a.checked_add(b),
        BinOp::Sub => a.checked_sub(b),
        BinOp::Mul => a.checked_mul(b),
        BinOp::Div => {
            if b == 0 {
                return Err(RuntimeError::DivisionByZero);
            }
            a.checked_div(b)
        }
        BinOp::Rem => {
            if b == 0 {
                return Err(RuntimeError::DivisionByZero);
            }
            a.checked_rem(b)
        }
    };
    r.map(Value::Int).ok_or(RuntimeError::Overflow)
}

fn float_op(op: BinOp, a: f64, b: f64) -> f64 {
    match op {
        BinOp::Add => a + b,
        BinOp::Sub => a - b,
        BinOp::Mul => a * b,
        BinOp::Div => a / b,
        BinOp::Rem => a % b,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ilang_lexer::tokenize;
    use ilang_parser::parse;

    fn run(src: &str) -> Result<Value, RuntimeError> {
        let toks = tokenize(src).unwrap();
        let ast = parse(&toks).unwrap();
        eval(&ast)
    }

    #[test]
    fn int_arithmetic() {
        assert_eq!(run("1 + 2 * 3").unwrap(), Value::Int(7));
        assert_eq!(run("(1 + 2) * 3").unwrap(), Value::Int(9));
        assert_eq!(run("7 / 2").unwrap(), Value::Int(3));
        assert_eq!(run("7 % 2").unwrap(), Value::Int(1));
        assert_eq!(run("-2 + 1").unwrap(), Value::Int(-1));
    }

    #[test]
    fn float_promotion() {
        assert_eq!(run("7.0 / 2").unwrap(), Value::Float(3.5));
        assert_eq!(run("1 + 2.0").unwrap(), Value::Float(3.0));
        assert_eq!(run("-2.5e1 + 1").unwrap(), Value::Float(-24.0));
    }

    #[test]
    fn division_by_zero_int() {
        assert_eq!(run("1 / 0"), Err(RuntimeError::DivisionByZero));
        assert_eq!(run("1 % 0"), Err(RuntimeError::DivisionByZero));
    }

    #[test]
    fn division_by_zero_float() {
        let v = run("1.0 / 0").unwrap();
        match v {
            Value::Float(x) => assert!(x.is_infinite()),
            _ => panic!("expected float inf"),
        }
        let v = run("0.0 / 0").unwrap();
        match v {
            Value::Float(x) => assert!(x.is_nan()),
            _ => panic!("expected float nan"),
        }
    }

    #[test]
    fn overflow_detected() {
        let src = format!("{} + 1", i64::MAX);
        assert_eq!(run(&src), Err(RuntimeError::Overflow));
    }

    #[test]
    fn display_format() {
        assert_eq!(Value::Int(7).to_string(), "7");
        assert_eq!(Value::Float(3.5).to_string(), "3.5");
        assert_eq!(Value::Float(3.0).to_string(), "3.0");
    }
}

#[cfg(test)]
mod _dev_deps {
    // ensure the lexer/parser appear in tests
}
