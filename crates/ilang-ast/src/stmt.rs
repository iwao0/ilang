use crate::expr::Expr;
use crate::intern::Symbol;
use crate::span::Span;
use crate::types::Type;

#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub stmts: Vec<Stmt>,
    pub tail: Option<Box<Expr>>,
}

#[derive(Debug, Clone)]
pub struct Stmt {
    pub kind: StmtKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StmtKind {
    Let {
        /// `pub let X = ...` at top level — exposes the binding as
        /// `module.X` to other modules. Only meaningful at top level;
        /// the parser rejects `pub let` inside fn/class bodies.
        /// `false` for nested lets and for unmarked top-level lets.
        is_pub: bool,
        /// `true` for `const x = expr` (one-time assignment —
        /// reassignment is rejected by the type checker). `false`
        /// for ordinary `let` bindings.
        is_const: bool,
        name: Symbol,
        ty: Option<Type>,
        value: Expr,
    },
    /// `let (a, b, ...) = tuple_expr` — flat tuple destructuring.
    /// Each slot is `Some(name)` or `None` for the `_` wildcard.
    /// No nesting in v1.
    LetTuple {
        elems: Box<[Option<Symbol>]>,
        value: Expr,
    },
    /// `let ClassName { f1, f2, ... } = struct_expr` —
    /// Rust-style struct destructuring. Field names equal binding
    /// names (no rename in v1). The class must match the value's
    /// runtime class (or a parent for inheritance).
    LetStruct {
        class: Symbol,
        fields: Box<[Symbol]>,
        value: Expr,
    },
    Expr(Expr),
}

impl Stmt {
    pub fn new(kind: StmtKind, span: Span) -> Self {
        Self { kind, span }
    }
}

// Span is metadata; comparing AST values for equality (e.g. in parser tests)
// should ignore source position so dummy spans line up with real ones.
impl PartialEq for Stmt {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind
    }
}
