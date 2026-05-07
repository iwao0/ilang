//! Post-parse AST normalization: rewrite `EnumName.Variant` shapes
//! that the parser couldn't disambiguate at parse time.
//!
//! With the `.` syntax replacing `::` for enum constructors, the
//! parser produces these shapes for `Color.Green` / `Result.Ok(5)`:
//!
//!   `Color.Green`     → `ExprKind::Field { obj: Var("Color"), name: "Green" }`
//!   `Result.Ok(5)`    → `ExprKind::MethodCall { obj: Var("Result"), method: "Ok", args: [5] }`
//!
//! When the receiver is a bare `Var` whose name matches a top-level
//! `enum` (or the built-in `Result`), this pass rewrites them into
//! `ExprKind::EnumCtor`. Anything that doesn't match (e.g.
//! `obj.field`, `console.log(...)`) passes through unchanged.
//!
//! Struct-payload constructors (`Color.Red { side: 1 }`) are produced
//! directly as `EnumCtor` by the parser via lookahead, so they don't
//! need rewriting here.

use std::collections::HashSet;

use ilang_ast::{
    Block, ClassDecl, CtorArgs, Expr, ExprKind, Item, MatchArm, Program, Span, Stmt, StmtKind,
    Symbol, Type,
};

use crate::error::ParseError;

/// Built-in enum names that are always available.
const BUILTIN_ENUMS: &[&str] = &["Result"];

#[derive(Default)]
struct Ctx {
    /// All names that resolve as enums after the program is fully
    /// loaded (built-ins + every `Item::Enum`'s name).
    enums: HashSet<Symbol>,
    /// Names that come from `use module` (whole-module imports).
    /// References like `module.foo` get rewritten to qualified
    /// `Var("module.foo")` (or `Call("module.foo", ...)`); the loader
    /// will have produced top-level items with those exact names.
    modules: HashSet<Symbol>,
}

/// Per-file normalize entry point. Validates that every dotted
/// `module.X` reference (in `new`, type position, struct literal,
/// parent class) names a module this file actually `use`s — see
/// `validate_program` — then runs the AST rewrites.
pub fn normalize(prog: Program) -> Result<Program, ParseError> {
    let ctx = build_ctx(&prog);
    // Reject `new module.Class()` / `let x: module.Class` whose
    // module prefix this file didn't `use`. Without the check, a
    // sibling module's `@export use` chain could leak every merged
    // submodule into a file that never opted in (silent leakage
    // through the umbrella prefix).
    validate_program(&prog, &ctx.modules)?;
    Ok(rewrite_program(prog, &ctx))
}

/// Loader-side entry point for the post-merge normalize run. The
/// merged Program intentionally contains zero `Item::Use`s (the
/// loader stripped them all), so per-file authorization has already
/// been verified at parse time; running `validate_program` here
/// would falsely reject every legitimate cross-module reference.
pub fn renormalize_merged(prog: Program) -> Program {
    let ctx = build_ctx(&prog);
    rewrite_program(prog, &ctx)
}

fn build_ctx(prog: &Program) -> Ctx {
    let mut ctx = Ctx::default();
    for s in BUILTIN_ENUMS {
        ctx.enums.insert((*s).into());
    }
    for item in &prog.items {
        match item {
            Item::Enum(e) => {
                ctx.enums.insert(e.name.clone());
            }
            Item::Use(u) if u.selective.is_none() => {
                ctx.modules.insert(u.module.clone());
            }
            _ => {}
        }
    }
    ctx
}

fn rewrite_program(prog: Program, ctx: &Ctx) -> Program {
    let items: Vec<Item> = prog.items.into_iter().map(|i| rewrite_item(i, ctx)).collect();
    let stmts: Vec<Stmt> = prog.stmts.into_iter().map(|s| rewrite_stmt(s, ctx)).collect();
    let tail = prog.tail.map(|e| rewrite_expr(e, ctx));
    Program {
        items,
        stmts,
        tail,
    }
}

// ─── Module-prefix authorization check ────────────────────────────────
//
// Only `New` (constructor) and Type-position references are checked
// here. Field / MethodCall paths already require the receiver name
// to be in `ctx.modules` before normalize collapses them to a
// qualified `Var` / `Call`, so they're safely gated.

fn check_dotted_ref(
    name: &Symbol,
    item_label: &str,
    span: Span,
    modules: &HashSet<Symbol>,
) -> Result<(), ParseError> {
    let s = name.as_str();
    if let Some((prefix, rest)) = s.split_once('.') {
        if !modules.contains(&Symbol::intern(prefix)) {
            return Err(ParseError::UnauthorizedModuleRef {
                module: Symbol::intern(prefix),
                item: Symbol::intern(if item_label.is_empty() { rest } else { item_label }),
                span,
            });
        }
    }
    Ok(())
}

fn validate_type(t: &Type, span: Span, modules: &HashSet<Symbol>) -> Result<(), ParseError> {
    match t {
        Type::Object(name) | Type::Enum(name) => {
            check_dotted_ref(name, "", span, modules)?
        }
        Type::Generic(g) => {
            check_dotted_ref(&g.base, "", span, modules)?;
            for a in g.args.iter() {
                validate_type(a, span, modules)?;
            }
        }
        Type::Array { elem, .. } | Type::Optional(elem) | Type::Weak(elem) => {
            validate_type(elem, span, modules)?
        }
        Type::Tuple(elems) => {
            for e in elems.iter() {
                validate_type(e, span, modules)?;
            }
        }
        Type::Fn(ft) => {
            for p in ft.params.iter() {
                validate_type(p, span, modules)?;
            }
            validate_type(&ft.ret, span, modules)?;
        }
        Type::RawPtr { inner, .. } => validate_type(inner, span, modules)?,
        _ => {}
    }
    Ok(())
}

fn validate_block(b: &Block, modules: &HashSet<Symbol>) -> Result<(), ParseError> {
    for s in b.stmts.iter() {
        validate_stmt(s, modules)?;
    }
    if let Some(t) = b.tail.as_ref() {
        validate_expr(t, modules)?;
    }
    Ok(())
}

fn validate_stmt(s: &Stmt, modules: &HashSet<Symbol>) -> Result<(), ParseError> {
    match &s.kind {
        StmtKind::Let { ty, value, .. } => {
            if let Some(t) = ty {
                validate_type(t, s.span, modules)?;
            }
            validate_expr(value, modules)?;
        }
        StmtKind::LetTuple { value, .. } | StmtKind::LetStruct { value, .. } => {
            validate_expr(value, modules)?;
        }
        StmtKind::Expr(e) => validate_expr(e, modules)?,
    }
    Ok(())
}

fn validate_expr(e: &Expr, modules: &HashSet<Symbol>) -> Result<(), ParseError> {
    match &e.kind {
        ExprKind::New { class, type_args, args, .. } => {
            check_dotted_ref(class, "", e.span, modules)?;
            for ta in type_args.iter() {
                validate_type(ta, e.span, modules)?;
            }
            for a in args.iter() {
                validate_expr(a, modules)?;
            }
        }
        ExprKind::Cast { expr, ty }
        | ExprKind::TypeTest { expr, ty }
        | ExprKind::TypeDowncast { expr, ty } => {
            validate_expr(expr, modules)?;
            validate_type(ty, e.span, modules)?;
        }
        ExprKind::FnExpr { params, ret, body } => {
            for p in params.iter() {
                validate_type(&p.ty, p.span, modules)?;
                if let Some(d) = &p.default {
                    validate_expr(d, modules)?;
                }
            }
            if let Some(r) = ret {
                validate_type(r, e.span, modules)?;
            }
            validate_block(body, modules)?;
        }
        ExprKind::Unary { expr, .. }
        | ExprKind::Some(expr)
        | ExprKind::Return(Some(expr))
        | ExprKind::Break(Some(expr)) => validate_expr(expr, modules)?,
        ExprKind::Binary { lhs, rhs, .. } | ExprKind::Logical { lhs, rhs, .. } => {
            validate_expr(lhs, modules)?;
            validate_expr(rhs, modules)?;
        }
        ExprKind::Call { args, .. } | ExprKind::SuperCall { args, .. } => {
            for a in args.iter() {
                validate_expr(a, modules)?;
            }
        }
        ExprKind::MethodCall { obj, args, .. } => {
            validate_expr(obj, modules)?;
            for a in args.iter() {
                validate_expr(a, modules)?;
            }
        }
        ExprKind::Field { obj, .. } => validate_expr(obj, modules)?,
        ExprKind::Assign { value, .. } => validate_expr(value, modules)?,
        ExprKind::AssignField { obj, value, .. } => {
            validate_expr(obj, modules)?;
            validate_expr(value, modules)?;
        }
        ExprKind::AssignIndex { obj, index, value } => {
            validate_expr(obj, modules)?;
            validate_expr(index, modules)?;
            validate_expr(value, modules)?;
        }
        ExprKind::EnumCtor { args, .. } => match args {
            CtorArgs::Unit => {}
            CtorArgs::Tuple(es) => {
                for a in es.iter() {
                    validate_expr(a, modules)?;
                }
            }
            CtorArgs::Struct(fs) => {
                for (_, a) in fs.iter() {
                    validate_expr(a, modules)?;
                }
            }
        },
        ExprKind::If { cond, then_branch, else_branch } => {
            validate_expr(cond, modules)?;
            validate_block(then_branch, modules)?;
            if let Some(e2) = else_branch {
                validate_expr(e2, modules)?;
            }
        }
        ExprKind::IfLet { expr, then_branch, else_branch, .. } => {
            validate_expr(expr, modules)?;
            validate_block(then_branch, modules)?;
            if let Some(e2) = else_branch {
                validate_expr(e2, modules)?;
            }
        }
        ExprKind::While { cond, body } => {
            validate_expr(cond, modules)?;
            validate_block(body, modules)?;
        }
        ExprKind::Loop { body } => validate_block(body, modules)?,
        ExprKind::ForIn { iter, body, .. } => {
            validate_expr(iter, modules)?;
            validate_block(body, modules)?;
        }
        ExprKind::Range { start, end, .. } => {
            if let Some(s) = start {
                validate_expr(s, modules)?;
            }
            if let Some(e2) = end {
                validate_expr(e2, modules)?;
            }
        }
        ExprKind::Block(b) => validate_block(b, modules)?,
        ExprKind::Array(es) | ExprKind::Tuple(es) => {
            for x in es.iter() {
                validate_expr(x, modules)?;
            }
        }
        ExprKind::StructLit { class, fields } => {
            check_dotted_ref(class, "", e.span, modules)?;
            for (_, x) in fields.iter() {
                validate_expr(x, modules)?;
            }
        }
        ExprKind::MapLit(entries) => {
            for (k, v) in entries.iter() {
                validate_expr(k, modules)?;
                validate_expr(v, modules)?;
            }
        }
        ExprKind::Match { scrutinee, arms } => {
            validate_expr(scrutinee, modules)?;
            for arm in arms.iter() {
                validate_expr(&arm.body, modules)?;
            }
        }
        ExprKind::Index { obj, index } => {
            validate_expr(obj, modules)?;
            validate_expr(index, modules)?;
        }
        // Leaf nodes / nodes with nothing module-qualifiable inside.
        ExprKind::Int(_)
        | ExprKind::Float(_)
        | ExprKind::Bool(_)
        | ExprKind::Str(_)
        | ExprKind::Var(_)
        | ExprKind::This
        | ExprKind::None
        | ExprKind::Continue
        | ExprKind::Closure { .. }
        | ExprKind::Return(None)
        | ExprKind::Break(None) => {}
    }
    Ok(())
}

fn validate_class(c: &ClassDecl, modules: &HashSet<Symbol>) -> Result<(), ParseError> {
    if let Some(parent) = &c.parent {
        check_dotted_ref(parent, "", c.span, modules)?;
    }
    for f in c.fields.iter() {
        validate_type(&f.ty, f.span, modules)?;
    }
    for sf in c.static_fields.iter() {
        validate_type(&sf.ty, sf.span, modules)?;
        validate_expr(&sf.value, modules)?;
    }
    for m in c.methods.iter().chain(c.static_methods.iter()) {
        for p in m.params.iter() {
            validate_type(&p.ty, p.span, modules)?;
            if let Some(d) = &p.default {
                validate_expr(d, modules)?;
            }
        }
        if let Some(r) = &m.ret {
            validate_type(r, m.span, modules)?;
        }
        validate_block(&m.body, modules)?;
    }
    for prop in c.properties.iter() {
        validate_type(&prop.ty, prop.span, modules)?;
        if let Some(g) = &prop.getter {
            validate_block(&g.body, modules)?;
        }
        if let Some(s) = &prop.setter {
            for p in s.params.iter() {
                validate_type(&p.ty, p.span, modules)?;
            }
            validate_block(&s.body, modules)?;
        }
    }
    Ok(())
}

fn validate_program(prog: &Program, modules: &HashSet<Symbol>) -> Result<(), ParseError> {
    for item in &prog.items {
        match item {
            Item::Fn(f) => {
                for p in f.params.iter() {
                    validate_type(&p.ty, p.span, modules)?;
                    if let Some(d) = &p.default {
                        validate_expr(d, modules)?;
                    }
                }
                if let Some(r) = &f.ret {
                    validate_type(r, f.span, modules)?;
                }
                validate_block(&f.body, modules)?;
            }
            Item::Class(c) => validate_class(c, modules)?,
            Item::Enum(_) | Item::Use(_) => {}
            Item::Const(c) => {
                if let Some(t) = &c.ty {
                    validate_type(t, c.span, modules)?;
                }
                validate_expr(&c.value, modules)?;
            }
            Item::ExternStatic(s) => validate_type(&s.ty, s.span, modules)?,
            Item::ExternC(b) => {
                for inner in b.items.iter() {
                    use ilang_ast::ExternCItem;
                    match inner {
                        ExternCItem::FnDef(f) => {
                            for p in f.params.iter() {
                                validate_type(&p.ty, p.span, modules)?;
                            }
                            if let Some(r) = &f.ret {
                                validate_type(r, f.span, modules)?;
                            }
                            validate_block(&f.body, modules)?;
                        }
                        ExternCItem::FnDecl { params, ret, span, .. } => {
                            for p in params.iter() {
                                validate_type(&p.ty, p.span, modules)?;
                            }
                            if let Some(r) = ret {
                                validate_type(r, *span, modules)?;
                            }
                        }
                        ExternCItem::Struct { fields, span, .. }
                        | ExternCItem::Union { fields, span, .. } => {
                            for f in fields.iter() {
                                validate_type(&f.ty, *span, modules)?;
                            }
                        }
                        ExternCItem::Static { ty, span, .. } => {
                            validate_type(ty, *span, modules)?;
                        }
                        ExternCItem::Class(c) => validate_class(c, modules)?,
                    }
                }
            }
        }
    }
    for s in &prog.stmts {
        validate_stmt(s, modules)?;
    }
    if let Some(t) = &prog.tail {
        validate_expr(t, modules)?;
    }
    Ok(())
}

fn rewrite_params(params: &mut [ilang_ast::Param], ctx: &Ctx) {
    for p in params.iter_mut() {
        if let Some(d) = p.default.take() {
            p.default = Some(rewrite_expr(d, ctx));
        }
    }
}

fn rewrite_item(item: Item, ctx: &Ctx) -> Item {
    match item {
        Item::Fn(mut f) => {
            rewrite_params(&mut f.params, ctx);
            f.body = rewrite_block(f.body, ctx);
            Item::Fn(f)
        }
        Item::Class(mut c) => {
            let methods = std::mem::take(&mut c.methods);
            c.methods = methods
                .into_iter()
                .map(|mut m| {
                    rewrite_params(&mut m.params, ctx);
                    let body = std::mem::replace(
                        &mut m.body,
                        Block { stmts: Vec::new(), tail: None },
                    );
                    m.body = rewrite_block(body, ctx);
                    m
                })
                .collect();
            let static_methods = std::mem::take(&mut c.static_methods);
            c.static_methods = static_methods
                .into_iter()
                .map(|mut m| {
                    rewrite_params(&mut m.params, ctx);
                    let body = std::mem::replace(
                        &mut m.body,
                        Block { stmts: Vec::new(), tail: None },
                    );
                    m.body = rewrite_block(body, ctx);
                    m
                })
                .collect();
            let static_fields = std::mem::take(&mut c.static_fields);
            c.static_fields = static_fields
                .into_iter()
                .map(|mut sf| {
                    sf.value = rewrite_expr(sf.value, ctx);
                    sf
                })
                .collect();
            let properties = std::mem::take(&mut c.properties);
            c.properties = properties
                .into_iter()
                .map(|mut p| {
                    if let Some(g) = p.getter.as_mut() {
                        let body = std::mem::replace(
                            &mut g.body,
                            Block { stmts: Vec::new(), tail: None },
                        );
                        g.body = rewrite_block(body, ctx);
                    }
                    if let Some(s) = p.setter.as_mut() {
                        rewrite_params(&mut s.params, ctx);
                        let body = std::mem::replace(
                            &mut s.body,
                            Block { stmts: Vec::new(), tail: None },
                        );
                        s.body = rewrite_block(body, ctx);
                    }
                    p
                })
                .collect();
            Item::Class(c)
        }
        Item::Enum(e) => Item::Enum(e),
        Item::Use(u) => Item::Use(u),
        Item::Const(mut c) => {
            c.value = rewrite_expr(c.value, ctx);
            Item::Const(c)
        }
        Item::ExternStatic(s) => Item::ExternStatic(s),
        Item::ExternC(mut b) => {
            // Walk fn definitions inside the block so module-qualified
            // calls (`test.foo(x)`) get rewritten to `Call("test.foo", x)`
            // the same way they would in regular fn bodies.
            for inner in &mut b.items {
                match inner {
                    ilang_ast::ExternCItem::FnDef(f) => {
                        rewrite_params(&mut f.params, ctx);
                        let body = std::mem::replace(
                            &mut f.body,
                            Block { stmts: Vec::new(), tail: None },
                        );
                        f.body = rewrite_block(body, ctx);
                    }
                    ilang_ast::ExternCItem::Class(c) => {
                        let methods = std::mem::take(&mut c.methods);
                        c.methods = methods
                            .into_iter()
                            .map(|mut m| {
                                rewrite_params(&mut m.params, ctx);
                                let body = std::mem::replace(
                                    &mut m.body,
                                    Block { stmts: Vec::new(), tail: None },
                                );
                                m.body = rewrite_block(body, ctx);
                                m
                            })
                            .collect();
                        let static_methods = std::mem::take(&mut c.static_methods);
                        c.static_methods = static_methods
                            .into_iter()
                            .map(|mut m| {
                                rewrite_params(&mut m.params, ctx);
                                let body = std::mem::replace(
                                    &mut m.body,
                                    Block { stmts: Vec::new(), tail: None },
                                );
                                m.body = rewrite_block(body, ctx);
                                m
                            })
                            .collect();
                        let static_fields = std::mem::take(&mut c.static_fields);
                        c.static_fields = static_fields
                            .into_iter()
                            .map(|mut sf| {
                                sf.value = rewrite_expr(sf.value, ctx);
                                sf
                            })
                            .collect();
                        let properties = std::mem::take(&mut c.properties);
                        c.properties = properties
                            .into_iter()
                            .map(|mut p| {
                                if let Some(g) = p.getter.as_mut() {
                                    let body = std::mem::replace(
                                        &mut g.body,
                                        Block { stmts: Vec::new(), tail: None },
                                    );
                                    g.body = rewrite_block(body, ctx);
                                }
                                if let Some(s) = p.setter.as_mut() {
                                    rewrite_params(&mut s.params, ctx);
                                    let body = std::mem::replace(
                                        &mut s.body,
                                        Block { stmts: Vec::new(), tail: None },
                                    );
                                    s.body = rewrite_block(body, ctx);
                                }
                                p
                            })
                            .collect();
                    }
                    _ => {}
                }
            }
            Item::ExternC(b)
        }
    }
}

fn rewrite_block(b: Block, ctx: &Ctx) -> Block {
    Block {
        stmts: Vec::from(b.stmts).into_iter().map(|s| rewrite_stmt(s, ctx)).collect(),
        tail: b.tail.map(|e| Box::new(rewrite_expr(*e, ctx))),
    }
}

fn rewrite_stmt(s: Stmt, ctx: &Ctx) -> Stmt {
    let kind = match s.kind {
        StmtKind::Let { name, ty, value } => StmtKind::Let {
            name,
            ty,
            value: rewrite_expr(value, ctx),
        },
        StmtKind::LetTuple { elems, value } => StmtKind::LetTuple {
            elems,
            value: rewrite_expr(value, ctx),
        },
        StmtKind::LetStruct { class, fields, value } => StmtKind::LetStruct {
            class,
            fields,
            value: rewrite_expr(value, ctx),
        },
        StmtKind::Expr(e) => StmtKind::Expr(rewrite_expr(e, ctx)),
    };
    Stmt { kind, span: s.span }
}

fn rewrite_expr(e: Expr, ctx: &Ctx) -> Expr {
    let span = e.span;
    let kind = match e.kind {
        // Recurse first so any nested `module.Enum` chain in `obj`
        // collapses (`Field(Field(Var("utils"), "Color"), "red")` →
        // `Field(Var("utils.Color"), "red")`) before the enum-ctor
        // check. Module-name bumping is itself a Field rewrite below.
        ExprKind::Field { obj, name } => {
            let obj = rewrite_expr(*obj, ctx);
            // Whole-module reference: `module.X` collapses to a
            // qualified `Var("module.X")` so the loader-merged
            // top-level item with that exact name is found.
            if let ExprKind::Var(receiver) = &obj.kind {
                if ctx.modules.contains(&Symbol::intern(receiver.as_str())) {
                    return Expr::new(
                        ExprKind::Var(Symbol::intern(&format!("{receiver}.{name}"))),
                        span,
                    );
                }
                // Existing rule: enum unit ctor.
                if ctx.enums.contains(&Symbol::intern(receiver.as_str())) {
                    return Expr::new(
                        ExprKind::EnumCtor {
                            enum_name: receiver.clone(),
                            variant: name,
                            args: CtorArgs::Unit,
                        },
                        span,
                    );
                }
            }
            ExprKind::Field {
                obj: Box::new(obj),
                name,
            }
        }
        ExprKind::MethodCall { obj, method, args } => {
            let obj = rewrite_expr(*obj, ctx);
            let new_args: Vec<Expr> =
                Vec::from(args).into_iter().map(|a| rewrite_expr(a, ctx)).collect();
            if let ExprKind::Var(receiver) = &obj.kind {
                // Whole-module function call: `module.foo(args)`
                // becomes `Call("module.foo", args)`.
                if ctx.modules.contains(&Symbol::intern(receiver.as_str())) {
                    return Expr::new(
                        ExprKind::Call {
                            callee: Symbol::intern(&format!("{receiver}.{method}")),
                            args: new_args.into(),
                        },
                        span,
                    );
                }
                if ctx.enums.contains(&Symbol::intern(receiver.as_str())) {
                    return Expr::new(
                        ExprKind::EnumCtor {
                            enum_name: receiver.clone(),
                            variant: method,
                            args: CtorArgs::Tuple(new_args.into()),
                        },
                        span,
                    );
                }
            }
            ExprKind::MethodCall {
                obj: Box::new(obj),
                method,
                args: new_args.into(),
            }
        }
        // Recurse through everything else.
        ExprKind::Unary { op, expr } => ExprKind::Unary {
            op,
            expr: Box::new(rewrite_expr(*expr, ctx)),
        },
        ExprKind::Binary { op, lhs, rhs } => ExprKind::Binary {
            op,
            lhs: Box::new(rewrite_expr(*lhs, ctx)),
            rhs: Box::new(rewrite_expr(*rhs, ctx)),
        },
        ExprKind::Logical { op, lhs, rhs } => ExprKind::Logical {
            op,
            lhs: Box::new(rewrite_expr(*lhs, ctx)),
            rhs: Box::new(rewrite_expr(*rhs, ctx)),
        },
        ExprKind::Cast { expr, ty } => ExprKind::Cast {
            expr: Box::new(rewrite_expr(*expr, ctx)),
            ty,
        },
        ExprKind::TypeTest { expr, ty } => ExprKind::TypeTest {
            expr: Box::new(rewrite_expr(*expr, ctx)),
            ty,
        },
        ExprKind::TypeDowncast { expr, ty } => ExprKind::TypeDowncast {
            expr: Box::new(rewrite_expr(*expr, ctx)),
            ty,
        },
        ExprKind::FnExpr { params, ret, body } => ExprKind::FnExpr {
            params,
            ret,
            body: rewrite_block(body, ctx),
        },
        ExprKind::Call { callee, args } => ExprKind::Call {
            callee,
            args: Vec::from(args).into_iter().map(|a| rewrite_expr(a, ctx)).collect(),
        },
        ExprKind::New { class, type_args, args, init_method } => ExprKind::New {
            class,
            type_args,
            args: Vec::from(args).into_iter().map(|a| rewrite_expr(a, ctx)).collect(),
            init_method,
        },
        ExprKind::Block(b) => ExprKind::Block(rewrite_block(b, ctx)),
        ExprKind::If {
            cond,
            then_branch,
            else_branch,
        } => ExprKind::If {
            cond: Box::new(rewrite_expr(*cond, ctx)),
            then_branch: rewrite_block(then_branch, ctx),
            else_branch: else_branch.map(|e| Box::new(rewrite_expr(*e, ctx))),
        },
        ExprKind::IfLet {
            name,
            expr,
            then_branch,
            else_branch,
        } => ExprKind::IfLet {
            name,
            expr: Box::new(rewrite_expr(*expr, ctx)),
            then_branch: rewrite_block(then_branch, ctx),
            else_branch: else_branch.map(|e| Box::new(rewrite_expr(*e, ctx))),
        },
        ExprKind::While { cond, body } => ExprKind::While {
            cond: Box::new(rewrite_expr(*cond, ctx)),
            body: rewrite_block(body, ctx),
        },
        ExprKind::Loop { body } => ExprKind::Loop {
            body: rewrite_block(body, ctx),
        },
        ExprKind::ForIn { var, iter, body } => ExprKind::ForIn {
            var,
            iter: Box::new(rewrite_expr(*iter, ctx)),
            body: rewrite_block(body, ctx),
        },
        ExprKind::Range { start, end, inclusive } => ExprKind::Range {
            start: start.map(|s| Box::new(rewrite_expr(*s, ctx))),
            end: end.map(|e| Box::new(rewrite_expr(*e, ctx))),
            inclusive,
        },
        ExprKind::Closure { fn_name, captures } => {
            ExprKind::Closure { fn_name, captures }
        }
        ExprKind::SuperCall { method, args } => ExprKind::SuperCall {
            method,
            args: Vec::from(args).into_iter().map(|a| rewrite_expr(a, ctx)).collect(),
        },
        ExprKind::Return(opt) => {
            ExprKind::Return(opt.map(|e| Box::new(rewrite_expr(*e, ctx))))
        }
        ExprKind::Break(opt) => {
            ExprKind::Break(opt.map(|e| Box::new(rewrite_expr(*e, ctx))))
        }
        ExprKind::Assign { target, value } => ExprKind::Assign {
            target,
            value: Box::new(rewrite_expr(*value, ctx)),
        },
        ExprKind::AssignField { obj, field, value } => ExprKind::AssignField {
            obj: Box::new(rewrite_expr(*obj, ctx)),
            field,
            value: Box::new(rewrite_expr(*value, ctx)),
        },
        ExprKind::AssignIndex { obj, index, value } => ExprKind::AssignIndex {
            obj: Box::new(rewrite_expr(*obj, ctx)),
            index: Box::new(rewrite_expr(*index, ctx)),
            value: Box::new(rewrite_expr(*value, ctx)),
        },
        ExprKind::Array(items) => {
            ExprKind::Array(Vec::from(items).into_iter().map(|e| rewrite_expr(e, ctx)).collect())
        }
        ExprKind::Tuple(items) => {
            ExprKind::Tuple(Vec::from(items).into_iter().map(|e| rewrite_expr(e, ctx)).collect())
        }
        // Struct literal: `Foo { a: 1, b: 2 }` desugars to a block
        // `{ let __sl = new Foo(); __sl.a = 1; __sl.b = 2; __sl }`.
        // The temp name embeds the source position so nested struct
        // literals don't collide.
        ExprKind::StructLit { class, fields } => {
            let tmp: Symbol = format!("__struct_lit_{}_{}", span.line, span.col).into();
            let mut stmts: Vec<ilang_ast::Stmt> = Vec::with_capacity(fields.len() + 1);
            stmts.push(ilang_ast::Stmt {
                kind: ilang_ast::StmtKind::Let {
                    name: tmp,
                    ty: None,
                    value: Expr::new(
                        ExprKind::New {
                            class,
                            type_args: Box::new([]),
                            args: Box::new([]),
                            init_method: None,
                        },
                        span,
                    ),
                },
                span,
            });
            for (fname, fval) in fields {
                let assign = Expr::new(
                    ExprKind::AssignField {
                        obj: Box::new(Expr::new(ExprKind::Var(tmp), span)),
                        field: fname,
                        value: Box::new(rewrite_expr(fval, ctx)),
                    },
                    span,
                );
                stmts.push(ilang_ast::Stmt {
                    kind: ilang_ast::StmtKind::Expr(assign),
                    span,
                });
            }
            return Expr::new(
                ExprKind::Block(ilang_ast::Block {
                    stmts: stmts.into(),
                    tail: Some(Box::new(Expr::new(ExprKind::Var(tmp), span))),
                }),
                span,
            );
        }
        ExprKind::MapLit(entries) => ExprKind::MapLit(
            entries
                .into_iter()
                .map(|(k, v)| (rewrite_expr(k, ctx), rewrite_expr(v, ctx)))
                .collect(),
        ),
        ExprKind::Index { obj, index } => ExprKind::Index {
            obj: Box::new(rewrite_expr(*obj, ctx)),
            index: Box::new(rewrite_expr(*index, ctx)),
        },
        ExprKind::Some(inner) => ExprKind::Some(Box::new(rewrite_expr(*inner, ctx))),
        ExprKind::EnumCtor {
            enum_name,
            variant,
            args,
        } => ExprKind::EnumCtor {
            enum_name,
            variant,
            args: match args {
                CtorArgs::Unit => CtorArgs::Unit,
                CtorArgs::Tuple(es) => CtorArgs::Tuple(
                    Vec::from(es).into_iter().map(|e| rewrite_expr(e, ctx)).collect(),
                ),
                CtorArgs::Struct(fs) => CtorArgs::Struct(
                    fs.into_iter()
                        .map(|(n, e)| (n, rewrite_expr(e, ctx)))
                        .collect(),
                ),
            },
        },
        ExprKind::Match { scrutinee, arms } => ExprKind::Match {
            scrutinee: Box::new(rewrite_expr(*scrutinee, ctx)),
            arms: arms
                .into_iter()
                .map(|arm: MatchArm| MatchArm {
                    pattern: arm.pattern,
                    body: rewrite_expr(arm.body, ctx),
                    span: arm.span,
                })
                .collect(),
        },
        // Trivial nodes
        other @ (ExprKind::Int(_)
        | ExprKind::Float(_)
        | ExprKind::Bool(_)
        | ExprKind::Str(_)
        | ExprKind::Var(_)
        | ExprKind::This
        | ExprKind::None
        | ExprKind::Continue) => other,
    };
    Expr { kind, span }
}
