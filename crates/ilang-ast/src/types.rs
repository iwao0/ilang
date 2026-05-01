#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Type {
    I64,
    F64,
    Bool,
    Unit,
}

impl std::fmt::Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Type::I64 => write!(f, "i64"),
            Type::F64 => write!(f, "f64"),
            Type::Bool => write!(f, "bool"),
            Type::Unit => write!(f, "()"),
        }
    }
}
