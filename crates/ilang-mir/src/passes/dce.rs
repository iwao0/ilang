//! Dead code elimination.
//!
//! Removes pure instructions whose `dst` `ValueId` is never used
//! by any other instruction or terminator. Pure means: no side
//! effects, no allocation, no ARC bookkeeping, no possible panic.
//! Allocator instructions (`New*`, `MakeClosure`) and ARC ops
//! (`Retain` / `Release`) are intentionally left alone — the
//! lowerer pairs each allocation with a matching release at scope
//! exit, so removing the alloc without removing the release would
//! break the rc balance.
//!
//! Drives a function-wide use-set scan; removes dead insts in a
//! single sweep; iterates to a fixed point so a removal that
//! exposes another dead def gets picked up.

use std::collections::HashSet;

use crate::inst::{Inst, Terminator, ValueId};
use crate::program::{Function, FunctionKind, Program};

#[derive(Debug, Default, Clone, Copy)]
pub struct Stats {
    pub insts_removed: usize,
}

impl std::ops::AddAssign for Stats {
    fn add_assign(&mut self, rhs: Self) {
        self.insts_removed += rhs.insts_removed;
    }
}

const MAX_FUNCTION_ITERS: usize = 5;

pub fn run_program(prog: &mut Program) -> Stats {
    let mut total = Stats::default();
    for f in prog.functions.iter_mut() {
        if matches!(f.kind, FunctionKind::Extern { .. }) {
            continue;
        }
        total += run_function(f);
    }
    total
}

pub fn run_function(func: &mut Function) -> Stats {
    let mut total = Stats::default();
    for _ in 0..MAX_FUNCTION_ITERS {
        let pass = run_function_once(func);
        total += pass;
        if pass.insts_removed == 0 {
            break;
        }
    }
    total
}

fn run_function_once(func: &mut Function) -> Stats {
    // Collect every `ValueId` referenced as a *use* (not a def)
    // across all blocks. Anything pure whose dst doesn't appear
    // here is safe to drop.
    let mut used: HashSet<ValueId> = HashSet::new();
    for block in &func.blocks {
        for inst in &block.insts {
            collect_uses(inst, &mut used);
        }
        collect_term_uses(&block.term, &mut used);
    }
    let mut stats = Stats::default();
    for block in func.blocks.iter_mut() {
        let before = block.insts.len();
        block.insts.retain(|inst| !is_dead_pure(inst, &used));
        stats.insts_removed += before - block.insts.len();
    }
    stats
}

/// `true` when the inst is a pure-by-construction value producer
/// and its `dst` isn't referenced anywhere. Allocators / ARC ops
/// / stores / calls all return `false` so they survive.
fn is_dead_pure(inst: &Inst, used: &HashSet<ValueId>) -> bool {
    use Inst::*;
    match inst {
        // Integer divide / remainder panics on a zero divisor, so
        // dropping them when their dst happens to be unused would
        // silently elide the runtime check. Float div is total —
        // no panic, fine to DCE.
        BinOp {
            op: crate::inst::BinOp::IDivS
                | crate::inst::BinOp::IDivU
                | crate::inst::BinOp::IRemS
                | crate::inst::BinOp::IRemU,
            ..
        } => false,
        Const { dst, .. }
        | ClosureSelf { dst }
        | BinOp { dst, .. }
        | UnOp { dst, .. }
        | Cast { dst, .. }
        | TupleExtract { dst, .. }
        | OptionalIsSome { dst, .. }
        | EnumTag { dst, .. }
        | EnumPayload { dst, .. }
        | EnumDiscStr { dst, .. }
        | LoadCapture { dst, .. }
        | TypeOf { dst, .. }
        | IsInstance { dst, .. }
        | LoadStatic { dst, .. }
        | FuncAddr { dst, .. } => !used.contains(dst),
        // Loads from heap structures could in theory crash on a
        // null pointer at runtime, so they aren't strictly pure.
        // Leave them to a future bounds-aware DCE.
        LoadField { .. }
        | ArrayLen { .. }
        | ArrayLoad { .. }
        | MapGet { .. }
        | OptionalUnwrap { .. }
        | DowncastOrNone { .. }
        | WeakUpgrade { .. } => false,
        // Allocators: dropping these without dropping the matching
        // Release would leak the rc. Out of scope for this pass.
        NewObject { .. }
        | NewArray { .. }
        | NewArrayEmpty { .. }
        | NewSimd { .. }
        | NewMap { .. }
        | NewTuple { .. }
        | NewOptional { .. }
        | NewEnum { .. }
        | MakeClosure { .. } => false,
        // Side-effecting / control-altering instructions stay.
        Call { .. }
        | CallIndirect { .. }
        | CallRawIndirect { .. }
        | VirtCall { .. }
        | ComCall { .. }
        | StoreField { .. }
        | ArrayStore { .. }
        | MapSet { .. }
        | StoreStatic { .. }
        | Retain { .. }
        | Release { .. }
        | WeakRetain { .. }
        | WeakRelease { .. }
        | Panic { .. }
        | DefLocal { .. }
        | UseLocal { .. } => false,
        // `&local` is pure as a value producer (just a stack
        // address) but the side effect — *pinning the local into a
        // StackSlot so codegen routes Def/Use through memory* — is
        // implicit. Treat it like a regular dst-bearing inst: only
        // dead when `dst` is unused. Even when dead, the codegen
        // still pins the local; dropping the inst doesn't undo
        // that. Either way DCE on an unreferenced AddrOf is fine.
        AddrOfLocal { dst, .. } => !used.contains(dst),
        // `&path.field` is pure as a value producer (offset add on a
        // pointer). When the dst is unused, DCE can drop it; the
        // address computation has no side effects.
        AddrOfField { dst, .. } => !used.contains(dst),
    }
}

fn collect_uses(inst: &Inst, set: &mut HashSet<ValueId>) {
    use Inst::*;
    match inst {
        Const { .. } | NewArrayEmpty { .. } | LoadCapture { .. }
        | LoadStatic { .. } | Panic { .. } | FuncAddr { .. }
        | ClosureSelf { .. } => {}
        BinOp { lhs, rhs, .. } => {
            set.insert(*lhs);
            set.insert(*rhs);
        }
        UnOp { src, .. } | Cast { src, .. } => {
            set.insert(*src);
        }
        Call { args, .. } => {
            for a in args.iter() {
                set.insert(*a);
            }
        }
        CallIndirect { callee, args, .. } => {
            set.insert(*callee);
            for a in args.iter() {
                set.insert(*a);
            }
        }
        CallRawIndirect { callee, args, .. } => {
            set.insert(*callee);
            for a in args.iter() {
                set.insert(*a);
            }
        }
        VirtCall { recv, args, .. } => {
            set.insert(*recv);
            for a in args.iter() {
                set.insert(*a);
            }
        }
        ComCall { recv, args, .. } => {
            set.insert(*recv);
            for a in args.iter() {
                set.insert(*a);
            }
        }
        NewObject { init_args, .. } => {
            for a in init_args.iter() {
                set.insert(*a);
            }
        }
        LoadField { obj, .. } => {
            set.insert(*obj);
        }
        StoreField { obj, value, .. } => {
            set.insert(*obj);
            set.insert(*value);
        }
        NewArray { items, .. } | NewTuple { items, .. } => {
            for it in items.iter() {
                set.insert(*it);
            }
        }
        NewSimd { lanes, .. } => {
            for it in lanes.iter() {
                set.insert(*it);
            }
        }
        ArrayLen { arr, .. } => {
            set.insert(*arr);
        }
        ArrayLoad { arr, idx, .. } => {
            set.insert(*arr);
            set.insert(*idx);
        }
        ArrayStore { arr, idx, value } => {
            set.insert(*arr);
            set.insert(*idx);
            set.insert(*value);
        }
        NewMap { entries, .. } => {
            for (k, v) in entries.iter() {
                set.insert(*k);
                set.insert(*v);
            }
        }
        MapGet { map, key, .. } => {
            set.insert(*map);
            set.insert(*key);
        }
        MapSet { map, key, value } => {
            set.insert(*map);
            set.insert(*key);
            set.insert(*value);
        }
        TupleExtract { tup, .. } => {
            set.insert(*tup);
        }
        NewOptional { value, .. } => {
            set.insert(*value);
        }
        OptionalIsSome { opt, .. } | OptionalUnwrap { opt, .. } => {
            set.insert(*opt);
        }
        NewEnum { payload, .. } => {
            for p in payload.iter() {
                set.insert(*p);
            }
        }
        EnumTag { value, .. } | EnumPayload { value, .. } | EnumDiscStr { value, .. } => {
            set.insert(*value);
        }
        MakeClosure { captures, .. } => {
            for c in captures.iter() {
                set.insert(*c);
            }
        }
        Retain { value } | Release { value } | WeakRetain { value } | WeakRelease { value } => {
            set.insert(*value);
        }
        WeakUpgrade { weak, .. } => {
            set.insert(*weak);
        }
        TypeOf { value, .. } | IsInstance { value, .. } | DowncastOrNone { value, .. } => {
            set.insert(*value);
        }
        StoreStatic { value, .. } => {
            set.insert(*value);
        }
        DefLocal { value, .. } => {
            set.insert(*value);
        }
        UseLocal { .. } => {}
        // `&local` doesn't reference any ValueId — it names the
        // local directly. No SSA uses to collect.
        AddrOfLocal { .. } => {}
        AddrOfField { obj, .. } => {
            set.insert(*obj);
        }
    }
}

fn collect_term_uses(term: &Terminator, set: &mut HashSet<ValueId>) {
    match term {
        Terminator::Br { args, .. } => {
            for a in args.iter() {
                set.insert(*a);
            }
        }
        Terminator::CondBr { cond, then_args, else_args, .. } => {
            set.insert(*cond);
            for a in then_args.iter() {
                set.insert(*a);
            }
            for a in else_args.iter() {
                set.insert(*a);
            }
        }
        Terminator::Switch { scrutinee, cases, default_args, .. } => {
            set.insert(*scrutinee);
            for case in cases.iter() {
                for a in case.args.iter() {
                    set.insert(*a);
                }
            }
            for a in default_args.iter() {
                set.insert(*a);
            }
        }
        Terminator::Return { value: Some(v), .. } => {
            set.insert(*v);
        }
        Terminator::Return { value: None, .. } | Terminator::Unreachable => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inst::{BinOp, BlockId, MirConst};
    use crate::program::{Block, FuncParam, Function, FunctionKind};
    use crate::types::MirTy;

    fn intern(s: &str) -> ilang_ast::Symbol {
        ilang_ast::Symbol::intern(s)
    }

    /// `let r = 2 + 3; let _unused = 99; r` — the unused Const
    /// should drop. The arithmetic chain stays because the return
    /// value depends on it.
    #[test]
    fn drops_unreferenced_const() {
        let blocks = vec![Block {
            params: Vec::new(),
            insts: vec![
                Inst::Const { dst: ValueId(0), value: MirConst::Int(2) },
                Inst::Const { dst: ValueId(1), value: MirConst::Int(3) },
                Inst::BinOp { dst: ValueId(2), op: BinOp::IAdd, lhs: ValueId(0), rhs: ValueId(1) },
                Inst::Const { dst: ValueId(3), value: MirConst::Int(99) }, // dead
            ],
            term: Terminator::Return { value: Some(ValueId(2)), release_value: false },
        }];
        let mut func = Function {
            name: intern("f"),
            display_name: intern("f"),
            params: Box::new([] as [FuncParam; 0]),
            ret: MirTy::I64,
            value_tys: vec![MirTy::I64; 4],
            value_spans: vec![None; 4],
            blocks,
            entry: BlockId(0),
            kind: FunctionKind::Local,
            closure_env: None,
            span: None,
            local_tys: Vec::new(),
            c_symbol: None,
            is_optional: false,
            libs: Vec::new(),
            is_variadic: false,
        };
        let stats = run_function(&mut func);
        assert_eq!(stats.insts_removed, 1);
        // The remaining 3 insts: Const 2, Const 3, IAdd.
        assert_eq!(func.blocks[0].insts.len(), 3);
    }

    /// After `const_fold` turns `BinOp(Const(2), Const(3))` into
    /// `Const(6)`, the two source Consts become dead and DCE picks
    /// them up.
    #[test]
    fn cascades_with_const_fold_output() {
        let blocks = vec![Block {
            params: Vec::new(),
            insts: vec![
                Inst::Const { dst: ValueId(0), value: MirConst::Int(2) }, // becomes dead
                Inst::Const { dst: ValueId(1), value: MirConst::Int(3) }, // becomes dead
                Inst::Const { dst: ValueId(2), value: MirConst::Int(6) }, // ex-BinOp
            ],
            term: Terminator::Return { value: Some(ValueId(2)), release_value: false },
        }];
        let mut func = Function {
            name: intern("f"),
            display_name: intern("f"),
            params: Box::new([] as [FuncParam; 0]),
            ret: MirTy::I64,
            value_tys: vec![MirTy::I64; 3],
            value_spans: vec![None; 3],
            blocks,
            entry: BlockId(0),
            kind: FunctionKind::Local,
            closure_env: None,
            span: None,
            local_tys: Vec::new(),
            c_symbol: None,
            is_optional: false,
            libs: Vec::new(),
            is_variadic: false,
        };
        let stats = run_function(&mut func);
        assert_eq!(stats.insts_removed, 2);
        // Only Const(6) survives.
        assert_eq!(func.blocks[0].insts.len(), 1);
    }
}
