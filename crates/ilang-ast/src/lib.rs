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

#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub stmts: Vec<Stmt>,
    pub tail: Option<Box<Expr>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    Let {
        name: String,
        ty: Option<Type>,
        value: Expr,
    },
    Expr(Expr),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Type {
    I64,
    F64,
    Bool,
    Unit,
}

impl std::fmt::Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Type::I64 => write!(f, "i64"),
            Type::F64 => write!(f, "f64"),
            Type::Bool => write!(f, "bool"),
            Type::Unit => write!(f, "()"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub name: String,
    pub ty: Type,
}

/// Attribute on a function declaration, e.g. `#[requires(net, file::read)]`.
/// Phase 2 parses these but does not enforce them.
#[derive(Debug, Clone, PartialEq)]
pub struct Attribute {
    pub name: String,
    pub args: Vec<AttrArg>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AttrArg {
    /// A capability path like `net` or `file::read`.
    Path(Vec<String>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct FnDecl {
    pub attrs: Vec<Attribute>,
    pub name: String,
    pub params: Vec<Param>,
    pub ret: Option<Type>,
    pub body: Block,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    Fn(FnDecl),
}

/// A top-level program: a sequence of items (fn declarations) and statements,
/// optionally followed by a trailing expression whose value is the program's
/// result (Rust-style block).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Program {
    pub items: Vec<Item>,
    pub stmts: Vec<Stmt>,
    pub tail: Option<Expr>,
}
