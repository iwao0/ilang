//! Cursor-context queries — given the buffer text and a byte offset,
//! decide where the cursor sits (inside `@extern(C) { … }`, at an
//! attribute position, inside `use M { … }`, inside a class body,
//! at a type position, after a let/const binder, brace depth, etc.).
//! Pure functions over `&str` + offset; no AST involved. Used by
//! `handle_completion` to pick which builder to call.

use ilang_ast::Span;

use crate::text;

/// Read the literal token at `span` from `src` — captures hex /
/// binary / octal prefixes, underscore separators, and any
/// integer / float type suffix. Returns `None` when the span
/// doesn't resolve to a contiguous identifier-like token.
pub(crate) fn literal_token_at(src: &str, span: Span) -> Option<String> {
    let off = text::line_col_to_offset(src, span.line, span.col)?;
    let bytes = src.as_bytes();
    let mut i = off;
    if i < bytes.len() && bytes[i] == b'-' {
        i += 1;
    }
    let start = i;
    while i < bytes.len()
        && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' || bytes[i] == b'.')
    {
        i += 1;
    }
    if i > start {
        std::str::from_utf8(&bytes[off..i]).ok().map(|s| s.to_string())
    } else {
        None
    }
}

/// Function / method completion items insert just their bare name.
/// (We used to insert `name($0)` to trigger signature help, but that
/// mangled valid uses where the user wants the name alone — passing a

/// `true` when the cursor sits inside an `@extern(C) { ... }` block.
/// Walks back across balanced braces; the first unmatched `{` is the
/// enclosing block, and we check whether `@extern(C)` precedes it
/// (with optional whitespace).
pub(crate) fn in_extern_c_block(text: &str, offset: usize) -> bool {
    let bytes = text.as_bytes();
    let end = offset.min(bytes.len());
    let mut depth: i32 = 0;
    let mut i = end;
    while i > 0 {
        i -= 1;
        match bytes[i] {
            b'}' => depth += 1,
            b'{' => {
                if depth > 0 {
                    depth -= 1;
                } else if extern_c_precedes(bytes, i) {
                    return true;
                }
                // Either way, keep walking past this `{` to inspect
                // outer enclosing braces too.
            }
            _ => {}
        }
    }
    false
}

/// `true` if `@extern(C)` (with optional whitespace) appears
/// immediately before byte index `at`.
fn extern_c_precedes(bytes: &[u8], at: usize) -> bool {
    let mut j = at;
    while j > 0 && matches!(bytes[j - 1], b' ' | b'\t' | b'\r' | b'\n') {
        j -= 1;
    }
    if j == 0 || bytes[j - 1] != b')' {
        return false;
    }
    let mut k = j - 1;
    while k > 0 && matches!(bytes[k - 1], b' ' | b'\t') {
        k -= 1;
    }
    if k == 0 || bytes[k - 1] != b'C' {
        return false;
    }
    k -= 1;
    while k > 0 && matches!(bytes[k - 1], b' ' | b'\t') {
        k -= 1;
    }
    if k == 0 || bytes[k - 1] != b'(' {
        return false;
    }
    k -= 1;
    while k > 0 && matches!(bytes[k - 1], b' ' | b'\t') {
        k -= 1;
    }
    if k < 6 || &bytes[k - 6..k] != b"extern" {
        return false;
    }
    let kk = k - 6;
    kk >= 1 && bytes[kk - 1] == b'@'
}

/// `true` when the cursor sits in attribute syntax — i.e. an `@`
/// (followed by an in-progress identifier) is the previous non-ident
/// character on the line.
pub(crate) fn at_attribute_position(text: &str, offset: usize) -> bool {
    let bytes = text.as_bytes();
    let end = offset.min(bytes.len());
    let mut i = end;
    while i > 0 {
        let b = bytes[i - 1];
        if b.is_ascii_alphanumeric() || b == b'_' {
            i -= 1;
        } else {
            break;
        }
    }
    i > 0 && bytes[i - 1] == b'@'
}

/// ilang attributes for completion. `(args)` snippets are inserted

/// been typed), returns the imported module name. Used by completion
/// to swap the global candidate list for the target module's own
/// exports — typing `N` after `use cocoa {` should offer `NSObject`,
/// not the buffer-local fn names that `global_completions` would
/// surface.
pub(crate) fn enclosing_use_module(text: &str, offset: usize) -> Option<String> {
    let bytes = text.as_bytes();
    if offset > bytes.len() {
        return None;
    }
    // Scan backward to find an unmatched `{`. Bail on `}` (balanced
    // close) and on `;` / `\n\n` boundaries the parser would treat as
    // a hard statement break — those can't sit inside a use list.
    let mut depth = 0i32;
    let mut i = offset;
    while i > 0 {
        i -= 1;
        match bytes[i] {
            b'}' => depth += 1,
            b'{' => {
                if depth == 0 {
                    // Found the candidate opener. Look at what
                    // precedes it: skip whitespace, then an
                    // identifier (and optional `as _` alias / `as
                    // <name>`), then the `use` keyword.
                    let mut j = i;
                    while j > 0 && matches!(bytes[j - 1], b' ' | b'\t') {
                        j -= 1;
                    }
                    // Optional `as _` / `as <ident>`.
                    let mut after_alias = j;
                    if j >= 1 && (bytes[j - 1] == b'_' || bytes[j - 1].is_ascii_alphanumeric()) {
                        let alias_end = j;
                        let mut k = j;
                        while k > 0 && (bytes[k - 1].is_ascii_alphanumeric() || bytes[k - 1] == b'_')
                        {
                            k -= 1;
                        }
                        let alias = &bytes[k..alias_end];
                        // Need a preceding `as` token to treat this
                        // as the alias rather than the module ident.
                        let mut a = k;
                        while a > 0 && matches!(bytes[a - 1], b' ' | b'\t') {
                            a -= 1;
                        }
                        if a >= 2 && &bytes[a - 2..a] == b"as" {
                            let before_as = a - 2;
                            let prev_is_boundary = before_as == 0
                                || !(bytes[before_as - 1].is_ascii_alphanumeric()
                                    || bytes[before_as - 1] == b'_');
                            if prev_is_boundary {
                                after_alias = before_as;
                                let _ = alias;
                            }
                        }
                    }
                    let mut j = after_alias;
                    while j > 0 && matches!(bytes[j - 1], b' ' | b'\t') {
                        j -= 1;
                    }
                    // Module ident.
                    if j == 0 {
                        return None;
                    }
                    let ident_end = j;
                    while j > 0 && (bytes[j - 1].is_ascii_alphanumeric() || bytes[j - 1] == b'_') {
                        j -= 1;
                    }
                    if j == ident_end {
                        return None;
                    }
                    let module = std::str::from_utf8(&bytes[j..ident_end]).ok()?.to_string();
                    let mut k = j;
                    while k > 0 && matches!(bytes[k - 1], b' ' | b'\t') {
                        k -= 1;
                    }
                    // `use` keyword (3 chars), preceded by a token
                    // boundary so we don't match e.g. `disuse`.
                    if k < 3 || &bytes[k - 3..k] != b"use" {
                        return None;
                    }
                    let before_use = k - 3;
                    if before_use > 0
                        && (bytes[before_use - 1].is_ascii_alphanumeric()
                            || bytes[before_use - 1] == b'_')
                    {
                        return None;
                    }
                    return Some(module);
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    None
}

/// If `offset` sits inside the body of a `class Foo { ... }` (or
/// `pub class Foo : Parent { ... }`) declaration, returns the
/// outermost enclosing class name. Used by completion to map a bare
/// `this.` receiver to the class whose fields / methods should be
/// listed.
///
/// Implementation: forward scan with brace-tracking. Each open brace
/// pushes the most-recently-seen `class Name` token onto a stack
/// (or `None` if the brace came from a non-class construct); each
/// close pops. At the end, the first `Some` on the stack from the
/// outside in is the enclosing class.
pub(crate) fn enclosing_class(text: &str, offset: usize) -> Option<String> {
    let bytes = text.as_bytes();
    let end = offset.min(bytes.len());
    let mut stack: Vec<Option<String>> = Vec::new();
    let mut pending_class: Option<String> = None;
    let mut i = 0;
    let mut in_line_comment = false;
    let mut block_depth: u32 = 0;
    let mut in_string = false;
    while i < end {
        let b = bytes[i];
        if in_line_comment {
            if b == b'\n' {
                in_line_comment = false;
            }
            i += 1;
            continue;
        }
        if block_depth > 0 {
            if b == b'/' && i + 1 < end && bytes[i + 1] == b'*' {
                block_depth += 1;
                i += 2;
                continue;
            }
            if b == b'*' && i + 1 < end && bytes[i + 1] == b'/' {
                block_depth -= 1;
                i += 2;
                continue;
            }
            i += 1;
            continue;
        }
        if in_string {
            if b == b'\\' && i + 1 < end {
                i += 2;
                continue;
            }
            if b == b'"' {
                in_string = false;
            }
            i += 1;
            continue;
        }
        if b == b'/' && i + 1 < end {
            if bytes[i + 1] == b'/' {
                in_line_comment = true;
                i += 2;
                continue;
            }
            if bytes[i + 1] == b'*' {
                block_depth = 1;
                i += 2;
                continue;
            }
        }
        if b == b'"' {
            in_string = true;
            i += 1;
            continue;
        }
        if b.is_ascii_alphabetic() || b == b'_' {
            let start = i;
            let mut j = i;
            while j < end && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
                j += 1;
            }
            let prev_boundary = start == 0
                || !(bytes[start - 1].is_ascii_alphanumeric() || bytes[start - 1] == b'_');
            if prev_boundary && &bytes[start..j] == b"class" {
                let mut k = j;
                while k < end && matches!(bytes[k], b' ' | b'\t') {
                    k += 1;
                }
                let name_start = k;
                while k < end && (bytes[k].is_ascii_alphanumeric() || bytes[k] == b'_') {
                    k += 1;
                }
                if k > name_start {
                    if let Ok(name) = std::str::from_utf8(&bytes[name_start..k]) {
                        pending_class = Some(name.to_string());
                    }
                }
                i = k;
                continue;
            }
            i = j;
            continue;
        }
        match b {
            b'{' => {
                stack.push(pending_class.take());
            }
            b'}' => {
                stack.pop();
                pending_class = None;
            }
            _ => {}
        }
        i += 1;
    }
    for entry in &stack {
        if let Some(name) = entry {
            return Some(name.clone());
        }
    }
    None
}

pub(crate) fn at_type_position(text: &str, offset: usize) -> bool {
    let bytes = text.as_bytes();
    let end = offset.min(bytes.len());
    let mut i = end;
    while i > 0 {
        let b = bytes[i - 1];
        if b.is_ascii_alphanumeric() || b == b'_' {
            i -= 1;
        } else {
            break;
        }
    }
    while i > 0 && matches!(bytes[i - 1], b' ' | b'\t') {
        i -= 1;
    }
    if i == 0 {
        return false;
    }
    match bytes[i - 1] {
        b':' => true,
        // `class C : A, ` — the comma extends the base list; scan
        // further back through one prior ident + optional `:` to
        // confirm we're inside a class-base list rather than an
        // arbitrary tuple / argument list.
        // Also handles `Map<K, ` and similar generic-arg positions.
        b',' => is_in_class_base_list(bytes, i - 1) || is_in_generic_args(bytes, i - 1),
        // `Foo<` — first generic argument slot.
        b'<' => is_in_generic_args(bytes, i),
        _ => false,
    }
}

/// Heuristic: decide whether `from` sits inside an unmatched `<...>`
/// whose opener is preceded by an identifier that is itself in a type
/// position. Powers type-completion inside `Map<K, V>` and other
/// generic-argument slots.
fn is_in_generic_args(bytes: &[u8], from: usize) -> bool {
    let mut depth: i32 = 0;
    let mut i = from;
    while i > 0 {
        let b = bytes[i - 1];
        match b {
            b'>' => depth += 1,
            b'<' => {
                if depth == 0 {
                    let mut k = i - 1;
                    while k > 0 && matches!(bytes[k - 1], b' ' | b'\t') {
                        k -= 1;
                    }
                    let id_end = k;
                    while k > 0 {
                        let c = bytes[k - 1];
                        if c.is_ascii_alphanumeric() || c == b'_' || c == b'.' {
                            k -= 1;
                        } else {
                            break;
                        }
                    }
                    if k == id_end {
                        return false;
                    }
                    let Ok(text) = std::str::from_utf8(bytes) else {
                        return false;
                    };
                    if at_type_position(text, id_end) {
                        return true;
                    }
                    // `new TypeName<` — argument slots inside the
                    // constructor's generic list are still types,
                    // even though the identifier itself isn't in a
                    // bare type-annotation position.
                    let mut p = k;
                    while p > 0 && matches!(bytes[p - 1], b' ' | b'\t') {
                        p -= 1;
                    }
                    const NEW_KW: &[u8] = b"new";
                    if p >= NEW_KW.len() && &bytes[p - NEW_KW.len()..p] == NEW_KW {
                        let boundary = p.checked_sub(NEW_KW.len() + 1).map(|b| bytes[b]);
                        let ok = match boundary {
                            None => true,
                            Some(c) => !(c.is_ascii_alphanumeric() || c == b'_'),
                        };
                        if ok {
                            return true;
                        }
                    }
                    return false;
                }
                depth -= 1;
            }
            b'\n' | b'(' | b'{' | b';' => return false,
            _ => {}
        }
        i -= 1;
    }
    false
}

/// Heuristic: walk backwards from a `,` to decide whether the
/// surrounding context is a `class Name : A, B, …` base list (so
/// completion suggests types) versus a regular call / tuple / arg
/// list (where suggesting types would be misleading). The check
/// is intentionally simple — find the nearest `:` or `(` / `{` /
/// `<` on the same line; only `:` (with a `class` token before
/// the identifier) qualifies.
fn is_in_class_base_list(bytes: &[u8], from: usize) -> bool {
    // Scan backwards on the current line for a `:` not preceded
    // by a `<` / `(` / `{` opener. If we hit one of those
    // openers, this isn't a base list.
    let mut j = from;
    while j > 0 {
        let b = bytes[j - 1];
        if b == b'\n' {
            return false;
        }
        if b == b'(' || b == b'{' || b == b'<' {
            return false;
        }
        if b == b':' {
            // Confirm the `:` is the base-list colon: walk back
            // through whitespace + an identifier; expect a `class`
            // keyword before it.
            let mut k = j - 1;
            while k > 0 && matches!(bytes[k - 1], b' ' | b'\t') {
                k -= 1;
            }
            while k > 0 {
                let c = bytes[k - 1];
                if c.is_ascii_alphanumeric() || c == b'_' {
                    k -= 1;
                } else {
                    break;
                }
            }
            while k > 0 && matches!(bytes[k - 1], b' ' | b'\t') {
                k -= 1;
            }
            // Expect `class ` keyword immediately before the name.
            const CLASS_KW: &[u8] = b"class";
            if k >= CLASS_KW.len() && &bytes[k - CLASS_KW.len()..k] == CLASS_KW {
                return true;
            }
            return false;
        }
        j -= 1;
    }
    false
}

/// (with optional whitespace and possibly a partial ident underway).
/// Used to suppress completion at the binder position — anything we
/// suggest there would shadow / overwrite the new name.
pub(crate) fn preceding_kw_introduces_binder(text: &str, offset: usize) -> bool {
    let bytes = text.as_bytes();
    let end = offset.min(bytes.len());
    // Skip the in-progress ident the user is typing.
    let mut i = end;
    while i > 0 {
        let b = bytes[i - 1];
        if b.is_ascii_alphanumeric() || b == b'_' {
            i -= 1;
        } else {
            break;
        }
    }
    while i > 0 && matches!(bytes[i - 1], b' ' | b'\t') {
        i -= 1;
    }
    for kw in ["let", "const"] {
        let n = kw.len();
        if i >= n && &bytes[i - n..i] == kw.as_bytes() {
            let prev = if i > n { Some(bytes[i - n - 1]) } else { None };
            let boundary = prev
                .map(|c| !c.is_ascii_alphanumeric() && c != b'_')
                .unwrap_or(true);
            if boundary {
                return true;
            }
        }
    }
    false
}

/// Brace depth of `text` at byte offset `offset`. Counts `{` and `}`
/// outside string / char / line / block comments. Used by completion
/// to filter keywords by context.
pub(crate) fn brace_depth_at(text: &str, offset: usize) -> i32 {
    let bytes = text.as_bytes();
    let end = offset.min(bytes.len());
    let mut depth: i32 = 0;
    let mut in_string = false;
    let mut in_line_comment = false;
    let mut block_depth: i32 = 0;
    let mut i = 0;
    while i < end {
        let b = bytes[i];
        if in_line_comment {
            if b == b'\n' {
                in_line_comment = false;
            }
            i += 1;
            continue;
        }
        if block_depth > 0 {
            if b == b'/' && i + 1 < end && bytes[i + 1] == b'*' {
                block_depth += 1;
                i += 2;
                continue;
            }
            if b == b'*' && i + 1 < end && bytes[i + 1] == b'/' {
                block_depth -= 1;
                i += 2;
                continue;
            }
            i += 1;
            continue;
        }
        if in_string {
            if b == b'\\' {
                i += 2;
                continue;
            }
            if b == b'"' {
                in_string = false;
            }
            i += 1;
            continue;
        }
        if b == b'/' && i + 1 < end {
            if bytes[i + 1] == b'/' {
                in_line_comment = true;
                i += 2;
                continue;
            }
            if bytes[i + 1] == b'*' {
                block_depth = 1;
                i += 2;
                continue;
            }
        }
        if b == b'"' {
            in_string = true;
        } else if b == b'{' {
            depth += 1;
        } else if b == b'}' {
            depth -= 1;
        }
        i += 1;
    }
    depth
}

/// Append keyword completions matching the cursor's brace context.

#[cfg(test)]
mod use_completion_tests {
    use super::enclosing_use_module;

    #[test]
    fn inside_single_line_use_brace() {
        let src = "use cocoa { N";
        assert_eq!(enclosing_use_module(src, src.len()).as_deref(), Some("cocoa"));
    }

    #[test]
    fn inside_multiline_use_brace() {
        let src = "use cocoa {\n    NSObject\n    N";
        assert_eq!(enclosing_use_module(src, src.len()).as_deref(), Some("cocoa"));
    }

    #[test]
    fn inside_use_with_alias_discard() {
        let src = "use cocoa as _ { N";
        assert_eq!(enclosing_use_module(src, src.len()).as_deref(), Some("cocoa"));
    }

    #[test]
    fn outside_use_brace_returns_none() {
        let src = "let x = { 1 + ";
        assert!(enclosing_use_module(src, src.len()).is_none());
    }

    #[test]
    fn after_closed_use_brace_returns_none() {
        let src = "use cocoa { NSObject }\nlet x = ";
        assert!(enclosing_use_module(src, src.len()).is_none());
    }
}

#[cfg(test)]
mod enclosing_class_tests {
    use super::enclosing_class;

    #[test]
    fn inside_method_body_simple() {
        let src = "\
pub class Foo {
    pub init() {
        ";
        assert_eq!(enclosing_class(src, src.len()).as_deref(), Some("Foo"));
    }

    #[test]
    fn inside_method_with_inheritance() {
        let src = "\
pub class Bar : Parent, Iface {
    pub run() {
        ";
        assert_eq!(enclosing_class(src, src.len()).as_deref(), Some("Bar"));
    }

    #[test]
    fn between_class_decls_returns_none() {
        let src = "class A { fn a() {} }\nclass B { fn b() {} }\n";
        assert!(enclosing_class(src, src.len()).is_none());
    }

    #[test]
    fn inside_method_after_block_statement() {
        let src = "\
class Foo {
    pub run() {
        if x == 0 { return }
        ";
        assert_eq!(enclosing_class(src, src.len()).as_deref(), Some("Foo"));
    }
}
