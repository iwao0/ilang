//! Byte-offset → line/column / LSP `Position` conversion helpers.
//!
//! LSP wire types index by UTF-16 code units conceptually, but our
//! source files are pure UTF-8 and the editor side maps positions
//! the same way; counting one `character` per Rust `char` matches
//! the JS-side handling and the existing fixtures.

use tower_lsp::lsp_types::{Position, Range};

pub(crate) fn compute_line_starts(src: &str) -> Vec<usize> {
    let mut starts = vec![0usize];
    for (i, ch) in src.char_indices() {
        if ch == '\n' {
            starts.push(i + 1);
        }
    }
    starts
}

pub(crate) fn byte_range_to_lsp_range(text: &str, start: usize, end: usize) -> Range {
    let (s_line, s_col) = byte_to_line_col(text, start);
    let (e_line, e_col) = byte_to_line_col(text, end);
    Range {
        start: Position {
            line: s_line,
            character: s_col,
        },
        end: Position {
            line: e_line,
            character: e_col,
        },
    }
}

pub(crate) fn byte_to_line_col(text: &str, target: usize) -> (u32, u32) {
    let mut line = 0u32;
    let mut col = 0u32;
    let mut byte = 0usize;
    for ch in text.chars() {
        if byte >= target {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
        byte += ch.len_utf8();
    }
    (line, col)
}

pub(crate) fn byte_to_position(text: &str, target: usize) -> Position {
    let (line, character) = byte_to_line_col(text, target);
    Position { line, character }
}
