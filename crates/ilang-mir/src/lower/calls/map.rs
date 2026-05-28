//! `Map<K, V>` instance method dispatch.

use ilang_ast::{Expr, Symbol};

use crate::inst::{FuncRef, Inst, ValueId};
use crate::types::MirTy;

use super::super::{BodyCx, LowerError};

impl<'a> BodyCx<'a> {
    pub(super) fn try_lower_map_method(
        &mut self,
        ov: ValueId,
        oty: &MirTy,
        obj_is_fresh: bool,
        method: Symbol,
        args: &[Expr],
    ) -> Result<Option<(ValueId, MirTy)>, LowerError> {
        let MirTy::Map { key, val } = oty else {
            return Ok(None);
        };
        let m = method.as_str();
        let (builtin_name, ret_ty) = match m {
            "get" => (
                "map_get_optional",
                MirTy::Optional(Box::new((**val).clone())),
            ),
            "has" => ("map_has", MirTy::Bool),
            "delete" => ("map_delete", MirTy::Bool),
            "set" => ("map_set", MirTy::Unit),
            "size" => ("map_size", MirTy::I64),
            "keys" => (
                "map_keys",
                MirTy::Array { elem: Box::new((**key).clone()), len: None },
            ),
            "values" => (
                "map_values",
                MirTy::Array { elem: Box::new((**val).clone()), len: None },
            ),
            "clear" => ("map_clear", MirTy::Unit),
            "entries" => (
                "map_entries",
                MirTy::Array {
                    elem: Box::new(MirTy::Tuple(Box::new([
                        (**key).clone(),
                        (**val).clone(),
                    ]))),
                    len: None,
                },
            ),
            "forEach" => ("map_for_each", MirTy::Unit),
            other => {
                return Err(LowerError::Other(format!("unknown map method `{other}`")));
            }
        };
        // Narrow key / value args to the map's declared K / V so a
        // float / int literal adopts the slot width (`m.set(k, 0.5)`
        // against `Map<_, f32>` must store an f32, not an f64 whose
        // low 32 bits read back as 0). `get`/`has`/`delete` take a key;
        // `set` takes a key then a value; other methods take none.
        let arg_types: Vec<Option<&MirTy>> = match m {
            "get" | "has" | "delete" => vec![Some(&**key)],
            "set" => vec![Some(&**key), Some(&**val)],
            _ => Vec::new(),
        };
        let mut arg_vals = vec![ov];
        let mut arg_meta: Vec<(bool, ValueId, MirTy)> = Vec::new();
        for (idx, a) in args.iter().enumerate() {
            let arg_is_fresh = self.is_fresh_object_expr(a);
            let target = arg_types.get(idx).copied().flatten();
            let (v, vty) = self.lower_arg_to(a, target)?;
            // Map host fns are uniformly (i64, i64, i64). Cast
            // smaller / float / bool args to i64 cells.
            let v_ext = if matches!(vty, MirTy::I64 | MirTy::U64)
                || vty.is_heap()
                || vty.is_float()
            {
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
            arg_meta.push((arg_is_fresh, v_ext, vty));
        }
        // `forEach`'s callback is `fn(key, value, env)`; float key /
        // value params travel in float registers, so pass their
        // float-kind tags (0=int/ptr, 1=f32, 2=f64) for the runtime to
        // call the closure through a matching ABI.
        if m == "forEach" {
            arg_vals.push(self.const_int(MirTy::I64, key.float_kind()));
            arg_vals.push(self.const_int(MirTy::I64, val.float_kind()));
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
        // `m.set` takes its own +1 share via host_map_set's
        // retain_by_kind. Mirror the AssignIndex path — for a fresh
        // value the caller's transient +1 is released here so the
        // only remaining share is the map's.
        if m == "set" {
            if let Some((is_fresh, vv, vty)) = arg_meta.get(1) {
                if *is_fresh && self.is_arc_heap(vty) {
                    self.fb.push_inst(Inst::Release { value: *vv });
                }
            }
        }
        // Fresh map receiver, non-Object result: release the map
        // after the dispatch so its cascade fires.
        if obj_is_fresh
            && !matches!(ret_ty, MirTy::Object(_))
            && m != "get"
            && m != "set"
        {
            self.fb.push_inst(Inst::Release { value: ov });
        }
        Ok(Some((dst.unwrap_or_else(|| self.const_unit()), ret_ty)))
    }
}
