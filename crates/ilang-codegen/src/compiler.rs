//! `JitCompiler` — drives `Program` → JIT module construction and the
//! ABI-thunked `__main` invocation. The actual lowering machinery lives
//! in `lower_stmt` / `lower_expr` / `lower_op` / `lower_ctrl`.

use std::collections::HashMap;

use cranelift::prelude::*;
use cranelift_codegen::ir::types::{F32, F64, I16, I32, I64, I8};
use cranelift_codegen::settings;
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{FuncId, Linkage, Module};
use ilang_ast::{ClassDecl, EnumDecl, FnDecl, Item, Program, VariantPayload, Symbol};

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
    jit_run_with(
        prog,
        &std::collections::HashMap::new(),
        &std::collections::HashMap::new(),
        &std::collections::HashMap::new(),
        &std::collections::HashMap::new(),
        &std::collections::HashMap::new(),
        &std::collections::HashMap::new(),
    )
}

/// Like `jit_run`, but takes the type checker's per-call inferred type
/// arguments (`fn_call_type_args` for generic fns, `enum_ctor_type_args`
/// for generic enum constructors). Used by the JIT pipeline to
/// monomorphize fns and enums whose type args are inferred from
/// argument types rather than written explicitly. Empty maps fall back
/// to non-generic behaviour.
pub fn jit_run_with(
    prog: &Program,
    fn_call_type_args: &std::collections::HashMap<
        ilang_ast::Span,
        (Symbol, Vec<ilang_ast::Type>),
    >,
    enum_ctor_type_args: &std::collections::HashMap<
        ilang_ast::Span,
        (Symbol, Vec<ilang_ast::Type>),
    >,
    loop_break_types: &std::collections::HashMap<ilang_ast::Span, ilang_ast::Type>,
    class_method_slots: &std::collections::HashMap<
        Symbol,
        std::collections::HashMap<Symbol, usize>,
    >,
    class_vtable_lens: &std::collections::HashMap<Symbol, usize>,
    fn_expr_captures: &std::collections::HashMap<
        ilang_ast::Span,
        Vec<(Symbol, ilang_ast::Type)>,
    >,
) -> Result<JitValue, CodegenError> {
    // Pipeline:
    //   hoist anon fns → monomorphize classes → monomorphize enums
    //   → monomorphize fns. After all four passes the program contains
    //   zero `Type::Generic` (except built-in `Map`), zero `FnExpr`,
    //   and zero generic decls.
    let (hoisted, closure_meta_in) =
        crate::monomorphize::hoist_anon_fns(prog, fn_expr_captures);
    let mono = crate::monomorphize::monomorphize(&hoisted);
    let mono = crate::monomorphize::monomorphize_enums(&mono, enum_ctor_type_args);
    let mono = crate::monomorphize::monomorphize_fns(&mono, fn_call_type_args);
    jit_run_inner(
        &mono,
        fn_call_type_args,
        loop_break_types,
        class_method_slots,
        class_vtable_lens,
        &closure_meta_in,
    )
}

fn jit_run_inner(
    prog: &Program,
    fn_call_type_args: &std::collections::HashMap<
        ilang_ast::Span,
        (Symbol, Vec<ilang_ast::Type>),
    >,
    loop_break_types: &std::collections::HashMap<ilang_ast::Span, ilang_ast::Type>,
    class_method_slots: &std::collections::HashMap<
        Symbol,
        std::collections::HashMap<Symbol, usize>,
    >,
    class_vtable_lens: &std::collections::HashMap<Symbol, usize>,
    closure_meta_in: &std::collections::HashMap<
        Symbol,
        crate::monomorphize::ClosureMetaIn,
    >,
) -> Result<JitValue, CodegenError> {
    let mut compiler = JitCompiler::new(prog)?;
    compiler.loop_break_types = loop_break_types.clone();
    compiler.class_method_slots = class_method_slots.clone();
    compiler.class_vtable_lens = class_vtable_lens.clone();
    compiler.fn_call_type_args = fn_call_type_args.clone();
    // Materialise synthetic Class/Fn decls from `@extern(C) { ... }`
    // blocks once, up front. Subsequent phases iterate over both
    // `prog.items` and these synthesised decls so the existing
    // pipeline keeps working unchanged.
    let extern_c_classes = synthesize_extern_c_classes(prog);
    let extern_c_fns = synthesize_extern_c_fns(prog);
    // 1a. Register every class / enum name → id, with empty layouts.
    //     This way `Child { p: Parent.weak }` resolves even when Parent
    //     is declared after Child, and likewise for enum forward-refs.
    //     This must happen BEFORE closure-meta conversion so captures
    //     of class types resolve to the correct JIT class id.
    // Register the built-in `TypeKind` enum first so its id is
    // stable across runs and `lower_typeof` can refer to it without
    // requiring user code to mention the name.
    compiler.declare_typekind_enum();
    for item in &prog.items {
        match item {
            Item::Class(c) => compiler.declare_class_name(c)?,
            Item::Enum(e) => compiler.declare_enum_layout(e)?,
            _ => {}
        }
    }
    for c in &extern_c_classes {
        compiler.declare_class_name(c)?;
    }
    // Convert each AST-level closure meta to JitTy form.
    for (name, meta) in closure_meta_in {
        let user_params: Vec<crate::ty::JitTy> = meta
            .user_param_tys
            .iter()
            .map(|t| crate::ty::JitTy::from_ast(
                t,
                meta.span,
                &compiler.class_ids,
                &compiler.enum_ids,
                &compiler.enum_layouts,
                &mut compiler.array_kinds,
                &mut compiler.optional_inners,
                &mut compiler.fn_signatures,
                &mut compiler.map_kinds,
                &mut compiler.tuple_kinds,
            ))
            .collect::<Result<_, _>>()?;
        let ret = if let Some(rt) = &meta.ret_ty {
            crate::ty::JitTy::from_ast(
                rt,
                meta.span,
                &compiler.class_ids,
                &compiler.enum_ids,
                &compiler.enum_layouts,
                &mut compiler.array_kinds,
                &mut compiler.optional_inners,
                &mut compiler.fn_signatures,
                &mut compiler.map_kinds,
                &mut compiler.tuple_kinds,
            )?
        } else {
            crate::ty::JitTy::Unit
        };
        let mut captures: Vec<(Symbol, crate::ty::JitTy)> = Vec::new();
        for (cn, ct) in &meta.captures {
            let jty = crate::ty::JitTy::from_ast(
                ct,
                meta.span,
                &compiler.class_ids,
                &compiler.enum_ids,
                &compiler.enum_layouts,
                &mut compiler.array_kinds,
                &mut compiler.optional_inners,
                &mut compiler.fn_signatures,
                &mut compiler.map_kinds,
                &mut compiler.tuple_kinds,
            )?;
            captures.push((cn.clone(), jty));
        }
        compiler
            .closure_ast_captures
            .insert(name.clone(), meta.captures.clone());
        compiler.closure_meta.insert(
            name.clone(),
            crate::env::ClosureMeta { user_params, ret, captures },
        );
    }
    // 1b. Compute field offsets now that every class id is in
    //     `class_ids`. Enums were finalized at declaration time
    //     (variants don't refer to other types in Phase 1).
    //
    //     `@extern(C) struct`es can embed each other inline (nested
    //     struct field), so the inner must be laid out before the
    //     outer. We sort the classes into dependency order with a
    //     small DFS topological sort — declaration order then no
    //     longer matters. A cycle is reported as an error.
    let mut class_decls: Vec<&ClassDecl> = prog
        .items
        .iter()
        .filter_map(|i| if let Item::Class(c) = i { Some(c) } else { None })
        .collect();
    class_decls.extend(extern_c_classes.iter());
    let order = topo_sort_classes(&class_decls)?;
    for idx in order {
        compiler.finalize_class_layout(class_decls[idx])?;
    }
    // 2. Declare every fn / method signature so cross-references resolve.
    for item in &prog.items {
        match item {
            Item::Fn(f) => compiler.declare_fn(f)?,
            Item::Class(c) => compiler.declare_methods(c)?,
            Item::Enum(_) => {}
            Item::Use(_) | Item::Const(_) | Item::ExternStatic(_) | Item::ExternC(_) => {}
        }
    }
    for f in &extern_c_fns {
        compiler.declare_fn(f)?;
    }
    for c in &extern_c_classes {
        // The struct/union variants are field-only and have no
        // methods, so this is a no-op for them. Only the
        // user-declared `class` variant inside `@extern(C) {}`
        // contributes here.
        if !c.methods.is_empty() || !c.static_methods.is_empty() {
            compiler.declare_methods(c)?;
        }
    }
    // 2b. Declare per-class drop wrappers so `new` lowering can embed
    //     their function pointers in the allocation header. Bodies are
    //     defined later (need user methods to be defined first).
    crate::drops::declare_class_drops(&mut compiler)?;
    // 2c. Build RTTI `TypeMeta` table. Must run before
    //     `allocate_vtables` so the latter can refer to per-class
    //     TypeMeta addresses by index. Stable storage in
    //     `compiler.type_metas` (capacity reserved upfront so
    //     pointers stay valid).
    compiler.build_type_metas();
    // 2d. Allocate per-class vtable storage (zeroed for now). Stable
    //     addresses are needed before lowering since `new` and
    //     virtual call sites embed them as `iconst`. Actual function
    //     pointers are written in by `populate_vtables` after
    //     `module.finalize_definitions()`.
    compiler.allocate_vtables();
    // 2d. Build the parent map now that class layouts exist.
    for layout in compiler.class_layouts.clone() {
        if let Some(p) = layout.parent {
            compiler.class_parents.insert(layout.name, p);
        }
    }
    // 3. Define every body.
    for item in &prog.items {
        match item {
            Item::Fn(f) => compiler.define_fn(f)?,
            Item::Class(c) => compiler.define_methods(c)?,
            Item::Enum(_) => {}
            Item::Use(_) | Item::Const(_) | Item::ExternStatic(_) | Item::ExternC(_) => {}
        }
    }
    for f in &extern_c_fns {
        compiler.define_fn(f)?;
    }
    for c in &extern_c_classes {
        if !c.methods.is_empty() || !c.static_methods.is_empty() {
            compiler.define_methods(c)?;
        }
    }
    let main_ret = compiler.define_main(prog)?;
    // 4. Define drop wrappers. Class drops can reference user deinit;
    //    array drops were declared lazily during lowering.
    crate::drops::define_class_drops(&mut compiler)?;
    crate::drops::define_array_drops(&mut compiler)?;
    crate::drops::define_enum_drops(&mut compiler)?;
    crate::drops::define_map_drops(&mut compiler)?;
    crate::drops::define_map_value_retains(&mut compiler)?;
    crate::drops::define_tuple_drops(&mut compiler)?;
    crate::drops::define_closure_drops(&mut compiler)?;
    compiler.finalize()?;
    Ok(compiler.run_main(main_ret))
}

/// Walk every class's `static_fields`, assign slot indices, and
/// pack initial values (folded literals) into a `Box<[i64]>`.
/// Returns `(storage, slot_map, type_map)`. The storage is i64-wide
/// per slot — f64 is bit-cast, bool is 0/1.
fn init_static_field_storage(
    prog: &Program,
) -> (
    Box<[i64]>,
    std::collections::HashMap<(Symbol, Symbol), usize>,
    std::collections::HashMap<(Symbol, Symbol), ilang_ast::Type>,
) {
    use ilang_ast::ExprKind;
    let mut slots: std::collections::HashMap<(Symbol, Symbol), usize> =
        std::collections::HashMap::new();
    let mut types: std::collections::HashMap<(Symbol, Symbol), ilang_ast::Type> =
        std::collections::HashMap::new();
    let mut values: Vec<i64> = Vec::new();
    let record_class = |c: &ilang_ast::ClassDecl,
                             values: &mut Vec<i64>,
                             slots: &mut std::collections::HashMap<(Symbol, Symbol), usize>,
                             types: &mut std::collections::HashMap<(Symbol, Symbol), ilang_ast::Type>| {
        for sf in &c.static_fields {
            // Array-typed statics get a 0 (null) seed here;
            // `__main`'s prologue allocates a real empty array
            // and stores the pointer before any user code runs.
            let bits = match &sf.value.kind {
                ExprKind::Int(n) => *n,
                ExprKind::Float(x) => x.to_bits() as i64,
                ExprKind::Bool(b) => *b as i64,
                ExprKind::Array(_) => 0,
                // The loader already folded; if anything else
                // shows up the typechecker will reject the
                // declaration before we get here.
                _ => 0,
            };
            let idx = values.len();
            values.push(bits);
            slots.insert((c.name.clone(), sf.name.clone()), idx);
            types.insert((c.name.clone(), sf.name.clone()), sf.ty.clone());
        }
    };
    for item in &prog.items {
        match item {
            Item::Class(c) => record_class(c, &mut values, &mut slots, &mut types),
            Item::ExternC(b) => {
                for inner in &b.items {
                    if let ilang_ast::ExternCItem::Class(c) = inner {
                        record_class(c, &mut values, &mut slots, &mut types);
                    }
                }
            }
            _ => {}
        }
    }
    (values.into_boxed_slice(), slots, types)
}

pub(crate) struct JitCompiler {
    pub(crate) module: JITModule,
    pub(crate) ctx: cranelift_codegen::Context,
    pub(crate) builder_ctx: FunctionBuilderContext,
    pub(crate) funcs: HashMap<Symbol, (FuncId, Vec<JitTy>, JitTy)>,
    pub(crate) class_ids: HashMap<Symbol, u32>,
    pub(crate) class_layouts: Vec<ClassLayout>,
    pub(crate) class_methods: Vec<HashMap<Symbol, MethodInfo>>,
    pub(crate) enum_ids: HashMap<Symbol, u32>,
    pub(crate) enum_layouts: Vec<EnumLayout>,
    pub(crate) array_kinds: Vec<ArrayKind>,
    pub(crate) optional_inners: Vec<JitTy>,
    pub(crate) fn_signatures: Vec<FnSignature>,
    pub(crate) map_kinds: Vec<crate::ty::MapKind>,
    pub(crate) tuple_kinds: Vec<crate::ty::TupleKind>,
    /// Per-tuple-kind drop helper: walks heap-typed elements and
    /// releases each. Lazily generated; absent when no element is
    /// heap (the runtime sees drop_fn=0 and skips the call).
    pub(crate) tuple_drops: HashMap<u32, Option<FuncId>>,
    /// Per-(K, V) drop helper: a JIT-generated extern "C" fn that the
    /// runtime calls back to release one heap-typed value when a Map
    /// entry is overwritten or the map dies. Lazily generated; absent
    /// when V is non-heap (drop_fn passed to runtime is 0).
    pub(crate) map_drops: HashMap<u32, Option<FuncId>>,
    /// Per-(K, V) value-retain wrapper. Same lifecycle as `map_drops`.
    pub(crate) map_value_retains: HashMap<u32, Option<FuncId>>,
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
    pub(crate) print_type_ref: FuncId,
    pub(crate) type_is_subtype: FuncId,
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
    pub(crate) str_replace_id: FuncId,
    pub(crate) str_slice_id: FuncId,
    pub(crate) str_split_id: FuncId,
    pub(crate) str_to_c_str_id: FuncId,
    pub(crate) free_c_str_id: FuncId,
    pub(crate) c_str_to_string_id: FuncId,
    pub(crate) cstr_array_to_strings_id: FuncId,
    pub(crate) libc_free_id: FuncId,
    pub(crate) alloc_closure_id: FuncId,
    pub(crate) retain_closure_id: FuncId,
    pub(crate) release_closure_id: FuncId,
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
    // Map<K, V> runtime imports.
    pub(crate) map_new_id: FuncId,
    pub(crate) retain_map_id: FuncId,
    pub(crate) release_map_id: FuncId,
    pub(crate) map_set_id: FuncId,
    pub(crate) map_has_id: FuncId,
    pub(crate) map_size_id: FuncId,
    pub(crate) map_delete_id: FuncId,
    pub(crate) map_index_get_id: FuncId,
    pub(crate) map_get_or_null_id: FuncId,
    pub(crate) map_keys_to_array_id: FuncId,
    pub(crate) map_values_to_array_id: FuncId,
    pub(crate) optional_box_new_id: FuncId,
    pub(crate) optional_box_retain_id: FuncId,
    pub(crate) optional_box_release_id: FuncId,
    pub(crate) panic_index_oob_id: FuncId,
    pub(crate) panic_div_zero_id: FuncId,
    pub(crate) panic_unwrap_none_id: FuncId,
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
    /// Per-`loop` expression span → result type (forwarded from the
    /// outer `TypeChecker`). Empty when no `break v` appears anywhere.
    pub(crate) loop_break_types:
        HashMap<ilang_ast::Span, ilang_ast::Type>,
    /// Open `libloading::Library` handles for every dlopen'd
    /// `@extern("libname")` library. Held here so the symbols stay
    /// valid as long as the JIT module does. Never read directly —
    /// just keeps the libraries alive.
    #[allow(dead_code)]
    pub(crate) native_libs: Vec<libloading::Library>,
    /// Names of fns declared with `@extern("libname")` — looked up at
    /// each Call site so the lowering can wrap string args / return
    /// in C-string conversions.
    pub(crate) native_extern_fns: std::collections::HashSet<Symbol>,
    /// Names declared with trailing `...` — printf-style variadics.
    /// The Cranelift call site builds a fresh per-call signature for
    /// these so trailing args flow through with their actual types.
    pub(crate) native_extern_variadic: std::collections::HashSet<Symbol>,
    /// Subset of `native_extern_fns` whose struct args are passed by
    /// value (split into 1–2 i64 chunks at call lowering). Always set
    /// for `@extern(C) {}`-block fns.
    pub(crate) native_extern_by_value: std::collections::HashSet<Symbol>,
    /// Per call-site span → (callee name, inferred type args). The
    /// type checker fills this for generic calls; the JIT reads it
    /// to resolve T at built-in helper sites like `arrayFromCArray<T>`.
    pub(crate) fn_call_type_args: std::collections::HashMap<
        ilang_ast::Span,
        (Symbol, Vec<ilang_ast::Type>),
    >,
    /// Resolved address per `@extern static` name, embedded as
    /// `iconst` at every read/write site so the load/store goes
    /// straight to the C global's storage.
    pub(crate) extern_static_addrs: std::collections::HashMap<Symbol, i64>,
    /// Declared type per `@extern static` name. The lower path uses
    /// it to pick the right Cranelift load/store width.
    pub(crate) extern_static_types: std::collections::HashMap<Symbol, ilang_ast::Type>,
    /// Every `@extern fn` (host or native lib). The fn-pointer arg
    /// marshalling at Call sites uses this to know whether to pass
    /// a raw `func_addr` (extern → C ABI fn pointer) or a closure
    /// box (regular ilang fn → trampoline). Includes the names in
    /// `native_extern_fns` plus all host-side `@extern fn` names.
    pub(crate) extern_fn_names: std::collections::HashSet<Symbol>,
    /// Storage backing every `static` field: each slot is one i64
    /// (for f64 / bool we bit-reinterpret). Allocated as a `Box<[i64]>`
    /// for pointer stability — the JITed code embeds slot addresses as
    /// `iconst`s, and the storage must outlive the JIT module.
    pub(crate) static_field_storage: Box<[i64]>,
    /// `(class, field) -> slot index` into `static_field_storage`.
    pub(crate) static_field_slots:
        std::collections::HashMap<(Symbol, Symbol), usize>,
    /// `(class, field) -> declared type`, kept on the JIT side so
    /// the lowering knows whether to bitcast / truncate.
    pub(crate) static_field_types:
        std::collections::HashMap<(Symbol, Symbol), ilang_ast::Type>,
    /// `(class_name, method_name) -> vtable slot` table forwarded
    /// from the typechecker. Used at virtual-call sites to compute
    /// the per-method index into a class's vtable.
    pub(crate) class_method_slots:
        std::collections::HashMap<Symbol, std::collections::HashMap<Symbol, usize>>,
    /// `class_name -> vtable size` (= max slot index + 1, or 0).
    /// Used to allocate the per-class vtable storage upfront.
    pub(crate) class_vtable_lens: std::collections::HashMap<Symbol, usize>,
    /// Per-class vtable storage. Each `Box<[i64]>` holds function
    /// pointers indexed by slot. Allocated zeroed before lowering;
    /// the actual addresses are written in by `populate_vtables`
    /// after `module.finalize_definitions()`.
    pub(crate) class_vtable_storage: Vec<Box<[i64]>>,
    /// Stable address of each class's vtable storage, indexed by
    /// class_id. Embedded as `iconst` in lowered `new` (passed to
    /// `alloc_object`) and at virtual-call sites.
    pub(crate) class_vtable_addrs: Vec<i64>,
    /// `class -> parent` (single inheritance). Forwarded from the
    /// typechecker so `super.method()` lowering can find the parent.
    pub(crate) class_parents: std::collections::HashMap<Symbol, Symbol>,
    /// Per-closure-wrapper metadata (user-facing sig + capture
    /// list). Filled in by the hoist pass via the typechecker's
    /// `fn_expr_captures` side table.
    pub(crate) closure_meta:
        std::collections::HashMap<Symbol, crate::env::ClosureMeta>,
    /// Lazy cache of trampoline FuncIds for top-level fns whose
    /// addresses are taken at runtime. Built on first encounter.
    pub(crate) closure_trampolines: std::collections::HashMap<Symbol, FuncId>,
    /// Per-closure-wrapper drop fn. `None` if the closure has no
    /// heap captures (no work to do, drop_fn_ptr is 0). Declared
    /// lazily by `closure_drop_fn_ptr`; bodies emitted by
    /// `define_closure_drops` after all closure-construct sites
    /// have been lowered.
    pub(crate) closure_drops: std::collections::HashMap<Symbol, Option<FuncId>>,
    /// Per-wrapper captured names + AST types. The JIT's
    /// post-hoist re-typecheck reads this so wrapper bodies type-
    /// check with captured names pre-bound to their original AST
    /// types.
    pub(crate) closure_ast_captures:
        std::collections::HashMap<Symbol, Vec<(Symbol, ilang_ast::Type)>>,
    /// RTTI: stable storage for `TypeMeta` records returned by
    /// `typeof(x): Type`. Indices are looked up via the address-only
    /// helpers below.
    pub(crate) type_metas: Vec<crate::runtime::TypeMeta>,
    /// `TypeMeta*` for each class, indexed by class_id. Used at the
    /// `typeof(class_value)` lowering to read the metadata pointer
    /// out of the object's vtable header (vtable[-1]).
    pub(crate) class_type_meta_addrs: Vec<i64>,
    /// `TypeMeta*` for each enum, indexed by enum_id (the enum's
    /// runtime type is its monomorphised name — the same for every
    /// value of that enum, so this needs no runtime dispatch).
    pub(crate) enum_type_meta_addrs: Vec<i64>,
    /// `TypeMeta*` for fixed primitive / structural types whose
    /// names don't depend on type arguments. Lookup by the
    /// `JitTy`-shaped key produced by `prim_type_meta_key`.
    pub(crate) prim_type_meta_addrs: std::collections::HashMap<&'static str, i64>,
}

/// How a `@extern(C) struct` struct flows across a `by_value` call boundary.
/// `Chunks(n)` means "split into `n` i64 GPR slots" (≤ 16 B integer
/// struct, AArch64 / SysV composite rule). `Indirect` means "pass a
/// pointer per the platform's struct-by-value ABI" — Cranelift's
/// `ArgumentPurpose::StructArgument(size)` handles the per-target
/// detail (AArch64: hidden pointer; SysV: copy onto stack).
#[derive(Debug, Clone, Copy)]
pub(crate) enum ByValueKind {
    Chunks(u32),
    /// Homogeneous Floating-point Aggregate: 1..=4 fields all of the
    /// same float type (`f32` or `f64`). Passed/returned in FP
    /// registers per AArch64 AAPCS64 (V0..V3) and x86_64 SysV (XMM
    /// regs for doubles, with f32 packed pairs).
    Hfa { elem: JitTy, count: u32 },
    Indirect,
}

pub(crate) fn repr_c_by_value_kind(layout: &crate::ty::ClassLayout) -> ByValueKind {
    // HFA: all fields are the same float type, 1..=4 of them, and
    // the size matches `count * elem_size` (no padding). Sort by
    // offset so we read the layout consistently.
    let mut entries: Vec<(u32, JitTy)> = layout
        .fields
        .values()
        .map(|&(off, ty)| (off, ty))
        .collect();
    entries.sort_by_key(|(off, _)| *off);
    let all_f32 = !entries.is_empty()
        && entries.iter().all(|(_, t)| matches!(t, JitTy::F32));
    let all_f64 = !entries.is_empty()
        && entries.iter().all(|(_, t)| matches!(t, JitTy::F64));
    if (all_f32 || all_f64) && entries.len() >= 1 && entries.len() <= 4 {
        let elem = if all_f32 { JitTy::F32 } else { JitTy::F64 };
        let count = entries.len() as u32;
        if layout.size == count * elem.size_bytes() {
            return ByValueKind::Hfa { elem, count };
        }
    }
    if layout.size == 0 {
        ByValueKind::Chunks(0)
    } else if layout.size <= 8 {
        ByValueKind::Chunks(1)
    } else if layout.size <= 16 {
        ByValueKind::Chunks(2)
    } else {
        ByValueKind::Indirect
    }
}

/// Topological sort of class declarations by inline-embedding edges.
///
/// A `@extern(C) struct` that embeds another `@extern(C) struct` must be
/// laid out *after* the embedded one (we need `inner.size` to assign
/// the field offset and grow the outer's size). Inheritance also
/// creates a parent-before-child dependency. The sort lets users
/// declare classes in any order; a cycle (which would mean an
/// infinite-size struct) is reported as an error.
/// Materialise a `ClassDecl` for every `struct` / `union` declared
/// inside an `@extern(C) { ... }` block. The synthesised decls go
/// through the same layout / drop / vtable pipeline as user-written
/// `@extern(C) struct` decls — `is_repr_c` is always set, and `is_packed`
/// / `is_union` flow through from the block's attributes.
pub(crate) fn synthesize_extern_c_classes(prog: &Program) -> Vec<ClassDecl> {
    let mut out = Vec::new();
    for item in &prog.items {
        let Item::ExternC(block) = item else { continue };
        for inner in &block.items {
            match inner {
                ilang_ast::ExternCItem::Struct {
                    name, fields, is_packed, span,
                } => {
                    out.push(ClassDecl {
                        name: name.clone(),
                        type_params: Box::new([]),
                        parent: None,
                        fields: fields.clone(),
                        methods: Box::new([]),
                        static_methods: Box::new([]),
                        static_fields: Box::new([]),
                        properties: Box::new([]),
                        extern_lib: None,
                        is_repr_c: true,
                        is_packed: *is_packed,
                        is_union: false,
                        span: *span,
                    });
                }
                ilang_ast::ExternCItem::Union { name, fields, span } => {
                    out.push(ClassDecl {
                        name: name.clone(),
                        type_params: Box::new([]),
                        parent: None,
                        fields: fields.clone(),
                        methods: Box::new([]),
                        static_methods: Box::new([]),
                        static_fields: Box::new([]),
                        properties: Box::new([]),
                        extern_lib: None,
                        is_repr_c: true,
                        is_packed: false,
                        is_union: true,
                        span: *span,
                    });
                }
                ilang_ast::ExternCItem::Class(c) => {
                    // Plain ilang ARC-managed class declared next to
                    // its FFI bindings. Pass through unchanged — the
                    // type checker / JIT pipeline treats it like any
                    // top-level `class`.
                    out.push(c.clone());
                }
                _ => {}
            }
        }
    }
    out
}

/// Materialise a `FnDecl` for every fn declared inside an
/// `@extern(C) { ... }` block. Decl-only items become `@extern(C[,
/// "lib"])` fns (no body, dlsym'd / host-registered); definitions
/// reuse the parsed body unchanged with a synthetic `@extern(C)`
/// attribute so the JIT applies the C calling convention.
pub(crate) fn synthesize_extern_c_statics(
    prog: &Program,
) -> Vec<ilang_ast::ExternStaticDecl> {
    let mut out = Vec::new();
    for item in &prog.items {
        let Item::ExternC(block) = item else { continue };
        for inner in &block.items {
            if let ilang_ast::ExternCItem::Static { name, ty, libs, optional: _, span } = inner {
                // `@optional` on statics is parsed but currently
                // ignored at registration time — host-form statics
                // must always be registered, and the only library
                // form path is dlsym which propagates lookup errors.
                out.push(ilang_ast::ExternStaticDecl {
                    name: name.clone(),
                    ty: ty.clone(),
                    lib: libs.first().cloned(),
                    span: *span,
                });
            }
        }
    }
    out
}

pub(crate) fn synthesize_extern_c_fns(prog: &Program) -> Vec<ilang_ast::FnDecl> {
    use ilang_ast::AttrArg;
    let mut out = Vec::new();
    for item in &prog.items {
        let Item::ExternC(block) = item else { continue };
        for inner in &block.items {
            match inner {
                ilang_ast::ExternCItem::FnDecl {
                    name, params, ret, libs, optional, variadic, c_symbol, span,
                } => {
                    // `@extern("libname", ...)` for the dlsym path;
                    // bare `@extern` for host-side. `@optional` maps
                    // to the `optional` flag. Append `byValue` so
                    // struct args pass by value (matches C ABI for
                    // extern(C) declarations — pointer struct args
                    // are written as `*MyStruct` and don't trigger
                    // the by_value chunk path).
                    let mut attr_args: Vec<AttrArg> =
                        libs.iter().map(|s| AttrArg::Str(s.as_str().to_string())).collect();
                    if *optional {
                        attr_args.push(AttrArg::Path(Box::new([Symbol::intern("optional")])));
                    }
                    if *variadic {
                        attr_args.push(AttrArg::Path(Box::new([Symbol::intern("variadic")])));
                    }
                    attr_args.push(AttrArg::Path(Box::new([Symbol::intern("byValue")])));
                    // `@symbol("name")` rides through as a separate
                    // attribute so native_extern can use it for the
                    // dlsym lookup while keeping the ilang-side name
                    // for everything else.
                    let mut attrs = vec![ilang_ast::Attribute {
                        name: "extern".into(),
                        args: attr_args.into(),
                    }];
                    if let Some(sym) = c_symbol {
                        attrs.push(ilang_ast::Attribute {
                            name: "symbol".into(),
                            args: Box::new([AttrArg::Str(sym.as_str().to_string())]),
                        });
                    }
                    out.push(ilang_ast::FnDecl {
                        attrs: attrs.into(),
                        name: name.clone(),
                        type_params: Box::new([]),
                        params: params.clone(),
                        ret: ret.clone(),
                        body: ilang_ast::Block {
                            stmts: Vec::new(),
                            tail: None,
                        },
                        span: *span,
                        is_override: false,
                    });
                }
                ilang_ast::ExternCItem::FnDef(f) => {
                    out.push(f.clone());
                }
                _ => {}
            }
        }
    }
    out
}

fn topo_sort_classes(classes: &[&ClassDecl]) -> Result<Vec<usize>, CodegenError> {
    use std::collections::HashSet;
    let mut name_to_idx = HashMap::with_capacity(classes.len());
    for (i, c) in classes.iter().enumerate() {
        name_to_idx.insert(c.name.clone(), i);
    }
    let deps_of = |c: &ClassDecl| -> Vec<usize> {
        let mut out = Vec::new();
        if let Some(p) = &c.parent {
            if let Some(&i) = name_to_idx.get(p) {
                out.push(i);
            }
        }
        for f in &c.fields {
            // Only nested-by-value fields create a layout dependency.
            // `@extern(C) struct` -> `@extern(C) struct` Object, or any fixed-length
            // array of a class type, embeds bytes inline.
            let referenced_class = match &f.ty {
                ilang_ast::Type::Object(name) => Some(name),
                _ => None,
            };
            if let Some(name) = referenced_class {
                if let Some(&i) = name_to_idx.get(name) {
                    if c.is_repr_c && classes[i].is_repr_c {
                        out.push(i);
                    }
                }
            }
        }
        out
    };
    let mut order = Vec::with_capacity(classes.len());
    let mut visited = vec![false; classes.len()];
    let mut on_stack = HashSet::new();
    fn dfs(
        i: usize,
        classes: &[&ClassDecl],
        deps_of: &dyn Fn(&ClassDecl) -> Vec<usize>,
        visited: &mut [bool],
        on_stack: &mut std::collections::HashSet<usize>,
        order: &mut Vec<usize>,
    ) -> Result<(), CodegenError> {
        if visited[i] {
            return Ok(());
        }
        if !on_stack.insert(i) {
            return Err(CodegenError::Unsupported {
                what: format!(
                    "@extern(C) struct {:?} participates in a cyclic embedding \
                     (would require an infinite-size struct)",
                    classes[i].name
                ),
                span: classes[i].span,
            });
        }
        for d in deps_of(classes[i]) {
            dfs(d, classes, deps_of, visited, on_stack, order)?;
        }
        on_stack.remove(&i);
        visited[i] = true;
        order.push(i);
        Ok(())
    }
    for i in 0..classes.len() {
        dfs(i, classes, &deps_of, &mut visited, &mut on_stack, &mut order)?;
    }
    Ok(order)
}

impl JitCompiler {
    fn new(prog: &Program) -> Result<Self, CodegenError> {
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
        builder.symbol(
            "ilang_jit_print_type_ref",
            crate::runtime::ilang_jit_print_type_ref as *const u8,
        );
        builder.symbol(
            "ilang_jit_type_is_subtype",
            crate::runtime::ilang_jit_type_is_subtype as *const u8,
        );
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
        builder.symbol("ilang_jit_str_replace", crate::runtime::ilang_jit_str_replace as *const u8);
        builder.symbol("ilang_jit_str_slice", crate::runtime::ilang_jit_str_slice as *const u8);
        builder.symbol("ilang_jit_str_split", crate::runtime::ilang_jit_str_split as *const u8);
        builder.symbol("ilang_jit_str_to_c_str", crate::runtime::ilang_jit_str_to_c_str as *const u8);
        builder.symbol("ilang_jit_free_c_str", crate::runtime::ilang_jit_free_c_str as *const u8);
        builder.symbol("ilang_jit_c_str_to_string", crate::runtime::ilang_jit_c_str_to_string as *const u8);
        builder.symbol(
            "ilang_jit_cstr_array_to_strings",
            crate::runtime::ilang_jit_cstr_array_to_strings as *const u8,
        );
        builder.symbol("ilang_jit_libc_free", crate::runtime::ilang_jit_libc_free as *const u8);
        builder.symbol("ilang_jit_alloc_closure", crate::runtime::ilang_jit_alloc_closure as *const u8);
        builder.symbol("ilang_jit_retain_closure", crate::runtime::ilang_jit_retain_closure as *const u8);
        builder.symbol("ilang_jit_release_closure", crate::runtime::ilang_jit_release_closure as *const u8);
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
        // Map<K, V> runtime symbols.
        builder.symbol("ilang_jit_map_new", crate::runtime::ilang_jit_map_new as *const u8);
        builder.symbol("ilang_jit_retain_map", crate::runtime::ilang_jit_retain_map as *const u8);
        builder.symbol("ilang_jit_release_map", crate::runtime::ilang_jit_release_map as *const u8);
        builder.symbol("ilang_jit_map_set", crate::runtime::ilang_jit_map_set as *const u8);
        builder.symbol("ilang_jit_map_has", crate::runtime::ilang_jit_map_has as *const u8);
        builder.symbol("ilang_jit_map_size", crate::runtime::ilang_jit_map_size as *const u8);
        builder.symbol("ilang_jit_map_delete", crate::runtime::ilang_jit_map_delete as *const u8);
        builder.symbol("ilang_jit_map_index_get", crate::runtime::ilang_jit_map_index_get as *const u8);
        builder.symbol("ilang_jit_map_get_or_null", crate::runtime::ilang_jit_map_get_or_null as *const u8);
        builder.symbol("ilang_jit_map_keys_to_array", crate::runtime::ilang_jit_map_keys_to_array as *const u8);
        builder.symbol("ilang_jit_map_values_to_array", crate::runtime::ilang_jit_map_values_to_array as *const u8);
        builder.symbol("ilang_jit_optional_box_new", crate::runtime::ilang_jit_optional_box_new as *const u8);
        builder.symbol("ilang_jit_optional_box_retain", crate::runtime::ilang_jit_optional_box_retain as *const u8);
        builder.symbol("ilang_jit_optional_box_release", crate::runtime::ilang_jit_optional_box_release as *const u8);
        builder.symbol("ilang_jit_panic_index_oob", crate::runtime::ilang_jit_panic_index_oob as *const u8);
        builder.symbol("ilang_jit_panic_div_zero", crate::runtime::ilang_jit_panic_div_zero as *const u8);
        builder.symbol("ilang_jit_panic_unwrap_none", crate::runtime::ilang_jit_panic_unwrap_none as *const u8);
        // Built-in `@extern` math fns. The names match the qualified
        // form produced by the loader (`math.sin`, etc.).
        crate::math_externs::register_math_symbols(&mut builder);
        crate::test_externs::register_test_symbols(&mut builder);
        crate::os_externs::register_os_symbols(&mut builder);
        // User `@extern("libfoo")` fns: dlopen each named library
        // (deduplicated) and register each symbol the program names.
        // The Library handles must outlive the JITModule, so we stash
        // them on the JitCompiler.
        let native_reg = crate::native_extern::register_native_externs(&mut builder, prog)?;
        // Pre-allocate static-field storage. Each slot is i64 wide
        // and covers i64/f64/bool (the latter two via bitcast). The
        // initial value comes from each field's folded literal.
        let (static_field_storage, static_field_slots, static_field_types) =
            init_static_field_storage(prog);
        // Use an arena-backed memory provider so every JIT'd function
        // lands in a single contiguous reservation. AArch64 BL has a
        // ±128MB range; without this, mmap can scatter functions far
        // enough apart that local fn-to-fn calls overflow the
        // relocation, panicking at finalize or producing SIGBUS at
        // runtime. 64 MiB easily fits everything we generate today.
        builder.memory_provider(Box::new(
            cranelift_jit::ArenaMemoryProvider::new_with_size(64 * 1024 * 1024)
                .map_err(|e| CodegenError::Cranelift(format!("arena: {e}")))?,
        ));
        let mut module = JITModule::new(builder);
        let ctx = module.make_context();

        // Declare signatures for every imported runtime function.
        let alloc_object_id = declare_import(
            &mut module,
            "ilang_jit_alloc_object",
            &[I64, I64, I64],
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
        let print_type_ref = declare_import(&mut module, "ilang_jit_print_type_ref", &[I64], None)?;
        let type_is_subtype =
            declare_import(&mut module, "ilang_jit_type_is_subtype", &[I64, I64], Some(I8))?;
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
        let str_replace_id =
            declare_import(&mut module, "ilang_jit_str_replace", &[I64, I64, I64], Some(I64))?;
        let str_slice_id =
            declare_import(&mut module, "ilang_jit_str_slice", &[I64, I64, I64], Some(I64))?;
        let str_split_id =
            declare_import(&mut module, "ilang_jit_str_split", &[I64, I64, I64], Some(I64))?;
        let str_to_c_str_id =
            declare_import(&mut module, "ilang_jit_str_to_c_str", &[I64], Some(I64))?;
        let free_c_str_id =
            declare_import(&mut module, "ilang_jit_free_c_str", &[I64], None)?;
        let c_str_to_string_id =
            declare_import(&mut module, "ilang_jit_c_str_to_string", &[I64], Some(I64))?;
        let cstr_array_to_strings_id = declare_import(
            &mut module,
            "ilang_jit_cstr_array_to_strings",
            &[I64, I64],
            Some(I64),
        )?;
        let libc_free_id =
            declare_import(&mut module, "ilang_jit_libc_free", &[I64], None)?;
        let alloc_closure_id =
            declare_import(&mut module, "ilang_jit_alloc_closure", &[I64, I64], Some(I64))?;
        let retain_closure_id =
            declare_import(&mut module, "ilang_jit_retain_closure", &[I64], None)?;
        let release_closure_id =
            declare_import(&mut module, "ilang_jit_release_closure", &[I64], None)?;
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
        // Map<K, V> imports.
        let map_new_id =
            declare_import(&mut module, "ilang_jit_map_new", &[I64, I64], Some(I64))?;
        let retain_map_id =
            declare_import(&mut module, "ilang_jit_retain_map", &[I64], None)?;
        let release_map_id =
            declare_import(&mut module, "ilang_jit_release_map", &[I64], None)?;
        let map_set_id =
            declare_import(&mut module, "ilang_jit_map_set", &[I64, I64, I64], None)?;
        let map_has_id =
            declare_import(&mut module, "ilang_jit_map_has", &[I64, I64], Some(I8))?;
        let map_size_id =
            declare_import(&mut module, "ilang_jit_map_size", &[I64], Some(I64))?;
        let map_delete_id =
            declare_import(&mut module, "ilang_jit_map_delete", &[I64, I64], Some(I8))?;
        let map_index_get_id =
            declare_import(&mut module, "ilang_jit_map_index_get", &[I64, I64], Some(I64))?;
        let map_get_or_null_id =
            declare_import(&mut module, "ilang_jit_map_get_or_null", &[I64, I64], Some(I64))?;
        let map_keys_to_array_id =
            declare_import(&mut module, "ilang_jit_map_keys_to_array", &[I64, I64, I64], Some(I64))?;
        let map_values_to_array_id =
            declare_import(&mut module, "ilang_jit_map_values_to_array", &[I64, I64, I64, I64], Some(I64))?;
        let optional_box_new_id =
            declare_import(&mut module, "ilang_jit_optional_box_new", &[I64], Some(I64))?;
        let optional_box_retain_id =
            declare_import(&mut module, "ilang_jit_optional_box_retain", &[I64], None)?;
        let optional_box_release_id =
            declare_import(&mut module, "ilang_jit_optional_box_release", &[I64, I64], None)?;
        let panic_index_oob_id =
            declare_import(&mut module, "ilang_jit_panic_index_oob", &[I64, I64], None)?;
        let panic_div_zero_id =
            declare_import(&mut module, "ilang_jit_panic_div_zero", &[], None)?;
        let panic_unwrap_none_id =
            declare_import(&mut module, "ilang_jit_panic_unwrap_none", &[], None)?;

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
            map_kinds: Vec::new(),
            map_drops: HashMap::new(),
            map_value_retains: HashMap::new(),
            tuple_kinds: Vec::new(),
            tuple_drops: HashMap::new(),
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
            print_type_ref,
            type_is_subtype,
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
            str_replace_id,
            str_slice_id,
            str_split_id,
            str_to_c_str_id,
            free_c_str_id,
            c_str_to_string_id,
            cstr_array_to_strings_id,
            libc_free_id,
            alloc_closure_id,
            retain_closure_id,
            release_closure_id,
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
            map_new_id,
            retain_map_id,
            release_map_id,
            map_set_id,
            map_has_id,
            map_size_id,
            map_delete_id,
            map_index_get_id,
            map_get_or_null_id,
            map_keys_to_array_id,
            map_values_to_array_id,
            optional_box_new_id,
            optional_box_retain_id,
            optional_box_release_id,
            panic_index_oob_id,
            panic_div_zero_id,
            panic_unwrap_none_id,
            interned_strings: Vec::new(),
            class_drops: Vec::new(),
            array_drops: HashMap::new(),
            enum_drops: HashMap::new(),
            loop_break_types: HashMap::new(),
            native_libs: native_reg.libs,
            native_extern_fns: native_reg.names,
            native_extern_variadic: native_reg.variadic,
            native_extern_by_value: native_reg.by_value,
            fn_call_type_args: std::collections::HashMap::new(),
            extern_static_addrs: native_reg.static_addrs,
            extern_static_types: prog
                .items
                .iter()
                .filter_map(|i| match i {
                    Item::ExternStatic(s) => Some((s.name.clone(), s.ty.clone())),
                    _ => None,
                })
                .chain(synthesize_extern_c_statics(prog).into_iter().map(|s| (s.name, s.ty)))
                .collect(),
            extern_fn_names: prog
                .items
                .iter()
                .filter_map(|item| match item {
                    Item::Fn(f) if f.attrs.iter().any(|a| a.name == "extern") => {
                        Some(f.name.clone())
                    }
                    _ => None,
                })
                .chain(synthesize_extern_c_fns(prog).iter().filter_map(|f| {
                    f.attrs.iter().any(|a| a.name == "extern").then(|| f.name.clone())
                }))
                .collect(),
            static_field_storage,
            static_field_slots,
            static_field_types,
            class_method_slots: std::collections::HashMap::new(),
            class_vtable_lens: std::collections::HashMap::new(),
            class_vtable_storage: Vec::new(),
            class_vtable_addrs: Vec::new(),
            class_parents: std::collections::HashMap::new(),
            closure_meta: std::collections::HashMap::new(),
            closure_trampolines: std::collections::HashMap::new(),
            closure_drops: std::collections::HashMap::new(),
            closure_ast_captures: std::collections::HashMap::new(),
            type_metas: Vec::new(),
            class_type_meta_addrs: Vec::new(),
            enum_type_meta_addrs: Vec::new(),
            prim_type_meta_addrs: std::collections::HashMap::new(),
        })
    }

    /// Register an enum's layout. For unit-only enums the runtime is a
    /// bare i32 tag. For enums with at least one payload variant we
    /// compute per-variant payload offsets and the max payload size
    /// for the tagged-union allocation. Resolving inner field types
    /// piggybacks on `JitTy::from_ast`, which can see the in-progress
    /// `enum_layouts` table for forward refs.
    /// Synthesize the built-in `TypeKind` enum's codegen layout.
    /// `TypeKind` is reserved at the type-checker level (so user code
    /// can't shadow it), which means it never appears as an
    /// `Item::Enum` here — we install it directly. Variant order
    /// must match the type checker's registration so the runtime
    /// tag values agree.
    fn declare_typekind_enum(&mut self) {
        let id = self.enum_layouts.len() as u32;
        self.enum_ids.insert("TypeKind".into(), id);
        let variants: Vec<Symbol> = [
            "primitive", "class", "enum", "optional", "array", "fn",
            "tuple", "string", "unit",
        ]
        .iter()
        .map(|s| (*s).into())
        .collect();
        let tags: Vec<i64> = (0..variants.len() as i64).collect();
        let n_variants = variants.len();
        self.enum_layouts.push(EnumLayout {
            name: "TypeKind".into(),
            variants,
            tags,
            all_unit: true,
            payloads: vec![EnumVariantLayout::Unit; n_variants],
            max_payload_size: 0,
            flags: false,
            flags_repr: None,
        });
    }

    fn declare_enum_layout(&mut self, e: &EnumDecl) -> Result<(), CodegenError> {
        let id = self.enum_layouts.len() as u32;
        self.enum_ids.insert(e.name.clone(), id);
        let all_unit = e
            .variants
            .iter()
            .all(|v| matches!(v.payload, VariantPayload::Unit));
        // Discriminant tags — explicit `variant = N` if given, else
        // `previous + 1` (with the leading variant defaulting to 0).
        // Mirrors C / Rust enum semantics.
        let mut tags = Vec::with_capacity(e.variants.len());
        let mut next: i64 = 0;
        for v in &e.variants {
            let t = v.discriminant.unwrap_or(next);
            tags.push(t);
            next = t + 1;
        }
        // Push a placeholder so JitTy::from_ast can see the entry while
        // we resolve payload field types.
        // For `@flags` enums, decide the underlying integer representation
        // up-front (default `u64`, matching the language's default int).
        let flags_repr = if e.flags {
            let jty = match e.repr_ty.as_ref() {
                Some(ilang_ast::Type::I8) => JitTy::I8,
                Some(ilang_ast::Type::I16) => JitTy::I16,
                Some(ilang_ast::Type::I32) => JitTy::I32,
                Some(ilang_ast::Type::I64) => JitTy::I64,
                Some(ilang_ast::Type::U8) => JitTy::U8,
                Some(ilang_ast::Type::U16) => JitTy::U16,
                Some(ilang_ast::Type::U32) => JitTy::U32,
                None | Some(ilang_ast::Type::U64) => JitTy::U64,
                Some(other) => {
                    return Err(CodegenError::Unsupported {
                        what: format!("@flags repr must be a numeric integer type, got {other:?}"),
                        span: e.span,
                    });
                }
            };
            Some(jty)
        } else {
            None
        };
        self.enum_layouts.push(EnumLayout {
            name: e.name.clone(),
            variants: e.variants.iter().map(|v| v.name.clone()).collect(),
            tags,
            all_unit,
            payloads: vec![EnumVariantLayout::Unit; e.variants.len()],
            max_payload_size: 0,
            flags: e.flags,
            flags_repr,
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
                            &mut self.optional_inners, &mut self.fn_signatures, &mut self.map_kinds, &mut self.tuple_kinds,
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
                    let mut map: HashMap<Symbol, (u32, JitTy)> = HashMap::new();
                    for f in fields {
                        let jty = JitTy::from_ast(
                            &f.ty,
                            f.span,
                            &self.class_ids,
                            &self.enum_ids,
                            &self.enum_layouts,
                            &mut self.array_kinds,
                            &mut self.optional_inners, &mut self.fn_signatures, &mut self.map_kinds, &mut self.tuple_kinds,
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
    fn declare_class_name(&mut self, c: &ClassDecl) -> Result<(), CodegenError> {
        let id = self.class_layouts.len() as u32;
        self.class_ids.insert(c.name.clone(), id);
        self.class_layouts.push(ClassLayout {
            name: c.name.clone(),
            fields: HashMap::new(),
            size: 0,
            parent: c.parent.clone(),
            extern_lib: c.extern_lib.clone(),
            is_repr_c: c.is_repr_c,
            align: 1,
            bitfields: HashMap::new(),
            flex_array: None,
        });
        self.class_methods.push(HashMap::new());
        Ok(())
    }

    /// Second pass: compute field offsets/sizes now that every class
    /// id is in the table. Splits out from `declare_class_name` so
    /// `Child { p: Parent.weak }` works when Parent is declared after
    /// Child in source order.
    fn finalize_class_layout(&mut self, c: &ClassDecl) -> Result<(), CodegenError> {
        let id = self.class_ids[&c.name] as usize;
        // Inheritance: start the child's layout from the parent's
        // field map and end-of-fields offset. Inherited fields keep
        // their parent's offsets (so a `Parent*` reading a `Child`
        // sees the same memory) and the child's added fields go
        // after.
        let (mut offset, mut max_align, mut fields) =
            if let Some(parent_name) = &c.parent {
                let pid = self.class_ids[parent_name] as usize;
                let parent = &self.class_layouts[pid];
                (parent.size, 1u32, parent.fields.clone())
            } else {
                (0u32, 1u32, HashMap::new())
            };
        let mut bitfields: HashMap<Symbol, crate::ty::BitfieldInfo> = HashMap::new();
        // Active bitfield run state (GCC-style packing): consecutive
        // `@bits` fields of the same underlying integer width share
        // one storage unit at `bf_unit_offset`. The run closes when
        // a non-bitfield arrives, the underlying type changes, or
        // the next field's width wouldn't fit in the remaining bits.
        let mut bf_unit_offset: u32 = 0;
        let mut bf_unit_size: u32 = 0; // 0 = no open run
        let mut bf_used_bits: u32 = 0;
        for field in &c.fields {
            let jty = JitTy::from_ast(
                &field.ty,
                field.span,
                &self.class_ids,
                &self.enum_ids,
                &self.enum_layouts,
                &mut self.array_kinds,
                &mut self.optional_inners, &mut self.fn_signatures, &mut self.map_kinds, &mut self.tuple_kinds,
            )?;
            // Bitfield: pack into a shared storage unit. The
            // type-checker restricts these to unsigned ints inside a
            // `@extern(C) struct`, so jty here is one of U8/U16/U32/U64.
            if let Some(width) = field.bits {
                let unit = jty.size_bytes();
                let unit_bits = unit * 8;
                let need_new_unit = bf_unit_size == 0
                    || bf_unit_size != unit
                    || bf_used_bits + width > unit_bits;
                if need_new_unit {
                    // Close any previous run first.
                    if bf_unit_size != 0 {
                        offset = bf_unit_offset + bf_unit_size;
                    }
                    offset = align_up(offset, unit);
                    bf_unit_offset = offset;
                    bf_unit_size = unit;
                    bf_used_bits = 0;
                    max_align = max_align.max(unit);
                }
                fields.insert(field.name.clone(), (bf_unit_offset, jty));
                bitfields.insert(
                    field.name.clone(),
                    crate::ty::BitfieldInfo {
                        bit_offset: bf_used_bits,
                        width,
                    },
                );
                bf_used_bits += width;
                continue;
            }
            // Non-bitfield: close any open bitfield run before placing.
            if bf_unit_size != 0 {
                offset = bf_unit_offset + bf_unit_size;
                bf_unit_size = 0;
                bf_used_bits = 0;
            }
            // Embedded nested struct: a `@extern(C) struct` field of another
            // `@extern(C) struct` lays its bytes inline (same as C
            // `struct A { struct B b; }`).
            //
            // Embedded numeric array: a `T[N]` field of a `@extern(C) struct`
            // class lays its bytes inline (same as C `T arr[N];`)
            // and the field's JIT type becomes `EmbeddedArray` so
            // index access knows to compute `base + i * elem_size`
            // rather than dereferencing a heap header.
            let (size, align, recorded_jty) = if let JitTy::Object(inner_id) = jty {
                let inner = &self.class_layouts[inner_id as usize];
                if c.is_repr_c && inner.is_repr_c {
                    // The topo sort guarantees the inner is laid out
                    // first. A zero-size inner here means the struct
                    // genuinely has no fields, which is allowed but
                    // unusual — fall through with size 0.
                    (inner.size.max(0), inner.align.max(1), jty)
                } else {
                    (jty.size_bytes(), jty.size_bytes().max(1), jty)
                }
            } else if let JitTy::Array(arr_id) = jty {
                let kind = self.array_kinds[arr_id as usize];
                if c.is_repr_c {
                    if let Some(n) = kind.fixed {
                        let elem_size = kind.elem.size_bytes();
                        let total = n * elem_size;
                        let align = elem_size.max(1);
                        (total, align, JitTy::EmbeddedArray(arr_id))
                    } else {
                        // Flexible array member — last field, no
                        // fixed length. Size is 0 in the layout;
                        // `new` widens the allocation by `n *
                        // elem_size` at runtime.
                        let elem_size = kind.elem.size_bytes();
                        let align = elem_size.max(1);
                        (0u32, align, JitTy::FlexArray(arr_id))
                    }
                } else {
                    (jty.size_bytes(), jty.size_bytes().max(1), jty)
                }
            } else {
                (jty.size_bytes(), jty.size_bytes().max(1), jty)
            };
            // `@extern(C) union`: every field sits at offset 0; the
            // class size becomes the maximum field size and the
            // alignment becomes the maximum field alignment. Writing
            // one field overwrites the others — the type checker
            // already restricted fields to non-heap primitives so
            // ARC integrity isn't at risk.
            //
            // `@packed`: every field sits at the next byte
            // (no alignment padding). Bitfield runs ignore packed —
            // they already use the storage unit width directly.
            let (field_offset, advance) = if c.is_union {
                (0u32, 0u32)
            } else {
                let effective_align = if c.is_packed { 1 } else { align };
                let aligned = align_up(offset, effective_align);
                (aligned, aligned + size - offset)
            };
            let effective_align = if c.is_packed && !c.is_union { 1 } else { align };
            fields.insert(field.name.clone(), (field_offset, recorded_jty));
            if c.is_union {
                // union size = max(field sizes). Track via offset.
                offset = offset.max(size);
            } else {
                offset += advance;
            }
            max_align = max_align.max(effective_align);
        }
        // Close any trailing bitfield run.
        if bf_unit_size != 0 {
            offset = bf_unit_offset + bf_unit_size;
        }
        // Packed structs have no end padding either: total size is
        // the sum of field sizes (already in `offset`).
        let size = if c.is_packed {
            offset.max(1)
        } else {
            align_up(offset.max(1), max_align)
        };
        self.class_layouts[id].fields = fields;
        self.class_layouts[id].align = max_align;
        self.class_layouts[id].bitfields = bitfields;
        // Record FAM: scan the fields map for any FlexArray entry.
        // (At most one — type checker enforces it as the last field.)
        let flex_array = self.class_layouts[id]
            .fields
            .values()
            .find_map(|(_, ty)| match ty {
                JitTy::FlexArray(arr_id) => Some(*arr_id),
                _ => None,
            });
        self.class_layouts[id].flex_array = flex_array;
        // Opaque-handle classes with a `deinit` get one hidden i64
        // slot at offset 0 — the wrapped C pointer. Without `deinit`
        // the value flows as a raw C pointer (no ilang allocation).
        let opaque_managed = c.extern_lib.is_some()
            && c.methods.iter().any(|m| m.name == "deinit");
        let size = if opaque_managed { 8 } else { size };
        self.class_layouts[id].size = size;
        Ok(())
    }

    fn declare_fn(&mut self, f: &FnDecl) -> Result<(), CodegenError> {
        // After the monomorphization passes, every fn we see should be
        // concrete. If a generic fn slips through, the call site that
        // referenced it had a non-monomorphizable arg context — surface
        // the failure here rather than panicking deeper in lowering.
        if !f.type_params.is_empty() {
            return Err(CodegenError::Unsupported {
                what: format!(
                    "generic fn {:?} reached the JIT — no concrete instantiation found",
                    f.name
                ),
                span: f.span,
            });
        }
        let (id, params, ret) = self.declare_fn_signature(f.name.as_str(), f, None)?;
        self.funcs.insert(f.name.clone(), (id, params, ret));
        Ok(())
    }

    /// Declare every method of a class as a top-level function with
    /// `this` as the implicit first parameter. Constructor (`init`) is
    /// no different from a regular method here — the special handling
    /// lives in the `new` lowering.
    fn declare_methods(&mut self, c: &ClassDecl) -> Result<(), CodegenError> {
        let class_id = self.class_ids[&c.name];
        // Inheritance: prepopulate this class's method table from
        // the parent's. Inherited methods that aren't overridden
        // resolve to the parent's compiled function (param ty stays
        // Object(parent_id) — the JIT pointer to a Child is layout-
        // compatible since headers and inherited fields match).
        // `init` and `deinit` are per-class — don't inherit them.
        if let Some(parent_name) = &c.parent {
            let pid = self.class_ids[parent_name] as usize;
            let parent_methods = self.class_methods[pid].clone();
            for (k, info) in parent_methods {
                if k == "init" || k == "deinit" {
                    continue;
                }
                self.class_methods[class_id as usize].insert(k, info);
            }
        }
        for m in &c.methods {
            let symbol = format!("__method_{}_{}", c.name, m.name);
            let (id, params, ret) =
                self.declare_fn_signature(symbol.as_str(), m, Some(JitTy::Object(class_id)))?;
            self.class_methods[class_id as usize].insert(
                m.name.clone(),
                MethodInfo { id, params, ret },
            );
        }
        // Property accessors are declared alongside methods, prefixed
        // with `__prop_get_` / `__prop_set_` so they don't collide with
        // user method names. lower_field / lower_assign_field look them
        // up by these prefixed keys.
        for prop in &c.properties {
            if let Some(g) = &prop.getter {
                let key = format!("__prop_get_{}", prop.name);
                let symbol = format!("__method_{}_{}", c.name, key);
                let (id, params, ret) =
                    self.declare_fn_signature(symbol.as_str(), g, Some(JitTy::Object(class_id)))?;
                self.class_methods[class_id as usize].insert(
                    key.into(),
                    MethodInfo { id, params, ret },
                );
            }
            if let Some(s) = &prop.setter {
                let key = format!("__prop_set_{}", prop.name);
                let symbol = format!("__method_{}_{}", c.name, key);
                let (id, params, ret) =
                    self.declare_fn_signature(symbol.as_str(), s, Some(JitTy::Object(class_id)))?;
                self.class_methods[class_id as usize].insert(
                    key.into(),
                    MethodInfo { id, params, ret },
                );
            }
        }
        // Static methods are registered as plain top-level fns under
        // `<ClassName>.<method>` (matching how the typechecker
        // resolves the call) so the existing `lc.funcs` lookup path
        // can find them without a separate dispatch mechanism.
        for m in &c.static_methods {
            let qualified = format!("{}.{}", c.name, m.name);
            let symbol = format!("__static_{}_{}", c.name, m.name);
            let (id, params, ret) = self.declare_fn_signature(symbol.as_str(), m, None)?;
            self.funcs.insert(qualified.into(), (id, params, ret));
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
            params.push(JitTy::from_ast(&p.ty, p.span, &self.class_ids, &self.enum_ids, &self.enum_layouts, &mut self.array_kinds, &mut self.optional_inners, &mut self.fn_signatures, &mut self.map_kinds, &mut self.tuple_kinds)?);
        }
        let ret = match &f.ret {
            Some(t) => JitTy::from_ast(t, f.span, &self.class_ids, &self.enum_ids, &self.enum_layouts, &mut self.array_kinds, &mut self.optional_inners, &mut self.fn_signatures, &mut self.map_kinds, &mut self.tuple_kinds)?,
            None => JitTy::Unit,
        };
        let mut sig = self.module.make_signature();
        if let Some(t) = this_ty {
            sig.params.push(AbiParam::new(t.cl().expect("object pointer")));
        }
        // `@extern("c", by_value)`: each `@extern(C) struct` struct param expands
        // into 1–2 i64 chunks per the integer-only ≤ 16 B composite
        // rule (AArch64 AAPCS64 / x86_64 SysV).
        let is_by_value = self.native_extern_by_value.contains(&f.name);
        // sret: when the by_value return is too big for register
        // packing, the C ABI passes a pointer to caller-allocated
        // storage in a hidden first parameter. Insert that param
        // *before* the user-visible ones so the calling-conv slot
        // assignment lines up (X8 on AArch64, RDI on SysV).
        let is_sret_return = is_by_value
            && matches!(ret, JitTy::Object(_))
            && {
                let class_id = match ret {
                    JitTy::Object(id) => id,
                    _ => unreachable!(),
                };
                let layout = &self.class_layouts[class_id as usize];
                matches!(repr_c_by_value_kind(layout), ByValueKind::Indirect)
            };
        if is_sret_return {
            sig.params.push(AbiParam::special(
                I64,
                cranelift_codegen::ir::ArgumentPurpose::StructReturn,
            ));
        }
        for p in &params {
            if is_by_value {
                if let JitTy::Object(class_id) = *p {
                    let layout = &self.class_layouts[class_id as usize];
                    match repr_c_by_value_kind(layout) {
                        ByValueKind::Chunks(n) => {
                            for _ in 0..n {
                                sig.params.push(AbiParam::new(I64));
                            }
                        }
                        ByValueKind::Hfa { elem, count } => {
                            // HFA: each element flows in its own FP
                            // register (V0..V3 / XMM0..XMM3).
                            // Cranelift's calling-conv assignment does
                            // the slot mapping when AbiParam types
                            // are F32/F64.
                            let cl_ty = elem.cl().expect("HFA elem has cl type");
                            for _ in 0..count {
                                sig.params.push(AbiParam::new(cl_ty));
                            }
                        }
                        ByValueKind::Indirect => {
                            // Cranelift's StructArgument purpose is
                            // only implemented on x86_64 — AArch64 /
                            // riscv64 / s390x panic on it. So:
                            //   - x86_64: StructArgument(size) (Cranelift copies onto stack per SysV)
                            //   - aarch64: regular i64 pointer (AAPCS64 passes >16 B
                            //     aggregates as a caller-allocated copy via pointer);
                            //     the call lowering allocates a stack slot and memcpys.
                            //   - other: error
                            #[cfg(target_arch = "x86_64")]
                            {
                                sig.params.push(AbiParam::special(
                                    I64,
                                    cranelift_codegen::ir::ArgumentPurpose::StructArgument(layout.size),
                                ));
                            }
                            #[cfg(target_arch = "aarch64")]
                            {
                                sig.params.push(AbiParam::new(I64));
                            }
                            #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
                            {
                                return Err(CodegenError::Unsupported {
                                    what: format!(
                                        "@extern fn {}: by_value param of struct {} > 16 B \
                                         is not supported on this target",
                                        f.name, layout.name
                                    ),
                                    span: f.span,
                                });
                            }
                        }
                    }
                    continue;
                }
            }
            sig.params.push(AbiParam::new(p.cl().ok_or_else(|| {
                CodegenError::Unsupported {
                    what: "unit-typed parameter".into(),
                    span: f.span,
                }
            })?));
        }
        if is_by_value {
            if let JitTy::Object(class_id) = ret {
                let layout = &self.class_layouts[class_id as usize];
                match repr_c_by_value_kind(layout) {
                    ByValueKind::Chunks(n) => {
                        for _ in 0..n {
                            sig.returns.push(AbiParam::new(I64));
                        }
                    }
                    ByValueKind::Hfa { elem, count } => {
                        let cl_ty = elem.cl().expect("HFA elem has cl type");
                        for _ in 0..count {
                            sig.returns.push(AbiParam::new(cl_ty));
                        }
                    }
                    ByValueKind::Indirect => {
                        // sret: the hidden pointer is already in
                        // params; the call has no register return.
                        // (Some ABIs additionally return the same
                        // pointer in a register; Cranelift handles
                        // that internally for the targets we care
                        // about.)
                    }
                }
            } else if let Some(t) = ret.cl() {
                sig.returns.push(AbiParam::new(t));
            }
        } else if let Some(t) = ret.cl() {
            sig.returns.push(AbiParam::new(t));
        }
        // `@extern` fns are linked as imports — the host registers
        // their actual addresses via `JITBuilder::symbol` (see
        // `register_extern_symbols` called during compiler creation).
        let linkage = if f.attrs.iter().any(|a| a.name == "extern") {
            Linkage::Import
        } else {
            Linkage::Local
        };
        let id = self
            .module
            .declare_function(symbol, linkage, &sig)
            .map_err(|e| CodegenError::Module(e.to_string()))?;
        Ok((id, params, ret))
    }

    fn define_fn(&mut self, f: &FnDecl) -> Result<(), CodegenError> {
        // Externs have no body to compile — the implementation comes
        // from the host symbol registered before module construction.
        if f.attrs.iter().any(|a| a.name == "extern") {
            return Ok(());
        }
        let (id, param_tys, ret_ty) = self.funcs[&f.name].clone();
        self.define_function_body(id, f, &param_tys, ret_ty, None)
    }

    fn define_methods(&mut self, c: &ClassDecl) -> Result<(), CodegenError> {
        let class_id = self.class_ids[&c.name];
        for m in &c.methods {
            let info = self.class_methods[class_id as usize][&m.name].clone();
            self.define_function_body(info.id, m, &info.params, info.ret, Some(class_id))?;
        }
        for prop in &c.properties {
            if let Some(g) = &prop.getter {
                let key = format!("__prop_get_{}", prop.name);
                let info = self.class_methods[class_id as usize][&key.as_str().into()].clone();
                self.define_function_body(info.id, g, &info.params, info.ret, Some(class_id))?;
            }
            if let Some(s) = &prop.setter {
                let key = format!("__prop_set_{}", prop.name);
                let info = self.class_methods[class_id as usize][&key.as_str().into()].clone();
                self.define_function_body(info.id, s, &info.params, info.ret, Some(class_id))?;
            }
        }
        for m in &c.static_methods {
            let qualified = format!("{}.{}", c.name, m.name);
            let (id, params, ret) = self.funcs[&qualified.as_str().into()].clone();
            // `this_class = None` — static methods don't get a `this`.
            self.define_function_body(id, m, &params, ret, None)?;
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
                let var = builder.declare_var(I64);
                let v = builder.block_params(entry)[block_param_idx];
                builder.def_var(var, v);
                block_param_idx += 1;
                Some((var, class_id))
            }
            None => None,
        };

        for (i, p) in f.params.iter().enumerate() {
            let pty = param_tys[i];
            let var = builder.declare_var(pty.cl().expect("non-unit checked at declare"));
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
                type_ref: self.print_type_ref,
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
                replace: self.str_replace_id,
                slice: self.str_slice_id,
                split: self.str_split_id,
                to_c_str: self.str_to_c_str_id,
                free_c_str: self.free_c_str_id,
                c_str_to_string: self.c_str_to_string_id,
                cstr_array_to_strings: self.cstr_array_to_strings_id,
                libc_free: self.libc_free_id,
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
            map_new_id: self.map_new_id,
            retain_map_id: self.retain_map_id,
            release_map_id: self.release_map_id,
            map_set_id: self.map_set_id,
            map_has_id: self.map_has_id,
            map_size_id: self.map_size_id,
            map_delete_id: self.map_delete_id,
            map_index_get_id: self.map_index_get_id,
            map_get_or_null_id: self.map_get_or_null_id,
            map_keys_to_array_id: self.map_keys_to_array_id,
            map_values_to_array_id: self.map_values_to_array_id,
            optional_box_new_id: self.optional_box_new_id,
            optional_box_retain_id: self.optional_box_retain_id,
            optional_box_release_id: self.optional_box_release_id,
            panic_index_oob_id: self.panic_index_oob_id,
            panic_div_zero_id: self.panic_div_zero_id,
            panic_unwrap_none_id: self.panic_unwrap_none_id,
            map_value_retains: &mut self.map_value_retains,
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
            map_kinds: &mut self.map_kinds,
            tuple_kinds: &mut self.tuple_kinds,
            class_type_meta_addrs: &self.class_type_meta_addrs,
            enum_type_meta_addrs: &self.enum_type_meta_addrs,
            prim_type_meta_addrs: &self.prim_type_meta_addrs,
            typekind_enum_id: *self.enum_ids.get(&Symbol::intern("TypeKind")).expect("TypeKind enum registered"),
            type_is_subtype: self.type_is_subtype,
            tuple_drops: &mut self.tuple_drops,
            map_drops: &mut self.map_drops,
            class_drops: &self.class_drops,
            array_drops: &mut self.array_drops,
            enum_drops: &mut self.enum_drops,
            loop_break_types: &self.loop_break_types,
            native_extern_fns: &self.native_extern_fns,
            extern_fn_names: &self.extern_fn_names,
            native_extern_variadic: &self.native_extern_variadic,
            native_extern_by_value: &self.native_extern_by_value,
            fn_call_type_args: &self.fn_call_type_args,
            extern_static_addrs: &self.extern_static_addrs,
            extern_static_types: &self.extern_static_types,
            static_field_slots: &self.static_field_slots,
            static_field_types: &self.static_field_types,
            static_field_base_addr: self.static_field_storage.as_ptr() as i64,
            class_vtable_addrs: &self.class_vtable_addrs,
            class_method_slots: &self.class_method_slots,
            class_parents: &self.class_parents,
            alloc_closure_id: self.alloc_closure_id,
            retain_closure_id: self.retain_closure_id,
            release_closure_id: self.release_closure_id,
            closure_meta: &self.closure_meta,
            closure_trampolines: &mut self.closure_trampolines,
            closure_drops: &mut self.closure_drops,
            closure_capture_env: None,
            current_class: this_class.map(|cid| {
                self.class_layouts[cid as usize].name.as_str().to_string()
            }),
        };
        // If this body is a closure wrapper, set up the
        // capture-env so Var lookups for captured names emit env
        // loads. The wrapper's first param `__env` is already
        // bound by the loop above.
        let captures_with_offsets: Vec<(Symbol, u32, JitTy)> =
            if let Some(meta) = self.closure_meta.get(&f.name) {
                meta.captures
                    .iter()
                    .enumerate()
                    .map(|(i, (n, jty))| (n.clone(), 8 + (i as u32) * 8, *jty))
                    .collect()
            } else {
                Vec::new()
            };
        if !captures_with_offsets.is_empty() {
            let env_var = lc.env.bindings.get(&"__env".into()).map(|&(v, _)| v);
            if let Some(v) = env_var {
                lc.closure_capture_env = Some(crate::env::ClosureEnv {
                    env_var: v,
                    captures: &captures_with_offsets,
                });
            }
        }
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
        // Tell the type checker about closure wrappers' captures
        // so their bodies type-check (free vars in the body
        // resolve to captured names).
        for (name, ast_caps) in &self.closure_ast_captures {
            tc.closure_wrapper_captures
                .insert(name.clone(), ast_caps.clone());
        }
        let prog_ty = tc.check(prog).map_err(|e| CodegenError::Cranelift(e.to_string()))?;
        let ret_ty = JitTy::from_ast(&prog_ty, ilang_ast::Span::dummy(), &self.class_ids, &self.enum_ids, &self.enum_layouts, &mut self.array_kinds, &mut self.optional_inners, &mut self.fn_signatures, &mut self.map_kinds, &mut self.tuple_kinds)?;

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
                type_ref: self.print_type_ref,
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
                replace: self.str_replace_id,
                slice: self.str_slice_id,
                split: self.str_split_id,
                to_c_str: self.str_to_c_str_id,
                free_c_str: self.free_c_str_id,
                cstr_array_to_strings: self.cstr_array_to_strings_id,
                c_str_to_string: self.c_str_to_string_id,
                libc_free: self.libc_free_id,
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
            map_new_id: self.map_new_id,
            retain_map_id: self.retain_map_id,
            release_map_id: self.release_map_id,
            map_set_id: self.map_set_id,
            map_has_id: self.map_has_id,
            map_size_id: self.map_size_id,
            map_delete_id: self.map_delete_id,
            map_index_get_id: self.map_index_get_id,
            map_get_or_null_id: self.map_get_or_null_id,
            map_keys_to_array_id: self.map_keys_to_array_id,
            map_values_to_array_id: self.map_values_to_array_id,
            optional_box_new_id: self.optional_box_new_id,
            optional_box_retain_id: self.optional_box_retain_id,
            optional_box_release_id: self.optional_box_release_id,
            panic_index_oob_id: self.panic_index_oob_id,
            panic_div_zero_id: self.panic_div_zero_id,
            panic_unwrap_none_id: self.panic_unwrap_none_id,
            map_value_retains: &mut self.map_value_retains,
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
            map_kinds: &mut self.map_kinds,
            tuple_kinds: &mut self.tuple_kinds,
            class_type_meta_addrs: &self.class_type_meta_addrs,
            enum_type_meta_addrs: &self.enum_type_meta_addrs,
            prim_type_meta_addrs: &self.prim_type_meta_addrs,
            typekind_enum_id: *self.enum_ids.get(&Symbol::intern("TypeKind")).expect("TypeKind enum registered"),
            type_is_subtype: self.type_is_subtype,
            tuple_drops: &mut self.tuple_drops,
            map_drops: &mut self.map_drops,
            class_drops: &self.class_drops,
            array_drops: &mut self.array_drops,
            enum_drops: &mut self.enum_drops,
            loop_break_types: &self.loop_break_types,
            native_extern_fns: &self.native_extern_fns,
            extern_fn_names: &self.extern_fn_names,
            extern_static_addrs: &self.extern_static_addrs,
            extern_static_types: &self.extern_static_types,
            native_extern_variadic: &self.native_extern_variadic,
            fn_call_type_args: &self.fn_call_type_args,
            native_extern_by_value: &self.native_extern_by_value,
            static_field_slots: &self.static_field_slots,
            static_field_types: &self.static_field_types,
            static_field_base_addr: self.static_field_storage.as_ptr() as i64,
            class_vtable_addrs: &self.class_vtable_addrs,
            class_method_slots: &self.class_method_slots,
            class_parents: &self.class_parents,
            alloc_closure_id: self.alloc_closure_id,
            retain_closure_id: self.retain_closure_id,
            release_closure_id: self.release_closure_id,
            closure_meta: &self.closure_meta,
            closure_trampolines: &mut self.closure_trampolines,
            closure_drops: &mut self.closure_drops,
            closure_capture_env: None,
            current_class: None,
        };
        // __main prologue: allocate an empty array for every
        // array-typed static field so a read before the user's
        // first explicit assignment still hands back a real (rc=1,
        // length=0) array instead of a null pointer. Plain
        // primitive statics already had their bit pattern written
        // into the storage box at JitCompiler construction time.
        for ((cls, fld), slot) in lc.static_field_slots.iter() {
            if let Some(ilang_ast::Type::Array { elem, fixed: None }) =
                lc.static_field_types.get(&(cls.clone(), fld.clone()))
            {
                let elem_size: i64 = match elem.as_ref() {
                    ilang_ast::Type::I8
                    | ilang_ast::Type::U8
                    | ilang_ast::Type::Bool => 1,
                    ilang_ast::Type::I16 | ilang_ast::Type::U16 => 2,
                    ilang_ast::Type::I32
                    | ilang_ast::Type::U32
                    | ilang_ast::Type::F32 => 4,
                    ilang_ast::Type::I64
                    | ilang_ast::Type::U64
                    | ilang_ast::Type::F64 => 8,
                    _ => continue,   // checker rejects other shapes
                };
                let r = lc.module.declare_func_in_func(lc.arrfns.new, builder.func);
                let elem_size_v = builder.ins().iconst(I64, elem_size);
                let len_v = builder.ins().iconst(I64, 0);
                let drop_fn_v = builder.ins().iconst(I64, 0);
                let call = builder.ins().call(r, &[elem_size_v, len_v, drop_fn_v]);
                let arr_ptr = builder.inst_results(call)[0];
                let addr = lc.static_field_base_addr + (*slot as i64) * 8;
                let addr_v = builder.ins().iconst(I64, addr);
                builder.ins().store(MemFlags::trusted(), arr_ptr, addr_v, 0);
            }
        }
        // Snapshot empty env so we know which top-level lets to release
        // at __main exit. Mirrors what lower_block_value does for blocks.
        let before: std::collections::HashSet<Symbol> =
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
            .filter(|(k, _)| !before.contains(k))
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
        // Vtable storage was zero-allocated; now that function
        // addresses are resolved, write each method's host pointer
        // into the appropriate slot.
        self.populate_vtables();
        Ok(())
    }

    /// Allocate one zero-initialised `Box<[i64]>` per class, sized
    /// according to the typechecker's `class_vtable_lens` table plus
    /// one leading slot reserved for the class's `TypeMeta*`. The
    /// reported `class_vtable_addrs[i]` points at slot 1 (the first
    /// method slot), so virtual-call sites address it at
    /// `vtable_ptr + slot*8` unchanged. RTTI reads `vtable_ptr - 8`
    /// to fetch the TypeMeta pointer.
    fn allocate_vtables(&mut self) {
        let n = self.class_layouts.len();
        self.class_vtable_storage = Vec::with_capacity(n);
        self.class_vtable_addrs = Vec::with_capacity(n);
        for (i, layout) in self.class_layouts.iter().enumerate() {
            let method_len = self
                .class_vtable_lens
                .get(&layout.name)
                .copied()
                .unwrap_or(0);
            // +1 leading slot for `TypeMeta*` (always present so
            // typeof works even on classes with no methods).
            let mut buf: Box<[i64]> = vec![0i64; method_len + 1].into_boxed_slice();
            // Eagerly write the TypeMeta pointer into slot 0 so
            // `is` / `as?` lowering can read it at compile time
            // (method pointers are filled in later by
            // `populate_vtables`, after `finalize_definitions`).
            buf[0] = self.class_type_meta_addrs[i];
            // Reported address points at slot 1 — i.e. past the
            // TypeMeta header — so existing slot arithmetic stays
            // valid and `vt_ptr - 8` reads the TypeMeta pointer.
            let addr = buf.as_ptr() as i64 + 8;
            self.class_vtable_storage.push(buf);
            self.class_vtable_addrs.push(addr);
        }
    }

    /// After finalize: for each class, write each (method, slot)
    /// entry's resolved function pointer into the vtable slot. The
    /// per-class `class_methods` table already contains inherited
    /// method entries (from the parent's table) for methods this
    /// class doesn't override, so the lookup is a single map hit.
    /// (The leading TypeMeta slot 0 was already filled in by
    /// `allocate_vtables`.)
    fn populate_vtables(&mut self) {
        for (class_idx, layout) in self.class_layouts.iter().enumerate() {
            let slots = match self.class_method_slots.get(&layout.name) {
                Some(s) => s.clone(),
                None => continue,
            };
            for (method_name, slot) in slots {
                let info = match self.class_methods[class_idx].get(&method_name) {
                    Some(i) => i.clone(),
                    None => continue,
                };
                let ptr = self.module.get_finalized_function(info.id) as i64;
                // +1 to skip the leading TypeMeta slot.
                self.class_vtable_storage[class_idx][slot + 1] = ptr;
            }
        }
    }

    /// Build the `TypeMeta` table covering every type the program
    /// might query at runtime: each class, each (monomorphised)
    /// enum, and a fixed roster of structural / primitive kinds.
    /// Pre-allocating into a `Vec<TypeMeta>` gives stable addresses
    /// — `Vec` reallocations would invalidate them, so we reserve
    /// the final capacity up front.
    fn build_type_metas(&mut self) {
        use crate::runtime::{alloc_str_saturated, TypeMeta};
        // TypeKind variant ordinals (declaration order in checker.rs):
        // primitive=0, class=1, enum=2, optional=3, array=4, fn=5,
        // tuple=6, string=7, unit=8.
        const K_PRIMITIVE: i32 = 0;
        const K_CLASS: i32 = 1;
        const K_ENUM: i32 = 2;
        const K_OPTIONAL: i32 = 3;
        const K_ARRAY: i32 = 4;
        const K_FN: i32 = 5;
        const K_TUPLE: i32 = 6;
        const K_STRING: i32 = 7;
        const K_UNIT: i32 = 8;
        // Pre-size so push() never reallocates and the addresses we
        // hand out below stay valid. We push `n_class + n_enum +
        // n_prim` entries total.
        let primitives: &[(&'static str, i32)] = &[
            ("i8", K_PRIMITIVE),
            ("i16", K_PRIMITIVE),
            ("i32", K_PRIMITIVE),
            ("i64", K_PRIMITIVE),
            ("u8", K_PRIMITIVE),
            ("u16", K_PRIMITIVE),
            ("u32", K_PRIMITIVE),
            ("u64", K_PRIMITIVE),
            ("f32", K_PRIMITIVE),
            ("f64", K_PRIMITIVE),
            ("bool", K_PRIMITIVE),
            ("string", K_STRING),
            ("()", K_UNIT),
            ("optional", K_OPTIONAL),
            ("array", K_ARRAY),
            ("fn", K_FN),
            ("tuple", K_TUPLE),
            ("weak", K_CLASS),
            ("Map", K_CLASS),
            ("Type", K_CLASS),
        ];
        let total = self.class_layouts.len() + self.enum_layouts.len() + primitives.len();
        self.type_metas = Vec::with_capacity(total);
        // Per-class entries — preserve class_id ordering so the
        // address vec lines up with `class_layouts`. Parent links
        // are filled in a second pass below (need every class's
        // TypeMeta address before we can wire them up).
        self.class_type_meta_addrs = Vec::with_capacity(self.class_layouts.len());
        for layout in &self.class_layouts {
            let name_ptr = alloc_str_saturated(layout.name.as_str().to_string());
            self.type_metas.push(TypeMeta {
                name: name_ptr,
                kind: K_CLASS,
                _pad: 0,
                parent: 0,
            });
            let idx = self.type_metas.len() - 1;
            self.class_type_meta_addrs
                .push(&self.type_metas[idx] as *const TypeMeta as i64);
        }
        // Resolve `extends Parent` chains now that every class has
        // a stable TypeMeta address.
        for (idx, layout) in self.class_layouts.iter().enumerate() {
            if let Some(parent_name) = layout.parent {
                if let Some(&pid) = self.class_ids.get(&parent_name) {
                    let parent_addr = self.class_type_meta_addrs[pid as usize];
                    self.type_metas[idx].parent = parent_addr;
                }
            }
        }
        // Per-enum entries. After monomorphisation `layout.name` is
        // `Result<i64, string>`; strip the `<...>` so the v1 RTTI
        // surface stays consistent with the interpreter (which has
        // no monomorphised name available). Phase 4's `typeArgs()`
        // will surface the args separately.
        self.enum_type_meta_addrs = Vec::with_capacity(self.enum_layouts.len());
        for layout in &self.enum_layouts {
            let raw = layout.name.as_str();
            let base = raw.split_once('<').map(|(b, _)| b).unwrap_or(raw);
            let name_ptr = alloc_str_saturated(base.to_string());
            self.type_metas.push(TypeMeta {
                name: name_ptr,
                kind: K_ENUM,
                _pad: 0,
                parent: 0,
            });
            let idx = self.type_metas.len() - 1;
            self.enum_type_meta_addrs
                .push(&self.type_metas[idx] as *const TypeMeta as i64);
        }
        // Structural / primitive entries — looked up by static name.
        for (name, kind) in primitives {
            let name_ptr = alloc_str_saturated((*name).to_string());
            self.type_metas.push(TypeMeta {
                name: name_ptr,
                kind: *kind,
                _pad: 0,
                parent: 0,
            });
            let idx = self.type_metas.len() - 1;
            let addr = &self.type_metas[idx] as *const TypeMeta as i64;
            self.prim_type_meta_addrs.insert(*name, addr);
        }
    }

    fn run_main(&mut self, ret: JitTy) -> JitValue {
        let (id, _, _) = self.funcs[&"__main".into()];
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
                        class: self.class_layouts[id as usize].name.as_str().to_string(),
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
                        class: self.class_layouts[class_id as usize].name.as_str().to_string(),
                        alive,
                    }
                }
                JitTy::Enum(id) => {
                    let tag = (std::mem::transmute::<_, extern "C" fn() -> i32>(ptr))()
                        as usize;
                    let layout = &self.enum_layouts[id as usize];
                    JitValue::Enum {
                        ty: layout.name.as_str().to_string(),
                        variant: layout
                            .variants
                            .get(tag)
                            .map(|s| s.as_str().to_string())
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
                JitTy::Map(id) => {
                    let p = (std::mem::transmute::<_, extern "C" fn() -> i64>(ptr))();
                    let kind = self.map_kinds[id as usize];
                    let size = if p == 0 {
                        0
                    } else {
                        crate::runtime::ilang_jit_map_size(p)
                    };
                    JitValue::Map {
                        key_ty: format!("{:?}", kind.key),
                        val_ty: format!("{:?}", kind.val),
                        size,
                    }
                }
                JitTy::Tuple(_) => {
                    let p = (std::mem::transmute::<_, extern "C" fn() -> i64>(ptr))();
                    JitValue::Tuple { ptr: p }
                }
                JitTy::EmbeddedArray(_) | JitTy::FlexArray(_) => unreachable!(
                    "embedded arrays only flow through chained access; the program's \
                     tail value would need to be a heap-managed type, not an inline slot"
                ),
                JitTy::TypeRef => {
                    let meta_ptr = (std::mem::transmute::<_, extern "C" fn() -> i64>(ptr))();
                    let name_ptr = *(meta_ptr as *const i64);
                    let s = (*(name_ptr as *const StringRc)).s.clone();
                    JitValue::TypeRef(s)
                }
                JitTy::Unit => {
                    (std::mem::transmute::<_, extern "C" fn()>(ptr))();
                    JitValue::Unit
                }
            }
        }
    }
}
