//! Token-aware document formatter for `.il`.
//!
//! Approach: re-lex the source, then walk tokens emitting canonical
//! whitespace between them while passing comments through verbatim.
//! The user's hand-written line breaks are respected (we never join
//! or split lines beyond collapsing 3+ blank lines into 1), so this
//! is a "mini" formatter — closer to gofmt's spirit than rustfmt's.
//!
//! What gets normalised:
//!   - 4-space indent recomputed from `{` / `}` depth
//!   - exactly one space after `,` / `;` / `:` (none before)
//!   - exactly one space around binary operators and `=` family
//!   - no space around `.`, `..`, `..=`, `::`
//!   - no space inside `(` / `[` boundaries
//!   - tabs flattened to spaces, trailing whitespace stripped, runs of
//!     3+ blank lines collapsed
//!
//! What the formatter intentionally avoids:
//!   - line wrapping / column-budget aware breaking
//!   - touching whitespace around `<` / `>` (ambiguous between
//!     comparison and generic brackets — leave the user's choice)
//!   - rewriting comments themselves
//!
//! On lex failure we return `None` so the LSP keeps the buffer as-is.
//!
//! The rules are intentionally conservative — output is meant to be a
//! superset of what users hand-write, not a strict reformat.
use ilang_lexer::{tokenize, Token, TokenKind};

const INDENT: &str = "    ";

pub fn format(src: &str) -> Option<String> {
    let tokens = tokenize(src).ok()?;
    let line_starts = compute_line_starts(src);
    let formatted = format_tokens(src, &tokens, &line_starts);
    if formatted == src {
        None
    } else {
        Some(formatted)
    }
}

fn compute_line_starts(src: &str) -> Vec<usize> {
    let mut starts = vec![0usize];
    for (i, ch) in src.char_indices() {
        if ch == '\n' {
            starts.push(i + 1);
        }
    }
    starts
}

/// `line` / `col` are 1-based; returns the byte offset of that
/// character in `src`. Walks the line from its start to count
/// characters (so multi-byte UTF-8 cols still resolve correctly).
fn line_col_to_byte(src: &str, line_starts: &[usize], line: u32, col: u32) -> usize {
    let line_idx = (line as usize).saturating_sub(1).min(line_starts.len().saturating_sub(1));
    let line_start = line_starts[line_idx];
    let target_col = col as usize;
    let mut byte = line_start;
    let mut current_col = 1usize;
    for ch in src[line_start..].chars() {
        if current_col >= target_col {
            return byte;
        }
        byte += ch.len_utf8();
        current_col += 1;
        if ch == '\n' {
            // Shouldn't happen for valid spans, but be safe.
            return byte;
        }
    }
    byte
}

fn token_byte_range(
    src: &str,
    line_starts: &[usize],
    tok: &Token,
) -> (usize, usize) {
    let start = line_col_to_byte(src, line_starts, tok.span.line, tok.span.col);
    let last_char_byte = line_col_to_byte(src, line_starts, tok.span.end_line, tok.span.end_col);
    let last_char = src[last_char_byte..].chars().next();
    let end = last_char_byte + last_char.map(|c| c.len_utf8()).unwrap_or(1);
    (start, end)
}

fn format_tokens(src: &str, tokens: &[Token], line_starts: &[usize]) -> String {
    let mut out = String::with_capacity(src.len());
    let mut depth: i32 = 0;
    let mut prev_end: usize = 0;
    let mut prev_kind: Option<&TokenKind> = None;
    let mut prev_prev_kind: Option<&TokenKind> = None;
    let mut at_line_start = true;

    for tok in tokens {
        if matches!(tok.kind, TokenKind::Eof) {
            // Trailing gap (after last code token).
            let trailing = &src[prev_end..];
            emit_gap(
                &mut out,
                trailing,
                depth,
                prev_kind,
                None,
                &mut at_line_start,
            );
            break;
        }
        let (start_byte, end_byte) = token_byte_range(src, line_starts, tok);
        let gap = &src[prev_end..start_byte];

        // The depth USED for indenting this token's leading
        // newline (if any) needs to outdent for `}` because the
        // close belongs to the outer block.
        let indent_depth = if matches!(tok.kind, TokenKind::RBrace) {
            (depth - 1).max(0)
        } else {
            depth.max(0)
        };

        emit_gap(
            &mut out,
            gap,
            indent_depth,
            prev_kind,
            Some((&tok.kind, prev_prev_kind)),
            &mut at_line_start,
        );

        // Emit the token's source slice verbatim — this preserves
        // numeric suffixes / hex / escapes / Unicode in identifiers.
        out.push_str(&src[start_byte..end_byte]);
        at_line_start = false;

        // Update brace depth from this token.
        match tok.kind {
            TokenKind::LBrace => depth += 1,
            TokenKind::RBrace => depth -= 1,
            _ => {}
        }
        prev_end = end_byte;
        prev_prev_kind = prev_kind;
        prev_kind = Some(&tok.kind);
    }

    // Final touch-ups: strip trailing whitespace per line, collapse
    // 3+ blank-line runs to 1, ensure exactly one trailing newline.
    finalize(&out)
}

/// Emit the run of whitespace + comments between two tokens (or
/// before the first / after the last). `next` is `Some((kind, prev_prev))`
/// when there's a following token; `None` for the final trailing gap.
fn emit_gap(
    out: &mut String,
    gap: &str,
    indent_depth: i32,
    prev: Option<&TokenKind>,
    next: Option<(&TokenKind, Option<&TokenKind>)>,
    at_line_start: &mut bool,
) {
    let items = scan_gap(gap);
    let has_newline = items
        .iter()
        .any(|i| matches!(i, GapItem::Newlines(n) if *n > 0));
    let has_comment = items.iter().any(|i| matches!(i, GapItem::Comment(_)));

    // Leading gap (no prev token): just emit comments + newlines
    // verbatim with indent. Used for files starting with comments.
    if prev.is_none() {
        emit_gap_with_breaks(out, &items, indent_depth, at_line_start);
        return;
    }

    if !has_newline && !has_comment {
        // Pure intra-line whitespace — apply canonical rule.
        if let Some((next_kind, prev_prev_kind)) = next {
            if needs_space(prev.unwrap(), next_kind, prev_prev_kind) {
                out.push(' ');
            }
        }
        return;
    }

    let trailing_inline_comment =
        emit_gap_with_breaks(out, &items, indent_depth, at_line_start);
    if trailing_inline_comment {
        if let Some((next_kind, _)) = next {
            // Mirror `needs_space`'s "tight to the left" rules so
            // a trailing block comment doesn't push a space before
            // a close-bracket / separator / chain operator.
            if !matches!(
                next_kind,
                TokenKind::RParen
                    | TokenKind::RBracket
                    | TokenKind::RBrace
                    | TokenKind::Comma
                    | TokenKind::Semicolon
                    | TokenKind::Colon
                    | TokenKind::ColonColon
                    | TokenKind::Dot
                    | TokenKind::DotDot
                    | TokenKind::DotDotEq
                    | TokenKind::Question
            ) {
                out.push(' ');
            }
        }
    }
}

/// Returns `true` if the gap ended with a block comment that has
/// no following newline — caller should add a space before the
/// next token unless that token is a close-bracket / separator.
fn emit_gap_with_breaks(
    out: &mut String,
    items: &[GapItem],
    indent_depth: i32,
    at_line_start: &mut bool,
) -> bool {
    let mut just_newlined = false;
    let mut last_inline_comment = false;
    for item in items {
        match item {
            GapItem::Newlines(n) => {
                let want = (*n).min(2); // cap blank-line runs
                for _ in 0..want {
                    // Strip any trailing spaces before the newline.
                    while out.ends_with(' ') {
                        out.pop();
                    }
                    out.push('\n');
                }
                if want > 0 {
                    just_newlined = true;
                    *at_line_start = true;
                    last_inline_comment = false;
                }
            }
            GapItem::Comment(text) => {
                if just_newlined {
                    push_indent(out, indent_depth);
                    just_newlined = false;
                    *at_line_start = false;
                } else if !out.is_empty() && !out.ends_with(' ') && !out.ends_with('\n') {
                    out.push(' ');
                }
                out.push_str(text);
                // Line comments are always followed by a newline
                // (the lexer's whitespace consumes the `\n` into
                // the next gap), so they never need an inline
                // trailing space. Block comments DO when they sit
                // mid-expression.
                last_inline_comment = !text.starts_with("//");
            }
        }
    }
    if just_newlined {
        push_indent(out, indent_depth);
    }
    last_inline_comment
}

fn push_indent(out: &mut String, depth: i32) {
    for _ in 0..depth.max(0) {
        out.push_str(INDENT);
    }
}

#[derive(Debug)]
enum GapItem {
    /// Number of newlines in this run of whitespace.
    Newlines(u32),
    /// A comment, including its `//` / `/* ... */` delimiters.
    Comment(String),
}

/// Walk the gap text (whitespace + comments) and produce a flat
/// list of items in source order. Whitespace runs are collapsed
/// into `Newlines(n)` (n = newline count); comments come through
/// verbatim.
fn scan_gap(gap: &str) -> Vec<GapItem> {
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
fn needs_space(
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

/// Strip per-line trailing whitespace, collapse 3+ blank-line
/// runs to one blank, and ensure exactly one trailing newline.
fn finalize(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut blank_run: u32 = 0;
    for line in s.split('\n') {
        let trimmed = line.trim_end_matches([' ', '\t']);
        if trimmed.is_empty() {
            blank_run += 1;
            if blank_run <= 2 {
                // up to one blank line (=> at most 2 consecutive '\n')
                out.push_str("\n");
            }
        } else {
            blank_run = 0;
            out.push_str(trimmed);
            out.push('\n');
        }
    }
    // Trim trailing blanks so file ends with exactly one newline.
    while out.ends_with("\n\n") {
        out.pop();
    }
    if !out.ends_with('\n') && !out.is_empty() {
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::format;

    fn fmt(src: &str) -> String {
        format(src).unwrap_or_else(|| src.to_string())
    }

    #[test]
    fn already_canonical() {
        let src = "fn main() {\n    let x = 1\n    x + 2\n}\n";
        assert_eq!(format(src), None);
    }

    #[test]
    fn fixes_indent() {
        let src = "fn main() {\nlet x = 1\nx + 2\n}\n";
        let want = "fn main() {\n    let x = 1\n    x + 2\n}\n";
        assert_eq!(fmt(src), want);
    }

    #[test]
    fn normalizes_assignment_spacing() {
        let src = "let x=1\n";
        let want = "let x = 1\n";
        assert_eq!(fmt(src), want);
    }

    #[test]
    fn normalizes_comma_spacing() {
        let src = "fn f(a:i64,b:i64):i64{a+b}\n";
        let want = "fn f(a: i64, b: i64): i64 { a + b }\n";
        assert_eq!(fmt(src), want);
    }

    #[test]
    fn no_space_inside_parens() {
        let src = "let x = ( 1 + 2 )\n";
        let want = "let x = (1 + 2)\n";
        assert_eq!(fmt(src), want);
    }

    #[test]
    fn no_space_around_dot() {
        let src = "let n = obj . field\n";
        let want = "let n = obj.field\n";
        assert_eq!(fmt(src), want);
    }

    #[test]
    fn collapses_extra_spaces() {
        let src = "let   x   =   1\n";
        let want = "let x = 1\n";
        assert_eq!(fmt(src), want);
    }

    #[test]
    fn preserves_line_comment() {
        let src = "let x = 1 // value\nlet y = 2\n";
        // Already canonical — formatter should be a no-op.
        assert_eq!(format(src), None);
    }

    #[test]
    fn block_comment_kept_inline() {
        let src = "let x = 1 /* note */ + 2\n";
        assert_eq!(format(src), None);
    }

    #[test]
    fn collapses_blank_runs() {
        let src = "let x = 1\n\n\n\nlet y = 2\n";
        let want = "let x = 1\n\nlet y = 2\n";
        assert_eq!(fmt(src), want);
    }

    #[test]
    fn closing_brace_outdents() {
        let src = "fn f() {\n    let x = 1\n    }\n";
        let want = "fn f() {\n    let x = 1\n}\n";
        assert_eq!(fmt(src), want);
    }

    #[test]
    fn range_no_space() {
        let src = "for i in 1 .. 5 { }\n";
        // Empty `{ }` block bodies stay as-is (block braces keep
        // a space inside; collapsing to `{}` would diverge from
        // the rest of the codebase's style).
        let want = "for i in 1..5 { }\n";
        assert_eq!(fmt(src), want);
    }

    #[test]
    fn optional_suffix_no_space() {
        let src = "let a : i64 ? = none\n";
        let want = "let a: i64? = none\n";
        assert_eq!(fmt(src), want);
    }

    #[test]
    fn unary_minus_no_space() {
        let src = "let x = -1\nlet y = a + -b\n";
        // Both are well-formed; formatter shouldn't insert a stray
        // space after the unary `-`.
        assert_eq!(format(src), None);
    }

    #[test]
    fn idempotent_on_messy_input() {
        let src = "class Foo{\na:i32\nb:string\ninit(x:i32,y:string){this.a=x;this.b=y}\ngreet():string{\"hi \"+this.b}\n}\nlet f=new Foo(1,\"a\")\nfor i in 0..10{console.log(i)}\n";
        let once = format(src).expect("first pass should reformat");
        let twice = format(&once);
        assert_eq!(
            twice, None,
            "second pass should be a no-op (idempotent), got: {twice:?}"
        );
    }

    #[test]
    fn preserves_inline_block_comment() {
        let src = "let x = 1 /* note */ + 2\n";
        // Already canonical — should be a no-op.
        assert_eq!(format(src), None);
    }

    #[test]
    fn tabs_become_four_spaces() {
        let src = "fn main() {\n\tlet x = 1\n}\n";
        let want = "fn main() {\n    let x = 1\n}\n";
        assert_eq!(fmt(src), want);
    }

    #[test]
    fn nested_braces() {
        let src = "fn outer() {\nfn inner() {\nlet x = 1\n}\n}\n";
        let want = "fn outer() {\n    fn inner() {\n        let x = 1\n    }\n}\n";
        assert_eq!(fmt(src), want);
    }

    #[test]
    fn ignores_braces_in_strings() {
        let src = "let s = \"a {b} c\"\nlet t = 1\n";
        assert_eq!(format(src), None);
    }

    #[test]
    fn ignores_braces_in_line_comments() {
        let src = "// what about { this }\nlet t = 1\n";
        assert_eq!(format(src), None);
    }

    #[test]
    fn ignores_braces_in_block_comments() {
        let src = "/* { not\n   a } block */\nlet t = 1\n";
        assert_eq!(format(src), None);
    }

    /// Walks every `.il` file under the cargo workspace, runs the
    /// formatter, and verifies the formatted output still tokenises
    /// cleanly. Catches whole-file regressions where the formatter
    /// loses or shifts syntactically meaningful tokens.
    #[test]
    fn formatter_preserves_lexability_on_corpus() {
        use std::path::PathBuf;
        // Workspace root is the parent of this crate's manifest.
        let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let workspace = crate_dir.parent().and_then(|p| p.parent()).unwrap();
        let mut paths: Vec<PathBuf> = Vec::new();
        collect_il(workspace, &mut paths);
        let mut tested = 0usize;
        for p in paths {
            let src = match std::fs::read_to_string(&p) {
                Ok(s) => s,
                Err(_) => continue,
            };
            // Original must parse; otherwise nothing to compare.
            if ilang_lexer::tokenize(&src).is_err() {
                continue;
            }
            let formatted = format(&src).unwrap_or_else(|| src.clone());
            let orig_toks =
                ilang_lexer::tokenize(&src).expect("orig tokenises");
            let fmt_toks = match ilang_lexer::tokenize(&formatted) {
                Ok(t) => t,
                Err(_) => panic!(
                    "formatter broke lex of {}: \n{}",
                    p.display(),
                    formatted
                ),
            };
            // Token kinds must line up — formatter can't drop or
            // invent any token. Spans and `leading_newline` may
            // shift legitimately, so compare kinds only.
            let orig_kinds: Vec<_> =
                orig_toks.iter().map(|t| &t.kind).collect();
            let fmt_kinds: Vec<_> =
                fmt_toks.iter().map(|t| &t.kind).collect();
            assert_eq!(
                orig_kinds,
                fmt_kinds,
                "token sequence diverged in {}",
                p.display()
            );
            // Idempotency on real code.
            let again = format(&formatted);
            assert!(
                again.is_none(),
                "format not idempotent on {}: {:?}",
                p.display(),
                again
            );
            tested += 1;
        }
        assert!(tested > 0, "didn't find any .il fixtures");
    }

    fn collect_il(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        for entry in rd.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if path.file_name().and_then(|n| n.to_str()) == Some("target") {
                    continue;
                }
                collect_il(&path, out);
            } else if path.extension().and_then(|e| e.to_str()) == Some("il") {
                out.push(path);
            }
        }
    }
}
