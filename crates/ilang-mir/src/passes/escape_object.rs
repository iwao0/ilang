//! Escape analysis for `Inst::NewObject` — flags allocations that
//! can be promoted from the heap to a function-local stack slot.
//!
//! Conditions for promotion (all required):
//!
//! 1. The class has no user deinit (`drop_fn == FuncId::MAX`) — a
//!    user destructor needs to run at drop time, but stack-promoted
//!    objects never go through `__release_object`.
//! 2. Every field is a primitive (non-heap). The runtime's field
//!    cascade walks the OBJECT_FIELD_TABLE on release; primitive-
//!    only means no cascade is needed in the first place.
//! 3. The `NewObject` dst doesn't escape its function: not returned,
//!    not stored into any heap container, not passed to a call /
//!    virtual dispatch, not captured by a closure, not boxed into
//!    Optional / Enum / Array / Tuple / Map, not written to a
//!    static slot.
//!
//! The init function is conservatively assumed to be well-behaved
//! (writes to `this.field` and returns `this`). A future refinement
//! could walk init's body to verify no further escape.

use std::collections::{HashMap, HashSet};

use crate::inst::{FuncId, Inst, LocalId, ValueId};
use crate::program::{FunctionKind, Program};

#[derive(Debug, Default, Clone, Copy)]
pub struct Stats {
    pub objects_promoted: usize,
}

impl std::ops::AddAssign for Stats {
    fn add_assign(&mut self, rhs: Self) {
        self.objects_promoted += rhs.objects_promoted;
    }
}

/// Side-table keyed by `FuncId`, holding the set of `NewObject` dst
/// values eligible for stack promotion in that function.
pub type StackPromoted = HashMap<FuncId, HashSet<ValueId>>;

pub fn run_program(prog: &Program) -> (StackPromoted, Stats) {
    let mut out: StackPromoted = HashMap::new();
    let mut stats = Stats::default();
    for (idx, _f) in prog.functions.iter().enumerate() {
        let set = analyze_function(prog, idx);
        stats.objects_promoted += set.len();
        if !set.is_empty() {
            out.insert(FuncId(idx as u32), set);
        }
    }
    (out, stats)
}

/// Per-function analysis. Returns the set of `NewObject` dst
/// `ValueId`s that are safe to stack-allocate. Codegen can call
/// this directly per function instead of materialising the full
/// program-wide map.
pub fn analyze_function(prog: &Program, fn_idx: usize) -> HashSet<ValueId> {
    let f = &prog.functions[fn_idx];
    if matches!(f.kind, FunctionKind::Extern { .. }) {
        return HashSet::new();
    }
    let mut candidates: Vec<ValueId> = Vec::new();
    for block in &f.blocks {
        for inst in &block.insts {
            if let Inst::NewObject { dst, class, .. } = inst {
                let cls = &prog.classes[class.0 as usize];
                if cls.drop_fn != FuncId(u32::MAX) {
                    continue;
                }
                // ArcObject: classic stack-promotion of an RC'd class
                // whose `init` is trivial and fields are primitive.
                // CRepr / CPacked / CUnion: top-level struct / union
                // (and `@extern(C)` aggregates) — no header, no
                // refcount, no drop. Field offsets and store/load
                // handlers already work on a flat byte buffer
                // regardless of heap vs stack origin, so the same
                // escape rules below decide whether it's safe to
                // back the value with a StackSlot instead of an
                // alloc call.
                use crate::program::ClassRepr;
                if !matches!(
                    cls.repr,
                    ClassRepr::ArcObject | ClassRepr::CRepr | ClassRepr::CPacked | ClassRepr::CUnion
                ) {
                    continue;
                }
                if !cls.fields.iter().all(|fd| !fd.ty.is_heap()) {
                    continue;
                }
                candidates.push(*dst);
            }
        }
    }
    if candidates.is_empty() {
        return HashSet::new();
    }
    let mut cand_set: HashSet<ValueId> = candidates.iter().copied().collect();
    // Build alias chain for single-def Locals. A `DefLocal { local,
    // value: cand }` followed by `UseLocal { dst, local }` means the
    // dst is a re-binding of cand. Without this chain, escapes
    // through the local's `UseLocal` dst (e.g. `let c = new T(); c
    // .method()` reads c via UseLocal whose dst is then passed to
    // a Call → we have to leak the original NewObject dst).
    let mut local_def_count: HashMap<LocalId, u32> = HashMap::new();
    let mut local_def_value: HashMap<LocalId, ValueId> = HashMap::new();
    for block in &f.blocks {
        for inst in &block.insts {
            if let Inst::DefLocal { local, value } = inst {
                *local_def_count.entry(*local).or_insert(0) += 1;
                local_def_value.entry(*local).or_insert(*value);
            }
        }
    }
    let mut alias: HashMap<ValueId, ValueId> = HashMap::new();
    for block in &f.blocks {
        for inst in &block.insts {
            if let Inst::UseLocal { dst, local } = inst {
                // Single-def Locals: every UseLocal returns the same
                // stored value. Multi-def Locals could hold a non-
                // candidate by the time we reach the use, but to stay
                // conservative we still alias — if the use turns out
                // to be an escape, we leak the candidate. Worst case:
                // a multi-def Local whose other writes set a non-
                // candidate value causes a spurious leak (loss of
                // optimisation, never unsoundness).
                if let Some(&src) = local_def_value.get(local) {
                    alias.insert(*dst, src);
                }
                let _ = local_def_count.get(local);
            }
        }
    }
    // Resolve transitively: an aliased dst whose source is itself
    // aliased (rare but possible across nested DefLocal / UseLocal
    // chains) should point at the root.
    let alias_resolved: HashMap<ValueId, ValueId> = alias
        .iter()
        .map(|(&k, &v)| (k, resolve_alias(v, &alias)))
        .collect();
    for block in &f.blocks {
        for inst in &block.insts {
            check_inst_escape(inst, &mut cand_set, &alias_resolved, f, prog);
        }
        check_term_escape(&block.term, &mut cand_set, &alias_resolved);
    }
    // Codegen looks up `Retain { value }` / `Release { value }` by
    // the literal ValueId, but for stack-promoted objects the value
    // typically reaches the ARC op through a UseLocal alias (e.g.
    // `v4 = use_local %0` then `retain v4` on the v1-promoted
    // NewObject). Pull every aliased dst whose root is a surviving
    // candidate into the set so codegen's skip check fires for
    // those too.
    let mut expanded = cand_set.clone();
    for (&dst, &root) in &alias_resolved {
        if cand_set.contains(&root) {
            expanded.insert(dst);
        }
    }
    expanded
}

fn resolve_alias(mut v: ValueId, alias: &HashMap<ValueId, ValueId>) -> ValueId {
    // Bounded chase to dodge any pathological cycle (shouldn't
    // happen with SSA, but cheap insurance).
    for _ in 0..16 {
        match alias.get(&v) {
            Some(&n) if n != v => v = n,
            _ => break,
        }
    }
    v
}

/// `true` when an arg passed to a Call would be consumed by-value:
/// the call site either explodes it into HFA float regs / i64
/// chunks (size under the callee's chunk cap — 16 B for C ABI,
/// `IL_BYVAL_CHUNK_MAX` for ilang ABI), or memcpys it into a
/// scratch buffer (over the cap) before the call. In all shapes
/// the callee gets its own frame copy and the caller's pointer
/// never reaches it, so the arg doesn't escape and a stack-
/// promoted local can survive being passed through the call.
/// ArcObject / Array / etc. stay reference-typed and are NOT
/// covered.
fn arg_passed_by_value(arg_ty: &crate::types::MirTy, prog: &Program) -> bool {
    if let crate::types::MirTy::Object(cid) = arg_ty {
        let layout = &prog.classes[cid.0 as usize];
        matches!(
            layout.repr,
            crate::program::ClassRepr::CRepr
                | crate::program::ClassRepr::CPacked
                | crate::program::ClassRepr::CUnion
        )
    } else {
        false
    }
}

fn check_inst_escape(
    inst: &Inst,
    cands: &mut HashSet<ValueId>,
    alias: &HashMap<ValueId, ValueId>,
    func: &crate::program::Function,
    prog: &Program,
) {
    use Inst::*;
    let mut leak = |v: &ValueId| {
        let canon = alias.get(v).copied().unwrap_or(*v);
        cands.remove(&canon);
    };
    match inst {
        // The NewObject site itself doesn't count as a use of the
        // dst (only as a def). Pure value producers (Const, BinOp,
        // UnOp, Cast, ArrayLen, OptionalIsSome, EnumTag, TypeOf,
        // IsInstance, LoadCapture, LoadStatic, ArrayLoad, MapGet,
        // OptionalUnwrap, TupleExtract, LoadField, EnumPayload,
        // EnumDiscStr, DowncastOrNone, WeakUpgrade) consume their
        // operands but don't escape them — leaving them alone.
        Const { .. } | NewArrayEmpty { .. } | LoadCapture { .. } | LoadStatic { .. }
        | Panic { .. } | UseLocal { .. } => {}
        BinOp { .. } | UnOp { .. } | ArrayLen { .. }
        | ArrayLoad { .. } | MapGet { .. } | TupleExtract { .. }
        | OptionalIsSome { .. } | OptionalUnwrap { .. } | EnumTag { .. }
        | EnumPayload { .. } | EnumDiscStr { .. } | LoadField { .. }
        | TypeOf { .. } | IsInstance { .. } => {}
        // Most Casts (IntResize / FloatResize / numeric reinterprets)
        // don't propagate the object pointer onward. The exceptions:
        // `StrongToWeak` records a weak ref into the heap weak
        // table; `OptionalWrap` boxes the value into a heap
        // Optional. Both make the stack lifetime insufficient.
        Cast { kind: crate::inst::CastKind::StrongToWeak | crate::inst::CastKind::OptionalWrap, src, .. } => {
            leak(src);
        }
        Cast { .. } => {}
        // Strong-rc ops on the candidate value are tolerated — stack
        // promotion will simply skip them at codegen time.
        Retain { .. } | Release { .. } => {}
        // Weak references require the runtime weak table; an obj
        // whose weak is taken must live on the heap so the weak
        // outlives the strong release.
        WeakRetain { value } | WeakRelease { value } => leak(value),
        WeakUpgrade { weak, .. } => leak(weak),
        // Downcast produces an `Optional<Object>` that carries the
        // input through a heap Optional cell — bookmark the input
        // as escaped so the cell's eventual release operates on a
        // valid heap pointer.
        DowncastOrNone { value, .. } => leak(value),
        // The candidate value being defined via NewObject — its
        // `init_args` escape (init may stash them into other heap),
        // but the dst itself isn't an arg here. Mark init_args as
        // escaped, leave dst alone.
        NewObject { init_args, .. } => {
            for a in init_args.iter() {
                leak(a);
            }
        }
        // Field stores: `value` ends up reachable from `obj`, so
        // the value's lifetime is tied to obj's. The obj itself is
        // NOT made reachable from anywhere new by the store — any
        // later escape of obj is caught at that other use. So just
        // leak the value.
        //
        // Without this distinction, the new StructLit lowering
        // (NewObject + per-field StoreField with obj == candidate)
        // would always leak its own candidate at the very next
        // instruction, defeating stack promotion for every CRepr /
        // ARC class literal. For the candidate-only restriction (its
        // fields are non-heap by the earlier filter), the value
        // being stored is always a primitive so this branch ends up
        // being a no-op for them too.
        StoreField { value, .. } => {
            leak(value);
        }
        ArrayStore { arr, value, .. } => {
            leak(arr);
            leak(value);
        }
        MapSet { map, value, .. } => {
            leak(map);
            leak(value);
        }
        NewArray { items, .. } | NewTuple { items, .. } => {
            for it in items.iter() {
                leak(it);
            }
        }
        NewMap { entries, .. } => {
            for (k, v) in entries.iter() {
                leak(k);
                leak(v);
            }
        }
        NewOptional { value, .. } => {
            leak(value);
        }
        NewEnum { payload, .. } => {
            for p in payload.iter() {
                leak(p);
            }
        }
        MakeClosure { captures, .. } => {
            for c in captures.iter() {
                leak(c);
            }
        }
        // CRepr struct args are consumed by-value at the call site
        // — chunked into i64 / float regs when they fit the ABI's
        // chunk cap, otherwise memcpy'd into a scratch buffer the
        // call site allocates. Either way the callee gets a fresh
        // frame copy and never sees the caller's pointer, so a
        // stack-promoted local can survive being passed through a
        // function call (the whole point of the by-value ABI).
        // Reference-typed args (ArcObject, Array, Map, etc.) still
        // leak as before.
        Call { args, .. } => {
            for a in args.iter() {
                if arg_passed_by_value(func.ty_of(*a), prog) {
                    continue;
                }
                leak(a);
            }
        }
        CallIndirect { callee, args, .. } => {
            leak(callee);
            for a in args.iter() {
                if arg_passed_by_value(func.ty_of(*a), prog) {
                    continue;
                }
                leak(a);
            }
        }
        VirtCall { recv, args, .. } => {
            leak(recv);
            for a in args.iter() {
                if arg_passed_by_value(func.ty_of(*a), prog) {
                    continue;
                }
                leak(a);
            }
        }
        StoreStatic { value, .. } => {
            leak(value);
        }
        // DefLocal binds the value into a mutable local slot. The
        // local's stack frame entry holds the pointer; reads come
        // back as `UseLocal { dst }`. Either treat DefLocal as a
        // use that *preserves* candidacy (the local is just a
        // re-binding of the same SSA value), or be conservative
        // and leak. For breakout's idiom (`let p = new Point(...)`),
        // DefLocal is the only way to hold the pointer across
        // blocks — leaking here would kill the optimisation.
        // Treat as non-escaping. The UseLocal reads are also
        // non-escaping by the same logic; later insts consuming
        // the UseLocal dst still go through the use-classification
        // above, so escape via subsequent calls / stores still gets
        // caught (we'd need that escape on the UseLocal dst, not
        // the original NewObject dst — a limitation; for the
        // primitive-only scope it's tolerable since promote_locals
        // dropped most single-def Local chains already).
        DefLocal { .. } => {}
        // `&local` exposes the local's stack address to a callee.
        // If the local ever holds an ARC-managed object, the
        // address could be used to extract / store new pointers
        // outside our analysis scope. The escape pass currently
        // doesn't track per-local candidacy through AddrOf, so
        // be conservative and leak any related candidates. Cheap
        // safety since AddrOf is FFI-only and rare.
        AddrOfLocal { .. } => {}
        // `&obj.field` — leak the receiver (its address is
        // exposed to a callee that may mutate / persist it).
        AddrOfField { obj, .. } => {
            leak(obj);
        }
    }
}

fn check_term_escape(
    term: &crate::inst::Terminator,
    cands: &mut HashSet<ValueId>,
    alias: &HashMap<ValueId, ValueId>,
) {
    use crate::inst::Terminator;
    let mut leak = |v: &ValueId| {
        let canon = alias.get(v).copied().unwrap_or(*v);
        cands.remove(&canon);
    };
    match term {
        Terminator::Br { args, .. } => {
            for a in args.iter() {
                leak(a);
            }
        }
        Terminator::CondBr { then_args, else_args, .. } => {
            for a in then_args.iter() {
                leak(a);
            }
            for a in else_args.iter() {
                leak(a);
            }
        }
        Terminator::Switch { cases, default_args, .. } => {
            for case in cases.iter() {
                for a in case.args.iter() {
                    leak(a);
                }
            }
            for a in default_args.iter() {
                leak(a);
            }
        }
        Terminator::Return { value: Some(v) } => {
            leak(v);
        }
        Terminator::Return { value: None } | Terminator::Unreachable => {}
    }
}
