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

#[derive(Debug, Clone)]
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
    /// `T.weak` — non-owning reference. `.get()` upgrades to `Some(obj)`
    /// if alive, `None` otherwise.
    Weak(std::rc::Weak<std::cell::RefCell<ObjectData>>),
    /// User-defined enum value. The payload kind matches the variant's
    /// declaration: Unit / positional Tuple / named Struct.
    Enum {
        ty: String,
        variant: String,
        payload: EnumPayload,
    },
    /// First-class function value. Wraps a `FnDecl` (named or
    /// anonymous) plus an optional captured environment. The
    /// environment is a snapshot of every free variable in the body
    /// at the moment the closure was created (capture-by-value:
    /// later mutations to the outer binding aren't visible here).
    /// Cheap to clone — both Rcs.
    Fn(Rc<ilang_ast::FnDecl>, Rc<HashMap<String, Value>>),
    /// Built-in `Map<K, V>`. Keys are restricted to hashable
    /// primitives (string / int / bool) at the type-checker level.
    /// Wrapped in `Rc<RefCell>` so passing/cloning is cheap and
    /// mutations through one binding are visible to all aliases.
    Map(Rc<RefCell<std::collections::HashMap<MapKey, Value>>>),
}

/// Hashable wrapper for the subset of `Value`s that can serve as
/// `Map` keys. Construction is fallible (`MapKey::from_value`) — only
/// strings, integers (signed widened to i64, unsigned to u64), and
/// booleans are accepted. Float keys are intentionally rejected (NaN
/// breaks `Eq`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum MapKey {
    Str(Rc<String>),
    Int(i64),
    UInt(u64),
    Bool(bool),
}

impl MapKey {
    pub fn from_value(v: &Value) -> Option<Self> {
        Some(match v {
            Value::Str(s) => MapKey::Str(s.clone()),
            Value::Int8(n) => MapKey::Int(*n as i64),
            Value::Int16(n) => MapKey::Int(*n as i64),
            Value::Int32(n) => MapKey::Int(*n as i64),
            Value::Int(n) => MapKey::Int(*n),
            Value::UInt8(n) => MapKey::UInt(*n as u64),
            Value::UInt16(n) => MapKey::UInt(*n as u64),
            Value::UInt32(n) => MapKey::UInt(*n as u64),
            Value::UInt64(n) => MapKey::UInt(*n),
            Value::Bool(b) => MapKey::Bool(*b),
            _ => return None,
        })
    }

    pub fn into_value(self) -> Value {
        match self {
            MapKey::Str(s) => Value::Str(s),
            MapKey::Int(n) => Value::Int(n),
            MapKey::UInt(n) => Value::UInt64(n),
            MapKey::Bool(b) => Value::Bool(b),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum EnumPayload {
    Unit,
    Tuple(Vec<Value>),
    Struct(HashMap<String, Value>),
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        use Value::*;
        match (self, other) {
            (Int8(a), Int8(b)) => a == b,
            (Int16(a), Int16(b)) => a == b,
            (Int32(a), Int32(b)) => a == b,
            (Int(a), Int(b)) => a == b,
            (UInt8(a), UInt8(b)) => a == b,
            (UInt16(a), UInt16(b)) => a == b,
            (UInt32(a), UInt32(b)) => a == b,
            (UInt64(a), UInt64(b)) => a == b,
            (Float32(a), Float32(b)) => a == b,
            (Float(a), Float(b)) => a == b,
            (Bool(a), Bool(b)) => a == b,
            (Str(a), Str(b)) => a == b,
            (Array(a), Array(b)) => a == b,
            (Unit, Unit) => true,
            (Object(a), Object(b)) => Rc::ptr_eq(a, b),
            (None, None) => true,
            (Some(a), Some(b)) => a == b,
            (Weak(a), Weak(b)) => std::rc::Weak::ptr_eq(a, b),
            (
                Enum {
                    ty: ta,
                    variant: va,
                    payload: pa,
                },
                Enum {
                    ty: tb,
                    variant: vb,
                    payload: pb,
                },
            ) => ta == tb && va == vb && pa == pb,
            _ => false,
        }
    }
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
            Value::Weak(w) => match w.upgrade() {
                Some(_) => write!(f, "weak(<alive>)"),
                None => write!(f, "weak(<dead>)"),
            },
            Value::Enum { ty, variant, payload } => match payload {
                EnumPayload::Unit => write!(f, "{ty}::{variant}"),
                EnumPayload::Tuple(items) => {
                    write!(f, "{ty}::{variant}(")?;
                    for (i, v) in items.iter().enumerate() {
                        if i > 0 {
                            write!(f, ", ")?;
                        }
                        write!(f, "{v}")?;
                    }
                    write!(f, ")")
                }
                EnumPayload::Struct(fields) => {
                    write!(f, "{ty}::{variant} {{")?;
                    let mut keys: Vec<&String> = fields.keys().collect();
                    keys.sort();
                    let mut first = true;
                    for k in keys {
                        if !first {
                            write!(f, ",")?;
                        }
                        first = false;
                        write!(f, " {}: {}", k, fields[k])?;
                    }
                    if !first {
                        write!(f, " ")?;
                    }
                    write!(f, "}}")
                }
            },
            Value::Fn(decl, _captures) => {
                if decl.name.is_empty() {
                    write!(f, "<fn>")
                } else {
                    write!(f, "<fn {}>", decl.name)
                }
            }
            Value::Map(m) => {
                let m = m.borrow();
                write!(f, "{{")?;
                let mut keys: Vec<&MapKey> = m.keys().collect();
                // Stable display order so test expectations don't depend
                // on hashmap iteration randomness.
                keys.sort_by_key(|k| format!("{}", (*k).clone().into_value()));
                let mut first = true;
                for k in keys {
                    if !first {
                        write!(f, ", ")?;
                    }
                    first = false;
                    let kv = k.clone().into_value();
                    write!(f, "{kv}: {}", m[k])?;
                }
                write!(f, "}}")
            }
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
