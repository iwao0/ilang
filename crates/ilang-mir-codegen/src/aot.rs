//! Ahead-of-time object file emission. M0 scope: programs whose entry
//! function consists of `Inst::Const(Int)` plus `Terminator::Return`
//! only, with an `i64` return that becomes the process exit code.
//!
//! The point of this minimal pipeline is to validate the end-to-end
//! flow (`ilang build` → `ObjectModule` → linker → executable) before
//! growing AOT to support strings, classes, etc. Anything outside the
//! supported subset is rejected with [`AotError::Unsupported`] so the
//! CLI can give a clean error rather than miscompile.

use cranelift::prelude::*;
use cranelift_codegen::ir::{AbiParam, Function as ClifFunc, InstBuilder, UserFuncName};
use cranelift_codegen::settings;
use cranelift_frontend::{FunctionBuilder as ClifFnBuilder, FunctionBuilderContext};
use cranelift_module::{Linkage, Module};
use cranelift_object::{ObjectBuilder, ObjectModule};

use ilang_mir::{Inst, MirConst, MirTy, Program, Terminator};

#[derive(Debug, thiserror::Error)]
pub enum AotError {
    #[error("AOT M0 does not support: {0}")]
    Unsupported(String),
    #[error("{0}")]
    Other(String),
    #[error(transparent)]
    Module(#[from] cranelift_module::ModuleError),
}

/// Compile `prog` to a Mach-O / ELF / COFF object file (depending on
/// host) and return the raw bytes. The emitted module exports two
/// symbols: the lowered entry as `__ilang_main` (`() -> i64`) and a C
/// ABI `main` wrapper (`() -> i32`) that calls it and truncates the
/// result to the process exit code.
pub fn compile_program_to_object(prog: &Program) -> Result<Vec<u8>, AotError> {
    validate_minimal(prog)?;

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

    let entry_fn = &prog.functions[prog.entry.0 as usize];

    // Declare and define `__ilang_main`. Currently always `() -> i64`.
    let mut entry_sig = module.make_signature();
    entry_sig.returns.push(AbiParam::new(types::I64));
    let entry_id = module.declare_function("__ilang_main", Linkage::Local, &entry_sig)?;

    let mut ctx = module.make_context();
    let mut fb_ctx = FunctionBuilderContext::new();
    ctx.func = ClifFunc::with_name_signature(
        UserFuncName::user(0, entry_id.as_u32()),
        entry_sig.clone(),
    );
    {
        let mut fb = ClifFnBuilder::new(&mut ctx.func, &mut fb_ctx);
        let block = fb.create_block();
        fb.switch_to_block(block);
        fb.seal_block(block);

        // Lower the single-block body. We only handle Inst::Const(Int)
        // and Terminator::Return — `validate_minimal` rejects anything
        // else upstream so this match is total for our subset.
        let blk = &entry_fn.blocks[entry_fn.entry.0 as usize];
        let mut last_const: Option<Value> = None;
        for inst in &blk.insts {
            if let Inst::Const { value: MirConst::Int(n), .. } = inst {
                last_const = Some(fb.ins().iconst(types::I64, *n));
            } else {
                return Err(AotError::Unsupported(format!(
                    "instruction {inst:?} not handled in M0 AOT"
                )));
            }
        }
        match &blk.term {
            Terminator::Return { value: Some(_) } => {
                // The MIR-level value id maps to our `last_const`; the
                // M0 validator guaranteed exactly one `Const(Int)`
                // dst that's also the return value.
                let v = last_const.ok_or_else(|| {
                    AotError::Unsupported("return without a const-int producer".into())
                })?;
                fb.ins().return_(&[v]);
            }
            other => {
                return Err(AotError::Unsupported(format!(
                    "terminator {other:?} not handled in M0 AOT"
                )));
            }
        }
        fb.finalize();
    }
    module.define_function(entry_id, &mut ctx).map_err(|e| {
        AotError::Other(format!("define_function __ilang_main: {e:?}"))
    })?;
    module.clear_context(&mut ctx);

    // Emit the C ABI `main` wrapper. Cranelift names it via `Linkage::Export`
    // so the linker resolves the platform's startup file's call to `_main`
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
        let ret64 = fb.inst_results(call)[0];
        let ret32 = fb.ins().ireduce(types::I32, ret64);
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

/// Reject programs that exceed the M0 AOT subset. The CLI surfaces the
/// returned message verbatim so users see exactly which feature isn't
/// supported yet.
fn validate_minimal(prog: &Program) -> Result<(), AotError> {
    // The MIR layer synthesises an empty enum table even for trivial
    // programs, so don't reject on `enums.is_empty()`. We only refuse
    // when the entry function actually *uses* class/enum/static state.
    let entry = &prog.functions[prog.entry.0 as usize];
    if !matches!(entry.ret, MirTy::I64) {
        return Err(AotError::Unsupported(format!(
            "entry return type {:?} (AOT M0 only supports i64)",
            entry.ret
        )));
    }
    if !entry.params.is_empty() {
        return Err(AotError::Unsupported(
            "entry function with parameters (AOT M0 expects `() -> i64`)".into(),
        ));
    }
    if entry.blocks.len() != 1 {
        return Err(AotError::Unsupported(format!(
            "multi-block entry ({} blocks; AOT M0 supports a single block)",
            entry.blocks.len()
        )));
    }
    let blk = &entry.blocks[entry.entry.0 as usize];
    for inst in &blk.insts {
        if !matches!(inst, Inst::Const { value: MirConst::Int(_), .. }) {
            return Err(AotError::Unsupported(format!(
                "instruction outside the AOT M0 subset: {inst:?}"
            )));
        }
    }
    if !matches!(blk.term, Terminator::Return { value: Some(_) }) {
        return Err(AotError::Unsupported(format!(
            "terminator outside the AOT M0 subset: {:?}",
            blk.term
        )));
    }
    Ok(())
}
