//! Semantic scope-conflict detection for `rename` requests.
//!
//! Classifies the rename target via its hover signature and walks
//! the appropriate scope to decide whether `new_name` would collide
//! with an existing binding.
//!
//! Categories covered (mirrors the user's punch list):
//!   - top-level decl (fn / class / interface / enum / const /
//!     struct / union) → buffer-local `Doc.symbols` + selectively-
//!     imported names
//!   - class field / method / static / property /
//!     getter / setter → `Doc.classes[Class]`
//!   - enum variant → re-parse the file's enum
//!   - local `let` → same-block siblings
//!   - function parameter → other params on the same fn

use ilang_ast::{
    Block, ClassDecl, EnumDecl, Expr, ExprKind, FnDecl, InterfaceDecl, Item, Param,
    Pattern, PatternBindings, PatternKind, Program, Span, Stmt, StmtKind,
    Symbol as AstSymbol,
};
use ilang_lexer::tokenize;
use ilang_parser::parse;

use crate::types::Doc;

/// Detect whether renaming the symbol at `decl_name_span` to
/// `new_name` would collide with an existing binding. `Ok(())`
/// means the rename is safe; `Err(msg)` is a one-line description
/// the LSP can surface as an `invalid_params` rejection.
pub(crate) fn detect(
    doc: &Doc,
    sig: &str,
    decl_name_span: Span,
    old_name: &str,
    new_name: &str,
) -> Result<(), String> {
    if old_name == new_name {
        return Ok(());
    }
    let Some(kind) = classify(sig) else { return Ok(()) };
    match kind {
        Kind::TopLevel => check_top_level(doc, decl_name_span, new_name),
        Kind::ClassMember { class } => {
            check_class_member(doc, &class, decl_name_span, new_name)
        }
        Kind::Variant { enum_name } => check_variant(&doc.text, &enum_name, new_name),
        Kind::Local => check_local(&doc.text, decl_name_span, new_name),
        Kind::Parameter => check_parameter(&doc.text, decl_name_span, new_name),
    }
}

#[derive(Debug)]
enum Kind {
    /// Top-level decl: fn / class / interface / enum / const /
    /// struct / union.
    TopLevel,
    /// Class member — fields / methods / static_methods /
    /// properties / getters / setters / static_fields. `class` is
    /// the declaring class name (used to index `Doc.classes`).
    ClassMember { class: String },
    /// Enum variant. `enum_name` is the enum's name.
    Variant { enum_name: String },
    /// `let` inside a function body (or tuple / struct destructure).
    Local,
    /// Function parameter (top-level fn or class method).
    Parameter,
}

/// Strip attribute lines (`@objc(\"...\")\n`) and return the last
/// non-empty line — the form `Class.method(...)` or `class Foo`
/// our signatures end with.
fn last_line(sig: &str) -> &str {
    sig.lines().last().unwrap_or(sig)
}

fn classify(sig: &str) -> Option<Kind> {
    // Class members: `(method) Class.m(...)`, `(static method)`,
    // `(property) Class.x: T`, `(getter)`, `(setter)`,
    // `(static property)`, `(static const)`.
    let class_member_prefixes = [
        "(static method) ",
        "(static getter) ",
        "(static setter) ",
        "(static property) ",
        "(static const) ",
        "(method) ",
        "(getter) ",
        "(setter) ",
        "(property) ",
    ];
    for prefix in class_member_prefixes {
        if let Some(rest) = sig.strip_prefix(prefix) {
            let tail = last_line(rest);
            let class = tail.split('.').next()?.to_string();
            return Some(Kind::ClassMember { class });
        }
    }
    if let Some(rest) = sig.strip_prefix("(variant) ") {
        let enum_name = rest.split('.').next()?.to_string();
        return Some(Kind::Variant { enum_name });
    }
    if sig.starts_with("(parameter) ") {
        return Some(Kind::Parameter);
    }
    if sig.starts_with("let ") || sig.starts_with("(for-binding) ") || sig.starts_with("(pattern) ") {
        return Some(Kind::Local);
    }
    // Top-level header forms. `fn ` may include `@objc fn ` /
    // `@extern fn ` prefixes; the last line check handles both.
    let head = last_line(sig);
    for prefix in [
        "fn ", "class ", "interface ", "enum ", "const ", "struct ", "union ",
    ] {
        if head.starts_with(prefix) {
            return Some(Kind::TopLevel);
        }
    }
    None
}

fn check_top_level(
    doc: &Doc,
    decl_name_span: Span,
    new_name: &str,
) -> Result<(), String> {
    let key = AstSymbol::intern(new_name);
    if let Some(sym) = doc.symbols.get(&key) {
        if sym.span != decl_name_span {
            return Err(format!(
                "`{new_name}` is already defined in this file"
            ));
        }
    }
    if doc.selective_use_names.contains(&key) {
        return Err(format!(
            "`{new_name}` collides with a `use` import"
        ));
    }
    Ok(())
}

fn check_class_member(
    doc: &Doc,
    class: &str,
    decl_name_span: Span,
    new_name: &str,
) -> Result<(), String> {
    let key = AstSymbol::intern(new_name);
    let class_key = AstSymbol::intern(class);
    let Some(info) = doc.classes.get(&class_key) else { return Ok(()) };
    // For each member map, a collision is anything carrying the
    // new_name except the target itself (matched by span).
    let buckets = [
        ("field/property", &info.fields),
        ("method", &info.methods),
        ("getter", &info.getters),
        ("setter", &info.setters),
    ];
    for (label, map) in buckets {
        if let Some(m) = map.get(&key) {
            if m.span != decl_name_span {
                return Err(format!(
                    "`{new_name}` is already a {label} on `{class}`"
                ));
            }
        }
    }
    Ok(())
}

fn check_variant(text: &str, enum_name: &str, new_name: &str) -> Result<(), String> {
    let Some(prog) = parse_ok(text) else { return Ok(()) };
    for item in &prog.items {
        if let Item::Enum(e) = item {
            if e.name.as_str() == enum_name {
                for v in e.variants.iter() {
                    if v.name.as_str() == new_name {
                        return Err(format!(
                            "`{enum_name}.{new_name}` already exists"
                        ));
                    }
                }
            }
        }
    }
    Ok(())
}

/// Local `let` rename — check the same block for a sibling binding
/// with `new_name`. Walks the parsed AST to find the binding at
/// `decl_name_span`'s line.
fn check_local(
    text: &str,
    decl_name_span: Span,
    new_name: &str,
) -> Result<(), String> {
    let Some(prog) = parse_ok(text) else { return Ok(()) };
    let mut visitor = LocalVisitor {
        target_line: decl_name_span.line,
        target_col:  decl_name_span.col,
        new_name,
        found:       None,
    };
    visitor.walk_program(&prog);
    visitor.found.unwrap_or(Ok(()))
}

/// Parameter rename — check other params on the same fn for a
/// name collision.
fn check_parameter(
    text: &str,
    decl_name_span: Span,
    new_name: &str,
) -> Result<(), String> {
    let Some(prog) = parse_ok(text) else { return Ok(()) };
    let mut visitor = ParamVisitor {
        target_line: decl_name_span.line,
        target_col:  decl_name_span.col,
        new_name,
        found:       None,
    };
    visitor.walk_program(&prog);
    visitor.found.unwrap_or(Ok(()))
}

fn parse_ok(text: &str) -> Option<Program> {
    let tokens = tokenize(text).ok()?;
    parse(&tokens).ok()
}

// ─── local let visitor ──────────────────────────────────────────

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

// ─── parameter visitor ──────────────────────────────────────────

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

// Quieten dead-code warnings from struct fields that exist only
// because we initialise them once and never read them again.
#[allow(dead_code)]
fn _silence(_: &ClassDecl, _: &EnumDecl, _: &InterfaceDecl, _: &Pattern, _: &PatternKind, _: &PatternBindings) {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyse_path_to_doc;

    fn prep(src: &str) -> (std::path::PathBuf, Doc) {
        let tmp = std::env::temp_dir().join(format!(
            "ilang_rename_conflicts_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(tmp.join("ilang.toml"), "[package]\nname = \"t\"\n").unwrap();
        let path = tmp.join("a.il");
        std::fs::write(&path, src).unwrap();
        let doc = analyse_path_to_doc(&path).expect("analyse");
        (path, doc)
    }

    #[test]
    fn detects_top_level_collision() {
        let src = "fn foo() { }\nfn bar() { }\nrun()\nfn run() { }\n";
        let (_path, doc) = prep(src);
        // Rename `foo` to `bar` — should collide.
        let target_span = doc.symbols.get(&AstSymbol::intern("foo")).unwrap().span;
        let err = detect(&doc, "fn foo()", target_span, "foo", "bar").unwrap_err();
        assert!(err.contains("already defined"), "got: {err}");
        // Rename `foo` to `baz` — no collision.
        detect(&doc, "fn foo()", target_span, "foo", "baz").unwrap();
    }

    #[test]
    fn detects_class_member_collision() {
        let src = "class C { x: i64; y: i64 }\nfn run() { }\nrun()\n";
        let (_path, doc) = prep(src);
        let class_info = doc.classes.get(&AstSymbol::intern("C")).unwrap();
        let target_span = class_info.fields.get(&AstSymbol::intern("x")).unwrap().span;
        let err = detect(
            &doc,
            "(property) C.x: i64",
            target_span,
            "x",
            "y",
        )
        .unwrap_err();
        assert!(err.contains("already"), "got: {err}");
    }

    #[test]
    fn detects_variant_collision() {
        let src = "enum Color { Red, Green, Blue }\nfn run() { }\nrun()\n";
        let (_path, doc) = prep(src);
        // Span isn't critical here — variant check just looks for
        // the new name in the enum's variant list.
        let dummy_span = ilang_ast::Span::new(1, 1);
        let err = detect(
            &doc,
            "(variant) Color.Red",
            dummy_span,
            "Red",
            "Green",
        )
        .unwrap_err();
        assert!(err.contains("already exists"), "got: {err}");
    }

    #[test]
    fn detects_same_block_let_collision() {
        let src = "fn run() {\n    let x = 1\n    let y = 2\n    let _ = x + y\n}\nrun()\n";
        let (_path, doc) = prep(src);
        // Target `let x = 1` is on line 2 (1-based). Its decl-name
        // span is line=2, col=9 (the `x` after `let `).
        let target_span = ilang_ast::Span::new(2, 9);
        let err = detect(&doc, "let x", target_span, "x", "y").unwrap_err();
        assert!(err.contains("already declared"), "got: {err}");
        // Renaming to an unused name is fine.
        detect(&doc, "let x", target_span, "x", "z").unwrap();
    }

    #[test]
    fn detects_parameter_collision() {
        let src = "fn add(a: i64, b: i64): i64 { a + b }\nrun()\nfn run() { }\n";
        let (_path, doc) = prep(src);
        // Param `a` on line 1. The parser stores `Param.span` at
        // the name position.
        let target_span = ilang_ast::Span::new(1, 8);
        let err = detect(
            &doc,
            "(parameter) a: i64",
            target_span,
            "a",
            "b",
        )
        .unwrap_err();
        assert!(err.contains("already a parameter"), "got: {err}");
    }
}
