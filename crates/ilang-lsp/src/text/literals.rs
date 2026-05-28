//! Literal-receiver detection for completion: recognise `"abc".`,
//! `(1.0).`, `(42).`, `(true).` before a dot and hand back a sentinel
//! receiver string the completion handler maps to the built-in
//! method list.

use tower_lsp::lsp_types::Position;

use super::line_col_to_offset;


/// Walk back from the cursor over whitespace and a leading `.` to find
/// the receiver identifier — used by completion to figure out what
/// class's members to list.
/// Sentinel receiver returned by `receiver_before_dot` when the
/// expression before `.` is a string literal (`"abc".`) — caller
/// recognises this and surfaces the built-in string methods.
pub(crate) const STR_LITERAL_RECEIVER: &str = "\"\"";

/// Sentinel receiver for a parenthesised float literal (`(1.0).` /
/// `(-3.14).`). The completion handler maps this to `Type::F64` and
/// emits the primitive-method list (`toString`, `isFinite`, `isNaN`).
pub(crate) const FLOAT_LITERAL_RECEIVER: &str = "(0.0)";

/// Sentinel receiver for a parenthesised int literal (`(1).` /
/// `(0xFF).` / `(-42).`). The completion handler maps this to
/// `Type::I64` (the default int type) and surfaces the numeric
/// primitive methods (just `toString` today).
pub(crate) const INT_LITERAL_RECEIVER: &str = "(0)";

/// Sentinel receiver for a parenthesised bool literal (`(true).` /
/// `(false).`). Mapped to `Type::Bool` so completion surfaces
/// `toString`.
pub(crate) const BOOL_LITERAL_RECEIVER: &str = "(false)";

/// Is `s` an ilang int literal? Accepts an optional leading sign,
/// decimal / `0x` hex / `0b` binary / `0o` octal bodies (with `_`
/// permitted between digits, matching the lexer), and an optional
/// trailing numeric type suffix (`i8`..`u64`, optionally preceded
/// by `_`). Mirrors `ilang-lexer/src/scanner.rs::read_number` +
/// `try_read_numeric_suffix`.
fn is_int_literal(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.is_empty() {
        return false;
    }
    let mut i = 0;
    if bytes[i] == b'-' || bytes[i] == b'+' {
        i += 1;
    }
    if i >= bytes.len() {
        return false;
    }
    // Radix prefix dispatch. `0x` / `0b` / `0o` (case-insensitive)
    // each demand at least one digit of the matching kind; `_` is
    // permitted between digits but not as the first body char.
    let digit_ok: fn(u8) -> bool = if bytes.len() - i >= 2
        && bytes[i] == b'0'
        && matches!(bytes[i + 1], b'x' | b'X' | b'b' | b'B' | b'o' | b'O')
    {
        let pred: fn(u8) -> bool = match bytes[i + 1] {
            b'x' | b'X' => |c: u8| c.is_ascii_hexdigit(),
            b'b' | b'B' => |c: u8| c == b'0' || c == b'1',
            _ => |c: u8| (b'0'..=b'7').contains(&c),
        };
        i += 2;
        pred
    } else {
        |c: u8| c.is_ascii_digit()
    };
    if i >= bytes.len() || !digit_ok(bytes[i]) {
        return false;
    }
    i += 1;
    while i < bytes.len() && (digit_ok(bytes[i]) || bytes[i] == b'_') {
        i += 1;
    }
    // Optional type suffix: one of i8/i16/i32/i64/u8/u16/u32/u64,
    // optionally preceded by `_` (matches `try_read_numeric_suffix`).
    // No float suffix — `1.0` already routes through `is_float_literal`,
    // and a bare `1f64` isn't recognised as a float by the lexer
    // (which demands the `.`), so it stays in the int family here.
    if i < bytes.len() {
        let mut j = i;
        if bytes[j] == b'_' {
            j += 1;
        }
        let suffix = match std::str::from_utf8(&bytes[j..]) {
            Ok(s) => s,
            Err(_) => return false,
        };
        if !matches!(
            suffix,
            "i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32" | "u64"
        ) {
            return false;
        }
        return true;
    }
    true
}

/// Is `s` an ilang float literal? Accepts an optional leading `-`,
/// at least one digit either side of the `.`, and an optional
/// `e[+-]?\d+` exponent. Hex / underscore digits aren't recognised
/// (ilang's float syntax doesn't allow them today).
fn is_float_literal(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.is_empty() {
        return false;
    }
    let mut i = 0;
    if bytes[i] == b'-' || bytes[i] == b'+' {
        i += 1;
    }
    let int_start = i;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i == int_start {
        return false;
    }
    if i >= bytes.len() || bytes[i] != b'.' {
        return false;
    }
    i += 1;
    let frac_start = i;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i == frac_start {
        return false;
    }
    if i < bytes.len() && (bytes[i] == b'e' || bytes[i] == b'E') {
        i += 1;
        if i < bytes.len() && (bytes[i] == b'+' || bytes[i] == b'-') {
            i += 1;
        }
        let exp_start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i == exp_start {
            return false;
        }
    }
    i == bytes.len()
}

pub(crate) fn receiver_before_dot(text: &str, pos: Position) -> Option<String> {
    let line = pos.line + 1;
    let col = pos.character + 1;
    let mut off = line_col_to_offset(text, line, col)?;
    let bytes = text.as_bytes();
    if off > bytes.len() {
        return None;
    }
    while off > 0 && matches!(bytes[off - 1], b' ' | b'\t') {
        off -= 1;
    }
    if off == 0 || bytes[off - 1] != b'.' {
        return None;
    }
    off -= 1;
    while off > 0 && matches!(bytes[off - 1], b' ' | b'\t') {
        off -= 1;
    }
    // String literal: receiver ends with a closing `"`. Walk back
    // through the literal body to find the matching opening `"`
    // (respecting `\"` escapes). Return a sentinel so the completion
    // handler routes through the built-in string-method list.
    if off > 0 && bytes[off - 1] == b'"' {
        let mut i = off - 1;
        loop {
            if i == 0 {
                return None;
            }
            i -= 1;
            if bytes[i] == b'"' {
                let mut bs = 0;
                let mut k = i;
                while k > 0 && bytes[k - 1] == b'\\' {
                    bs += 1;
                    k -= 1;
                }
                if bs % 2 == 0 {
                    return Some(STR_LITERAL_RECEIVER.to_string());
                }
            }
        }
    }
    // Parenthesised float literal: `(1.0).`. Walk back to the matching
    // `(` (no nested parens — just a literal between them), check that
    // the trimmed inner body parses as a float literal, and return the
    // FLOAT_LITERAL_RECEIVER sentinel so completion surfaces the f64
    // primitive methods.
    if off > 0 && bytes[off - 1] == b')' {
        let close = off - 1;
        let mut i = close;
        while i > 0 && bytes[i - 1] != b'(' {
            i -= 1;
        }
        if i > 0 {
            let inner_bytes = &bytes[i..close];
            if let Ok(inner) = std::str::from_utf8(inner_bytes) {
                let trimmed = inner.trim();
                if is_float_literal(trimmed) {
                    return Some(FLOAT_LITERAL_RECEIVER.to_string());
                }
                if is_int_literal(trimmed) {
                    return Some(INT_LITERAL_RECEIVER.to_string());
                }
                if trimmed == "true" || trimmed == "false" {
                    return Some(BOOL_LITERAL_RECEIVER.to_string());
                }
            }
        }
    }
    let end = off;
    while off > 0 {
        let b = bytes[off - 1];
        if b.is_ascii_alphanumeric() || b == b'_' || b == b'.' {
            off -= 1;
        } else {
            break;
        }
    }
    let s = std::str::from_utf8(&bytes[off..end]).ok()?.to_string();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}
