use crate::stmt::Block;

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Int(i64),
    Float(f64),
    Bool(bool),
    Var(String),
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
    Call {
        callee: String,
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
    /// Assignment to an existing variable. Always evaluates to `Unit`.
    Assign {
        target: String,
        value: Box<Expr>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Pos,
    Not,
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogicalOp {
    And,
    Or,
}
