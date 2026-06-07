//! `Set<T>` instance method dispatch. Float receivers go through
//! dedicated `$set.*F{32,64}` entry points so cranelift's
//! float-register ABI delivers the raw value.

use ilang_ast::{Expr, Symbol};

use crate::inst::{FuncRef, Inst, ValueId};
use crate::types::MirTy;

use super::super::{BodyCx, LowerError};

impl<'a> BodyCx<'a> {
    pub(super) fn try_lower_set_method(
        &mut self,
        ov: ValueId,
        oty: &MirTy,
        obj_is_fresh: bool,
        method: Symbol,
        args: &[Expr],
    ) -> Result<Option<(ValueId, MirTy)>, LowerError> {
        let MirTy::Set { elem } = oty else {
            return Ok(None);
        };
        let elem_is_f32 = matches!(**elem, MirTy::F32);
        let elem_is_f64 = matches!(**elem, MirTy::F64);
        let elem_is_float = elem_is_f32 || elem_is_f64;
        let set_ty = MirTy::Set { elem: elem.clone() };
        let arr_ty = MirTy::Array { elem: elem.clone(), len: None };
        let m = method.as_str();
        let (builtin_name, ret_ty) = match m {
            "add" if elem_is_f32 => ("set_add_f32", MirTy::Unit),
            "add" if elem_is_f64 => ("set_add_f64", MirTy::Unit),
            "add" => ("set_add", MirTy::Unit),
            "has" if elem_is_f32 => ("set_has_f32", MirTy::Bool),
            "has" if elem_is_f64 => ("set_has_f64", MirTy::Bool),
            "has" => ("set_has", MirTy::Bool),
            "delete" if elem_is_f32 => ("set_delete_f32", MirTy::Bool),
            "delete" if elem_is_f64 => ("set_delete_f64", MirTy::Bool),
            "delete" => ("set_delete", MirTy::Bool),
            "size" => ("set_size", MirTy::I64),
            "clear" => ("set_clear", MirTy::Unit),
            "values" => ("set_values", arr_ty),
            "forEach" if elem_is_f32 => ("set_for_each_f32", MirTy::Unit),
            "forEach" if elem_is_f64 => ("set_for_each_f64", MirTy::Unit),
            "forEach" => ("set_for_each", MirTy::Unit),
            "union" => ("set_union", set_ty),
            "intersection" => ("set_intersection", set_ty),
            "difference" => ("set_difference", set_ty),
            "isSubsetOf" => ("set_is_subset_of", MirTy::Bool),
            "isSupersetOf" => ("set_is_superset_of", MirTy::Bool),
            "isDisjointFrom" => ("set_is_disjoint_from", MirTy::Bool),
            other => {
                return Err(LowerError::Other(format!("unknown set method `{other}`")));
            }
        };
        // `add` / `has` / `delete` take an element-typed arg and
        // need the float coerce / i64 widen below. Others take a
        // closure or another set — already in native ABI shape.
        let arg_is_elem = matches!(m, "add" | "has" | "delete");
        let mut arg_vals = vec![ov];
        // Track a fresh heap element transient passed to `add`; the set
        // adopts its own +1 (host_set_add retains string elements), so
        // the caller's transient is released after the call.
        let mut fresh_elem: Option<(ValueId, MirTy)> = None;
        // forEach's callback closure: the runtime consumes the +1.
        // Track whether to retain a borrowed reference.
        let mut foreach_cb: Option<(ValueId, bool)> = None;
        for (idx, a) in args.iter().enumerate() {
            let arg_is_fresh = self.is_fresh_object_expr(a);
            let (v, vty) = self.lower_expr(a)?;
            let v_ext = if arg_is_elem && elem_is_float {
                if &vty == &**elem {
                    v
                } else {
                    self.coerce(v, &vty, elem, a.span)?
                }
            } else if !arg_is_elem {
                v
            } else if matches!(vty, MirTy::I64 | MirTy::U64) || vty.is_heap() {
                v
            } else if vty.is_int() || matches!(vty, MirTy::Bool) {
                let dst_v = self.fb.new_value(MirTy::I64);
                self.fb.push_inst(Inst::Cast {
                    dst: dst_v,
                    kind: crate::inst::CastKind::IntResize,
                    src: v,
                });
                dst_v
            } else {
                v
            };
            if m == "add" && idx == 0 && arg_is_fresh && self.is_arc_heap(&vty) {
                fresh_elem = Some((v_ext, vty.clone()));
            }
            if m == "forEach" && idx == 0 {
                foreach_cb = Some((v_ext, arg_is_fresh));
            }
            arg_vals.push(v_ext);
        }
        // Runtime takes ownership of the forEach callback's +1 — retain
        // when the caller passes a borrowed reference.
        if let Some((cb_v, is_fresh)) = foreach_cb {
            if !is_fresh {
                self.fb.push_inst(Inst::Retain { value: cb_v });
            }
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
        if let Some((ev, _)) = fresh_elem {
            self.fb.push_inst(Inst::Release { value: ev });
        }
        if obj_is_fresh && !matches!(ret_ty, MirTy::Object(_)) {
            self.fb.push_inst(Inst::Release { value: ov });
        }
        Ok(Some((dst.unwrap_or_else(|| self.const_unit()), ret_ty)))
    }
}
