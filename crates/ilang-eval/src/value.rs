#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Value {
    Int(i64),
    Float(f64),
    Bool(bool),
    Unit,
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Int(n) => write!(f, "{n}"),
            Value::Float(x) => {
                if x.is_finite() && x.fract() == 0.0 {
                    write!(f, "{x:.1}")
                } else {
                    write!(f, "{x}")
                }
            }
            Value::Bool(b) => write!(f, "{b}"),
            Value::Unit => write!(f, "()"),
        }
    }
}
