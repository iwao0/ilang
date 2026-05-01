#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Int(i64),
    Float(f64),
    Unary { op: UnOp, expr: Box<Expr> },
    Binary { op: BinOp, lhs: Box<Expr>, rhs: Box<Expr> },
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
