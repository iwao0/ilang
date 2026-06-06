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
    kind_tag_of, print_kind_id, print_kind_id_for_print_kind,
    print_kind_of, PrintKind, KIND_NONE,
};
use super::{
    alloc_global_class_id, alloc_global_enum_id, lower_program_into, BuiltinDecl, Compiled,
    CompileError, LoweringOutputs, OBJECT_HEADER_BYTES,
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
    let global_eid = |local: u32| enum_global[local as usize];

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
    // Populate Object field table — host_release_object_fields uses
    // it to cascade releases through heap-shaped fields.
    {
        // Populate the runtime's per-class cascade table so
        // `__release_object_fields` walks heap-shaped fields at
        // rc = 0. Both backends populate the same `ilang-runtime`
        // registry (AOT mirrors these calls from `__ilang_aot_init`).
        for class in &prog.classes {
            for (i, f) in class.fields.iter().enumerate() {
                let cascade_tag = kind_tag_of(&f.ty, &prog.classes);
                if cascade_tag != KIND_NONE {
                    let off = OBJECT_HEADER_BYTES as i64 + (i as i64) * 8;
                    ilang_runtime::__register_object_field(
                        global_cid(class.id.0) as i64,
                        off,
                        cascade_tag,
                    );
                }
            }
            // Mirror NewObject codegen: regular classes alloc
            // header + n_fields*8 bytes. Skip CRepr/packed/union
            // (different free path). Weakable classes also register
            // here — `__release_object` defers the free when any weak
            // refs are pending, and `__release_weak`'s final decrement
            // performs the free once the last weak observer is gone.
            let skip_free = matches!(
                class.repr,
                ilang_mir::ClassRepr::CRepr
                    | ilang_mir::ClassRepr::CPacked
                    | ilang_mir::ClassRepr::CUnion
            );
            if !skip_free {
                let size = OBJECT_HEADER_BYTES as i64 + (class.fields.len() as i64) * 8;
                ilang_runtime::__register_class_size(
                    global_cid(class.id.0) as i64,
                    size,
                );
            }
        }
    }
    // Populate the class-print-info registry in `ilang-runtime` —
    // `__print_object` walks an object's fields via the runtime's
    // copy. AOT mirrors the same registrations from
    // `__ilang_aot_init` using data-symbol-backed strings.
    let class_name_to_id: HashMap<ilang_ast::Symbol, ilang_mir::types::ClassId> =
        prog.classes.iter().map(|c| (c.name, c.id)).collect();
    for class in &prog.classes {
        let gcid = global_cid(class.id.0) as i64;
        let name_ptr = ilang_runtime::leak_cstring(class.name.as_str().to_string());
        ilang_runtime::__register_class_print_name(gcid, name_ptr);
        let is_struct = matches!(
            class.repr,
            ilang_mir::ClassRepr::CRepr
                | ilang_mir::ClassRepr::CPacked
                | ilang_mir::ClassRepr::CUnion
        );
        for (i, f) in class.fields.iter().enumerate() {
            let pk = print_kind_id(&f.ty);
            let fname_ptr = ilang_runtime::leak_cstring(f.name.as_str().to_string());
            ilang_runtime::__register_class_print_field(gcid, i as i64, fname_ptr, pk);
            if is_struct {
                // CRepr / CPacked / CUnion: also populate the struct
                // print registry with each field's natural byte
                // offset so `__print_struct` can read with C layout.
                // Bit-field fields don't have a byte slot of their
                // own — skip them rather than report a fake offset.
                if f.bit_field.is_some() {
                    continue;
                }
                let off = class
                    .c_field_offsets
                    .get(i)
                    .copied()
                    .unwrap_or(0);
                // Inline nested struct: pass the nested global cid so
                // the formatter recurses on its inlined bytes rather
                // than misreading the cell as a heap pointer.
                let nested_cid: i64 = if let MirTy::Object(nc) = &f.ty {
                    let nested = &prog.classes[nc.0 as usize];
                    if matches!(
                        nested.repr,
                        ilang_mir::ClassRepr::CRepr
                            | ilang_mir::ClassRepr::CPacked
                            | ilang_mir::ClassRepr::CUnion
                    ) {
                        global_cid(nc.0) as i64
                    } else {
                        0
                    }
                } else {
                    0
                };
                let fname_ptr = ilang_runtime::leak_cstring(f.name.as_str().to_string());
                ilang_runtime::__register_struct_print_field(
                    gcid, i as i64, fname_ptr, pk, off, nested_cid,
                );
            }
        }
        // Reflection meta — parent class id, method names, and the
        // per-field / per-method types so `typeof(x).fieldType("name")`
        // / `methodReturn(...)` / `methodParams(...)` can resolve.
        // Parent: 0 means "no parent" on the runtime side.
        let parent_id = class
            .parent
            .map(|p| global_cid(p.0) as i64)
            .unwrap_or(0);
        ilang_runtime::__register_type_parent(gcid, parent_id);
        for (i, m) in class.methods.iter().enumerate() {
            let mname_ptr = ilang_runtime::leak_cstring(m.name.as_str().to_string());
            ilang_runtime::__register_type_method(gcid, i as i64, mname_ptr);
            let func = &prog.functions[m.func.0 as usize];
            let ret_id = mir_ty_to_type_id(&func.ret, &global_cid);
            let mname_ptr_ret = ilang_runtime::leak_cstring(m.name.as_str().to_string());
            ilang_runtime::__register_type_method_return(gcid, mname_ptr_ret, ret_id);
            for (pi, p) in func.params.iter().enumerate() {
                // Skip the leading `this` parameter on non-static
                // methods so `methodParams` reports the user-visible
                // signature.
                if !m.is_static && pi == 0 {
                    continue;
                }
                let pid = mir_ty_to_type_id(&p.ty, &global_cid);
                let mname_ptr_p = ilang_runtime::leak_cstring(m.name.as_str().to_string());
                ilang_runtime::__register_type_method_param(
                    gcid, mname_ptr_p, pi as i64, pid,
                );
            }
        }
        // Skip parent fields — MIR's `class.fields` prepends every
        // inherited field for layout reasons, but reflection reports
        // only the names declared on this class itself.
        let parent_field_count = class
            .parent
            .map(|p| prog.classes[p.0 as usize].fields.len())
            .unwrap_or(0);
        for f in class.fields.iter().skip(parent_field_count) {
            let fty_id = mir_ty_to_type_id(&f.ty, &global_cid);
            let fname_ptr_t = ilang_runtime::leak_cstring(f.name.as_str().to_string());
            ilang_runtime::__register_type_field_type(gcid, fname_ptr_t, fty_id);
        }
        ilang_runtime::__register_type_declared_field_count(
            gcid,
            (class.fields.len() - parent_field_count) as i64,
        );
        // Generic instance type args — recovered from the monomorphised
        // class name, since the post-monomorph MIR ClassLayout doesn't
        // carry the original `<T, U>` substitutions explicitly.
        let arg_names = parse_class_name_type_args(class.name.as_str());
        for (i, arg) in arg_names.iter().enumerate() {
            let aid = type_arg_id_by_name(arg, &class_name_to_id, &global_cid);
            ilang_runtime::__register_type_arg(gcid, i as i64, aid);
        }
    }
    // Populate the enum print registry (`__register_enum_print_name`
    // / variant names / payload kinds) and the per-variant cascade
    // table in `ilang-runtime` so both backends see the same enum
    // metadata. AOT mirrors these registrations from
    // `__ilang_aot_init`.
    for e in &prog.enums {
        let global_id = global_eid(e.id.0);
        let name_ptr = ilang_runtime::leak_cstring(e.name.as_str().to_string());
        ilang_runtime::__register_enum_print_name(global_id as i64, name_ptr);
        // Tell the reflection runtime which global enum id maps to
        // the injected TypeKind so `__type_kind` can box discriminants
        // through `__enum_unit_get`.
        if e.name.as_str() == "TypeKind" {
            ilang_runtime::__register_typekind_enum_id(global_id as i64);
        }
        let is_str_repr = matches!(e.repr, MirTy::Str);
        for v in &e.variants {
            // Keep the payload's actual MIR types alongside the print
            // kinds: the release cascade is len-sensitive (fixed vs
            // dynamic arrays differ), but `PrintKind` collapses both to
            // `Array`, so the cascade tag is derived from the MIR type.
            let payload_tys: Vec<MirTy> = match &v.payload {
                ilang_mir::VariantPayload::Unit => Vec::new(),
                ilang_mir::VariantPayload::Tuple(tys) => tys.to_vec(),
                ilang_mir::VariantPayload::Struct(fs) => {
                    fs.iter().map(|(_, t)| t.clone()).collect()
                }
            };
            let kinds: Vec<PrintKind> =
                payload_tys.iter().map(print_kind_of).collect();
            let vname_ptr = ilang_runtime::leak_cstring(v.name.as_str().to_string());
            ilang_runtime::__register_enum_print_variant_name(
                global_id as i64,
                v.discriminant,
                vname_ptr,
            );
            for (i, k) in kinds.iter().enumerate() {
                // Cascade tag (KIND_*) for release cascade — derived
                // from the MIR type so fixed-length array payloads
                // (inline, no heap header) aren't freed as heap arrays.
                let cascade_tag = kind_tag_of(&payload_tys[i], &prog.classes);
                if cascade_tag != KIND_NONE {
                    ilang_runtime::__register_enum_payload_kind(
                        global_id as i64,
                        v.discriminant,
                        i as i64,
                        cascade_tag,
                    );
                }
                // Print tag (PK_*) for `__print_enum`.
                let pk = print_kind_id_for_print_kind(k);
                ilang_runtime::__register_enum_print_variant_payload_pk(
                    global_id as i64,
                    v.discriminant,
                    i as i64,
                    pk,
                );
            }
            if is_str_repr {
                if let Some(s) = v.discriminant_str.as_ref() {
                    // Mirror into the runtime registry that
                    // `__enum_disc_str` reads so AOT-built programs
                    // see the same mapping.
                    let sp = ilang_runtime::leak_cstring(s.clone());
                    ilang_runtime::__register_enum_disc_str(
                        global_id as i64,
                        v.discriminant,
                        sp,
                    );
                }
            }
        }
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
