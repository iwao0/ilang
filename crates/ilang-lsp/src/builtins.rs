//! Hover signatures for built-in members the type checker
//! pre-registers (FFI helpers, string / array methods).

use ilang_ast::Type;

/// Hover signatures for the FFI marshalling helpers callable inside
/// `@extern(C) {}` blocks. The type checker pre-registers these but
/// the buffer doesn't declare them, so users would otherwise see no
/// hover.
pub(crate) fn ffi_helper_signature(name: &str) -> Option<&'static str> {
    Some(match name {
        "stringFromCstr" => "fn stringFromCstr(p: *const char): string",
        "cstrFromString" => "fn cstrFromString(s: string): *char",
        "freeCstr" => "fn freeCstr(p: *char)",
        "bytesFromBuffer" => "fn bytesFromBuffer(p: *const void, n: size_t): u8[]",
        "readU8" => "fn readU8(p: *const void, offset: i64): u8",
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
