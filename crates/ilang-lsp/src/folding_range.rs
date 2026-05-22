//! `textDocument/foldingRange` provider.
//!
//! Approach: re-lex the source and pair up `{` / `}` tokens. Every
//! pair that spans more than one source line becomes a foldable
//! region. Multi-line `use M { … }` imports surface with the
//! `Imports` kind so editors can fold the import block separately.
//!
//! Brace pairing comes off the lexer's own bracket counts — strings
//! and comments are already skipped there, so we don't have to
//! re-implement that state machine.

use ilang_lexer::{tokenize, Token, TokenKind};
use tower_lsp::lsp_types::{FoldingRange, FoldingRangeKind};

pub(crate) fn build(text: &str) -> Vec<FoldingRange> {
    let Ok(tokens) = tokenize(text) else { return Vec::new() };
    let mut out: Vec<FoldingRange> = Vec::new();
    let mut stack: Vec<OpenBrace> = Vec::new();

    for (i, tok) in tokens.iter().enumerate() {
        match tok.kind {
            TokenKind::LBrace => {
                stack.push(OpenBrace {
                    line: tok.span.line,
                    // Track whether this `{` opens an `use M { … }`
                    // import block by checking the previous tokens.
                    is_import_block: is_after_use_module(&tokens, i),
                });
            }
            TokenKind::RBrace => {
                if let Some(open) = stack.pop() {
                    let close_line = tok.span.line;
                    if close_line > open.line {
                        out.push(FoldingRange {
                            start_line:      open.line.saturating_sub(1),
                            start_character: None,
                            // Stop one line short of the closing
                            // `}` so it stays visible after the
                            // fold collapses.
                            end_line:        close_line.saturating_sub(2),
                            end_character:   None,
                            kind:            if open.is_import_block {
                                Some(FoldingRangeKind::Imports)
                            } else {
                                None
                            },
                            collapsed_text:  None,
                        });
                    }
                }
            }
            _ => {}
        }
    }
    out
}

struct OpenBrace {
    line:            u32,
    is_import_block: bool,
}

/// `true` when the `{` at `tokens[idx]` is preceded (skipping
/// whitespace) by `use IDENT` — the selective-import opener
/// (`use foundation { NSObject, NSWindow }`).
fn is_after_use_module(tokens: &[Token], idx: usize) -> bool {
    if idx < 2 {
        return false;
    }
    let prev = &tokens[idx - 1];
    let prev2 = &tokens[idx - 2];
    matches!(prev.kind, TokenKind::Ident(_))
        && matches!(prev2.kind, TokenKind::Use)
}
