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
    /// Names of fns whose `string` return value is **owned by the
    /// callee** — i.e. the C side allocated it (e.g. `strdup`) and
    /// it must be freed with `libc::free` after we copy the bytes.
    /// Set by the `owned_return` flag in `@extern("libname", owned_return)`.
    pub owned_return: HashSet<String>,
    /// `caller fn name → free fn name` — overrides the default
    /// `libc::free` for the `owned_return` cleanup. Set by the
    /// `free_with.<name>` flag, used when the library has its own
    /// allocator (`sqlite3_free`, `xmlFree`, `OPENSSL_free`, etc.).
    pub owned_return_free_with: std::collections::HashMap<String, String>,
    /// Names of fns declared with the `variadic` flag — `printf`,
    /// `fprintf`, etc. The declared param list is the fixed prefix;
    /// the call site can supply any number of extra args, each
    /// type-checked permissively and marshalled by its actual type.
    pub variadic: HashSet<String>,
}

pub(crate) fn register_native_externs(
    builder: &mut JITBuilder,
    prog: &Program,
) -> Result<NativeExternRegistry, CodegenError> {
    use std::collections::HashMap;
    let mut libs: HashMap<String, Library> = HashMap::new();
    let mut names: HashSet<String> = HashSet::new();
    let mut owned_return: HashSet<String> = HashSet::new();
    let mut owned_return_free_with: HashMap<String, String> = HashMap::new();
    let mut variadic: HashSet<String> = HashSet::new();
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
    let repr_c_classes: HashSet<String> = prog
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Class(c) if c.is_repr_c => Some(c.name.clone()),
            _ => None,
        })
        .collect();
    for item in &prog.items {
        let Item::Fn(f) = item else { continue };
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
        let mut flag_owned_return = false;
        let mut flag_optional = false;
        let mut flag_variadic = false;
        let mut free_with: Option<String> = None;
        for arg in &extern_attr.args {
            match arg {
                AttrArg::Str(s) => lib_names.push(s.clone()),
                AttrArg::Path(parts) if parts.as_slice() == ["owned_return"] => {
                    flag_owned_return = true;
                }
                AttrArg::Path(parts) if parts.as_slice() == ["optional"] => {
                    flag_optional = true;
                }
                AttrArg::Path(parts) if parts.as_slice() == ["variadic"] => {
                    flag_variadic = true;
                }
                // `free_with.<fn_name>` — override the default
                // libc::free with a library-specific deallocator.
                // The fn name can be a module-qualified path
                // (`free_with.test.foo` → fn `test.foo`).
                AttrArg::Path(parts) if parts.len() >= 2 && parts[0] == "free_with" => {
                    free_with = Some(parts[1..].join("."));
                }
                AttrArg::Path(parts) => {
                    return Err(CodegenError::Unsupported {
                        what: format!(
                            "@extern: unknown flag `{}` (allowed: `owned_return`, `optional`, `variadic`, `free_with.<fn_name>`)",
                            parts.join(".")
                        ),
                        span: f.span,
                    });
                }
            }
        }
        let lib_name = lib_names.first().cloned().expect("filter above guarantees a Str arg");
        let fallback_names: &[String] = &lib_names[1..];
        if flag_variadic {
            variadic.insert(f.name.clone());
        }
        validate_native_signature(f, &opaque_classes, &repr_c_classes)?;
        if flag_owned_return {
            // owned_return only meaningful for string returns. Reject
            // it on other return types so the user notices the typo.
            let ret_is_str = matches!(f.ret, Some(Type::Str));
            if !ret_is_str {
                return Err(CodegenError::Unsupported {
                    what: format!(
                        "@extern fn {}: `owned_return` requires a `string` return type",
                        f.name
                    ),
                    span: f.span,
                });
            }
            owned_return.insert(f.name.clone());
        }
        if let Some(free_fn) = free_with {
            if !flag_owned_return {
                return Err(CodegenError::Unsupported {
                    what: format!(
                        "@extern fn {}: `free_with.{}` only makes sense alongside `owned_return`",
                        f.name, free_fn
                    ),
                    span: f.span,
                });
            }
            // Verify the named fn is declared as an `@extern` fn in
            // the same program with the right shape (one i64 / Str
            // / opaque-class param, no return).
            let target = prog.items.iter().find_map(|i| match i {
                Item::Fn(g)
                    if g.name == free_fn
                        && g.attrs.iter().any(|a| a.name == "extern") =>
                {
                    Some(g)
                }
                _ => None,
            });
            let target = target.ok_or_else(|| CodegenError::Unsupported {
                what: format!(
                    "@extern fn {}: `free_with.{}` references unknown extern fn",
                    f.name, free_fn
                ),
                span: f.span,
            })?;
            if target.params.len() != 1 {
                return Err(CodegenError::Unsupported {
                    what: format!(
                        "@extern fn {}: `free_with.{}` must take exactly one i64 / opaque-class parameter",
                        f.name, free_fn
                    ),
                    span: f.span,
                });
            }
            // Accept i64 or an opaque-extern class type. The C-side
            // ABI for both is just a raw pointer.
            let p_ty = &target.params[0].ty;
            let ok_param = matches!(p_ty, Type::I64)
                || matches!(p_ty, Type::Object(name) if opaque_classes.contains(name));
            if !ok_param {
                return Err(CodegenError::Unsupported {
                    what: format!(
                        "@extern fn {}: `free_with.{}` parameter must be i64 or an `@extern` class (got {})",
                        f.name, free_fn, p_ty
                    ),
                    span: f.span,
                });
            }
            owned_return_free_with.insert(f.name.clone(), free_fn);
        }
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
        // Resolve the symbol (the fn's source name is used as the
        // symbol name — same convention as the existing math externs).
        let symbol_bytes = f.name.as_bytes();
        let sym_result: Result<libloading::Symbol<*const u8>, libloading::Error> =
            unsafe { lib.get(symbol_bytes) };
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
                        "@extern(\"{lib_name}\") fn {}: symbol not found: {e}",
                        f.name
                    )));
                }
            }
        };
        builder.symbol(&f.name, ptr);
        names.insert(f.name.clone());
    }
    Ok(NativeExternRegistry {
        libs: libs.into_values().collect(),
        names,
        owned_return,
        owned_return_free_with,
        variadic,
    })
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
        vec![format!("lib{name}.dylib"), format!("{name}.dylib")]
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
