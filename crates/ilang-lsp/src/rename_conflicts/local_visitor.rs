//! Walks a parsed program looking for the same-block siblings of a
//! local `let` binding identified by `(target_line, target_col)`, so
//! `check_local` can refuse a rename that would shadow another
//! visible binding at the same depth.

#![allow(unused_imports)]

use ilang_ast::{
    Block, ClassDecl, EnumDecl, Expr, ExprKind, FnDecl, InterfaceDecl, Item, Param,
    Pattern, PatternBindings, PatternKind, Program, Span, Stmt, StmtKind,
    Symbol as AstSymbol,
};

use crate::text;

pub(super) fn check_local(
    text: &str,
    decl_name_span: Span,
    new_name: &str,
) -> Result<(), String> {
    let Some(prog) = text::try_parse(text) else { return Ok(()) };
    let mut visitor = LocalVisitor {
        target_line: decl_name_span.line,
        target_col:  decl_name_span.col,
        new_name,
        found:       None,
    };
    visitor.walk_program(&prog);
    visitor.found.unwrap_or(Ok(()))
}

struct LocalVisitor<'a> {
    target_line: u32,
    target_col:  u32,
    new_name:    &'a str,
    /// `Some(Ok(()))` once we've located the target's block and
    /// confirmed no collision. `Some(Err(msg))` on collision. `None`
    /// while the search is still in progress.
    found:       Option<Result<(), String>>,
}

impl<'a> LocalVisitor<'a> {
    fn walk_program(&mut self, prog: &Program) {
        for item in &prog.items {
            if self.found.is_some() {
                return;
            }
            self.walk_item(item);
        }
        // Top-level stmts (script-style code outside any fn).
        self.walk_block_stmts(&prog.stmts, prog.tail.as_ref());
    }
    fn walk_item(&mut self, item: &Item) {
        match item {
            Item::Fn(f) => self.walk_block(&f.body),
            Item::Class(c) => {
                for m in c.methods.iter() {
                    self.walk_block(&m.body);
                }
                for m in c.static_methods.iter() {
                    self.walk_block(&m.body);
                }
                for p in c.properties.iter() {
                    if let Some(g) = &p.getter {
                        self.walk_block(&g.body);
                    }
                    if let Some(s) = &p.setter {
                        self.walk_block(&s.body);
                    }
                }
            }
            Item::ExternC(b) => {
                for inner in b.items.iter() {
                    match inner {
                        ilang_ast::ExternCItem::FnDef(f) => self.walk_block(&f.body),
                        ilang_ast::ExternCItem::Class(c) => {
                            for m in c.methods.iter() {
                                self.walk_block(&m.body);
                            }
                            for m in c.static_methods.iter() {
                                self.walk_block(&m.body);
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    fn walk_block(&mut self, b: &Block) {
        self.walk_block_stmts(&b.stmts, b.tail.as_deref());
    }
    fn walk_block_stmts(&mut self, stmts: &[Stmt], tail: Option<&Expr>) {
        if self.found.is_some() {
            return;
        }
        // Pass 1: does the target sit in this block's let stmts?
        let target_here = stmts.iter().any(|s| match &s.kind {
            StmtKind::Let { .. } | StmtKind::LetTuple { .. } | StmtKind::LetStruct { .. } => {
                s.span.line == self.target_line
            }
            _ => false,
        });
        if target_here {
            // Check siblings (excluding the target itself, matched
            // by line position) for `new_name`.
            for s in stmts {
                if s.span.line == self.target_line {
                    continue;
                }
                if let StmtKind::Let { name, .. } = &s.kind {
                    if name.as_str() == self.new_name {
                        self.found = Some(Err(format!(
                            "`{}` is already declared in this block",
                            self.new_name
                        )));
                        return;
                    }
                }
            }
            self.found = Some(Ok(()));
            return;
        }
        // Otherwise recurse into each stmt / tail expression.
        for s in stmts {
            self.walk_stmt(s);
            if self.found.is_some() {
                return;
            }
        }
        if let Some(t) = tail {
            self.walk_expr(t);
        }
    }
    #[allow(dead_code)]
    fn _ignore_target_col(&self) {
        let _ = self.target_col;
    }
    fn walk_stmt(&mut self, s: &Stmt) {
        match &s.kind {
            StmtKind::Let { value, .. }
            | StmtKind::LetTuple { value, .. }
            | StmtKind::LetStruct { value, .. } => self.walk_expr(value),
            StmtKind::Expr(e) => self.walk_expr(e),
        }
    }
    fn walk_expr(&mut self, e: &Expr) {
        if self.found.is_some() {
            return;
        }
        match &e.kind {
            ExprKind::Block(b) => self.walk_block(b),
            ExprKind::If { cond, then_branch, else_branch, .. } => {
                self.walk_expr(cond);
                self.walk_block(then_branch);
                if let Some(eb) = else_branch {
                    self.walk_expr(eb);
                }
            }
            ExprKind::IfLet { expr, then_branch, else_branch, .. } => {
                self.walk_expr(expr);
                self.walk_block(then_branch);
                if let Some(eb) = else_branch {
                    self.walk_expr(eb);
                }
            }
            ExprKind::While { cond, body } => {
                self.walk_expr(cond);
                self.walk_block(body);
            }
            ExprKind::Loop { body } | ExprKind::ForIn { body, .. } => {
                if let ExprKind::ForIn { iter, .. } = &e.kind {
                    self.walk_expr(iter);
                }
                self.walk_block(body);
            }
            ExprKind::Match { scrutinee, arms } => {
                self.walk_expr(scrutinee);
                for arm in arms.iter() {
                    self.walk_expr(&arm.body);
                }
            }
            ExprKind::Call { args, .. } => {
                for a in args.iter() {
                    self.walk_expr(a);
                }
            }
            ExprKind::MethodCall { obj, args, .. } => {
                self.walk_expr(obj);
                for a in args.iter() {
                    self.walk_expr(a);
                }
            }
            ExprKind::New { args, .. } => {
                for a in args.iter() {
                    self.walk_expr(a);
                }
            }
            ExprKind::Binary { lhs, rhs, .. } | ExprKind::Logical { lhs, rhs, .. } => {
                self.walk_expr(lhs);
                self.walk_expr(rhs);
            }
            ExprKind::Unary { expr, .. } => self.walk_expr(expr),
            ExprKind::Cast { expr, .. }
            | ExprKind::TypeTest { expr, .. }
            | ExprKind::TypeDowncast { expr, .. } => self.walk_expr(expr),
            ExprKind::Field { obj, .. } => self.walk_expr(obj),
            ExprKind::Index { obj, index } => {
                self.walk_expr(obj);
                self.walk_expr(index);
            }
            ExprKind::Array(elems) | ExprKind::Tuple(elems) => {
                for e in elems.iter() {
                    self.walk_expr(e);
                }
            }
            ExprKind::AssignField { obj, value, .. } => {
                self.walk_expr(obj);
                self.walk_expr(value);
            }
            ExprKind::AssignIndex { obj, index, value } => {
                self.walk_expr(obj);
                self.walk_expr(index);
                self.walk_expr(value);
            }
            ExprKind::Some(e)
            | ExprKind::Await(e)
            | ExprKind::Return(Some(e))
            | ExprKind::Break(Some(e)) => self.walk_expr(e),
            ExprKind::FnExpr { body, .. } => self.walk_block(body),
            _ => {}
        }
    }
}
