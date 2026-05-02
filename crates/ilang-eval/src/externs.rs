//! Built-in `@extern fn` host implementations. The interpreter
//! routes any extern fn call through `invoke_extern(name, args)`,
//! which looks up the name and runs the corresponding Rust function.
//!
//! Names match the qualified form produced by the loader (e.g.
//! `math.sin`), so collisions across modules are impossible.

use crate::value::Value;

/// Try to invoke an extern fn by qualified name. Returns `None` if
/// the name isn't registered (caller surfaces this as a runtime
/// error — it usually means the extern attribute was on a fn the
/// runtime doesn't know about, which is a stdlib/runtime mismatch).
pub fn invoke_extern(name: &str, args: &[Value]) -> Option<Value> {
    match name {
        // math — single-arg f64 → f64
        "math.sin" => Some(Value::Float(f1(args)?.sin())),
        "math.cos" => Some(Value::Float(f1(args)?.cos())),
        "math.tan" => Some(Value::Float(f1(args)?.tan())),
        "math.asin" => Some(Value::Float(f1(args)?.asin())),
        "math.acos" => Some(Value::Float(f1(args)?.acos())),
        "math.atan" => Some(Value::Float(f1(args)?.atan())),
        "math.sqrt" => Some(Value::Float(f1(args)?.sqrt())),
        "math.exp" => Some(Value::Float(f1(args)?.exp())),
        "math.ln" => Some(Value::Float(f1(args)?.ln())),
        "math.log10" => Some(Value::Float(f1(args)?.log10())),
        "math.log2" => Some(Value::Float(f1(args)?.log2())),
        "math.floor" => Some(Value::Float(f1(args)?.floor())),
        "math.ceil" => Some(Value::Float(f1(args)?.ceil())),
        "math.round" => Some(Value::Float(f1(args)?.round())),
        "math.abs" => Some(Value::Float(f1(args)?.abs())),
        // math — two-arg f64
        "math.atan2" => {
            let (y, x) = f2(args)?;
            Some(Value::Float(y.atan2(x)))
        }
        "math.pow" => {
            let (base, exp) = f2(args)?;
            Some(Value::Float(base.powf(exp)))
        }
        _ => None,
    }
}

fn f1(args: &[Value]) -> Option<f64> {
    if args.len() != 1 {
        return None;
    }
    as_f64(&args[0])
}

fn f2(args: &[Value]) -> Option<(f64, f64)> {
    if args.len() != 2 {
        return None;
    }
    Some((as_f64(&args[0])?, as_f64(&args[1])?))
}

fn as_f64(v: &Value) -> Option<f64> {
    match v {
        Value::Float(x) => Some(*x),
        Value::Float32(x) => Some(*x as f64),
        // Allow integer args to coerce — convenient for `math.sqrt(16)`
        // even though the declared signature is f64. The type checker
        // already accepts this via the int-literal-to-float rule.
        Value::Int(n) => Some(*n as f64),
        Value::Int8(n) => Some(*n as f64),
        Value::Int16(n) => Some(*n as f64),
        Value::Int32(n) => Some(*n as f64),
        Value::UInt8(n) => Some(*n as f64),
        Value::UInt16(n) => Some(*n as f64),
        Value::UInt32(n) => Some(*n as f64),
        Value::UInt64(n) => Some(*n as f64),
        _ => None,
    }
}

/// Names that the interpreter / JIT recognize as extern handlers.
/// Useful for the JIT side which needs to register Rust function
/// pointers during JIT module construction.
pub fn known_extern_names() -> &'static [&'static str] {
    &[
        "math.sin", "math.cos", "math.tan", "math.asin", "math.acos", "math.atan",
        "math.atan2", "math.sqrt", "math.pow", "math.exp", "math.ln", "math.log10",
        "math.log2", "math.floor", "math.ceil", "math.round", "math.abs",
    ]
}
