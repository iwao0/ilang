//! Inter-token spacing rules. `scan_gap` classifies the original gap
//! between two tokens into newline / comment / blank items, and
//! `needs_space` decides whether canonical formatting wants a single
//! space between two adjacent tokens.

use ilang_lexer::TokenKind;


#[derive(Debug)]
pub(super) enum GapItem {
    /// Number of newlines in this run of whitespace.
    Newlines(u32),
    /// A comment, including its `//` / `/* ... */` delimiters.
    Comment(String),
}

/// Walk the gap text (whitespace + comments) and produce a flat
/// list of items in source order. Whitespace runs are collapsed
/// into `Newlines(n)` (n = newline count); comments come through
/// verbatim.
pub(super) fn scan_gap(gap: &str) -> Vec<GapItem> {
    let bytes = gap.as_bytes();
    let mut items: Vec<GapItem> = Vec::new();
    let mut i = 0usize;
    let mut newlines: u32 = 0;
    let mut in_ws = true;
    while i < bytes.len() {
        let c = bytes[i];
        if c == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
            if newlines > 0 || in_ws {
                items.push(GapItem::Newlines(newlines));
                newlines = 0;
            }
            // Line comment to end of line (no newline char yet).
            let start = i;
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            items.push(GapItem::Comment(
                std::str::from_utf8(&bytes[start..i])
                    .unwrap_or("")
                    .trim_end_matches([' ', '\t'])
                    .to_string(),
            ));
            in_ws = false;
            continue;
        }
        if c == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'*' {
            if newlines > 0 || in_ws {
                items.push(GapItem::Newlines(newlines));
                newlines = 0;
            }
            // Block comment, possibly nested.
            let start = i;
            i += 2;
            let mut depth: u32 = 1;
            while i < bytes.len() && depth > 0 {
                if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'*' {
                    depth += 1;
                    i += 2;
                } else if i + 1 < bytes.len() && bytes[i] == b'*' && bytes[i + 1] == b'/' {
                    depth -= 1;
                    i += 2;
                } else {
                    i += 1;
                }
            }
            items.push(GapItem::Comment(
                std::str::from_utf8(&bytes[start..i]).unwrap_or("").to_string(),
            ));
            in_ws = false;
            continue;
        }
        if c == b'\n' {
            newlines += 1;
            in_ws = true;
            i += 1;
        } else if c == b' ' || c == b'\t' || c == b'\r' {
            in_ws = true;
            i += 1;
        } else {
            // Shouldn't happen for a well-formed gap (lexer would
            // have eaten anything non-comment / non-whitespace),
            // but be defensive.
            i += 1;
        }
    }
    if in_ws && newlines > 0 {
        items.push(GapItem::Newlines(newlines));
    } else if newlines > 0 {
        items.push(GapItem::Newlines(newlines));
    }
    items
}

/// Decide whether a single space goes between `prev` and `next`
/// when no newline / comment intervenes. `prev_prev` lets us
/// distinguish unary `-` / `+` (preceded by an op-like token)
/// from binary ones (preceded by an expression-end token).
pub(super) fn needs_space(
    prev: &TokenKind,
    next: &TokenKind,
    prev_prev: Option<&TokenKind>,
) -> bool {
    use TokenKind::*;

    // No space inside open / before close parens & brackets.
    // Braces (`{` / `}`) keep a space — block bodies on a single
    // line look like `{ expr }` in this codebase. Empty blocks
    // `{}` likewise pass through unchanged.
    if matches!(prev, LParen | LBracket) {
        return false;
    }
    if matches!(next, RParen | RBracket) {
        return false;
    }

    // Separators bind tight to the left.
    if matches!(next, Comma | Semicolon | Colon | ColonColon) {
        return false;
    }

    // No space around `.` / `..` / `..=` / `::`.
    if matches!(prev, Dot | DotDot | DotDotEq | ColonColon) {
        return false;
    }
    if matches!(next, Dot | DotDot | DotDotEq) {
        return false;
    }

    // No space before `?` (Optional type / `as?`).
    if matches!(next, Question) {
        return false;
    }

    // Suffix `?` followed by another suffix / atom expects no
    // space when it's still part of a type (`A?[]`, `A?` alone).
    // Without proper context this would be misclassified, so
    // leave the default (space) to keep things readable.
    let _ = prev; // silence unused warning when handled below

    // Unary `!` / `~` / prefix `-` / `+`: no space after.
    if matches!(prev, Bang | Tilde) {
        return false;
    }
    // Attribute prefix `@` binds tight to the following ident
    // (`@flags`, `@extern`, `@lib`).
    if matches!(prev, At) {
        return false;
    }
    if matches!(prev, Minus | Plus)
        && prev_prev.map(|p| !is_expression_end(p)).unwrap_or(true)
    {
        return false;
    }

    // Function call / indexing: no space between expression-end and `(` / `[`.
    if matches!(next, LParen | LBracket) {
        if prev_kind_is_callable(prev) {
            return false;
        }
        return true;
    }

    // Default: one space.
    true
}

/// True for tokens that can sit at the right edge of an
/// expression — i.e. before a binary operator or before a
/// function-call `(`. Used to disambiguate prefix vs binary
/// `-` / `+` and to decide whether `(` opens a call.
fn is_expression_end(t: &TokenKind) -> bool {
    use TokenKind::*;
    matches!(
        t,
        Ident(_)
            | Int(_)
            | Float(_)
            | Str(_)
            | True
            | False
            | This
            | RParen
            | RBracket
            | RBrace
            | Question
    )
}

fn prev_kind_is_callable(t: &TokenKind) -> bool {
    use TokenKind::*;
    matches!(t, Ident(_) | RParen | RBracket | RBrace | This | Super)
}
