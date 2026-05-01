#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type {
    I32,
    I64,
    F32,
    F64,
    Bool,
    Unit,
    /// Instance of a user-defined class, identified by class name.
    Object(String),
    /// Internal-only type used by built-in signatures (e.g. `console.log`)
    /// that accept any value. The parser does not produce it; user code
    /// cannot annotate a binding with it.
    Any,
}

impl std::fmt::Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Type::I32 => write!(f, "i32"),
            Type::I64 => write!(f, "i64"),
            Type::F32 => write!(f, "f32"),
            Type::F64 => write!(f, "f64"),
            Type::Bool => write!(f, "bool"),
            Type::Unit => write!(f, "()"),
            Type::Object(name) => write!(f, "{name}"),
            Type::Any => write!(f, "any"),
        }
    }
}
