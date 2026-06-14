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
        // `.hashCode(): i64` on every numeric / bool. Integer /
        // bool receivers widen with the source's signedness via
        // `IntResize` (uextend for u*, sextend for i*); float
        // receivers route through `$math.hashCode_f{32,64}` so the
        // bit pattern survives intact (the bare `as i64` cast would
        // truncate the value rather than reinterpret it).
        if args.is_empty() && method.as_str() == "hashCode" {
            if matches!(oty, MirTy::F32 | MirTy::F64) {
                let builtin = if matches!(oty, MirTy::F32) {
                    "math_hash_code_f32"
                } else {
                    "math_hash_code_f64"
                };
                let v = self.fb.new_value(MirTy::I64);
                self.fb.push_inst(Inst::Call {
                    dst: Some(v),
                    callee: FuncRef::Builtin(Symbol::intern(builtin)),
                    args: Box::new([ov]),
                });
                return Ok(Some((v, MirTy::I64)));
            }
            if oty.is_int() || matches!(oty, MirTy::Bool) {
                let v = self.fb.new_value(MirTy::I64);
                self.fb.push_inst(Inst::Cast {
                    dst: v,
                    kind: crate::inst::CastKind::IntResize,
                    src: ov,
                });
                return Ok(Some((v, MirTy::I64)));
            }
        }
        // `.hashCode(): i64` and `.equals(other): bool` on enum values —
        // structural over discriminant + payload, routed to the runtime
        // helpers (the same `@derive(Eq, Hash)` protocol used for nested
        // class fields, so an enum field works too). The receiver is
        // borrowed; the dispatcher releases a fresh receiver, and a fresh
        // `equals` argument is released here.
        if let MirTy::Enum(_) = oty {
            if args.is_empty() && method.as_str() == "hashCode" {
                let v = self.fb.new_value(MirTy::I64);
                self.fb.push_inst(Inst::Call {
                    dst: Some(v),
                    callee: FuncRef::Builtin(Symbol::intern("enum_structural_hash")),
                    args: Box::new([ov]),
                });
                return Ok(Some((v, MirTy::I64)));
            }
            if args.len() == 1 && method.as_str() == "equals" {
                let arg_is_fresh = self.is_fresh_object_expr(&args[0]);
                let (other, _) = self.lower_expr(&args[0])?;
                let v = self.fb.new_value(MirTy::Bool);
                self.fb.push_inst(Inst::Call {
                    dst: Some(v),
                    callee: FuncRef::Builtin(Symbol::intern("enum_structural_eq")),
                    args: Box::new([ov, other]),
                });
                if arg_is_fresh {
                    self.fb.push_inst(Inst::Release { value: other });
                }
                return Ok(Some((v, MirTy::Bool)));
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
