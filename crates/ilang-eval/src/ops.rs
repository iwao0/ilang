use std::rc::Rc;

use ilang_ast::{BinOp, Span, Type, UnOp};

use crate::error::RuntimeError;
use crate::value::Value;

// ─── Helpers ────────────────────────────────────────────────────────────

fn dummy() -> Span {
    Span::dummy()
}

/// Map a numeric `Value` to its declared `Type`. Non-numeric values map
/// to `Type::Unit` (callers should reject this case before reaching here).
fn type_of(v: &Value) -> Type {
    match v {
        Value::Int8(_) => Type::I8,
        Value::Int16(_) => Type::I16,
        Value::Int32(_) => Type::I32,
        Value::Int(_) => Type::I64,
        Value::UInt8(_) => Type::U8,
        Value::UInt16(_) => Type::U16,
        Value::UInt32(_) => Type::U32,
        Value::UInt64(_) => Type::U64,
        Value::Float32(_) => Type::F32,
        Value::Float(_) => Type::F64,
        _ => Type::Unit,
    }
}

/// Promote both operands of a binary op to a common numeric type. Mirrors
/// `numeric_result` in ilang-types: same-signedness ints widen, mixed
/// signed/unsigned is unreachable here (the type checker rejects it),
/// any int + any float promotes to a float wide enough to hold the int.
fn promote(l: Value, r: Value) -> (Value, Value) {
    use Value::*;
    let l_int = matches!(
        l,
        Int8(_) | Int16(_) | Int32(_) | Int(_)
            | UInt8(_) | UInt16(_) | UInt32(_) | UInt64(_)
    );
    let r_int = matches!(
        r,
        Int8(_) | Int16(_) | Int32(_) | Int(_)
            | UInt8(_) | UInt16(_) | UInt32(_) | UInt64(_)
    );
    let l_float = matches!(l, Float32(_) | Float(_));
    let r_float = matches!(r, Float32(_) | Float(_));

    if l_int && r_int {
        // Same signedness assumed (type checker enforces). Pick the wider.
        let lt = type_of(&l);
        let rt = type_of(&r);
        if lt.int_width() >= rt.int_width() {
            return (l, cast_value(r, &lt));
        }
        return (cast_value(l, &rt), r);
    }
    if l_float && r_float {
        let needs_f64 = matches!(l, Float(_)) || matches!(r, Float(_));
        let target = if needs_f64 { Type::F64 } else { Type::F32 };
        return (cast_value(l, &target), cast_value(r, &target));
    }
    // Mixed int + float.
    let int_t = if l_int { type_of(&l) } else { type_of(&r) };
    let float_t = if l_float { type_of(&l) } else { type_of(&r) };
    let needs_f64 = matches!(float_t, Type::F64) || int_t.int_width() >= 32;
    let target = if needs_f64 { Type::F64 } else { Type::F32 };
    (cast_value(l, &target), cast_value(r, &target))
}

// ─── Unary ──────────────────────────────────────────────────────────────

pub(crate) fn apply_unary(op: UnOp, v: Value) -> Result<Value, RuntimeError> {
    let span = dummy();
    match (op, v) {
        // Identity (`+x`).
        (UnOp::Pos, v) if matches!(
            v,
            Value::Int8(_) | Value::Int16(_) | Value::Int32(_) | Value::Int(_)
                | Value::UInt8(_) | Value::UInt16(_) | Value::UInt32(_) | Value::UInt64(_)
                | Value::Float32(_) | Value::Float(_)
        ) =>
        {
            Ok(v)
        }
        // Negation (signed only — checker rejects unsigned). Wraps
        // on `MIN` (e.g. `-i8::MIN` stays at `i8::MIN`) to match the
        // JIT, which uses Cranelift's plain `ineg` / arithmetic.
        (UnOp::Neg, Value::Int8(n)) => Ok(Value::Int8(n.wrapping_neg())),
        (UnOp::Neg, Value::Int16(n)) => Ok(Value::Int16(n.wrapping_neg())),
        (UnOp::Neg, Value::Int32(n)) => Ok(Value::Int32(n.wrapping_neg())),
        (UnOp::Neg, Value::Int(n)) => Ok(Value::Int(n.wrapping_neg())),
        (UnOp::Neg, Value::Float32(f)) => Ok(Value::Float32(-f)),
        (UnOp::Neg, Value::Float(f)) => Ok(Value::Float(-f)),
        (UnOp::Not, Value::Bool(b)) => Ok(Value::Bool(!b)),
        // Bit-not on every int width (signed and unsigned).
        (UnOp::BitNot, Value::Int8(n)) => Ok(Value::Int8(!n)),
        (UnOp::BitNot, Value::Int16(n)) => Ok(Value::Int16(!n)),
        (UnOp::BitNot, Value::Int32(n)) => Ok(Value::Int32(!n)),
        (UnOp::BitNot, Value::Int(n)) => Ok(Value::Int(!n)),
        (UnOp::BitNot, Value::UInt8(n)) => Ok(Value::UInt8(!n)),
        (UnOp::BitNot, Value::UInt16(n)) => Ok(Value::UInt16(!n)),
        (UnOp::BitNot, Value::UInt32(n)) => Ok(Value::UInt32(!n)),
        (UnOp::BitNot, Value::UInt64(n)) => Ok(Value::UInt64(!n)),
        _ => Err(RuntimeError::TypeError {
            msg: "invalid unary operand".into(),
            span,
        }),
    }
}

// ─── Binary arithmetic / comparison / bit ───────────────────────────────

pub(crate) fn apply_binary(op: BinOp, l: Value, r: Value) -> Result<Value, RuntimeError> {
    if matches!(
        op,
        BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge
    ) {
        return compare(op, l, r);
    }
    if matches!(
        op,
        BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor | BinOp::Shl | BinOp::Shr
    ) {
        return bit_op(op, l, r);
    }
    // String concatenation: `s + t` allocates a new string.
    if let (BinOp::Add, Value::Str(a), Value::Str(b)) = (op, &l, &r) {
        let mut out = String::with_capacity(a.len() + b.len());
        out.push_str(a);
        out.push_str(b);
        return Ok(Value::Str(Rc::new(out)));
    }
    let (l, r) = promote(l, r);
    arith(op, l, r)
}

/// `arith` assumes both operands have already been promoted to the same
/// concrete numeric variant.
fn arith(op: BinOp, l: Value, r: Value) -> Result<Value, RuntimeError> {
    macro_rules! int_arith {
        ($a:expr, $b:expr, $ctor:ident) => {{
            // Add / Sub / Mul wrap on overflow to match the JIT,
            // which uses Cranelift's plain `iadd` / `isub` / `imul`.
            // Div / Rem still trap on a zero divisor (no IEEE-style
            // result for integers).
            let r = match op {
                BinOp::Add => $a.wrapping_add($b),
                BinOp::Sub => $a.wrapping_sub($b),
                BinOp::Mul => $a.wrapping_mul($b),
                BinOp::Div => {
                    if $b == 0 {
                        return Err(RuntimeError::DivisionByZero { span: dummy() });
                    }
                    $a.wrapping_div($b)
                }
                BinOp::Rem => {
                    if $b == 0 {
                        return Err(RuntimeError::DivisionByZero { span: dummy() });
                    }
                    $a.wrapping_rem($b)
                }
                _ => unreachable!("non-arith op in arith()"),
            };
            Ok(Value::$ctor(r))
        }};
    }
    macro_rules! float_arith {
        ($a:expr, $b:expr, $ctor:ident) => {
            Ok(Value::$ctor(match op {
                BinOp::Add => $a + $b,
                BinOp::Sub => $a - $b,
                BinOp::Mul => $a * $b,
                BinOp::Div => $a / $b,
                BinOp::Rem => $a % $b,
                _ => unreachable!("non-arith op in arith()"),
            }))
        };
    }
    match (l, r) {
        (Value::Int8(a), Value::Int8(b)) => int_arith!(a, b, Int8),
        (Value::Int16(a), Value::Int16(b)) => int_arith!(a, b, Int16),
        (Value::Int32(a), Value::Int32(b)) => int_arith!(a, b, Int32),
        (Value::Int(a), Value::Int(b)) => int_arith!(a, b, Int),
        (Value::UInt8(a), Value::UInt8(b)) => int_arith!(a, b, UInt8),
        (Value::UInt16(a), Value::UInt16(b)) => int_arith!(a, b, UInt16),
        (Value::UInt32(a), Value::UInt32(b)) => int_arith!(a, b, UInt32),
        (Value::UInt64(a), Value::UInt64(b)) => int_arith!(a, b, UInt64),
        (Value::Float32(a), Value::Float32(b)) => float_arith!(a, b, Float32),
        (Value::Float(a), Value::Float(b)) => float_arith!(a, b, Float),
        _ => Err(RuntimeError::TypeError {
            msg: "invalid binary operands".into(),
            span: dummy(),
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
            span: dummy(),
        });
    }
    if let (Value::Bool(a), Value::Bool(b)) = (&l, &r) {
        if matches!(op, BinOp::Eq | BinOp::Ne) {
            return Ok(Value::Bool(if op == BinOp::Eq { a == b } else { a != b }));
        }
    }
    if let (Value::Str(a), Value::Str(b)) = (&l, &r) {
        if matches!(op, BinOp::Eq | BinOp::Ne) {
            let same = a.as_str() == b.as_str();
            return Ok(Value::Bool(if op == BinOp::Eq { same } else { !same }));
        }
    }
    let l_t = type_of(&l);
    let r_t = type_of(&r);
    if !l_t.is_numeric() || !r_t.is_numeric() {
        return Err(RuntimeError::TypeError {
            msg: "invalid comparison operands".into(),
            span: dummy(),
        });
    }
    let (l, r) = promote(l, r);
    let ord = match (&l, &r) {
        (Value::Int8(a), Value::Int8(b)) => Some(a.cmp(b)),
        (Value::Int16(a), Value::Int16(b)) => Some(a.cmp(b)),
        (Value::Int32(a), Value::Int32(b)) => Some(a.cmp(b)),
        (Value::Int(a), Value::Int(b)) => Some(a.cmp(b)),
        (Value::UInt8(a), Value::UInt8(b)) => Some(a.cmp(b)),
        (Value::UInt16(a), Value::UInt16(b)) => Some(a.cmp(b)),
        (Value::UInt32(a), Value::UInt32(b)) => Some(a.cmp(b)),
        (Value::UInt64(a), Value::UInt64(b)) => Some(a.cmp(b)),
        (Value::Float32(a), Value::Float32(b)) => a.partial_cmp(b),
        (Value::Float(a), Value::Float(b)) => a.partial_cmp(b),
        _ => unreachable!("promote should have unified the variants"),
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
            span: dummy(),
        }),
    }
}

// ─── Bit ops + shifts ───────────────────────────────────────────────────

/// Mask the shift amount to the operand's bit width, matching
/// Cranelift's ishl / sshr / ushr semantics so interpreter and JIT
/// agree on `x << n` for every `n` (including negatives and amounts
/// >= width). All supported widths are powers of two, so width-1 is
/// the correct mask.
fn shift_amount(amount: i64, width: u32) -> u32 {
    (amount as u32) & (width - 1)
}

fn bit_op(op: BinOp, l: Value, r: Value) -> Result<Value, RuntimeError> {
    let (l, r) = promote(l, r);
    macro_rules! shift {
        ($a:expr, $b:expr, $width:literal, $ctor:ident) => {{
            let k = shift_amount($b as i64, $width);
            Value::$ctor(match op {
                BinOp::Shl => $a.wrapping_shl(k),
                BinOp::Shr => $a.wrapping_shr(k),
                _ => unreachable!("non-shift op in shift!()"),
            })
        }};
    }
    macro_rules! bitwise {
        ($a:expr, $b:expr, $ctor:ident, $width:literal) => {{
            match op {
                BinOp::BitAnd => Value::$ctor($a & $b),
                BinOp::BitOr => Value::$ctor($a | $b),
                BinOp::BitXor => Value::$ctor($a ^ $b),
                BinOp::Shl | BinOp::Shr => shift!($a, $b, $width, $ctor),
                _ => unreachable!("non-bit op in bit_op()"),
            }
        }};
    }
    Ok(match (l, r) {
        (Value::Int8(a), Value::Int8(b)) => bitwise!(a, b, Int8, 8),
        (Value::Int16(a), Value::Int16(b)) => bitwise!(a, b, Int16, 16),
        (Value::Int32(a), Value::Int32(b)) => bitwise!(a, b, Int32, 32),
        (Value::Int(a), Value::Int(b)) => bitwise!(a, b, Int, 64),
        (Value::UInt8(a), Value::UInt8(b)) => bitwise!(a, b, UInt8, 8),
        (Value::UInt16(a), Value::UInt16(b)) => bitwise!(a, b, UInt16, 16),
        (Value::UInt32(a), Value::UInt32(b)) => bitwise!(a, b, UInt32, 32),
        (Value::UInt64(a), Value::UInt64(b)) => bitwise!(a, b, UInt64, 64),
        _ => {
            return Err(RuntimeError::TypeError {
                msg: "bitwise operators require integer operands".into(),
                span: dummy(),
            });
        }
    })
}

// ─── `as` cast ──────────────────────────────────────────────────────────

/// Apply an `as` cast at runtime. The type checker has already validated
/// that the conversion is permitted; here we just compute the new value.
/// Goes through `i128` (for ints) or `f64` (for floats) as an intermediate
/// so the source variant doesn't matter.
/// Common-int-view of a numeric value. Returns `None` for
/// non-numeric values (heap, enum, object, …).
pub fn numeric_to_i128(v: &Value) -> Option<i128> {
    match v {
        Value::Int8(n) => Some(*n as i128),
        Value::Int16(n) => Some(*n as i128),
        Value::Int32(n) => Some(*n as i128),
        Value::Int(n) => Some(*n as i128),
        Value::UInt8(n) => Some(*n as i128),
        Value::UInt16(n) => Some(*n as i128),
        Value::UInt32(n) => Some(*n as i128),
        Value::UInt64(n) => Some(*n as i128),
        Value::Bool(b) => Some(*b as i128),
        _ => None,
    }
}

pub(crate) fn cast_value(v: Value, target: &Type) -> Value {
    let from_int: Option<i128> = match &v {
        Value::Int8(n) => Some(*n as i128),
        Value::Int16(n) => Some(*n as i128),
        Value::Int32(n) => Some(*n as i128),
        Value::Int(n) => Some(*n as i128),
        Value::UInt8(n) => Some(*n as i128),
        Value::UInt16(n) => Some(*n as i128),
        Value::UInt32(n) => Some(*n as i128),
        Value::UInt64(n) => Some(*n as i128),
        Value::Bool(b) => Some(*b as i128),
        _ => None,
    };
    let from_float: Option<f64> = match &v {
        Value::Float32(f) => Some(*f as f64),
        Value::Float(f) => Some(*f),
        _ => None,
    };
    match target {
        Type::I8 => Value::Int8(from_int.map(|n| n as i8).unwrap_or_else(|| from_float.unwrap_or(0.0) as i8)),
        Type::I16 => Value::Int16(from_int.map(|n| n as i16).unwrap_or_else(|| from_float.unwrap_or(0.0) as i16)),
        Type::I32 => Value::Int32(from_int.map(|n| n as i32).unwrap_or_else(|| from_float.unwrap_or(0.0) as i32)),
        Type::I64 => Value::Int(from_int.map(|n| n as i64).unwrap_or_else(|| from_float.unwrap_or(0.0) as i64)),
        Type::U8 => Value::UInt8(from_int.map(|n| n as u8).unwrap_or_else(|| from_float.unwrap_or(0.0) as u8)),
        Type::U16 => Value::UInt16(from_int.map(|n| n as u16).unwrap_or_else(|| from_float.unwrap_or(0.0) as u16)),
        Type::U32 => Value::UInt32(from_int.map(|n| n as u32).unwrap_or_else(|| from_float.unwrap_or(0.0) as u32)),
        Type::U64 => Value::UInt64(from_int.map(|n| n as u64).unwrap_or_else(|| from_float.unwrap_or(0.0) as u64)),
        Type::F32 => Value::Float32(
            from_int
                .map(|n| n as f32)
                .unwrap_or_else(|| from_float.unwrap_or(0.0) as f32),
        ),
        Type::F64 => Value::Float(
            from_int
                .map(|n| n as f64)
                .unwrap_or_else(|| from_float.unwrap_or(0.0)),
        ),
        // For an array target, deep-cast each element. This keeps the
        // runtime representation aligned with the declared element type
        // (e.g. `let a: i32[] = [1, 2]` actually stores `Int32` values).
        // A fresh `Rc` is allocated, so an annotated re-binding does not
        // alias with the source.
        Type::Array { elem, .. } => {
            if let Value::Array(arr) = v {
                let casted: Vec<Value> = arr
                    .borrow()
                    .iter()
                    .cloned()
                    .map(|el| cast_value(el, elem))
                    .collect();
                Value::Array(std::rc::Rc::new(std::cell::RefCell::new(casted)))
            } else {
                v
            }
        }
        // Optional target: pass `None`/`Some` through untouched, otherwise
        // auto-wrap a bare value with `Some` (deep-casting the inner so
        // `let x: i32? = 5` stores `Some(Int32(5))`).
        Type::Optional(inner) => match v {
            Value::None => Value::None,
            Value::Some(boxed) => Value::Some(Box::new(cast_value(*boxed, inner))),
            other => Value::Some(Box::new(cast_value(other, inner))),
        },
        // Weak target: downgrade a strong Object reference. A value
        // already typed as Weak (re-binding) passes through.
        Type::Weak(_inner) => match v {
            Value::Weak(_) => v,
            Value::Object(obj) => Value::Weak(std::rc::Rc::downgrade(&obj)),
            other => other, // type checker should have caught mismatches
        },
        _ => v,
    }
}
