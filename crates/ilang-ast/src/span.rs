/// Source position attached to AST nodes and errors. Lines and columns are
/// 1-based and inclusive on both ends. `line` / `col` mark the start; the
/// extent of the token / construct is `end_line` / `end_col`. For a
/// single-character span (or when the end position is unknown), the end
/// equals the start. `Span::dummy()` is used by tests that compare AST
/// values without caring about the exact position (the AST's `PartialEq`
/// ignores spans).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    pub line: u32,
    pub col: u32,
    pub end_line: u32,
    pub end_col: u32,
}

impl Span {
    /// Construct a single-position span (start == end). Use this when only
    /// the start position is known.
    pub const fn new(line: u32, col: u32) -> Self {
        Self {
            line,
            col,
            end_line: line,
            end_col: col,
        }
    }

    /// Construct a range span. `end_line` / `end_col` mark the position of
    /// the last character that belongs to this span (inclusive).
    pub const fn range(line: u32, col: u32, end_line: u32, end_col: u32) -> Self {
        Self {
            line,
            col,
            end_line,
            end_col,
        }
    }

    pub const fn dummy() -> Self {
        Self {
            line: 0,
            col: 0,
            end_line: 0,
            end_col: 0,
        }
    }

    /// Returns `true` if start == end (no extent recorded).
    pub const fn is_point(&self) -> bool {
        self.line == self.end_line && self.col == self.end_col
    }

    /// Build a span that starts where `self` starts and ends where
    /// `other` ends. Used by the parser to extend a leading token's
    /// span to cover everything up through the trailing one (e.g.
    /// the LHS span widened by the RHS span of a binary expression).
    pub const fn to(&self, other: Span) -> Span {
        Span {
            line: self.line,
            col: self.col,
            end_line: other.end_line,
            end_col: other.end_col,
        }
    }
}

impl std::fmt::Display for Span {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_point() {
            write!(f, "[{}:{}]", self.line, self.col)
        } else {
            write!(
                f,
                "[{}:{}-{}:{}]",
                self.line, self.col, self.end_line, self.end_col
            )
        }
    }
}
