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

use crate::error::CodegenError;

pub(crate) fn register_native_externs(
    builder: &mut JITBuilder,
    prog: &Program,
) -> Result<Vec<Library>, CodegenError> {
    use std::collections::HashMap;
    let mut libs: HashMap<String, Library> = HashMap::new();
    for item in &prog.items {
        let Item::Fn(f) = item else { continue };
        // Find an `@extern("libname")` attribute (string-arg form).
        // `@extern` with no args is the legacy host-side form, which
        // is registered separately by math_externs / test_externs and
        // doesn't need a library lookup.
        let lib_name = f.attrs.iter().find_map(|a| {
            if a.name != "extern" {
                return None;
            }
            for arg in &a.args {
                if let AttrArg::Str(s) = arg {
                    return Some(s.clone());
                }
            }
            None
        });
        let Some(lib_name) = lib_name else { continue };
        validate_native_signature(f)?;
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
    }
    // Move the Library handles out — JitCompiler keeps them alive.
    Ok(libs.into_values().collect())
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
    matches!(t, Type::I64 | Type::F64 | Type::Bool)
}
