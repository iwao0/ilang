//! ARC peephole: cancel `Retain v` / `Release v` pairs whose net
//! effect on `v`'s refcount is zero across a small window.
//!
//! Two patterns:
//!
//! - **Intra-block** (M2-α): `Retain v` and `Release v` in the same
//!   block with only safe-to-cross insts between them.
//! - **Extended-BB** (M2-β step 1): `Retain v` near the end of `B1`,
//!   `B1` ends with an unconditional `Br B2(.. v ..)`, `B2` has
//!   exactly one predecessor (`B1`), and `Release w` near the start
//!   of `B2` where `w` is the block-param that received `v`. The
//!   block-arg/param plumbing stays untouched; only the two ARC
//!   insts are removed.
//!
//! Both patterns share the same notion of "safe to cross": pure
//! arithmetic / loads / extracts / unrelated ARC ops. Calls,
//! stores, allocations, and anything else are barriers.
//!
//! Dominator-aware whole-CFG cancellation and escape analysis are
//! deferred to later M2 steps.

use crate::inst::{BlockId, Inst, Terminator, ValueId};
use crate::program::{Function, Program};

#[derive(Debug, Default, Clone, Copy)]
pub struct Stats {
    pub pairs_removed: usize,
}

impl std::ops::AddAssign for Stats {
    fn add_assign(&mut self, rhs: Self) {
        self.pairs_removed += rhs.pairs_removed;
    }
}

pub fn run_program(prog: &mut Program) -> Stats {
    let mut total = Stats::default();
    for f in &mut prog.functions {
        total += run_function(f);
    }
    total
}

pub fn run_function(func: &mut Function) -> Stats {
    let mut total = Stats::default();
    // Iterate to fixed point so an extended-BB removal that exposes
    // a fresh intra-block pair (or vice versa) gets picked up.
    loop {
        let intra = run_intra_block(func);
        let cross = run_extended_bb(func);
        let pass = intra.pairs_removed + cross.pairs_removed;
        total.pairs_removed += pass;
        if pass == 0 {
            break;
        }
    }
    total
}

fn run_intra_block(func: &mut Function) -> Stats {
    let mut total = Stats::default();
    for block in &mut func.blocks {
        total += run_block_insts(&mut block.insts);
    }
    total
}

fn run_extended_bb(func: &mut Function) -> Stats {
    let mut stats = Stats::default();
    let preds = predecessors(func);
    let n = func.blocks.len();
    for b1_idx in 0..n {
        let (b2_idx, args) = match &func.blocks[b1_idx].term {
            Terminator::Br { dst, args } => (dst.0 as usize, args.clone()),
            _ => continue,
        };
        if b2_idx == b1_idx {
            // Self-loop — the param-renaming would alias `v` to its
            // own param, breaking the value-identity assumptions.
            // Skip to keep the analysis local.
            continue;
        }
        if preds.get(b2_idx).map(|p| p.len()).unwrap_or(0) != 1 {
            continue;
        }
        let b2_params: Vec<ValueId> = func.blocks[b2_idx].params.clone();
        if b2_params.len() != args.len() {
            // Validator should reject this, but guard anyway.
            continue;
        }
        let Some((retain_pos, retain_v)) =
            scan_back_for_retain(&func.blocks[b1_idx].insts)
        else {
            continue;
        };
        // Map B1's value to the corresponding B2 block-param.
        let Some(arg_pos) = args.iter().position(|x| *x == retain_v) else {
            continue;
        };
        let b2_v = b2_params[arg_pos];
        let Some(release_pos) =
            scan_forward_for_release(&func.blocks[b2_idx].insts, b2_v)
        else {
            continue;
        };
        // Apply removal. b1 != b2 was checked above, so the two
        // mutations don't overlap.
        func.blocks[b1_idx].insts.remove(retain_pos);
        func.blocks[b2_idx].insts.remove(release_pos);
        stats.pairs_removed += 1;
    }
    stats
}

fn predecessors(func: &Function) -> Vec<Vec<BlockId>> {
    let mut preds = vec![Vec::new(); func.blocks.len()];
    for (idx, block) in func.blocks.iter().enumerate() {
        let from = BlockId(idx as u32);
        match &block.term {
            Terminator::Br { dst, .. } => preds[dst.0 as usize].push(from),
            Terminator::CondBr {
                then_block, else_block, ..
            } => {
                preds[then_block.0 as usize].push(from);
                preds[else_block.0 as usize].push(from);
            }
            Terminator::Switch { cases, default, .. } => {
                preds[default.0 as usize].push(from);
                for c in cases.iter() {
                    preds[c.dst.0 as usize].push(from);
                }
            }
            Terminator::Return { .. } | Terminator::Unreachable => {}
        }
    }
    preds
}

/// Scan B1's insts from the end, looking for a `Retain v` such that
/// every inst between it and the terminator is safe to cross.
fn scan_back_for_retain(insts: &[Inst]) -> Option<(usize, ValueId)> {
    for i in (0..insts.len()).rev() {
        match &insts[i] {
            Inst::Retain { value } => return Some((i, *value)),
            inst if is_safe_to_cross(inst) => continue,
            _ => return None,
        }
    }
    None
}

/// Scan B2's insts from the start, looking for `Release v` such that
/// every inst before it is safe to cross AND none of them use `v`.
fn scan_forward_for_release(insts: &[Inst], v: ValueId) -> Option<usize> {
    for (i, inst) in insts.iter().enumerate() {
        if let Inst::Release { value } = inst {
            if *value == v {
                return Some(i);
            }
        }
        if uses_value(inst, v) {
            return None;
        }
        if !is_safe_to_cross(inst) {
            return None;
        }
    }
    None
}

fn run_block_insts(insts: &mut Vec<Inst>) -> Stats {
    let mut remove = vec![false; insts.len()];
    let mut stats = Stats::default();

    let mut i = 0;
    while i < insts.len() {
        if remove[i] {
            i += 1;
            continue;
        }
        if let Inst::Retain { value: v } = insts[i] {
            // Scan forward for a matching Release on the same value.
            let mut j = i + 1;
            while j < insts.len() {
                if remove[j] {
                    j += 1;
                    continue;
                }
                let inst = &insts[j];
                if let Inst::Release { value: w } = inst {
                    if *w == v {
                        // Pair found — both safe to drop because
                        // every inst we scanned past was either
                        // whitelisted-pure or a Retain/Release of an
                        // unrelated value, and none of them used `v`.
                        remove[i] = true;
                        remove[j] = true;
                        stats.pairs_removed += 1;
                        break;
                    }
                }
                if uses_value(inst, v) || !is_safe_to_cross(inst) {
                    break;
                }
                j += 1;
            }
        }
        i += 1;
    }

    if stats.pairs_removed > 0 {
        // Compact in place, preserving order.
        let mut k = 0;
        for idx in 0..insts.len() {
            if !remove[idx] {
                insts.swap(k, idx);
                k += 1;
            }
        }
        insts.truncate(k);
    }
    stats
}

/// `true` iff `inst` could be skipped over when looking for a matching
/// `Release` — it doesn't observe / mutate / escape the candidate
/// value's refcount.
///
/// Whitelist intentionally narrow. New Inst variants default to
/// "barrier" until reviewed.
fn is_safe_to_cross(inst: &Inst) -> bool {
    match inst {
        // Pure value production / arithmetic / loads.
        Inst::Const { .. }
        | Inst::BinOp { .. }
        | Inst::UnOp { .. }
        | Inst::Cast { .. }
        | Inst::LoadField { .. }
        | Inst::ArrayLen { .. }
        | Inst::ArrayLoad { .. }
        | Inst::MapGet { .. }
        | Inst::TupleExtract { .. }
        | Inst::OptionalIsSome { .. }
        | Inst::OptionalUnwrap { .. }
        | Inst::EnumTag { .. }
        | Inst::EnumPayload { .. }
        | Inst::LoadCapture { .. }
        | Inst::LoadStatic { .. }
        | Inst::UseLocal { .. }
        | Inst::TypeOf { .. }
        | Inst::IsInstance { .. } => true,

        // Retain/Release of another value — safe (operand check
        // will catch matches on v).
        Inst::Retain { .. }
        | Inst::Release { .. }
        | Inst::WeakRetain { .. }
        | Inst::WeakRelease { .. } => true,

        // Everything else is a barrier: calls (host or user — may
        // observe global refcount), allocations (drop on OOM),
        // stores (alias-to-v risk), terminator-like Panic, etc.
        _ => false,
    }
}

/// `true` iff `inst` reads `v` as an operand. Defines (the `dst`
/// field) don't count.
fn uses_value(inst: &Inst, v: ValueId) -> bool {
    let mut hit = false;
    let mut check = |x: ValueId| {
        if x == v {
            hit = true;
        }
    };
    match inst {
        Inst::Const { .. }
        | Inst::NewArrayEmpty { .. }
        | Inst::LoadCapture { .. }
        | Inst::LoadStatic { .. }
        | Inst::UseLocal { .. }
        | Inst::Panic { .. } => {}
        Inst::BinOp { lhs, rhs, .. } => {
            check(*lhs);
            check(*rhs);
        }
        Inst::UnOp { src, .. } | Inst::Cast { src, .. } => check(*src),
        Inst::Call { args, .. } => {
            for a in args.iter() {
                check(*a);
            }
        }
        Inst::CallIndirect { callee, args, .. } => {
            check(*callee);
            for a in args.iter() {
                check(*a);
            }
        }
        Inst::VirtCall { recv, args, .. } => {
            check(*recv);
            for a in args.iter() {
                check(*a);
            }
        }
        Inst::NewObject { init_args, .. } => {
            for a in init_args.iter() {
                check(*a);
            }
        }
        Inst::LoadField { obj, .. } => check(*obj),
        Inst::StoreField { obj, value, .. } => {
            check(*obj);
            check(*value);
        }
        Inst::NewArray { items, .. } | Inst::NewTuple { items, .. } => {
            for a in items.iter() {
                check(*a);
            }
        }
        Inst::ArrayLen { arr, .. } => check(*arr),
        Inst::ArrayLoad { arr, idx, .. } => {
            check(*arr);
            check(*idx);
        }
        Inst::ArrayStore { arr, idx, value } => {
            check(*arr);
            check(*idx);
            check(*value);
        }
        Inst::NewMap { entries, .. } => {
            for (k, val) in entries.iter() {
                check(*k);
                check(*val);
            }
        }
        Inst::MapGet { map, key, .. } => {
            check(*map);
            check(*key);
        }
        Inst::MapSet { map, key, value } => {
            check(*map);
            check(*key);
            check(*value);
        }
        Inst::TupleExtract { tup, .. } => check(*tup),
        Inst::NewOptional { value, .. }
        | Inst::OptionalIsSome { opt: value, .. }
        | Inst::OptionalUnwrap { opt: value, .. } => check(*value),
        Inst::NewEnum { payload, .. } => {
            for a in payload.iter() {
                check(*a);
            }
        }
        Inst::EnumTag { value, .. } => check(*value),
        Inst::EnumPayload { value, .. } => check(*value),
        Inst::MakeClosure { captures, .. } => {
            for a in captures.iter() {
                check(*a);
            }
        }
        Inst::Retain { value }
        | Inst::Release { value }
        | Inst::WeakRetain { value }
        | Inst::WeakRelease { value } => check(*value),
        Inst::WeakUpgrade { weak, .. } => check(*weak),
        Inst::TypeOf { value, .. } => check(*value),
        Inst::IsInstance { value, .. } => check(*value),
        Inst::DowncastOrNone { value, .. } => check(*value),
        Inst::StoreStatic { value, .. } => check(*value),
        Inst::DefLocal { value, .. } => check(*value),
    }
    hit
}
