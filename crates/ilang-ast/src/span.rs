/// Source position attached to AST nodes and errors. Lines and columns are
/// 1-based; `Span::dummy()` is used by tests that compare AST values without
/// caring about the exact position (the AST's `PartialEq` ignores spans).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    pub line: u32,
    pub col: u32,
}

impl Span {
    pub const fn new(line: u32, col: u32) -> Self {
        Self { line, col }
    }

    pub const fn dummy() -> Self {
        Self { line: 0, col: 0 }
    }
}

impl std::fmt::Display for Span {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}:{}]", self.line, self.col)
    }
}
