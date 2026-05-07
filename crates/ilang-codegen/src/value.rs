// Edition 2024 promotes unsafe-op-in-unsafe-fn from allow to warn; the
// `unsafe fn` walkers in this module read JIT heap layouts via raw
// pointers throughout, so opting back to the implicit-unsafe form
// keeps them readable.
#![allow(unsafe_op_in_unsafe_fn)]

//! Host-side representation of JIT results (`JitValue`) and the
//! reverse-walker that rebuilds it from a JIT heap layout.


use crate::runtime::{ArrayHeader, StringRc};
use crate::ty::{
    ArrayKind, ClassLayout, EnumLayout, EnumVariantLayout, JitTy, ENUM_PAYLOAD_OFFSET,
    ENUM_TAG_OFFSET,
};

/// Reconstruct a host-side `JitValue::Enum` for an enum-heap pointer.
/// Used by `run_main` and `read_array` for enum results / array
/// elements.
pub(crate) unsafe fn read_enum_heap(
    ptr: i64,
    enum_id: u32,
    enum_layouts: &[EnumLayout],
    array_kinds: &[ArrayKind],
    class_layouts: &[ClassLayout],
    optional_inners: &[JitTy],
) -> JitValue {
    let layout = &enum_layouts[enum_id as usize];
    let tag = *((ptr + ENUM_TAG_OFFSET as i64) as *const i32) as i64;
    let idx = layout.tags.iter().position(|&t| t == tag);
    let variant_name = idx
        .and_then(|i| layout.variants.get(i))
        
        .map(|s| s.as_str().to_string()).unwrap_or_else(|| format!("?{tag}"));
    let payload_addr = ptr + ENUM_PAYLOAD_OFFSET as i64;
    let payload = match idx.and_then(|i| layout.payloads.get(i)) {
        Some(EnumVariantLayout::Unit) | None => JitEnumPayload::Unit,
        Some(EnumVariantLayout::Tuple(entries)) => {
            let items = entries
                .iter()
                .map(|(off, fty)| {
                    read_field(
                        payload_addr + (*off as i64),
                        *fty,
                        array_kinds,
                        class_layouts,
                        enum_layouts,
                        optional_inners,
                    )
                })
                .collect();
            JitEnumPayload::Tuple(items)
        }
        Some(EnumVariantLayout::Struct(map)) => {
            let mut items: Vec<(String, JitValue)> = map
                .iter()
                .map(|(name, (off, fty))| {
                    (
                        name.as_str().to_string(),
                        read_field(
                            payload_addr + (*off as i64),
                            *fty,
                            array_kinds,
                            class_layouts,
                            enum_layouts,
                            optional_inners,
                        ),
                    )
                })
                .collect();
            items.sort_by(|a, b| a.0.cmp(&b.0));
            JitEnumPayload::Struct(items)
        }
    };
    JitValue::Enum {
        ty: layout.name.as_str().to_string(),
        variant: variant_name,
        payload,
    }
}

/// Read a single typed field from a raw memory address into a host
/// `JitValue`. Used by `read_enum_heap` for payload fields.
unsafe fn read_field(
    addr: i64,
    fty: JitTy,
    array_kinds: &[ArrayKind],
    class_layouts: &[ClassLayout],
    enum_layouts: &[EnumLayout],
    optional_inners: &[JitTy],
) -> JitValue {
    match fty {
        JitTy::I8 => JitValue::I8(*(addr as *const i8)),
        JitTy::I16 => JitValue::I16(*(addr as *const i16)),
        JitTy::I32 => JitValue::I32(*(addr as *const i32)),
        JitTy::I64 => JitValue::I64(*(addr as *const i64)),
        JitTy::U8 => JitValue::U8(*(addr as *const u8)),
        JitTy::U16 => JitValue::U16(*(addr as *const u16)),
        JitTy::U32 => JitValue::U32(*(addr as *const u32)),
        JitTy::U64 => JitValue::U64(*(addr as *const u64)),
        JitTy::F32 => JitValue::F32(*(addr as *const f32)),
        JitTy::F64 => JitValue::F64(*(addr as *const f64)),
        JitTy::Bool => JitValue::Bool(*(addr as *const i8) != 0),
        JitTy::Str => JitValue::Str((*(*(addr as *const i64) as *const StringRc)).s.clone()),
        JitTy::Object(id) => JitValue::Object {
            class: class_layouts[id as usize].name.as_str().to_string(),
            ptr: *(addr as *const i64),
        },
        JitTy::Array(id) => JitValue::Array(read_array(
            *(addr as *const i64),
            array_kinds[id as usize],
            array_kinds,
            class_layouts,
            enum_layouts,
            optional_inners,
        )),
        JitTy::Optional(id) => read_optional_pointer(
            *(addr as *const i64),
            optional_inners[id as usize],
            array_kinds,
            class_layouts,
            enum_layouts,
            optional_inners,
        ),
        JitTy::Weak(class_id) => {
            let raw = *(addr as *const i64);
            let alive = if raw == 0 {
                false
            } else {
                *((raw - 24) as *const i64) > 0
            };
            JitValue::Weak {
                class: class_layouts[class_id as usize].name.as_str().to_string(),
                alive,
            }
        }
        JitTy::Enum(id) => {
            let tag = *(addr as *const i32) as i64;
            let layout = &enum_layouts[id as usize];
            let idx = layout.tags.iter().position(|&t| t == tag);
            JitValue::Enum {
                ty: layout.name.as_str().to_string(),
                variant: idx
                    .and_then(|i| layout.variants.get(i))
                    
                    .map(|s| s.as_str().to_string()).unwrap_or_else(|| format!("?{tag}")),
                payload: JitEnumPayload::Unit,
            }
        }
        JitTy::EnumHeap(id) => read_enum_heap(
            *(addr as *const i64),
            id,
            enum_layouts,
            array_kinds,
            class_layouts,
            optional_inners,
        ),
        JitTy::Fn(_) => JitValue::Fn(*(addr as *const i64)),
        JitTy::Map(_) => JitValue::Map {
            key_ty: "?".into(),
            val_ty: "?".into(),
            size: 0,
        },
        JitTy::Tuple(_) => JitValue::Tuple { ptr: *(addr as *const i64) },
        JitTy::EmbeddedArray(_) | JitTy::FlexArray(_) => unreachable!(
            "embedded arrays are inline bytes — not surfaced through read paths"
        ),
        JitTy::Unit => JitValue::Unit,
    }
}

/// Walk a `T?` slot whose pointer is at `p` (i64 representation; 0 = none).
/// Used by `run_main` for the program's tail value and by `read_array`
/// for Optional elements.
#[allow(clippy::too_many_arguments)]
pub(crate) unsafe fn read_optional_pointer(
    p: i64,
    inner: JitTy,
    array_kinds: &[ArrayKind],
    class_layouts: &[ClassLayout],
    enum_layouts: &[EnumLayout],
    optional_inners: &[JitTy],
) -> JitValue {
    if p == 0 {
        return JitValue::None;
    }
    let v = match inner {
        JitTy::Str => JitValue::Str((*(p as *const StringRc)).s.clone()),
        JitTy::Object(id) => JitValue::Object {
            class: class_layouts[id as usize].name.as_str().to_string(),
            ptr: p,
        },
        JitTy::Array(id) => JitValue::Array(read_array(
            p,
            array_kinds[id as usize],
            array_kinds,
            class_layouts,
            enum_layouts,
            optional_inners,
        )),
        JitTy::Weak(class_id) => {
            let alive = *((p - 24) as *const i64) > 0;
            JitValue::Weak {
                class: class_layouts[class_id as usize].name.as_str().to_string(),
                alive,
            }
        }
        JitTy::EnumHeap(id) => read_enum_heap(
            p,
            id,
            enum_layouts,
            array_kinds,
            class_layouts,
            optional_inners,
        ),
        // Primitive Optional: payload is at p + 8 (after rc).
        JitTy::I8 => JitValue::I8(*((p + 8) as *const i8)),
        JitTy::I16 => JitValue::I16(*((p + 8) as *const i16)),
        JitTy::I32 => JitValue::I32(*((p + 8) as *const i32)),
        JitTy::I64 => JitValue::I64(*((p + 8) as *const i64)),
        JitTy::U8 => JitValue::U8(*((p + 8) as *const u8)),
        JitTy::U16 => JitValue::U16(*((p + 8) as *const u16)),
        JitTy::U32 => JitValue::U32(*((p + 8) as *const u32)),
        JitTy::U64 => JitValue::U64(*((p + 8) as *const u64)),
        JitTy::F32 => JitValue::F32(*((p + 8) as *const f32)),
        JitTy::F64 => JitValue::F64(*((p + 8) as *const f64)),
        JitTy::Bool => JitValue::Bool(*((p + 8) as *const i8) != 0),
        JitTy::Enum(id) => {
            let tag = *((p + 8) as *const i32) as i64;
            let layout = &enum_layouts[id as usize];
            let idx = layout.tags.iter().position(|&t| t == tag);
            JitValue::Enum {
                ty: layout.name.as_str().to_string(),
                variant: idx
                    .and_then(|i| layout.variants.get(i))
                    
                    .map(|s| s.as_str().to_string()).unwrap_or_else(|| format!("?{tag}")),
                payload: JitEnumPayload::Unit,
            }
        }
        _ => unreachable!("unexpected Optional inner type"),
    };
    JitValue::Some(Box::new(v))
}

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
    /// `T?` — `None` is the absent state; `Some(v)` wraps the present
    /// inner value (always heap-typed in JIT).
    None,
    Some(Box<JitValue>),
    /// `T.weak` — the host-side image is just the class name and a
    /// liveness bit. The JIT pointer isn't surfaced because using it
    /// without going through `weak_get` would defeat ARC.
    Weak { class: String, alive: bool },
    /// User-defined enum value surfaced to the host. Unit variants
    /// have no payload (mirrors Phase 1); payload-carrying variants
    /// (Phase 2) attach the inner values.
    Enum {
        ty: String,
        variant: String,
        payload: JitEnumPayload,
    },
    /// First-class function value — surfaced as the raw code address.
    /// Two `Fn(p)` values compare equal iff they point at the same
    /// JITed function.
    Fn(i64),
    /// Tuple result surfaced as a raw heap pointer. Element-level
    /// unwrap isn't done here — tests print individual elements via
    /// `console.log(t[i])` from inside the JIT program.
    Tuple { ptr: i64 },
    /// Built-in `Map<K, V>` surfaced to the host. Internals are
    /// summarized; full key/value enumeration would require dispatching
    /// on K/V kinds and is out of scope for the simple Display.
    Map { key_ty: String, val_ty: String, size: i64 },
}

#[derive(Debug, Clone, PartialEq)]
pub enum JitEnumPayload {
    Unit,
    Tuple(Vec<JitValue>),
    Struct(Vec<(String, JitValue)>),
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
            JitValue::None => write!(f, "none"),
            JitValue::Some(v) => write!(f, "some({v})"),
            JitValue::Weak { class, alive: true } => write!(f, "<weak {class} alive>"),
            JitValue::Weak { class, alive: false } => write!(f, "<weak {class} dead>"),
            JitValue::Fn(p) => write!(f, "<fn @ {p:#x}>"),
            JitValue::Tuple { ptr } => write!(f, "<tuple @ {ptr:#x}>"),
            JitValue::Map { key_ty, val_ty, size } => {
                write!(f, "<Map<{key_ty}, {val_ty}> size={size}>")
            }
            JitValue::Enum { ty, variant, payload } => match payload {
                JitEnumPayload::Unit => write!(f, "{ty}::{variant}"),
                JitEnumPayload::Tuple(items) => {
                    write!(f, "{ty}::{variant}(")?;
                    for (i, v) in items.iter().enumerate() {
                        if i > 0 {
                            write!(f, ", ")?;
                        }
                        write!(f, "{v}")?;
                    }
                    write!(f, ")")
                }
                JitEnumPayload::Struct(fields) => {
                    write!(f, "{ty}::{variant} {{ ")?;
                    for (i, (name, v)) in fields.iter().enumerate() {
                        if i > 0 {
                            write!(f, ", ")?;
                        }
                        write!(f, "{name}: {v}")?;
                    }
                    write!(f, " }}")
                }
            },
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
#[allow(clippy::too_many_arguments)]
pub(crate) unsafe fn read_array(
    header_ptr: i64,
    kind: ArrayKind,
    array_kinds: &[ArrayKind],
    class_layouts: &[ClassLayout],
    enum_layouts: &[EnumLayout],
    optional_inners: &[JitTy],
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
                class: class_layouts[id as usize].name.as_str().to_string(),
                ptr: *(p as *const i64),
            },
            JitTy::Weak(class_id) => {
                let raw = *(p as *const i64);
                let alive = if raw == 0 {
                    false
                } else {
                    *((raw - 24) as *const i64) > 0
                };
                JitValue::Weak {
                    class: class_layouts[class_id as usize].name.as_str().to_string(),
                    alive,
                }
            }
            JitTy::Array(id) => JitValue::Array(read_array(
                *(p as *const i64),
                array_kinds[id as usize],
                array_kinds,
                class_layouts,
                enum_layouts,
                optional_inners,
            )),
            JitTy::Optional(id) => read_optional_pointer(
                *(p as *const i64),
                optional_inners[id as usize],
                array_kinds,
                class_layouts,
                enum_layouts,
                optional_inners,
            ),
            JitTy::Enum(id) => {
                let tag = *(p as *const i32) as i64;
                let layout = &enum_layouts[id as usize];
                let idx = layout.tags.iter().position(|&t| t == tag);
                JitValue::Enum {
                    ty: layout.name.as_str().to_string(),
                    variant: idx
                        .and_then(|i| layout.variants.get(i))
                        
                        .map(|s| s.as_str().to_string()).unwrap_or_else(|| format!("?{tag}")),
                    payload: JitEnumPayload::Unit,
                }
            }
            JitTy::EnumHeap(id) => read_enum_heap(
                *(p as *const i64),
                id,
                enum_layouts,
                array_kinds,
                class_layouts,
                optional_inners,
            ),
            JitTy::Fn(_) => JitValue::Fn(*(p as *const i64)),
            JitTy::Map(_) => JitValue::Map {
                key_ty: "?".into(),
                val_ty: "?".into(),
                size: 0,
            },
            JitTy::Tuple(_) => JitValue::Tuple { ptr: *(p as *const i64) },
            JitTy::EmbeddedArray(_) | JitTy::FlexArray(_) => unreachable!(
                "embedded arrays are inline bytes — not surfaced through read paths"
            ),
            JitTy::Unit => JitValue::Unit,
        };
        out.push(v);
    }
    out
}
