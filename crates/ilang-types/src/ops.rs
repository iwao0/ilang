use ilang_ast::{BinOp, Type};

use crate::error::TypeError;

/// `from` can be assigned to a binding of type `to`. Numeric widening from
/// `i64` to `f64` is allowed (matches the runtime's promotion rule).
pub(crate) fn assignable(from: Type, to: Type) -> bool {
    if from == to {
        return true;
    }
    matches!((from, to), (Type::I64, Type::F64))
}

pub(crate) fn bin_result(op: BinOp, l: Type, r: Type) -> Result<Type, TypeError> {
    let is_compare = matches!(
        op,
        BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge
    );
    let numeric_result = match (l, r) {
        (Type::I64, Type::I64) => Some(Type::I64),
        (Type::F64, Type::F64) => Some(Type::F64),
        (Type::I64, Type::F64) | (Type::F64, Type::I64) => Some(Type::F64),
        _ => None,
    };
    if is_compare {
        // Equality is allowed on bool too; ordering is numeric only.
        if matches!(op, BinOp::Eq | BinOp::Ne) && l == Type::Bool && r == Type::Bool {
            return Ok(Type::Bool);
        }
        if numeric_result.is_some() {
            return Ok(Type::Bool);
        }
        return Err(TypeError::BadBinary(l, r));
    }
    numeric_result.ok_or(TypeError::BadBinary(l, r))
}
