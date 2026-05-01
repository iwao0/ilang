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
    /// User-defined `enum`. Phase 1 layout: a 4-byte i32 ordinal tag
    /// (no heap, no rc). Phase 2 will add a tagged-union variant for
    /// enums with payload-carrying variants. The id indexes the
    /// compiler's `enum_layouts` table.
    Enum(u32),
    Unit,
}

impl JitTy {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_ast(
        t: &Type,
        span: ilang_ast::Span,
        class_ids: &HashMap<String, u32>,
        enum_ids: &HashMap<String, u32>,
        array_kinds: &mut Vec<ArrayKind>,
        optional_inners: &mut Vec<JitTy>,
    ) -> Result<Self, CodegenError> {
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
                // when the name matches one.
                if let Some(eid) = enum_ids.get(name).copied() {
                    JitTy::Enum(eid)
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
                JitTy::Enum(id)
            }
            Type::Array { elem, fixed } => {
                let elem_jty = JitTy::from_ast(elem, span, class_ids, enum_ids, array_kinds, optional_inners)?;
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
                let inner_jty = JitTy::from_ast(inner, span, class_ids, enum_ids, array_kinds, optional_inners)?;
                if !matches!(inner_jty, JitTy::Object(_) | JitTy::Str | JitTy::Array(_) | JitTy::Weak(_)) {
                    return Err(CodegenError::UnsupportedType {
                        ty: t.clone(),
                        span,
                    });
                }
                let id = intern_optional_inner(optional_inners, inner_jty);
                JitTy::Optional(id)
            }
            Type::Weak(inner) => {
                let inner_jty = JitTy::from_ast(inner, span, class_ids, enum_ids, array_kinds, optional_inners)?;
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
            | JitTy::Weak(_) => I64,
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
            | JitTy::Object(_)
            | JitTy::Str
            | JitTy::Array(_)
            | JitTy::Optional(_)
            | JitTy::Weak(_) => 8,
            JitTy::Unit => 0,
        }
    }

    /// True for any heap-managed type — Object, Str, Array, or any
    /// Optional thereof. Drives retain/release wiring across the
    /// lowering passes.
    pub(crate) fn is_heap(self) -> bool {
        matches!(
            self,
            JitTy::Object(_) | JitTy::Str | JitTy::Array(_) | JitTy::Optional(_) | JitTy::Weak(_)
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

#[derive(Debug, Clone)]
pub(crate) struct ClassLayout {
    pub name: String,
    pub fields: HashMap<String, (u32, JitTy)>,
    pub size: u32,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ArrayKind {
    pub elem: JitTy,
    pub fixed: Option<u32>,
}

/// Per-enum layout information used by the JIT. Phase 1 supports only
/// unit-only enums (Phase 2 will extend `variants` with payload field
/// metadata for tagged-union codegen).
#[derive(Debug, Clone)]
pub(crate) struct EnumLayout {
    pub name: String,
    /// Variant names in declaration order; the index is the i32 tag.
    pub variants: Vec<String>,
    /// `true` when every variant is unit (Phase 1 representation: bare
    /// i32 tag, no heap). `false` is reserved for Phase 2.
    pub all_unit: bool,
    /// Per-variant payload metadata (Phase 2). Indices align with
    /// `variants`; for Phase 1 every entry is `EnumVariantLayout::Unit`.
    #[allow(dead_code)]
    pub payloads: Vec<EnumVariantLayout>,
}

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
