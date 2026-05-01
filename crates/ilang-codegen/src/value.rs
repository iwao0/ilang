//! Host-side representation of JIT results (`JitValue`) and the
//! reverse-walker that rebuilds it from a JIT heap layout.

use crate::runtime::{ArrayHeader, StringRc};
use crate::ty::{ArrayKind, ClassLayout, JitTy};

#[derive(Debug, Clone, PartialEq)]
pub enum JitValue {
    I8(i8),
    I16(i16),
    I32(i32),
    I64(i64),
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    F32(f32),
    F64(f64),
    Bool(bool),
    Object { class: String, ptr: i64 },
    Str(String),
    Array(Vec<JitValue>),
    Unit,
}

impl std::fmt::Display for JitValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JitValue::I8(n) => write!(f, "{n}"),
            JitValue::I16(n) => write!(f, "{n}"),
            JitValue::I32(n) => write!(f, "{n}"),
            JitValue::I64(n) => write!(f, "{n}"),
            JitValue::U8(n) => write!(f, "{n}"),
            JitValue::U16(n) => write!(f, "{n}"),
            JitValue::U32(n) => write!(f, "{n}"),
            JitValue::U64(n) => write!(f, "{n}"),
            JitValue::F32(x) => fmt_float(f, *x as f64),
            JitValue::F64(x) => fmt_float(f, *x),
            JitValue::Bool(b) => write!(f, "{b}"),
            JitValue::Object { class, ptr } => write!(f, "<{class} @ {ptr:#x}>"),
            JitValue::Str(s) => write!(f, "{s}"),
            JitValue::Array(items) => {
                write!(f, "[")?;
                for (i, v) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{v}")?;
                }
                write!(f, "]")
            }
            JitValue::Unit => Ok(()),
        }
    }
}

fn fmt_float(f: &mut std::fmt::Formatter<'_>, x: f64) -> std::fmt::Result {
    if x.is_finite() && x.fract() == 0.0 {
        write!(f, "{x:.1}")
    } else {
        write!(f, "{x}")
    }
}

/// Walk a JITed array's heap layout and rebuild a `Vec<JitValue>` for
/// the host. Recurses into element type so nested arrays / strings /
/// objects round-trip correctly.
pub(crate) unsafe fn read_array(
    header_ptr: i64,
    kind: ArrayKind,
    array_kinds: &[ArrayKind],
    class_layouts: &[ClassLayout],
) -> Vec<JitValue> {
    if header_ptr == 0 {
        return Vec::new();
    }
    let header = header_ptr as *const ArrayHeader;
    let len = (*header).len as usize;
    let data = (*header).data_ptr;
    let elem_size = kind.elem.size_bytes() as i64;
    let mut out = Vec::with_capacity(len);
    for i in 0..len {
        let p = data + (i as i64) * elem_size;
        let v = match kind.elem {
            JitTy::I8 => JitValue::I8(*(p as *const i8)),
            JitTy::I16 => JitValue::I16(*(p as *const i16)),
            JitTy::I32 => JitValue::I32(*(p as *const i32)),
            JitTy::I64 => JitValue::I64(*(p as *const i64)),
            JitTy::U8 => JitValue::U8(*(p as *const u8)),
            JitTy::U16 => JitValue::U16(*(p as *const u16)),
            JitTy::U32 => JitValue::U32(*(p as *const u32)),
            JitTy::U64 => JitValue::U64(*(p as *const u64)),
            JitTy::F32 => JitValue::F32(*(p as *const f32)),
            JitTy::F64 => JitValue::F64(*(p as *const f64)),
            JitTy::Bool => JitValue::Bool(*(p as *const i8) != 0),
            JitTy::Str => JitValue::Str((*(*(p as *const i64) as *const StringRc)).s.clone()),
            JitTy::Object(id) => JitValue::Object {
                class: class_layouts[id as usize].name.clone(),
                ptr: *(p as *const i64),
            },
            JitTy::Array(id) => JitValue::Array(read_array(
                *(p as *const i64),
                array_kinds[id as usize],
                array_kinds,
                class_layouts,
            )),
            JitTy::Unit => JitValue::Unit,
        };
        out.push(v);
    }
    out
}
