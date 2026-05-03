//! Lowering context plumbing: the variable environment + the FFI
//! function-id tables passed down through `lower_*` calls.

use std::collections::{HashMap, HashSet};

use cranelift::prelude::*;
use cranelift_jit::JITModule;
use cranelift_module::{FuncId, Linkage, Module};

use crate::error::CodegenError;
use ilang_ast::Span;
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
    pub replace: FuncId,
    pub slice: FuncId,
    pub split: FuncId,
    pub trim: FuncId,
    /// `string` ↔ `*const c_char` marshalling used by
    /// `@extern("libname")` calls. `to_c_str` allocates a NUL-
    /// terminated copy, `free_c_str` frees it after the call,
    /// `c_str_to_string` copies a C-owned pointer back into a
    /// fresh StringRc.
    pub to_c_str: FuncId,
    pub free_c_str: FuncId,
    pub c_str_to_string: FuncId,
    /// `libc::free` for `@extern(..., owned_return)` returns.
    pub libc_free: FuncId,
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
    pub map_new_id: FuncId,
    pub retain_map_id: FuncId,
    pub release_map_id: FuncId,
    pub map_set_id: FuncId,
    pub map_has_id: FuncId,
    pub map_size_id: FuncId,
    pub map_delete_id: FuncId,
    pub map_index_get_id: FuncId,
    pub map_get_or_null_id: FuncId,
    pub map_keys_to_array_id: FuncId,
    pub map_values_to_array_id: FuncId,
    pub optional_box_new_id: FuncId,
    pub optional_box_retain_id: FuncId,
    pub optional_box_release_id: FuncId,
    pub panic_index_oob_id: FuncId,
    pub panic_div_zero_id: FuncId,
    pub panic_unwrap_none_id: FuncId,
    /// Per-(K, V) value-retain helper, lazily generated. Mirrors
    /// `map_drops` in shape but emits `emit_retain_heap` per V instead.
    pub map_value_retains: &'a mut HashMap<u32, Option<FuncId>>,
    pub module: &'a mut JITModule,
    pub env: &'a mut Env,
    /// Stack of currently-open loops. Each entry is
    /// `(continue_block, after_block, break_value_slot)`. The slot is
    /// `Some((var, jit_ty))` when this is a `loop` whose result type is
    /// non-Unit (so `break v` stores `v` into `var` before jumping to
    /// `after`); `None` for `while` / `for` / Unit-result `loop`.
    pub loops: Vec<(Block, Block, Option<(cranelift::prelude::Variable, crate::ty::JitTy)>)>,
    /// Per-`loop` expression span → result type, populated by the
    /// typechecker. Read by `lower_loop` to allocate the result slot.
    pub loop_break_types: &'a HashMap<Span, ilang_ast::Type>,
    /// Names of `@extern("libname")` fns. Read by Call lowering to
    /// decide whether to insert string ↔ C-string conversions.
    pub native_extern_fns: &'a std::collections::HashSet<String>,
    pub extern_fn_names: &'a std::collections::HashSet<String>,
    /// Subset of `native_extern_fns` whose `string` return is
    /// callee-owned (`strdup`-style). The Call lowering emits
    /// `libc::free` on the C pointer after copying it.
    pub native_extern_owned_return: &'a std::collections::HashSet<String>,
    pub native_extern_free_with: &'a std::collections::HashMap<String, String>,
    pub native_extern_variadic: &'a std::collections::HashSet<String>,
    pub native_extern_by_value: &'a std::collections::HashSet<String>,
    /// `(class, field) -> slot index` into `static_field_base_addr`.
    /// Read by Field / AssignField lowering on `ClassName.field`.
    pub static_field_slots:
        &'a std::collections::HashMap<(String, String), usize>,
    /// `(class, field) -> declared type` (i64/f64/bool). Lowering
    /// uses it to bitcast / truncate after loading the i64 slot.
    pub static_field_types:
        &'a std::collections::HashMap<(String, String), ilang_ast::Type>,
    /// Base address of the `Box<[i64]>` static-field storage,
    /// embedded as an iconst in lowered field accesses.
    pub static_field_base_addr: i64,
    /// Per-class vtable base addresses, indexed by class id. Used
    /// at virtual-method call sites and at `new` (passed to
    /// `alloc_object`). 0 if a class has no vtable.
    pub class_vtable_addrs: &'a [i64],
    /// `(class, method) -> slot index` into the vtable. Forwarded
    /// from the typechecker. The lowering uses `(class.name, method)`
    /// as key (class name resolved from receiver's JitTy::Object id).
    pub class_method_slots:
        &'a std::collections::HashMap<String, std::collections::HashMap<String, usize>>,
    /// `class -> parent` map. Used by `super.method(...)` lowering
    /// (resolved at compile time to the parent's specific function).
    pub class_parents: &'a std::collections::HashMap<String, String>,
    /// Lexical class for the body currently being lowered. `Some`
    /// while lowering a method body (so `super` knows whose parent
    /// to look up); `None` for top-level fns and `__main`.
    pub current_class: Option<String>,
    /// Runtime helper to allocate a closure struct
    /// (`[fn_ptr | env_field0 | ...]`). Stage A storage is leaked
    /// (no ARC); Stage B/C will integrate retain/release.
    pub alloc_closure_id: FuncId,
    pub retain_closure_id: FuncId,
    pub release_closure_id: FuncId,
    /// `(closure_wrapper_name) -> (param tys + ret ty + capture
    /// names+tys)`. Set when a closure expression is lowered or
    /// when a top-level fn ref is auto-trampolined. Used to look
    /// up signatures and capture offsets.
    pub closure_meta:
        &'a std::collections::HashMap<String, ClosureMeta>,
    /// Cache of trampoline FuncIds for top-level fns whose
    /// addresses were taken (`let f = some_top_level`). Built
    /// lazily during lowering and reused on subsequent refs.
    pub closure_trampolines: &'a mut std::collections::HashMap<String, FuncId>,
    /// Per-wrapper drop FuncId cache (closure_drops in JitCompiler).
    /// `None` value = no heap captures, drop_fn_ptr is 0.
    pub closure_drops: &'a mut std::collections::HashMap<String, Option<FuncId>>,
    /// While lowering a closure-wrapper body, this holds the
    /// `(env_var, capture_offsets)` so a Var(name) lookup can
    /// emit `load(env + offset)` instead of failing.
    pub closure_capture_env: Option<ClosureEnv<'a>>,
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
    pub map_kinds: &'a mut Vec<crate::ty::MapKind>,
    pub tuple_kinds: &'a mut Vec<crate::ty::TupleKind>,
    /// Per-tuple-kind drop wrapper, lazily declared during lowering.
    /// `None` means no element is heap so no drop is needed.
    pub tuple_drops: &'a mut HashMap<u32, Option<FuncId>>,
    /// Per-(K, V) drop helper for Map values; lazily generated.
    pub map_drops: &'a mut HashMap<u32, Option<FuncId>>,
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

/// Per-closure-wrapper metadata: the wrapper's user-facing param
/// types (env_ptr stripped), the return type, and the capture
/// list (name + JIT type) in offset order.
#[derive(Clone, Debug)]
pub(crate) struct ClosureMeta {
    pub user_params: Vec<crate::ty::JitTy>,
    pub ret: crate::ty::JitTy,
    pub captures: Vec<(String, crate::ty::JitTy)>,
}

/// Capture environment in scope while lowering a closure body.
/// `env_var` is the Cranelift Variable holding the env_ptr (the
/// closure struct itself); `captures` lists each captured name +
/// (offset_from_env, jit_type).
pub(crate) struct ClosureEnv<'a> {
    pub env_var: Variable,
    pub captures: &'a [(String, u32, crate::ty::JitTy)],
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
