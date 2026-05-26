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
    let raw_char = || Type::RawPtr {
        is_const: false,
        inner: Box::new(Type::CChar),
    };
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
        "cstrArrayToStrings" => Type::Array {
            elem: Box::new(Type::Str),
            fixed: None,
        },
        "errnoCheck" => Type::Optional(Box::new(Type::I32)),
        "errnoCheckI64" => Type::Optional(Box::new(Type::I64)),
        _ => return None,
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
        "cstrArrayToStrings" => "fn cstrArrayToStrings(p: *const *const char): string[]",
        "errnoCheck" => "fn errnoCheck(rc: i32): i32?",
        "errnoCheckI64" => "fn errnoCheckI64(rc: i64): i64?",
        _ => return None,
    })
}

/// Static factories on the `string` primitive type (used by the
/// type-checker's `name == "string"` static-call arm). Driven into
/// completion when the user types `string.` outside any shadowing
/// local — same shape as the `f32.` / `i64.` associated-constant
/// completion paths.
pub(crate) fn string_static_method_names() -> &'static [&'static str] {
    &["fromUtf16"]
}

pub(crate) fn string_static_method_sig(method: &str) -> Option<String> {
    let body = match method {
        "fromUtf16" => "fromUtf16(units: u16[]): string",
        _ => return None,
    };
    Some(format!("(static) string.{body}"))
}

pub(crate) fn string_static_method_doc(method: &str) -> Option<&'static str> {
    Some(match method {
        "fromUtf16" => "Decodes a UTF-16 code-unit buffer into a fresh UTF-8 `string`. The whole `u16[]` is consumed — a trailing `0x0000` is kept as a literal U+0000, so pair with `encodeUtf16(false)` for strict round-trip. Unpaired surrogates are replaced with U+FFFD.",
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
        "concat",
        "indexOf",
        "lastIndexOf",
        "encodeUtf16",
    ]
}

/// Built-in methods shared by every numeric primitive and `bool`,
/// plus the float-only `isFinite` / `isNaN`. `primitive_method_sig`
/// gates the float-only entries on the receiver type so the
/// completion list filters them out for ints / bools.
pub(crate) fn primitive_method_names() -> &'static [&'static str] {
    &["toString", "isFinite", "isNaN"]
}

pub(crate) fn primitive_method_sig(method: &str, ty: &Type) -> Option<String> {
    let is_float = matches!(ty, Type::F32 | Type::F64);
    let body = match method {
        "toString" => "toString(): string".to_string(),
        "isFinite" if is_float => "isFinite(): bool".to_string(),
        "isNaN" if is_float => "isNaN(): bool".to_string(),
        _ => return None,
    };
    Some(format!("(method) {ty}.{body}"))
}

pub(crate) fn primitive_method_doc(method: &str) -> Option<&'static str> {
    Some(match method {
        "toString" => "Returns the value's decimal (`123`) or JS-style float (`1.5`) string. `true` / `false` for `bool`.",
        "isFinite" => "Returns `true` when the value is a finite real number (not NaN, not ±Infinity).",
        "isNaN" => "Returns `true` when the value is IEEE-754 NaN. By definition `NaN != NaN`, so `==` can't be used for this check.",
        _ => return None,
    })
}

pub(crate) fn array_method_names() -> &'static [&'static str] {
    &[
        "push", "pop", "shift", "unshift", "remove", "removeAt",
        "indexOf", "includes", "find", "findIndex", "every", "some",
        "slice", "concat", "reverse", "fill", "sort",
        "map", "filter", "forEach", "join",
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
        "concat" => "concat(other: string): string",
        "indexOf" => "indexOf(needle: string, fromIndex?: i64): i64",
        "lastIndexOf" => "lastIndexOf(needle: string, fromIndex?: i64): i64",
        "encodeUtf16" => "encodeUtf16(nulTerminated: bool = true): u16[]",
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
        "concat" => "Returns a new string formed by appending `other` to this string.",
        "indexOf" => "Returns the code-point index of the first occurrence of `needle` at or after `fromIndex` (default 0). Returns `-1` if not found.",
        "lastIndexOf" => "Returns the code-point index of the last occurrence of `needle` at or before `fromIndex` (default: end of string). Returns `-1` if not found.",
        "encodeUtf16" => "Encodes this string as UTF-16 code units and returns a fresh `u16[]`. With `nulTerminated = true` (the default) the buffer ends with `0x0000` so it can be passed straight to Win32 W-suffix APIs via the implicit `u16[] → *const u16` coercion.",
        _ => return None,
    })
}

pub(crate) fn array_method_sig(method: &str, elem: &Type) -> Option<String> {
    let body = match method {
        "push" => format!("push(v: {elem}): ()"),
        "pop" => format!("pop(): {elem}?"),
        "shift" => format!("shift(): {elem}?"),
        "unshift" => format!("unshift(v: {elem}): ()"),
        "remove" => format!("remove(v: {elem}): bool"),
        "removeAt" => format!("removeAt(i: i64): {elem}?"),
        "indexOf" => format!("indexOf(v: {elem}): i64"),
        "includes" => format!("includes(v: {elem}): bool"),
        "find" => format!("find(pred: fn({elem}): bool): {elem}?"),
        "findIndex" => format!("findIndex(pred: fn({elem}): bool): i64"),
        "every" => format!("every(pred: fn({elem}): bool): bool"),
        "some" => format!("some(pred: fn({elem}): bool): bool"),
        "slice" => format!("slice(start: i64, end: i64): {elem}[]"),
        "concat" => format!("concat(other: {elem}[]): {elem}[]"),
        "reverse" => format!("reverse(): {elem}[]"),
        "fill" => format!("fill(v: {elem}): ()"),
        "sort" => format!("sort(cmp: fn({elem}, {elem}): i64): {elem}[]"),
        "map" => format!("map<U>(f: fn({elem}): U): U[]"),
        "filter" => format!("filter(pred: fn({elem}): bool): {elem}[]"),
        "forEach" => format!("forEach(f: fn({elem}): ()): ()"),
        // `join` is only legal on `string[]` — surface it just for
        // that elem so the completion lines up with the type-checker.
        "join" if matches!(elem, Type::Str) => "join(sep: string): string".to_string(),
        _ => return None,
    };
    Some(format!("(method) {elem}[].{body}"))
}

pub(crate) fn map_method_names() -> &'static [&'static str] {
    &[
        "get", "set", "has", "delete", "size", "keys", "values",
        "clear", "entries", "forEach",
    ]
}

pub(crate) fn map_method_sig(method: &str, k: &Type, v: &Type) -> Option<String> {
    let body = match method {
        "get" => format!("get(key: {k}): {v}?"),
        "set" => format!("set(key: {k}, value: {v}): ()"),
        "has" => format!("has(key: {k}): bool"),
        "delete" => format!("delete(key: {k}): bool"),
        "size" => "size(): i64".to_string(),
        "keys" => format!("keys(): {k}[]"),
        "values" => format!("values(): {v}[]"),
        "clear" => "clear(): ()".to_string(),
        "entries" => format!("entries(): ({k}, {v})[]"),
        "forEach" => format!("forEach(cb: fn({k}, {v}): ()): ()"),
        _ => return None,
    };
    Some(format!("(method) Map<{k}, {v}>.{body}"))
}

/// Associated-constant names exposed on `f32` / `f64`. Matches the
/// set the type checker accepts in `check_field`'s float-prim arm.
pub(crate) fn float_prim_const_names() -> &'static [&'static str] {
    &[
        "NaN", "Infinity", "NegInfinity",
        "Min", "Max", "MinPositive", "Epsilon",
    ]
}

/// Associated-constant names exposed on every signed / unsigned
/// integer (`i8` … `u64`). Just `Min` / `Max` — Rust-style bounds,
/// not JS's "minimum positive" sense of `MIN`.
pub(crate) fn int_prim_const_names() -> &'static [&'static str] {
    &["Min", "Max"]
}

pub(crate) fn int_prim_const_sig(receiver: &str, name: &str) -> Option<String> {
    let recognised_recv = matches!(
        receiver,
        "i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32" | "u64"
    );
    if !recognised_recv {
        return None;
    }
    if !matches!(name, "Min" | "Max") {
        return None;
    }
    Some(format!("(constant) {receiver}.{name}: {receiver}"))
}

pub(crate) fn int_prim_const_doc(name: &str) -> Option<&'static str> {
    Some(match name {
        "Min" => "Smallest representable value (0 for unsigned types, most-negative for signed).",
        "Max" => "Largest representable value.",
        _ => return None,
    })
}

pub(crate) fn float_prim_const_sig(receiver: &str, name: &str) -> Option<String> {
    if !matches!(receiver, "f32" | "f64") {
        return None;
    }
    let recognised = matches!(
        name,
        "NaN" | "Infinity" | "NegInfinity"
            | "Min" | "Max" | "MinPositive" | "Epsilon"
    );
    if !recognised {
        return None;
    }
    Some(format!("(constant) {receiver}.{name}: {receiver}"))
}

pub(crate) fn float_prim_const_doc(name: &str) -> Option<&'static str> {
    Some(match name {
        "NaN" => "IEEE-754 NaN. `NaN == NaN` is `false`.",
        "Infinity" => "Positive infinity (e.g. `1.0 / 0.0`).",
        "NegInfinity" => "Negative infinity (e.g. `-1.0 / 0.0`).",
        "Min" => "Most negative finite value (Rust-style — `MIN`, not the smallest positive).",
        "Max" => "Largest finite value.",
        "MinPositive" => "Smallest positive normal value.",
        "Epsilon" => "Gap between `1.0` and the next representable value.",
        _ => return None,
    })
}

pub(crate) fn set_method_names() -> &'static [&'static str] {
    &[
        "add", "has", "delete", "size", "clear",
        "values", "forEach",
        "union", "intersection", "difference",
        "isSubsetOf", "isSupersetOf", "isDisjointFrom",
    ]
}

pub(crate) fn set_method_sig(method: &str, t: &Type) -> Option<String> {
    let body = match method {
        "add" => format!("add(v: {t}): ()"),
        "has" => format!("has(v: {t}): bool"),
        "delete" => format!("delete(v: {t}): bool"),
        "size" => "size(): i64".to_string(),
        "clear" => "clear(): ()".to_string(),
        "values" => format!("values(): {t}[]"),
        "forEach" => format!("forEach(cb: fn({t}): ()): ()"),
        "union" => format!("union(other: Set<{t}>): Set<{t}>"),
        "intersection" => format!("intersection(other: Set<{t}>): Set<{t}>"),
        "difference" => format!("difference(other: Set<{t}>): Set<{t}>"),
        "isSubsetOf" => format!("isSubsetOf(other: Set<{t}>): bool"),
        "isSupersetOf" => format!("isSupersetOf(other: Set<{t}>): bool"),
        "isDisjointFrom" => format!("isDisjointFrom(other: Set<{t}>): bool"),
        _ => return None,
    };
    Some(format!("(method) Set<{t}>.{body}"))
}

pub(crate) fn set_method_doc(method: &str) -> Option<&'static str> {
    Some(match method {
        "add" => "Inserts `v`. Duplicates (equal under the element's `==`) are ignored.",
        "has" => "Returns `true` when `v` is already in the set.",
        "delete" => "Removes `v`. Returns `true` when an entry existed, `false` otherwise.",
        "size" => "Returns the number of elements currently stored.",
        "clear" => "Removes every element.",
        "values" => "Returns a new array of every element, in arbitrary order.",
        "forEach" => "Calls `cb(element)` once per entry. Callback returns `()`; the set's contents must not be mutated during the call.",
        "union" => "Returns a new set containing every element present in either `self` or `other`.",
        "intersection" => "Returns a new set containing only the elements present in both `self` and `other`.",
        "difference" => "Returns a new set containing elements of `self` that are not in `other`.",
        "isSubsetOf" => "Returns `true` when every element of `self` is also in `other`.",
        "isSupersetOf" => "Returns `true` when every element of `other` is also in `self`.",
        "isDisjointFrom" => "Returns `true` when `self` and `other` share no elements.",
        _ => return None,
    })
}

pub(crate) fn map_method_doc(method: &str) -> Option<&'static str> {
    Some(match method {
        "get" => "Returns `some(value)` for `key`, or `none` when the key is absent.",
        "set" => "Inserts `value` under `key`, replacing any existing entry.",
        "has" => "Returns `true` when `key` has an associated entry.",
        "delete" => "Removes the entry for `key`. Returns `true` when an entry existed, `false` otherwise.",
        "size" => "Returns the number of entries currently stored.",
        "keys" => "Returns a new array of every key, in insertion order.",
        "values" => "Returns a new array of every value, in insertion order.",
        "clear" => "Removes every entry. Equivalent to calling `delete` on each key.",
        "entries" => "Returns a new array of `(key, value)` tuples, in arbitrary order.",
        "forEach" => "Calls `cb(key, value)` once per entry. Callback returns `()`; the map's contents must not be mutated during the call.",
        _ => return None,
    })
}

/// Hover documentation for the built-in array methods.
pub(crate) fn array_method_doc(method: &str) -> Option<&'static str> {
    Some(match method {
        "push" => "Appends `v` to the end of the array. Mutates the receiver.",
        "pop" => "Removes and returns the last element, or `none` when the array is empty.",
        "shift" => "Removes and returns the first element, or `none` when the array is empty.",
        "unshift" => "Inserts `v` at index 0, shifting the existing elements right.",
        "remove" => "Removes the first element equal to `v`. Returns `true` when an element was removed, `false` otherwise.",
        "removeAt" => "Removes the element at index `i` and returns it, or `none` when `i` is out of `[0, length)`.",
        "indexOf" => "Returns the index of the first element equal to `v`, or `-1` when no element matches.",
        "includes" => "Returns `true` when any element equals `v`.",
        "find" => "Returns the first element for which `pred(elem)` returns `true`, or `none` when nothing matches.",
        "findIndex" => "Returns the index of the first element for which `pred(elem)` returns `true`, or `-1` when nothing matches.",
        "every" => "Returns `true` when `pred(elem)` returns `true` for every element. Vacuously `true` on an empty array.",
        "some" => "Returns `true` when `pred(elem)` returns `true` for at least one element. `false` on an empty array.",
        "slice" => "Returns a new array covering indices `[start, end)`. Indices are clamped to the array's length.",
        "concat" => "Returns a new array whose contents are this array followed by `other`. Source arrays are untouched.",
        "reverse" => "Returns a new array with the elements in reverse order. The receiver is untouched.",
        "fill" => "Overwrites every cell with `v`. Mutates the receiver.",
        "sort" => "Returns a new array sorted by `cmp(a, b)` — negative for `a < b`, zero for equal, positive for `a > b`.",
        "map" => "Returns a new array of `f(elem)` for each element, in order.",
        "filter" => "Returns a new array of every element for which `pred(elem)` returns `true`.",
        "forEach" => "Invokes `f` on each element in order. Returns nothing.",
        "join" => "Concatenates the strings in this `string[]` with `sep` between each pair, returning a single string.",
        _ => return None,
    })
}
