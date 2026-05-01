#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Int(i64),
    Float(f64),
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
    Call {
        callee: String,
        args: Vec<Expr>,
    },
    Block(Block),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Pos,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
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
    Unit,
}

impl std::fmt::Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Type::I64 => write!(f, "i64"),
            Type::F64 => write!(f, "f64"),
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
