//! `JitCompiler` — drives `Program` → JIT module construction and the
//! ABI-thunked `__main` invocation. The actual lowering machinery lives
//! in `lower_stmt` / `lower_expr` / `lower_op` / `lower_ctrl`.

use std::collections::HashMap;

use cranelift::prelude::*;
use cranelift_codegen::ir::types::{F32, F64, I16, I32, I64, I8};
use cranelift_codegen::settings;
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{FuncId, Linkage, Module};
use ilang_ast::{ClassDecl, FnDecl, Item, Program};

use crate::arc::{emit_release_heap, emit_retain_heap, is_aliased_heap_source};
use crate::env::{declare_import, ArrayFns, Env, LowerCtx, PrintFns, StrFns};
use crate::error::CodegenError;
use crate::lower_expr::lower_expr;
use crate::lower_op::emit_return;
use crate::lower_stmt::{lower_block_value, lower_stmt};
use crate::runtime::{
    ilang_jit_alloc_object, ilang_jit_array_new, ilang_jit_array_push_f32,
    ilang_jit_array_push_f64, ilang_jit_array_push_i16, ilang_jit_array_push_i32,
    ilang_jit_array_push_i64, ilang_jit_array_push_i8, ilang_jit_print_bool,
    ilang_jit_print_f32, ilang_jit_print_f64, ilang_jit_print_i64,
    ilang_jit_print_newline, ilang_jit_print_space, ilang_jit_print_str,
    ilang_jit_print_u64, ilang_jit_release_array, ilang_jit_release_object,
    ilang_jit_release_string, ilang_jit_retain_array, ilang_jit_retain_object,
    ilang_jit_retain_string, ilang_jit_str_concat, ilang_jit_str_eq, StringRc,
};
use crate::ty::{align_up, ArrayKind, ClassLayout, JitTy, MethodInfo};
use crate::value::{read_array, JitValue};

pub fn jit_run(prog: &Program) -> Result<JitValue, CodegenError> {
    let mut compiler = JitCompiler::new()?;
    // 1. Assign class ids and compute layouts before anything else needs
    //    to look up `Type::Object(name)`.
    for item in &prog.items {
        if let Item::Class(c) = item {
            compiler.declare_class(c)?;
        }
    }
    // 2. Declare every fn / method signature so cross-references resolve.
    for item in &prog.items {
        match item {
            Item::Fn(f) => compiler.declare_fn(f)?,
            Item::Class(c) => compiler.declare_methods(c)?,
        }
    }
    // 3. Define every body.
    for item in &prog.items {
        match item {
            Item::Fn(f) => compiler.define_fn(f)?,
            Item::Class(c) => compiler.define_methods(c)?,
        }
    }
    let main_ret = compiler.define_main(prog)?;
    compiler.finalize()?;
    Ok(compiler.run_main(main_ret))
}

struct JitCompiler {
    module: JITModule,
    ctx: cranelift_codegen::Context,
    builder_ctx: FunctionBuilderContext,
    funcs: HashMap<String, (FuncId, Vec<JitTy>, JitTy)>,
    class_ids: HashMap<String, u32>,
    class_layouts: Vec<ClassLayout>,
    class_methods: Vec<HashMap<String, MethodInfo>>,
    array_kinds: Vec<ArrayKind>,
    alloc_object_id: FuncId,
    retain_object_id: FuncId,
    release_object_id: FuncId,
    /// Per-type FFI print helpers used to lower `console.log(...)`.
    print_i64: FuncId,
    print_u64: FuncId,
    print_f64: FuncId,
    print_f32: FuncId,
    print_bool: FuncId,
    print_space: FuncId,
    print_newline: FuncId,
    print_str: FuncId,
    str_concat: FuncId,
    str_eq: FuncId,
    retain_string_id: FuncId,
    release_string_id: FuncId,
    array_new: FuncId,
    retain_array_id: FuncId,
    release_array_id: FuncId,
    array_push_i8: FuncId,
    array_push_i16: FuncId,
    array_push_i32: FuncId,
    array_push_i64: FuncId,
    array_push_f32: FuncId,
    array_push_f64: FuncId,
    /// Each string literal is interned at compile time as a `Box<StringRc>`
    /// with a saturated rc. The pointer is embedded as an `iconst` when
    /// the literal is referenced. Holding the boxes here keeps them alive
    /// until the compiler is dropped.
    interned_strings: Vec<Box<StringRc>>,
}

impl JitCompiler {
    fn new() -> Result<Self, CodegenError> {
        let flag_builder = settings::builder();
        let isa_builder = cranelift_native::builder()
            .map_err(|e| CodegenError::Cranelift(format!("isa builder: {e}")))?;
        let isa = isa_builder
            .finish(settings::Flags::new(flag_builder))
            .map_err(|e| CodegenError::Cranelift(format!("isa: {e}")))?;
        let mut builder = JITBuilder::with_isa(isa, cranelift_module::default_libcall_names());
        // Expose runtime FFI symbols to the JIT.
        builder.symbol(
            "ilang_jit_alloc_object",
            ilang_jit_alloc_object as *const u8,
        );
        builder.symbol(
            "ilang_jit_retain_object",
            ilang_jit_retain_object as *const u8,
        );
        builder.symbol(
            "ilang_jit_release_object",
            ilang_jit_release_object as *const u8,
        );
        builder.symbol("ilang_jit_print_i64", ilang_jit_print_i64 as *const u8);
        builder.symbol("ilang_jit_print_u64", ilang_jit_print_u64 as *const u8);
        builder.symbol("ilang_jit_print_f64", ilang_jit_print_f64 as *const u8);
        builder.symbol("ilang_jit_print_f32", ilang_jit_print_f32 as *const u8);
        builder.symbol("ilang_jit_print_bool", ilang_jit_print_bool as *const u8);
        builder.symbol("ilang_jit_print_space", ilang_jit_print_space as *const u8);
        builder.symbol(
            "ilang_jit_print_newline",
            ilang_jit_print_newline as *const u8,
        );
        builder.symbol("ilang_jit_print_str", ilang_jit_print_str as *const u8);
        builder.symbol("ilang_jit_str_concat", ilang_jit_str_concat as *const u8);
        builder.symbol("ilang_jit_str_eq", ilang_jit_str_eq as *const u8);
        builder.symbol(
            "ilang_jit_retain_string",
            ilang_jit_retain_string as *const u8,
        );
        builder.symbol(
            "ilang_jit_release_string",
            ilang_jit_release_string as *const u8,
        );
        builder.symbol("ilang_jit_array_new", ilang_jit_array_new as *const u8);
        builder.symbol(
            "ilang_jit_retain_array",
            ilang_jit_retain_array as *const u8,
        );
        builder.symbol(
            "ilang_jit_release_array",
            ilang_jit_release_array as *const u8,
        );
        builder.symbol(
            "ilang_jit_array_push_i8",
            ilang_jit_array_push_i8 as *const u8,
        );
        builder.symbol(
            "ilang_jit_array_push_i16",
            ilang_jit_array_push_i16 as *const u8,
        );
        builder.symbol(
            "ilang_jit_array_push_i32",
            ilang_jit_array_push_i32 as *const u8,
        );
        builder.symbol(
            "ilang_jit_array_push_i64",
            ilang_jit_array_push_i64 as *const u8,
        );
        builder.symbol(
            "ilang_jit_array_push_f32",
            ilang_jit_array_push_f32 as *const u8,
        );
        builder.symbol(
            "ilang_jit_array_push_f64",
            ilang_jit_array_push_f64 as *const u8,
        );
        let mut module = JITModule::new(builder);
        let ctx = module.make_context();

        // Declare signatures for every imported runtime function.
        let alloc_object_id = declare_import(
            &mut module,
            "ilang_jit_alloc_object",
            &[I64, I64],
            Some(I64),
        )?;
        let retain_object_id =
            declare_import(&mut module, "ilang_jit_retain_object", &[I64], None)?;
        let release_object_id = declare_import(
            &mut module,
            "ilang_jit_release_object",
            &[I64, I64],
            None,
        )?;
        let print_i64 = declare_import(&mut module, "ilang_jit_print_i64", &[I64], None)?;
        let print_u64 = declare_import(&mut module, "ilang_jit_print_u64", &[I64], None)?;
        let print_f64 = declare_import(&mut module, "ilang_jit_print_f64", &[F64], None)?;
        let print_f32 = declare_import(&mut module, "ilang_jit_print_f32", &[F32], None)?;
        let print_bool = declare_import(&mut module, "ilang_jit_print_bool", &[I8], None)?;
        let print_space = declare_import(&mut module, "ilang_jit_print_space", &[], None)?;
        let print_newline =
            declare_import(&mut module, "ilang_jit_print_newline", &[], None)?;
        let print_str = declare_import(&mut module, "ilang_jit_print_str", &[I64], None)?;
        let str_concat = declare_import(
            &mut module,
            "ilang_jit_str_concat",
            &[I64, I64],
            Some(I64),
        )?;
        let str_eq =
            declare_import(&mut module, "ilang_jit_str_eq", &[I64, I64], Some(I8))?;
        let retain_string_id =
            declare_import(&mut module, "ilang_jit_retain_string", &[I64], None)?;
        let release_string_id =
            declare_import(&mut module, "ilang_jit_release_string", &[I64], None)?;
        let array_new =
            declare_import(&mut module, "ilang_jit_array_new", &[I64, I64], Some(I64))?;
        let retain_array_id =
            declare_import(&mut module, "ilang_jit_retain_array", &[I64], None)?;
        let release_array_id = declare_import(
            &mut module,
            "ilang_jit_release_array",
            &[I64, I64],
            None,
        )?;
        let array_push_i8 =
            declare_import(&mut module, "ilang_jit_array_push_i8", &[I64, I8], None)?;
        let array_push_i16 =
            declare_import(&mut module, "ilang_jit_array_push_i16", &[I64, I16], None)?;
        let array_push_i32 =
            declare_import(&mut module, "ilang_jit_array_push_i32", &[I64, I32], None)?;
        let array_push_i64 =
            declare_import(&mut module, "ilang_jit_array_push_i64", &[I64, I64], None)?;
        let array_push_f32 =
            declare_import(&mut module, "ilang_jit_array_push_f32", &[I64, F32], None)?;
        let array_push_f64 =
            declare_import(&mut module, "ilang_jit_array_push_f64", &[I64, F64], None)?;

        Ok(Self {
            module,
            ctx,
            builder_ctx: FunctionBuilderContext::new(),
            funcs: HashMap::new(),
            class_ids: HashMap::new(),
            class_layouts: Vec::new(),
            class_methods: Vec::new(),
            array_kinds: Vec::new(),
            alloc_object_id,
            retain_object_id,
            release_object_id,
            print_i64,
            print_u64,
            print_f64,
            print_f32,
            print_bool,
            print_space,
            print_newline,
            print_str,
            str_concat,
            str_eq,
            retain_string_id,
            release_string_id,
            array_new,
            retain_array_id,
            release_array_id,
            array_push_i8,
            array_push_i16,
            array_push_i32,
            array_push_i64,
            array_push_f32,
            array_push_f64,
            interned_strings: Vec::new(),
        })
    }

    fn declare_class(&mut self, c: &ClassDecl) -> Result<(), CodegenError> {
        let id = self.class_layouts.len() as u32;
        self.class_ids.insert(c.name.clone(), id);
        let mut offset = 0u32;
        let mut max_align = 1u32;
        let mut fields = HashMap::new();
        for field in &c.fields {
            let jty = JitTy::from_ast(&field.ty, field.span, &self.class_ids, &mut self.array_kinds)?;
            let size = jty.size_bytes();
            let align = size.max(1);
            offset = align_up(offset, align);
            fields.insert(field.name.clone(), (offset, jty));
            offset += size;
            max_align = max_align.max(align);
        }
        let size = align_up(offset.max(1), max_align);
        self.class_layouts.push(ClassLayout {
            name: c.name.clone(),
            fields,
            size,
        });
        self.class_methods.push(HashMap::new());
        Ok(())
    }

    fn declare_fn(&mut self, f: &FnDecl) -> Result<(), CodegenError> {
        let (id, params, ret) = self.declare_fn_signature(&f.name, f, None)?;
        self.funcs.insert(f.name.clone(), (id, params, ret));
        Ok(())
    }

    /// Declare every method of a class as a top-level function with
    /// `this` as the implicit first parameter. Constructor (`init`) is
    /// no different from a regular method here — the special handling
    /// lives in the `new` lowering.
    fn declare_methods(&mut self, c: &ClassDecl) -> Result<(), CodegenError> {
        let class_id = self.class_ids[&c.name];
        for m in &c.methods {
            let symbol = format!("__method_{}_{}", c.name, m.name);
            let (id, params, ret) =
                self.declare_fn_signature(&symbol, m, Some(JitTy::Object(class_id)))?;
            self.class_methods[class_id as usize].insert(
                m.name.clone(),
                MethodInfo { id, params, ret },
            );
        }
        Ok(())
    }

    /// Shared helper for `declare_fn` and `declare_methods`. `this_ty`,
    /// when `Some`, is prepended to the param list so methods get an
    /// implicit `this` pointer.
    fn declare_fn_signature(
        &mut self,
        symbol: &str,
        f: &FnDecl,
        this_ty: Option<JitTy>,
    ) -> Result<(FuncId, Vec<JitTy>, JitTy), CodegenError> {
        let mut params = Vec::with_capacity(f.params.len());
        for p in &f.params {
            params.push(JitTy::from_ast(&p.ty, p.span, &self.class_ids, &mut self.array_kinds)?);
        }
        let ret = match &f.ret {
            Some(t) => JitTy::from_ast(t, f.span, &self.class_ids, &mut self.array_kinds)?,
            None => JitTy::Unit,
        };
        let mut sig = self.module.make_signature();
        if let Some(t) = this_ty {
            sig.params.push(AbiParam::new(t.cl().expect("object pointer")));
        }
        for p in &params {
            sig.params.push(AbiParam::new(p.cl().ok_or_else(|| {
                CodegenError::Unsupported {
                    what: "unit-typed parameter".into(),
                    span: f.span,
                }
            })?));
        }
        if let Some(t) = ret.cl() {
            sig.returns.push(AbiParam::new(t));
        }
        let id = self
            .module
            .declare_function(symbol, Linkage::Local, &sig)
            .map_err(|e| CodegenError::Module(e.to_string()))?;
        Ok((id, params, ret))
    }

    fn define_fn(&mut self, f: &FnDecl) -> Result<(), CodegenError> {
        let (id, param_tys, ret_ty) = self.funcs[&f.name].clone();
        self.define_function_body(id, f, &param_tys, ret_ty, None)
    }

    fn define_methods(&mut self, c: &ClassDecl) -> Result<(), CodegenError> {
        let class_id = self.class_ids[&c.name];
        for m in &c.methods {
            let info = self.class_methods[class_id as usize][&m.name].clone();
            self.define_function_body(info.id, m, &info.params, info.ret, Some(class_id))?;
        }
        Ok(())
    }

    fn define_function_body(
        &mut self,
        id: FuncId,
        f: &FnDecl,
        param_tys: &[JitTy],
        ret_ty: JitTy,
        this_class: Option<u32>,
    ) -> Result<(), CodegenError> {
        self.module.clear_context(&mut self.ctx);
        self.ctx.func.signature =
            self.module.declarations().get_function_decl(id).signature.clone();

        let mut builder = FunctionBuilder::new(&mut self.ctx.func, &mut self.builder_ctx);
        let entry = builder.create_block();
        builder.append_block_params_for_function_params(entry);
        builder.switch_to_block(entry);
        builder.seal_block(entry);

        let mut env = Env::default();
        let mut block_param_idx = 0usize;

        // Bind `this` first, if this is a method.
        let this = match this_class {
            Some(class_id) => {
                let var = Variable::new(env.next_var_id());
                builder.declare_var(var, I64);
                let v = builder.block_params(entry)[block_param_idx];
                builder.def_var(var, v);
                block_param_idx += 1;
                Some((var, class_id))
            }
            None => None,
        };

        for (i, p) in f.params.iter().enumerate() {
            let pty = param_tys[i];
            let var = Variable::new(env.next_var_id());
            builder.declare_var(var, pty.cl().expect("non-unit checked at declare"));
            let v = builder.block_params(entry)[block_param_idx + i];
            builder.def_var(var, v);
            env.bindings.insert(p.name.clone(), (var, pty));
        }

        let mut lc = LowerCtx {
            funcs: &self.funcs,
            class_layouts: &self.class_layouts,
            class_methods: &self.class_methods,
            alloc_object_id: self.alloc_object_id,
            retain_object_id: self.retain_object_id,
            release_object_id: self.release_object_id,
            print: PrintFns {
                i64: self.print_i64,
                u64: self.print_u64,
                f64: self.print_f64,
                f32: self.print_f32,
                bool: self.print_bool,
                space: self.print_space,
                newline: self.print_newline,
                str: self.print_str,
            },
            strfns: StrFns {
                concat: self.str_concat,
                eq: self.str_eq,
                retain: self.retain_string_id,
                release: self.release_string_id,
            },
            arrfns: ArrayFns {
                new: self.array_new,
                retain: self.retain_array_id,
                release: self.release_array_id,
                push_i8: self.array_push_i8,
                push_i16: self.array_push_i16,
                push_i32: self.array_push_i32,
                push_i64: self.array_push_i64,
                push_f32: self.array_push_f32,
                push_f64: self.array_push_f64,
            },
            module: &mut self.module,
            env: &mut env,
            loops: Vec::new(),
            this,
            interned_strings: &mut self.interned_strings,
            array_kinds: &mut self.array_kinds,
        };
        let body = lower_block_value(&mut builder, &mut lc, &f.body)?;
        // Release heap-typed params (and `this` for methods) at function
        // exit. Caller used `emit_retain_heap` on each before the call,
        // so the rc comes out balanced.
        let mut param_releases: Vec<(Variable, JitTy)> = f
            .params
            .iter()
            .filter_map(|p| {
                let &(var, jty) = lc.env.bindings.get(&p.name)?;
                if matches!(jty, JitTy::Object(_) | JitTy::Str | JitTy::Array(_)) {
                    Some((var, jty))
                } else {
                    None
                }
            })
            .collect();
        // Skip releasing `this` inside `deinit` — release_object already
        // owns the lifecycle here. Releasing again would re-enter
        // release_object on rc=0 and infinite-loop.
        if let Some((this_var, class_id)) = this {
            if f.name != "deinit" {
                param_releases.push((this_var, JitTy::Object(class_id)));
            }
        }
        for (var, jty) in param_releases {
            let p = builder.use_var(var);
            emit_release_heap(&mut builder, &mut lc, p, jty);
        }
        emit_return(&mut builder, ret_ty, body, f.span)?;
        builder.finalize();

        self.module
            .define_function(id, &mut self.ctx)
            .map_err(|e| CodegenError::Module(e.to_string()))?;
        Ok(())
    }

    fn define_main(&mut self, prog: &Program) -> Result<JitTy, CodegenError> {
        let mut tc = ilang_types::TypeChecker::new();
        let prog_ty = tc.check(prog).map_err(|e| CodegenError::Cranelift(e.to_string()))?;
        let ret_ty = JitTy::from_ast(&prog_ty, ilang_ast::Span::dummy(), &self.class_ids, &mut self.array_kinds)?;

        let mut sig = self.module.make_signature();
        if let Some(t) = ret_ty.cl() {
            sig.returns.push(AbiParam::new(t));
        }
        let id = self
            .module
            .declare_function("__main", Linkage::Export, &sig)
            .map_err(|e| CodegenError::Module(e.to_string()))?;

        self.module.clear_context(&mut self.ctx);
        self.ctx.func.signature = sig;

        let mut builder = FunctionBuilder::new(&mut self.ctx.func, &mut self.builder_ctx);
        let entry = builder.create_block();
        builder.switch_to_block(entry);
        builder.seal_block(entry);

        let mut env = Env::default();
        let mut lc = LowerCtx {
            funcs: &self.funcs,
            class_layouts: &self.class_layouts,
            class_methods: &self.class_methods,
            alloc_object_id: self.alloc_object_id,
            retain_object_id: self.retain_object_id,
            release_object_id: self.release_object_id,
            print: PrintFns {
                i64: self.print_i64,
                u64: self.print_u64,
                f64: self.print_f64,
                f32: self.print_f32,
                bool: self.print_bool,
                space: self.print_space,
                newline: self.print_newline,
                str: self.print_str,
            },
            strfns: StrFns {
                concat: self.str_concat,
                eq: self.str_eq,
                retain: self.retain_string_id,
                release: self.release_string_id,
            },
            arrfns: ArrayFns {
                new: self.array_new,
                retain: self.retain_array_id,
                release: self.release_array_id,
                push_i8: self.array_push_i8,
                push_i16: self.array_push_i16,
                push_i32: self.array_push_i32,
                push_i64: self.array_push_i64,
                push_f32: self.array_push_f32,
                push_f64: self.array_push_f64,
            },
            module: &mut self.module,
            env: &mut env,
            loops: Vec::new(),
            this: None,
            interned_strings: &mut self.interned_strings,
            array_kinds: &mut self.array_kinds,
        };
        // Snapshot empty env so we know which top-level lets to release
        // at __main exit. Mirrors what lower_block_value does for blocks.
        let before: std::collections::HashSet<String> =
            lc.env.bindings.keys().cloned().collect();
        for s in &prog.stmts {
            lower_stmt(&mut builder, &mut lc, s)?;
        }
        let tail_kind = prog.tail.as_ref().map(|e| &e.kind);
        let body = match &prog.tail {
            // A unit-typed tail (e.g. `console.log(...)`) is fine — we'll
            // emit a bare `return` and won't try to coerce a value.
            Some(t) => lower_expr(&mut builder, &mut lc, t)?,
            None => None,
        };
        // Retain only aliased heap tails so the upcoming top-level let
        // releases don't free what we're returning. Fresh heap tails
        // already arrive with rc=1.
        if let Some((v, t)) = body {
            if matches!(t, JitTy::Object(_) | JitTy::Str | JitTy::Array(_))
                && tail_kind.map(is_aliased_heap_source).unwrap_or(false)
            {
                emit_retain_heap(&mut builder, &mut lc, v, t);
            }
        }
        let mut releases: Vec<(Variable, JitTy)> = lc
            .env
            .bindings
            .iter()
            .filter(|(k, _)| !before.contains(k.as_str()))
            .filter_map(|(_, &(var, jty))| {
                if matches!(jty, JitTy::Object(_) | JitTy::Str | JitTy::Array(_)) {
                    Some((var, jty))
                } else {
                    None
                }
            })
            .collect();
        // LIFO release so the most-recently-bound (likely depending on
        // earlier ones) drops first.
        releases.sort_by_key(|(var, _)| std::cmp::Reverse(var.as_u32()));
        for (var, jty) in releases {
            let p = builder.use_var(var);
            emit_release_heap(&mut builder, &mut lc, p, jty);
        }
        emit_return(&mut builder, ret_ty, body, ilang_ast::Span::dummy())?;
        builder.finalize();

        self.module
            .define_function(id, &mut self.ctx)
            .map_err(|e| CodegenError::Module(e.to_string()))?;
        self.funcs.insert("__main".into(), (id, vec![], ret_ty));
        Ok(ret_ty)
    }

    fn finalize(&mut self) -> Result<(), CodegenError> {
        self.module
            .finalize_definitions()
            .map_err(|e| CodegenError::Module(e.to_string()))?;
        Ok(())
    }

    fn run_main(&mut self, ret: JitTy) -> JitValue {
        let (id, _, _) = self.funcs["__main"];
        let ptr = self.module.get_finalized_function(id);
        unsafe {
            match ret {
                JitTy::I8 => JitValue::I8((std::mem::transmute::<_, extern "C" fn() -> i8>(ptr))()),
                JitTy::I16 => {
                    JitValue::I16((std::mem::transmute::<_, extern "C" fn() -> i16>(ptr))())
                }
                JitTy::I32 => {
                    JitValue::I32((std::mem::transmute::<_, extern "C" fn() -> i32>(ptr))())
                }
                JitTy::I64 => {
                    JitValue::I64((std::mem::transmute::<_, extern "C" fn() -> i64>(ptr))())
                }
                JitTy::U8 => JitValue::U8((std::mem::transmute::<_, extern "C" fn() -> u8>(ptr))()),
                JitTy::U16 => {
                    JitValue::U16((std::mem::transmute::<_, extern "C" fn() -> u16>(ptr))())
                }
                JitTy::U32 => {
                    JitValue::U32((std::mem::transmute::<_, extern "C" fn() -> u32>(ptr))())
                }
                JitTy::U64 => {
                    JitValue::U64((std::mem::transmute::<_, extern "C" fn() -> u64>(ptr))())
                }
                JitTy::F32 => {
                    JitValue::F32((std::mem::transmute::<_, extern "C" fn() -> f32>(ptr))())
                }
                JitTy::F64 => {
                    JitValue::F64((std::mem::transmute::<_, extern "C" fn() -> f64>(ptr))())
                }
                JitTy::Bool => {
                    let v = (std::mem::transmute::<_, extern "C" fn() -> i8>(ptr))();
                    JitValue::Bool(v != 0)
                }
                JitTy::Object(id) => {
                    let p = (std::mem::transmute::<_, extern "C" fn() -> i64>(ptr))();
                    JitValue::Object {
                        class: self.class_layouts[id as usize].name.clone(),
                        ptr: p,
                    }
                }
                JitTy::Str => {
                    let p = (std::mem::transmute::<_, extern "C" fn() -> i64>(ptr))();
                    let s = (*(p as *const StringRc)).s.clone();
                    JitValue::Str(s)
                }
                JitTy::Array(id) => {
                    let header_ptr = (std::mem::transmute::<_, extern "C" fn() -> i64>(ptr))();
                    JitValue::Array(read_array(
                        header_ptr,
                        self.array_kinds[id as usize],
                        &self.array_kinds,
                        &self.class_layouts,
                    ))
                }
                JitTy::Unit => {
                    (std::mem::transmute::<_, extern "C" fn()>(ptr))();
                    JitValue::Unit
                }
            }
        }
    }
}
