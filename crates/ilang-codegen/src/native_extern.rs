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
}

pub(crate) fn register_native_externs(
    builder: &mut JITBuilder,
    prog: &Program,
) -> Result<NativeExternRegistry, CodegenError> {
    use std::collections::HashMap;
    let mut libs: HashMap<String, Library> = HashMap::new();
    let mut names: HashSet<String> = HashSet::new();
    let mut owned_return: HashSet<String> = HashSet::new();
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
        let mut lib_name: Option<String> = None;
        let mut flag_owned_return = false;
        let mut flag_optional = false;
        for arg in &extern_attr.args {
            match arg {
                AttrArg::Str(s) => lib_name = Some(s.clone()),
                AttrArg::Path(parts) if parts.as_slice() == ["owned_return"] => {
                    flag_owned_return = true;
                }
                AttrArg::Path(parts) if parts.as_slice() == ["optional"] => {
                    flag_optional = true;
                }
                AttrArg::Path(parts) => {
                    return Err(CodegenError::Unsupported {
                        what: format!(
                            "@extern: unknown flag `{}` (allowed: `owned_return`, `optional`)",
                            parts.join(".")
                        ),
                        span: f.span,
                    });
                }
            }
        }
        let lib_name = lib_name.expect("filter above guarantees a Str arg");
        validate_native_signature(f, &opaque_classes)?;
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
        // Open (or reuse) the library. Bare names (no `.`) get OS-
        // specific candidates; literal names like `libc.dylib` are
        // tried as-is.
        if !libs.contains_key(&lib_name) {
            match open_library(&lib_name) {
                Ok(lib) => {
                    libs.insert(lib_name.clone(), lib);
                    crate::runtime::record_lib_loaded(&lib_name, true);
                }
                Err(e) => {
                    if flag_optional {
                        // Mark this lib as missing; subsequent fns
                        // mapped to it also turn into stubs. Calls
                        // to those fns at runtime abort with a clear
                        // message — guard with `os.libLoaded(...)`.
                        crate::runtime::record_lib_loaded(&lib_name, false);
                    } else {
                        return Err(CodegenError::Module(format!(
                            "@extern(\"{lib_name}\") fn {}: cannot dlopen library: {e}",
                            f.name
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
    })
}

fn validate_native_signature(
    f: &ilang_ast::FnDecl,
    opaque_classes: &HashSet<String>,
) -> Result<(), CodegenError> {
    for p in &f.params {
        if !is_native_abi_type(&p.ty, opaque_classes) {
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
        if !is_native_abi_type(ret, opaque_classes) && *ret != Type::Unit {
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

fn is_native_abi_type(t: &Type, opaque_classes: &HashSet<String>) -> bool {
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
        Type::Object(name) => opaque_classes.contains(name),
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
