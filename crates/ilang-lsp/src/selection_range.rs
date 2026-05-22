//! `textDocument/selectionRange` provider.
//!
//! Token-based expansion: for each requested position we build a
//! chain
//!     identifier-at-cursor
//!     → innermost `(`/`[`/`{` group containing cursor
//!     → next outer group
//!     → ...
//!     → whole document
//!
//! The chain is robust against the AST's incomplete `end_line` /
//! `end_col` data — bracket pairs come straight off the lexer, so
//! every step has a real source range.

use ilang_lexer::{tokenize, Token, TokenKind};
use tower_lsp::lsp_types::{Position, Range, SelectionRange};

pub(crate) fn build_for(text: &str, positions: &[Position]) -> Vec<SelectionRange> {
    let Ok(tokens) = tokenize(text) else {
        return positions.iter().map(|p| singleton(*p)).collect();
    };
    let pairs = build_bracket_pairs(&tokens);
    positions
        .iter()
        .map(|p| chain_for(text, &tokens, &pairs, *p))
        .collect()
}

fn singleton(p: Position) -> SelectionRange {
    SelectionRange {
        range:  Range { start: p, end: p },
        parent: None,
    }
}

/// A matched bracket pair on the source: open and close span
/// positions (1-based line / col), used to drive selection-range
/// chains. Sorted later by area so smaller pairs come first.
#[derive(Clone, Copy, Debug)]
struct BracketPair {
    open_line: u32,
    open_col:  u32,
    /// Position of the matching close token (exclusive end — the
    /// character past the close bracket).
    close_line: u32,
    close_col:  u32,
}

fn build_bracket_pairs(tokens: &[Token]) -> Vec<BracketPair> {
    let mut stack: Vec<(u32, u32, TokenKind)> = Vec::new();
    let mut out = Vec::new();
    for tok in tokens {
        match tok.kind {
            TokenKind::LParen | TokenKind::LBracket | TokenKind::LBrace => {
                stack.push((tok.span.line, tok.span.col, tok.kind.clone()));
            }
            TokenKind::RParen | TokenKind::RBracket | TokenKind::RBrace => {
                if let Some((ol, oc, _ok)) = stack.pop() {
                    out.push(BracketPair {
                        open_line:  ol,
                        open_col:   oc,
                        close_line: tok.span.line,
                        // `+ 1` so the range includes the closing
                        // character itself.
                        close_col:  tok.span.col + 1,
                    });
                }
            }
            _ => {}
        }
    }
    out
}

fn chain_for(
    text: &str,
    tokens: &[Token],
    pairs: &[BracketPair],
    pos: Position,
) -> SelectionRange {
    let target_line = pos.line + 1;
    let target_col = pos.character + 1;

    // Smallest scope first: the identifier / literal / token under
    // the cursor.
    let mut ranges: Vec<Range> = Vec::new();
    if let Some(tok_range) = token_at(tokens, target_line, target_col) {
        ranges.push(tok_range);
    }

    // Bracket pairs ranked by area so the innermost containing
    // pair comes first.
    let mut containing: Vec<&BracketPair> = pairs
        .iter()
        .filter(|p| pair_contains(p, target_line, target_col))
        .collect();
    containing.sort_by_key(|p| pair_area(p));
    for p in containing {
        ranges.push(Range {
            start: Position {
                line:      p.open_line.saturating_sub(1),
                character: p.open_col.saturating_sub(1),
            },
            end: Position {
                line:      p.close_line.saturating_sub(1),
                character: p.close_col.saturating_sub(1),
            },
        });
    }

    // Last: whole document.
    if let Some(doc_range) = whole_document_range(text) {
        ranges.push(doc_range);
    }

    // De-dupe consecutive identical ranges (could happen if a
    // bracket pair touches the token).
    ranges.dedup();

    if ranges.is_empty() {
        return singleton(pos);
    }
    let mut iter = ranges.into_iter();
    let mut node = SelectionRange {
        range:  iter.next().unwrap(),
        parent: None,
    };
    for r in iter {
        node = SelectionRange {
            range:  r,
            parent: Some(Box::new(node)),
        };
    }
    node
}

fn pair_contains(p: &BracketPair, line: u32, col: u32) -> bool {
    let after_start = (line, col) >= (p.open_line, p.open_col);
    let before_end = (line, col) <= (p.close_line, p.close_col);
    after_start && before_end
}

fn pair_area(p: &BracketPair) -> u64 {
    let dl = p.close_line.saturating_sub(p.open_line) as u64;
    let dc = if p.close_line == p.open_line {
        p.close_col.saturating_sub(p.open_col) as u64
    } else {
        // Cross-line: factor lines first, columns second.
        p.close_col as u64
    };
    dl * 1_000_000 + dc
}

fn token_at(tokens: &[Token], line: u32, col: u32) -> Option<Range> {
    for tok in tokens {
        let TokenKind::Ident(name) = &tok.kind else { continue };
        if tok.span.line != line {
            continue;
        }
        let start = tok.span.col;
        let end = start + name.len() as u32;
        if col >= start && col <= end {
            return Some(Range {
                start: Position {
                    line:      tok.span.line.saturating_sub(1),
                    character: start.saturating_sub(1),
                },
                end: Position {
                    line:      tok.span.line.saturating_sub(1),
                    character: end.saturating_sub(1),
                },
            });
        }
    }
    None
}

fn whole_document_range(text: &str) -> Option<Range> {
    let line_count = text.lines().count() as u32;
    if line_count == 0 {
        return None;
    }
    let last_line_len = text.lines().last().map(|l| l.len() as u32).unwrap_or(0);
    Some(Range {
        start: Position { line: 0, character: 0 },
        end: Position {
            line:      line_count.saturating_sub(1),
            character: last_line_len,
        },
    })
}
