//! Resolve `@extern("libname") fn foo(...): T` declarations by
//! dlopen-ing the named dynamic libraries and registering each
//! symbol with the JIT builder. The returned `Library` handles must
//! outlive the JIT module — `JitCompiler` keeps them in a field.
//!
//! Minimal scope: only `i64` / `f64` / `bool` parameter and return
//! types are accepted. Strings, objects, arrays, optional, etc. are
//! rejected at signature-validation time so we don't need any
//! marshalling logic at the boundary.

use cranelift_jit::JITBuilder;
use ilang_ast::{AttrArg, Item, Program, Type};
use libloading::Library;
use std::collections::HashSet;

use crate::error::CodegenError;

pub(crate) struct NativeExternRegistry {
    pub libs: Vec<Library>,
    /// Names of every fn that was registered as a native extern.
    /// The Call lowering reads this to decide whether to insert
    /// `string` ↔ C-string conversions around the call.
    pub names: HashSet<String>,
    /// Names of fns declared with trailing `...` — printf-style
    /// variadics. The declared param list is the fixed prefix; the
    /// call site can supply any number of extra args, each
    /// type-checked permissively and marshalled by its actual type.
    pub variadic: HashSet<String>,
    /// Names of fns whose struct parameters are passed by value
    /// (split into 1–2 i64 chunks per the AArch64 / SysV
    /// "integer-only ≤ 16 B" composite rule). Always set for fns
    /// synthesized from `@extern(C) {}` blocks.
    pub by_value: HashSet<String>,
    /// Resolved address for each `static name: T` declaration inside
    /// `@extern(C) {}`: the C global's runtime location, ready to
    /// be embedded as `iconst` at every read/write site. Library-form
    /// statics resolve via dlsym; host-form use the addresses
    /// pre-registered by host modules.
    pub static_addrs: std::collections::HashMap<String, i64>,
}

pub(crate) fn register_native_externs(
    builder: &mut JITBuilder,
    prog: &Program,
) -> Result<NativeExternRegistry, CodegenError> {
    use std::collections::HashMap;
    let mut libs: HashMap<String, Library> = HashMap::new();
    let mut names: HashSet<String> = HashSet::new();
    let mut variadic: HashSet<String> = HashSet::new();
    let mut by_value: HashSet<String> = HashSet::new();
    let mut static_addrs: HashMap<String, i64> = HashMap::new();
    // Host modules pre-register addresses for `@extern static`
    // declarations they own. Library-form statics are dlsym'd
    // below.
    crate::test_externs::register_test_static_addrs(&mut static_addrs);
    // Pre-collect names of opaque-handle classes — `@extern("lib")
    // class Foo {}`. These are valid as native-extern fn parameter
    // and return types (marshalled as raw i64 pointers).
    let opaque_classes: HashSet<String> = prog
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Class(c) if c.extern_lib.is_some() => Some(c.name.clone()),
            _ => None,
        })
        .collect();
    // Pre-collect names of `@repr(C) class Foo { ... }` — C-compat
    // structs that flow into native fns as `T *` (a pointer to the
    // user data area).
    let synth_classes = crate::compiler::synthesize_extern_c_classes(prog);
    let synth_fns = crate::compiler::synthesize_extern_c_fns(prog);
    let synth_statics = crate::compiler::synthesize_extern_c_statics(prog);
    let repr_c_classes: HashSet<String> = prog
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Class(c) if c.is_repr_c => Some(c.name.clone()),
            _ => None,
        })
        .chain(synth_classes.iter().filter_map(|c| {
            c.is_repr_c.then(|| c.name.clone())
        }))
        .collect();
    let mut all_extern_fns: Vec<&ilang_ast::FnDecl> = prog
        .items
        .iter()
        .filter_map(|i| if let Item::Fn(f) = i { Some(f) } else { None })
        .collect();
    all_extern_fns.extend(synth_fns.iter());
    // All `@extern(C) {}`-block-synthesized fns pass structs by value
    // (matches the C ABI, which has no other choice). Register them
    // here so the by_value validation path covers host-form fns too.
    for f in &synth_fns {
        by_value.insert(f.name.clone());
    }
    for f in &all_extern_fns {
        // Find an `@extern("libname")` attribute (string-arg form).
        // `@extern` with no args is the legacy host-side form, which
        // is registered separately by math_externs / test_externs and
        // doesn't need a library lookup.
        // Find an `@extern("libname", ...flags)` attribute and pull
        // out (lib_name, flag_set) in one pass.
        let extern_attr = f.attrs.iter().find(|a| {
            a.name == "extern" && a.args.iter().any(|x| matches!(x, AttrArg::Str(_)))
        });
        let Some(extern_attr) = extern_attr else { continue };
        // The first Str arg is the canonical library name (the
        // user's preferred / documented name). Additional Str args
        // are tried in order if the primary fails — covers dist /
        // version differences (`libssl.so.3` vs `libssl.so.1.1`).
        let mut lib_names: Vec<String> = Vec::new();
        let mut flag_optional = false;
        for arg in &extern_attr.args {
            match arg {
                AttrArg::Str(s) => lib_names.push(s.clone()),
                AttrArg::Path(parts) if parts.as_slice() == ["optional"] => {
                    flag_optional = true;
                }
                AttrArg::Path(parts) if parts.as_slice() == ["variadic"] => {
                    variadic.insert(f.name.clone());
                }
                AttrArg::Path(parts) if parts.as_slice() == ["byValue"] => {
                    by_value.insert(f.name.clone());
                }
                AttrArg::Path(parts) if parts.as_slice() == ["C"] => {
                    // Synthesized marker on `@extern(C)` block items —
                    // ignored at this layer; the synth pipeline already
                    // applied the relevant ABI rules.
                }
                AttrArg::Path(parts) => {
                    return Err(CodegenError::Unsupported {
                        what: format!(
                            "@extern: unknown flag `{}` (allowed: `optional`, `variadic`, `byValue`)",
                            parts.join(".")
                        ),
                        span: f.span,
                    });
                }
                AttrArg::Int(_) => {
                    return Err(CodegenError::Unsupported {
                        what: "@extern: integer arg not allowed".into(),
                        span: f.span,
                    });
                }
            }
        }
        let lib_name = lib_names.first().cloned().expect("filter above guarantees a Str arg");
        let fallback_names: &[String] = &lib_names[1..];
        validate_native_signature(f, &opaque_classes, &repr_c_classes)?;
        // Open (or reuse) the library. Try the primary name first,
        // then any fallback names in order. Bare names (no `.`)
        // get OS-specific candidate suffixes; literal names like
        // `libc.dylib` are tried as-is. Whichever name succeeds
        // becomes the cached entry under the *primary* key — the
        // user's program references the lib by its primary name in
        // `os.libLoaded(...)` regardless of which alternate took.
        if !libs.contains_key(&lib_name) {
            let mut last_err: Option<String> = None;
            let mut opened: Option<Library> = None;
            for cand in std::iter::once(&lib_name).chain(fallback_names.iter()) {
                match open_library(cand) {
                    Ok(lib) => {
                        opened = Some(lib);
                        break;
                    }
                    Err(e) => {
                        last_err = Some(format!("{cand}: {e}"));
                    }
                }
            }
            match opened {
                Some(lib) => {
                    libs.insert(lib_name.clone(), lib);
                    crate::runtime::record_lib_loaded(&lib_name, true, None);
                }
                None => {
                    let err_msg = last_err.unwrap_or_else(|| "no candidate".into());
                    if flag_optional {
                        crate::runtime::record_lib_loaded(
                            &lib_name,
                            false,
                            Some(err_msg),
                        );
                    } else {
                        return Err(CodegenError::Module(format!(
                            "@extern(\"{lib_name}\") fn {}: cannot dlopen library: {}",
                            f.name, err_msg
                        )));
                    }
                }
            }
        }
        if !libs.contains_key(&lib_name) {
            // Library was marked missing via the optional path; bind
            // this fn's symbol to the abort stub so any call site
            // surfaces a runtime error instead of an unresolved
            // symbol crash.
            builder.symbol(
                &f.name,
                crate::runtime::ilang_optional_extern_stub_abort as *const u8,
            );
            names.insert(f.name.clone());
            continue;
        }
        let lib = &libs[&lib_name];
        // Resolve the symbol. `@symbol("name")` overrides the C name
        // — without it, the ilang-side fn name is used (matches the
        // existing math externs convention).
        let c_symbol = f
            .attrs
            .iter()
            .find(|a| a.name == "symbol")
            .and_then(|a| a.args.first())
            .and_then(|arg| match arg {
                AttrArg::Str(s) => Some(s.clone()),
                _ => None,
            })
            .unwrap_or_else(|| f.name.clone());
        let sym_result: Result<libloading::Symbol<*const u8>, libloading::Error> =
            unsafe { lib.get(c_symbol.as_bytes()) };
        let ptr = match sym_result {
            Ok(sym) => unsafe { *sym.into_raw() },
            Err(e) => {
                if flag_optional {
                    // Lib loaded but symbol missing: same stub-abort
                    // treatment as a missing lib. The user has to
                    // probe before calling.
                    crate::runtime::ilang_optional_extern_stub_abort as *const u8
                } else {
                    return Err(CodegenError::Module(format!(
                        "@extern(\"{lib_name}\") fn {} (C symbol {c_symbol:?}): symbol not found: {e}",
                        f.name
                    )));
                }
            }
        };
        builder.symbol(&f.name, ptr);
        names.insert(f.name.clone());
    }
    // Host-form `@extern fn` (no library arg) doesn't enter the loop
    // above (Str-arg filter), so sweep them here for `byValue` so the
    // validation path picks them up.
    for f in &all_extern_fns {
        let Some(extern_attr) = f.attrs.iter().find(|a| a.name == "extern") else {
            continue;
        };
        for arg in &extern_attr.args {
            if let AttrArg::Path(parts) = arg {
                if parts.as_slice() == ["byValue"] {
                    by_value.insert(f.name.clone());
                } else if parts.as_slice() == ["variadic"] {
                    variadic.insert(f.name.clone());
                }
            }
        }
    }
    for f in &all_extern_fns {
        if !by_value.contains(&f.name) {
            continue;
        }
        validate_by_value_fn(f, prog, &synth_classes, &repr_c_classes, &opaque_classes)?;
    }
    // Resolve `@extern static` addresses. Library-form goes through
    // dlsym in whichever lib was named (opening it now if it wasn't
    // already opened by a fn declaration). Host-form must already be
    // present in `static_addrs` from a host registration call.
    let all_statics = prog
        .items
        .iter()
        .filter_map(|i| if let Item::ExternStatic(s) = i { Some(s) } else { None })
        .chain(synth_statics.iter());
    for s in all_statics {
        if let Some(lib_name) = &s.lib {
            if !libs.contains_key(lib_name) {
                let lib = open_library(lib_name).map_err(|e| {
                    CodegenError::Module(format!(
                        "@extern(\"{lib_name}\") static {}: {e}",
                        s.name
                    ))
                })?;
                libs.insert(lib_name.clone(), lib);
            }
            let lib = &libs[lib_name];
            let sym_result: Result<libloading::Symbol<*const u8>, libloading::Error> =
                unsafe { lib.get(s.name.as_bytes()) };
            let ptr = match sym_result {
                Ok(sym) => (unsafe { *sym.into_raw() }) as i64,
                Err(e) => {
                    return Err(CodegenError::Module(format!(
                        "@extern(\"{lib_name}\") static {}: symbol not found: {e}",
                        s.name
                    )));
                }
            };
            static_addrs.insert(s.name.clone(), ptr);
        } else if !static_addrs.contains_key(&s.name) {
            return Err(CodegenError::Module(format!(
                "@extern static {}: no host-side address registered for this name",
                s.name
            )));
        }
    }
    Ok(NativeExternRegistry {
        libs: libs.into_values().collect(),
        names,
        variadic,
        by_value,
        static_addrs,
    })
}

/// Field-level validation for a `byValue` extern fn. Rejects struct
/// params whose fields aren't in the GPR-only integer subset that
/// the call lowering knows how to pack into 1–2 i64 chunks.
fn validate_by_value_fn(
    f: &ilang_ast::FnDecl,
    prog: &Program,
    synth_classes: &[ilang_ast::ClassDecl],
    repr_c_classes: &HashSet<String>,
    opaque_classes: &HashSet<String>,
) -> Result<(), CodegenError> {
    let mut check = |ty: &Type, role: &str, span: ilang_ast::Span| -> Result<(), CodegenError> {
        let Type::Object(name) = ty else { return Ok(()); };
        if !repr_c_classes.contains(name) && !opaque_classes.contains(name) {
            return Err(CodegenError::Unsupported {
                what: format!(
                    "@extern fn {}: by_value {} of type {} is \
                     not a `@repr(C)` class",
                    f.name, role, name
                ),
                span,
            });
        }
        let class_decl = prog.items.iter().find_map(|i| match i {
            Item::Class(c) if &c.name == name => Some(c),
            _ => None,
        }).or_else(|| synth_classes.iter().find(|c| &c.name == name));
        let Some(class_decl) = class_decl else { return Ok(()); };
        // HFA: 1..=4 fields all of the same float type — passes via
        // FP registers per AArch64 AAPCS64 / SysV.
        let all_f32 = !class_decl.fields.is_empty()
            && class_decl.fields.iter().all(|fl| matches!(fl.ty, Type::F32));
        let all_f64 = !class_decl.fields.is_empty()
            && class_decl.fields.iter().all(|fl| matches!(fl.ty, Type::F64));
        let is_hfa = (all_f32 || all_f64) && class_decl.fields.len() <= 4;
        if is_hfa {
            return Ok(());
        }
        for fld in &class_decl.fields {
            let ok = matches!(
                fld.ty,
                Type::I8 | Type::I16 | Type::I32 | Type::I64
                | Type::U8 | Type::U16 | Type::U32 | Type::U64
                | Type::Bool
                // Raw C pointer / pointer-width int — i64-sized
                // integer at the ABI level. Treated identically to
                // i64 for chunk packing.
                | Type::RawPtr { .. }
                | Type::Size | Type::SSize
                // C `char` is i8.
                | Type::CChar
            );
            if !ok {
                return Err(CodegenError::Unsupported {
                    what: format!(
                        "@extern fn {}: by_value {} (struct {}) \
                         contains field {:?} of type {} — supported \
                         shapes are integer/bool fields or homogeneous \
                         float aggregates (1..=4 same-type f32 / f64 \
                         fields, HFA)",
                        f.name, role, name, fld.name, fld.ty
                    ),
                    span: fld.span,
                });
            }
        }
        Ok(())
    };
    for p in &f.params {
        check(&p.ty, &format!("param {:?}", p.name), p.span)?;
    }
    if let Some(ret_ty) = &f.ret {
        check(ret_ty, "return", f.span)?;
    }
    Ok(())
}

fn validate_native_signature(
    f: &ilang_ast::FnDecl,
    opaque_classes: &HashSet<String>,
    repr_c_classes: &HashSet<String>,
) -> Result<(), CodegenError> {
    for p in &f.params {
        if !is_native_abi_type(&p.ty, opaque_classes, repr_c_classes) {
            return Err(CodegenError::Unsupported {
                what: format!(
                    "@extern fn {}: parameter type {} not supported \
                     (allowed: any int width i8..i64 / u8..u64 / f32 / f64 / \
                     bool / string / @extern class)",
                    f.name, p.ty
                ),
                span: p.span,
            });
        }
    }
    if let Some(ret) = &f.ret {
        if !is_native_abi_type(ret, opaque_classes, repr_c_classes) && *ret != Type::Unit {
            return Err(CodegenError::Unsupported {
                what: format!(
                    "@extern fn {}: return type {} not supported \
                     (allowed: any int width i8..i64 / u8..u64 / f32 / f64 / \
                     bool / string / () / @extern class / T?)",
                    f.name, ret
                ),
                span: f.span,
            });
        }
    }
    Ok(())
}

/// Subset of types valid inside a callback `fn(...)` param/ret. Tighter
/// than `is_native_abi_type` because the C ABI for the inner call has
/// no place for ARC / closures — only fixed-width primitives and raw
/// C pointers ride the registers cleanly.
fn is_callback_arg_type(t: &Type) -> bool {
    matches!(
        t,
        Type::I8 | Type::I16 | Type::I32 | Type::I64
        | Type::U8 | Type::U16 | Type::U32 | Type::U64
        | Type::F32 | Type::F64
        | Type::Bool
    )
}

fn is_native_abi_type(
    t: &Type,
    opaque_classes: &HashSet<String>,
    repr_c_classes: &HashSet<String>,
) -> bool {
    match t {
        // Numeric primitives — every width that maps to a concrete
        // Cranelift type. Sub-int-width args/returns rely on the
        // calling convention to extend; AbiParam picks the right
        // sext/uext flag from the source type's signedness (see
        // `extern_signature_param` in compiler.rs).
        Type::I8 | Type::I16 | Type::I32 | Type::I64
        | Type::U8 | Type::U16 | Type::U32 | Type::U64
        | Type::F32 | Type::F64
        | Type::Bool | Type::Str => true,
        // C ABI types from @extern(C) blocks. All flow as machine
        // words (`i64` for pointers, `i8` for char, etc.) at the
        // calling-conv level — no marshalling at the boundary.
        Type::RawPtr { .. } => true,
        Type::CVoid | Type::CChar | Type::Size | Type::SSize => true,
        // C function pointer (`int (*)(int, int)` etc.) — a fn
        // type whose params and return are themselves native ABI
        // (primitive widths only; no nested fn / opaque / string
        // for now). Capture-free top-level fns can be passed via
        // `func_addr` at the call site; closure values aren't
        // supported yet (the C side has no env-ptr slot).
        Type::Fn { params, ret } => {
            params.iter().all(|p| is_callback_arg_type(p))
                && (matches!(ret.as_ref(), Type::Unit) || is_callback_arg_type(ret))
        }
        // Numeric arrays passed as a `void *` buffer pointer. The
        // C side reads or writes bytes within `len * sizeof(elem)`;
        // ilang keeps the ARC header and the buffer survives the
        // call. Both fixed and dynamic arrays share the same heap
        // layout, so both are allowed.
        Type::Array { elem, .. } => matches!(
            elem.as_ref(),
            Type::I8 | Type::I16 | Type::I32 | Type::I64
            | Type::U8 | Type::U16 | Type::U32 | Type::U64
            | Type::F32 | Type::F64,
        ),
        // Opaque-handle types: `@extern("lib") class Foo {}`. Stored
        // at runtime as a raw i64 C pointer.
        // C-compat structs: `@repr(C) class Foo { ... }`. Marshalled
        // as a `T *` to the user-data area (ARC header sits before
        // the pointer; C only sees the field bytes).
        Type::Object(name) => {
            opaque_classes.contains(name) || repr_c_classes.contains(name)
        }
        // `Foo?` where Foo is an opaque handle — same i64 storage
        // with `0` as the null/none sentinel.
        Type::Optional(inner) => match inner.as_ref() {
            Type::Object(name) => opaque_classes.contains(name),
            _ => false,
        },
        _ => false,
    }
}

/// Try to dlopen `lib_name`. If the name contains a `.` or `/`,
/// treat it as a literal filename (the user knows the exact form
/// they want — e.g. `libc.dylib`, `libm.so.6`, `./build/foo.so`).
/// Otherwise it's a bare module name like `m` / `c` / `sqlite3` and
/// we try the OS-specific candidates in order until one opens.
fn open_library(lib_name: &str) -> Result<Library, libloading::Error> {
    if lib_name.contains('.') || lib_name.contains('/') || lib_name.contains('\\') {
        return unsafe { Library::new(lib_name) };
    }
    let mut last_err: Option<libloading::Error> = None;
    for cand in candidates_for(lib_name) {
        match unsafe { Library::new(&cand) } {
            Ok(lib) => return Ok(lib),
            Err(e) => last_err = Some(e),
        }
    }
    Err(last_err.expect("candidates_for returns at least one entry"))
}

fn candidates_for(name: &str) -> Vec<String> {
    if cfg!(target_os = "macos") {
        // First try the bare names — dyld picks them up from system
        // paths or DYLD_*_LIBRARY_PATH. Then fall back to common
        // Homebrew install dirs (Apple Silicon = /opt/homebrew, Intel
        // = /usr/local) so user-installed libs like SDL2 work
        // out-of-the-box without needing to set env vars.
        vec![
            format!("lib{name}.dylib"),
            format!("{name}.dylib"),
            format!("/opt/homebrew/lib/lib{name}.dylib"),
            format!("/opt/homebrew/lib/{name}.dylib"),
            format!("/usr/local/lib/lib{name}.dylib"),
            format!("/usr/local/lib/{name}.dylib"),
        ]
    } else if cfg!(target_os = "windows") {
        vec![format!("{name}.dll"), format!("lib{name}.dll")]
    } else {
        // Linux / *BSD / others: try the unversioned `.so` first
        // (development symlink), then common SONAME suffixes.
        let mut out = vec![format!("lib{name}.so")];
        for n in [6, 5, 4, 3, 2, 1, 0] {
            out.push(format!("lib{name}.so.{n}"));
        }
        out
    }
}
