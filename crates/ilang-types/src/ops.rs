use ilang_ast::{BinOp, Type};

use crate::error::TypeError;

/// `from` can be assigned to a binding of type `to`. Numeric widening from
/// `i64` to `f64` is allowed (matches the runtime's promotion rule).
pub(crate) fn assignable(from: &Type, to: &Type) -> bool {
    if from == to {
        return true;
    }
    if matches!(to, Type::Any) {
        return true;
    }
    // Internal-only: `Any` on the source side stands for an
    // unsolved type variable (e.g. the `E` in `Result::Ok(5)` where
    // E hasn't been pinned by an annotation yet). Accepting it as
    // assignable to anything lets a later annotation refine the
    // type. The parser doesn't produce `Any` so this can't be
    // exploited from user code.
    if matches!(from, Type::Any) {
        return true;
    }
    // Generic instantiations: same base, args must be pairwise
    // assignable. (We don't do variance — invariant for now, matches
    // Rust's defaults.)
    if let (
        Type::Generic { base: b1, args: a1 },
        Type::Generic { base: b2, args: a2 },
    ) = (from, to)
    {
        if b1 == b2 && a1.len() == a2.len() {
            return a1.iter().zip(a2.iter()).all(|(p, q)| assignable(p, q));
        }
        return false;
    }
    // `none` (typed as Optional<Any>) is assignable to any Optional.
    if let (Type::Optional(inner), Type::Optional(_)) = (from, to) {
        if matches!(inner.as_ref(), Type::Any) {
            return true;
        }
    }
    // T? to U?: structural check on the inner.
    if let (Type::Optional(a), Type::Optional(b)) = (from, to) {
        return assignable(a, b);
    }
    // Auto-wrap: T → T? where T is assignable to inner. Lets
    // `let x: User? = some_user` and `f(arg: User?)` work.
    if let Type::Optional(inner) = to {
        return assignable(from, inner);
    }
    // Auto-downgrade: a strong `T` reference can be assigned to a
    // `T.weak` slot. Same-class match required (no implicit subtyping).
    if let Type::Weak(inner) = to {
        return from == inner.as_ref();
    }
    // Arrays: element types must match exactly. Fixed lengths must agree;
    // there is intentionally no implicit conversion between fixed and
    // dynamic arrays (use a copy/conversion when explicit ones land).
    if let (
        Type::Array { elem: e1, fixed: f1 },
        Type::Array { elem: e2, fixed: f2 },
    ) = (from, to)
    {
        return e1 == e2 && f1 == f2;
    }
    // Same-signedness ints convert freely (widening or narrowing).
    if from.is_signed_int() && to.is_signed_int() {
        return true;
    }
    if from.is_unsigned_int() && to.is_unsigned_int() {
        return true;
    }
    // Any int → any float is implicit. Float → int and signed ↔ unsigned
    // require an explicit `as` cast.
    if from.is_int() && to.is_float() {
        return true;
    }
    if from.is_float() && to.is_float() {
        return true;
    }
    false
}

/// True if an integer literal `n` can fit (by value) into a target int
/// type. For `U64` we accept any i64 since `0xFFFFFFFFFFFFFFFF` parses
/// as i64 = -1 and is meant to be the bit pattern.
pub(crate) fn int_literal_fits(n: i64, t: &Type) -> bool {
    match t {
        Type::I8 => i8::try_from(n).is_ok(),
        Type::I16 => i16::try_from(n).is_ok(),
        Type::I32 => i32::try_from(n).is_ok(),
        Type::I64 => true,
        Type::U8 => u8::try_from(n).is_ok(),
        Type::U16 => u16::try_from(n).is_ok(),
        Type::U32 => u32::try_from(n).is_ok(),
        Type::U64 => true,
        _ => false,
    }
}

/// Result type for an arithmetic binary op given the two operand types.
/// Returns `None` for unsupported combinations (e.g. signed mixed with
/// unsigned without an explicit cast). Comparison ops always return
/// `Bool`; this helper handles numeric promotion only.
pub(crate) fn numeric_result(l: &Type, r: &Type) -> Option<Type> {
    use Type::*;
    // Reject non-numeric inputs up front. Without this guard the
    // "one int + one float" fallthrough at the end silently treats
    // arbitrary types as `F32`, which made `Object == Object` and
    // `Array == Array` quietly pass type-checking.
    if !l.is_numeric() || !r.is_numeric() {
        return None;
    }
    if l == r {
        return Some(l.clone());
    }
    // Both ints: same signedness widens to the wider one; mixed signedness
    // is rejected (the user must `as`-cast one side).
    if l.is_int() && r.is_int() {
        if l.is_signed_int() != r.is_signed_int() {
            return None;
        }
        return Some(if l.int_width() >= r.int_width() {
            l.clone()
        } else {
            r.clone()
        });
    }
    // Both floats: widest wins.
    if l.is_float() && r.is_float() {
        return Some(if matches!(l, F64) || matches!(r, F64) { F64 } else { F32 });
    }
    // One int + one float: result is float. Wider int forces f64 to keep
    // precision (an i32/u32/i64/u64 doesn't fit losslessly in f32).
    let (int_t, float_t) = if l.is_int() { (l, r) } else { (r, l) };
    let needs_f64 = matches!(float_t, F64) || int_t.int_width() >= 32;
    Some(if needs_f64 { F64 } else { F32 })
}

pub(crate) fn bin_result(op: BinOp, l: &Type, r: &Type) -> Result<Type, TypeError> {
    let is_bit = matches!(
        op,
        BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor | BinOp::Shl | BinOp::Shr
    );
    if is_bit {
        // Bit ops accept any int (signed or unsigned); same promotion
        // rules as arithmetic. Mixed-signedness still requires `as`.
        if l.is_int() && r.is_int() {
            if let Some(t) = numeric_result(l, r) {
                return Ok(t);
            }
            return Err(mixed_signedness_or_bad(l, r));
        }
        return Err(TypeError::BadBinary {
            lhs: l.clone(),
            rhs: r.clone(),
            span: ilang_ast::Span::dummy(),
        });
    }
    let is_compare = matches!(
        op,
        BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge
    );
    let result = numeric_result(l, r);
    if is_compare {
        if matches!(op, BinOp::Eq | BinOp::Ne) && l == &Type::Bool && r == &Type::Bool {
            return Ok(Type::Bool);
        }
        // String supports == and != (structural equality), but not ordering.
        if matches!(op, BinOp::Eq | BinOp::Ne) && l == &Type::Str && r == &Type::Str {
            return Ok(Type::Bool);
        }
        // Object identity: same class on both sides supports == / !=.
        if matches!(op, BinOp::Eq | BinOp::Ne) {
            if let (Type::Object(a), Type::Object(b)) = (l, r) {
                if a == b {
                    return Ok(Type::Bool);
                }
            }
        }
        if result.is_some() {
            return Ok(Type::Bool);
        }
        return Err(mixed_signedness_or_bad(l, r));
    }
    // String concatenation: `+` between two strings yields a new string.
    if matches!(op, BinOp::Add) && l == &Type::Str && r == &Type::Str {
        return Ok(Type::Str);
    }
    result.ok_or_else(|| mixed_signedness_or_bad(l, r))
}

/// Pick the more helpful error for a failed numeric op: if both sides
/// are integers but their signedness disagrees, point the user at the
/// `as` cast they need; otherwise fall back to the generic BadBinary.
fn mixed_signedness_or_bad(l: &Type, r: &Type) -> TypeError {
    if l.is_int() && r.is_int() && l.is_signed_int() != r.is_signed_int() {
        return TypeError::MixedSignedness {
            lhs: l.clone(),
            rhs: r.clone(),
            span: ilang_ast::Span::dummy(),
        };
    }
    TypeError::BadBinary {
        lhs: l.clone(),
        rhs: r.clone(),
        span: ilang_ast::Span::dummy(),
    }
}
