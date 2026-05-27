//! `Promise<T>` instance methods — `then` and `catch`.

use ilang_ast::{Expr, Symbol};

use crate::inst::{FuncRef, Inst, ValueId};
use crate::types::MirTy;

use super::super::{BodyCx, LowerError};
use super::kind_tag_of_mir;

impl<'a> BodyCx<'a> {
    pub(super) fn try_lower_promise_method(
        &mut self,
        ov: ValueId,
        oty: &MirTy,
        method: Symbol,
        args: &[Expr],
    ) -> Result<Option<(ValueId, MirTy)>, LowerError> {
        let MirTy::Promise(inner) = oty else {
            return Ok(None);
        };
        let m = method.as_str();
        if m != "then" && m != "catch" {
            return Ok(None);
        }
        if args.len() != 1 {
            return Err(LowerError::Other(format!(
                "Promise.{m} takes 1 callback arg"
            )));
        }
        // Lower the callback closure; from its fn-ty we figure out
        // the downstream Promise's element type (then's
        // `cb: fn(T): U` ⇒ Promise<U>; catch's `cb: fn(string): T`
        // ⇒ Promise<T>).
        let cb_is_fresh = self.is_fresh_object_expr(&args[0]);
        let (cb_v, cb_ty) = self.lower_expr(&args[0])?;
        let out_inner = match (&cb_ty, m) {
            (MirTy::Fn(ft), _) => ft.ret.clone(),
            (_, "catch") => (**inner).clone(),
            _ => MirTy::Unit,
        };
        let out_kind = kind_tag_of_mir(&out_inner, self.classes);
        let out_kind_v = self.const_int(MirTy::I64, out_kind);
        // Runtime takes ownership of the callback's +1.
        if !cb_is_fresh {
            self.fb.push_inst(Inst::Retain { value: cb_v });
        }
        let result_ty = MirTy::Promise(Box::new(out_inner));
        let dst = self.fb.new_value(result_ty.clone());
        let builtin = if m == "then" { "promise_then" } else { "promise_catch" };
        self.fb.push_inst(Inst::Call {
            dst: Some(dst),
            callee: FuncRef::Builtin(Symbol::intern(builtin)),
            args: Box::new([ov, cb_v, out_kind_v]),
        });
        Ok(Some((dst, result_ty)))
    }
}
