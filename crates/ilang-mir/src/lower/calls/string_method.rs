//! `string` instance method dispatch (charAt / includes / split /
//! replace / slice / indexOf / encodeUtf16 / ...).

use ilang_ast::{Expr, Symbol};

use crate::inst::{FuncRef, Inst, ValueId};
use crate::types::MirTy;

use super::super::{BodyCx, LowerError};

impl<'a> BodyCx<'a> {
    pub(super) fn try_lower_string_method(
        &mut self,
        ov: ValueId,
        oty: &MirTy,
        method: Symbol,
        args: &[Expr],
    ) -> Result<Option<(ValueId, MirTy)>, LowerError> {
        if !matches!(oty, MirTy::Str) {
            return Ok(None);
        }
        let m = method.as_str();
        let (builtin_name, ret_ty) = match m {
            "charAt" => ("str_char_at", MirTy::Str),
            "includes" => ("str_includes", MirTy::Bool),
            "startsWith" => ("str_starts_with", MirTy::Bool),
            "endsWith" => ("str_ends_with", MirTy::Bool),
            "toUpper" => ("str_to_upper", MirTy::Str),
            "toLower" => ("str_to_lower", MirTy::Str),
            "trim" => ("str_trim", MirTy::Str),
            "split" => (
                "str_split",
                MirTy::Array { elem: Box::new(MirTy::Str), len: None },
            ),
            "replace" => ("str_replace", MirTy::Str),
            "slice" => ("str_slice", MirTy::Str),
            "concat" => ("str_concat", MirTy::Str),
            "indexOf" => ("str_index_of", MirTy::I64),
            "lastIndexOf" => ("str_last_index_of", MirTy::I64),
            "encodeUtf16" => (
                "str_encode_utf16",
                MirTy::Array { elem: Box::new(MirTy::U16), len: None },
            ),
            "hashCode" => ("str_hash_code", MirTy::I64),
            other => {
                return Err(LowerError::Other(format!(
                    "unknown string method `{other}`"
                )));
            }
        };
        let mut arg_vals = vec![ov];
        for a in args {
            let (v, _) = self.lower_expr(a)?;
            arg_vals.push(v);
        }
        // Pad the optional `fromIndex` of indexOf/lastIndexOf with
        // the i64::MIN "omitted" sentinel the runtime recognises.
        if matches!(m, "indexOf" | "lastIndexOf") && arg_vals.len() == 2 {
            let pad = self.const_int(MirTy::I64, i64::MIN);
            arg_vals.push(pad);
        }
        // `encodeUtf16` defaults to NUL-terminated. When the user
        // omits the bool arg, inject `true` (1).
        if m == "encodeUtf16" && arg_vals.len() == 1 {
            let pad = self.const_int(MirTy::I64, 1);
            arg_vals.push(pad);
        }
        let dst = if matches!(ret_ty, MirTy::Unit) {
            None
        } else {
            Some(self.fb.new_value(ret_ty.clone()))
        };
        self.fb.push_inst(Inst::Call {
            dst,
            callee: FuncRef::Builtin(Symbol::intern(builtin_name)),
            args: arg_vals.into_boxed_slice(),
        });
        Ok(Some((dst.unwrap_or_else(|| self.const_unit()), ret_ty)))
    }
}
