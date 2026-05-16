//! ABI helpers: building `Signature` for an ilang function under
//! its calling convention (host-form / ilang-form / `@extern(C)`),
//! CRepr struct passing rules (chunked / HFA / indirect), and
//! per-element clif type / stride utilities for arrays + raw fields.

use cranelift::prelude::*;
use cranelift_codegen::ir::{AbiParam, Signature};
use cranelift_frontend::FunctionBuilder as ClifFnBuilder;
use cranelift_module::Module;

use ilang_mir::{Function as MirFunction, MirTy, Program};

use crate::ty::mir_to_clif;
use super::CompileError;

/// Build a Cranelift `Signature` matching the calling convention
/// for `f`:
///
/// - `@extern(C)` fns use the C ABI, with CRepr struct args either
///   chunked into 1–2 i64 GPRs, spread across HFA float regs, or
///   passed indirectly through an `sret` hidden first param.
/// - Other ilang fns get a uniform "i64 cells + trailing hidden
///   env-pointer" shape so closures and direct calls share one
///   layout.
pub(super) fn clif_signature_for<M: Module>(
    module: &M,
    f: &MirFunction,
    prog: &Program,
) -> Result<Signature, CompileError> {
    let mut sig = module.make_signature();
    let is_extern = matches!(f.kind, ilang_mir::FunctionKind::Extern { .. });
    // CRepr / CPacked / CUnion params and returns ALWAYS use the
    // by-value rules — chunked (≤16 B → 1-2 i64), HFA (≤4 same float),
    // or indirect sret (>16 B). This was previously gated on
    // `is_extern` (i.e. only `@extern(C)` fns got it), forcing every
    // ilang→ilang call that passed a struct to fall back to pointer
    // passing — which in turn defeated stack promotion since
    // pointer-passing makes the value escape to the callee. Applying
    // the same rules across all function kinds gives true value
    // semantics: the callee gets its own copy in registers / its own
    // frame slot, and the caller's stack-promoted struct survives
    // the call unharmed.
    //
    // ArcObject params / returns stay pointer-typed (they're
    // reference types — sharing the pointer IS the semantics).
    let sret_size = struct_indirect(&f.ret, prog);
    if sret_size.is_some() {
        sig.params.push(AbiParam::special(
            types::I64,
            cranelift_codegen::ir::ArgumentPurpose::StructReturn,
        ));
    }
    for p in f.params.iter() {
        if let Some((elem_ct, count)) = struct_hfa(&p.ty, prog) {
            for _ in 0..count {
                sig.params.push(AbiParam::new(elem_ct));
            }
            continue;
        }
        if let Some(chunks) = struct_chunks(&p.ty, prog) {
            for _ in 0..chunks {
                sig.params.push(AbiParam::new(types::I64));
            }
            continue;
        }
        if let Some(ct) = mir_to_clif(&p.ty) {
            sig.params.push(AbiParam::new(ct));
        } else {
            return Err(CompileError::Unsupported("unit / void params"));
        }
    }
    if !is_extern {
        sig.params.push(AbiParam::new(types::I64));
    }
    if sret_size.is_some() {
        // sret: no clif-level return value; the caller's hidden
        // pointer receives the bytes.
        return Ok(sig);
    }
    if !matches!(f.ret, MirTy::Unit) {
        if let Some((elem_ct, count)) = struct_hfa(&f.ret, prog) {
            for _ in 0..count {
                sig.returns.push(AbiParam::new(elem_ct));
            }
            return Ok(sig);
        }
        if let Some(chunks) = struct_chunks(&f.ret, prog) {
            for _ in 0..chunks {
                sig.returns.push(AbiParam::new(types::I64));
            }
            return Ok(sig);
        }
        let ret = mir_to_clif(&f.ret)
            .ok_or(CompileError::Unsupported("unit return through ABI"))?;
        sig.returns.push(AbiParam::new(ret));
    }
    Ok(sig)
}

/// For an `@extern(C)` CRepr struct ≤ 16 B: returns `Some(chunks)`
/// where `chunks` is 1 or 2 i64 GPR slots. > 16 B / non-CRepr / non-
/// Object types return `None` (caller treats as pointer-sized i64).
pub(super) fn struct_chunks(ty: &MirTy, prog: &Program) -> Option<usize> {
    if let MirTy::Object(cid) = ty {
        let layout = &prog.classes[cid.0 as usize];
        if matches!(
            layout.repr,
            ilang_mir::ClassRepr::CRepr | ilang_mir::ClassRepr::CPacked
        ) {
            if layout.c_size <= 8 {
                return Some(1);
            }
            if layout.c_size <= 16 {
                return Some(2);
            }
        }
    }
    None
}

/// HFA detection (AArch64 AAPCS64 / x86_64 SysV "homogeneous
/// floating-point aggregate"): 1–4 fields, all the same float type.
/// Returns `Some((elem_clif_type, count))` so the caller can push a
/// matching `AbiParam(F32|F64)` per element.
pub(super) fn struct_hfa(ty: &MirTy, prog: &Program) -> Option<(cranelift::prelude::Type, usize)> {
    use cranelift::prelude::types as ct;
    if let MirTy::Object(cid) = ty {
        let layout = &prog.classes[cid.0 as usize];
        if !matches!(layout.repr, ilang_mir::ClassRepr::CRepr) {
            return None;
        }
        if layout.fields.is_empty() || layout.fields.len() > 4 {
            return None;
        }
        let mut clif_ty: Option<cranelift::prelude::Type> = None;
        for f in &layout.fields {
            let ct_for = match &f.ty {
                MirTy::F32 => ct::F32,
                MirTy::F64 => ct::F64,
                _ => return None,
            };
            match clif_ty {
                None => clif_ty = Some(ct_for),
                Some(prev) if prev != ct_for => return None,
                _ => {}
            }
        }
        return clif_ty.map(|c| (c, layout.fields.len()));
    }
    None
}

/// Larger CRepr structs (> 16 B) are returned through a hidden
/// pointer (`ArgumentPurpose::StructReturn`). Returns `Some(c_size)`
/// for those, `None` for chunkable / non-CRepr / non-Object types.
pub(super) fn struct_indirect(ty: &MirTy, prog: &Program) -> Option<i64> {
    if let MirTy::Object(cid) = ty {
        let layout = &prog.classes[cid.0 as usize];
        if matches!(
            layout.repr,
            ilang_mir::ClassRepr::CRepr | ilang_mir::ClassRepr::CPacked
        ) && layout.c_size > 16
        {
            return Some(layout.c_size);
        }
    }
    None
}

pub(super) fn elem_byte_stride(t: &MirTy) -> i64 {
    match t {
        MirTy::I8 | MirTy::U8 | MirTy::CChar | MirTy::Bool => 1,
        MirTy::I16 | MirTy::U16 => 2,
        MirTy::I32 | MirTy::U32 | MirTy::F32 => 4,
        _ => 8,
    }
}

/// Cranelift type to use for a packed array load/store of `t`. Only
/// the small numeric types get tight packing; everything else uses
/// the i64 cell path (returns `None`).
pub(super) fn elem_clif_type(t: &MirTy) -> Option<cranelift::prelude::Type> {
    use cranelift::prelude::types as ct;
    match t {
        MirTy::I8 | MirTy::U8 | MirTy::CChar | MirTy::Bool => Some(ct::I8),
        MirTy::I16 | MirTy::U16 => Some(ct::I16),
        MirTy::I32 | MirTy::U32 => Some(ct::I32),
        MirTy::F32 => Some(ct::F32),
        MirTy::F64 => Some(ct::F64),
        _ => None,
    }
}

/// `elem_clif_type` extended to see through unit-only enums. A
/// `MirTy::Enum` is reduced to its underlying repr (`u8`/`u16`/
/// `u32`/`i32`/...) so CRepr struct fields typed against an enum
/// load/store at the right width — without this, the field falls
/// into the `i64` catch-all and reads/writes 8 bytes at the
/// (already u16-sized) offset, corrupting subsequent fields.
/// Payload-bearing enums stay opaque (they're heap pointers).
pub(super) fn celem_clif_type_with_enum(
    prog: &ilang_mir::Program,
    t: &MirTy,
) -> Option<cranelift::prelude::Type> {
    if let MirTy::Enum(eid) = t {
        let layout = &prog.enums[eid.0 as usize];
        let unit_only = layout
            .variants
            .iter()
            .all(|v| matches!(v.payload, ilang_mir::VariantPayload::Unit));
        if unit_only {
            return elem_clif_type(&layout.repr);
        }
    }
    elem_clif_type(t)
}

/// Truncate a Cranelift value to fit the target type if it is wider;
/// otherwise pass through (assumes the source already matches).
pub(super) fn ireduce_or_pass(
    fb: &mut ClifFnBuilder,
    v: cranelift::prelude::Value,
    target: cranelift::prelude::Type,
) -> cranelift::prelude::Value {
    let cur = fb.func.dfg.value_type(v);
    if cur == target {
        return v;
    }
    if target.is_int() && cur.is_int() {
        if cur.bits() > target.bits() {
            return fb.ins().ireduce(target, v);
        }
        if cur.bits() < target.bits() {
            return fb.ins().uextend(target, v);
        }
    }
    v
}

/// Bring a clif value up to i64 by sign/zero-extension or bitcast.
/// Used when storing a primitive into an i64-cell-shaped slot
/// (object field, array cell, static slot, optional payload).
pub(super) fn extend_to_i64(fb: &mut ClifFnBuilder, v: Value) -> Value {
    let ty = fb.func.dfg.value_type(v);
    if ty == types::I64 {
        v
    } else if ty == types::F64 {
        fb.ins().bitcast(types::I64, MemFlags::new(), v)
    } else if ty == types::F32 {
        let r32 = fb.ins().bitcast(types::I32, MemFlags::new(), v);
        fb.ins().uextend(types::I64, r32)
    } else {
        fb.ins().uextend(types::I64, v)
    }
}

/// Inverse of `extend_to_i64`: take an i64-cell value and produce
/// the right-sized clif value for `target_ty`.
pub(super) fn reduce_from_i64(fb: &mut ClifFnBuilder, target_ty: &MirTy, raw: Value) -> Value {
    match target_ty {
        MirTy::I64 | MirTy::U64 | MirTy::Size | MirTy::SSize => raw,
        MirTy::I32 | MirTy::U32 => fb.ins().ireduce(types::I32, raw),
        MirTy::I16 | MirTy::U16 => fb.ins().ireduce(types::I16, raw),
        MirTy::I8 | MirTy::U8 | MirTy::Bool | MirTy::CChar => fb.ins().ireduce(types::I8, raw),
        MirTy::F64 => fb.ins().bitcast(types::F64, MemFlags::new(), raw),
        MirTy::F32 => {
            let r32 = fb.ins().ireduce(types::I32, raw);
            fb.ins().bitcast(types::F32, MemFlags::new(), r32)
        }
        _ => raw,
    }
}
