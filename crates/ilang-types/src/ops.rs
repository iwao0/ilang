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
    let from_int = matches!(from, Type::I32 | Type::I64);
    let to_int = matches!(to, Type::I32 | Type::I64);
    let from_float = matches!(from, Type::F32 | Type::F64);
    let to_float = matches!(to, Type::F32 | Type::F64);
    // C/JS-style: any int converts implicitly to any int (truncating if
    // narrower); any int converts to any float; float widens to wider
    // float. Float → int is *not* implicit — requires `as`.
    (from_int && to_int) || (from_int && to_float) || (from_float && to_float)
}

/// Result type for an arithmetic binary op given the two operand types.
/// Comparison ops always return `Bool`; this helper handles numeric
/// promotion only.
pub(crate) fn numeric_result(l: &Type, r: &Type) -> Option<Type> {
    use Type::*;
    Some(match (l, r) {
        (I32, I32) => I32,
        (I32, I64) | (I64, I32) | (I64, I64) => I64,
        (F32, F32) => F32,
        (I32, F32) | (F32, I32) => F32,
        (F32, F64) | (F64, F32) | (F64, F64) => F64,
        (I64, F32) | (F32, I64) | (I32, F64) | (F64, I32) | (I64, F64) | (F64, I64) => F64,
        _ => return None,
    })
}

pub(crate) fn bin_result(op: BinOp, l: &Type, r: &Type) -> Result<Type, TypeError> {
    let is_bit = matches!(
        op,
        BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor | BinOp::Shl | BinOp::Shr
    );
    if is_bit {
        // Bit ops accept any int width; result follows numeric promotion.
        if matches!(l, Type::I32 | Type::I64) && matches!(r, Type::I32 | Type::I64) {
            return Ok(numeric_result(l, r).unwrap());
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
        if result.is_some() {
            return Ok(Type::Bool);
        }
        return Err(TypeError::BadBinary {
            lhs: l.clone(),
            rhs: r.clone(),
            span: ilang_ast::Span::dummy(),
        });
    }
    result.ok_or_else(|| TypeError::BadBinary {
        lhs: l.clone(),
        rhs: r.clone(),
        span: ilang_ast::Span::dummy(),
    })
}
