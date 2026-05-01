use crate::expr::Expr;
use crate::item::Item;
use crate::stmt::Stmt;

/// A top-level program: a sequence of items (fn declarations) and statements,
/// optionally followed by a trailing expression whose value is the program's
/// result (Rust-style block).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Program {
    pub items: Vec<Item>,
    pub stmts: Vec<Stmt>,
    pub tail: Option<Expr>,
}
