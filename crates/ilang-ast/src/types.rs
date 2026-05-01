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
    /// Internal-only type used by built-in signatures (e.g. `console.log`)
    /// that accept any value. The parser does not produce it; user code
    /// cannot annotate a binding with it.
    Any,
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
            Type::Any => write!(f, "any"),
        }
    }
}
