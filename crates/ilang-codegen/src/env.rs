//! Lowering context plumbing: the variable environment + the FFI
//! function-id tables passed down through `lower_*` calls.

use std::collections::{HashMap, HashSet};

use cranelift::prelude::*;
use cranelift_jit::JITModule;
use cranelift_module::{FuncId, Linkage, Module};

use crate::error::CodegenError;
use crate::runtime::{StringRc, STRING_RC_SATURATED};
use crate::ty::{ArrayKind, ClassLayout, EnumLayout, FnSignature, JitTy, MethodInfo};

/// Declare an external runtime symbol with the given signature so
/// `module.declare_func_in_func` can produce a call ref later.
pub(crate) fn declare_import(
    module: &mut JITModule,
    name: &str,
    params: &[types::Type],
    ret: Option<types::Type>,
) -> Result<FuncId, CodegenError> {
    let mut sig = module.make_signature();
    for p in params {
        sig.params.push(AbiParam::new(*p));
    }
    if let Some(r) = ret {
        sig.returns.push(AbiParam::new(r));
    }
    module
        .declare_function(name, Linkage::Import, &sig)
        .map_err(|e| CodegenError::Module(e.to_string()))
}

#[derive(Default)]
pub(crate) struct Env {
    pub bindings: HashMap<String, (Variable, JitTy)>,
    /// Names introduced by `let x = <unit-typed-expr>` (e.g.
    /// `let x = loop { ... }`, `let x = console.log(...)`). Unit has
    /// no Cranelift representation so we don't allocate a `Variable`
    /// — but we still track the name so subsequent references resolve
    /// to a no-value (`Ok(None)`) result, matching the interpreter
    /// where Unit values can flow through bindings.
    pub unit_bindings: HashSet<String>,
    next_id: u32,
}

impl Env {
    pub fn next_var_id(&mut self) -> usize {
        let id = self.next_id as usize;
        self.next_id += 1;
        id
    }
}

pub(crate) struct PrintFns {
    pub i64: FuncId,
    pub u64: FuncId,
    pub f64: FuncId,
    pub f32: FuncId,
    pub bool: FuncId,
    pub space: FuncId,
    pub newline: FuncId,
    pub str: FuncId,
}

/// FFI helpers for the heap String runtime.
pub(crate) struct StrFns {
    pub concat: FuncId,
    pub eq: FuncId,
    pub retain: FuncId,
    pub release: FuncId,
    pub length: FuncId,
    pub char_at: FuncId,
    pub includes: FuncId,
    pub starts_with: FuncId,
    pub ends_with: FuncId,
    pub to_upper: FuncId,
    pub to_lower: FuncId,
    pub trim: FuncId,
}

/// FFI helpers for the heap array runtime. `push_<width>` is picked by
/// the JIT based on the static element type.
pub(crate) struct ArrayFns {
    pub new: FuncId,
    pub retain: FuncId,
    pub release: FuncId,
    pub push_i8: FuncId,
    pub push_i16: FuncId,
    pub push_i32: FuncId,
    pub push_i64: FuncId,
    pub push_f32: FuncId,
    pub push_f64: FuncId,
}

pub(crate) struct LowerCtx<'a> {
    pub funcs: &'a HashMap<String, (FuncId, Vec<JitTy>, JitTy)>,
    pub class_layouts: &'a [ClassLayout],
    pub class_methods: &'a [HashMap<String, MethodInfo>],
    pub enum_layouts: &'a [EnumLayout],
    pub alloc_object_id: FuncId,
    pub retain_object_id: FuncId,
    pub release_object_id: FuncId,
    pub retain_weak_id: FuncId,
    pub release_weak_id: FuncId,
    pub weak_get_id: FuncId,
    pub print: PrintFns,
    pub strfns: StrFns,
    pub arrfns: ArrayFns,
    pub module: &'a mut JITModule,
    pub env: &'a mut Env,
    pub loops: Vec<(Block, Block)>,
    /// `(this var, class id)` while compiling a method body.
    pub this: Option<(Variable, u32)>,
    /// Declared return type of the function currently being lowered;
    /// `Unit` for `__main` when the program has no tail expression.
    /// Used by `ExprKind::Return` to coerce the value.
    pub current_ret_ty: JitTy,
    /// `true` while compiling a `deinit` body. The early-return path
    /// must skip releasing `this` to avoid re-entering `release_object`
    /// on rc=0.
    pub current_fn_is_deinit: bool,
    /// Per-compilation interning bucket for string literals; the boxed
    /// StringRc is held here so its storage lives for the compiler's
    /// lifetime, and its pointer is embedded as `iconst`. The interned
    /// rc is saturated so `release_string` never frees these.
    pub interned_strings: &'a mut Vec<Box<StringRc>>,
    pub array_kinds: &'a mut Vec<ArrayKind>,
    pub optional_inners: &'a mut Vec<JitTy>,
    pub fn_signatures: &'a mut Vec<FnSignature>,
    /// Per-class drop wrappers, indexed by class id. `None` for trivial
    /// classes (no `deinit`, no heap fields). See drops.rs.
    pub class_drops: &'a [Option<FuncId>],
    /// Per-array-kind drop wrappers, populated lazily during lowering
    /// (the compiler discovers new array kinds while lowering and
    /// declares drop fns on the fly). `None` means the kind has no
    /// heap elements, so no per-element release is needed.
    pub array_drops: &'a mut HashMap<u32, Option<FuncId>>,
    /// Per-enum drop wrappers, declared lazily during lowering. `None`
    /// means the enum has no heap-typed payload fields anywhere.
    pub enum_drops: &'a mut HashMap<u32, Option<FuncId>>,
}

impl<'a> LowerCtx<'a> {
    pub fn intern_string(&mut self, s: &str) -> i64 {
        let boxed = Box::new(StringRc {
            rc: STRING_RC_SATURATED,
            s: s.to_string(),
        });
        let ptr = boxed.as_ref() as *const StringRc as i64;
        self.interned_strings.push(boxed);
        ptr
    }
}

/// Reverse-lookup from class name to id so the lowering paths can
/// resolve annotations like `let x: Foo = ...` without a full
/// TypeChecker.
pub(crate) fn class_ids_from(lc: &LowerCtx) -> HashMap<String, u32> {
    lc.class_layouts
        .iter()
        .enumerate()
        .map(|(i, l)| (l.name.clone(), i as u32))
        .collect()
}

pub(crate) fn enum_ids_from(lc: &LowerCtx) -> HashMap<String, u32> {
    lc.enum_layouts
        .iter()
        .enumerate()
        .map(|(i, l)| (l.name.clone(), i as u32))
        .collect()
}
