use crate::intern::Symbol;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type {
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
    /// Immutable UTF-8 string. Stored at runtime as `Rc<String>`.
    Str,
    Unit,
    /// Instance of a user-defined class, identified by class name.
    Object(Symbol),
    /// Instance of a user-defined generic class with concrete type
    /// arguments (e.g. `Box<i64>`). Non-generic classes use `Object`.
    /// Boxed because `GenericTy` is significantly larger than the
    /// remaining variants — keeps the enum compact.
    Generic(Box<GenericTy>),
    /// Reference to a type parameter inside a generic class body
    /// (e.g. `T` in `class Box<T> { x: T }`). Replaced with concrete
    /// types via substitution when the class is instantiated.
    TypeVar(Symbol),
    /// Function value type — `fn(T1, T2): R`. Carries no captured
    /// state (no closures yet); at runtime it's a code pointer.
    /// Boxed for the same reason as `Generic` — keeps the enum size
    /// down to the width of the smaller variants.
    Fn(Box<FnTy>),
    /// Value of a user-defined `enum`, identified by name. The set of
    /// variants and their payloads live in the type checker's enum
    /// signature table.
    Enum(Symbol),
    /// Array of `elem`. `fixed = Some(n)` is a fixed-length array of
    /// exactly n elements; `fixed = None` is a growable array (`push` is
    /// allowed only on the latter). Both share the same runtime layout.
    Array {
        elem: Box<Type>,
        fixed: Option<usize>,
    },
    /// Anonymous product type `(T1, T2, ...)`. Always 2+ elements
    /// (`(T)` parses as grouping; `()` is `Unit`). Heterogeneous —
    /// indexing requires a constant integer literal.
    Tuple(Box<[Type]>),
    /// `T?` — value that may be present (`some(v)`) or absent (`none`).
    /// Construction auto-wraps a `T` in any context expecting a `T?`.
    Optional(Box<Type>),
    /// `T.weak` — non-owning reference to a class instance. Doesn't
    /// retain the object; `.get()` returns `T?` (some if alive, none
    /// if all strong refs are gone). Inner is restricted to `Object`.
    Weak(Box<Type>),
    /// Internal-only type used by built-in signatures (e.g. `console.log`)
    /// that accept any value. The parser does not produce it; user code
    /// cannot annotate a binding with it.
    Any,
    /// Raw C pointer — only nameable inside an `@extern(C) { ... }`
    /// block. `*char`, `*void`, `*const char`, `*i32`, `*MyStruct`,
    /// etc. The bool is `true` for `*const T`, `false` for plain `*T`
    /// (no `*mut` exists; mutability is the default since C lacks the
    /// distinction at the type-system level we model). Values of this
    /// type cannot escape the block — extern fn returns of pointer
    /// type must be wrapped by an in-block helper that converts to
    /// an ilang type.
    RawPtr { is_const: bool, inner: Box<Type> },
    /// `void` — only valid as the inner of `*void` / `*const void`.
    /// Has no values.
    CVoid,
    /// `char` — C `char` type. Inside an `@extern(C)` block, distinct
    /// from `i8` / `u8` to convey C-string-ness. Same ABI as i8.
    CChar,
    /// `size_t` — pointer-width unsigned integer. Aliases `u64` on
    /// 64-bit targets.
    Size,
    /// `ssize_t` — pointer-width signed integer. Aliases `i64` on
    /// 64-bit targets.
    SSize,
    /// SIMD vector type — `simd.f32x4`, `simd.f32x2`, etc. Element
    /// type and lane count describe what cranelift's
    /// `F32X4` / `F32X2` etc. carries; construction is done via
    /// array-literal coercion (`let v: simd.f32x4 = [1, 2, 3, 4]`).
    /// First-class operations (element-wise add etc.) aren't
    /// exposed yet — the type is here so SIMD values can flow
    /// through ObjC binding boundaries that take Apple's
    /// `vector_floatN` types.
    Simd { elem: SimdElem, lanes: u32 },
}

/// Element type for `Type::Simd`. Mirrors the lane-type set
/// cranelift can fit in a single NEON Q / D register on arm64
/// (and equivalent SSE/AVX lane widths on x86). Other widths
/// can be added once a binding actually needs them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimdElem {
    F32,
    F64,
    I8,
    I16,
    I32,
    I64,
}

impl SimdElem {
    /// Single-letter prefix used in the surface name (`f32x4`,
    /// `i32x4`, ...).
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
    /// `Type` corresponding to this element width, for the
    /// element coercion check.
    pub fn as_scalar_type(self) -> Type {
        match self {
            SimdElem::F32 => Type::F32,
            SimdElem::F64 => Type::F64,
            SimdElem::I8 => Type::I8,
            SimdElem::I16 => Type::I16,
            SimdElem::I32 => Type::I32,
            SimdElem::I64 => Type::I64,
        }
    }
}

/// Inner data for `Type::Generic` — kept separate so `Type` can box it
/// and stay small.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenericTy {
    pub base: Symbol,
    pub args: Box<[Type]>,
}

/// Inner data for `Type::Fn` — kept separate for the same reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FnTy {
    pub params: Box<[Type]>,
    pub ret: Type,
}

impl Type {
    /// Convenience constructor for `Type::Generic`.
    pub fn generic(base: impl Into<Symbol>, args: Vec<Type>) -> Self {
        Type::Generic(Box::new(GenericTy { base: base.into(), args: args.into() }))
    }
    /// Convenience constructor for `Type::Fn`.
    pub fn func(params: Vec<Type>, ret: Type) -> Self {
        Type::Fn(Box::new(FnTy { params: params.into(), ret }))
    }
}

impl Type {
    pub fn is_signed_int(&self) -> bool {
        matches!(self, Type::I8 | Type::I16 | Type::I32 | Type::I64 | Type::SSize)
    }
    pub fn is_unsigned_int(&self) -> bool {
        matches!(self, Type::U8 | Type::U16 | Type::U32 | Type::U64 | Type::Size)
    }
    pub fn is_int(&self) -> bool {
        self.is_signed_int() || self.is_unsigned_int()
    }
    pub fn is_float(&self) -> bool {
        matches!(self, Type::F32 | Type::F64)
    }
    pub fn is_numeric(&self) -> bool {
        self.is_int() || self.is_float()
    }
    /// Bit width of an integer type. 0 for non-integers.
    pub fn int_width(&self) -> u32 {
        match self {
            Type::I8 | Type::U8 => 8,
            Type::I16 | Type::U16 => 16,
            Type::I32 | Type::U32 => 32,
            // `size_t` / `ssize_t` alias `u64` / `i64` on 64-bit targets.
            Type::I64 | Type::U64 | Type::Size | Type::SSize => 64,
            _ => 0,
        }
    }
}

impl std::fmt::Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Type::I8 => write!(f, "i8"),
            Type::I16 => write!(f, "i16"),
            Type::I32 => write!(f, "i32"),
            Type::I64 => write!(f, "i64"),
            Type::U8 => write!(f, "u8"),
            Type::U16 => write!(f, "u16"),
            Type::U32 => write!(f, "u32"),
            Type::U64 => write!(f, "u64"),
            Type::F32 => write!(f, "f32"),
            Type::F64 => write!(f, "f64"),
            Type::Str => write!(f, "string"),
            Type::Bool => write!(f, "bool"),
            Type::Unit => write!(f, "()"),
            Type::Object(name) => write!(f, "{name}"),
            Type::Generic(g) => {
                write!(f, "{}<", g.base)?;
                for (i, a) in g.args.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{a}")?;
                }
                write!(f, ">")
            }
            Type::TypeVar(name) => write!(f, "{name}"),
            Type::Fn(ft) => {
                write!(f, "fn(")?;
                for (i, p) in ft.params.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{p}")?;
                }
                if matches!(ft.ret, Type::Unit) {
                    write!(f, ")")
                } else {
                    write!(f, "): {}", ft.ret)
                }
            }
            Type::Enum(name) => write!(f, "{name}"),
            Type::Array { elem, fixed: None } => write!(f, "{elem}[]"),
            Type::Array { elem, fixed: Some(n) } => write!(f, "{elem}[{n}]"),
            Type::Tuple(elems) => {
                write!(f, "(")?;
                for (i, t) in elems.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{t}")?;
                }
                write!(f, ")")
            }
            Type::Optional(inner) => write!(f, "{inner}?"),
            Type::Weak(inner) => write!(f, "{inner}.weak"),
            Type::Any => write!(f, "any"),
            Type::RawPtr { is_const: true, inner } => write!(f, "*const {inner}"),
            Type::RawPtr { is_const: false, inner } => write!(f, "*{inner}"),
            Type::CVoid => write!(f, "void"),
            Type::CChar => write!(f, "char"),
            Type::Size => write!(f, "size_t"),
            Type::SSize => write!(f, "ssize_t"),
            Type::Simd { elem, lanes } => write!(f, "simd.{}x{}", elem.name_prefix(), lanes),
        }
    }
}
