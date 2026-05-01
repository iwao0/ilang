use std::rc::Rc;

use ilang_ast::{BinOp, Span, UnOp};

use crate::error::RuntimeError;
use crate::value::Value;

pub(crate) fn apply_unary(op: UnOp, v: Value) -> Result<Value, RuntimeError> {
    match (op, &v) {
        (UnOp::Pos, Value::Int(_)) | (UnOp::Pos, Value::Float(_)) => Ok(v),
        (UnOp::Neg, Value::Int(n)) => n
            .checked_neg()
            .map(Value::Int)
            .ok_or(RuntimeError::Overflow {
                span: Span::dummy(),
            }),
        (UnOp::Neg, Value::Float(f)) => Ok(Value::Float(-f)),
        (UnOp::Not, Value::Bool(b)) => Ok(Value::Bool(!b)),
        (UnOp::BitNot, Value::Int(n)) => Ok(Value::Int(!n)),
        _ => Err(RuntimeError::TypeError {
            msg: "invalid unary operand".into(),
            span: Span::dummy(),
        }),
    }
}

pub(crate) fn apply_binary(op: BinOp, l: Value, r: Value) -> Result<Value, RuntimeError> {
    let is_compare = matches!(
        op,
        BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge
    );
    if is_compare {
        return compare(op, l, r);
    }
    let is_bit = matches!(
        op,
        BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor | BinOp::Shl | BinOp::Shr
    );
    if is_bit {
        return bit_op(op, l, r);
    }
    match (l, r) {
        (Value::Int(a), Value::Int(b)) => int_op(op, a, b),
        (a @ (Value::Int(_) | Value::Float(_)), b @ (Value::Int(_) | Value::Float(_))) => {
            Ok(Value::Float(float_op(op, to_f64(&a), to_f64(&b))))
        }
        _ => Err(RuntimeError::TypeError {
            msg: "invalid binary operands".into(),
            span: Span::dummy(),
        }),
    }
}

fn compare(op: BinOp, l: Value, r: Value) -> Result<Value, RuntimeError> {
    use std::cmp::Ordering;
    if let (Value::Object(a), Value::Object(b)) = (&l, &r) {
        if matches!(op, BinOp::Eq | BinOp::Ne) {
            let same = Rc::ptr_eq(a, b);
            return Ok(Value::Bool(if op == BinOp::Eq { same } else { !same }));
        }
        return Err(RuntimeError::TypeError {
            msg: "objects support only == and !=".into(),
            span: Span::dummy(),
        });
    }
    let ord = match (&l, &r) {
        (Value::Int(a), Value::Int(b)) => Some(a.cmp(b)),
        (Value::Int(_), Value::Float(_))
        | (Value::Float(_), Value::Int(_))
        | (Value::Float(_), Value::Float(_)) => to_f64(&l).partial_cmp(&to_f64(&r)),
        (Value::Bool(a), Value::Bool(b)) if matches!(op, BinOp::Eq | BinOp::Ne) => Some(a.cmp(b)),
        _ => {
            return Err(RuntimeError::TypeError {
                msg: "invalid comparison operands".into(),
                span: Span::dummy(),
            });
        }
    };
    let result = match (op, ord) {
        (BinOp::Eq, Some(o)) => o == Ordering::Equal,
        (BinOp::Ne, Some(o)) => o != Ordering::Equal,
        (BinOp::Lt, Some(o)) => o == Ordering::Less,
        (BinOp::Le, Some(o)) => o != Ordering::Greater,
        (BinOp::Gt, Some(o)) => o == Ordering::Greater,
        (BinOp::Ge, Some(o)) => o != Ordering::Less,
        (BinOp::Eq, None) => false,
        (BinOp::Ne, None) => true,
        (_, None) => false,
        _ => unreachable!("non-comparison op in compare()"),
    };
    Ok(Value::Bool(result))
}

pub(crate) fn as_bool(v: Value) -> Result<bool, RuntimeError> {
    match v {
        Value::Bool(b) => Ok(b),
        _ => Err(RuntimeError::TypeError {
            msg: "expected bool".into(),
            span: Span::dummy(),
        }),
    }
}

fn to_f64(v: &Value) -> f64 {
    match v {
        Value::Int(n) => *n as f64,
        Value::Float(f) => *f,
        _ => 0.0,
    }
}

fn int_op(op: BinOp, a: i64, b: i64) -> Result<Value, RuntimeError> {
    let dummy = Span::dummy();
    let r = match op {
        BinOp::Add => a.checked_add(b),
        BinOp::Sub => a.checked_sub(b),
        BinOp::Mul => a.checked_mul(b),
        BinOp::Div => {
            if b == 0 {
                return Err(RuntimeError::DivisionByZero { span: dummy });
            }
            a.checked_div(b)
        }
        BinOp::Rem => {
            if b == 0 {
                return Err(RuntimeError::DivisionByZero { span: dummy });
            }
            a.checked_rem(b)
        }
        _ => unreachable!("non-arithmetic BinOp in int_op"),
    };
    r.map(Value::Int).ok_or(RuntimeError::Overflow { span: dummy })
}

fn bit_op(op: BinOp, l: Value, r: Value) -> Result<Value, RuntimeError> {
    let (Value::Int(a), Value::Int(b)) = (l, r) else {
        return Err(RuntimeError::TypeError {
            msg: "bitwise operators require i64 operands".into(),
            span: Span::dummy(),
        });
    };
    Ok(Value::Int(match op {
        BinOp::BitAnd => a & b,
        BinOp::BitOr => a | b,
        BinOp::BitXor => a ^ b,
        // Shifts: clamp the rhs into [0, 63] using Rust's wrapping_shl/shr
        // semantics by masking with 63 to avoid panics on out-of-range shifts.
        BinOp::Shl => a.wrapping_shl((b as u32) & 63),
        BinOp::Shr => a.wrapping_shr((b as u32) & 63),
        _ => unreachable!("non-bit BinOp in bit_op"),
    }))
}

fn float_op(op: BinOp, a: f64, b: f64) -> f64 {
    match op {
        BinOp::Add => a + b,
        BinOp::Sub => a - b,
        BinOp::Mul => a * b,
        BinOp::Div => a / b,
        BinOp::Rem => a % b,
        _ => unreachable!("non-arithmetic BinOp in float_op"),
    }
}
