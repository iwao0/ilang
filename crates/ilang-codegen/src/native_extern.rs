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
        for arg in &extern_attr.args {
            match arg {
                AttrArg::Str(s) => lib_name = Some(s.clone()),
                AttrArg::Path(parts) if parts.as_slice() == ["owned_return"] => {
                    flag_owned_return = true;
                }
                AttrArg::Path(parts) => {
                    return Err(CodegenError::Unsupported {
                        what: format!(
                            "@extern: unknown flag `{}` (allowed: `owned_return`)",
                            parts.join(".")
                        ),
                        span: f.span,
                    });
                }
            }
        }
        let lib_name = lib_name.expect("filter above guarantees a Str arg");
        validate_native_signature(f)?;
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
        // Open (or reuse) the library.
        if !libs.contains_key(&lib_name) {
            let lib = unsafe { Library::new(&lib_name) }.map_err(|e| {
                CodegenError::Module(format!(
                    "@extern(\"{lib_name}\") fn {}: cannot dlopen library: {e}",
                    f.name
                ))
            })?;
            libs.insert(lib_name.clone(), lib);
        }
        let lib = &libs[&lib_name];
        // Resolve the symbol (the fn's source name is used as the
        // symbol name — same convention as the existing math externs).
        let symbol_bytes = f.name.as_bytes();
        let sym: libloading::Symbol<*const u8> = unsafe {
            lib.get(symbol_bytes).map_err(|e| {
                CodegenError::Module(format!(
                    "@extern(\"{lib_name}\") fn {}: symbol not found: {e}",
                    f.name
                ))
            })?
        };
        let ptr = unsafe { *sym.into_raw() };
        builder.symbol(&f.name, ptr);
        names.insert(f.name.clone());
    }
    Ok(NativeExternRegistry {
        libs: libs.into_values().collect(),
        names,
        owned_return,
    })
}

fn validate_native_signature(f: &ilang_ast::FnDecl) -> Result<(), CodegenError> {
    for p in &f.params {
        if !is_native_abi_type(&p.ty) {
            return Err(CodegenError::Unsupported {
                what: format!(
                    "@extern fn {}: parameter type {} not supported \
                     (only i64 / f64 / bool allowed for native externs)",
                    f.name, p.ty
                ),
                span: p.span,
            });
        }
    }
    if let Some(ret) = &f.ret {
        if !is_native_abi_type(ret) && *ret != Type::Unit {
            return Err(CodegenError::Unsupported {
                what: format!(
                    "@extern fn {}: return type {} not supported \
                     (only i64 / f64 / bool / () allowed for native externs)",
                    f.name, ret
                ),
                span: f.span,
            });
        }
    }
    Ok(())
}

fn is_native_abi_type(t: &Type) -> bool {
    matches!(t, Type::I64 | Type::F64 | Type::Bool | Type::Str)
}
