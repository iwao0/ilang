//! Walks a parsed program looking for parameter siblings of the fn
//! whose body contains the target identifier at
//! `(target_line, target_col)`, so `check_parameter` can refuse a
//! rename that would collide with another parameter of the same fn.

#![allow(unused_imports)]

use ilang_ast::{
    Block, ClassDecl, EnumDecl, Expr, ExprKind, FnDecl, InterfaceDecl, Item, Param,
    Pattern, PatternBindings, PatternKind, Program, Span, Stmt, StmtKind,
    Symbol as AstSymbol,
};

use crate::text;

pub(super) fn check_parameter(
    text: &str,
    decl_name_span: Span,
    new_name: &str,
) -> Result<(), String> {
    let Some(prog) = text::try_parse(text) else { return Ok(()) };
    let mut visitor = ParamVisitor {
        target_line: decl_name_span.line,
        target_col:  decl_name_span.col,
        new_name,
        found:       None,
    };
    visitor.walk_program(&prog);
    visitor.found.unwrap_or(Ok(()))
}

struct ParamVisitor<'a> {
    target_line: u32,
    target_col:  u32,
    new_name:    &'a str,
    found:       Option<Result<(), String>>,
}

impl<'a> ParamVisitor<'a> {
    fn walk_program(&mut self, prog: &Program) {
        for item in &prog.items {
            if self.found.is_some() {
                return;
            }
            self.walk_item(item);
        }
    }
    fn walk_item(&mut self, item: &Item) {
        match item {
            Item::Fn(f) => self.check_fn(f),
            Item::Class(c) => {
                for m in c.methods.iter() {
                    self.check_fn(m);
                }
                for m in c.static_methods.iter() {
                    self.check_fn(m);
                }
                for p in c.properties.iter() {
                    if let Some(g) = &p.getter {
                        self.check_fn(g);
                    }
                    if let Some(s) = &p.setter {
                        self.check_fn(s);
                    }
                }
            }
            Item::Interface(i) => {
                // Interface method params can be renamed too —
                // sibling-param collision still applies.
                for m in i.methods.iter() {
                    self.check_params(&m.params);
                }
            }
            Item::ExternC(b) => {
                for inner in b.items.iter() {
                    match inner {
                        ilang_ast::ExternCItem::FnDef(f) => self.check_fn(f),
                        ilang_ast::ExternCItem::FnDecl { params, .. } => {
                            self.check_params(params);
                        }
                        ilang_ast::ExternCItem::Class(c) => {
                            for m in c.methods.iter() {
                                self.check_fn(m);
                            }
                            for m in c.static_methods.iter() {
                                self.check_fn(m);
                            }
                        }
                        _ => {}
                    }
                }
                for iface in b.interfaces.iter() {
                    for m in iface.methods.iter() {
                        self.check_params(&m.params);
                    }
                }
            }
            _ => {}
        }
    }
    fn check_fn(&mut self, f: &FnDecl) {
        self.check_params(&f.params);
    }
    fn check_params(&mut self, params: &[Param]) {
        if self.found.is_some() {
            return;
        }
        // Does this fn's param list contain the target? Match on
        // line + col when both are known; some param sources only
        // record the line.
        let exact_match = params.iter().any(|p| {
            p.span.line == self.target_line && p.span.col == self.target_col
        });
        let line_match = params.iter().any(|p| p.span.line == self.target_line);
        if !exact_match && !line_match {
            return;
        }
        for p in params {
            let same_as_target =
                p.span.line == self.target_line && p.span.col == self.target_col;
            if same_as_target {
                continue;
            }
            if p.name.as_str() == self.new_name {
                self.found = Some(Err(format!(
                    "`{}` is already a parameter on this function",
                    self.new_name
                )));
                return;
            }
        }
        self.found = Some(Ok(()));
    }
}
