//! `locate_*` helpers — given a declaration's keyword `Span`, walk the
//! source to find the exact `Span` of a name token inside it (the
//! parser only records the keyword position for many constructs, so
//! hover / F12 / rename need this to land on the identifier itself).

use ilang_ast::Span;

use super::{line_col_to_offset, offset_to_line_col};


/// Locate the start of the type token in a `name: T` form (field
/// declarations, params, etc.). Skips the `name` identifier, the
/// trailing whitespace, the `:`, more whitespace, and lands on the
/// first character of the type. Returns `None` if the layout
/// doesn't match (e.g., no `:` follows the name).
pub(crate) fn locate_type_after_colon(
    text: &str,
    name_span: Span,
    name: &str,
) -> Option<Span> {
    let off = line_col_to_offset(text, name_span.line, name_span.col)?;
    let bytes = text.as_bytes();
    let mut i = off + name.len();
    while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
        i += 1;
    }
    if i >= bytes.len() || bytes[i] != b':' {
        return None;
    }
    i += 1;
    while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
        i += 1;
    }
    if i >= bytes.len() {
        return None;
    }
    let b = bytes[i];
    if !b.is_ascii_alphabetic() && b != b'_' {
        return None;
    }
    let (line, col) = offset_to_line_col(text, i)?;
    Some(Span::new(line, col))
}

/// Locate the start of an fn's return-type token. `name_span` /
/// `name` identify the fn name; we scan past it for the param list
/// opener `(`, brace-walk to the matching `)`, then look for `:`
/// followed by the type's first character. Returns `None` for fns
/// without a return type, malformed headers, or when the type starts
/// with a non-identifier character we can't anchor on (e.g.
/// `*const T`, `T[]`) — in those cases there's nothing for the type
/// ref to be anchored at, so skipping is correct.
pub(crate) fn locate_fn_return_type(
    text: &str,
    name_span: Span,
    name: &str,
) -> Option<Span> {
    let off = line_col_to_offset(text, name_span.line, name_span.col)?;
    let bytes = text.as_bytes();
    let mut i = off + name.len();
    // Skip whitespace + optional generic argument list `<...>` —
    // we don't track nested generics for ret-type anchoring, so the
    // simple `<>` skip is enough for `fn foo<T>(...)`.
    while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
        i += 1;
    }
    if i < bytes.len() && bytes[i] == b'<' {
        let mut depth = 1usize;
        i += 1;
        while i < bytes.len() && depth > 0 {
            match bytes[i] {
                b'<' => depth += 1,
                b'>' => depth -= 1,
                _ => {}
            }
            i += 1;
        }
    }
    while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
        i += 1;
    }
    if i >= bytes.len() || bytes[i] != b'(' {
        return None;
    }
    // Skip the param list with paren-balance, ignoring nested brackets
    // because parameter default expressions can carry arbitrary tokens.
    let mut depth = 1usize;
    i += 1;
    while i < bytes.len() && depth > 0 {
        match bytes[i] {
            b'(' => depth += 1,
            b')' => depth -= 1,
            _ => {}
        }
        i += 1;
    }
    while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
        i += 1;
    }
    if i >= bytes.len() || bytes[i] != b':' {
        return None;
    }
    i += 1;
    while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
        i += 1;
    }
    if i >= bytes.len() {
        return None;
    }
    let b = bytes[i];
    if !b.is_ascii_alphabetic() && b != b'_' {
        return None;
    }
    let (line, col) = offset_to_line_col(text, i)?;
    Some(Span::new(line, col))
}

/// Locate the Nth base name in a class declaration's `: Base1, Base2`
/// list. `class_span` points at the `class` keyword.
pub(crate) fn locate_class_base_name(
    text: &str,
    class_span: Span,
    index: usize,
) -> Option<Span> {
    let off = line_col_to_offset(text, class_span.line, class_span.col)?;
    let bytes = text.as_bytes();
    let mut i = off;
    while i < bytes.len() && bytes[i] != b'{' {
        if bytes[i] == b':' {
            i += 1;
            break;
        }
        i += 1;
    }
    if i >= bytes.len() {
        return None;
    }
    let mut found = 0usize;
    while i < bytes.len() {
        while i < bytes.len()
            && (bytes[i] == b' ' || bytes[i] == b'\t' || bytes[i] == b',')
        {
            i += 1;
        }
        if i >= bytes.len() || bytes[i] == b'{' {
            return None;
        }
        if found == index {
            let b = bytes[i];
            if !b.is_ascii_alphabetic() && b != b'_' {
                return None;
            }
            let (line, col) = offset_to_line_col(text, i)?;
            return Some(Span::new(line, col));
        }
        found += 1;
        while i < bytes.len() && bytes[i] != b',' && bytes[i] != b'{' {
            i += 1;
        }
        if i < bytes.len() && bytes[i] == b'{' {
            return None;
        }
    }
    None
}

/// Locate the property name after a `get` or `set` keyword.
pub(crate) fn locate_property_name(text: &str, kw_span: Span, name: &str) -> Option<Span> {
    let off = line_col_to_offset(text, kw_span.line, kw_span.col)?;
    let bytes = text.as_bytes();
    // Skip 3 keyword chars (`get` / `set`).
    let mut i = off + 3;
    while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
        i += 1;
    }
    let nb = name.as_bytes();
    if bytes.len() - i >= nb.len() && &bytes[i..i + nb.len()] == nb {
        let next = bytes.get(i + nb.len()).copied().unwrap_or(b' ');
        if !next.is_ascii_alphanumeric() && next != b'_' {
            let (line, col) = offset_to_line_col(text, i)?;
            return Some(name_span(line, col, name));
        }
    }
    None
}

/// Locate the `name` token after a `let` keyword. The Stmt span points
/// at `let`, so we skip the keyword + whitespace to land on the binder.
pub(crate) fn locate_let_name(text: &str, stmt_span: Span, name: &str) -> Option<Span> {
    locate_let_name_with_kw(text, stmt_span, "let", name)
}

/// Same as `locate_let_name` but parameterised on the keyword length —
/// works for `use`, `let`, etc.
pub(crate) fn locate_let_name_with_kw(
    text: &str,
    kw_span: Span,
    kw: &str,
    name: &str,
) -> Option<Span> {
    let off = line_col_to_offset(text, kw_span.line, kw_span.col)?;
    let bytes = text.as_bytes();
    let mut i = off + kw.len();
    while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
        i += 1;
    }
    let nb = name.as_bytes();
    if bytes.len() - i >= nb.len() && &bytes[i..i + nb.len()] == nb {
        let next = bytes.get(i + nb.len()).copied().unwrap_or(b' ');
        if !next.is_ascii_alphanumeric() && next != b'_' {
            let (line, col) = offset_to_line_col(text, i)?;
            return Some(name_span(line, col, name));
        }
    }
    None
}

/// Locate the binding identifier inside an `if let some(<name>) = ...`
/// expression. The IfLet AST node only carries its outer span (the `if`
/// keyword), so we scan forward to the next `some(` and read the
/// identifier inside its parentheses.
pub(crate) fn locate_if_let_some_name(
    text: &str,
    if_span: Span,
    name: &str,
) -> Option<Span> {
    let off = line_col_to_offset(text, if_span.line, if_span.col)?;
    let bytes = text.as_bytes();
    let needle = b"some(";
    let mut i = off;
    while i + needle.len() <= bytes.len() {
        if &bytes[i..i + needle.len()] == needle {
            let mut j = i + needle.len();
            while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t') {
                j += 1;
            }
            let nb = name.as_bytes();
            if bytes.len() - j >= nb.len() && &bytes[j..j + nb.len()] == nb {
                let next = bytes.get(j + nb.len()).copied().unwrap_or(b' ');
                if !next.is_ascii_alphanumeric() && next != b'_' {
                    let (line, col) = offset_to_line_col(text, j)?;
                    return Some(name_span(line, col, name));
                }
            }
        }
        i += 1;
    }
    None
}

/// Read the identifier starting at `span.line` / `span.col` in `text`.
/// Returns `None` if the position isn't on an identifier character.
/// Used by the dotted-ref walker to learn which segment of a logical
/// dotted name the buffer literally starts with (e.g. `math` in
/// `math.abs()` even though the AST resolved the callee to
/// `std.math.abs` via an alias rewrite).
pub(crate) fn read_identifier_at(text: &str, span: Span) -> Option<String> {
    let offset = line_col_to_offset(text, span.line, span.col)?;
    let bytes = text.as_bytes();
    if offset >= bytes.len() {
        return None;
    }
    let first = bytes[offset];
    if !first.is_ascii_alphabetic() && first != b'_' {
        return None;
    }
    let mut end = offset;
    while end < bytes.len()
        && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_')
    {
        end += 1;
    }
    std::str::from_utf8(&bytes[offset..end]).ok().map(|s| s.to_string())
}

/// Find the `name` identifier that follows the next `.` after `obj_span`.
/// Returns its (line, col). Used to attach a precise span to `Field` and
/// `MethodCall` references whose AST nodes only carry the receiver's
/// span.
pub(crate) fn locate_dot_name(text: &str, obj_span: Span, name: &str) -> Option<(u32, u32)> {
    let offset = line_col_to_offset(text, obj_span.line, obj_span.col)?;
    let bytes = text.as_bytes();
    let mut i = offset;
    let mut paren_depth: i32 = 0;
    let mut bracket_depth: i32 = 0;
    while i < bytes.len() {
        let b = bytes[i];
        match b {
            b'(' => paren_depth += 1,
            b')' => paren_depth -= 1,
            b'[' => bracket_depth += 1,
            b']' => bracket_depth -= 1,
            b'.' if paren_depth <= 0 && bracket_depth <= 0 => {
                let mut j = i + 1;
                while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t') {
                    j += 1;
                }
                let nb = name.as_bytes();
                if bytes.len() - j >= nb.len() && &bytes[j..j + nb.len()] == nb {
                    let next = bytes.get(j + nb.len()).copied().unwrap_or(b' ');
                    if !next.is_ascii_alphanumeric() && next != b'_' {
                        return offset_to_line_col(text, j);
                    }
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// Find the bare identifier `name` inside a `use M { ... }` selective
/// import, starting at the `use` keyword's span. Returns the
/// (line, col) of the matching identifier, skipping content between
/// `use M` and the opening `{`. Stops at the closing `}`.
pub(crate) fn locate_selective_name(
    text: &str,
    use_span: Span,
    name: &str,
) -> Option<(u32, u32)> {
    let off = line_col_to_offset(text, use_span.line, use_span.col)?;
    let bytes = text.as_bytes();
    // Walk forward to the opening `{`.
    let mut i = off;
    while i < bytes.len() && bytes[i] != b'{' {
        if bytes[i] == b'\n' {
            // Selective-import braces are required on the same logical
            // line as the `use M` form; abandon if we hit EOL first.
            return None;
        }
        i += 1;
    }
    if i >= bytes.len() {
        return None;
    }
    i += 1; // step past `{`
    let nb = name.as_bytes();
    while i < bytes.len() && bytes[i] != b'}' {
        let b = bytes[i];
        // Identifier start: ASCII letter or `_`.
        if b.is_ascii_alphabetic() || b == b'_' {
            let start = i;
            let mut j = i;
            while j < bytes.len()
                && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_')
            {
                j += 1;
            }
            if &bytes[start..j] == nb {
                return offset_to_line_col(text, start);
            }
            i = j;
            continue;
        }
        i += 1;
    }
    None
}


/// Build a single-line span that covers the given identifier's full
/// extent (start at `(line, col)`, ending after `name` chars).
fn name_span(line: u32, col: u32, name: &str) -> Span {
    let chars = name.chars().count() as u32;
    let end_col = col + chars.saturating_sub(1);
    Span::range(line, col, line, end_col.max(col))
}
