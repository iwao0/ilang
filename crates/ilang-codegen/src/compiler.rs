//! `JitCompiler` — drives `Program` → JIT module construction and the
//! ABI-thunked `__main` invocation. The actual lowering machinery lives
//! in `lower_stmt` / `lower_expr` / `lower_op` / `lower_ctrl`.

use std::collections::HashMap;

use cranelift::prelude::*;
use cranelift_codegen::ir::types::{F32, F64, I16, I32, I64, I8};
use cranelift_codegen::settings;
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{FuncId, Linkage, Module};
use ilang_ast::{ClassDecl, EnumDecl, FnDecl, Item, Program, VariantPayload};

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
    ilang_jit_print_newline, ilang_jit_print_space,
    ilang_jit_print_str, ilang_jit_print_u64, ilang_jit_release_array, ilang_jit_release_object,
    ilang_jit_release_string, ilang_jit_retain_array, ilang_jit_retain_object,
    ilang_jit_retain_string, ilang_jit_retain_weak, ilang_jit_str_char_at,
    ilang_jit_str_concat, ilang_jit_str_ends_with, ilang_jit_str_eq, ilang_jit_str_includes,
    ilang_jit_str_length, ilang_jit_str_starts_with, ilang_jit_str_to_lower,
    ilang_jit_str_to_upper, ilang_jit_str_trim,
    ilang_jit_release_weak, ilang_jit_weak_get, StringRc,
};
use crate::ty::{
    align_up, ArrayKind, ClassLayout, EnumLayout, EnumVariantLayout, FnSignature, JitTy,
    MethodInfo,
};
use crate::value::{read_array, JitValue};

pub fn jit_run(prog: &Program) -> Result<JitValue, CodegenError> {
    // Hoist anonymous-function expressions to top-level synthetic
    // fns so the JIT only ever sees named functions. Then
    // monomorphize generic classes. After both passes the program
    // is plain non-generic, no FnExpr nodes remain.
    let hoisted = crate::monomorphize::hoist_anon_fns(prog);
    let mono = crate::monomorphize::monomorphize(&hoisted);
    jit_run_inner(&mono)
}

fn jit_run_inner(prog: &Program) -> Result<JitValue, CodegenError> {
    let mut compiler = JitCompiler::new()?;
    // 1a. Register every class / enum name → id, with empty layouts.
    //     This way `Child { p: Parent.weak }` resolves even when Parent
    //     is declared after Child, and likewise for enum forward-refs.
    for item in &prog.items {
        match item {
            Item::Class(c) => compiler.declare_class_name(c),
            Item::Enum(e) => compiler.declare_enum_layout(e)?,
            _ => {}
        }
    }
    // 1b. Compute field offsets now that every class id is in
    //     `class_ids`. Enums were finalized at declaration time
    //     (variants don't refer to other types in Phase 1).
    for item in &prog.items {
        if let Item::Class(c) = item {
            compiler.finalize_class_layout(c)?;
        }
    }
    // 2. Declare every fn / method signature so cross-references resolve.
    for item in &prog.items {
        match item {
            Item::Fn(f) => compiler.declare_fn(f)?,
            Item::Class(c) => compiler.declare_methods(c)?,
            Item::Enum(_) => {}
            Item::Use(_) => {}
        }
    }
    // 2b. Declare per-class drop wrappers so `new` lowering can embed
    //     their function pointers in the allocation header. Bodies are
    //     defined later (need user methods to be defined first).
    crate::drops::declare_class_drops(&mut compiler)?;
    // 3. Define every body.
    for item in &prog.items {
        match item {
            Item::Fn(f) => compiler.define_fn(f)?,
            Item::Class(c) => compiler.define_methods(c)?,
            Item::Enum(_) => {}
            Item::Use(_) => {}
        }
    }
    let main_ret = compiler.define_main(prog)?;
    // 4. Define drop wrappers. Class drops can reference user deinit;
    //    array drops were declared lazily during lowering.
    crate::drops::define_class_drops(&mut compiler)?;
    crate::drops::define_array_drops(&mut compiler)?;
    crate::drops::define_enum_drops(&mut compiler)?;
    compiler.finalize()?;
    Ok(compiler.run_main(main_ret))
}

pub(crate) struct JitCompiler {
    pub(crate) module: JITModule,
    pub(crate) ctx: cranelift_codegen::Context,
    pub(crate) builder_ctx: FunctionBuilderContext,
    pub(crate) funcs: HashMap<String, (FuncId, Vec<JitTy>, JitTy)>,
    pub(crate) class_ids: HashMap<String, u32>,
    pub(crate) class_layouts: Vec<ClassLayout>,
    pub(crate) class_methods: Vec<HashMap<String, MethodInfo>>,
    pub(crate) enum_ids: HashMap<String, u32>,
    pub(crate) enum_layouts: Vec<EnumLayout>,
    pub(crate) array_kinds: Vec<ArrayKind>,
    pub(crate) optional_inners: Vec<JitTy>,
    pub(crate) fn_signatures: Vec<FnSignature>,
    pub(crate) alloc_object_id: FuncId,
    pub(crate) retain_object_id: FuncId,
    pub(crate) release_object_id: FuncId,
    /// Per-type FFI print helpers used to lower `console.log(...)`.
    pub(crate) print_i64: FuncId,
    pub(crate) print_u64: FuncId,
    pub(crate) print_f64: FuncId,
    pub(crate) print_f32: FuncId,
    pub(crate) print_bool: FuncId,
    pub(crate) print_space: FuncId,
    pub(crate) print_newline: FuncId,
    pub(crate) print_str: FuncId,
    pub(crate) str_concat: FuncId,
    pub(crate) str_eq: FuncId,
    pub(crate) retain_string_id: FuncId,
    pub(crate) release_string_id: FuncId,
    pub(crate) str_length_id: FuncId,
    pub(crate) str_char_at_id: FuncId,
    pub(crate) str_includes_id: FuncId,
    pub(crate) str_starts_with_id: FuncId,
    pub(crate) str_ends_with_id: FuncId,
    pub(crate) str_to_upper_id: FuncId,
    pub(crate) str_to_lower_id: FuncId,
    pub(crate) str_trim_id: FuncId,
    pub(crate) array_new: FuncId,
    pub(crate) retain_array_id: FuncId,
    pub(crate) release_array_id: FuncId,
    pub(crate) retain_weak_id: FuncId,
    pub(crate) release_weak_id: FuncId,
    pub(crate) weak_get_id: FuncId,
    pub(crate) array_push_i8: FuncId,
    pub(crate) array_push_i16: FuncId,
    pub(crate) array_push_i32: FuncId,
    pub(crate) array_push_i64: FuncId,
    pub(crate) array_push_f32: FuncId,
    pub(crate) array_push_f64: FuncId,
    /// Each string literal is interned at compile time as a `Box<StringRc>`
    /// with a saturated rc. The pointer is embedded as an `iconst` when
    /// the literal is referenced. Holding the boxes here keeps them alive
    /// until the compiler is dropped.
    pub(crate) interned_strings: Vec<Box<StringRc>>,
    /// Per-class drop wrappers (parallel to `class_layouts`). Declared
    /// during `declare_class_drops` and defined later, after methods.
    /// `None` indicates no wrapper is needed (no deinit, no heap fields).
    pub(crate) class_drops: Vec<Option<FuncId>>,
    /// Per-array-kind drop wrappers, declared lazily during lowering.
    /// `None` indicates the kind has no heap elements.
    pub(crate) array_drops: HashMap<u32, Option<FuncId>>,
    /// Per-enum drop wrappers, declared lazily during lowering.
    /// `None` means the enum has no heap-typed payload anywhere.
    pub(crate) enum_drops: HashMap<u32, Option<FuncId>>,
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
        builder.symbol("ilang_jit_str_length", ilang_jit_str_length as *const u8);
        builder.symbol("ilang_jit_str_char_at", ilang_jit_str_char_at as *const u8);
        builder.symbol("ilang_jit_str_includes", ilang_jit_str_includes as *const u8);
        builder.symbol("ilang_jit_str_starts_with", ilang_jit_str_starts_with as *const u8);
        builder.symbol("ilang_jit_str_ends_with", ilang_jit_str_ends_with as *const u8);
        builder.symbol("ilang_jit_str_to_upper", ilang_jit_str_to_upper as *const u8);
        builder.symbol("ilang_jit_str_to_lower", ilang_jit_str_to_lower as *const u8);
        builder.symbol("ilang_jit_str_trim", ilang_jit_str_trim as *const u8);
        builder.symbol("ilang_jit_array_new", ilang_jit_array_new as *const u8);
        builder.symbol(
            "ilang_jit_retain_array",
            ilang_jit_retain_array as *const u8,
        );
        builder.symbol(
            "ilang_jit_release_array",
            ilang_jit_release_array as *const u8,
        );
        builder.symbol("ilang_jit_retain_weak", ilang_jit_retain_weak as *const u8);
        builder.symbol("ilang_jit_release_weak", ilang_jit_release_weak as *const u8);
        builder.symbol("ilang_jit_weak_get", ilang_jit_weak_get as *const u8);
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
        let str_length_id =
            declare_import(&mut module, "ilang_jit_str_length", &[I64], Some(I64))?;
        let str_char_at_id = declare_import(
            &mut module,
            "ilang_jit_str_char_at",
            &[I64, I64],
            Some(I64),
        )?;
        let str_includes_id = declare_import(
            &mut module,
            "ilang_jit_str_includes",
            &[I64, I64],
            Some(I8),
        )?;
        let str_starts_with_id = declare_import(
            &mut module,
            "ilang_jit_str_starts_with",
            &[I64, I64],
            Some(I8),
        )?;
        let str_ends_with_id = declare_import(
            &mut module,
            "ilang_jit_str_ends_with",
            &[I64, I64],
            Some(I8),
        )?;
        let str_to_upper_id =
            declare_import(&mut module, "ilang_jit_str_to_upper", &[I64], Some(I64))?;
        let str_to_lower_id =
            declare_import(&mut module, "ilang_jit_str_to_lower", &[I64], Some(I64))?;
        let str_trim_id =
            declare_import(&mut module, "ilang_jit_str_trim", &[I64], Some(I64))?;
        let array_new =
            declare_import(&mut module, "ilang_jit_array_new", &[I64, I64, I64], Some(I64))?;
        let retain_array_id =
            declare_import(&mut module, "ilang_jit_retain_array", &[I64], None)?;
        let release_array_id = declare_import(
            &mut module,
            "ilang_jit_release_array",
            &[I64, I64],
            None,
        )?;
        let retain_weak_id =
            declare_import(&mut module, "ilang_jit_retain_weak", &[I64], None)?;
        let release_weak_id =
            declare_import(&mut module, "ilang_jit_release_weak", &[I64, I64], None)?;
        let weak_get_id =
            declare_import(&mut module, "ilang_jit_weak_get", &[I64], Some(I64))?;
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
            enum_ids: HashMap::new(),
            enum_layouts: Vec::new(),
            array_kinds: Vec::new(),
            optional_inners: Vec::new(),
            fn_signatures: Vec::new(),
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
            str_length_id,
            str_char_at_id,
            str_includes_id,
            str_starts_with_id,
            str_ends_with_id,
            str_to_upper_id,
            str_to_lower_id,
            str_trim_id,
            array_new,
            retain_array_id,
            release_array_id,
            retain_weak_id,
            release_weak_id,
            weak_get_id,
            array_push_i8,
            array_push_i16,
            array_push_i32,
            array_push_i64,
            array_push_f32,
            array_push_f64,
            interned_strings: Vec::new(),
            class_drops: Vec::new(),
            array_drops: HashMap::new(),
            enum_drops: HashMap::new(),
        })
    }

    /// Register an enum's layout. For unit-only enums the runtime is a
    /// bare i32 tag. For enums with at least one payload variant we
    /// compute per-variant payload offsets and the max payload size
    /// for the tagged-union allocation. Resolving inner field types
    /// piggybacks on `JitTy::from_ast`, which can see the in-progress
    /// `enum_layouts` table for forward refs.
    fn declare_enum_layout(&mut self, e: &EnumDecl) -> Result<(), CodegenError> {
        let id = self.enum_layouts.len() as u32;
        self.enum_ids.insert(e.name.clone(), id);
        let all_unit = e
            .variants
            .iter()
            .all(|v| matches!(v.payload, VariantPayload::Unit));
        // Push a placeholder so JitTy::from_ast can see the entry while
        // we resolve payload field types.
        self.enum_layouts.push(EnumLayout {
            name: e.name.clone(),
            variants: e.variants.iter().map(|v| v.name.clone()).collect(),
            all_unit,
            payloads: vec![EnumVariantLayout::Unit; e.variants.len()],
            max_payload_size: 0,
        });
        if all_unit {
            return Ok(());
        }
        let mut payloads = Vec::with_capacity(e.variants.len());
        let mut max_size = 0u32;
        for variant in &e.variants {
            let (vlayout, vsize) = match &variant.payload {
                VariantPayload::Unit => (EnumVariantLayout::Unit, 0u32),
                VariantPayload::Tuple(tys) => {
                    let mut offset = 0u32;
                    let mut entries = Vec::with_capacity(tys.len());
                    for t in tys {
                        let jty = JitTy::from_ast(
                            t,
                            variant.span,
                            &self.class_ids,
                            &self.enum_ids,
                            &self.enum_layouts,
                            &mut self.array_kinds,
                            &mut self.optional_inners, &mut self.fn_signatures,
                        )?;
                        let size = jty.size_bytes();
                        let align = size.max(1);
                        offset = align_up(offset, align);
                        entries.push((offset, jty));
                        offset += size;
                    }
                    let total = align_up(offset, 8);
                    (EnumVariantLayout::Tuple(entries), total)
                }
                VariantPayload::Struct(fields) => {
                    let mut offset = 0u32;
                    let mut map: HashMap<String, (u32, JitTy)> = HashMap::new();
                    for f in fields {
                        let jty = JitTy::from_ast(
                            &f.ty,
                            f.span,
                            &self.class_ids,
                            &self.enum_ids,
                            &self.enum_layouts,
                            &mut self.array_kinds,
                            &mut self.optional_inners, &mut self.fn_signatures,
                        )?;
                        let size = jty.size_bytes();
                        let align = size.max(1);
                        offset = align_up(offset, align);
                        map.insert(f.name.clone(), (offset, jty));
                        offset += size;
                    }
                    let total = align_up(offset, 8);
                    (EnumVariantLayout::Struct(map), total)
                }
            };
            payloads.push(vlayout);
            if vsize > max_size {
                max_size = vsize;
            }
        }
        let entry = &mut self.enum_layouts[id as usize];
        entry.payloads = payloads;
        entry.max_payload_size = max_size;
        Ok(())
    }

    /// First pass: register the class name → id mapping with an empty
    /// layout, so other classes' field types can refer to this one
    /// (`Parent.weak`, `Child?`, etc.) regardless of declaration order.
    fn declare_class_name(&mut self, c: &ClassDecl) {
        let id = self.class_layouts.len() as u32;
        self.class_ids.insert(c.name.clone(), id);
        self.class_layouts.push(ClassLayout {
            name: c.name.clone(),
            fields: HashMap::new(),
            size: 0,
        });
        self.class_methods.push(HashMap::new());
    }

    /// Second pass: compute field offsets/sizes now that every class
    /// id is in the table. Splits out from `declare_class_name` so
    /// `Child { p: Parent.weak }` works when Parent is declared after
    /// Child in source order.
    fn finalize_class_layout(&mut self, c: &ClassDecl) -> Result<(), CodegenError> {
        let id = self.class_ids[&c.name] as usize;
        let mut offset = 0u32;
        let mut max_align = 1u32;
        let mut fields = HashMap::new();
        for field in &c.fields {
            let jty = JitTy::from_ast(
                &field.ty,
                field.span,
                &self.class_ids,
                &self.enum_ids,
                &self.enum_layouts,
                &mut self.array_kinds,
                &mut self.optional_inners, &mut self.fn_signatures,
            )?;
            let size = jty.size_bytes();
            let align = size.max(1);
            offset = align_up(offset, align);
            fields.insert(field.name.clone(), (offset, jty));
            offset += size;
            max_align = max_align.max(align);
        }
        let size = align_up(offset.max(1), max_align);
        self.class_layouts[id].fields = fields;
        self.class_layouts[id].size = size;
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
            params.push(JitTy::from_ast(&p.ty, p.span, &self.class_ids, &self.enum_ids, &self.enum_layouts, &mut self.array_kinds, &mut self.optional_inners, &mut self.fn_signatures)?);
        }
        let ret = match &f.ret {
            Some(t) => JitTy::from_ast(t, f.span, &self.class_ids, &self.enum_ids, &self.enum_layouts, &mut self.array_kinds, &mut self.optional_inners, &mut self.fn_signatures)?,
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
            enum_layouts: &self.enum_layouts,
            alloc_object_id: self.alloc_object_id,
            retain_object_id: self.retain_object_id,
            release_object_id: self.release_object_id,
            retain_weak_id: self.retain_weak_id,
            release_weak_id: self.release_weak_id,
            weak_get_id: self.weak_get_id,
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
                length: self.str_length_id,
                char_at: self.str_char_at_id,
                includes: self.str_includes_id,
                starts_with: self.str_starts_with_id,
                ends_with: self.str_ends_with_id,
                to_upper: self.str_to_upper_id,
                to_lower: self.str_to_lower_id,
                trim: self.str_trim_id,
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
            current_ret_ty: ret_ty,
            current_fn_is_deinit: f.name == "deinit",
            interned_strings: &mut self.interned_strings,
            array_kinds: &mut self.array_kinds,
            optional_inners: &mut self.optional_inners,
            fn_signatures: &mut self.fn_signatures,
            class_drops: &self.class_drops,
            array_drops: &mut self.array_drops,
            enum_drops: &mut self.enum_drops,
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
                if jty.is_heap() {
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
        let ret_ty = JitTy::from_ast(&prog_ty, ilang_ast::Span::dummy(), &self.class_ids, &self.enum_ids, &self.enum_layouts, &mut self.array_kinds, &mut self.optional_inners, &mut self.fn_signatures)?;

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
            enum_layouts: &self.enum_layouts,
            alloc_object_id: self.alloc_object_id,
            retain_object_id: self.retain_object_id,
            release_object_id: self.release_object_id,
            retain_weak_id: self.retain_weak_id,
            release_weak_id: self.release_weak_id,
            weak_get_id: self.weak_get_id,
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
                length: self.str_length_id,
                char_at: self.str_char_at_id,
                includes: self.str_includes_id,
                starts_with: self.str_starts_with_id,
                ends_with: self.str_ends_with_id,
                to_upper: self.str_to_upper_id,
                to_lower: self.str_to_lower_id,
                trim: self.str_trim_id,
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
            current_ret_ty: ret_ty,
            current_fn_is_deinit: false,
            interned_strings: &mut self.interned_strings,
            array_kinds: &mut self.array_kinds,
            optional_inners: &mut self.optional_inners,
            fn_signatures: &mut self.fn_signatures,
            class_drops: &self.class_drops,
            array_drops: &mut self.array_drops,
            enum_drops: &mut self.enum_drops,
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
            if t.is_heap()
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
                if jty.is_heap() {
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
                        &self.enum_layouts,
                        &self.optional_inners,
                    ))
                }
                JitTy::Optional(id) => {
                    let p = (std::mem::transmute::<_, extern "C" fn() -> i64>(ptr))();
                    crate::value::read_optional_pointer(
                        p,
                        self.optional_inners[id as usize],
                        &self.array_kinds,
                        &self.class_layouts,
                        &self.enum_layouts,
                        &self.optional_inners,
                    )
                }
                JitTy::Weak(class_id) => {
                    let p = (std::mem::transmute::<_, extern "C" fn() -> i64>(ptr))();
                    let alive = if p == 0 {
                        false
                    } else {
                        *((p - 24) as *const i64) > 0
                    };
                    JitValue::Weak {
                        class: self.class_layouts[class_id as usize].name.clone(),
                        alive,
                    }
                }
                JitTy::Enum(id) => {
                    let tag = (std::mem::transmute::<_, extern "C" fn() -> i32>(ptr))()
                        as usize;
                    let layout = &self.enum_layouts[id as usize];
                    JitValue::Enum {
                        ty: layout.name.clone(),
                        variant: layout
                            .variants
                            .get(tag)
                            .cloned()
                            .unwrap_or_else(|| format!("?{tag}")),
                        payload: crate::value::JitEnumPayload::Unit,
                    }
                }
                JitTy::EnumHeap(id) => {
                    let p = (std::mem::transmute::<_, extern "C" fn() -> i64>(ptr))();
                    crate::value::read_enum_heap(
                        p,
                        id,
                        &self.enum_layouts,
                        &self.array_kinds,
                        &self.class_layouts,
                        &self.optional_inners,
                    )
                }
                JitTy::Fn(_) => {
                    let p = (std::mem::transmute::<_, extern "C" fn() -> i64>(ptr))();
                    JitValue::Fn(p)
                }
                JitTy::Unit => {
                    (std::mem::transmute::<_, extern "C" fn()>(ptr))();
                    JitValue::Unit
                }
            }
        }
    }
}
