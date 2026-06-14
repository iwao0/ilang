//! Body lowering: walk method / fn / `__main` bodies (plus pending
//! closures) into MIR after the registration passes have populated
//! the id tables.

use std::collections::HashMap;

use ilang_ast::{self as ast, Expr, FnDecl, Stmt, Symbol};

use crate::builder::FunctionBuilder;
use crate::inst::{FuncId, FuncRef, Inst, Terminator};
use crate::program::{FuncParam, FunctionKind};
use crate::types::MirTy;

use super::super::collect::{
    collect_cellified_names_block, collect_cellified_names_expr, collect_cellified_names_stmt,
};
use super::super::env::{Binding, Env};
use super::super::{BodyCx, Lower, LowerError, PendingClosure};

impl Lower {

    pub(in crate::lower) fn lower_class_methods(&mut self, cd: &ast::ClassDecl) -> Result<(), LowerError> {
        let class_id = *self.class_ids.get(&cd.name).expect("class registered");
        for m in cd.methods.iter() {
            // Generic-method templates are skipped at registration
            // (see `decl/class.rs`); they have no `method_ids` slot,
            // so don't try to lower their body here either.
            if !m.type_params.is_empty() {
                continue;
            }
            self.lower_method(class_id, cd.name, m)?;
        }
        // Static methods → lower like a free fn (no `this` injection).
        for sm in cd.static_methods.iter() {
            if !sm.type_params.is_empty() {
                continue;
            }
            self.lower_static_method(class_id, cd.name, sm)?;
        }
        // Property getters/setters — lowered like instance methods,
        // but the m.name passed in for the lookup is a synthetic
        // `prop::get` / `prop::set` key (built in declare_class_methods).
        // Static accessors take a different path: their body has no
        // `this`, and the synthesised fn is registered as a top-level
        // static method under a distinct mangled name.
        for prop in cd.properties.iter() {
            if let Some(g) = &prop.getter {
                let mut g2 = g.clone();
                if prop.is_static {
                    g2.name = Symbol::intern(&format!("get_static_{}", prop.name));
                    self.lower_static_method(class_id, cd.name, &g2)?;
                } else {
                    g2.name = Symbol::intern(&format!("{}::get", prop.name));
                    self.lower_method(class_id, cd.name, &g2)?;
                }
            }
            if let Some(s) = &prop.setter {
                let mut s2 = s.clone();
                if prop.is_static {
                    s2.name = Symbol::intern(&format!("set_static_{}", prop.name));
                    self.lower_static_method(class_id, cd.name, &s2)?;
                } else {
                    s2.name = Symbol::intern(&format!("{}::set", prop.name));
                    self.lower_method(class_id, cd.name, &s2)?;
                }
            }
        }
        Ok(())
    }

    pub(in crate::lower) fn lower_pending_closure(&mut self, pc: PendingClosure) -> Result<(), LowerError> {
        let mut fb = FunctionBuilder::new(pc.name, pc.name, pc.ret.clone(), FunctionKind::Local);
        fb.set_span(pc.span);
        let entry = fb.new_block();
        fb.switch_to(entry);

        let mut env = Env::default();
        let mut params_box = Vec::with_capacity(pc.params.len());
        for (pname, pty) in pc.params.iter() {
            let v = fb.add_block_param(entry, pty.clone());
            env.bind(*pname, v, pty.clone());
            params_box.push(FuncParam {
                name: *pname,
                ty: pty.clone(),
                value: v,
            });
        }

        // Build the captures-in-scope map.
        let mut caps_map: HashMap<Symbol, (u32, MirTy)> = HashMap::new();
        let mut cell_caps_set: std::collections::HashSet<Symbol> =
            std::collections::HashSet::new();
        for (i, c) in pc.captures.iter().enumerate() {
            caps_map.insert(c.name, (i as u32, c.ty.clone()));
            if c.is_cell {
                cell_caps_set.insert(c.name);
            }
        }

        // Names cellified inside this closure body too (for nested
        // FnExprs that mutate further captures).
        let mut cellify_inner: std::collections::HashSet<Symbol> =
            std::collections::HashSet::new();
        collect_cellified_names_block(&pc.body, &mut cellify_inner);

        let __cells_empty: std::collections::HashSet<Symbol> = std::collections::HashSet::new();
        let mut bcx = BodyCx {
            fb: &mut fb,
            env: &mut env,
            ret_ty: pc.ret.clone(),
            fn_ids: &mut self.fn_ids,
            fn_sigs: &mut self.fn_sigs,
            loops: Vec::new(),
            this_class: pc.enclosing_this_class,
            classes: &self.classes,
            class_meta: &self.class_meta,
            interface_ids: &self.interface_ids,
            iface_method_slots: &self.iface_method_slots,
            iface_parents: &self.iface_parents,
            iface_method_sigs: &self.iface_method_sigs,
            com_interfaces: &self.com_interfaces,
            com_iface_slots: &self.com_iface_slots,
            enum_ids: &self.enum_ids,
            enum_meta: &self.enum_meta,
            enums: &self.enums,
            statics: &self.statics,
            pending: &mut self.pending_closures,
            funcs: &mut self.funcs,
            anon_counter: &mut self.anon_counter,
            captures_in_scope: Some(&caps_map),
            cell_captures: Some(&cell_caps_set),
            overloads: &self.overloads,
            cellify_set: &cellify_inner,
            repl_slots: &self.repl_slots,
            is_main_body: false,
            binding_self_name: None,
            closure_self: pc.self_ref.clone(),
            crepr_owned_locals: std::collections::HashSet::new(),
            crepr_return_owned: std::collections::HashSet::new(),
            last_block_tail_owned: false,
            last_arg_wrapped: false,
                        live_fresh_scrutinees: Vec::new(),
            return_sweep_base: usize::MAX,
            in_fn_body_top: false,
        };
        let needs_retain = pc
            .body
            .tail
            .as_ref()
            .map(|e| bcx.callee_retain_decision(e))
            .unwrap_or(false);
        let ret_hint = bcx.ret_ty.clone();
        let tail = bcx.lower_block_for_fn_body(&pc.body, Some(&ret_hint))?;
        // Owned-ness of the tail for the wrap-coerce release in
        // finalise_return: a fresh tail owns its +1, and a block
        // tail the alias/borrow retain bumped does too.
        let tail_owned = bcx.last_block_tail_owned
            || pc.body
                .tail
                .as_ref()
                .map(|t| bcx.is_fresh_object_expr(t))
                .unwrap_or(false);
        if needs_retain {
            bcx.emit_callee_retain(&tail);
        }
        bcx.finalise_return(tail, tail_owned)?;

        let env_layout = if pc.captures.is_empty() {
            None
        } else {
            Some(crate::program::EnvLayout {
                captures: pc.captures.clone(),
            })
        };
        let mut func = fb.finish(params_box.into_boxed_slice());
        func.closure_env = env_layout;
        self.funcs[pc.func_id.0 as usize] = func;
        Ok(())
    }

    pub(in crate::lower) fn lower_static_method(
        &mut self,
        class_id: crate::types::ClassId,
        class_name: Symbol,
        m: &FnDecl,
    ) -> Result<(), LowerError> {
        let id = *self
            .class_meta
            .get(&class_id)
            .unwrap()
            .static_method_ids
            .get(&m.name)
            .expect("static method pre-registered");
        let sig = self
            .class_meta
            .get(&class_id)
            .unwrap()
            .static_method_sigs
            .get(&m.name)
            .cloned()
            .expect("static sig pre-registered");

        let mangled = Symbol::intern(&format!("{}.{}", class_name, m.name));
        let mut fb = FunctionBuilder::new(mangled, m.name, sig.ret.clone(), FunctionKind::Local);
        fb.set_span(m.span);
        let entry = fb.new_block();
        fb.switch_to(entry);

        let mut env = Env::default();
        let mut params_box = Vec::with_capacity(sig.params.len());
        for (param, pty) in m.params.iter().zip(sig.params.iter()) {
            let v = fb.add_block_param(entry, pty.clone());
            env.bind(param.name, v, pty.clone());
            params_box.push(FuncParam {
                name: param.name,
                ty: pty.clone(),
                value: v,
            });
        }

        let mut __cellify: std::collections::HashSet<Symbol> = std::collections::HashSet::new();
        collect_cellified_names_block(&m.body, &mut __cellify);
        let mut bcx = BodyCx {
            fb: &mut fb,
            env: &mut env,
            ret_ty: sig.ret.clone(),
            fn_ids: &mut self.fn_ids,
            fn_sigs: &mut self.fn_sigs,
            loops: Vec::new(),
            this_class: None, // static — no `this`
            classes: &self.classes,
            class_meta: &self.class_meta,
            interface_ids: &self.interface_ids,
            iface_method_slots: &self.iface_method_slots,
            iface_parents: &self.iface_parents,
            iface_method_sigs: &self.iface_method_sigs,
            com_interfaces: &self.com_interfaces,
            com_iface_slots: &self.com_iface_slots,
            enum_ids: &self.enum_ids,
            enum_meta: &self.enum_meta,
            enums: &self.enums,
            statics: &self.statics,
            pending: &mut self.pending_closures,
            funcs: &mut self.funcs,
            anon_counter: &mut self.anon_counter,
            captures_in_scope: None,
            cell_captures: None,
            cellify_set: &__cellify,
            overloads: &self.overloads,
            repl_slots: &self.repl_slots,
            is_main_body: false,
            binding_self_name: None,
            closure_self: None,
            crepr_owned_locals: std::collections::HashSet::new(),
            crepr_return_owned: std::collections::HashSet::new(),
            last_block_tail_owned: false,
            last_arg_wrapped: false,
                        live_fresh_scrutinees: Vec::new(),
            return_sweep_base: usize::MAX,
            in_fn_body_top: false,
        };
        let needs_retain = m
            .body
            .tail
            .as_ref()
            .map(|e| bcx.callee_retain_decision(e))
            .unwrap_or(false);
        let ret_hint = bcx.ret_ty.clone();
        let tail = bcx.lower_block_for_fn_body(&m.body, Some(&ret_hint))?;
        // Owned-ness of the tail for the wrap-coerce release in
        // finalise_return: a fresh tail owns its +1, and a block
        // tail the alias/borrow retain bumped does too.
        let tail_owned = bcx.last_block_tail_owned
            || m.body
                .tail
                .as_ref()
                .map(|t| bcx.is_fresh_object_expr(t))
                .unwrap_or(false);
        if needs_retain {
            bcx.emit_callee_retain(&tail);
        }
        bcx.finalise_return(tail, tail_owned)?;

        let func = fb.finish(params_box.into_boxed_slice());
        self.funcs[id.0 as usize] = func;
        Ok(())
    }

    pub(in crate::lower) fn lower_method(
        &mut self,
        class_id: crate::types::ClassId,
        _class_name: Symbol,
        m: &FnDecl,
    ) -> Result<(), LowerError> {
        let id = *self
            .class_meta
            .get(&class_id)
            .unwrap()
            .method_ids
            .get(&m.name)
            .expect("method pre-registered");
        let sig = self
            .class_meta
            .get(&class_id)
            .unwrap()
            .method_sigs
            .get(&m.name)
            .cloned()
            .expect("method sig pre-registered");

        // Use the FuncId's pre-registered name so property getters /
        // setters keep their unique `Class.get_<prop>` identity (the
        // `m.name` we got is the synthetic `<prop>::get` key).
        let mangled = self.funcs[id.0 as usize].name;
        let kind = self.funcs[id.0 as usize].kind.clone();
        let mut fb = FunctionBuilder::new(mangled, m.name, sig.ret.clone(), kind);
        fb.set_span(m.span);
        let entry = fb.new_block();
        fb.switch_to(entry);

        // First param is `this`.
        let this_v = fb.add_block_param(entry, sig.params[0].clone());
        let mut params_box = vec![FuncParam {
            name: Symbol::intern("this"),
            ty: sig.params[0].clone(),
            value: this_v,
        }];

        let mut env = Env::default();
        env.bind(Symbol::intern("this"), this_v, sig.params[0].clone());

        for (param, pty) in m.params.iter().zip(sig.params.iter().skip(1)) {
            let v = fb.add_block_param(entry, pty.clone());
            env.bind(param.name, v, pty.clone());
            params_box.push(FuncParam {
                name: param.name,
                ty: pty.clone(),
                value: v,
            });
        }

        let mut __cellify: std::collections::HashSet<Symbol> = std::collections::HashSet::new();
        collect_cellified_names_block(&m.body, &mut __cellify);
        let mut bcx = BodyCx {
            fb: &mut fb,
            env: &mut env,
            ret_ty: sig.ret.clone(),
            fn_ids: &mut self.fn_ids,
            fn_sigs: &mut self.fn_sigs,
            loops: Vec::new(),
            this_class: Some(class_id),
            classes: &self.classes,
            class_meta: &self.class_meta,
            interface_ids: &self.interface_ids,
            iface_method_slots: &self.iface_method_slots,
            iface_parents: &self.iface_parents,
            iface_method_sigs: &self.iface_method_sigs,
            com_interfaces: &self.com_interfaces,
            com_iface_slots: &self.com_iface_slots,
            enum_ids: &self.enum_ids,
            enum_meta: &self.enum_meta,
            enums: &self.enums,
            statics: &self.statics,
            pending: &mut self.pending_closures,
            funcs: &mut self.funcs,
            anon_counter: &mut self.anon_counter,
            captures_in_scope: None,
            cell_captures: None,
            cellify_set: &__cellify,
            overloads: &self.overloads,
            repl_slots: &self.repl_slots,
            is_main_body: false,
            binding_self_name: None,
            closure_self: None,
            crepr_owned_locals: std::collections::HashSet::new(),
            crepr_return_owned: std::collections::HashSet::new(),
            last_block_tail_owned: false,
            last_arg_wrapped: false,
                        live_fresh_scrutinees: Vec::new(),
            return_sweep_base: usize::MAX,
            in_fn_body_top: false,
        };
        let needs_retain = m
            .body
            .tail
            .as_ref()
            .map(|e| bcx.callee_retain_decision(e))
            .unwrap_or(false);
        let ret_hint = bcx.ret_ty.clone();
        let tail = bcx.lower_block_for_fn_body(&m.body, Some(&ret_hint))?;
        // Owned-ness of the tail for the wrap-coerce release in
        // finalise_return: a fresh tail owns its +1, and a block
        // tail the alias/borrow retain bumped does too.
        let tail_owned = bcx.last_block_tail_owned
            || m.body
                .tail
                .as_ref()
                .map(|t| bcx.is_fresh_object_expr(t))
                .unwrap_or(false);
        let is_init = matches!(m.name.as_str(), "init");
        if is_init {
            bcx.fb.set_terminator(Terminator::Return { value: Some(this_v), release_value: false });
        } else {
            if needs_retain {
                bcx.emit_callee_retain(&tail);
            }
            bcx.finalise_return(tail, tail_owned)?;
        }

        let func = fb.finish(params_box.into_boxed_slice());
        self.funcs[id.0 as usize] = func;
        Ok(())
    }

    pub(in crate::lower) fn lower_fn(&mut self, fd: &FnDecl) -> Result<(), LowerError> {
        // `@intrinsic` fns have no user body — `declare_intrinsic_fn`
        // already populated the MIR Function as a sig-only extern.
        // Nothing to lower here.
        if fd.intrinsic_name.is_some() {
            return Ok(());
        }
        // Resolve the right mangled name by matching this FnDecl's
        // param types against the candidates registered for `fd.name`.
        let target_params: Vec<MirTy> = fd
            .params
            .iter()
            .map(|p| self.resolve_ty(&p.ty))
            .collect::<Result<Vec<_>, _>>()?;
        let candidates = self
            .overloads
            .get(&fd.name)
            .cloned()
            .unwrap_or_default();
        let mangled = candidates
            .iter()
            .copied()
            .find(|m| {
                self.fn_sigs
                    .get(m)
                    .map(|s| s.params == target_params)
                    .unwrap_or(false)
            })
            .unwrap_or(fd.name);
        let sig = self
            .fn_sigs
            .get(&mangled)
            .cloned()
            .ok_or_else(|| LowerError::Other(format!("fn {} not pre-registered", fd.name)))?;
        let id = *self.fn_ids.get(&mangled).expect("declared above");

        let mut fb = FunctionBuilder::new(
            mangled,
            fd.name,
            sig.ret.clone(),
            FunctionKind::Local,
        );
        fb.set_span(fd.span);
        let entry = fb.new_block();
        fb.switch_to(entry);

        let mut env = Env::default();
        let mut params_box = Vec::with_capacity(fd.params.len());
        for (param, pty) in fd.params.iter().zip(sig.params.iter()) {
            let v = fb.add_block_param(entry, pty.clone());
            env.bind(param.name, v, pty.clone());
            params_box.push(FuncParam {
                name: param.name,
                ty: pty.clone(),
                value: v,
            });
        }

        let mut __cellify: std::collections::HashSet<Symbol> = std::collections::HashSet::new();
        collect_cellified_names_block(&fd.body, &mut __cellify);
        let mut bcx = BodyCx {
            fb: &mut fb,
            env: &mut env,
            ret_ty: sig.ret.clone(),
            fn_ids: &mut self.fn_ids,
            fn_sigs: &mut self.fn_sigs,
            loops: Vec::new(),
            this_class: None,
            classes: &self.classes,
            class_meta: &self.class_meta,
            interface_ids: &self.interface_ids,
            iface_method_slots: &self.iface_method_slots,
            iface_parents: &self.iface_parents,
            iface_method_sigs: &self.iface_method_sigs,
            com_interfaces: &self.com_interfaces,
            com_iface_slots: &self.com_iface_slots,
            enum_ids: &self.enum_ids,
            enum_meta: &self.enum_meta,
            enums: &self.enums,
            statics: &self.statics,
            pending: &mut self.pending_closures,
            funcs: &mut self.funcs,
            anon_counter: &mut self.anon_counter,
            captures_in_scope: None,
            cell_captures: None,
            cellify_set: &__cellify,
            overloads: &self.overloads,
            repl_slots: &self.repl_slots,
            is_main_body: false,
            binding_self_name: None,
            closure_self: None,
            crepr_owned_locals: std::collections::HashSet::new(),
            crepr_return_owned: std::collections::HashSet::new(),
            last_block_tail_owned: false,
            last_arg_wrapped: false,
                        live_fresh_scrutinees: Vec::new(),
            return_sweep_base: usize::MAX,
            in_fn_body_top: false,
        };
        let needs_retain = fd
            .body
            .tail
            .as_ref()
            .map(|e| bcx.callee_retain_decision(e))
            .unwrap_or(false);
        let ret_hint = bcx.ret_ty.clone();
        let tail = bcx.lower_block_for_fn_body(&fd.body, Some(&ret_hint))?;
        // Owned-ness of the tail for the wrap-coerce release in
        // finalise_return: a fresh tail owns its +1, and a block
        // tail the alias/borrow retain bumped does too.
        let tail_owned = bcx.last_block_tail_owned
            || fd.body
                .tail
                .as_ref()
                .map(|t| bcx.is_fresh_object_expr(t))
                .unwrap_or(false);
        if needs_retain {
            bcx.emit_callee_retain(&tail);
        }
        bcx.finalise_return(tail, tail_owned)?;

        let func = fb.finish(params_box.into_boxed_slice());
        self.funcs[id.0 as usize] = func;
        Ok(())
    }

    pub(in crate::lower) fn lower_main(&mut self, stmts: &[Stmt], tail: Option<&Expr>) -> Result<(), LowerError> {
        let release_slots_at_exit = self.release_slots_at_exit;
        let main_name = Symbol::intern("$main");
        let mut fb = FunctionBuilder::new(main_name, main_name, MirTy::I64, FunctionKind::Local);
        let entry = fb.new_block();
        fb.switch_to(entry);

        let mut env = Env::default();

        // Lower statements then tail.
        let mut __cellify: std::collections::HashSet<Symbol> = std::collections::HashSet::new();
        for s in stmts {
            collect_cellified_names_stmt(s, &mut __cellify);
        }
        if let Some(t) = tail {
            collect_cellified_names_expr(t, &mut __cellify);
        }
        let mut bcx = BodyCx {
            fb: &mut fb,
            env: &mut env,
            ret_ty: MirTy::I64,
            fn_ids: &mut self.fn_ids,
            fn_sigs: &mut self.fn_sigs,
            loops: Vec::new(),
            this_class: None,
            classes: &self.classes,
            class_meta: &self.class_meta,
            interface_ids: &self.interface_ids,
            iface_method_slots: &self.iface_method_slots,
            iface_parents: &self.iface_parents,
            iface_method_sigs: &self.iface_method_sigs,
            com_interfaces: &self.com_interfaces,
            com_iface_slots: &self.com_iface_slots,
            enum_ids: &self.enum_ids,
            enum_meta: &self.enum_meta,
            enums: &self.enums,
            statics: &self.statics,
            pending: &mut self.pending_closures,
            funcs: &mut self.funcs,
            anon_counter: &mut self.anon_counter,
            captures_in_scope: None,
            cell_captures: None,
            cellify_set: &__cellify,
            overloads: &self.overloads,
            repl_slots: &self.repl_slots,
            is_main_body: true,
            binding_self_name: None,
            closure_self: None,
            crepr_owned_locals: std::collections::HashSet::new(),
            crepr_return_owned: std::collections::HashSet::new(),
            last_block_tail_owned: false,
            last_arg_wrapped: false,
                        live_fresh_scrutinees: Vec::new(),
            return_sweep_base: usize::MAX,
            in_fn_body_top: false,
        };
        for stmt in stmts {
            bcx.lower_stmt(stmt)?;
        }
        let tail_val = match tail {
            Some(expr) => Some(bcx.lower_expr(expr)?),
            None => None,
        };

        // Drain the event loop (pending Promise continuations / timers)
        // BEFORE releasing the top-level lets below. A pending
        // continuation can hold a heap object whose `deinit` touches a
        // top-level global (e.g. `deinits[0] = ...`); if the drain runs
        // after the globals are freed — as the external drains in
        // `run_main` / the AOT `main` wrapper do, since they fire after
        // `__main` returns — that deinit dereferences freed memory and
        // the runtime aborts with an out-of-bounds panic. Draining here,
        // while the globals are still alive, fixes the ordering. The
        // external drains remain as a harmless no-op (queue empty).
        bcx.fb.push_inst(Inst::Call {
            dst: None,
            callee: FuncRef::Builtin(Symbol::intern("promise_drain")),
            args: Box::new([]),
        });

        // Release every top-level heap-typed `let` in reverse-bind
        // order so deinit fires before the process exits — matches
        // the existing `release_globals_at_exit` semantics.
        let top_scope: Vec<(Symbol, Binding)> = bcx
            .env
            .scopes
            .first()
            .cloned()
            .unwrap_or_default();
        let needs_release = |ty: &MirTy| ty.is_heap();
        for (_name, binding) in top_scope.into_iter().rev() {
            match binding {
                Binding::Local(lid, ty) if needs_release(&ty) => {
                    // CRepr Locals: only release the ones that own
                    // their backing buffer (mirrors the check in
                    // `release_top_scope_objects`). A `let p =
                    // r.origin` borrow stays alive with its
                    // parent struct.
                    if let MirTy::Object(cid) = &ty {
                        let layout = &bcx.classes[cid.0 as usize];
                        let is_crepr = matches!(
                            layout.repr,
                            crate::program::ClassRepr::CRepr
                                | crate::program::ClassRepr::CPacked
                                | crate::program::ClassRepr::CUnion
                        );
                        if is_crepr && !bcx.crepr_owned_locals.contains(&lid) {
                            continue;
                        }
                    }
                    let v = bcx.fb.new_value(ty.clone());
                    bcx.fb.push_inst(Inst::UseLocal { dst: v, local: lid });
                    bcx.fb.push_inst(Inst::Release { value: v });
                }
                Binding::Ssa(v, ty) if needs_release(&ty) => {
                    bcx.fb.push_inst(Inst::Release { value: v });
                }
                Binding::Cell(cell_v, _) => {
                    // Drop the scope's share of the cell itself (the
                    // value type is the `T[]` cell array, so the
                    // release cascades into the inner) — mirrors
                    // `release_top_scope_objects` / the break sweep.
                    bcx.fb.push_inst(Inst::Release { value: cell_v });
                }
                _ => {}
            }
        }
        // Slot-backed top-level heap lets: balance the retain at
        // store-time with a release at process exit so any class
        // `deinit` fires before main returns. Emitted in
        // descending-slot order to mirror reverse-bind LIFO release.
        // Interactive REPL chunks skip the slot sweep
        // (`release_slots_at_exit` = false): slots must outlive the
        // chunk, and releasing them here handed the next chunk a
        // freed pointer (`let arr = [1,2,3]` then `arr[1]` on the
        // following line read freed memory).
        let mut slot_releases: Vec<(u32, MirTy)> = if release_slots_at_exit {
            bcx.repl_slots
                .iter()
                .filter(|(_, (_, ty))| needs_release(ty))
                .map(|(_, (idx, ty))| (*idx, ty.clone()))
                .collect()
        } else {
            Vec::new()
        };
        slot_releases.sort_by(|a, b| b.0.cmp(&a.0));
        for (idx, ty) in slot_releases {
            let idx_v = bcx.const_int(MirTy::I64, idx as i64);
            let raw = bcx.fb.new_value(MirTy::I64);
            bcx.fb.push_inst(Inst::Call {
                dst: Some(raw),
                callee: FuncRef::Builtin(Symbol::intern("$repl.loadSlot")),
                args: Box::new([idx_v]),
            });
            let v = bcx.i64_to_slot_value(raw, &ty)?;
            bcx.fb.push_inst(Inst::Release { value: v });
        }

        // __main returns `i64` (process exit code). If the program
        // tail is an i64 expression, return that; otherwise return 0.
        let ret_val = match tail_val {
            Some((v, MirTy::I64)) => v,
            _ => bcx.const_int(MirTy::I64, 0),
        };
        bcx.fb.set_terminator(Terminator::Return { value: Some(ret_val), release_value: false });

        let func = fb.finish(Box::new([]));
        let id = FuncId(self.funcs.len() as u32);
        self.funcs.push(func);
        self.main_id = Some(id);
        Ok(())
    }
}
