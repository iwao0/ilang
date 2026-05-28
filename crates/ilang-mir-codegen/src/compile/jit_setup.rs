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
    alloc_global_class_id, alloc_global_enum_id, lower_program_into, walk_mir_ty, BuiltinDecl,
    Compiled, CompileError, LoweringOutputs, OBJECT_HEADER_BYTES,
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
        // Scan the whole program for any MirTy::Weak(C) reference —
        // classes that appear as a weak target stay OUT of the size
        // table so release_object's free path skips them. Without
        // this, a `let w: Node.weak = strong; …; strong = …` flow
        // (see weak_basic.il) would have the weak peek into freed
        // memory once the original strong drops. The leak we accept
        // for those classes is bounded — programs that use weak
        // refs are usually small fixed graphs.
        let mut weakable: std::collections::HashSet<u32> =
            std::collections::HashSet::new();
        let scan = |ty: &MirTy, set: &mut std::collections::HashSet<u32>| {
            walk_mir_ty(ty, &mut |t| {
                if let MirTy::Weak(c) = t {
                    set.insert(c.0);
                }
            });
        };
        for class in &prog.classes {
            for f in &class.fields {
                scan(&f.ty, &mut weakable);
            }
        }
        for f in &prog.functions {
            for p in f.params.iter() {
                scan(&p.ty, &mut weakable);
            }
            scan(&f.ret, &mut weakable);
            for l in f.value_tys.iter() {
                scan(l, &mut weakable);
            }
            for l in f.local_tys.iter() {
                scan(l, &mut weakable);
            }
        }

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
            // (different free path) and any class referenced via
            // Weak (would dangle weak peeks).
            let skip_free = matches!(
                class.repr,
                ilang_mir::ClassRepr::CRepr
                    | ilang_mir::ClassRepr::CPacked
                    | ilang_mir::ClassRepr::CUnion
            ) || weakable.contains(&class.id.0);
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
