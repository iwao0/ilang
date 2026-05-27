//! `ObjCBlock<fn(...)>.invoke(args)` — call the block. Receiver
//! lowers to `MirTy::I64` (the block pointer), so we disambiguate
//! on lowered arg shapes alone. New shapes append, matching the
//! `BlockKind` table in `ilang_runtime::objc_blocks`.

use ilang_ast::{Expr, Symbol};

use crate::inst::{FuncRef, Inst, ValueId};
use crate::types::MirTy;

use super::super::{BodyCx, LowerError};

impl<'a> BodyCx<'a> {
    /// `Some(...)` when the call is an ObjCBlock invoke; otherwise
    /// `None` so the caller continues with other instance dispatch.
    pub(super) fn try_lower_objc_block_invoke(
        &mut self,
        ov: ValueId,
        oty: &MirTy,
        method: Symbol,
        args: &[Expr],
    ) -> Result<Option<(ValueId, MirTy)>, LowerError> {
        if !((method.as_str() == "invoke" || method.as_str() == "$objc.invokeIdToId")
            && matches!(oty, MirTy::I64))
        {
            return Ok(None);
        }
        // The type checker only routes ObjCBlock.invoke here (other
        // i64 receivers don't expose `invoke`), so the gate above is
        // safe in practice.
        let mut arg_vs: Vec<ValueId> = Vec::with_capacity(args.len());
        let mut arg_tys: Vec<MirTy> = Vec::with_capacity(args.len());
        for a in args {
            let (v, t) = self.lower_expr(a)?;
            arg_vs.push(v);
            arg_tys.push(t);
        }
        // The mangler tagged i64-returning invokes with a distinct
        // method name so MIR can pick the obj-to-obj runtime invoker
        // (`__ilang_invoke_obj_to_obj_block`, returns i64). The
        // void-returning fast-paths keep the original `invoke` name.
        let returns_id = method.as_str() == "$objc.invokeIdToId";
        let builtin = if returns_id {
            match arg_tys.as_slice() {
                [MirTy::I64] => Some("invoke_obj_to_obj_block"),
                _ => None,
            }
        } else {
            match arg_tys.as_slice() {
                [] => Some("invoke_void_block"),
                [MirTy::I64] => Some("invoke_obj_block"),
                [MirTy::I64, MirTy::I64] => Some("invoke_void_bytes_block"),
                [MirTy::I64, MirTy::I64, MirTy::I64] => Some("invoke_void_three_obj_block"),
                [MirTy::Bool] => Some("invoke_void_bool_block"),
                _ => None,
            }
        };
        if let Some(name) = builtin {
            let mut call_args: Vec<ValueId> = Vec::with_capacity(arg_vs.len() + 1);
            call_args.push(ov);
            call_args.extend(arg_vs);
            if returns_id {
                let dst = self.fb.new_value(MirTy::I64);
                self.fb.push_inst(Inst::Call {
                    dst: Some(dst),
                    callee: FuncRef::Builtin(Symbol::intern(name)),
                    args: call_args.into_boxed_slice(),
                });
                return Ok(Some((dst, MirTy::I64)));
            } else {
                self.fb.push_inst(Inst::Call {
                    dst: None,
                    callee: FuncRef::Builtin(Symbol::intern(name)),
                    args: call_args.into_boxed_slice(),
                });
                return Ok(Some((self.const_unit(), MirTy::Unit)));
            }
        }
        Err(LowerError::Other(format!(
            "ObjCBlock.invoke(...) signature not yet supported: \
             returns_id={returns_id}, args={:?}",
            arg_tys
        )))
    }
}
