//! ARC peephole: cancel `Retain v` / `Release v` pairs whose net
//! effect on `v`'s refcount is zero across a small window.
//!
//! Three patterns:
//!
//! - **Intra-block** (M2-α): `Retain v` and `Release v` in the same
//!   block with only safe-to-cross insts between them.
//! - **Local-aware intra-block** (M2-γ): same as above, but
//!   `DefLocal`/`UseLocal` chains are followed so two `ValueId`s
//!   that name the same runtime object via a mutable slot count as
//!   the same value. `let x = …; retain x; let y = x; …; release y;
//!   release x` collapses cleanly even though `retain` and the
//!   first `release` use different `ValueId`s.
//! - **Extended-BB chain** (M2-β step 1+2): `Retain v` near the end
//!   of some block `B0`, with `B0` ending in an unconditional `Br`.
//!   We follow the unique-predecessor chain `B0 → B1 → … → Bn`,
//!   renaming `v` through each block's arg→param mapping, scanning
//!   each block's body left-to-right for the matching `Release`.
//!   Intermediate blocks must be entirely safe-to-cross.
//!
//! All three patterns share the same notion of "safe to cross": pure
//! arithmetic / loads / extracts / unrelated ARC ops / mutable-slot
//! reads and writes. Calls, stores into heaps, allocations, and
//! anything else are barriers.
//!
//! Dominator-aware whole-CFG cancellation and escape analysis are
//! deferred to later M2 steps.

use std::collections::{HashMap, HashSet};

use crate::inst::{BlockId, Inst, LocalId, Terminator, ValueId};
use crate::program::{Function, Program};

#[derive(Debug, Default, Clone, Copy)]
pub struct Stats {
    /// Pairs removed by the intra-block (incl. local-aware) peephole.
    pub intra_block: usize,
    /// Pairs removed by the single-pred chain walker.
    pub chain: usize,
    /// `intra_block + chain` — kept for backwards compatibility with
    /// the unit tests that read this field directly.
    pub pairs_removed: usize,
}

impl std::ops::AddAssign for Stats {
    fn add_assign(&mut self, rhs: Self) {
        self.intra_block += rhs.intra_block;
        self.chain += rhs.chain;
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
        let preds = predecessors(func);
        let mut equiv = build_function_equiv(func, &preds);
        let intra = run_intra_block(func, &mut equiv);
        let cross = run_extended_bb(func, &preds, &mut equiv);
        let pass = intra.pairs_removed + cross.pairs_removed;
        total.intra_block += intra.pairs_removed;
        total.chain += cross.pairs_removed;
        total.pairs_removed += pass;
        if pass == 0 {
            break;
        }
    }
    total
}

fn run_intra_block(func: &mut Function, equiv: &mut ValueEquiv) -> Stats {
    let mut total = Stats::default();
    for block in &mut func.blocks {
        total += run_block_insts(&mut block.insts, equiv);
    }
    total
}

fn run_extended_bb(
    func: &mut Function,
    preds: &[Vec<BlockId>],
    equiv: &mut ValueEquiv,
) -> Stats {
    let mut stats = Stats::default();
    let n = func.blocks.len();
    for b0_idx in 0..n {
        let Some((retain_pos, retain_v)) =
            scan_back_for_retain(&func.blocks[b0_idx].insts)
        else {
            continue;
        };
        let Some((release_block, release_pos)) =
            find_chain_release(func, preds, equiv, b0_idx, retain_v)
        else {
            continue;
        };
        debug_assert_ne!(release_block, b0_idx);
        func.blocks[b0_idx].insts.remove(retain_pos);
        func.blocks[release_block].insts.remove(release_pos);
        stats.pairs_removed += 1;
    }
    stats
}

/// Walk the unique-predecessor chain starting from `start_block`'s
/// terminator and look for a `Release w` with `equiv(w) == target`.
/// Returns `(block, inst_idx)` of the matching Release if the chain
/// reaches one without crossing a barrier.
fn find_chain_release(
    func: &Function,
    preds: &[Vec<BlockId>],
    equiv: &mut ValueEquiv,
    start_block: usize,
    start_v: ValueId,
) -> Option<(usize, usize)> {
    let target = equiv.find(start_v);
    let mut visited: HashSet<usize> = HashSet::new();
    visited.insert(start_block);
    let mut current_block = start_block;
    loop {
        let next_idx = match &func.blocks[current_block].term {
            Terminator::Br { dst, .. } => dst.0 as usize,
            _ => return None,
        };
        if preds.get(next_idx).map(|p| p.len()).unwrap_or(0) != 1 {
            return None;
        }
        if !visited.insert(next_idx) {
            return None;
        }

        for (i, inst) in func.blocks[next_idx].insts.iter().enumerate() {
            if let Inst::Release { value: w } = inst {
                if equiv.find(*w) == target {
                    return Some((next_idx, i));
                }
            }
            if !is_safe_to_cross(inst) {
                return None;
            }
        }
        // Passed through `next_idx`'s entire body — extend the
        // chain by following its terminator next round.
        current_block = next_idx;
    }
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

fn run_block_insts(insts: &mut Vec<Inst>, equiv: &mut ValueEquiv) -> Stats {
    let mut remove = vec![false; insts.len()];
    let mut stats = Stats::default();

    let mut i = 0;
    while i < insts.len() {
        if remove[i] {
            i += 1;
            continue;
        }
        if let Inst::Retain { value: v } = insts[i] {
            let target = equiv.find(v);
            let mut j = i + 1;
            while j < insts.len() {
                if remove[j] {
                    j += 1;
                    continue;
                }
                let inst = &insts[j];
                if let Inst::Release { value: w } = inst {
                    if equiv.find(*w) == target {
                        remove[i] = true;
                        remove[j] = true;
                        stats.pairs_removed += 1;
                        break;
                    }
                }
                if !is_safe_to_cross(inst) {
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

/// Per-block runtime-equivalence union-find over `ValueId`s: two
/// `ValueId`s become equivalent when they flow through the same
/// mutable slot via `DefLocal` / `UseLocal` without an intervening
/// rebind. Heap allocations and computed values stay in singleton
/// classes.
///
/// The map is timing-sensitive: only the most-recently-`def_local`'d
/// value is unioned with subsequent reads of that local, so a slot
/// rebind cleanly partitions the equivalence classes on either side
/// of the new `DefLocal`.
#[derive(Default)]
struct ValueEquiv {
    parent: HashMap<ValueId, ValueId>,
}

impl ValueEquiv {
    fn find(&mut self, v: ValueId) -> ValueId {
        let mut cur = v;
        let mut path: Vec<ValueId> = Vec::new();
        loop {
            match self.parent.get(&cur).copied() {
                None => {
                    self.parent.insert(cur, cur);
                    break;
                }
                Some(p) if p == cur => break,
                Some(p) => {
                    path.push(cur);
                    cur = p;
                }
            }
        }
        for x in path {
            self.parent.insert(x, cur);
        }
        cur
    }

    fn union(&mut self, a: ValueId, b: ValueId) {
        let ra = self.find(a);
        let rb = self.find(b);
        if ra != rb {
            self.parent.insert(ra, rb);
        }
    }
}

/// Function-wide equivalence built from three sources:
///
/// 1. **Per-block local-slot tracking**: within each block the most
///    recent `DefLocal %X = v` is unioned with subsequent
///    `UseLocal %X → w` reads. Slot rebinds partition the equivalence
///    so values on either side stay distinct.
/// 2. **Single-DefLocal locals**: if a local has exactly one
///    `DefLocal` site in the entire function, every `UseLocal` of it
///    (in any block) is unioned with that single value. Multi-DefLocal
///    locals stay block-local to avoid over-unioning values that join
///    at a slot.
/// 3. **Single-pred block-arg/param edges**: when a terminator
///    transfers `args[i]` into a successor whose only predecessor is
///    the current block, `args[i]` and `params[i]` are unioned.
///    Branches into multi-pred merge blocks are skipped to avoid
///    the same join-induced over-union.
fn build_function_equiv(func: &Function, preds: &[Vec<BlockId>]) -> ValueEquiv {
    let mut equiv = ValueEquiv::default();

    // (1) and prep for (2): scan every block for DefLocal sites.
    let mut def_count: HashMap<LocalId, usize> = HashMap::new();
    let mut single_def_value: HashMap<LocalId, ValueId> = HashMap::new();
    for block in &func.blocks {
        for inst in &block.insts {
            if let Inst::DefLocal { local, value } = inst {
                *def_count.entry(*local).or_insert(0) += 1;
                single_def_value.insert(*local, *value);
            }
        }
    }
    let single_def: HashMap<LocalId, ValueId> = single_def_value
        .into_iter()
        .filter(|(l, _)| def_count.get(l) == Some(&1))
        .collect();

    for block in &func.blocks {
        // Per-block local tracking (1).
        let mut holds: HashMap<LocalId, ValueId> = HashMap::new();
        for inst in &block.insts {
            match inst {
                Inst::DefLocal { local, value } => {
                    holds.insert(*local, *value);
                }
                Inst::UseLocal { dst, local } => {
                    // Prefer the per-block holds (timing-aware); for
                    // single-DefLocal locals fall back to the global
                    // value when the local hasn't been written in
                    // this block yet.
                    if let Some(&v) = holds.get(local) {
                        equiv.union(*dst, v);
                    } else if let Some(&v) = single_def.get(local) {
                        equiv.union(*dst, v);
                    }
                }
                _ => {}
            }
        }

        // Single-pred block-arg/param edges (3).
        let union_args = |equiv: &mut ValueEquiv,
                          dst_idx: usize,
                          args: &[ValueId]| {
            if preds.get(dst_idx).map(|p| p.len()) != Some(1) {
                return;
            }
            let params = &func.blocks[dst_idx].params;
            if params.len() != args.len() {
                return;
            }
            for (a, p) in args.iter().zip(params.iter()) {
                equiv.union(*a, *p);
            }
        };
        match &block.term {
            Terminator::Br { dst, args } => {
                union_args(&mut equiv, dst.0 as usize, args);
            }
            Terminator::CondBr {
                then_block,
                then_args,
                else_block,
                else_args,
                ..
            } => {
                union_args(&mut equiv, then_block.0 as usize, then_args);
                union_args(&mut equiv, else_block.0 as usize, else_args);
            }
            Terminator::Switch {
                cases,
                default,
                default_args,
                ..
            } => {
                union_args(&mut equiv, default.0 as usize, default_args);
                for c in cases.iter() {
                    union_args(&mut equiv, c.dst.0 as usize, &c.args);
                }
            }
            Terminator::Return { .. } | Terminator::Unreachable => {}
        }
    }
    equiv
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
        | Inst::TypeOf { .. }
        | Inst::IsInstance { .. } => true,

        // Mutable-slot reads / writes — observed through the
        // local-aware equivalence map, never bumps refcounts on
        // their own.
        Inst::UseLocal { .. } | Inst::DefLocal { .. } => true,

        // Retain/Release of another value — safe (the equivalence
        // check decides whether one matches our candidate).
        Inst::Retain { .. }
        | Inst::Release { .. }
        | Inst::WeakRetain { .. }
        | Inst::WeakRelease { .. } => true,

        // Everything else is a barrier: calls (host or user — may
        // observe global refcount), allocations (drop on OOM),
        // stores into heaps, terminator-like Panic, etc.
        _ => false,
    }
}
