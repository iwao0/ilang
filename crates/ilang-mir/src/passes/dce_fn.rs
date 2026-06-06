//! Function- and class-level dead-code elimination.
//!
//! Reachability sweep from `Program::entry` plus a few non-static roots
//! (`$objc.imp.*` IMP functions registered by name with the runtime,
//! `FunctionKind::ExternBody` callable from C). Any function not
//! reachable from those roots is removed from `Program::functions` and
//! every surviving `FuncId` is remapped to its new index.
//!
//! A reachable class (touched via `NewObject`, `IsInstance`,
//! `DowncastOrNone`, `AddrOfField`, an `Object(c)` / `Weak(c)` type in
//! a reachable function's signature / value-tys / closure env / field
//! type) drags its `drop_fn`, every `MethodDecl.func`, and every
//! `VTable.slot` into the live set. This over-approximates virtual
//! dispatch and ARC drop chains without needing a full class hierarchy
//! analysis.
//!
//! Once the closure converges the pass compacts `Program::functions`
//! AND `Program::classes`, then walks every remaining `FuncId` /
//! `ClassId` reference (function signatures, value types, instructions,
//! class fields, vtables, static slots, `FunctionKind::Init` / `Drop`)
//! and rewrites them to the new indices. `prog.vtables` keeps its
//! indices (live classes' `ClassLayout::vtable` points into it); dead-
//! class vtables stay in place but their `class` field becomes stale —
//! codegen never iterates `prog.vtables` directly, so the stale data
//! is unreferenced.

use std::collections::VecDeque;

use crate::inst::{FuncId, FuncRef, Inst};
use crate::program::{FunctionKind, Program};
use crate::types::{ClassId, MirTy};

#[derive(Debug, Default, Clone, Copy)]
pub struct Stats {
    pub fns_removed: usize,
    pub classes_removed: usize,
}

/// `FuncId(u32::MAX)` is the lowerer's "no function" sentinel — used
/// for classes without a drop chain, NewObject without a user init,
/// etc. Treat it as a no-op everywhere this pass touches a FuncId.
const NO_FUNC: u32 = u32::MAX;

pub fn run_program(prog: &mut Program) -> Stats {
    let n_fn = prog.functions.len();
    let n_cls = prog.classes.len();
    let mut live_fn = vec![false; n_fn];
    let mut live_cls = vec![false; n_cls];
    let mut fn_wq: VecDeque<u32> = VecDeque::new();
    let mut cls_wq: VecDeque<u32> = VecDeque::new();

    // Roots: __main, every `$objc.imp.*` (resolved by runtime name
    // lookup, never via a static call site), and every `@extern(C)
    // fn body` reachable from outside ilang code.
    mark_fn(prog.entry.0, &mut live_fn, &mut fn_wq);
    for (i, f) in prog.functions.iter().enumerate() {
        let sym = f
            .c_symbol
            .as_ref()
            .map(|s| s.as_str())
            .unwrap_or_else(|| f.name.as_str());
        // Roots:
        //   - `$objc.imp.*` IMPs (registered by name with the runtime)
        //   - `@extern(C) fn body` (callable from C)
        //   - every `Extern { sig_only: true }` declaration. These
        //     are pure dlsym/`@lib` imports; codegen also runs a
        //     per-extern `try_open_lib` / `__register_lib_group_*` /
        //     `@optional`-stub pass in `jit_setup` keyed off the
        //     extern's `libs` / `c_symbol` metadata. Dropping an
        //     extern silently skips those side-table updates, which
        //     breaks foundation bindings that arrange the side
        //     tables once per binding block. The compile cost is
        //     negligible (no body to lower), so keep them all live.
        if sym.starts_with("$objc.imp.")
            || matches!(f.kind, FunctionKind::ExternBody)
            || matches!(f.kind, FunctionKind::Extern { .. })
        {
            mark_fn(i as u32, &mut live_fn, &mut fn_wq);
        }
    }
    // Any static slot whose owner class survives via other paths gets
    // pulled in automatically through field-type / method walks. The
    // slot itself stays in `prog.statics` either way — `LoadStatic` /
    // `StoreStatic` carry a `StaticSlotId`, which this pass doesn't
    // remap. Owner-class IDs on dead-class static slots become stale,
    // but only the MIR printer reads `s.owner`.

    loop {
        let mut grew = false;
        while let Some(id) = fn_wq.pop_front() {
            grew = true;
            let f = &prog.functions[id as usize];
            for p in f.params.iter() {
                walk_ty(&p.ty, &mut live_cls, &mut cls_wq);
            }
            walk_ty(&f.ret, &mut live_cls, &mut cls_wq);
            for ty in &f.value_tys {
                walk_ty(ty, &mut live_cls, &mut cls_wq);
            }
            for ty in &f.local_tys {
                walk_ty(ty, &mut live_cls, &mut cls_wq);
            }
            if let Some(env) = &f.closure_env {
                for c in &env.captures {
                    walk_ty(&c.ty, &mut live_cls, &mut cls_wq);
                }
            }
            match f.kind {
                FunctionKind::Trampoline { target } => {
                    mark_fn(target.0, &mut live_fn, &mut fn_wq);
                }
                FunctionKind::Init { class } | FunctionKind::Drop { class } => {
                    mark_cls(class.0, &mut live_cls, &mut cls_wq);
                }
                _ => {}
            }
            for block in &f.blocks {
                for inst in &block.insts {
                    walk_inst(inst, &mut live_fn, &mut fn_wq, &mut live_cls, &mut cls_wq);
                }
            }
        }
        while let Some(cid) = cls_wq.pop_front() {
            grew = true;
            let cls = &prog.classes[cid as usize];
            mark_fn(cls.drop_fn.0, &mut live_fn, &mut fn_wq);
            for m in &cls.methods {
                mark_fn(m.func.0, &mut live_fn, &mut fn_wq);
            }
            if let Some(vt_idx) = cls.vtable {
                if let Some(vt) = prog.vtables.get(vt_idx as usize) {
                    for f in &vt.slots {
                        mark_fn(f.0, &mut live_fn, &mut fn_wq);
                    }
                }
            }
            for fld in &cls.fields {
                walk_ty(&fld.ty, &mut live_cls, &mut cls_wq);
            }
            if let Some(p) = cls.parent {
                mark_cls(p.0, &mut live_cls, &mut cls_wq);
            }
        }
        if !grew {
            break;
        }
    }

    // ----- Build mappings -----
    let mut fn_map: Vec<Option<u32>> = vec![None; n_fn];
    let mut next_fn: u32 = 0;
    for (i, live) in live_fn.iter().enumerate() {
        if *live {
            fn_map[i] = Some(next_fn);
            next_fn += 1;
        }
    }
    let fns_kept = next_fn as usize;
    let fns_removed = n_fn - fns_kept;

    let mut cls_map: Vec<Option<u32>> = vec![None; n_cls];
    let mut next_cls: u32 = 0;
    for (i, live) in live_cls.iter().enumerate() {
        if *live {
            cls_map[i] = Some(next_cls);
            next_cls += 1;
        }
    }
    let cls_kept = next_cls as usize;
    let cls_removed = n_cls - cls_kept;

    if fns_removed == 0 && cls_removed == 0 {
        return Stats::default();
    }

    // ----- Optional dangling-reference report -----
    if std::env::var_os("ILANG_DCE_FN_DEBUG").is_some() {
        report_dangling(prog, &fn_map, &cls_map);
    }

    // ----- Compact functions -----
    if fns_removed > 0 {
        let old_fns = std::mem::take(&mut prog.functions);
        let mut new_fns = Vec::with_capacity(fns_kept);
        for (i, f) in old_fns.into_iter().enumerate() {
            if live_fn[i] {
                new_fns.push(f);
            }
        }
        prog.functions = new_fns;
    }

    // ----- Compact classes -----
    if cls_removed > 0 {
        let old_cls = std::mem::take(&mut prog.classes);
        let mut new_cls = Vec::with_capacity(cls_kept);
        for (i, mut c) in old_cls.into_iter().enumerate() {
            if live_cls[i] {
                // Self-id must match the new position so codegen's
                // `class_global[class.id.0]` lookups land on the
                // right per-compile global id.
                c.id = ClassId(cls_map[i].unwrap());
                new_cls.push(c);
            }
        }
        prog.classes = new_cls;
    }

    // ----- Remap every surviving reference -----
    prog.entry = remap_fn(prog.entry, &fn_map);

    for f in &mut prog.functions {
        match &mut f.kind {
            FunctionKind::Trampoline { target } => *target = remap_fn(*target, &fn_map),
            FunctionKind::Init { class } => *class = remap_cls(*class, &cls_map),
            FunctionKind::Drop { class } => *class = remap_cls(*class, &cls_map),
            _ => {}
        }
        for p in f.params.iter_mut() {
            remap_ty(&mut p.ty, &cls_map);
        }
        remap_ty(&mut f.ret, &cls_map);
        for ty in &mut f.value_tys {
            remap_ty(ty, &cls_map);
        }
        for ty in &mut f.local_tys {
            remap_ty(ty, &cls_map);
        }
        if let Some(env) = &mut f.closure_env {
            for c in &mut env.captures {
                remap_ty(&mut c.ty, &cls_map);
            }
        }
        for block in &mut f.blocks {
            for inst in &mut block.insts {
                remap_inst(inst, &fn_map, &cls_map);
            }
        }
    }

    for cls in &mut prog.classes {
        // `cls.id` was rewritten above during compaction.
        if let Some(p) = cls.parent {
            cls.parent = Some(remap_cls(p, &cls_map));
        }
        cls.drop_fn = remap_fn(cls.drop_fn, &fn_map);
        for m in &mut cls.methods {
            m.func = remap_fn(m.func, &fn_map);
        }
        for fld in &mut cls.fields {
            remap_ty(&mut fld.ty, &cls_map);
        }
    }

    for vt in &mut prog.vtables {
        let owner_live = cls_map
            .get(vt.class.0 as usize)
            .and_then(|m| *m)
            .is_some();
        if owner_live {
            vt.class = remap_cls(vt.class, &cls_map);
            for slot in &mut vt.slots {
                *slot = remap_fn(*slot, &fn_map);
            }
        } else {
            // Owner class is gone — nothing reads this vtable any
            // more (classes' `ClassLayout::vtable` only point from
            // live classes). Blank slot funcs so a stray indexing
            // bug downstream lands on `NO_FUNC` rather than a stale
            // FuncId pointing at an unrelated function.
            for slot in &mut vt.slots {
                *slot = FuncId(NO_FUNC);
            }
        }
    }

    for s in &mut prog.statics {
        if let Some(Some(new)) = cls_map.get(s.owner.0 as usize) {
            s.owner = ClassId(*new);
        }
        // Otherwise the owner class is dead — leave `s.owner` as the
        // stale ClassId. Only the MIR printer reads this field; the
        // codegen ignores it.
    }

    Stats { fns_removed, classes_removed: cls_removed }
}

fn mark_fn(idx: u32, live: &mut [bool], wq: &mut VecDeque<u32>) {
    if idx == NO_FUNC {
        return;
    }
    let i = idx as usize;
    if i < live.len() && !live[i] {
        live[i] = true;
        wq.push_back(idx);
    }
}

fn mark_cls(idx: u32, live: &mut [bool], wq: &mut VecDeque<u32>) {
    let i = idx as usize;
    if i < live.len() && !live[i] {
        live[i] = true;
        wq.push_back(idx);
    }
}

fn walk_ty(ty: &MirTy, live_cls: &mut [bool], cls_wq: &mut VecDeque<u32>) {
    match ty {
        MirTy::Object(c) | MirTy::Weak(c) => mark_cls(c.0, live_cls, cls_wq),
        MirTy::Array { elem, .. }
        | MirTy::Optional(elem)
        | MirTy::Promise(elem)
        | MirTy::Set { elem } => walk_ty(elem, live_cls, cls_wq),
        MirTy::Tuple(elems) => {
            for t in elems.iter() {
                walk_ty(t, live_cls, cls_wq);
            }
        }
        MirTy::Map { key, val } => {
            walk_ty(key, live_cls, cls_wq);
            walk_ty(val, live_cls, cls_wq);
        }
        MirTy::Fn(ft) | MirTy::RawFn(ft) => {
            for p in ft.params.iter() {
                walk_ty(p, live_cls, cls_wq);
            }
            walk_ty(&ft.ret, live_cls, cls_wq);
        }
        MirTy::RawPtr { inner, .. } => walk_ty(inner, live_cls, cls_wq),
        _ => {}
    }
}

fn walk_inst(
    inst: &Inst,
    live_fn: &mut [bool],
    fn_wq: &mut VecDeque<u32>,
    live_cls: &mut [bool],
    cls_wq: &mut VecDeque<u32>,
) {
    match inst {
        Inst::Call { callee: FuncRef::Local(fid), .. } => mark_fn(fid.0, live_fn, fn_wq),
        Inst::NewObject { init, class, .. } => {
            mark_fn(init.0, live_fn, fn_wq);
            mark_cls(class.0, live_cls, cls_wq);
        }
        Inst::MakeClosure { func, .. } => mark_fn(func.0, live_fn, fn_wq),
        Inst::FuncAddr { func, .. } => mark_fn(func.0, live_fn, fn_wq),
        Inst::NewArray { elem, .. } | Inst::NewArrayEmpty { elem, .. } => {
            walk_ty(elem, live_cls, cls_wq);
        }
        Inst::NewMap { key, val, .. } => {
            walk_ty(key, live_cls, cls_wq);
            walk_ty(val, live_cls, cls_wq);
        }
        Inst::IsInstance { class, .. }
        | Inst::DowncastOrNone { class, .. }
        | Inst::AddrOfField { class, .. } => {
            mark_cls(class.0, live_cls, cls_wq);
        }
        _ => {}
    }
}

fn remap_fn(id: FuncId, mapping: &[Option<u32>]) -> FuncId {
    if id.0 == NO_FUNC {
        return id;
    }
    FuncId(
        mapping[id.0 as usize]
            .expect("dce_fn: live code references a function that was removed"),
    )
}

fn remap_cls(id: ClassId, mapping: &[Option<u32>]) -> ClassId {
    ClassId(
        mapping[id.0 as usize]
            .expect("dce_fn: live code references a class that was removed"),
    )
}

fn remap_ty(ty: &mut MirTy, mapping: &[Option<u32>]) {
    match ty {
        MirTy::Object(c) | MirTy::Weak(c) => {
            *c = remap_cls(*c, mapping);
        }
        MirTy::Array { elem, .. }
        | MirTy::Optional(elem)
        | MirTy::Promise(elem)
        | MirTy::Set { elem } => remap_ty(elem, mapping),
        MirTy::Tuple(elems) => {
            for t in elems.iter_mut() {
                remap_ty(t, mapping);
            }
        }
        MirTy::Map { key, val } => {
            remap_ty(key, mapping);
            remap_ty(val, mapping);
        }
        MirTy::Fn(ft) | MirTy::RawFn(ft) => {
            for p in ft.params.iter_mut() {
                remap_ty(p, mapping);
            }
            remap_ty(&mut ft.ret, mapping);
        }
        MirTy::RawPtr { inner, .. } => remap_ty(inner, mapping),
        _ => {}
    }
}

fn remap_inst(inst: &mut Inst, fn_map: &[Option<u32>], cls_map: &[Option<u32>]) {
    match inst {
        Inst::Call { callee: FuncRef::Local(fid), .. } => *fid = remap_fn(*fid, fn_map),
        Inst::NewObject { init, class, .. } => {
            *init = remap_fn(*init, fn_map);
            *class = remap_cls(*class, cls_map);
        }
        Inst::MakeClosure { func, .. } => *func = remap_fn(*func, fn_map),
        Inst::FuncAddr { func, .. } => *func = remap_fn(*func, fn_map),
        Inst::NewArray { elem, .. } | Inst::NewArrayEmpty { elem, .. } => {
            remap_ty(elem, cls_map);
        }
        Inst::NewMap { key, val, .. } => {
            remap_ty(key, cls_map);
            remap_ty(val, cls_map);
        }
        Inst::IsInstance { class, .. }
        | Inst::DowncastOrNone { class, .. }
        | Inst::AddrOfField { class, .. } => {
            *class = remap_cls(*class, cls_map);
        }
        _ => {}
    }
}

/// Diagnostic helper. With `ILANG_DCE_FN_DEBUG=1`, print every
/// reference whose target the reachability walk failed to mark live.
/// Each printed line is a missed edge in the walk — fix the walk
/// rather than silencing the panic.
fn report_dangling(prog: &Program, fn_map: &[Option<u32>], cls_map: &[Option<u32>]) {
    let check_fn = |id: FuncId, ctx: &str| {
        if id.0 == NO_FUNC {
            return;
        }
        if fn_map[id.0 as usize].is_none() {
            eprintln!("[dce_fn] dangling fn ref: {} -> FuncId({})", ctx, id.0);
        }
    };
    let check_cls = |id: ClassId, ctx: &str| {
        if cls_map[id.0 as usize].is_none() {
            eprintln!("[dce_fn] dangling cls ref: {} -> ClassId({})", ctx, id.0);
        }
    };
    if prog.entry.0 != NO_FUNC && fn_map[prog.entry.0 as usize].is_none() {
        eprintln!("[dce_fn] dangling fn ref: prog.entry -> FuncId({})", prog.entry.0);
    }
    for (i, f) in prog.functions.iter().enumerate() {
        if !fn_map.get(i).and_then(|m| *m).is_some() {
            continue;
        }
        match f.kind {
            FunctionKind::Trampoline { target } => check_fn(target, &format!("fn#{i} trampoline.target")),
            FunctionKind::Init { class } => check_cls(class, &format!("fn#{i} kind.Init")),
            FunctionKind::Drop { class } => check_cls(class, &format!("fn#{i} kind.Drop")),
            _ => {}
        }
        for (bi, block) in f.blocks.iter().enumerate() {
            for inst in &block.insts {
                match inst {
                    Inst::Call { callee: FuncRef::Local(fid), .. } => check_fn(*fid, &format!("fn#{i} blk#{bi} Call")),
                    Inst::NewObject { init, class, .. } => {
                        check_fn(*init, &format!("fn#{i} blk#{bi} NewObject.init"));
                        check_cls(*class, &format!("fn#{i} blk#{bi} NewObject.class"));
                    }
                    Inst::MakeClosure { func, .. } => check_fn(*func, &format!("fn#{i} blk#{bi} MakeClosure")),
                    Inst::FuncAddr { func, .. } => check_fn(*func, &format!("fn#{i} blk#{bi} FuncAddr")),
                    Inst::IsInstance { class, .. } => check_cls(*class, &format!("fn#{i} blk#{bi} IsInstance")),
                    Inst::DowncastOrNone { class, .. } => check_cls(*class, &format!("fn#{i} blk#{bi} DowncastOrNone")),
                    Inst::AddrOfField { class, .. } => check_cls(*class, &format!("fn#{i} blk#{bi} AddrOfField")),
                    _ => {}
                }
            }
        }
    }
    for (ci, cls) in prog.classes.iter().enumerate() {
        if cls_map.get(ci).and_then(|m| *m).is_none() {
            continue;
        }
        check_fn(cls.drop_fn, &format!("class#{ci} ({}) drop_fn", cls.name.as_str()));
        for (mi, m) in cls.methods.iter().enumerate() {
            check_fn(m.func, &format!("class#{ci} ({}) method#{mi} ({})", cls.name.as_str(), m.name.as_str()));
        }
        if let Some(p) = cls.parent {
            check_cls(p, &format!("class#{ci} parent"));
        }
    }
}
