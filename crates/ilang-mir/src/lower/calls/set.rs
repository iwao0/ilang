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
        for a in args {
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
            arg_vals.push(v_ext);
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
        if obj_is_fresh && !matches!(ret_ty, MirTy::Object(_)) {
            self.fb.push_inst(Inst::Release { value: ov });
        }
        Ok(Some((dst.unwrap_or_else(|| self.const_unit()), ret_ty)))
    }
}
