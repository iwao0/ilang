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
    matches!((from, to), (Type::I64, Type::F64))
}

pub(crate) fn bin_result(op: BinOp, l: &Type, r: &Type) -> Result<Type, TypeError> {
    let is_bit = matches!(
        op,
        BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor | BinOp::Shl | BinOp::Shr
    );
    if is_bit {
        if l == &Type::I64 && r == &Type::I64 {
            return Ok(Type::I64);
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
    let numeric_result = match (l, r) {
        (Type::I64, Type::I64) => Some(Type::I64),
        (Type::F64, Type::F64) => Some(Type::F64),
        (Type::I64, Type::F64) | (Type::F64, Type::I64) => Some(Type::F64),
        _ => None,
    };
    if is_compare {
        // Equality is allowed on bool too; ordering is numeric only.
        if matches!(op, BinOp::Eq | BinOp::Ne) && l == &Type::Bool && r == &Type::Bool {
            return Ok(Type::Bool);
        }
        if numeric_result.is_some() {
            return Ok(Type::Bool);
        }
        return Err(TypeError::BadBinary {
            lhs: l.clone(),
            rhs: r.clone(),
            span: ilang_ast::Span::dummy(),
        });
    }
    numeric_result.ok_or_else(|| TypeError::BadBinary {
            lhs: l.clone(),
            rhs: r.clone(),
            span: ilang_ast::Span::dummy(),
        })
}
