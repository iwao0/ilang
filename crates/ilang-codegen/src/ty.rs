//! JIT-side type tag (`JitTy`) and the small bookkeeping types that
//! live alongside it (class layouts, array-kind interning, method info).

use std::collections::HashMap;

use cranelift::prelude::*;
use cranelift_codegen::ir::types::{F32, F64, I16, I32, I64, I8};
use cranelift_module::FuncId;
use ilang_ast::Type;
use std::collections::HashMap as StdHashMap;

use crate::error::CodegenError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum JitTy {
    I8,
    I16,
    I32,
    I64,
    U8,
    U16,
    U32,
    U64,
    F32,
    F64,
    Bool,
    /// Heap pointer to a class instance. The id indexes into the
    /// compiler's `class_layouts` / `class_methods` vecs.
    Object(u32),
    /// Heap pointer to a `Box<StringRc>`.
    Str,
    /// Heap pointer to an `ArrayHeader`. The id indexes the compiler's
    /// `array_kinds` side table for element type / fixed length.
    Array(u32),
    /// `T?` represented as a nullable pointer (0 = none). Inner must be
    /// a heap type — Optional<primitive> isn't supported in JIT yet
    /// (would require a tagged 16-byte layout). The id indexes the
    /// compiler's `optional_inners` side table.
    Optional(u32),
    /// `T.weak` — non-owning reference to a class instance. Stored as
    /// the same i64 pointer as the strong form; lifecycle goes through
    /// the weak retain/release helpers and `weak_get` checks liveness.
    /// The id is a class id, identical in shape to `Object(class_id)`.
    Weak(u32),
    /// User-defined `enum`, all variants unit. Stored as a 4-byte
    /// i32 ordinal tag (no heap, no rc). The id indexes the
    /// compiler's `enum_layouts` table.
    Enum(u32),
    /// User-defined `enum` with at least one payload-carrying variant.
    /// Heap-allocated tagged union — the same allocation header used
    /// for objects, with `[tag: i32 | padding | payload]` as the user
    /// area. The id indexes `enum_layouts`.
    EnumHeap(u32),
    /// Function pointer (`fn(T1, T2): R`). Stored as a raw i64 code
    /// address. The id indexes the compiler's `fn_signatures` table
    /// for the params/return types needed at call_indirect sites.
    Fn(u32),
    /// Built-in `Map<K, V>`. The id indexes the compiler's `map_kinds`
    /// side table for the key / value JitTys; storage is an `i64`
    /// pointer to a `MapHeader` (see runtime.rs).
    Map(u32),
    /// Anonymous tuple `(T1, T2, ...)`. Heap-allocated like an Object —
    /// shares the strong/weak/drop/vtable header so retain/release can
    /// reuse the object helpers. The id indexes the compiler's
    /// `tuple_kinds` side table for per-element layout.
    Tuple(u32),
    Unit,
}

impl JitTy {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_ast(
        t: &Type,
        span: ilang_ast::Span,
        class_ids: &HashMap<String, u32>,
        enum_ids: &HashMap<String, u32>,
        enum_layouts: &[EnumLayout],
        array_kinds: &mut Vec<ArrayKind>,
        optional_inners: &mut Vec<JitTy>,
        fn_signatures: &mut Vec<FnSignature>,
        map_kinds: &mut Vec<MapKind>,
        tuple_kinds: &mut Vec<TupleKind>,
    ) -> Result<Self, CodegenError> {
        if let Type::Fn { params, ret } = t {
            let mut p = Vec::with_capacity(params.len());
            for pt in params {
                p.push(Self::from_ast(
                    pt,
                    span,
                    class_ids,
                    enum_ids,
                    enum_layouts,
                    array_kinds,
                    optional_inners,
                    fn_signatures,
                    map_kinds,
                    tuple_kinds,
                )?);
            }
            let r = Self::from_ast(
                ret,
                span,
                class_ids,
                enum_ids,
                enum_layouts,
                array_kinds,
                optional_inners,
                fn_signatures,
                map_kinds,
                tuple_kinds,
            )?;
            let id = intern_fn_sig(fn_signatures, FnSignature { params: p, ret: r });
            return Ok(JitTy::Fn(id));
        }
        if let Type::Tuple(elems) = t {
            let mut jtys = Vec::with_capacity(elems.len());
            for et in elems {
                jtys.push(Self::from_ast(
                    et,
                    span,
                    class_ids,
                    enum_ids,
                    enum_layouts,
                    array_kinds,
                    optional_inners,
                    fn_signatures,
                    map_kinds,
                    tuple_kinds,
                )?);
            }
            let id = intern_tuple_kind(tuple_kinds, jtys);
            return Ok(JitTy::Tuple(id));
        }
        // Built-in `Map<K, V>` flows through monomorphization as
        // `Type::Generic { base: "Map", args: [K, V] }`. Resolve K and V
        // recursively, intern the pair, and produce a JitTy::Map handle.
        if let Type::Generic { base, args } = t {
            if base == "Map" && args.len() == 2 {
                let key = Self::from_ast(
                    &args[0], span, class_ids, enum_ids, enum_layouts,
                    array_kinds, optional_inners, fn_signatures, map_kinds, tuple_kinds,
                )?;
                let val = Self::from_ast(
                    &args[1], span, class_ids, enum_ids, enum_layouts,
                    array_kinds, optional_inners, fn_signatures, map_kinds, tuple_kinds,
                )?;
                let id = intern_map_kind(map_kinds, MapKind { key, val });
                return Ok(JitTy::Map(id));
            }
        }
        Ok(match t {
            Type::I8 => JitTy::I8,
            Type::I16 => JitTy::I16,
            Type::I32 => JitTy::I32,
            Type::I64 => JitTy::I64,
            Type::U8 => JitTy::U8,
            Type::U16 => JitTy::U16,
            Type::U32 => JitTy::U32,
            Type::U64 => JitTy::U64,
            Type::F32 => JitTy::F32,
            Type::F64 => JitTy::F64,
            Type::Bool => JitTy::Bool,
            Type::Str => JitTy::Str,
            Type::Unit => JitTy::Unit,
            Type::Object(name) => {
                // The parser produces Object(name) for any user-defined
                // type — could be a class or an enum. Enum lookup wins
                // when the name matches one. We need access to the
                // enum_layouts to know whether the storage is the
                // unit-tag form or a tagged-union heap allocation; we
                // smuggle that decision via a sentinel inner type:
                // EnumHeap is chosen when at least one variant has a
                // payload. We can't inspect layouts here without the
                // table; mark as Enum and let the caller (the JIT
                // compiler at the same point in its pipeline as
                // enum_layouts) resolve the storage. To keep this pure,
                // we store the storage decision at decl time by using
                // EnumHeap when payloads exist (see compiler.rs).
                if let Some(eid) = enum_ids.get(name).copied() {
                    if enum_layouts[eid as usize].all_unit {
                        JitTy::Enum(eid)
                    } else {
                        JitTy::EnumHeap(eid)
                    }
                } else {
                    let id = class_ids.get(name).copied().ok_or_else(|| {
                        CodegenError::Unsupported {
                            what: format!("unknown class {name:?}"),
                            span,
                        }
                    })?;
                    JitTy::Object(id)
                }
            }
            Type::Enum(name) => {
                let id = enum_ids.get(name).copied().ok_or_else(|| {
                    CodegenError::Unsupported {
                        what: format!("unknown enum {name:?}"),
                        span,
                    }
                })?;
                if enum_layouts[id as usize].all_unit {
                    JitTy::Enum(id)
                } else {
                    JitTy::EnumHeap(id)
                }
            }
            Type::Array { elem, fixed } => {
                let elem_jty = JitTy::from_ast(elem, span, class_ids, enum_ids, enum_layouts, array_kinds, optional_inners, fn_signatures, map_kinds, tuple_kinds)?;
                let id = intern_array_kind(
                    array_kinds,
                    ArrayKind {
                        elem: elem_jty,
                        fixed: fixed.map(|n| n as u32),
                    },
                );
                JitTy::Array(id)
            }
            Type::Optional(inner) => {
                let inner_jty = JitTy::from_ast(inner, span, class_ids, enum_ids, enum_layouts, array_kinds, optional_inners, fn_signatures, map_kinds, tuple_kinds)?;
                // Heap inner: nullable pointer (0 = None). Primitive
                // inner: heap-boxed payload (see runtime.rs for the
                // [rc, payload] layout). Either way it stores as i64.
                if matches!(inner_jty, JitTy::Unit | JitTy::Optional(_)) {
                    return Err(CodegenError::UnsupportedType {
                        ty: t.clone(),
                        span,
                    });
                }
                let id = intern_optional_inner(optional_inners, inner_jty);
                JitTy::Optional(id)
            }
            Type::Weak(inner) => {
                let inner_jty = JitTy::from_ast(inner, span, class_ids, enum_ids, enum_layouts, array_kinds, optional_inners, fn_signatures, map_kinds, tuple_kinds)?;
                match inner_jty {
                    JitTy::Object(class_id) => JitTy::Weak(class_id),
                    _ => {
                        return Err(CodegenError::UnsupportedType {
                            ty: t.clone(),
                            span,
                        });
                    }
                }
            }
            other => {
                return Err(CodegenError::UnsupportedType {
                    ty: other.clone(),
                    span,
                });
            }
        })
    }

    pub(crate) fn cl(self) -> Option<types::Type> {
        Some(match self {
            JitTy::I8 | JitTy::U8 | JitTy::Bool => I8,
            JitTy::I16 | JitTy::U16 => I16,
            JitTy::I32 | JitTy::U32 | JitTy::Enum(_) => I32,
            JitTy::I64
            | JitTy::U64
            | JitTy::Object(_)
            | JitTy::Str
            | JitTy::Array(_)
            | JitTy::Optional(_)
            | JitTy::Weak(_)
            | JitTy::EnumHeap(_)
            | JitTy::Fn(_)
            | JitTy::Map(_)
            | JitTy::Tuple(_) => I64,
            JitTy::F32 => F32,
            JitTy::F64 => F64,
            JitTy::Unit => return None,
        })
    }

    pub(crate) fn size_bytes(self) -> u32 {
        match self {
            JitTy::I8 | JitTy::U8 | JitTy::Bool => 1,
            JitTy::I16 | JitTy::U16 => 2,
            JitTy::I32 | JitTy::U32 | JitTy::F32 | JitTy::Enum(_) => 4,
            JitTy::I64
            | JitTy::U64
            | JitTy::F64
            | JitTy::EnumHeap(_)
            | JitTy::Object(_)
            | JitTy::Str
            | JitTy::Array(_)
            | JitTy::Optional(_)
            | JitTy::Weak(_)
            | JitTy::Fn(_)
            | JitTy::Map(_)
            | JitTy::Tuple(_) => 8,
            JitTy::Unit => 0,
        }
    }

    /// True for any heap-managed type — Object, Str, Array, or any
    /// Optional thereof. Drives retain/release wiring across the
    /// lowering passes.
    pub(crate) fn is_heap(self) -> bool {
        matches!(
            self,
            JitTy::Object(_)
                | JitTy::Str
                | JitTy::Array(_)
                | JitTy::Optional(_)
                | JitTy::Weak(_)
                | JitTy::EnumHeap(_)
                | JitTy::Map(_)
                | JitTy::Fn(_)
                | JitTy::Tuple(_)
        )
    }

    pub(crate) fn is_signed_int(self) -> bool {
        matches!(self, JitTy::I8 | JitTy::I16 | JitTy::I32 | JitTy::I64)
    }
    pub(crate) fn is_unsigned_int(self) -> bool {
        matches!(self, JitTy::U8 | JitTy::U16 | JitTy::U32 | JitTy::U64)
    }
    pub(crate) fn is_int(self) -> bool {
        self.is_signed_int() || self.is_unsigned_int()
    }
    pub(crate) fn is_float(self) -> bool {
        matches!(self, JitTy::F32 | JitTy::F64)
    }
    pub(crate) fn int_width(self) -> u32 {
        match self {
            JitTy::I8 | JitTy::U8 => 8,
            JitTy::I16 | JitTy::U16 => 16,
            JitTy::I32 | JitTy::U32 => 32,
            JitTy::I64 | JitTy::U64 => 64,
            _ => 0,
        }
    }
}

/// Cached signature of a function-pointer type. Indexed by `JitTy::Fn(id)`.
#[derive(Debug, Clone)]
pub(crate) struct FnSignature {
    pub params: Vec<JitTy>,
    pub ret: JitTy,
}

pub(crate) fn intern_fn_sig(table: &mut Vec<FnSignature>, sig: FnSignature) -> u32 {
    if let Some(idx) = table.iter().position(|s| s.params == sig.params && s.ret == sig.ret) {
        return idx as u32;
    }
    let idx = table.len() as u32;
    table.push(sig);
    idx
}

#[derive(Debug, Clone)]
pub(crate) struct ClassLayout {
    pub name: String,
    pub fields: HashMap<String, (u32, JitTy)>,
    pub size: u32,
    /// `extends Parent` — name of the parent class. The parent's
    /// fields are laid out first (same offsets as in the parent),
    /// the child's added fields follow. `None` for root classes.
    pub parent: Option<String>,
    /// `Some(libname)` for `@extern("lib") class Foo {}` — the
    /// runtime value is a raw C pointer (not an ARC-managed
    /// allocation). retain/release are skipped, fields are empty,
    /// `new` is rejected by the type checker.
    pub extern_lib: Option<String>,
    /// `true` for `@repr(C) class Foo { ... }`. Field offsets use
    /// natural C alignment, no methods/init, and nested repr_c
    /// fields are embedded inline.
    pub is_repr_c: bool,
    /// Strictest alignment requirement of any field in the class
    /// (max of field-type sizes for primitives, recursive for
    /// nested repr_c). Total `size` is rounded up to this.
    pub align: u32,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ArrayKind {
    pub elem: JitTy,
    pub fixed: Option<u32>,
}

/// Layout of a tuple kind. `offsets` and `size` are computed once at
/// intern time using the same `align_up` rule as `ClassLayout` so the
/// runtime ARC helpers (which expect the standard object header) work
/// unchanged.
#[derive(Debug, Clone)]
pub(crate) struct TupleKind {
    pub elems: Vec<JitTy>,
    pub offsets: Vec<u32>,
    pub size: u32,
}

pub(crate) fn intern_tuple_kind(
    table: &mut Vec<TupleKind>,
    elems: Vec<JitTy>,
) -> u32 {
    if let Some(idx) = table.iter().position(|t| t.elems == elems) {
        return idx as u32;
    }
    let mut offsets = Vec::with_capacity(elems.len());
    let mut off: u32 = 0;
    for ty in &elems {
        let sz = ty.size_bytes();
        let align = sz.max(1);
        off = align_up(off, align);
        offsets.push(off);
        off += sz;
    }
    let size = align_up(off, 8).max(8);
    let id = table.len() as u32;
    table.push(TupleKind { elems, offsets, size });
    id
}

/// Per-(K, V) info for a `Map<K, V>` instantiation. Indexed by
/// `JitTy::Map(id)`. Values are stored as raw 8-byte slots (heap V's
/// pointer or primitive V's bits); the Rust-side `MapHeader` carries a
/// per-kind drop function so heap-typed values are released correctly
/// when overwritten or when the map dies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MapKind {
    pub key: JitTy,
    pub val: JitTy,
}

pub(crate) fn intern_map_kind(table: &mut Vec<MapKind>, k: MapKind) -> u32 {
    if let Some(idx) = table.iter().position(|x| *x == k) {
        return idx as u32;
    }
    let idx = table.len() as u32;
    table.push(k);
    idx
}

/// Per-enum layout information used by the JIT. For unit-only enums
/// the storage is a bare i32 ordinal. For enums with at least one
/// payload variant, each `new` allocates a tagged-union object whose
/// user area is `[tag: i32 | padding | payload bytes]` where
/// `max_payload_size` is the max across variants.
#[derive(Debug, Clone)]
pub(crate) struct EnumLayout {
    pub name: String,
    /// Variant names in declaration order; the index is the i32 tag.
    pub variants: Vec<String>,
    /// `true` when every variant is unit (Phase 1 representation: bare
    /// i32 tag, no heap).
    pub all_unit: bool,
    /// Per-variant payload metadata; offsets are within the user
    /// payload area (i.e. `addr + ENUM_PAYLOAD_OFFSET + offset`).
    pub payloads: Vec<EnumVariantLayout>,
    /// Max payload size across variants (0 for unit-only enums).
    /// The user_size passed to `alloc_object` is
    /// `ENUM_PAYLOAD_OFFSET + max_payload_size`.
    pub max_payload_size: u32,
}

/// Tag lives at offset 0 from the user pointer. Payload starts at
/// offset 8 to keep 8-byte alignment for any inner field.
pub(crate) const ENUM_TAG_OFFSET: i32 = 0;
pub(crate) const ENUM_PAYLOAD_OFFSET: i32 = 8;

/// Per-variant payload layout. Phase 1 carries only `Unit`; Phase 2
/// adds tuple/struct payloads with offset tables for the JIT.
#[allow(dead_code)] // Tuple/Struct used by Phase 2.
#[derive(Debug, Clone)]
pub(crate) enum EnumVariantLayout {
    Unit,
    /// Positional payload — `(offset, type)` pairs, in declaration
    /// order, relative to the start of the user payload area.
    Tuple(Vec<(u32, JitTy)>),
    /// Named payload — name → (offset, type).
    Struct(StdHashMap<String, (u32, JitTy)>),
}

/// Intern an array type, returning a stable side-table id. Linear scan
/// is fine — programs rarely have more than a handful of array types.
pub(crate) fn intern_array_kind(kinds: &mut Vec<ArrayKind>, kind: ArrayKind) -> u32 {
    if let Some((i, _)) = kinds.iter().enumerate().find(|(_, k)| {
        k.elem == kind.elem && k.fixed == kind.fixed
    }) {
        return i as u32;
    }
    let id = kinds.len() as u32;
    kinds.push(kind);
    id
}

/// Intern an Optional inner type. The same approach as array kinds —
/// dedup by structural equality, return a side-table id.
pub(crate) fn intern_optional_inner(inners: &mut Vec<JitTy>, inner: JitTy) -> u32 {
    if let Some((i, _)) = inners.iter().enumerate().find(|(_, t)| **t == inner) {
        return i as u32;
    }
    let id = inners.len() as u32;
    inners.push(inner);
    id
}

#[derive(Debug, Clone)]
pub(crate) struct MethodInfo {
    pub id: FuncId,
    /// Parameter types as declared (excludes the implicit `this`).
    pub params: Vec<JitTy>,
    pub ret: JitTy,
}

pub(crate) fn align_up(offset: u32, align: u32) -> u32 {
    (offset + align - 1) & !(align - 1)
}

pub(crate) fn common_numeric_ty(l: JitTy, r: JitTy) -> Option<JitTy> {
    if l == r {
        return Some(l);
    }
    if matches!(l, JitTy::Object(_)) || matches!(r, JitTy::Object(_)) {
        return None;
    }
    if l.is_int() && r.is_int() {
        if l.is_signed_int() != r.is_signed_int() {
            return None;
        }
        return Some(if l.int_width() >= r.int_width() { l } else { r });
    }
    if l.is_float() && r.is_float() {
        return Some(if matches!(l, JitTy::F64) || matches!(r, JitTy::F64) {
            JitTy::F64
        } else {
            JitTy::F32
        });
    }
    let (int_t, float_t) = if l.is_int() { (l, r) } else { (r, l) };
    let needs_f64 = matches!(float_t, JitTy::F64) || int_t.int_width() >= 32;
    Some(if needs_f64 { JitTy::F64 } else { JitTy::F32 })
}

/// (Value, type) tuple — the canonical lowering result for an
/// expression with a value.
pub(crate) type TV = (Value, JitTy);
