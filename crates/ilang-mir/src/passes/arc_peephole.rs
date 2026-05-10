//! ARC peephole: cancel `Retain v` / `Release v` pairs that live in
//! the same basic block with no observable effect between them.
//!
//! M2-α scope — only the safest pattern. cross-BB / dominator-aware
//! removal and escape analysis are deferred.
//!
//! Pair is removable when ALL hold:
//!   1. Both insts are in the same block.
//!   2. They reference the same `ValueId`.
//!   3. No inst between them uses `v` as an operand.
//!   4. No inst between them is a barrier — anything that could
//!      transitively observe `v`'s refcount or escape it. We use a
//!      whitelist of known-safe Insts (pure arithmetic / loads /
//!      extracts / unrelated retain/release).

use crate::inst::{Inst, ValueId};
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
    for block in &mut func.blocks {
        total += run_block_insts(&mut block.insts);
    }
    total
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
