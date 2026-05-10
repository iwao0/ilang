//! Ahead-of-time object file emission. M1 scope: a single function
//! whose body uses primitive arithmetic (`BinOp` / `UnOp`), local
//! bindings (`DefLocal` / `UseLocal`), and straight-line plus basic-
//! block control flow (`Br` / `CondBr` / `Return`). Heap types,
//! function calls, classes, enums, closures and ARC are out of scope
//! and rejected with a clean [`AotError::Unsupported`] error.
//!
//! The point of this layer is the `ilang build` end-to-end pipeline
//! (`ObjectModule` → linker → executable). Each iteration grows the
//! supported subset by handling more `Inst` / `Terminator` variants;
//! anything unhandled bails out instead of miscompiling.

use std::collections::HashMap;

use cranelift::prelude::*;
use cranelift_codegen::ir::{AbiParam, Function as ClifFunc, InstBuilder, UserFuncName};
use cranelift_codegen::settings;
use cranelift_frontend::{FunctionBuilder as ClifFnBuilder, FunctionBuilderContext, Variable};
use cranelift_module::{Linkage, Module};
use cranelift_object::{ObjectBuilder, ObjectModule};

use ilang_mir::{
    FuncId as MirFuncId, FuncRef, FunctionKind, Inst, MirConst, MirTy, Program, Terminator,
    UnOp, ValueId,
};

use crate::compile::lower_binop;
use crate::ty::mir_to_clif;

#[derive(Debug, thiserror::Error)]
pub enum AotError {
    #[error("AOT does not yet support: {0}")]
    Unsupported(String),
    #[error("{0}")]
    Other(String),
    #[error(transparent)]
    Module(#[from] cranelift_module::ModuleError),
}

/// Compile `prog` to a Mach-O / ELF / COFF object file (depending on
/// host) and return the raw bytes. The emitted module exports two
/// symbols: the lowered entry as `__ilang_main` and a C ABI `main`
/// wrapper that calls it and truncates the result to the process exit
/// code (i32).
pub fn compile_program_to_object(prog: &Program) -> Result<Vec<u8>, AotError> {
    let entry = &prog.functions[prog.entry.0 as usize];
    validate_subset(prog, entry)?;

    // Surface a clean error if the entry's return type can't fold to
    // an exit code. `build_signature` would catch this later anyway,
    // but throwing here produces a more pointed message.
    mir_to_clif(&entry.ret).ok_or_else(|| {
        AotError::Unsupported(format!("entry return type {:?}", entry.ret))
    })?;

    let isa_builder = cranelift_native::builder()
        .map_err(|e| AotError::Other(format!("cranelift_native: {e}")))?;
    let mut flag_builder = settings::builder();
    // ObjectModule requires PIC; the JIT path doesn't.
    flag_builder
        .set("is_pic", "true")
        .map_err(|e| AotError::Other(format!("set is_pic: {e}")))?;
    let isa = isa_builder
        .finish(settings::Flags::new(flag_builder))
        .map_err(|e| AotError::Other(format!("isa: {e}")))?;

    let builder = ObjectBuilder::new(
        isa,
        b"ilang_aot".to_vec(),
        cranelift_module::default_libcall_names(),
    )
    .map_err(|e| AotError::Other(format!("ObjectBuilder: {e}")))?;
    let mut module = ObjectModule::new(builder);

    // Declare every Local function up front so call sites resolve in
    // any order. The entry fn is exported under the stable internal
    // name `__ilang_main`; other fns keep their MIR-level mangled name
    // (already monomorphised).
    let mut fn_ids: HashMap<MirFuncId, cranelift_module::FuncId> =
        HashMap::with_capacity(prog.functions.len());
    let mut fn_sigs: HashMap<MirFuncId, cranelift_codegen::ir::Signature> =
        HashMap::with_capacity(prog.functions.len());
    for (idx, func) in prog.functions.iter().enumerate() {
        let mid = MirFuncId(idx as u32);
        let sig = build_signature(&module, func)?;
        let symbol_name = if mid == prog.entry {
            "__ilang_main"
        } else {
            func.name.as_str()
        };
        let cid = module.declare_function(symbol_name, Linkage::Local, &sig)?;
        fn_ids.insert(mid, cid);
        fn_sigs.insert(mid, sig);
    }

    let mut ctx = module.make_context();
    let mut fb_ctx = FunctionBuilderContext::new();
    for (idx, func) in prog.functions.iter().enumerate() {
        let mid = MirFuncId(idx as u32);
        let cid = fn_ids[&mid];
        let sig = fn_sigs[&mid].clone();
        ctx.func = ClifFunc::with_name_signature(
            UserFuncName::user(0, cid.as_u32()),
            sig,
        );
        {
            let mut fb = ClifFnBuilder::new(&mut ctx.func, &mut fb_ctx);
            lower_function_body(func, &mut fb, &mut module, &fn_ids)?;
            fb.finalize();
        }
        module.define_function(cid, &mut ctx).map_err(|e| {
            AotError::Other(format!(
                "define_function {}: {e:?}",
                func.name
            ))
        })?;
        module.clear_context(&mut ctx);
    }
    let entry_id = fn_ids[&prog.entry];

    // Emit the C ABI `main` wrapper. Cranelift names it via Linkage::Export
    // so the linker resolves the platform startup file's call to `_main`
    // / `main` against this symbol.
    let mut main_sig = module.make_signature();
    main_sig.returns.push(AbiParam::new(types::I32));
    let main_id = module.declare_function("main", Linkage::Export, &main_sig)?;
    ctx.func = ClifFunc::with_name_signature(
        UserFuncName::user(0, main_id.as_u32()),
        main_sig.clone(),
    );
    {
        let mut fb = ClifFnBuilder::new(&mut ctx.func, &mut fb_ctx);
        let block = fb.create_block();
        fb.switch_to_block(block);
        fb.seal_block(block);

        let entry_ref = module.declare_func_in_func(entry_id, fb.func);
        let call = fb.ins().call(entry_ref, &[]);
        // The entry's return value (any int / bool / f64) gets folded
        // into the i32 exit code. Bool widens to i32, ints reduce or
        // extend to i32, floats convert to int.
        let raw = fb.inst_results(call).first().copied();
        let ret32 = match raw {
            Some(v) => coerce_to_i32(&mut fb, v, &entry.ret),
            None => fb.ins().iconst(types::I32, 0),
        };
        fb.ins().return_(&[ret32]);
        fb.finalize();
    }
    module.define_function(main_id, &mut ctx).map_err(|e| {
        AotError::Other(format!("define_function main: {e:?}"))
    })?;

    let product = module.finish();
    product
        .emit()
        .map_err(|e| AotError::Other(format!("emit object: {e}")))
}

fn build_signature(
    module: &ObjectModule,
    func: &ilang_mir::Function,
) -> Result<cranelift_codegen::ir::Signature, AotError> {
    let mut sig = module.make_signature();
    for p in func.params.iter() {
        let ct = mir_to_clif(&p.ty).ok_or_else(|| {
            AotError::Unsupported(format!(
                "fn {} param {} type {:?}",
                func.name, p.name, p.ty
            ))
        })?;
        sig.params.push(AbiParam::new(ct));
    }
    if !matches!(func.ret, MirTy::Unit) {
        let ct = mir_to_clif(&func.ret).ok_or_else(|| {
            AotError::Unsupported(format!(
                "fn {} return type {:?}",
                func.name, func.ret
            ))
        })?;
        sig.returns.push(AbiParam::new(ct));
    }
    Ok(sig)
}

fn lower_function_body(
    entry: &ilang_mir::Function,
    fb: &mut ClifFnBuilder,
    module: &mut ObjectModule,
    fn_ids: &HashMap<MirFuncId, cranelift_module::FuncId>,
) -> Result<(), AotError> {
    // Allocate a clif Block per MIR block. The entry block carries the
    // function's params (matching `build_signature`); non-entry blocks
    // get their MIR-declared params, dropped if Unit-typed.
    let mut blocks: Vec<cranelift::prelude::Block> = Vec::with_capacity(entry.blocks.len());
    for (i, blk) in entry.blocks.iter().enumerate() {
        let b = fb.create_block();
        if i == entry.entry.0 as usize {
            for p in entry.params.iter() {
                if let Some(ct) = mir_to_clif(&p.ty) {
                    fb.append_block_param(b, ct);
                }
            }
        } else {
            for &p in &blk.params {
                let pty = entry.ty_of(p);
                if let Some(ct) = mir_to_clif(pty) {
                    fb.append_block_param(b, ct);
                }
            }
        }
        blocks.push(b);
    }

    // Declare a Cranelift Variable per MIR local. Locals carry across
    // blocks via Cranelift's SSA construction.
    // Locals with no clif counterpart (`unit`) get a sentinel `Variable`
    // that's never used. Mirroring the JIT path lets `DefLocal` /
    // `UseLocal` for unit-typed bindings no-op cleanly.
    let mut locals: Vec<Variable> = Vec::with_capacity(entry.local_tys.len());
    let mut local_has_clif: Vec<bool> = Vec::with_capacity(entry.local_tys.len());
    for ty in entry.local_tys.iter() {
        match mir_to_clif(ty) {
            Some(ct) => {
                locals.push(fb.declare_var(ct));
                local_has_clif.push(true);
            }
            None => {
                // Placeholder — never read because every code path that
                // would hit this var is also Unit-typed.
                locals.push(fb.declare_var(types::I8));
                local_has_clif.push(false);
            }
        }
    }

    // Lower each block. `vmap` is per-function (ValueIds are unique
    // across blocks); block params seed it with the block's clif args.
    let mut vmap: HashMap<ValueId, Value> = HashMap::new();
    for (i, blk) in entry.blocks.iter().enumerate() {
        let cb = blocks[i];
        fb.switch_to_block(cb);
        let cps = fb.block_params(cb).to_vec();
        let mut k = 0usize;
        if i == entry.entry.0 as usize {
            for p in entry.params.iter() {
                if mir_to_clif(&p.ty).is_some() {
                    vmap.insert(p.value, cps[k]);
                    k += 1;
                }
            }
        } else {
            for &p in &blk.params {
                let pty = entry.ty_of(p);
                if mir_to_clif(pty).is_some() {
                    vmap.insert(p, cps[k]);
                    k += 1;
                }
            }
        }
        for inst in &blk.insts {
            lower_inst_minimal(fb, inst, &mut vmap, &locals, &local_has_clif, entry, module, fn_ids)?;
        }
        lower_term(fb, &blk.term, &vmap, &blocks)?;
    }

    // Seal every block now that all predecessor edges are visited.
    for &b in &blocks {
        fb.seal_block(b);
    }
    Ok(())
}

fn lower_inst_minimal(
    fb: &mut ClifFnBuilder,
    inst: &Inst,
    vmap: &mut HashMap<ValueId, Value>,
    locals: &[Variable],
    local_has_clif: &[bool],
    func: &ilang_mir::Function,
    module: &mut ObjectModule,
    fn_ids: &HashMap<MirFuncId, cranelift_module::FuncId>,
) -> Result<(), AotError> {
    match inst {
        Inst::Const { dst, value } => {
            let ty = func.ty_of(*dst);
            // Unit values have no clif counterpart — leaving them out
            // of vmap matches the JIT path and lets terminators /
            // block-arg propagation skip them via filter_map.
            if matches!(ty, MirTy::Unit) || matches!(value, MirConst::Unit) {
                return Ok(());
            }
            let v = match value {
                MirConst::Int(n) => {
                    let ct = mir_to_clif(ty).ok_or_else(|| {
                        AotError::Unsupported(format!("Const(Int) target type {ty:?}"))
                    })?;
                    fb.ins().iconst(ct, *n)
                }
                MirConst::Bool(b) => {
                    fb.ins().iconst(types::I8, if *b { 1 } else { 0 })
                }
                MirConst::F64(bits) => {
                    fb.ins().f64const(f64::from_bits(*bits))
                }
                MirConst::F32(bits) => {
                    fb.ins().f32const(f32::from_bits(*bits))
                }
                _ => {
                    return Err(AotError::Unsupported(format!(
                        "Const variant {value:?}"
                    )));
                }
            };
            vmap.insert(*dst, v);
        }
        Inst::BinOp { dst, op, lhs, rhs } => {
            let lv = vmap[lhs];
            let rv = vmap[rhs];
            // Integer division by zero is left as undefined for now —
            // panic emission needs runtime symbols we don't link yet.
            let v = lower_binop(fb, *op, lv, rv);
            vmap.insert(*dst, v);
        }
        Inst::UnOp { dst, op, src } => {
            let sv = vmap[src];
            let v = match op {
                UnOp::INeg => fb.ins().ineg(sv),
                UnOp::FNeg => fb.ins().fneg(sv),
                UnOp::Not => fb.ins().bnot(sv),
                UnOp::BoolNot => {
                    let zero = fb.ins().iconst(types::I8, 0);
                    fb.ins().icmp(IntCC::Equal, sv, zero)
                }
            };
            vmap.insert(*dst, v);
        }
        Inst::DefLocal { local, value } => {
            // Unit-typed locals have no real clif slot — skip the
            // def_var so we don't try to fetch an absent vmap entry.
            if !local_has_clif[local.0 as usize] {
                return Ok(());
            }
            let var = locals[local.0 as usize];
            let v = vmap[value];
            fb.def_var(var, v);
        }
        Inst::UseLocal { dst, local } => {
            if !local_has_clif[local.0 as usize] {
                return Ok(());
            }
            let var = locals[local.0 as usize];
            let v = fb.use_var(var);
            vmap.insert(*dst, v);
        }
        Inst::Call { dst, callee, args } => {
            let mid = match callee {
                FuncRef::Local(id) => *id,
                FuncRef::Builtin(sym) => {
                    return Err(AotError::Unsupported(format!(
                        "builtin call `{}` (no AOT runtime symbols yet)",
                        sym
                    )));
                }
                FuncRef::Extern { sym, .. } => {
                    return Err(AotError::Unsupported(format!(
                        "@extern call `{}` (AOT extern dlopen is a follow-up)",
                        sym
                    )));
                }
            };
            let cid = *fn_ids.get(&mid).ok_or_else(|| {
                AotError::Other(format!("unknown callee FuncId({})", mid.0))
            })?;
            let fr = module.declare_func_in_func(cid, fb.func);
            let cargs: Vec<Value> = args
                .iter()
                .filter_map(|a| vmap.get(a).copied())
                .collect();
            let call = fb.ins().call(fr, &cargs);
            if let Some(d) = dst {
                let results = fb.inst_results(call);
                if let Some(r) = results.first().copied() {
                    vmap.insert(*d, r);
                }
            }
        }
        other => {
            return Err(AotError::Unsupported(format!(
                "instruction {other:?}"
            )));
        }
    }
    Ok(())
}

fn lower_term(
    fb: &mut ClifFnBuilder,
    term: &Terminator,
    vmap: &HashMap<ValueId, Value>,
    blocks: &[cranelift::prelude::Block],
) -> Result<(), AotError> {
    match term {
        Terminator::Return { value } => {
            match value.and_then(|v| vmap.get(&v).copied()) {
                Some(cv) => {
                    fb.ins().return_(&[cv]);
                }
                None => {
                    fb.ins().return_(&[]);
                }
            }
            Ok(())
        }
        Terminator::Br { dst, args } => {
            let cargs = visible_block_args(args, vmap);
            fb.ins().jump(blocks[dst.0 as usize], cargs.iter());
            Ok(())
        }
        Terminator::CondBr { cond, then_block, then_args, else_block, else_args } => {
            let c = vmap[cond];
            let ta = visible_block_args(then_args, vmap);
            let ea = visible_block_args(else_args, vmap);
            fb.ins().brif(
                c,
                blocks[then_block.0 as usize],
                ta.iter(),
                blocks[else_block.0 as usize],
                ea.iter(),
            );
            Ok(())
        }
        Terminator::Unreachable => {
            fb.ins().trap(TrapCode::user(0).expect("trap code 0"));
            Ok(())
        }
        Terminator::Switch { .. } => Err(AotError::Unsupported(
            "Switch terminator (use if/else for now)".into(),
        )),
    }
}

/// Turn MIR block arguments into Cranelift `BlockArg`s, dropping any
/// unit-typed values (no clif counterpart). Mirrors the JIT path's
/// `visible` helper so both backends see the same arg list.
fn visible_block_args(
    args: &[ValueId],
    vmap: &HashMap<ValueId, Value>,
) -> Vec<cranelift_codegen::ir::BlockArg> {
    args.iter()
        .filter_map(|a| vmap.get(a).copied().map(|v| v.into()))
        .collect()
}

/// Truncate / extend / convert the entry's return value to a process
/// exit code (i32). Bool widens, wider ints reduce, narrower ints
/// extend. Floats round toward zero.
fn coerce_to_i32(fb: &mut ClifFnBuilder, v: Value, ty: &MirTy) -> Value {
    let cur = fb.func.dfg.value_type(v);
    if cur == types::I32 {
        return v;
    }
    if cur.is_int() {
        let cur_bits = cur.bits();
        let dst_bits = types::I32.bits();
        if cur_bits < dst_bits {
            // Sub-i32 → i32. Sign-extend signed types, zero-extend
            // bool / unsigned. We classify by MirTy since clif int
            // types don't carry signedness.
            return if matches!(
                ty,
                MirTy::I8 | MirTy::I16 | MirTy::I32 | MirTy::I64
            ) {
                fb.ins().sextend(types::I32, v)
            } else {
                fb.ins().uextend(types::I32, v)
            };
        }
        return fb.ins().ireduce(types::I32, v);
    }
    if cur == types::F64 || cur == types::F32 {
        return fb.ins().fcvt_to_sint_sat(types::I32, v);
    }
    // Fall back: zero exit code for types we don't know how to fold.
    fb.ins().iconst(types::I32, 0)
}

fn validate_subset(
    prog: &Program,
    entry: &ilang_mir::Function,
) -> Result<(), AotError> {
    if !prog.classes.is_empty() {
        return Err(AotError::Unsupported(
            "classes — heap types not yet wired into AOT".into(),
        ));
    }
    if !prog.statics.is_empty() {
        return Err(AotError::Unsupported(
            "static slots — not yet wired into AOT".into(),
        ));
    }
    for f in prog.functions.iter() {
        if !matches!(f.kind, FunctionKind::Local) {
            return Err(AotError::Unsupported(format!(
                "fn {} kind {:?} — only Local functions are supported",
                f.name, f.kind
            )));
        }
        if f.closure_env.is_some() {
            return Err(AotError::Unsupported(format!(
                "closure capture in fn {} — not yet supported",
                f.name
            )));
        }
    }
    if !entry.params.is_empty() {
        return Err(AotError::Unsupported(
            "entry function with parameters (expected `() -> T`)".into(),
        ));
    }
    Ok(())
}
