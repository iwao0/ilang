//! `.toString()`, `.isFinite()`, `.isNaN()` on primitive
//! (numeric / bool / string) receivers.

use ilang_ast::{Expr, Symbol};

use crate::inst::{FuncRef, Inst, ValueId};
use crate::types::MirTy;

use super::super::{BodyCx, LowerError};

impl<'a> BodyCx<'a> {
    pub(super) fn try_lower_scalar_method(
        &mut self,
        ov: ValueId,
        oty: &MirTy,
        method: Symbol,
        args: &[Expr],
    ) -> Result<Option<(ValueId, MirTy)>, LowerError> {
        // `.toString()` is available on every numeric / bool / string.
        if method.as_str() == "toString" && args.is_empty() {
            if oty.is_int() || oty.is_float() || matches!(oty, MirTy::Bool | MirTy::Str) {
                let v = self.fb.new_value(MirTy::Str);
                // Per-width split for floats: cranelift's float-arg
                // ABI distinguishes f32 from f64, so the codegen
                // needs a separate FuncId per width.
                let builtin = match oty {
                    MirTy::Bool => "bool_to_string",
                    MirTy::Str => "str_to_string",
                    MirTy::F32 => "float_to_string_f32",
                    MirTy::F64 => "float_to_string_f64",
                    _ => "int_to_string",
                };
                self.fb.push_inst(Inst::Call {
                    dst: Some(v),
                    callee: FuncRef::Builtin(Symbol::intern(builtin)),
                    args: Box::new([ov]),
                });
                return Ok(Some((v, MirTy::Str)));
            }
        }
        // `.isFinite()` / `.isNaN()` on f32 / f64. Per-width entry
        // points because cranelift's float-arg ABI distinguishes
        // the two; result is i64 (0/1) reduced to language-level
        // Bool (i8).
        if args.is_empty()
            && matches!(method.as_str(), "isFinite" | "isNaN")
            && oty.is_float()
        {
            let builtin = match (oty, method.as_str()) {
                (MirTy::F32, "isFinite") => "math_is_finite_f32",
                (MirTy::F64, "isFinite") => "math_is_finite_f64",
                (MirTy::F32, "isNaN") => "math_is_nan_f32",
                (MirTy::F64, "isNaN") => "math_is_nan_f64",
                _ => unreachable!(),
            };
            let raw = self.fb.new_value(MirTy::I64);
            self.fb.push_inst(Inst::Call {
                dst: Some(raw),
                callee: FuncRef::Builtin(Symbol::intern(builtin)),
                args: Box::new([ov]),
            });
            let b = self.fb.new_value(MirTy::Bool);
            self.fb.push_inst(Inst::Cast {
                dst: b,
                kind: crate::inst::CastKind::IntResize,
                src: raw,
            });
            return Ok(Some((b, MirTy::Bool)));
        }
        Ok(None)
    }
}
