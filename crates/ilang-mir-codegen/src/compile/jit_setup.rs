//! JIT entry points: `compile_program` / `compile_with_builtins`.
//!
//! Build a `JITBuilder`, register the canonical `ilang_runtime::__*`
//! symbol table, drive `lower_program_into` to populate the module,
//! finalize, and mirror the program's class / enum / closure /
//! drop-fn metadata into the runtime registries so the cascade and
//! print paths can find it at run-time.

use std::collections::HashMap;

use cranelift_codegen::settings;
use cranelift_jit::{JITBuilder, JITModule};

use ilang_mir::{FuncId, MirTy, Program};

use super::host_misc::{host_optional_missing_stub, lookup_symbol_in_process, process_symbol_exists};
use super::host_os::try_open_lib;
use super::print_kind::{
    kind_tag_of, print_kind_id, print_kind_id_for_print_kind, print_kind_of, KIND_NONE,
};
use super::{
    alloc_global_class_id, alloc_global_enum_id, lower_program_into, BuiltinDecl, Compiled,
    CompileError, LoweringOutputs,
};

pub fn compile_program(prog: &Program) -> Result<Compiled, CompileError> {
    compile_with_builtins(prog, &[])
}

/// Like `compile_program`, but registers host-provided builtins. Call
/// sites that name `Inst::Call(FuncRef::Builtin(sym))` resolve to
/// these.
pub fn compile_with_builtins(
    prog: &Program,
    builtins: &[BuiltinDecl],
) -> Result<Compiled, CompileError> {
    let isa_builder = cranelift_native::builder()
        .map_err(|e| CompileError::Other(format!("cranelift_native: {e}")))?;
    let flag_builder = settings::builder();
    let isa = isa_builder
        .finish(settings::Flags::new(flag_builder))
        .map_err(|e| CompileError::Other(format!("isa: {e}")))?;

    // Allocate globally-unique class / enum ids for this compile so
    // its runtime tables don't collide with any other module's. The
    // GLOBAL id is what gets stored into heap-object headers, and
    // every host table lookup keys off it.
    let class_global: Vec<u32> = (0..prog.classes.len())
        .map(|_| alloc_global_class_id())
        .collect();
    let enum_global: Vec<u32> = (0..prog.enums.len())
        .map(|_| alloc_global_enum_id())
        .collect();
    let global_cid = |local: u32| class_global[local as usize];

    // Pre-build a per-(global_class_id, slot) → method-fn-id map from
    // the MIR. The actual function addresses are filled in after
    // `finalize_definitions()` and exposed to JIT code via the
    // `__virt_dispatch` host helper.
    let mut vtable_entries: HashMap<(u32, u32), FuncId> = HashMap::new();
    for class in &prog.classes {
        for m in &class.methods {
            if let Some(slot) = m.slot {
                vtable_entries.insert((global_cid(class.id.0), slot.0), m.func);
            }
        }
    }

    let mut jit_builder = JITBuilder::with_isa(isa, cranelift_module::default_libcall_names());
    // On Windows, Cranelift's built-in resolver only checks the main
    // executable and ucrtbase.dll. Register a custom lookup that
    // searches all loaded modules so DLLs opened via LoadLibraryA
    // (e.g. SDL2.dll) are visible to the JIT.
    jit_builder.symbol_lookup_fn(Box::new(|name| lookup_symbol_in_process(name)));
    super::jit_symbols::register_runtime_symbols(&mut jit_builder);
    for b in builtins {
        jit_builder.symbol(b.name, b.ptr);
    }
    // Eagerly dlopen every `@lib(...)` library declared anywhere
    // in the program. Without this, dlsym(RTLD_DEFAULT, "SDL_Init")
    // misses because the dynamic loader hasn't pulled libSDL2 into
    // the process yet. Handles are leaked via Box::leak so the
    // libs stay live for the JIT's lifetime.
    {
        let mut tried: std::collections::HashSet<String> = std::collections::HashSet::new();
        for f in &prog.functions {
            if !matches!(f.kind, ilang_mir::FunctionKind::Extern { .. }) {
                continue;
            }
            for lib in &f.libs {
                let name = lib.as_str().to_string();
                if !tried.insert(name.clone()) {
                    continue;
                }
                let _ = try_open_lib(&name);
            }
        }
    }
    // For every `@extern(C) @optional` fn whose target dlsym would
    // fail (the lib couldn't be opened, or the symbol is missing),
    // bind a host stub so finalize doesn't panic. The stub aborts
    // when actually invoked — callers are expected to gate via
    // `os.libLoaded(...)` first.
    // Collect `@lib(...)` groups so `os.libLoaded(name)` can fall
    // through to alternates declared on the same fn. Lives in
    // `ilang-runtime` now so both backends share the registry.
    for f in &prog.functions {
        if matches!(f.kind, ilang_mir::FunctionKind::Extern { .. })
            && f.libs.len() > 1
        {
            let g = ilang_runtime::__register_lib_group_begin();
            for sym in f.libs.iter() {
                let p = ilang_runtime::leak_cstring(sym.as_str().to_string());
                ilang_runtime::__register_lib_group_member(g, p);
            }
        }
    }
    for f in &prog.functions {
        if !matches!(f.kind, ilang_mir::FunctionKind::Extern { .. }) {
            continue;
        }
        if !f.is_optional {
            continue;
        }
        let sym_name = f
            .c_symbol
            .as_ref()
            .map(|s| s.as_str().to_string())
            .unwrap_or_else(|| f.name.as_str().to_string());
        // Only register the stub if no real symbol exists in the
        // current process (dlsym from RTLD_DEFAULT). Otherwise the
        // JIT would prefer the stub over the real implementation.
        if !process_symbol_exists(&sym_name) {
            jit_builder.symbol(sym_name, host_optional_missing_stub as *const u8);
        }
    }
    let mut module = JITModule::new(jit_builder);

    // Declare the alloc builtin so NewObject can call it.
    let LoweringOutputs {
        fn_ids,
        extern_fn_ids,
        missing_optional_fn_ids: _,
        extern_alias_fn_ids: _,
    } = lower_program_into(&mut module, prog, builtins, &class_global, &enum_global)?;
    module
        .finalize_definitions()
        .map_err(CompileError::Module)?;

    // Populate the runtime vtable now that fn addresses are stable.
    // Don't clear — entries are keyed by GLOBAL (class_id, slot) and
    // accumulate so parallel modules coexist without trampling.
    {
        for ((cid, slot), fid) in &vtable_entries {
            if let Some(cl_id) = fn_ids.get(fid) {
                let addr = module.get_finalized_function(*cl_id) as i64;
                ilang_runtime::__register_vtable_entry(*cid as i64, *slot as i64, addr);
            }
        }
    }
    // Register `@objc class : Parent` IMP function addresses with
    // the runtime so the parser-generated `register()` body can
    // resolve them via `__ilang_objc_imp_lookup` — JIT-emitted
    // functions aren't reachable through `dlsym(RTLD_DEFAULT)`.
    for (idx, f) in prog.functions.iter().enumerate() {
        let mid = FuncId(idx as u32);
        if extern_fn_ids.contains(&mid) {
            continue;
        }
        let symbol_name = f
            .c_symbol
            .as_ref()
            .map(|s| s.as_str())
            .unwrap_or_else(|| f.name.as_str());
        if !symbol_name.starts_with("$objc.imp.") {
            continue;
        }
        if let Some(cl_id) = fn_ids.get(&mid) {
            let addr = module.get_finalized_function(*cl_id) as usize;
            ilang_runtime::__register_objc_imp(symbol_name.to_string(), addr);
        }
    }
    // Class layout — print info (`$class.registerPrintName` /
    // `…PrintField` / `…StructPrintField`), heap-cascade table
    // (`$class.registerObjectField`), and total allocation size
    // (`$class.registerSize`). Shared walker in
    // `compile::registration`; the JIT sink dispatches each event
    // straight to the matching `ilang_runtime::__register_class_*`
    // extern. AOT emits the same events as IR calls from
    // `__ilang_aot_init`.
    {
        let mut sink = super::registration::ClassLayoutSink_JIT;
        let classes = &prog.classes;
        super::registration::emit_class_layout_registrations(
            prog,
            &class_global,
            print_kind_id,
            |ty| kind_tag_of(ty, classes),
            &mut sink,
        );
    }
    // Reflection-meta registrations — parent / methods / field
    // types / declared field count / generic instance args. Shared
    // walker; the JIT sink dispatches each event straight to the
    // `ilang_runtime::__register_type_*` host fn.
    {
        let mut sink = super::registration::ReflectionSink_JIT;
        super::registration::emit_reflection_registrations(
            prog,
            &class_global,
            &mut sink,
        );
    }
    // Enum print + cascade + disc-str + TypeKind-id registrations.
    // Shared walker; the JIT sink dispatches each event to the
    // matching `ilang_runtime::__register_enum_*` extern.
    {
        let mut sink = super::registration::EnumRegistrationSink_JIT;
        let classes = &prog.classes;
        super::registration::emit_enum_registrations(
            prog,
            &enum_global,
            |ty| print_kind_id_for_print_kind(&print_kind_of(ty)),
            |ty| kind_tag_of(ty, classes),
            &mut sink,
        );
    }
    // Populate closure capture / size tables in the runtime crate —
    // `ilang_runtime::__release_closure` walks the captures table at
    // rc-zero. AOT mirrors these registrations from
    // `__ilang_aot_init`; both backends share one map.
    for (idx, func) in prog.functions.iter().enumerate() {
        if extern_fn_ids.contains(&FuncId(idx as u32)) {
            continue;
        }
        let env = match &func.closure_env {
            Some(e) => e,
            None => continue,
        };
        let mid = FuncId(idx as u32);
        let cl_id = match fn_ids.get(&mid) {
            Some(c) => *c,
            None => continue,
        };
        let addr = module.get_finalized_function(cl_id) as i64;
        for (i, cap) in env.captures.iter().enumerate() {
            if cap.is_cell {
                // Cells are 1-element arrays — leak for now.
                continue;
            }
            let tag = kind_tag_of(&cap.ty, &prog.classes);
            if tag == KIND_NONE {
                continue;
            }
            let off = 16 + (i as i64) * 8;
            ilang_runtime::__register_closure_capture(addr, off, tag);
        }
        let total_size = (2 + env.captures.len() as i64) * 8;
        ilang_runtime::__register_closure_size(addr, total_size);
    }
    // Populate fn-name registry — `__print_fn` looks up the fn
    // address (closure[0]) and prints "<fn NAME>" / "<fn>". Lives
    // in `ilang-runtime` so AOT-built programs see the same table.
    // Skip extern fns (no compiled body) and synthetic names.
    for (idx, func) in prog.functions.iter().enumerate() {
        if extern_fn_ids.contains(&FuncId(idx as u32)) {
            continue;
        }
        let mid = FuncId(idx as u32);
        if let Some(cl_id) = fn_ids.get(&mid) {
            let addr = module.get_finalized_function(*cl_id) as i64;
            let name = func.name.as_str();
            if !name.starts_with("$anon.fn_") && !name.starts_with("$main") {
                let plain = name.split("__").next().unwrap_or(name);
                let name_ptr = ilang_runtime::leak_cstring(plain.to_string());
                ilang_runtime::__register_fn_name(addr, name_ptr);
            }
        }
    }
    // Populate the class-id → drop fn registry. `drop_fn` is set by
    // the lowering whenever a class declares `deinit`. Subclasses
    // that don't redefine deinit inherit the parent's via the
    // method table — read it back from the lowered class.
    for class in &prog.classes {
        if class.drop_fn.0 != u32::MAX {
            if let Some(cl_id) = fn_ids.get(&class.drop_fn) {
                let addr = module.get_finalized_function(*cl_id) as i64;
                ilang_runtime::__register_drop(global_cid(class.id.0) as i64, addr);
            }
        }
    }

    let entry_fn = &prog.functions[prog.entry.0 as usize];
    let entry_ret = entry_fn.ret.clone();
    let entry = *fn_ids.get(&prog.entry).expect("entry registered");

    Ok(Compiled { module, entry, entry_ret })
}

/// Split a mangled monomorph class name (e.g. `Box<i64>`,
/// `Pair<string, i64>`, `Box<Box<i64>>`) into its top-level type
/// arguments. Returns an empty vector for non-generic names.
pub(crate) fn parse_class_name_type_args(name: &str) -> Vec<&str> {
    let Some(start) = name.find('<') else { return Vec::new() };
    let end = match name.rfind('>') {
        Some(e) if e > start => e,
        _ => return Vec::new(),
    };
    let inner = &name[start + 1..end];
    let mut depth: i32 = 0;
    let mut args: Vec<&str> = Vec::new();
    let mut last = 0usize;
    for (i, c) in inner.char_indices() {
        match c {
            '<' => depth += 1,
            '>' => depth -= 1,
            ',' if depth == 0 => {
                args.push(inner[last..i].trim());
                last = i + c.len_utf8();
            }
            _ => {}
        }
    }
    args.push(inner[last..].trim());
    args
}

/// Resolve a single type-arg textual name to the reflection runtime's
/// class id. Recognises every primitive name plus a class registered
/// in `class_ids`; anything else falls back to 0 (unknown).
pub(crate) fn type_arg_id_by_name(
    name: &str,
    class_ids: &HashMap<ilang_ast::Symbol, ilang_mir::types::ClassId>,
    global_cid: &dyn Fn(u32) -> u32,
) -> i64 {
    use ilang_runtime::{
        TYPE_ID_BOOL, TYPE_ID_F32, TYPE_ID_F64, TYPE_ID_I16, TYPE_ID_I32,
        TYPE_ID_I64, TYPE_ID_I8, TYPE_ID_STRING, TYPE_ID_U16, TYPE_ID_U32,
        TYPE_ID_U64, TYPE_ID_U8, TYPE_ID_UNIT,
    };
    match name {
        "i8" => return TYPE_ID_I8,
        "i16" => return TYPE_ID_I16,
        "i32" => return TYPE_ID_I32,
        "i64" => return TYPE_ID_I64,
        "u8" => return TYPE_ID_U8,
        "u16" => return TYPE_ID_U16,
        "u32" => return TYPE_ID_U32,
        "u64" => return TYPE_ID_U64,
        "f32" => return TYPE_ID_F32,
        "f64" => return TYPE_ID_F64,
        "bool" => return TYPE_ID_BOOL,
        "string" => return TYPE_ID_STRING,
        "()" | "unit" => return TYPE_ID_UNIT,
        _ => {}
    }
    if let Some(cid) = class_ids.get(&ilang_ast::Symbol::intern(name)) {
        return global_cid(cid.0) as i64;
    }
    0
}

/// Map a `MirTy` to the `class_id` the reflection runtime uses. Real
/// classes report their JIT-global cid; primitives / structural types
/// report a negative virtual id whose name `__class_name` resolves.
pub(crate) fn mir_ty_to_type_id(
    ty: &MirTy,
    global_cid: &dyn Fn(u32) -> u32,
) -> i64 {
    use ilang_runtime::{
        TYPE_ID_ARRAY, TYPE_ID_BOOL, TYPE_ID_ENUM, TYPE_ID_F32, TYPE_ID_F64,
        TYPE_ID_FN, TYPE_ID_I16, TYPE_ID_I32, TYPE_ID_I64, TYPE_ID_I8,
        TYPE_ID_MAP, TYPE_ID_OPTIONAL, TYPE_ID_PROMISE, TYPE_ID_SET,
        TYPE_ID_STRING, TYPE_ID_TUPLE, TYPE_ID_U16, TYPE_ID_U32, TYPE_ID_U64,
        TYPE_ID_U8, TYPE_ID_UNIT, TYPE_ID_WEAK,
    };
    match ty {
        MirTy::Object(c) => global_cid(c.0) as i64,
        MirTy::Weak(_) => TYPE_ID_WEAK,
        MirTy::Enum(_) => TYPE_ID_ENUM,
        MirTy::Str => TYPE_ID_STRING,
        MirTy::Bool => TYPE_ID_BOOL,
        MirTy::I64 | MirTy::SSize => TYPE_ID_I64,
        MirTy::U64 | MirTy::Size => TYPE_ID_U64,
        MirTy::I32 => TYPE_ID_I32,
        MirTy::U32 => TYPE_ID_U32,
        MirTy::I16 => TYPE_ID_I16,
        MirTy::U16 => TYPE_ID_U16,
        MirTy::I8 | MirTy::CChar => TYPE_ID_I8,
        MirTy::U8 => TYPE_ID_U8,
        MirTy::F64 => TYPE_ID_F64,
        MirTy::F32 => TYPE_ID_F32,
        MirTy::Unit | MirTy::CVoid => TYPE_ID_UNIT,
        MirTy::Array { .. } => TYPE_ID_ARRAY,
        MirTy::Tuple(_) => TYPE_ID_TUPLE,
        MirTy::Optional(_) => TYPE_ID_OPTIONAL,
        MirTy::Map { .. } => TYPE_ID_MAP,
        MirTy::Set { .. } => TYPE_ID_SET,
        MirTy::Promise(_) => TYPE_ID_PROMISE,
        MirTy::Fn(_) | MirTy::RawFn(_) => TYPE_ID_FN,
        MirTy::RawPtr { .. } | MirTy::TypeVar(_) | MirTy::Simd { .. }
        | MirTy::TypeHandle => 0,
    }
}
