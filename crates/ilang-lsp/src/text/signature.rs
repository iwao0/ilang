//! Signature / call-context parsing: pull a parameter type out of a
//! rendered signature, compute inlay-hint parameter offsets, and
//! resolve the call / generic-argument context around the cursor for
//! signature help.

use tower_lsp::lsp_types::Position;

use crate::helpers::sig_body_skip_attrs;

use super::{line_col_to_offset, STR_LITERAL_RECEIVER};


/// Given a signature label like `(method) Counter.init(a: i64, b: i64)`,
/// return byte-offset ranges for each parameter span. The LSP client
/// uses them to bold the active parameter.
/// Extract the type name from the `i`-th parameter slot of a fn-style
/// signature label like `fn makeWindow(title: string, mask:
/// NSWindowStyleMask): NSWindow`. Returns the bare type name (no
/// `[]` / `?` / `*` suffixes) so completion can match it against
/// known classes / enums / vars. Returns `None` for out-of-range
/// indices or unparseable slots.
pub(crate) fn nth_param_type_name(signature: &str, arg_index: usize) -> Option<String> {
    let offsets = parameter_offsets(signature);
    let (s, e) = offsets.get(arg_index).copied()?;
    let slot = signature.get(s as usize..e as usize)?;
    let after_colon = slot.split_once(':').map(|(_, t)| t.trim())?;
    // Strip the common suffixes / prefixes that don't affect the
    // base type name. Order-sensitive: peel `*const` / `*` prefixes
    // before walking the trailing chars.
    let mut t = after_colon;
    for prefix in ["*const ", "*mut ", "*"] {
        if let Some(rest) = t.strip_prefix(prefix) {
            t = rest.trim_start();
            break;
        }
    }
    let mut end = t.len();
    for (i, c) in t.char_indices() {
        if !(c.is_ascii_alphanumeric() || c == '_' || c == '.') {
            end = i;
            break;
        }
    }
    let bare = &t[..end];
    if bare.is_empty() { None } else { Some(bare.to_string()) }
}

pub(crate) fn parameter_offsets(label: &str) -> Vec<(u32, u32)> {
    // Strip leading `@attr` / `@attr(...)` lines so the scanner
    // doesn't lock onto the `(` inside e.g. `@lib("user32")` and
    // treat its content as the parameter list. Returned offsets
    // are still relative to the original `label`, so callers that
    // pass them into `ParameterLabel::LabelOffsets` keep working.
    let stripped = sig_body_skip_attrs(label);
    let prefix_len = (label.len() - stripped.len()) as u32;
    parameter_offsets_raw(stripped.as_bytes())
        .into_iter()
        .map(|(s, e)| (s + prefix_len, e + prefix_len))
        .collect()
}

fn parameter_offsets_raw(bytes: &[u8]) -> Vec<(u32, u32)> {
    // Skip the parser-tag prefix (`(method) `, `(static method) `,
    // `(getter) ` …). Each prefix is a `(...)` pair containing only
    // alphabetic chars + whitespace; keep peeling them until the
    // next `(` is the real parameter list. Without this the
    // rposition trick below picked the `()` from a return type
    // like `): ()` and reported zero parameters.
    let mut scan_from = 0usize;
    loop {
        while scan_from < bytes.len()
            && matches!(bytes[scan_from], b' ' | b'\t' | b'\r' | b'\n')
        {
            scan_from += 1;
        }
        if bytes.get(scan_from).copied() != Some(b'(') {
            break;
        }
        // Find the matching `)` and check the contents look tag-like.
        let mut depth = 1i32;
        let mut end = scan_from + 1;
        while end < bytes.len() && depth > 0 {
            match bytes[end] {
                b'(' => depth += 1,
                b')' => depth -= 1,
                _ => {}
            }
            end += 1;
        }
        if depth != 0 {
            break;
        }
        let inside = &bytes[scan_from + 1..end - 1];
        let tag_like = !inside.is_empty()
            && inside
                .iter()
                .all(|b| b.is_ascii_alphabetic() || matches!(*b, b' ' | b'\t'));
        if !tag_like {
            break;
        }
        scan_from = end;
    }
    // First `(` after the prefix tags is the parameter list opener.
    let Some(open) = (scan_from..bytes.len()).find(|&i| bytes[i] == b'(') else {
        return Vec::new();
    };
    let mut depth = 1i32;
    let mut close_idx: Option<usize> = None;
    let mut j = open + 1;
    while j < bytes.len() {
        match bytes[j] {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    close_idx = Some(j);
                    break;
                }
            }
            _ => {}
        }
        j += 1;
    }
    let Some(close) = close_idx else {
        return Vec::new();
    };
    if close <= open + 1 {
        return Vec::new();
    }
    let mut out: Vec<(u32, u32)> = Vec::new();
    let mut start = open + 1;
    let mut paren_depth = 0i32;
    let mut bracket_depth = 0i32;
    for i in start..close {
        let b = bytes[i];
        match b {
            b'(' => paren_depth += 1,
            b')' => paren_depth -= 1,
            b'[' => bracket_depth += 1,
            b']' => bracket_depth -= 1,
            b',' if paren_depth == 0 && bracket_depth == 0 => {
                let s = trim_offset(bytes, start, i);
                if s.0 < s.1 {
                    out.push((s.0 as u32, s.1 as u32));
                }
                start = i + 1;
            }
            _ => {}
        }
    }
    let s = trim_offset(bytes, start, close);
    if s.0 < s.1 {
        out.push((s.0 as u32, s.1 as u32));
    }
    out
}

fn trim_offset(bytes: &[u8], mut s: usize, mut e: usize) -> (usize, usize) {
    while s < e && (bytes[s] == b' ' || bytes[s] == b'\t') {
        s += 1;
    }
    while e > s && (bytes[e - 1] == b' ' || bytes[e - 1] == b'\t') {
        e -= 1;
    }
    (s, e)
}

pub(crate) struct CallContext {
    pub callee: String,
    pub is_new: bool,
    pub arg_index: usize,
}

/// Generic-argument signature context, returned when the cursor sits
/// inside `TypeName<...>`. `arg_index` is the zero-based slot the
/// cursor is currently filling (number of `,`s after the opening `<`).
pub(crate) struct GenericContext {
    pub type_name: String,
    pub type_params: Vec<&'static str>,
    pub arg_index: usize,
    pub short_doc: Option<&'static str>,
}

/// Detect when the cursor sits inside `TypeName<...>` and return the
/// type's generic parameter list along with the active slot. Only
/// recognizes the built-in generic types the type checker pre-registers
/// (`Map`, `Promise`, `Result`, `ObjCBlock`) — user-declared generics
/// would need a doc-table lookup we don't currently maintain here.
pub(crate) fn generic_args_context_at(text: &str, pos: Position) -> Option<GenericContext> {
    let off = line_col_to_offset(text, pos.line + 1, pos.character + 1)?;
    let bytes = text.as_bytes();
    if off > bytes.len() {
        return None;
    }
    let mut angle_depth: i32 = 0;
    let mut paren_depth: i32 = 0;
    let mut commas: usize = 0;
    let mut i = off;
    while i > 0 {
        let b = bytes[i - 1];
        match b {
            b')' | b']' => paren_depth += 1,
            b'(' | b'[' => {
                if paren_depth == 0 {
                    return None;
                }
                paren_depth -= 1;
            }
            b'>' if paren_depth == 0 => angle_depth += 1,
            b',' if paren_depth == 0 && angle_depth == 0 => commas += 1,
            b'<' if paren_depth == 0 => {
                if angle_depth == 0 {
                    let mut k = i - 1;
                    while k > 0 && matches!(bytes[k - 1], b' ' | b'\t') {
                        k -= 1;
                    }
                    let id_end = k;
                    while k > 0 {
                        let c = bytes[k - 1];
                        if c.is_ascii_alphanumeric() || c == b'_' {
                            k -= 1;
                        } else {
                            break;
                        }
                    }
                    if k == id_end {
                        return None;
                    }
                    let name = std::str::from_utf8(&bytes[k..id_end]).ok()?.to_string();
                    let (params, doc): (Vec<&'static str>, Option<&'static str>) =
                        match name.as_str() {
                            "Map" => (
                                vec!["K", "V"],
                                Some("Built-in associative map: keys of type K, values of type V."),
                            ),
                            "Promise" => (
                                vec!["T"],
                                Some("Built-in asynchronous value resolving to T."),
                            ),
                            "Result" => (
                                vec!["T", "E"],
                                Some("Built-in success/error enum with ok(T) and err(E) variants."),
                            ),
                            "ObjCBlock" => (
                                vec!["F"],
                                Some("Built-in Objective-C block whose closure type matches F."),
                            ),
                            _ => return None,
                        };
                    return Some(GenericContext {
                        type_name: name,
                        type_params: params,
                        arg_index: commas,
                        short_doc: doc,
                    });
                }
                angle_depth -= 1;
            }
            b'\n' | b'{' | b';' => return None,
            _ => {}
        }
        i -= 1;
    }
    None
}

/// Find the `callee(...)` containing the cursor by scanning backwards
/// past balanced parens / brackets.
pub(crate) fn call_context_at(text: &str, pos: Position) -> Option<CallContext> {
    let line = pos.line + 1;
    let col = pos.character + 1;
    let mut off = line_col_to_offset(text, line, col)?;
    let bytes = text.as_bytes();
    if off > bytes.len() {
        return None;
    }
    let mut paren_depth: i32 = 0;
    let mut bracket_depth: i32 = 0;
    let mut commas: usize = 0;
    while off > 0 {
        off -= 1;
        let b = bytes[off];
        match b {
            b')' | b']' => {
                if b == b')' {
                    paren_depth += 1;
                } else {
                    bracket_depth += 1;
                }
            }
            b'(' => {
                if paren_depth > 0 {
                    paren_depth -= 1;
                } else {
                    break;
                }
            }
            b'[' => {
                if bracket_depth > 0 {
                    bracket_depth -= 1;
                }
            }
            b',' if paren_depth == 0 && bracket_depth == 0 => {
                commas += 1;
            }
            _ => {}
        }
    }
    if bytes.get(off).copied() != Some(b'(') {
        return None;
    }
    let mut i = off;
    while i > 0 && matches!(bytes[i - 1], b' ' | b'\t') {
        i -= 1;
    }
    let end = i;
    while i > 0 {
        let b = bytes[i - 1];
        if b.is_ascii_alphanumeric() || b == b'_' || b == b'.' {
            i -= 1;
        } else {
            break;
        }
    }
    let mut callee = std::str::from_utf8(&bytes[i..end]).ok()?.to_string();
    if callee.is_empty() {
        return None;
    }
    // String literal as the method receiver — `"abc".method(`. The
    // identifier walker above kept the leading `.` but couldn't enter
    // the string body. Rewrite the callee to start with the
    // `STR_LITERAL_RECEIVER` sentinel so the signature-help handler
    // routes through the built-in string-method table.
    if callee.starts_with('.') && i > 0 && bytes[i - 1] == b'"' {
        let mut k = i - 1;
        let mut found = false;
        while k > 0 {
            k -= 1;
            if bytes[k] == b'"' {
                let mut bs = 0;
                let mut q = k;
                while q > 0 && bytes[q - 1] == b'\\' {
                    bs += 1;
                    q -= 1;
                }
                if bs % 2 == 0 {
                    found = true;
                    break;
                }
            }
        }
        if found {
            callee = format!("{}{}", STR_LITERAL_RECEIVER, callee);
        }
    }
    let mut j = i;
    while j > 0 && matches!(bytes[j - 1], b' ' | b'\t') {
        j -= 1;
    }
    let is_new = j >= 3
        && &bytes[j - 3..j] == b"new"
        && {
            let prev = if j >= 4 { Some(bytes[j - 4]) } else { None };
            prev.map(|c| !c.is_ascii_alphanumeric() && c != b'_')
                .unwrap_or(true)
        };
    Some(CallContext {
        callee,
        is_new,
        arg_index: commas,
    })
}
