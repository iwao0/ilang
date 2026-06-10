//! ABI helpers: building `Signature` for an ilang function under
//! its calling convention (host-form / ilang-form / `@extern(C)`),
//! CRepr struct passing rules (chunked / HFA / indirect), and
//! per-element clif type / stride utilities for arrays + raw fields.
//!
//! ## CRepr by-value byte thresholds
//!
//! Two thresholds bound how big a CRepr struct can be before the
//! ABI switches from "chunk it into i64 registers" to "pass a
//! pointer to a caller-side scratch copy". They differ by ABI:
//!
//! - **C ABI** (real C library / `@extern(C) @lib(...)` callees):
//!   fixed at 16 bytes by the AArch64 AAPCS64 / x86_64 SysV
//!   "≤2 integer regs" rule. Changing it would break interop
//!   with every C function that takes a struct by value.
//! - **ilang ABI** (`Local` / `ExternBody` callees): set higher
//!   to keep moderate-sized structs in registers across ilang
//!   call boundaries. This is purely a perf/codegen knob — both
//!   sides of the call boundary use the same value, so any
//!   number is sound; tune it where the chunk vs. memcpy
//!   tradeoff makes sense for the workload.
//!
//! Sret (indirect return) uses the same threshold as the
//! corresponding "chunkable" path on the same side, so any return
//! that doesn't fit in chunks goes through a hidden pointer the
//! caller pre-allocated.

use cranelift::prelude::*;
use cranelift_codegen::ir::{AbiParam, Signature};
use cranelift_frontend::FunctionBuilder as ClifFnBuilder;
use cranelift_module::Module;

use ilang_mir::{FunctionKind, Function as MirFunction, MirTy, Program};

use crate::ty::mir_to_clif;
use super::CompileError;

/// C SysV / AArch64 AAPCS64 "≤2 integer registers" rule. Fixed
/// by the platform ABI — do not change.
pub(super) const C_BYVAL_CHUNK_MAX: i64 = 16;

/// Cutoff for ilang's internal by-value calling convention.
/// Structs up to this many bytes are passed as i64 chunks in
/// registers; larger ones go through a caller-side memcpy into a
/// scratch StackSlot whose pointer is then handed off. Tunable.
pub(super) const IL_BYVAL_CHUNK_MAX: i64 = 64;

/// Whether `f` follows the C ABI (real C functions, callbacks
/// exposed under C ABI) vs the looser ilang ABI. The two differ
/// only in the by-value chunk threshold *and* whether internal-fn
/// CRepr struct returns are forced through sret (only the looser
/// ilang ABI is — C interop must stay platform-conformant).
pub(super) fn is_c_abi(f: &MirFunction) -> bool {
    matches!(
        f.kind,
        FunctionKind::Extern { .. } | FunctionKind::ExternBody
    )
}

/// Picks the right by-value chunk cap for the given callee: C
/// functions stay at the platform-fixed 16 B; everything else
/// uses the larger ilang cap. Call sites that already know the
/// callee's `FunctionKind` should use this so the chunk schema
/// matches what `clif_signature_for` declared.
pub(super) fn chunk_max_for(f: &MirFunction) -> i64 {
    if is_c_abi(f) { C_BYVAL_CHUNK_MAX } else { IL_BYVAL_CHUNK_MAX }
}

/// The chunk cap that applies to a struct *return*. Unlike
/// arguments (which can spill onto the stack when they overflow the
/// GPRs), return values must fit in the ABI's return registers:
///
/// - **x86_64 System V** returns integer aggregates in at most two
///   GPRs (rax:rdx) — 16 bytes. Anything larger must come back
///   through an sret hidden pointer, so the return cap is clamped to
///   16 B regardless of the looser ilang argument cap. Without this,
///   a 32 B ilang-ABI struct return asks Cranelift for four return
///   registers and fails with "Too many return values to fit in
///   registers" (#9510).
/// - **AArch64 AAPCS64 / Win64** let Cranelift spread the ilang-ABI
///   return across more registers, so they keep the full cap.
///
/// (Float HFAs return in v0..v3 on every target and are decided
/// before this cap is consulted, so they're unaffected.)
pub(super) fn ret_chunk_max(
    call_conv: cranelift_codegen::isa::CallConv,
    chunk_max: i64,
) -> i64 {
    if call_conv == cranelift_codegen::isa::CallConv::SystemV {
        chunk_max.min(C_BYVAL_CHUNK_MAX)
    } else {
        chunk_max
    }
}

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
    // CRepr / CPacked / CUnion params and returns use the by-value
    // rules — chunked into i64 GPRs, HFA float regs, or an indirect
    // sret return. The chunk-vs-memcpy threshold differs by ABI:
    // C ABI is fixed at 16 B by the platform spec; ilang ABI uses
    // the higher `IL_BYVAL_CHUNK_MAX`. ArcObject / Array / etc.
    // stay pointer-typed (reference semantics).
    let chunk_max = if is_c_abi(f) { C_BYVAL_CHUNK_MAX } else { IL_BYVAL_CHUNK_MAX };
    // HFA (spreading a float struct across multiple float return
    // registers) only works on System V AMD64 and AArch64 AAPCS64.
    // Windows fastcall allows only one return-value register, so the
    // HFA path is disabled there — float structs fall through to the
    // i64 chunks path instead (1 i64 per 8 bytes, bit-packed).
    let hfa_ok = sig.call_conv != cranelift_codegen::isa::CallConv::WindowsFastcall;
    // ABI decision tree for the return slot: HFA float regs first
    // (4 floats fit even when c_size > chunk_max — e.g. NSRect is
    // 32 bytes but rides v0..v3), then GPR chunks, then indirect
    // sret. Without checking HFA first, NSRect-returning ObjC
    // selectors (`-[NSScreen frame]` etc.) silently sret-pre-alloc
    // a buffer the callee never writes to, leaving zeros.
    // Returns are register-bound (no stack spill), so they use a
    // possibly-tighter cap than arguments — see `ret_chunk_max` /
    // `struct_hfa_ret`.
    let ret_max = ret_chunk_max(sig.call_conv, chunk_max);
    // Internal (non-extern) fns force every CRepr struct return
    // through sret so the callee can hand its buffer to the caller's
    // pre-alloc'd slot and free it at the return site. The chunk /
    // HFA paths would leave the callee buffer leaked (see HANDOFF
    // notes on `crepr_struct_field_discard.il`).
    let force_internal_sret = !is_c_abi(f);
    let ret_hfa = if force_internal_sret {
        None
    } else {
        struct_hfa_ret(&f.ret, prog, sig.call_conv)
    };
    let sret_size = if ret_hfa.is_some() {
        None
    } else if force_internal_sret {
        struct_sret_for_internal(&f.ret, prog)
    } else {
        struct_indirect_with_max(&f.ret, prog, ret_max)
    };
    if sret_size.is_some() {
        sig.params.push(AbiParam::special(
            types::I64,
            cranelift_codegen::ir::ArgumentPurpose::StructReturn,
        ));
    }
    for p in f.params.iter() {
        if hfa_ok {
            if let Some((elem_ct, count)) = struct_hfa(&p.ty, prog) {
                for _ in 0..count {
                    sig.params.push(AbiParam::new(elem_ct));
                }
                continue;
            }
        }
        if let Some(chunks) = struct_chunks_with_max(&p.ty, prog, chunk_max) {
            for _ in 0..chunks {
                sig.params.push(AbiParam::new(types::I64));
            }
            continue;
        }
        // CRepr over the chunk cap (neither HFA nor chunkable). How
        // the aggregate travels depends on the ABI:
        //
        // - **x86_64 System V** classifies a >16 B aggregate as
        //   MEMORY and copies its bytes onto the stack at the call —
        //   there is *no* pointer indirection. Cranelift models this
        //   with the `StructArgument` purpose (caller passes a pointer
        //   to the source bytes, Cranelift copies them into the
        //   outgoing stack slot). Without this the struct would be
        //   handed over as a bare pointer in a register, which the C
        //   callee reads as garbage stack bytes.
        // - **AArch64 AAPCS64 / Win64** pass the same aggregate as a
        //   pointer to a caller-side copy, which is exactly the plain
        //   i64 pointer path below (the call site memcpys into a
        //   scratch buffer first). Cranelift also doesn't implement
        //   `StructArgument` on AArch64, so that path stays manual.
        if sig.call_conv == cranelift_codegen::isa::CallConv::SystemV {
            if let Some(size) = struct_byval_size_with_max(&p.ty, prog, chunk_max) {
                sig.params.push(AbiParam::special(
                    types::I64,
                    cranelift_codegen::ir::ArgumentPurpose::StructArgument(size as u32),
                ));
                continue;
            }
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
        if let Some((elem_ct, count)) = ret_hfa {
            for _ in 0..count {
                sig.returns.push(AbiParam::new(elem_ct));
            }
            return Ok(sig);
        }
        if let Some(chunks) = struct_chunks_with_max(&f.ret, prog, ret_max) {
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

/// Number of i64 chunk slots to pass `ty` in, capped at the given
/// `max_bytes`. CRepr / CPacked structs ≤ `max_bytes` get
/// `ceil(c_size / 8)` slots; everything else (>max_bytes, non-CRepr,
/// non-Object) returns `None` so the caller falls back to either the
/// memcpy-scratch path or plain pointer passing.
pub(super) fn struct_chunks_with_max(
    ty: &MirTy,
    prog: &Program,
    max_bytes: i64,
) -> Option<usize> {
    if let MirTy::Object(cid) = ty {
        let layout = &prog.classes[cid.0 as usize];
        if matches!(
            layout.repr,
            ilang_mir::ClassRepr::CRepr | ilang_mir::ClassRepr::CPacked
        ) && layout.c_size > 0
            && layout.c_size <= max_bytes
        {
            // Round up to 8-byte cells: a 12 B struct rides in 2
            // i64 chunks, a 24 B struct in 3, etc.
            let chunks = ((layout.c_size + 7) / 8) as usize;
            return Some(chunks);
        }
    }
    None
}

/// HFA detection (AArch64 AAPCS64 / x86_64 SysV "homogeneous
/// floating-point aggregate"): 1–4 fields, all the same float type.
/// Returns `Some((elem_clif_type, count))` so the caller can push a
/// matching `AbiParam(F32|F64)` per element.
pub(super) fn struct_hfa(ty: &MirTy, prog: &Program) -> Option<(cranelift::prelude::Type, usize)> {
    if !matches!(ty, MirTy::Object(_)) {
        return None;
    }
    let mut floats: Vec<cranelift::prelude::Type> = Vec::new();
    if !flatten_hfa_floats(ty, prog, &mut floats) {
        return None;
    }
    if floats.is_empty() || floats.len() > 4 {
        return None;
    }
    let first = floats[0];
    if floats.iter().all(|t| *t == first) {
        Some((first, floats.len()))
    } else {
        None
    }
}

/// HFA for a *return* value, honoring the target's float return-register
/// budget. Unlike arguments (8 SSE / v registers to spread across), a
/// float aggregate that comes back in registers is limited on the
/// return path:
///
/// - **x86_64 System V** returns floats in xmm0:xmm1 — at most 2. A 3-
///   or 4-float aggregate exceeds that and must come back via sret, so
///   this returns `None` and the caller falls through to the chunk /
///   sret path. (AArch64's 4-float v0..v3 budget is unaffected.)
/// - **Windows fastcall** has no HFA return path at all.
///
/// Returning `None` here keeps the return shape consistent across the
/// signature, the callee's `ret` lowering, and the caller's result
/// reconstruction — they all consult this same helper.
pub(super) fn struct_hfa_ret(
    ty: &MirTy,
    prog: &Program,
    call_conv: cranelift_codegen::isa::CallConv,
) -> Option<(cranelift::prelude::Type, usize)> {
    if call_conv == cranelift_codegen::isa::CallConv::WindowsFastcall {
        return None;
    }
    let (ct, count) = struct_hfa(ty, prog)?;
    let max_floats = if cfg!(target_arch = "x86_64") { 2 } else { 4 };
    if count <= max_floats {
        Some((ct, count))
    } else {
        None
    }
}

/// Recursively collect every float leaf of a CRepr struct so
/// HFA detection can see through nested geometry types like
/// `NSRect { origin: NSPoint, size: NSSize }`. Returns `false`
/// the moment a non-float / non-CRepr / non-fit element is hit
/// so callers can short-circuit.
fn flatten_hfa_floats(
    ty: &MirTy,
    prog: &Program,
    out: &mut Vec<cranelift::prelude::Type>,
) -> bool {
    use cranelift::prelude::types as ct;
    if out.len() > 4 {
        return false;
    }
    match ty {
        MirTy::F32 => {
            out.push(ct::F32);
            true
        }
        MirTy::F64 => {
            out.push(ct::F64);
            true
        }
        MirTy::Object(cid) => {
            let layout = &prog.classes[cid.0 as usize];
            if !matches!(layout.repr, ilang_mir::ClassRepr::CRepr) {
                return false;
            }
            if layout.fields.is_empty() {
                return false;
            }
            for f in &layout.fields {
                if !flatten_hfa_floats(&f.ty, prog, out) {
                    return false;
                }
            }
            true
        }
        _ => false,
    }
}

/// `Some(c_size)` for a CRepr struct / union / packed that's
/// bigger than the given `max_bytes` chunk cap — these don't fit
/// in the chunk path so the call site must memcpy the bytes into
/// a scratch buffer and pass that buffer's pointer. Returns `None`
/// for types that DO fit in chunks (or HFA float regs) and for
/// non-CRepr Object types (which stay reference-typed).
pub(super) fn struct_byval_size_with_max(
    ty: &MirTy,
    prog: &Program,
    max_bytes: i64,
) -> Option<i64> {
    if let MirTy::Object(cid) = ty {
        let layout = &prog.classes[cid.0 as usize];
        if matches!(
            layout.repr,
            ilang_mir::ClassRepr::CRepr
                | ilang_mir::ClassRepr::CPacked
                | ilang_mir::ClassRepr::CUnion
        ) && layout.c_size > max_bytes
            && struct_hfa(ty, prog).is_none()
        {
            return Some(layout.c_size);
        }
    }
    None
}

/// Sret hidden-pointer return for *every* CRepr struct / packed
/// returned by an ilang-internal fn (`is_extern == false`). The C
/// ABI's chunk / HFA paths are skipped here so callee-allocated
/// CRepr buffers can be handed to the caller's sret slot and freed
/// at the same site — chunk-return path leaks the callee buffer
/// because the chunks-load happens after `release_top_scope_objects`
/// has already moved past the local. C-extern callees still go
/// through the by-size chunk → HFA → sret cascade so SDL2 / wgpu /
/// objc_msgSend value-returning APIs keep their platform ABI.
pub(super) fn struct_sret_for_internal(
    ty: &MirTy,
    prog: &Program,
) -> Option<i64> {
    if let MirTy::Object(cid) = ty {
        let layout = &prog.classes[cid.0 as usize];
        if matches!(
            layout.repr,
            ilang_mir::ClassRepr::CRepr | ilang_mir::ClassRepr::CPacked
        ) && layout.c_size > 0
        {
            return Some(layout.c_size);
        }
    }
    None
}

/// Sret hidden-pointer return for any CRepr struct / packed whose
/// bytes overflow the given `max_bytes` chunk cap on the return
/// side. Returns `Some(c_size)` to size the caller's pre-allocated
/// destination buffer.
pub(super) fn struct_indirect_with_max(
    ty: &MirTy,
    prog: &Program,
    max_bytes: i64,
) -> Option<i64> {
    if let MirTy::Object(cid) = ty {
        let layout = &prog.classes[cid.0 as usize];
        if matches!(
            layout.repr,
            ilang_mir::ClassRepr::CRepr | ilang_mir::ClassRepr::CPacked
        ) && layout.c_size > max_bytes
        {
            return Some(layout.c_size);
        }
    }
    None
}

pub(super) fn elem_byte_stride(t: &MirTy) -> i64 {
    // Single source of truth lives on `MirTy` so the runtime
    // higher-order helpers (which take a stride argument) and the
    // codegen load/store paths can't drift apart.
    t.elem_byte_stride()
}

/// `Some(c_size)` if `t` is a CRepr / CPacked / CUnion class
/// (`@extern(C) struct` and plain top-level `struct` both count —
/// `ClassLayout::repr` is the discriminator). Used by array
/// codegen to pick an inline stride and a no-ARC kind tag, so
/// dynamic `T[]` arrays of CRepr structs match the byte layout
/// C expects for `Elem buf[N]`.
pub(super) fn crepr_struct_c_size(t: &MirTy, classes: &[ilang_mir::ClassLayout]) -> Option<i64> {
    if let MirTy::Object(cid) = t {
        let layout = &classes[cid.0 as usize];
        if matches!(
            layout.repr,
            ilang_mir::ClassRepr::CRepr
                | ilang_mir::ClassRepr::CPacked
                | ilang_mir::ClassRepr::CUnion
        ) && layout.c_size > 0
        {
            return Some(layout.c_size);
        }
    }
    None
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
        // SIMD lanes pack tightly; the cranelift vector type carries
        // the full `lanes × lane_bytes` width so the array NewArray
        // store / ArrayLoad load both hit the natural NEON D/Q-reg
        // path instead of falling through to the i64 catch-all.
        MirTy::Simd { .. } => crate::ty::mir_to_clif(t),
        _ => None,
    }
}

/// Cranelift scalar type for a single SIMD lane. Unlike
/// `elem_clif_type`, this covers `i64` / `u64` lanes (used by
/// `simd.i64x2`) — those are deliberately left out of the array-cell
/// path because i64 fields elsewhere use the `i64`-cell catch-all,
/// but they're valid lane scalars in their own right.
pub(super) fn simd_lane_clif_type(
    t: &MirTy,
) -> Option<cranelift::prelude::Type> {
    use cranelift::prelude::types as ct;
    match t {
        MirTy::I64 | MirTy::U64 => Some(ct::I64),
        _ => elem_clif_type(t),
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
