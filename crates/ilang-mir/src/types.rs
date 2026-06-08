//! MIR-level type representation.
//!
//! Mirrors `ilang_ast::Type` but tuned for SSA: every value carries a
//! `MirTy`. Generic instantiation collapses `TypeVar` away during
//! monomorphisation, so post-monomorph MIR contains no `TypeVar`.

use ilang_ast::Symbol;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum MirTy {
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
    Str,
    Unit,
    /// Heap class instance. Carries a `ClassId` index into
    /// `Program::classes` (post-monomorph all generics are concrete).
    Object(ClassId),
    /// Weak reference to a class instance.
    Weak(ClassId),
    /// `enum E` instance, after monomorph.
    Enum(EnumId),
    /// Inline enum slot inside a CRepr / CPacped / CUnion struct
    /// field. Carries the same `EnumId` as `Enum(_)` but is laid
    /// out at the underlying repr's width (`u8` / `u16` / `u32` /
    /// `i32`...) instead of as an 8-byte heap-box pointer.
    ///
    /// Only appears in `meta.field_ty` for CRepr-family struct
    /// fields whose declared type is a unit-only, int-repr enum.
    /// SSA values **never** carry this variant — `LoadField` lower
    /// promotes the field to `MirTy::Enum(eid)` via
    /// `BodyCx::loaded_field_ty` so every downstream op sees a
    /// regular heap enum cell. The variant exists so the
    /// retain/release predicates (`is_heap` / `is_arc_slot`) can
    /// statically exclude the inline slot without each call site
    /// re-deriving "is the parent class CRepr?".
    CReprEnum(EnumId),
    /// Dynamic / fixed array. `len = None` is dynamic, `Some(n)` is
    /// `T[N]` (length is part of the type).
    Array { elem: Box<MirTy>, len: Option<usize> },
    Tuple(Box<[MirTy]>),
    Optional(Box<MirTy>),
    Map { key: Box<MirTy>, val: Box<MirTy> },
    /// `Set<T>` — built-in hash set. Element kind constraints match
    /// `Map`'s keys (string / integer / bool); element insertion /
    /// lookup / deletion live behind the `$set.*` runtime helpers.
    Set { elem: Box<MirTy> },
    /// `Promise<T>` — built-in async value. Heap-allocated, atomic
    /// refcount, settled exactly once.
    Promise(Box<MirTy>),
    /// Closure / first-class function value. Always `(fn_ptr, env_ptr)`
    /// at runtime; the env may be null for trampoline-wrapped top-level
    /// functions.
    Fn(Box<MirFnTy>),
    /// Raw C function pointer — bare 8-byte code address, no closure box.
    /// Produced by `*void as fn(...)` casts inside @extern(C) blocks
    /// (typical use: typing the result of `GetProcAddress` / `dlsym`).
    /// At call time uses `Inst::CallRawIndirect`: no fn_ptr-from-offset-0
    /// load, no trailing env arg.
    ///
    /// At ABI level RawFn is a plain pointer-sized value (8 bytes on x64),
    /// so most match arms can treat it identically to `RawPtr` /
    /// `CVoid` / `I64`.
    RawFn(Box<MirFnTy>),
    /// Raw C pointer — only present inside @extern(C) function bodies.
    RawPtr { is_const: bool, inner: Box<MirTy> },
    /// `void` (return only) and `*void`.
    CVoid,
    CChar,
    /// `size_t` / `ssize_t` — alias `u64` / `i64` on 64-bit targets but
    /// kept distinct here so FFI layouts are unambiguous.
    Size,
    SSize,
    /// Pre-monomorph type variable. Eliminated by `monomorphize`.
    TypeVar(Symbol),
    /// SIMD vector — matches `ilang_ast::Type::Simd`. Element kind
    /// is one of `F32/F64/I8/I16/I32/I64`; `lanes` is the lane count
    /// (`F32X4` → `{elem: F32, lanes: 4}`). Construction is via array
    /// literal coercion; values flow through cranelift as the
    /// matching `F32X4` / `I32X4` etc. type.
    Simd { elem: SimdElem, lanes: u32 },
    /// Runtime type handle returned by `typeof(x)`. At ABI level
    /// it's a plain i64 (the dynamic class / enum / primitive id);
    /// the dedicated MirTy variant lets the field / method access
    /// lowering recognise it even when the value has been bound to
    /// a local (`let t = typeof(x); t.name`) so we can route
    /// `.name` / `.kind` / `.fields` / `.methods` / `.parent` /
    /// `.typeArgs` and the per-member lookup methods through their
    /// runtime builtins.
    TypeHandle,
}

/// MIR-side lane element type for `MirTy::Simd`. Mirrors
/// `ilang_ast::SimdElem` 1:1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SimdElem {
    F32,
    F64,
    I8,
    I16,
    I32,
    I64,
}

impl SimdElem {
    /// Single-letter prefix used in the printed form (`f32x4`,
    /// `i32x4`, ...). Mirrors `ilang_ast::SimdElem::name_prefix`.
    pub fn name_prefix(self) -> &'static str {
        match self {
            SimdElem::F32 => "f32",
            SimdElem::F64 => "f64",
            SimdElem::I8 => "i8",
            SimdElem::I16 => "i16",
            SimdElem::I32 => "i32",
            SimdElem::I64 => "i64",
        }
    }
    /// Lane width in bytes — `lanes * lane_bytes()` is the total
    /// vector byte size.
    pub fn lane_bytes(self) -> i64 {
        match self {
            SimdElem::I8 => 1,
            SimdElem::I16 => 2,
            SimdElem::I32 | SimdElem::F32 => 4,
            SimdElem::I64 | SimdElem::F64 => 8,
        }
    }
    /// Equivalent scalar `MirTy` for the lane (used when lowering
    /// per-element loads / coercions).
    pub fn as_scalar_mir(self) -> MirTy {
        match self {
            SimdElem::F32 => MirTy::F32,
            SimdElem::F64 => MirTy::F64,
            SimdElem::I8 => MirTy::I8,
            SimdElem::I16 => MirTy::I16,
            SimdElem::I32 => MirTy::I32,
            SimdElem::I64 => MirTy::I64,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MirFnTy {
    pub params: Box<[MirTy]>,
    pub ret: MirTy,
}

/// Index into `Program::classes`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ClassId(pub u32);

/// Index into `Program::enums`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EnumId(pub u32);

impl MirTy {
    pub fn is_signed_int(&self) -> bool {
        matches!(
            self,
            MirTy::I8 | MirTy::I16 | MirTy::I32 | MirTy::I64 | MirTy::SSize
        )
    }
    pub fn is_unsigned_int(&self) -> bool {
        matches!(
            self,
            MirTy::U8 | MirTy::U16 | MirTy::U32 | MirTy::U64 | MirTy::Size
        )
    }
    pub fn is_int(&self) -> bool {
        self.is_signed_int() || self.is_unsigned_int()
    }
    pub fn is_float(&self) -> bool {
        matches!(self, MirTy::F32 | MirTy::F64)
    }
    pub fn is_numeric(&self) -> bool {
        self.is_int() || self.is_float()
    }
    pub fn int_width(&self) -> u32 {
        match self {
            MirTy::I8 | MirTy::U8 => 8,
            MirTy::I16 | MirTy::U16 => 16,
            MirTy::I32 | MirTy::U32 => 32,
            MirTy::I64 | MirTy::U64 | MirTy::Size | MirTy::SSize => 64,
            _ => 0,
        }
    }
    pub fn is_heap(&self) -> bool {
        matches!(
            self,
            MirTy::Str
                | MirTy::Object(_)
                | MirTy::Weak(_)
                | MirTy::Enum(_)
                | MirTy::Array { .. }
                | MirTy::Tuple(_)
                | MirTy::Optional(_)
                | MirTy::Map { .. }
                | MirTy::Set { .. }
                | MirTy::Promise(_)
                | MirTy::Fn(_)
        )
    }
    /// Float-kind tag for the runtime closure-call ABI: 0 = integer /
    /// pointer cell, 1 = `f32`, 2 = `f64`. The higher-order array
    /// helpers use this to call a closure through an ABI that matches
    /// its float parameter / return type instead of the integer path.
    pub fn float_kind(&self) -> i64 {
        match self {
            MirTy::F32 => 1,
            MirTy::F64 => 2,
            _ => 0,
        }
    }
    /// Per-element byte stride when this type is stored in a packed
    /// array cell. Small numeric types pack tightly (1/2/4 bytes);
    /// SIMD vectors pack as `lanes × lane_bytes`; everything else uses
    /// an 8-byte cell. Keep in sync with the codegen load/store paths.
    pub fn elem_byte_stride(&self) -> i64 {
        match self {
            MirTy::I8 | MirTy::U8 | MirTy::CChar | MirTy::Bool => 1,
            MirTy::I16 | MirTy::U16 => 2,
            MirTy::I32 | MirTy::U32 | MirTy::F32 => 4,
            MirTy::Simd { elem, lanes } => elem.lane_bytes() * (*lanes as i64),
            _ => 8,
        }
    }
}

impl std::fmt::Display for MirTy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MirTy::I8 => write!(f, "i8"),
            MirTy::I16 => write!(f, "i16"),
            MirTy::I32 => write!(f, "i32"),
            MirTy::I64 => write!(f, "i64"),
            MirTy::U8 => write!(f, "u8"),
            MirTy::U16 => write!(f, "u16"),
            MirTy::U32 => write!(f, "u32"),
            MirTy::U64 => write!(f, "u64"),
            MirTy::F32 => write!(f, "f32"),
            MirTy::F64 => write!(f, "f64"),
            MirTy::Bool => write!(f, "bool"),
            MirTy::Str => write!(f, "string"),
            MirTy::Unit => write!(f, "()"),
            MirTy::Object(c) => write!(f, "obj#{}", c.0),
            MirTy::Weak(c) => write!(f, "weak#{}", c.0),
            MirTy::Enum(e) => write!(f, "enum#{}", e.0),
            MirTy::CReprEnum(e) => write!(f, "crepr_enum#{}", e.0),
            MirTy::Array { elem, len: None } => write!(f, "{elem}[]"),
            MirTy::Array { elem, len: Some(n) } => write!(f, "{elem}[{n}]"),
            MirTy::Tuple(elems) => {
                write!(f, "(")?;
                for (i, t) in elems.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{t}")?;
                }
                write!(f, ")")
            }
            MirTy::Optional(inner) => write!(f, "{inner}?"),
            MirTy::Map { key, val } => write!(f, "Map<{key}, {val}>"),
            MirTy::Set { elem } => write!(f, "Set<{elem}>"),
            MirTy::Promise(inner) => write!(f, "Promise<{inner}>"),
            MirTy::Fn(ft) => {
                write!(f, "fn(")?;
                for (i, p) in ft.params.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{p}")?;
                }
                if matches!(ft.ret, MirTy::Unit) {
                    write!(f, ")")
                } else {
                    write!(f, "): {}", ft.ret)
                }
            }
            MirTy::RawFn(ft) => {
                write!(f, "rawfn(")?;
                for (i, p) in ft.params.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{p}")?;
                }
                if matches!(ft.ret, MirTy::Unit) {
                    write!(f, ")")
                } else {
                    write!(f, "): {}", ft.ret)
                }
            }
            MirTy::RawPtr { is_const: true, inner } => write!(f, "*const {inner}"),
            MirTy::RawPtr { is_const: false, inner } => write!(f, "*{inner}"),
            MirTy::CVoid => write!(f, "void"),
            MirTy::CChar => write!(f, "char"),
            MirTy::Size => write!(f, "size_t"),
            MirTy::SSize => write!(f, "ssize_t"),
            MirTy::TypeVar(sym) => write!(f, "${sym}"),
            MirTy::Simd { elem, lanes } => {
                let p = match elem {
                    SimdElem::F32 => "f32",
                    SimdElem::F64 => "f64",
                    SimdElem::I8 => "i8",
                    SimdElem::I16 => "i16",
                    SimdElem::I32 => "i32",
                    SimdElem::I64 => "i64",
                };
                write!(f, "simd.{p}x{lanes}")
            }
            MirTy::TypeHandle => write!(f, "Type"),
        }
    }
}
