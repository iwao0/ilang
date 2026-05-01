//! JIT-side type tag (`JitTy`) and the small bookkeeping types that
//! live alongside it (class layouts, array-kind interning, method info).

use std::collections::HashMap;

use cranelift::prelude::*;
use cranelift_codegen::ir::types::{F32, F64, I16, I32, I64, I8};
use cranelift_module::FuncId;
use ilang_ast::Type;

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
    Unit,
}

impl JitTy {
    pub(crate) fn from_ast(
        t: &Type,
        span: ilang_ast::Span,
        class_ids: &HashMap<String, u32>,
        array_kinds: &mut Vec<ArrayKind>,
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
                let id = class_ids.get(name).copied().ok_or_else(|| {
                    CodegenError::Unsupported {
                        what: format!("unknown class {name:?}"),
                        span,
                    }
                })?;
                JitTy::Object(id)
            }
            Type::Array { elem, fixed } => {
                let elem_jty = JitTy::from_ast(elem, span, class_ids, array_kinds)?;
                let id = intern_array_kind(
                    array_kinds,
                    ArrayKind {
                        elem: elem_jty,
                        fixed: fixed.map(|n| n as u32),
                    },
                );
                JitTy::Array(id)
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
            JitTy::I32 | JitTy::U32 => I32,
            JitTy::I64 | JitTy::U64 | JitTy::Object(_) | JitTy::Str | JitTy::Array(_) => I64,
            JitTy::F32 => F32,
            JitTy::F64 => F64,
            JitTy::Unit => return None,
        })
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
    pub(crate) fn size_bytes(self) -> u32 {
        match self {
            JitTy::I8 | JitTy::U8 | JitTy::Bool => 1,
            JitTy::I16 | JitTy::U16 => 2,
            JitTy::I32 | JitTy::U32 | JitTy::F32 => 4,
            JitTy::I64
            | JitTy::U64
            | JitTy::F64
            | JitTy::Object(_)
            | JitTy::Str
            | JitTy::Array(_) => 8,
            JitTy::Unit => 0,
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
