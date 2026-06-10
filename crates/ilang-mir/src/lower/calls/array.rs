//! `Array<T>` instance method dispatch + `Optional<T>.unwrap`.

use ilang_ast::{Expr, Symbol};

use crate::inst::{FuncRef, Inst, ValueId};
use crate::types::MirTy;

use super::super::utils::retain_if_heap;
use super::super::{BodyCx, LowerError};
use super::kind_tag_of_mir;

impl<'a> BodyCx<'a> {
    pub(super) fn try_lower_optional_method(
        &mut self,
        ov: ValueId,
        oty: &MirTy,
        method: Symbol,
        args: &[Expr],
    ) -> Result<Option<(ValueId, MirTy)>, LowerError> {
        let MirTy::Optional(inner) = oty else {
            return Ok(None);
        };
        if method.as_str() != "unwrap" {
            return Ok(None);
        }
        if !args.is_empty() {
            return Err(LowerError::Other("Optional.unwrap takes no args".into()));
        }
        let inner = (**inner).clone();
        let v = self.fb.new_value(inner.clone());
        self.fb.push_inst(Inst::OptionalUnwrap { dst: v, opt: ov });
        // The unwrapped value aliases the Optional cell's `value`
        // slot — same heap pointer. Without a retain, the receiver
        // and the Optional cell's eventual cascade-release would
        // both decrement the same rc, double-freeing the inner.
        if matches!(
            inner,
            MirTy::Object(_)
                | MirTy::Array { .. }
                | MirTy::Tuple(_)
                | MirTy::Map { .. }
                | MirTy::Optional(_)
                | MirTy::Fn(_)
                | MirTy::Str
        ) {
            self.fb.push_inst(Inst::Retain { value: v });
        }
        Ok(Some((v, inner)))
    }

    pub(super) fn try_lower_array_method(
        &mut self,
        ov: ValueId,
        oty: &MirTy,
        method: Symbol,
        args: &[Expr],
    ) -> Result<Option<(ValueId, MirTy)>, LowerError> {
        let MirTy::Array { elem, .. } = oty else {
            return Ok(None);
        };
        let elem = elem.clone();
        let m = method.as_str();
        let out = match m {
            "push" => self.lower_array_push(ov, &elem, args)?,
            "pop" => self.lower_array_pop(ov, &elem)?,
            "removeAt" => self.lower_array_remove_at(ov, &elem, args)?,
            "remove" => self.lower_array_remove(ov, &elem, args)?,
            "indexOf" => self.lower_array_index_of(ov, args)?,
            "map" => self.lower_array_map(ov, &elem, args)?,
            "filter" => self.lower_array_filter(ov, &elem, args)?,
            "forEach" => self.lower_array_for_each(ov, &elem, args)?,
            "find" => self.lower_array_find(ov, &elem, args)?,
            "findIndex" => self.lower_array_find_index(ov, &elem, args)?,
            "every" | "some" => self.lower_array_every_some(ov, &elem, m, args)?,
            "concat" => self.lower_array_concat(ov, &elem, args)?,
            "reverse" => self.lower_array_reverse(ov, &elem)?,
            "join" => self.lower_array_join(ov, args)?,
            "shift" => self.lower_array_shift(ov, &elem)?,
            "unshift" => self.lower_array_unshift(ov, &elem, args)?,
            "fill" => self.lower_array_fill(ov, &elem, args)?,
            "sort" => self.lower_array_sort(ov, &elem, args)?,
            "slice" => self.lower_array_slice(ov, &elem, args)?,
            "includes" => self.lower_array_includes(ov, args)?,
            _ => return Ok(None),
        };
        Ok(Some(out))
    }

    fn lower_array_push(
        &mut self,
        ov: ValueId,
        elem: &MirTy,
        args: &[Expr],
    ) -> Result<(ValueId, MirTy), LowerError> {
        if args.len() != 1 {
            return Err(LowerError::Other("Array.push takes 1 arg".into()));
        }
        let value_is_fresh = self.is_fresh_object_expr(&args[0]);
        let (coerced, _) = self.lower_arg_to(&args[0], Some(elem))?;
        // Bump rc on borrowed heap values — `array_push` stores the
        // cell verbatim, but `__release_array`'s cascade will
        // eventually release every stored element.
        if !value_is_fresh {
            retain_if_heap(&mut self.fb, coerced, elem);
        }
        self.fb.push_inst(Inst::Call {
            dst: None,
            callee: FuncRef::Builtin(Symbol::intern("array_push")),
            args: Box::new([ov, coerced]),
        });
        Ok((self.const_unit(), MirTy::Unit))
    }

    fn lower_array_pop(
        &mut self,
        ov: ValueId,
        elem: &MirTy,
    ) -> Result<(ValueId, MirTy), LowerError> {
        let opt_ty = MirTy::Optional(Box::new(elem.clone()));
        let v = self.fb.new_value(opt_ty.clone());
        self.fb.push_inst(Inst::Call {
            dst: Some(v),
            callee: FuncRef::Builtin(Symbol::intern("array_pop")),
            args: Box::new([ov]),
        });
        Ok((v, opt_ty))
    }

    fn lower_array_remove_at(
        &mut self,
        ov: ValueId,
        elem: &MirTy,
        args: &[Expr],
    ) -> Result<(ValueId, MirTy), LowerError> {
        if args.len() != 1 {
            return Err(LowerError::Other("Array.removeAt takes 1 arg".into()));
        }
        let opt_ty = MirTy::Optional(Box::new(elem.clone()));
        let (iv, ity) = self.lower_expr(&args[0])?;
        let iv = if ity == MirTy::I64 {
            iv
        } else {
            self.coerce(iv, &ity, &MirTy::I64, args[0].span)?
        };
        let v = self.fb.new_value(opt_ty.clone());
        self.fb.push_inst(Inst::Call {
            dst: Some(v),
            callee: FuncRef::Builtin(Symbol::intern("array_remove_at")),
            args: Box::new([ov, iv]),
        });
        Ok((v, opt_ty))
    }

    fn lower_array_remove(
        &mut self,
        ov: ValueId,
        elem: &MirTy,
        args: &[Expr],
    ) -> Result<(ValueId, MirTy), LowerError> {
        if args.len() != 1 {
            return Err(LowerError::Other("Array.remove takes 1 arg".into()));
        }
        let needle_is_fresh = self.is_fresh_object_expr(&args[0]);
        let (coerced, vty) = self.lower_arg_to(&args[0], Some(elem))?;
        let v = self.fb.new_value(MirTy::Bool);
        self.fb.push_inst(Inst::Call {
            dst: Some(v),
            callee: FuncRef::Builtin(Symbol::intern("array_remove")),
            args: Box::new([ov, coerced]),
        });
        // host_array_remove releases the *stored* element's share on a
        // hit; the needle itself is only read. A fresh needle's
        // transient +1 drops here (borrowed needles keep their slot's
        // share as usual).
        if needle_is_fresh && self.is_arc_heap(&vty) {
            self.fb.push_inst(Inst::Release { value: coerced });
        }
        Ok((v, MirTy::Bool))
    }

    fn lower_array_index_of(
        &mut self,
        ov: ValueId,
        args: &[Expr],
    ) -> Result<(ValueId, MirTy), LowerError> {
        if args.len() != 1 {
            return Err(LowerError::Other("Array.indexOf takes 1 arg".into()));
        }
        let needle_is_fresh = self.is_fresh_object_expr(&args[0]);
        let (av, vty) = self.lower_expr(&args[0])?;
        let v = self.fb.new_value(MirTy::I64);
        self.fb.push_inst(Inst::Call {
            dst: Some(v),
            callee: FuncRef::Builtin(Symbol::intern("array_index_of")),
            args: Box::new([ov, av]),
        });
        // The runtime only reads the needle — drop a fresh transient's
        // +1 (`arr.indexOf("b" + suffix)` leaked one string per call).
        if needle_is_fresh && self.is_arc_heap(&vty) {
            self.fb.push_inst(Inst::Release { value: av });
        }
        Ok((v, MirTy::I64))
    }

    fn lower_array_map(
        &mut self,
        ov: ValueId,
        elem: &MirTy,
        args: &[Expr],
    ) -> Result<(ValueId, MirTy), LowerError> {
        if args.len() != 1 {
            return Err(LowerError::Other("Array.map takes 1 arg".into()));
        }
        let cb_is_fresh = self.is_fresh_object_expr(&args[0]);
        let (fv, fty) = self.lower_expr(&args[0])?;
        // Result element type is the closure's return type.
        let ret_ty = if let MirTy::Fn(ft) = &fty {
            ft.ret.clone()
        } else {
            elem.clone()
        };
        let arr_ty = MirTy::Array { elem: Box::new(ret_ty.clone()), len: None };
        let kind = kind_tag_of_mir(&ret_ty, self.classes);
        let kind_v = self.const_int(MirTy::I64, kind);
        // The result element type can differ from the input's, so the
        // runtime can't infer the output cell width — pass it through.
        let stride_v = self.const_int(MirTy::I64, ret_ty.elem_byte_stride());
        // Float-kind tags so the runtime calls the closure through an
        // ABI matching its (input) parameter and (output) return type.
        let arg_fk_v = self.const_int(MirTy::I64, elem.float_kind());
        let ret_fk_v = self.const_int(MirTy::I64, ret_ty.float_kind());
        // Runtime takes ownership of the closure's +1 and releases
        // at the end of iteration — retain when the caller passes a
        // borrowed reference so the original binding stays valid.
        if !cb_is_fresh {
            self.fb.push_inst(Inst::Retain { value: fv });
        }
        let v = self.fb.new_value(arr_ty.clone());
        self.fb.push_inst(Inst::Call {
            dst: Some(v),
            callee: FuncRef::Builtin(Symbol::intern("array_map")),
            args: Box::new([ov, fv, kind_v, stride_v, arg_fk_v, ret_fk_v]),
        });
        Ok((v, arr_ty))
    }

    fn lower_array_filter(
        &mut self,
        ov: ValueId,
        elem: &MirTy,
        args: &[Expr],
    ) -> Result<(ValueId, MirTy), LowerError> {
        if args.len() != 1 {
            return Err(LowerError::Other("Array.filter takes 1 arg".into()));
        }
        let arr_ty = MirTy::Array { elem: Box::new(elem.clone()), len: None };
        let cb_is_fresh = self.is_fresh_object_expr(&args[0]);
        let (fv, _) = self.lower_expr(&args[0])?;
        let arg_fk_v = self.const_int(MirTy::I64, elem.float_kind());
        if !cb_is_fresh {
            self.fb.push_inst(Inst::Retain { value: fv });
        }
        let v = self.fb.new_value(arr_ty.clone());
        self.fb.push_inst(Inst::Call {
            dst: Some(v),
            callee: FuncRef::Builtin(Symbol::intern("array_filter")),
            args: Box::new([ov, fv, arg_fk_v]),
        });
        Ok((v, arr_ty))
    }

    fn lower_array_for_each(
        &mut self,
        ov: ValueId,
        elem: &MirTy,
        args: &[Expr],
    ) -> Result<(ValueId, MirTy), LowerError> {
        if args.len() != 1 {
            return Err(LowerError::Other("Array.forEach takes 1 arg".into()));
        }
        let cb_is_fresh = self.is_fresh_object_expr(&args[0]);
        let (fv, _) = self.lower_expr(&args[0])?;
        let arg_fk_v = self.const_int(MirTy::I64, elem.float_kind());
        if !cb_is_fresh {
            self.fb.push_inst(Inst::Retain { value: fv });
        }
        self.fb.push_inst(Inst::Call {
            dst: None,
            callee: FuncRef::Builtin(Symbol::intern("array_for_each")),
            args: Box::new([ov, fv, arg_fk_v]),
        });
        Ok((self.const_unit(), MirTy::Unit))
    }

    fn lower_array_find(
        &mut self,
        ov: ValueId,
        elem: &MirTy,
        args: &[Expr],
    ) -> Result<(ValueId, MirTy), LowerError> {
        if args.len() != 1 {
            return Err(LowerError::Other("Array.find takes 1 arg".into()));
        }
        let opt_ty = MirTy::Optional(Box::new(elem.clone()));
        let cb_is_fresh = self.is_fresh_object_expr(&args[0]);
        let (fv, _) = self.lower_expr(&args[0])?;
        let arg_fk_v = self.const_int(MirTy::I64, elem.float_kind());
        if !cb_is_fresh {
            self.fb.push_inst(Inst::Retain { value: fv });
        }
        let v = self.fb.new_value(opt_ty.clone());
        self.fb.push_inst(Inst::Call {
            dst: Some(v),
            callee: FuncRef::Builtin(Symbol::intern("array_find")),
            args: Box::new([ov, fv, arg_fk_v]),
        });
        Ok((v, opt_ty))
    }

    fn lower_array_find_index(
        &mut self,
        ov: ValueId,
        elem: &MirTy,
        args: &[Expr],
    ) -> Result<(ValueId, MirTy), LowerError> {
        if args.len() != 1 {
            return Err(LowerError::Other("Array.findIndex takes 1 arg".into()));
        }
        let cb_is_fresh = self.is_fresh_object_expr(&args[0]);
        let (fv, _) = self.lower_expr(&args[0])?;
        let arg_fk_v = self.const_int(MirTy::I64, elem.float_kind());
        if !cb_is_fresh {
            self.fb.push_inst(Inst::Retain { value: fv });
        }
        let v = self.fb.new_value(MirTy::I64);
        self.fb.push_inst(Inst::Call {
            dst: Some(v),
            callee: FuncRef::Builtin(Symbol::intern("array_find_index")),
            args: Box::new([ov, fv, arg_fk_v]),
        });
        Ok((v, MirTy::I64))
    }

    fn lower_array_every_some(
        &mut self,
        ov: ValueId,
        elem: &MirTy,
        m: &str,
        args: &[Expr],
    ) -> Result<(ValueId, MirTy), LowerError> {
        if args.len() != 1 {
            return Err(LowerError::Other(format!("Array.{m} takes 1 arg")));
        }
        let cb_is_fresh = self.is_fresh_object_expr(&args[0]);
        let (fv, _) = self.lower_expr(&args[0])?;
        let arg_fk_v = self.const_int(MirTy::I64, elem.float_kind());
        if !cb_is_fresh {
            self.fb.push_inst(Inst::Retain { value: fv });
        }
        let builtin = if m == "every" { "array_every" } else { "array_some" };
        let v = self.fb.new_value(MirTy::Bool);
        self.fb.push_inst(Inst::Call {
            dst: Some(v),
            callee: FuncRef::Builtin(Symbol::intern(builtin)),
            args: Box::new([ov, fv, arg_fk_v]),
        });
        Ok((v, MirTy::Bool))
    }

    fn lower_array_concat(
        &mut self,
        ov: ValueId,
        elem: &MirTy,
        args: &[Expr],
    ) -> Result<(ValueId, MirTy), LowerError> {
        if args.len() != 1 {
            return Err(LowerError::Other("Array.concat takes 1 arg".into()));
        }
        let arr_ty = MirTy::Array { elem: Box::new(elem.clone()), len: None };
        let (av, _) = self.lower_arg_to(&args[0], Some(&arr_ty))?;
        let v = self.fb.new_value(arr_ty.clone());
        self.fb.push_inst(Inst::Call {
            dst: Some(v),
            callee: FuncRef::Builtin(Symbol::intern("array_concat")),
            args: Box::new([ov, av]),
        });
        Ok((v, arr_ty))
    }

    fn lower_array_reverse(
        &mut self,
        ov: ValueId,
        elem: &MirTy,
    ) -> Result<(ValueId, MirTy), LowerError> {
        let arr_ty = MirTy::Array { elem: Box::new(elem.clone()), len: None };
        let v = self.fb.new_value(arr_ty.clone());
        self.fb.push_inst(Inst::Call {
            dst: Some(v),
            callee: FuncRef::Builtin(Symbol::intern("array_reverse")),
            args: Box::new([ov]),
        });
        Ok((v, arr_ty))
    }

    fn lower_array_join(
        &mut self,
        ov: ValueId,
        args: &[Expr],
    ) -> Result<(ValueId, MirTy), LowerError> {
        if args.len() != 1 {
            return Err(LowerError::Other("Array.join takes 1 arg".into()));
        }
        let (sv, _) = self.lower_expr(&args[0])?;
        let v = self.fb.new_value(MirTy::Str);
        self.fb.push_inst(Inst::Call {
            dst: Some(v),
            callee: FuncRef::Builtin(Symbol::intern("array_join")),
            args: Box::new([ov, sv]),
        });
        Ok((v, MirTy::Str))
    }

    fn lower_array_shift(
        &mut self,
        ov: ValueId,
        elem: &MirTy,
    ) -> Result<(ValueId, MirTy), LowerError> {
        let opt_ty = MirTy::Optional(Box::new(elem.clone()));
        let v = self.fb.new_value(opt_ty.clone());
        self.fb.push_inst(Inst::Call {
            dst: Some(v),
            callee: FuncRef::Builtin(Symbol::intern("array_shift")),
            args: Box::new([ov]),
        });
        Ok((v, opt_ty))
    }

    fn lower_array_unshift(
        &mut self,
        ov: ValueId,
        elem: &MirTy,
        args: &[Expr],
    ) -> Result<(ValueId, MirTy), LowerError> {
        if args.len() != 1 {
            return Err(LowerError::Other("Array.unshift takes 1 arg".into()));
        }
        let value_is_fresh = self.is_fresh_object_expr(&args[0]);
        let (coerced, _) = self.lower_arg_to(&args[0], Some(elem))?;
        if !value_is_fresh {
            retain_if_heap(&mut self.fb, coerced, elem);
        }
        self.fb.push_inst(Inst::Call {
            dst: None,
            callee: FuncRef::Builtin(Symbol::intern("array_unshift")),
            args: Box::new([ov, coerced]),
        });
        Ok((self.const_unit(), MirTy::Unit))
    }

    fn lower_array_fill(
        &mut self,
        ov: ValueId,
        elem: &MirTy,
        args: &[Expr],
    ) -> Result<(ValueId, MirTy), LowerError> {
        if args.len() != 1 {
            return Err(LowerError::Other("Array.fill takes 1 arg".into()));
        }
        let (coerced, _) = self.lower_arg_to(&args[0], Some(elem))?;
        self.fb.push_inst(Inst::Call {
            dst: None,
            callee: FuncRef::Builtin(Symbol::intern("array_fill")),
            args: Box::new([ov, coerced]),
        });
        Ok((self.const_unit(), MirTy::Unit))
    }

    fn lower_array_sort(
        &mut self,
        ov: ValueId,
        elem: &MirTy,
        args: &[Expr],
    ) -> Result<(ValueId, MirTy), LowerError> {
        if args.len() != 1 {
            return Err(LowerError::Other("Array.sort takes 1 arg".into()));
        }
        let arr_ty = MirTy::Array { elem: Box::new(elem.clone()), len: None };
        let cb_is_fresh = self.is_fresh_object_expr(&args[0]);
        let (fv, _) = self.lower_expr(&args[0])?;
        let arg_fk_v = self.const_int(MirTy::I64, elem.float_kind());
        if !cb_is_fresh {
            self.fb.push_inst(Inst::Retain { value: fv });
        }
        let v = self.fb.new_value(arr_ty.clone());
        self.fb.push_inst(Inst::Call {
            dst: Some(v),
            callee: FuncRef::Builtin(Symbol::intern("array_sort")),
            args: Box::new([ov, fv, arg_fk_v]),
        });
        Ok((v, arr_ty))
    }

    fn lower_array_slice(
        &mut self,
        ov: ValueId,
        elem: &MirTy,
        args: &[Expr],
    ) -> Result<(ValueId, MirTy), LowerError> {
        let arr_ty = MirTy::Array { elem: Box::new(elem.clone()), len: None };
        let mut arg_vals = vec![ov];
        for a in args {
            let (v, _) = self.lower_expr(a)?;
            arg_vals.push(v);
        }
        let v = self.fb.new_value(arr_ty.clone());
        self.fb.push_inst(Inst::Call {
            dst: Some(v),
            callee: FuncRef::Builtin(Symbol::intern("array_slice")),
            args: arg_vals.into_boxed_slice(),
        });
        Ok((v, arr_ty))
    }

    fn lower_array_includes(
        &mut self,
        ov: ValueId,
        args: &[Expr],
    ) -> Result<(ValueId, MirTy), LowerError> {
        if args.len() != 1 {
            return Err(LowerError::Other("Array.includes takes 1 arg".into()));
        }
        let needle_is_fresh = self.is_fresh_object_expr(&args[0]);
        let (av, vty) = self.lower_expr(&args[0])?;
        let v = self.fb.new_value(MirTy::Bool);
        self.fb.push_inst(Inst::Call {
            dst: Some(v),
            callee: FuncRef::Builtin(Symbol::intern("array_includes")),
            args: Box::new([ov, av]),
        });
        // Read-only needle — see lower_array_index_of.
        if needle_is_fresh && self.is_arc_heap(&vty) {
            self.fb.push_inst(Inst::Release { value: av });
        }
        Ok((v, MirTy::Bool))
    }
}
