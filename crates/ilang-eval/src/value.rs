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
    Int8(i8),
    Int16(i16),
    Int32(i32),
    Int(i64),
    UInt8(u8),
    UInt16(u16),
    UInt32(u32),
    UInt64(u64),
    Float32(f32),
    Float(f64),
    Bool(bool),
    /// Immutable UTF-8 string. Wrapped in `Rc` so passing/cloning is cheap.
    Str(Rc<String>),
    /// Array shared via `Rc<RefCell<...>>` (ARC). Element vector is
    /// mutable in place — mutation through one binding is visible to all
    /// aliases, matching the JS array model.
    Array(Rc<RefCell<Vec<Value>>>),
    Unit,
    Object(ObjectRef),
    /// `T?` — `None` is the absent state, `Some(v)` wraps a present value.
    /// The static type information needed to distinguish "string?-none"
    /// from "i64?-none" lives in the type checker; the runtime treats
    /// `None` uniformly.
    None,
    Some(Box<Value>),
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Int8(n) => write!(f, "{n}"),
            Value::Int16(n) => write!(f, "{n}"),
            Value::Int32(n) => write!(f, "{n}"),
            Value::Int(n) => write!(f, "{n}"),
            Value::UInt8(n) => write!(f, "{n}"),
            Value::UInt16(n) => write!(f, "{n}"),
            Value::UInt32(n) => write!(f, "{n}"),
            Value::UInt64(n) => write!(f, "{n}"),
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
            Value::Str(s) => write!(f, "{s}"),
            Value::Array(arr) => {
                let arr = arr.borrow();
                write!(f, "[")?;
                for (i, v) in arr.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{v}")?;
                }
                write!(f, "]")
            }
            Value::Unit => write!(f, "()"),
            Value::None => write!(f, "none"),
            Value::Some(v) => write!(f, "some({v})"),
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
