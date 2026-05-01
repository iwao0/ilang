use std::rc::Rc;

use ilang_ast::{BinOp, Span, UnOp};

use crate::error::RuntimeError;
use crate::value::Value;

pub(crate) fn apply_unary(op: UnOp, v: Value) -> Result<Value, RuntimeError> {
    let dummy = Span::dummy();
    match (op, v) {
        (UnOp::Pos, v @ (Value::Int32(_) | Value::Int(_) | Value::Float32(_) | Value::Float(_))) => {
            Ok(v)
        }
        (UnOp::Neg, Value::Int32(n)) => n
            .checked_neg()
            .map(Value::Int32)
            .ok_or(RuntimeError::Overflow { span: dummy }),
        (UnOp::Neg, Value::Int(n)) => n
            .checked_neg()
            .map(Value::Int)
            .ok_or(RuntimeError::Overflow { span: dummy }),
        (UnOp::Neg, Value::Float32(f)) => Ok(Value::Float32(-f)),
        (UnOp::Neg, Value::Float(f)) => Ok(Value::Float(-f)),
        (UnOp::Not, Value::Bool(b)) => Ok(Value::Bool(!b)),
        (UnOp::BitNot, Value::Int32(n)) => Ok(Value::Int32(!n)),
        (UnOp::BitNot, Value::Int(n)) => Ok(Value::Int(!n)),
        _ => Err(RuntimeError::TypeError {
            msg: "invalid unary operand".into(),
            span: dummy,
        }),
    }
}

/// Numeric "rank" used to pick the result type of a mixed-type op.
/// Higher rank wins; an i64 promoted into a float context goes to f64
/// (not f32) because f32 can't hold the full i64 range without losing
/// precision.
fn promote(l: Value, r: Value) -> (Value, Value) {
    use Value::*;
    let any_float = matches!(l, Float32(_) | Float(_)) || matches!(r, Float32(_) | Float(_));
    let needs_f64 = matches!(l, Int(_) | Float(_)) || matches!(r, Int(_) | Float(_));
    if any_float {
        if needs_f64 {
            (to_f64(l), to_f64(r))
        } else {
            (to_f32(l), to_f32(r))
        }
    } else {
        // Both ints. If either side is i64, use i64; otherwise both i32.
        let needs_i64 = matches!(l, Int(_)) || matches!(r, Int(_));
        if needs_i64 {
            (to_i64(l), to_i64(r))
        } else {
            (l, r)
        }
    }
}

fn to_i64(v: Value) -> Value {
    match v {
        Value::Int32(n) => Value::Int(n as i64),
        Value::Int(_) => v,
        _ => v,
    }
}

fn to_f32(v: Value) -> Value {
    match v {
        Value::Int32(n) => Value::Float32(n as f32),
        Value::Int(n) => Value::Float32(n as f32),
        Value::Float32(_) => v,
        Value::Float(f) => Value::Float32(f as f32),
        _ => v,
    }
}

fn to_f64(v: Value) -> Value {
    match v {
        Value::Int32(n) => Value::Float(n as f64),
        Value::Int(n) => Value::Float(n as f64),
        Value::Float32(f) => Value::Float(f as f64),
        Value::Float(_) => v,
        _ => v,
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
    // Arithmetic: promote both operands to a common type, then dispatch
    // on the (now equal) variants.
    let (l, r) = promote(l, r);
    match (l, r) {
        (Value::Int32(a), Value::Int32(b)) => arith_i32(op, a, b),
        (Value::Int(a), Value::Int(b)) => arith_i64(op, a, b),
        (Value::Float32(a), Value::Float32(b)) => Ok(Value::Float32(arith_f32(op, a, b))),
        (Value::Float(a), Value::Float(b)) => Ok(Value::Float(arith_f64(op, a, b))),
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
    if let (Value::Bool(a), Value::Bool(b)) = (&l, &r) {
        if matches!(op, BinOp::Eq | BinOp::Ne) {
            return Ok(Value::Bool(if op == BinOp::Eq { a == b } else { a != b }));
        }
    }
    let is_numeric = |v: &Value| {
        matches!(
            v,
            Value::Int32(_) | Value::Int(_) | Value::Float32(_) | Value::Float(_)
        )
    };
    if !is_numeric(&l) || !is_numeric(&r) {
        return Err(RuntimeError::TypeError {
            msg: "invalid comparison operands".into(),
            span: Span::dummy(),
        });
    }
    let (l, r) = promote(l, r);
    let ord = match (&l, &r) {
        (Value::Int32(a), Value::Int32(b)) => Some(a.cmp(b)),
        (Value::Int(a), Value::Int(b)) => Some(a.cmp(b)),
        (Value::Float32(a), Value::Float32(b)) => a.partial_cmp(b),
        (Value::Float(a), Value::Float(b)) => a.partial_cmp(b),
        _ => unreachable!("promote() should have unified the variants"),
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

fn arith_i32(op: BinOp, a: i32, b: i32) -> Result<Value, RuntimeError> {
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
        _ => unreachable!("non-arithmetic BinOp in arith_i32"),
    };
    r.map(Value::Int32)
        .ok_or(RuntimeError::Overflow { span: dummy })
}

fn arith_i64(op: BinOp, a: i64, b: i64) -> Result<Value, RuntimeError> {
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
        _ => unreachable!("non-arithmetic BinOp in arith_i64"),
    };
    r.map(Value::Int)
        .ok_or(RuntimeError::Overflow { span: dummy })
}

fn arith_f32(op: BinOp, a: f32, b: f32) -> f32 {
    match op {
        BinOp::Add => a + b,
        BinOp::Sub => a - b,
        BinOp::Mul => a * b,
        BinOp::Div => a / b,
        BinOp::Rem => a % b,
        _ => unreachable!("non-arithmetic BinOp in arith_f32"),
    }
}

fn arith_f64(op: BinOp, a: f64, b: f64) -> f64 {
    match op {
        BinOp::Add => a + b,
        BinOp::Sub => a - b,
        BinOp::Mul => a * b,
        BinOp::Div => a / b,
        BinOp::Rem => a % b,
        _ => unreachable!("non-arithmetic BinOp in arith_f64"),
    }
}

fn bit_op(op: BinOp, l: Value, r: Value) -> Result<Value, RuntimeError> {
    let (l, r) = promote(l, r);
    match (l, r) {
        (Value::Int32(a), Value::Int32(b)) => Ok(Value::Int32(do_bit_i32(op, a, b)?)),
        (Value::Int(a), Value::Int(b)) => Ok(Value::Int(do_bit_i64(op, a, b)?)),
        _ => Err(RuntimeError::TypeError {
            msg: "bitwise operators require integer operands".into(),
            span: Span::dummy(),
        }),
    }
}

/// Apply `<<` / `>>`. Out-of-range shift amounts return 0 (every bit gets
/// shifted out); negative shift amounts are a runtime error since they
/// have no well-defined meaning.
fn checked_shift(op: BinOp, a: i64, b: i64, width: u32) -> Result<i64, RuntimeError> {
    if b < 0 {
        return Err(RuntimeError::TypeError {
            msg: format!("negative shift amount: {b}"),
            span: Span::dummy(),
        });
    }
    if b >= width as i64 {
        return Ok(0);
    }
    Ok(match op {
        BinOp::Shl => a.wrapping_shl(b as u32),
        BinOp::Shr => a.wrapping_shr(b as u32),
        _ => unreachable!(),
    })
}

fn do_bit_i32(op: BinOp, a: i32, b: i32) -> Result<i32, RuntimeError> {
    Ok(match op {
        BinOp::BitAnd => a & b,
        BinOp::BitOr => a | b,
        BinOp::BitXor => a ^ b,
        BinOp::Shl | BinOp::Shr => checked_shift(op, a as i64, b as i64, 32)? as i32,
        _ => unreachable!("non-bit BinOp in do_bit_i32"),
    })
}

fn do_bit_i64(op: BinOp, a: i64, b: i64) -> Result<i64, RuntimeError> {
    Ok(match op {
        BinOp::BitAnd => a & b,
        BinOp::BitOr => a | b,
        BinOp::BitXor => a ^ b,
        BinOp::Shl | BinOp::Shr => checked_shift(op, a, b, 64)?,
        _ => unreachable!("non-bit BinOp in do_bit_i64"),
    })
}

/// Apply an `as` cast at runtime. The type checker has already validated
/// that the conversion is one of the permitted numeric/bool combinations.
pub(crate) fn cast_value(v: Value, target: &ilang_ast::Type) -> Value {
    use ilang_ast::Type;
    match target {
        Type::I32 => Value::Int32(match v {
            Value::Int32(n) => n,
            Value::Int(n) => n as i32,
            Value::Float32(f) => f as i32,
            Value::Float(f) => f as i32,
            Value::Bool(b) => b as i32,
            _ => 0,
        }),
        Type::I64 => Value::Int(match v {
            Value::Int32(n) => n as i64,
            Value::Int(n) => n,
            Value::Float32(f) => f as i64,
            Value::Float(f) => f as i64,
            Value::Bool(b) => b as i64,
            _ => 0,
        }),
        Type::F32 => Value::Float32(match v {
            Value::Int32(n) => n as f32,
            Value::Int(n) => n as f32,
            Value::Float32(f) => f,
            Value::Float(f) => f as f32,
            Value::Bool(b) => b as i32 as f32,
            _ => 0.0,
        }),
        Type::F64 => Value::Float(match v {
            Value::Int32(n) => n as f64,
            Value::Int(n) => n as f64,
            Value::Float32(f) => f as f64,
            Value::Float(f) => f,
            Value::Bool(b) => b as i32 as f64,
            _ => 0.0,
        }),
        _ => v,
    }
}
