//! Shared helpers used by multiple MIR optimisation passes.
//!
//! The remap utilities visit every `ValueId` reference inside an
//! instruction or terminator with a caller-supplied closure. Both
//! `inline` and `promote_locals` need this; centralising the match
//! arms means one place to maintain when new MIR opcodes land.

use std::collections::HashMap;

use crate::inst::{Inst, Terminator, ValueId};

/// Visit every `ValueId` field of an instruction with a remapping
/// callback. The closure can rename uses, defs, or both — passes
/// decide which behaviour they want; this helper just walks the
/// shape. Tracks every `Inst` variant so additions show up here as
/// a compile error.
pub fn remap_inst(inst: &mut Inst, mut remap: impl FnMut(&mut ValueId)) {
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
        CallRawIndirect { dst, callee, args, .. } => {
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
        ComCall { dst, recv, args, .. } => {
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
        NewSimd { dst, lanes } => {
            remap(dst);
            for it in lanes.iter_mut() {
                remap(it);
            }
        }
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
        FuncAddr { dst, .. } => remap(dst),
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
        AddrOfLocal { dst, .. } => remap(dst),
        AddrOfField { dst, obj, .. } => {
            remap(dst);
            remap(obj);
        }
    }
}

/// Apply a `ValueId` rename map to every value reference in a
/// terminator (branch args, condition, scrutinee, return value).
pub fn remap_terminator(term: &mut Terminator, rename: &HashMap<ValueId, ValueId>) {
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
