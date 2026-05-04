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
    Object(String),
    /// Instance of a user-defined generic class with concrete type
    /// arguments (e.g. `Box<i64>`). Non-generic classes use `Object`.
    Generic { base: String, args: Vec<Type> },
    /// Reference to a type parameter inside a generic class body
    /// (e.g. `T` in `class Box<T> { x: T }`). Replaced with concrete
    /// types via substitution when the class is instantiated.
    TypeVar(String),
    /// Function value type — `fn(T1, T2): R`. Carries no captured
    /// state (no closures yet); at runtime it's a code pointer.
    Fn { params: Vec<Type>, ret: Box<Type> },
    /// Value of a user-defined `enum`, identified by name. The set of
    /// variants and their payloads live in the type checker's enum
    /// signature table.
    Enum(String),
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
    Tuple(Vec<Type>),
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
    /// `out<T>` — out-pointer parameter on an `@extern` fn. The C
    /// side declares `*mut T`; the JIT allocates a stack slot at
    /// the call site, passes its address as the actual arg, and
    /// after the call surfaces the written value as an extra
    /// return component (a tuple if the fn already had a non-Unit
    /// return). Only valid as a parameter type on extern fns; T
    /// must be a numeric primitive or bool.
    Out(Box<Type>),
}

impl Type {
    pub fn is_signed_int(&self) -> bool {
        matches!(self, Type::I8 | Type::I16 | Type::I32 | Type::I64)
    }
    pub fn is_unsigned_int(&self) -> bool {
        matches!(self, Type::U8 | Type::U16 | Type::U32 | Type::U64)
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
            Type::I64 | Type::U64 => 64,
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
            Type::Generic { base, args } => {
                write!(f, "{base}<")?;
                for (i, a) in args.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{a}")?;
                }
                write!(f, ">")
            }
            Type::TypeVar(name) => write!(f, "{name}"),
            Type::Fn { params, ret } => {
                write!(f, "fn(")?;
                for (i, p) in params.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{p}")?;
                }
                if matches!(ret.as_ref(), Type::Unit) {
                    write!(f, ")")
                } else {
                    write!(f, "): {ret}")
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
            Type::Out(inner) => write!(f, "out<{inner}>"),
        }
    }
}
