//! Function-body inlining pass.
//!
//! Inlines direct calls (`Inst::Call` with `FuncRef::Local(fid)`)
//! into their caller when the callee is a small single-block leaf.
//! The scope is intentionally narrow:
//!
//! * Callee must be `FunctionKind::Local` with no `closure_env`.
//! * Callee body must be a single block ending in `Terminator::Return`.
//! * Callee instruction count ≤ `INLINE_BUDGET`.
//! * Callee body must not reference `LocalId` (`DefLocal` / `UseLocal`),
//!   capture loads, closure construction, or panic — those couple
//!   the body to per-function machinery the inliner doesn't track.
//! * Callee must not call itself (direct recursion check; mutual
//!   recursion is implicitly avoided because we don't iterate to a
//!   fixed point yet).
//!
//! Effect: the call site's `Inst::Call` is replaced by a remapped
//! copy of the callee's body, and any later use of the original
//! call's `dst` is rewritten to point at the callee's returned
//! value. ARC bookkeeping is unchanged — the inlined body emits the
//! same `Retain` / `Release` shape it would have had at call time.
//!
//! Cascading is handled per-function: `run_function` iterates to a
//! fixed point (capped at `MAX_FUNCTION_ITERS`) so a call that
//! exposes another inline opportunity through its newly-spliced
//! body gets picked up on the next round. Direct self-recursion is
//! rejected; mutual recursion exits the cascade through the same
//! self-rec check after the cycle's bodies fold back into one
//! function.

use std::collections::HashMap;

use crate::inst::{FuncRef, Inst, Terminator, ValueId};
use crate::program::{Function, FunctionKind, Program};
use crate::types::MirTy;

/// Maximum non-trivial instruction count for an inline candidate.
/// Picked empirically: covers single-expression leaf helpers
/// (`fn square(x): x * x`, `fn clamp(...)`, accessor wrappers) while
/// leaving larger bodies alone. Tweak if compile time / code size
/// regresses.
const INLINE_BUDGET: usize = 8;

/// Cap on the per-function fixed-point loop. Each iteration runs
/// the splice pass once over every block. Five rounds covers
/// reasonable cascade depth (a → b → c → d → e leaf chains) while
/// guarding against exponential blow-up if a hypothetical mutually-
/// recursive pair slips past the direct-self-rec check.
const MAX_FUNCTION_ITERS: usize = 5;

#[derive(Debug, Default, Clone, Copy)]
pub struct Stats {
    pub calls_inlined: usize,
}

impl std::ops::AddAssign for Stats {
    fn add_assign(&mut self, rhs: Self) {
        self.calls_inlined += rhs.calls_inlined;
    }
}

#[derive(Clone)]
struct Candidate {
    param_values: Vec<ValueId>,
    /// Cloned body instructions — copied into the caller after value
    /// remapping.
    insts: Vec<Inst>,
    /// Value the callee returns, or `None` for unit-return.
    return_val: Option<ValueId>,
    /// Full `value_tys` of the callee — the inliner needs the type
    /// of every internal `ValueId` it allocates fresh in the caller.
    value_tys: Vec<MirTy>,
}

/// Visit every `ValueId` field of an instruction with a remapping
/// callback. The remap closure decides whether each ID is a `dst`
/// (defined-here) or a `use` (operand) by index — callers pass the
/// same closure for both since the inliner only renames; ordering
/// doesn't matter.
fn remap_inst(inst: &mut Inst, mut remap: impl FnMut(&mut ValueId)) {
    use Inst::*;
    match inst {
        Const { dst, .. } => remap(dst),
        BinOp { dst, lhs, rhs, .. } => {
            remap(dst);
            remap(lhs);
            remap(rhs);
        }
        UnOp { dst, src, .. } => {
            remap(dst);
            remap(src);
        }
        Cast { dst, src, .. } => {
            remap(dst);
            remap(src);
        }
        Call { dst, args, .. } => {
            if let Some(d) = dst {
                remap(d);
            }
            for a in args.iter_mut() {
                remap(a);
            }
        }
        CallIndirect { dst, callee, args, .. } => {
            if let Some(d) = dst {
                remap(d);
            }
            remap(callee);
            for a in args.iter_mut() {
                remap(a);
            }
        }
        VirtCall { dst, recv, args, .. } => {
            if let Some(d) = dst {
                remap(d);
            }
            remap(recv);
            for a in args.iter_mut() {
                remap(a);
            }
        }
        NewObject { dst, init_args, .. } => {
            remap(dst);
            for a in init_args.iter_mut() {
                remap(a);
            }
        }
        LoadField { dst, obj, .. } => {
            remap(dst);
            remap(obj);
        }
        StoreField { obj, value, .. } => {
            remap(obj);
            remap(value);
        }
        NewArray { dst, items, .. } => {
            remap(dst);
            for it in items.iter_mut() {
                remap(it);
            }
        }
        NewArrayEmpty { dst, .. } => remap(dst),
        ArrayLen { dst, arr } => {
            remap(dst);
            remap(arr);
        }
        ArrayLoad { dst, arr, idx } => {
            remap(dst);
            remap(arr);
            remap(idx);
        }
        ArrayStore { arr, idx, value } => {
            remap(arr);
            remap(idx);
            remap(value);
        }
        NewMap { dst, entries, .. } => {
            remap(dst);
            for (k, v) in entries.iter_mut() {
                remap(k);
                remap(v);
            }
        }
        MapGet { dst, map, key } => {
            remap(dst);
            remap(map);
            remap(key);
        }
        MapSet { map, key, value } => {
            remap(map);
            remap(key);
            remap(value);
        }
        NewTuple { dst, items } => {
            remap(dst);
            for it in items.iter_mut() {
                remap(it);
            }
        }
        TupleExtract { dst, tup, .. } => {
            remap(dst);
            remap(tup);
        }
        NewOptional { dst, value } => {
            remap(dst);
            remap(value);
        }
        OptionalIsSome { dst, opt } => {
            remap(dst);
            remap(opt);
        }
        OptionalUnwrap { dst, opt } => {
            remap(dst);
            remap(opt);
        }
        NewEnum { dst, payload, .. } => {
            remap(dst);
            for p in payload.iter_mut() {
                remap(p);
            }
        }
        EnumTag { dst, value } => {
            remap(dst);
            remap(value);
        }
        EnumPayload { dst, value, .. } => {
            remap(dst);
            remap(value);
        }
        EnumDiscStr { dst, value, .. } => {
            remap(dst);
            remap(value);
        }
        MakeClosure { dst, captures, .. } => {
            remap(dst);
            for c in captures.iter_mut() {
                remap(c);
            }
        }
        LoadCapture { dst, .. } => remap(dst),
        Retain { value } => remap(value),
        Release { value } => remap(value),
        WeakRetain { value } => remap(value),
        WeakRelease { value } => remap(value),
        WeakUpgrade { dst, weak } => {
            remap(dst);
            remap(weak);
        }
        TypeOf { dst, value } => {
            remap(dst);
            remap(value);
        }
        IsInstance { dst, value, .. } => {
            remap(dst);
            remap(value);
        }
        DowncastOrNone { dst, value, .. } => {
            remap(dst);
            remap(value);
        }
        LoadStatic { dst, .. } => remap(dst),
        StoreStatic { value, .. } => remap(value),
        Panic { .. } => {}
        DefLocal { value, .. } => remap(value),
        UseLocal { dst, .. } => remap(dst),
    }
}

/// `true` when the inst references a `LocalId`, builds / loads from
/// a closure env, or panics — these all keep the inliner conservative
/// by skipping the candidate function entirely.
fn inst_blocks_inlining(inst: &Inst) -> bool {
    matches!(
        inst,
        Inst::DefLocal { .. }
            | Inst::UseLocal { .. }
            | Inst::MakeClosure { .. }
            | Inst::LoadCapture { .. }
            | Inst::Panic { .. }
    )
}

fn extract_candidate(f: &Function, self_id: usize) -> Option<Candidate> {
    if !matches!(f.kind, FunctionKind::Local) {
        return None;
    }
    if f.closure_env.is_some() {
        return None;
    }
    if f.blocks.len() != 1 {
        return None;
    }
    let block = &f.blocks[0];
    if block.insts.len() > INLINE_BUDGET {
        return None;
    }
    let return_val = match &block.term {
        Terminator::Return { value } => *value,
        _ => return None,
    };
    for inst in &block.insts {
        if inst_blocks_inlining(inst) {
            return None;
        }
        // Recursive direct call — bail. Mutual recursion is harder
        // to detect cheaply; we accept the risk that a small mutually-
        // recursive pair sneaks through and gets infinite-iterated if
        // the pass is run in a fixed-point loop, but the caller-side
        // run_program runs once so this is currently fine.
        if let Inst::Call { callee: FuncRef::Local(fid), .. } = inst {
            if fid.0 as usize == self_id {
                return None;
            }
        }
    }
    Some(Candidate {
        param_values: f.params.iter().map(|p| p.value).collect(),
        insts: block.insts.clone(),
        return_val,
        value_tys: f.value_tys.clone(),
    })
}

pub fn run_program(prog: &mut Program) -> Stats {
    let candidates: Vec<Option<Candidate>> = prog
        .functions
        .iter()
        .enumerate()
        .map(|(i, f)| extract_candidate(f, i))
        .collect();
    let mut stats = Stats::default();
    for caller in prog.functions.iter_mut() {
        // Don't recurse into extern declarations (no body) or trampolines.
        match caller.kind {
            FunctionKind::Extern { .. } => continue,
            _ => {}
        }
        stats += run_function(caller, &candidates);
    }
    stats
}

fn run_function(caller: &mut Function, candidates: &[Option<Candidate>]) -> Stats {
    let mut total = Stats::default();
    // Iterate to a fixed point so a Call that exposes another inline
    // opportunity through its newly-spliced body gets caught on the
    // next round (e.g. `outer` → `mid` → `leaf` chains). Hard cap on
    // iterations as a guard against accidental mutual recursion
    // sneaking past the direct-self-rec eligibility check.
    for _ in 0..MAX_FUNCTION_ITERS {
        let pass = run_function_once(caller, candidates);
        total += pass;
        if pass.calls_inlined == 0 {
            break;
        }
    }
    total
}

fn run_function_once(caller: &mut Function, candidates: &[Option<Candidate>]) -> Stats {
    let mut stats = Stats::default();
    // Function-wide rename map: original `Call.dst` → remapped
    // callee return value. Accumulated across every block so a call
    // whose dst flows through a successor block (e.g. the loop
    // header of `for x in make_arr()`, where the iter ValueId is
    // consumed in the body block) still gets rewritten there.
    let mut post_rename: HashMap<ValueId, ValueId> = HashMap::new();
    let n_blocks = caller.blocks.len();
    for b in 0..n_blocks {
        stats += inline_in_block(caller, b, candidates, &mut post_rename);
    }
    if !post_rename.is_empty() {
        for block in caller.blocks.iter_mut() {
            for inst in block.insts.iter_mut() {
                remap_inst(inst, |v| {
                    if let Some(&n) = post_rename.get(v) {
                        *v = n;
                    }
                });
            }
            remap_terminator(&mut block.term, &post_rename);
        }
    }
    stats
}

fn inline_in_block(
    caller: &mut Function,
    block_idx: usize,
    candidates: &[Option<Candidate>],
    post_rename: &mut HashMap<ValueId, ValueId>,
) -> Stats {
    let mut stats = Stats::default();
    let old_insts = std::mem::take(&mut caller.blocks[block_idx].insts);
    let mut new_insts: Vec<Inst> = Vec::with_capacity(old_insts.len());
    for mut inst in old_insts {
        // Apply any pending renames inherited from earlier inlines.
        // Safe to apply to defs too: callee-side dsts get fresh IDs
        // that can't alias caller-side Call.dst.
        if !post_rename.is_empty() {
            remap_inst(&mut inst, |v| {
                if let Some(&n) = post_rename.get(v) {
                    *v = n;
                }
            });
        }
        if let Inst::Call { dst, callee: FuncRef::Local(fid), args } = &inst {
            if let Some(Some(cand)) = candidates.get(fid.0 as usize) {
                // Build a per-call value-id remap. Params map to the
                // actual args; every other ValueId gets a fresh entry
                // in the caller's value_tys.
                let mut remap: HashMap<ValueId, ValueId> = HashMap::new();
                for (i, pv) in cand.param_values.iter().enumerate() {
                    remap.insert(*pv, args[i]);
                }
                for callee_inst in &cand.insts {
                    let mut copy = callee_inst.clone();
                    remap_inst(&mut copy, |v| {
                        if let Some(&n) = remap.get(v) {
                            *v = n;
                        } else {
                            let ty = cand
                                .value_tys
                                .get(v.0 as usize)
                                .cloned()
                                .unwrap_or(MirTy::I64);
                            let fresh = ValueId(caller.value_tys.len() as u32);
                            caller.value_tys.push(ty);
                            caller.value_spans.push(None);
                            remap.insert(*v, fresh);
                            *v = fresh;
                        }
                    });
                    new_insts.push(copy);
                }
                if let Some(d) = dst {
                    let ret = cand.return_val.and_then(|rv| remap.get(&rv).copied());
                    if let Some(r) = ret {
                        post_rename.insert(*d, r);
                    }
                }
                stats.calls_inlined += 1;
                continue;
            }
        }
        new_insts.push(inst);
    }
    caller.blocks[block_idx].insts = new_insts;
    stats
}

fn remap_terminator(term: &mut Terminator, rename: &HashMap<ValueId, ValueId>) {
    let map = |v: &mut ValueId| {
        if let Some(&n) = rename.get(v) {
            *v = n;
        }
    };
    match term {
        Terminator::Br { args, .. } => {
            for a in args.iter_mut() {
                map(a);
            }
        }
        Terminator::CondBr { cond, then_args, else_args, .. } => {
            map(cond);
            for a in then_args.iter_mut() {
                map(a);
            }
            for a in else_args.iter_mut() {
                map(a);
            }
        }
        Terminator::Switch { scrutinee, cases, default_args, .. } => {
            map(scrutinee);
            for case in cases.iter_mut() {
                for a in case.args.iter_mut() {
                    map(a);
                }
            }
            for a in default_args.iter_mut() {
                map(a);
            }
        }
        Terminator::Return { value: Some(v) } => map(v),
        Terminator::Return { value: None }
        | Terminator::Unreachable => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inst::{BinOp, BlockId, FuncId, MirConst};
    use crate::program::{Block, FuncParam, FunctionKind};
    use ilang_ast::Symbol;

    fn intern(s: &str) -> Symbol {
        Symbol::intern(s)
    }

    /// Build:
    ///   fn add(a: i64, b: i64): i64 { a + b }
    ///   fn main(): i64 { add(2, 3) }
    /// and verify `main` ends up with the BinOp inlined directly.
    #[test]
    fn inlines_leaf_arithmetic_fn() {
        // add: params (v0 a, v1 b); body v2 = v0 + v1; return v2
        let add_params = vec![
            FuncParam { name: intern("a"), ty: MirTy::I64, value: ValueId(0) },
            FuncParam { name: intern("b"), ty: MirTy::I64, value: ValueId(1) },
        ];
        let add_body = vec![Inst::BinOp {
            dst: ValueId(2),
            op: BinOp::IAdd,
            lhs: ValueId(0),
            rhs: ValueId(1),
        }];
        let add = Function {
            name: intern("add"),
            display_name: intern("add"),
            params: add_params.into_boxed_slice(),
            ret: MirTy::I64,
            value_tys: vec![MirTy::I64, MirTy::I64, MirTy::I64],
            value_spans: vec![None, None, None],
            blocks: vec![Block {
                params: Vec::new(),
                insts: add_body,
                term: Terminator::Return { value: Some(ValueId(2)) },
            }],
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

        // main: v0 = const 2; v1 = const 3; v2 = call add(v0, v1); return v2
        let main = Function {
            name: intern("main"),
            display_name: intern("main"),
            params: Box::new([]),
            ret: MirTy::I64,
            value_tys: vec![MirTy::I64, MirTy::I64, MirTy::I64],
            value_spans: vec![None, None, None],
            blocks: vec![Block {
                params: Vec::new(),
                insts: vec![
                    Inst::Const {
                        dst: ValueId(0),
                        value: MirConst::Int(2),
                    },
                    Inst::Const {
                        dst: ValueId(1),
                        value: MirConst::Int(3),
                    },
                    Inst::Call {
                        dst: Some(ValueId(2)),
                        callee: FuncRef::Local(FuncId(0)),
                        args: Box::new([ValueId(0), ValueId(1)]),
                    },
                ],
                term: Terminator::Return { value: Some(ValueId(2)) },
            }],
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

        let mut prog = Program {
            functions: vec![add, main],
            classes: Vec::new(),
            enums: Vec::new(),
            vtables: Vec::new(),
            statics: Vec::new(),
            entry: FuncId(1),
        };
        let stats = run_program(&mut prog);
        assert_eq!(stats.calls_inlined, 1);

        let main_block = &prog.functions[1].blocks[0];
        // Const, Const, BinOp (inlined), no Call.
        assert_eq!(main_block.insts.len(), 3);
        assert!(matches!(main_block.insts[2], Inst::BinOp { op: BinOp::IAdd, .. }));
        // Return must reach the inlined BinOp's dst, not the original
        // Call.dst (which no longer exists).
        if let Terminator::Return { value: Some(rv) } = &main_block.term {
            let bin_dst = match &main_block.insts[2] {
                Inst::BinOp { dst, .. } => *dst,
                _ => unreachable!(),
            };
            assert_eq!(*rv, bin_dst);
        } else {
            panic!("main terminator should be Return");
        }
    }
}
