use ilang_ast::{BinOp, Type};

use crate::error::TypeError;

/// `from` can be assigned to a binding of type `to`. Numeric widening from
/// `i64` to `f64` is allowed (matches the runtime's promotion rule).
pub(crate) fn assignable(from: &Type, to: &Type) -> bool {
    if from == to {
        return true;
    }
    // Either side is the type-checker error sentinel: silently accept
    // so we don't pile cascading "type X vs Y" follow-ups on top of an
    // already-reported failure upstream.
    if matches!(from, Type::Error) || matches!(to, Type::Error) {
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
    if let (Type::Generic(g1), Type::Generic(g2)) = (from, to) {
        if g1.base == g2.base && g1.args.len() == g2.args.len() {
            return g1.args.iter().zip(g2.args.iter()).all(|(p, q)| assignable(p, q));
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
    // Raw C pointer: `*T` is assignable to `*const T` (loses write
    // capability) but not vice versa. Inner types must match
    // exactly — no covariance through pointer dereference.
    //
    // Exception: `*void` / `*const void` is the C-style "untyped
    // pointer" and flows into any `*T` slot under the usual
    // const-narrowing rule. This matches C's `void *` semantics
    // and lets binding modules ship a single typed `NULL: *void`
    // that callers drop into any handle parameter without an
    // `as *T` boilerplate. Only meaningful inside @extern(C)
    // scope — `*void` isn't nameable elsewhere.
    if let (
        Type::RawPtr { is_const: from_c, inner: from_inner },
        Type::RawPtr { is_const: to_c, inner: to_inner },
    ) = (from, to)
    {
        let from_is_void = matches!(**from_inner, Type::CVoid);
        let to_is_void = matches!(**to_inner, Type::CVoid);
        if from_inner == to_inner || from_is_void || to_is_void {
            return *to_c || !*from_c;
        }
        return false;
    }
    // NOTE: implicit `Object(N) → *N` coercion is intentionally
    // not allowed. Callers must spell out `&value` to convert an
    // `@extern(C) struct` value into the `*Struct` argument shape
    // — same explicitness ilang requires for `&local` / `&field`
    // in any other FFI context. Array-to-pointer (`T[] → *T`)
    // stays implicit because the array's storage layout already
    // makes the raw pointer the natural representation.
    // ilang `T[]` → `*T` / `*const T` raw pointer at the C boundary.
    // The array's data pointer is what's actually passed; the ARC
    // header / length sit at negative offsets and stay invisible to
    // C. Element type must match exactly, except that `*void` (the
    // C-untyped-pointer escape hatch) accepts any element shape.
    if let (
        Type::Array { elem, fixed: _ },
        Type::RawPtr { inner, .. },
    ) = (from, to)
    {
        if matches!(**inner, Type::CVoid) {
            return true;
        }
        return elem.as_ref() == inner.as_ref();
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
        // `ssize_t` aliases `i64` on 64-bit targets.
        Type::I64 | Type::SSize => true,
        Type::U8 => u8::try_from(n).is_ok(),
        Type::U16 => u16::try_from(n).is_ok(),
        Type::U32 => u32::try_from(n).is_ok(),
        // `size_t` aliases `u64` on 64-bit targets.
        Type::U64 | Type::Size => true,
        _ => false,
    }
}

/// Result type for an arithmetic binary op given the two operand types.
/// Returns `None` for unsupported combinations (e.g. signed mixed with
/// unsigned without an explicit cast). Comparison ops always return
/// `Bool`; this helper handles numeric promotion only.
pub(crate) fn numeric_result(l: &Type, r: &Type) -> Option<Type> {
    use Type::*;
    // Error sentinel on either side suppresses further checking — return
    // Error so the caller propagates it without raising a fresh BadBinary.
    if matches!(l, Type::Error) || matches!(r, Type::Error) {
        return Some(Type::Error);
    }
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
    // Error sentinel on either side: swallow the binop so a stale
    // failure doesn't manifest as a phantom BadBinary on top of the
    // original error.
    if matches!(l, Type::Error) || matches!(r, Type::Error) {
        return Ok(Type::Error);
    }
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
        // Function identity: matching `fn(...)` signatures support
        // `==` / `!=` (reference equality on the underlying closure
        // pointer; matches Node.js's `removeListener` semantics).
        // Two `let f = fn(...)` / `let g = fn(...)` are NOT equal —
        // they're distinct heap allocations.
        if matches!(op, BinOp::Eq | BinOp::Ne) {
            if let (Type::Fn(a), Type::Fn(b)) = (l, r) {
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
