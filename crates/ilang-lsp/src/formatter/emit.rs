//! Token-level emitter. `format_tokens` walks the (possibly
//! match-broken) token stream and rebuilds the source with canonical
//! indentation and inter-token gaps, delegating per-gap spacing
//! decisions to the [`spacing`](super::spacing) module and the final
//! trailing-whitespace cleanup to `rewrap::finalize`.

use ilang_lexer::{Token, TokenKind};

use super::rewrap::finalize;
use super::spacing::{needs_space, scan_gap, GapItem};
use super::{line_col_to_byte, INDENT};


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

pub(super) fn format_tokens(src: &str, tokens: &[Token], line_starts: &[usize]) -> String {
    let mut out = String::with_capacity(src.len());
    let mut brace_depth: i32 = 0;
    // Indent for unclosed `(...)` / `[...]` whose open spans a
    // newline before its close. Each such open bumps depth by 1
    // until matched. Single-line `f(a, b)` doesn't bump.
    let mut paren_depth: i32 = 0;
    let mut paren_stack: Vec<bool> = Vec::new();
    let mut prev_end: usize = 0;
    let mut prev_kind: Option<&TokenKind> = None;
    let mut prev_prev_kind: Option<&TokenKind> = None;
    let mut at_line_start = true;
    // `@objc(...)` attribute tracking. Set when we see the leading
    // `@ objc (` triple; each nested `(` bumps, each `)` decrements;
    // when it returns to 0 the matching close of the attribute has
    // been emitted and we force a newline before the next token so
    // `@objc("sel") pub release()` always reflows to two lines.
    let mut objc_attr_depth: i32 = 0;
    let mut just_closed_objc_attr = false;

    for (idx, tok) in tokens.iter().enumerate() {
        if matches!(tok.kind, TokenKind::Eof) {
            // Trailing gap (after last code token).
            let trailing = &src[prev_end..];
            emit_gap(
                &mut out,
                trailing,
                brace_depth + paren_depth,
                prev_kind,
                None,
                &mut at_line_start,
            );
            break;
        }
        let (start_byte, end_byte) = token_byte_range(src, line_starts, tok);
        let gap = &src[prev_end..start_byte];

        let total_depth = brace_depth + paren_depth;
        // `}` closes a brace-block — outdent before printing.
        // Likewise the matching close of a multi-line `(...)` /
        // `[...]` should sit at the OUTER indent.
        let closing_multiline_paren = matches!(
            tok.kind,
            TokenKind::RParen | TokenKind::RBracket
        ) && paren_stack.last().copied().unwrap_or(false);
        let indent_depth = if matches!(tok.kind, TokenKind::RBrace)
            || closing_multiline_paren
        {
            (total_depth - 1).max(0)
        } else {
            total_depth.max(0)
        };

        // If the previous gap closed an `@objc(...)` attribute and
        // the user didn't already put a newline before this token,
        // force one so the next item (method header, class header,
        // etc.) sits on its own indented line.
        let force_break = just_closed_objc_attr
            && !matches!(tok.kind, TokenKind::At)
            && !gap.contains('\n');
        if force_break {
            // Strip any trailing space that emit_gap may have added.
            while out.ends_with(' ') {
                out.pop();
            }
            out.push('\n');
            push_indent(&mut out, indent_depth);
        } else {
            emit_gap(
                &mut out,
                gap,
                indent_depth,
                prev_kind,
                Some((&tok.kind, prev_prev_kind)),
                &mut at_line_start,
            );
        }
        just_closed_objc_attr = false;

        // Emit the token's source slice verbatim — this preserves
        // numeric suffixes / hex / escapes / Unicode in identifiers.
        out.push_str(&src[start_byte..end_byte]);
        at_line_start = false;

        // Update depth tracking from this token.
        match tok.kind {
            TokenKind::LBrace => brace_depth += 1,
            TokenKind::RBrace => brace_depth -= 1,
            TokenKind::LParen | TokenKind::LBracket => {
                // Multi-line if the next non-EOF token starts on a
                // later line than this open delimiter.
                let multi = tokens
                    .get(idx + 1)
                    .map(|t| t.span.line > tok.span.end_line)
                    .unwrap_or(false);
                paren_stack.push(multi);
                if multi {
                    paren_depth += 1;
                }
                if objc_attr_depth > 0 && matches!(tok.kind, TokenKind::LParen) {
                    objc_attr_depth += 1;
                } else if matches!(tok.kind, TokenKind::LParen)
                    && is_objc_attr_open(prev_kind, prev_prev_kind)
                {
                    objc_attr_depth = 1;
                }
            }
            TokenKind::RParen | TokenKind::RBracket => {
                let was_multi = paren_stack.pop().unwrap_or(false);
                if was_multi {
                    paren_depth -= 1;
                }
                if objc_attr_depth > 0 && matches!(tok.kind, TokenKind::RParen) {
                    objc_attr_depth -= 1;
                    if objc_attr_depth == 0 {
                        just_closed_objc_attr = true;
                    }
                }
            }
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

/// `( ` just observed; was the run `@ objc (`? Used by the formatter
/// to recognise the start of an `@objc("selector:")` attribute and
/// force a newline once its matching `)` closes.
fn is_objc_attr_open(prev: Option<&TokenKind>, prev_prev: Option<&TokenKind>) -> bool {
    matches!(prev, Some(TokenKind::Ident(n)) if n.as_str() == "objc")
        && matches!(prev_prev, Some(TokenKind::At))
}

fn push_indent(out: &mut String, depth: i32) {
    for _ in 0..depth.max(0) {
        out.push_str(INDENT);
    }
}
