//! Hover signatures for built-in members the type checker
//! pre-registers (FFI helpers, string / array methods).

use ilang_ast::Type;

/// Hover signatures for the FFI marshalling helpers callable inside
/// `@extern(C) {}` blocks. The type checker pre-registers these but
/// the buffer doesn't declare them, so users would otherwise see no
/// hover.
/// Return type of an FFI marshalling helper as a structured `Type`.
/// Used by the LSP's `infer_expr` so that `let p = cstrFromString(s)`
/// hovers as `let p: *char` instead of falling off the lookup. Mirrors
/// the signature strings below; keep the two in sync.
pub(crate) fn ffi_helper_return_type(name: &str) -> Option<Type> {
    use ilang_ast::Symbol;
    let raw_char = || Type::RawPtr {
        is_const: false,
        inner: Box::new(Type::CChar),
    };
    let raw_const_char = || Type::RawPtr {
        is_const: true,
        inner: Box::new(Type::CChar),
    };
    let _ = Symbol::intern; // imported for clarity, may be unused
    Some(match name {
        "stringFromCstr" => Type::Str,
        "cstrFromString" => raw_char(),
        "freeCstr" => Type::Unit,
        "bytesFromBuffer" => Type::Array {
            elem: Box::new(Type::U8),
            fixed: None,
        },
        "readI8" => Type::I8,
        "readI16" => Type::I16,
        "readI32" => Type::I32,
        "readI64" => Type::I64,
        "readU8" => Type::U8,
        "readU16" => Type::U16,
        "readU32" => Type::U32,
        "readU64" => Type::U64,
        "readF32" => Type::F32,
        "readF64" => Type::F64,
        "writeI8" | "writeI16" | "writeI32" | "writeI64"
        | "writeU8" | "writeU16" | "writeU32" | "writeU64"
        | "writeF32" | "writeF64" => Type::Unit,
        "fnAddr" => Type::I64,
        "cstrArrayToStrings" => Type::Array {
            elem: Box::new(Type::Str),
            fixed: None,
        },
        "errnoCheck" => Type::Optional(Box::new(Type::I32)),
        "errnoCheckI64" => Type::Optional(Box::new(Type::I64)),
        // `arrayFromCArray<T>(p, n): T[]` — generic, no concrete
        // return without resolving T from the arg's pointer type.
        // Leave None for now; hover on this one stays untyped.
        _ => {
            let _ = raw_const_char;
            return None;
        }
    })
}

pub(crate) fn ffi_helper_signature(name: &str) -> Option<&'static str> {
    Some(match name {
        "stringFromCstr" => "fn stringFromCstr(p: *const char): string",
        "cstrFromString" => "fn cstrFromString(s: string): *char",
        "freeCstr" => "fn freeCstr(p: *char)",
        "bytesFromBuffer" => "fn bytesFromBuffer(p: *const void, n: size_t): u8[]",
        "readI8" => "fn readI8(p: *const void, offset: i64): i8",
        "readI16" => "fn readI16(p: *const void, offset: i64): i16",
        "readI32" => "fn readI32(p: *const void, offset: i64): i32",
        "readI64" => "fn readI64(p: *const void, offset: i64): i64",
        "readU8" => "fn readU8(p: *const void, offset: i64): u8",
        "readU16" => "fn readU16(p: *const void, offset: i64): u16",
        "readU32" => "fn readU32(p: *const void, offset: i64): u32",
        "readU64" => "fn readU64(p: *const void, offset: i64): u64",
        "readF32" => "fn readF32(p: *const void, offset: i64): f32",
        "readF64" => "fn readF64(p: *const void, offset: i64): f64",
        "writeI8" => "fn writeI8(p: *void, offset: i64, value: i8)",
        "writeI16" => "fn writeI16(p: *void, offset: i64, value: i16)",
        "writeI32" => "fn writeI32(p: *void, offset: i64, value: i32)",
        "writeI64" => "fn writeI64(p: *void, offset: i64, value: i64)",
        "writeU8" => "fn writeU8(p: *void, offset: i64, value: u8)",
        "writeU16" => "fn writeU16(p: *void, offset: i64, value: u16)",
        "writeU32" => "fn writeU32(p: *void, offset: i64, value: u32)",
        "writeU64" => "fn writeU64(p: *void, offset: i64, value: u64)",
        "writeF32" => "fn writeF32(p: *void, offset: i64, value: f32)",
        "writeF64" => "fn writeF64(p: *void, offset: i64, value: f64)",
        "fnAddr" => "fn fnAddr<F>(f: F): i64",
        "arrayFromCArray" => "fn arrayFromCArray<T>(p: *const T, n: size_t): T[]",
        "cstrArrayToStrings" => "fn cstrArrayToStrings(p: *const *const char): string[]",
        "errnoCheck" => "fn errnoCheck(rc: i32): i32?",
        "errnoCheckI64" => "fn errnoCheckI64(rc: i64): i64?",
        _ => return None,
    })
}

pub(crate) fn string_method_names() -> &'static [&'static str] {
    &[
        "charAt",
        "includes",
        "startsWith",
        "endsWith",
        "toUpper",
        "toLower",
        "trim",
        "replace",
        "split",
        "slice",
    ]
}

pub(crate) fn array_method_names() -> &'static [&'static str] {
    &[
        "push", "pop", "indexOf", "includes", "slice", "map", "filter", "forEach",
    ]
}

pub(crate) fn string_method_sig(method: &str) -> Option<String> {
    let body = match method {
        "charAt" => "charAt(i: i64): string",
        "includes" => "includes(needle: string): bool",
        "startsWith" => "startsWith(prefix: string): bool",
        "endsWith" => "endsWith(suffix: string): bool",
        "toUpper" => "toUpper(): string",
        "toLower" => "toLower(): string",
        "trim" => "trim(): string",
        "replace" => "replace(from: string, to: string): string",
        "split" => "split(sep: string): string[]",
        "slice" => "slice(start: i64, end: i64): string",
        _ => return None,
    };
    Some(format!("(method) string.{body}"))
}

/// Hover documentation for the built-in `string` methods. Keep
/// each entry short and concrete — the hover popup is a few lines
/// at most.
pub(crate) fn string_method_doc(method: &str) -> Option<&'static str> {
    Some(match method {
        "charAt" => "Returns the 1-character substring at byte offset `i`. Out-of-range indices return an empty string.",
        "includes" => "Returns `true` when `needle` occurs anywhere in this string.",
        "startsWith" => "Returns `true` when this string begins with `prefix`.",
        "endsWith" => "Returns `true` when this string ends with `suffix`.",
        "toUpper" => "Returns a new string with every ASCII letter upper-cased. Non-ASCII bytes pass through unchanged.",
        "toLower" => "Returns a new string with every ASCII letter lower-cased. Non-ASCII bytes pass through unchanged.",
        "trim" => "Returns a new string with leading and trailing ASCII whitespace removed.",
        "replace" => "Returns a new string with every occurrence of `from` replaced by `to`. Non-overlapping, left-to-right.",
        "split" => "Splits this string on every occurrence of `sep`. Empty `sep` yields each byte as a 1-char element.",
        "slice" => "Returns the substring covering byte offsets `[start, end)`. Indices are clamped to the string's length.",
        _ => return None,
    })
}

pub(crate) fn array_method_sig(method: &str, elem: &Type) -> Option<String> {
    let body = match method {
        "push" => format!("push(v: {elem}): ()"),
        "pop" => format!("pop(): {elem}?"),
        "indexOf" => format!("indexOf(v: {elem}): i64"),
        "includes" => format!("includes(v: {elem}): bool"),
        "slice" => format!("slice(start: i64, end: i64): {elem}[]"),
        "map" => format!("map<U>(f: fn({elem}): U): U[]"),
        "filter" => format!("filter(pred: fn({elem}): bool): {elem}[]"),
        "forEach" => format!("forEach(f: fn({elem}): ()): ()"),
        _ => return None,
    };
    Some(format!("(method) {elem}[].{body}"))
}

/// Hover documentation for the built-in array methods.
pub(crate) fn array_method_doc(method: &str) -> Option<&'static str> {
    Some(match method {
        "push" => "Appends `v` to the end of the array. Mutates the receiver.",
        "pop" => "Removes and returns the last element, or `none` when the array is empty.",
        "indexOf" => "Returns the index of the first element equal to `v`, or `-1` when no element matches.",
        "includes" => "Returns `true` when any element equals `v`.",
        "slice" => "Returns a new array covering indices `[start, end)`. Indices are clamped to the array's length.",
        "map" => "Returns a new array of `f(elem)` for each element, in order.",
        "filter" => "Returns a new array of every element for which `pred(elem)` returns `true`.",
        "forEach" => "Invokes `f` on each element in order. Returns nothing.",
        _ => return None,
    })
}
