use crate::span::Span;
use crate::stmt::Block;

#[derive(Debug, Clone)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

impl Expr {
    pub fn new(kind: ExprKind, span: Span) -> Self {
        Self { kind, span }
    }
}

// AST equality is structural over `kind`; spans are metadata and would
// otherwise force tests to thread exact source positions through every
// expected-tree literal.
impl PartialEq for Expr {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
    Var(String),
    /// The implicit receiver `this` inside a method body.
    This,
    Unary {
        op: UnOp,
        expr: Box<Expr>,
    },
    Binary {
        op: BinOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    /// Short-circuit logical operator (`&&`, `||`). Separated from `Binary`
    /// so the evaluator can avoid evaluating the rhs when not needed.
    Logical {
        op: LogicalOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    /// Free function call: `foo(args)`. Method calls go through MethodCall.
    Call {
        callee: String,
        args: Vec<Expr>,
    },
    /// `obj.field` — field read.
    Field {
        obj: Box<Expr>,
        name: String,
    },
    /// `obj.method(args)`.
    MethodCall {
        obj: Box<Expr>,
        method: String,
        args: Vec<Expr>,
    },
    /// `new ClassName(args)`.
    New {
        class: String,
        args: Vec<Expr>,
    },
    Block(Block),
    If {
        cond: Box<Expr>,
        then_branch: Block,
        /// `None`, another `If` (for `else if`), or a `Block`.
        else_branch: Option<Box<Expr>>,
    },
    While {
        cond: Box<Expr>,
        body: Block,
    },
    /// Infinite loop. Exits only via `break` (or returning from the
    /// enclosing function once `return` exists). Always evaluates to `Unit`.
    Loop {
        body: Block,
    },
    /// Exit the innermost enclosing `loop`/`while`. Type checker rejects
    /// occurrences outside any loop, including across function boundaries.
    Break,
    /// Skip to the next iteration of the innermost enclosing loop.
    Continue,
    /// Assignment to an existing variable. Always evaluates to `Unit`.
    Assign {
        target: String,
        value: Box<Expr>,
    },
    /// Assignment to a field: `obj.field = value`. Evaluates to `Unit`.
    AssignField {
        obj: Box<Expr>,
        field: String,
        value: Box<Expr>,
    },
    /// Numeric (or `bool`-to-int) cast: `expr as Type`.
    Cast {
        expr: Box<Expr>,
        ty: crate::types::Type,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Pos,
    Not,
    /// Bitwise NOT (`~`). Operand must be `i64`.
    BitNot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    /// Bitwise operators. All require `i64` on both sides and produce `i64`.
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogicalOp {
    And,
    Or,
}
