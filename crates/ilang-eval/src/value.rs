use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// Heap-allocated object data shared via `Rc` (ARC for our single-threaded
/// interpreter). Field map is mutable through `RefCell`.
#[derive(Debug, PartialEq)]
pub struct ObjectData {
    pub class: String,
    pub fields: HashMap<String, Value>,
}

pub type ObjectRef = Rc<RefCell<ObjectData>>;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int32(i32),
    Int(i64),
    Float32(f32),
    Float(f64),
    Bool(bool),
    Unit,
    Object(ObjectRef),
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Int32(n) => write!(f, "{n}"),
            Value::Int(n) => write!(f, "{n}"),
            Value::Float32(x) => {
                if x.is_finite() && x.fract() == 0.0 {
                    write!(f, "{x:.1}")
                } else {
                    write!(f, "{x}")
                }
            }
            Value::Float(x) => {
                if x.is_finite() && x.fract() == 0.0 {
                    write!(f, "{x:.1}")
                } else {
                    write!(f, "{x}")
                }
            }
            Value::Bool(b) => write!(f, "{b}"),
            Value::Unit => write!(f, "()"),
            Value::Object(o) => {
                let o = o.borrow();
                write!(f, "{} {{", o.class)?;
                let mut first = true;
                // HashMap iteration order is not stable; sort for predictable
                // output in tests and the REPL.
                let mut keys: Vec<&String> = o.fields.keys().collect();
                keys.sort();
                for k in keys {
                    if !first {
                        write!(f, ",")?;
                    }
                    first = false;
                    write!(f, " {}: {}", k, o.fields[k])?;
                }
                if !first {
                    write!(f, " ")?;
                }
                write!(f, "}}")
            }
        }
    }
}
