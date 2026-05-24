use crate::intern::Symbol;

/// Source position attached to AST nodes and errors. Lines and columns are
/// 1-based and inclusive on both ends. `line` / `col` mark the start; the
/// extent of the token / construct is `end_line` / `end_col`. For a
/// single-character span (or when the end position is unknown), the end
/// equals the start. `Span::dummy()` is used by tests that compare AST
/// values without caring about the exact position (the AST's `PartialEq`
/// ignores spans).
///
/// `source_file` records the path the span came from. Lexer / parser
/// leave it as `Symbol::intern("")` (no path known); the loader sweeps
/// every span in a freshly-parsed Program and stamps it with the
/// canonical file path so cross-module errors don't read as if they
/// came from the entry file (`<entry> [231:34]: undefined class
/// "appkit.NSPoint"` when the actual offender lives in `appkit.il`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    pub line: u32,
    pub col: u32,
    pub end_line: u32,
    pub end_col: u32,
    pub source_file: Symbol,
}

impl Span {
    /// Construct a single-position span (start == end). Use this when only
    /// the start position is known.
    pub fn new(line: u32, col: u32) -> Self {
        Self {
            line,
            col,
            end_line: line,
            end_col: col,
            source_file: Symbol::intern(""),
        }
    }

    /// Construct a range span. `end_line` / `end_col` mark the position of
    /// the last character that belongs to this span (inclusive).
    pub fn range(line: u32, col: u32, end_line: u32, end_col: u32) -> Self {
        Self {
            line,
            col,
            end_line,
            end_col,
            source_file: Symbol::intern(""),
        }
    }

    pub fn dummy() -> Self {
        Self {
            line: 0,
            col: 0,
            end_line: 0,
            end_col: 0,
            source_file: Symbol::intern(""),
        }
    }

    /// Returns `true` if start == end (no extent recorded).
    pub const fn is_point(&self) -> bool {
        self.line == self.end_line && self.col == self.end_col
    }

    /// `true` when `source_file` hasn't been set (empty string). The
    /// loader stamps real spans with their canonical path; the
    /// remaining empty entries are either the entry file's own spans
    /// or compiler-synthesised AST nodes that borrowed someone
    /// else's span.
    pub fn has_source_file(&self) -> bool {
        self.source_file.as_str() != ""
    }

    /// Build a span that starts where `self` starts and ends where
    /// `other` ends. Used by the parser to extend a leading token's
    /// span to cover everything up through the trailing one (e.g.
    /// the LHS span widened by the RHS span of a binary expression).
    pub fn to(&self, other: Span) -> Span {
        Span {
            line: self.line,
            col: self.col,
            end_line: other.end_line,
            end_col: other.end_col,
            source_file: self.source_file,
        }
    }
}

impl std::fmt::Display for Span {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Always render just the start position. The end position is
        // kept on the struct for editors that highlight ranges
        // (`span_full_to_range` in the LSP), but in CLI diagnostics
        // the `start-end` form was noisy without adding actionable
        // information.
        write!(f, "[{}:{}]", self.line, self.col)
    }
}
