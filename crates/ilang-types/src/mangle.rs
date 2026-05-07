//! Post-typecheck pass that renames overloaded `Item::Fn` declarations
//! to per-overload mangled names and rewrites every `Call` site to
//! match. Downstream stages (loader-merge, monomorphization,
//! interpreter, JIT) then see plain non-overloaded fn names again.
//!
//! Names with only one declaration are left alone — keeps error
//! messages and stack traces readable.

use std::collections::{HashMap, HashSet};

use ilang_ast::{
    Block, ClassDecl, Expr, ExprKind, FieldDecl, FnDecl, Item, Param, Program, Span, Stmt,
    StmtKind, Symbol, Type, Variant, VariantPayload,
};

/// Mangle an overloaded fn name to `<name>__<param1>_<param2>_...`.
/// Each param-type Display rendering with `<`/`>`/`,`/spaces stripped
/// so the result is a usable identifier.
fn mangled_name(base: Symbol, params: &[Type]) -> Symbol {
    let base = base.as_str();
    if params.is_empty() {
        return Symbol::intern(&format!("{base}__"));
    }
    let mut s = String::from(base);
    s.push_str("__");
    for (i, p) in params.iter().enumerate() {
        if i > 0 {
            s.push('_');
        }
        let mut t = format!("{p}");
        t = t.replace([' ', '<', '>', ',', '?', '[', ']', '.', '(', ')'], "_");
        s.push_str(&t);
    }
    Symbol::intern(&s)
}

/// Apply the mangling pass. `picks[span] = (callee_name, sig_idx)`
/// gives the typechecker's chosen overload for each non-generic
/// **fn** call. `method_picks[span] = (class, method, sig_idx)` does
/// the same for class methods (including `init` selected at `new`).
pub fn mangle_overloads(
    prog: Program,
    picks: &HashMap<Span, (Symbol, usize)>,
    method_picks: &HashMap<Span, (Symbol, Symbol, usize)>,
    default_fills: &HashMap<Span, Vec<Expr>>,
) -> Program {
    // 1. Group Item::Fn entries by source name to see which ones are
    //    actually overloaded.
    let mut fn_indices: HashMap<Symbol, Vec<usize>> = HashMap::new();
    for (i, item) in prog.items.iter().enumerate() {
        if let Item::Fn(f) = item {
            fn_indices.entry(f.name.clone()).or_default().push(i);
        }
    }
    // Names with exactly one declaration: no mangling needed.
    let overloaded: HashSet<Symbol> = fn_indices
        .iter()
        .filter(|(_, v)| v.len() > 1)
        .map(|(k, _)| k.clone())
        .collect();

    // Same idea for class methods. `(class_name, method_name) → Vec<method-position-in-class.methods>`.
    // Also reach into `@extern(C) {}` blocks: classes declared
    // there can be overloaded the same way as top-level ones.
    let mut method_indices: HashMap<(Symbol, Symbol), Vec<usize>> = HashMap::new();
    let walk_class_methods = |c: &ClassDecl, method_indices: &mut HashMap<(Symbol, Symbol), Vec<usize>>| {
        for (i, m) in c.methods.iter().enumerate() {
            method_indices.entry((c.name.clone(), m.name.clone())).or_default().push(i);
        }
    };
    for item in &prog.items {
        match item {
            Item::Class(c) => walk_class_methods(c, &mut method_indices),
            Item::ExternC(b) => {
                for inner in &b.items {
                    if let ilang_ast::ExternCItem::Class(c) = inner {
                        walk_class_methods(c, &mut method_indices);
                    }
                }
            }
            _ => {}
        }
    }
    let overloaded_methods: HashSet<(Symbol, Symbol)> = method_indices
        .iter()
        .filter(|(_, v)| v.len() > 1)
        .map(|(k, _)| k.clone())
        .collect();

    if overloaded.is_empty() && overloaded_methods.is_empty() && default_fills.is_empty() {
        return prog;
    }

    // 2. Build per-(name, sig_idx) → mangled-name tables for both
    //    fns and class methods. Sig_idx is the index in the
    //    typechecker's overload list, which matches declaration
    //    order.
    let mut new_names: HashMap<(Symbol, usize), Symbol> = HashMap::new();
    let mut item_new_name: HashMap<usize, Symbol> = HashMap::new();
    for name in &overloaded {
        for (idx, item_pos) in fn_indices[name].iter().enumerate() {
            if let Item::Fn(f) = &prog.items[*item_pos] {
                let mangled = mangled_name(*name, &param_types(f));
                new_names.insert((name.clone(), idx), mangled.clone());
                item_new_name.insert(*item_pos, mangled);
            }
        }
    }

    let mut new_method_names: HashMap<(Symbol, Symbol, usize), Symbol> = HashMap::new();
    for (class_name, method_name) in &overloaded_methods {
        // Find the class — either at top level or inside an
        // `@extern(C) {}` block — and walk its methods in
        // declaration order; sig_idx is the position of each
        // matching name.
        let class_decl: Option<&ClassDecl> = prog.items.iter().find_map(|it| match it {
            Item::Class(c) if &c.name == class_name => Some(c),
            Item::ExternC(b) => b.items.iter().find_map(|inner| match inner {
                ilang_ast::ExternCItem::Class(c) if &c.name == class_name => Some(c),
                _ => None,
            }),
            _ => None,
        });
        if let Some(c) = class_decl {
            let mut sig_idx = 0;
            for m in &c.methods {
                if m.name == *method_name {
                    let mangled = mangled_name(*method_name, &param_types(m));
                    new_method_names.insert(
                        (class_name.clone(), method_name.clone(), sig_idx),
                        mangled,
                    );
                    sig_idx += 1;
                }
            }
        }
    }

    let ctx = Ctx {
        overloaded: &overloaded,
        new_names: &new_names,
        item_new_name: &item_new_name,
        overloaded_methods: &overloaded_methods,
        new_method_names: &new_method_names,
        picks,
        method_picks,
        default_fills,
    };

    // 3. Rewrite Items: rename matching FnDecls + class methods;
    //    recurse into bodies to rewrite Calls and MethodCalls.
    let new_items: Vec<Item> = prog
        .items
        .into_iter()
        .enumerate()
        .map(|(i, item)| rewrite_item(i, item, &ctx))
        .collect();
    let new_stmts: Vec<Stmt> = Vec::from(prog.stmts).into_iter().map(|s| rewrite_stmt(s, &ctx)).collect();
    let new_tail = prog.tail.map(|e| rewrite_expr(e, &ctx));

    Program {
        items: new_items.into(),
        stmts: new_stmts.into(),
        tail: new_tail,
    }
}

struct Ctx<'a> {
    overloaded: &'a HashSet<Symbol>,
    new_names: &'a HashMap<(Symbol, usize), Symbol>,
    item_new_name: &'a HashMap<usize, Symbol>,
    overloaded_methods: &'a HashSet<(Symbol, Symbol)>,
    new_method_names: &'a HashMap<(Symbol, Symbol, usize), Symbol>,
    picks: &'a HashMap<Span, (Symbol, usize)>,
    method_picks: &'a HashMap<Span, (Symbol, Symbol, usize)>,
    /// Per-call-site default-arg fills produced by the type checker.
    /// Each entry is the list of (already type-checked) trailing
    /// default expressions appended to the call's args during this
    /// rewrite. Empty for calls without missing trailing args.
    default_fills: &'a HashMap<Span, Vec<Expr>>,
}

fn param_types(f: &FnDecl) -> Vec<Type> {
    f.params.iter().map(|p| p.ty.clone()).collect()
}

fn rewrite_item(item_pos: usize, item: Item, ctx: &Ctx) -> Item {
    match item {
        Item::Fn(mut f) => {
            if let Some(mangled) = ctx.item_new_name.get(&item_pos) {
                f.name = mangled.clone();
            }
            f.body = rewrite_block(f.body, ctx);
            Item::Fn(f)
        }
        Item::Class(mut c) => {
            rewrite_class_in_place(&mut c, ctx);
            Item::Class(c)
        }
        Item::Enum(e) => Item::Enum(e),
        Item::Use(u) => Item::Use(u),
        Item::Const(c) => Item::Const(c),
        Item::ExternC(mut b) => {
            // Recurse into the block. ilang `FnDef` bodies need
            // call rewriting; ilang `Class` decls need both method
            // renaming and body rewriting — same as top-level
            // classes.
            for inner in b.items.iter_mut() {
                match inner {
                    ilang_ast::ExternCItem::FnDef(f) => {
                        let body = std::mem::replace(
                            &mut f.body,
                            Block { stmts: Vec::new(), tail: None },
                        );
                        f.body = rewrite_block(body, ctx);
                    }
                    ilang_ast::ExternCItem::Class(c) => {
                        rewrite_class_in_place(c, ctx);
                    }
                    _ => {}
                }
            }
            Item::ExternC(b)
        }
    }
}

fn rewrite_class_in_place(c: &mut ClassDecl, ctx: &Ctx) {
    // Rename overloaded methods. Walk in declaration order so
    // sig_idx matches what the typechecker recorded.
    let class_name = c.name.clone();
    let mut sig_counter: HashMap<Symbol, usize> = HashMap::new();
    for m in &mut c.methods {
        let body = std::mem::replace(
            &mut m.body,
            Block { stmts: Vec::new(), tail: None },
        );
        m.body = rewrite_block(body, ctx);
        let key = (class_name.clone(), m.name.clone());
        if ctx.overloaded_methods.contains(&key) {
            let idx = sig_counter.entry(m.name.clone()).or_insert(0);
            let mangled = ctx
                .new_method_names
                .get(&(class_name.clone(), m.name.clone(), *idx))
                .cloned();
            if let Some(new_name) = mangled {
                m.name = new_name;
            }
            *idx += 1;
        }
    }
    // Static method bodies need rewriting (calls to overloaded
    // fns) but the static methods themselves aren't currently
    // overloadable — no name munging.
    for m in &mut c.static_methods {
        let body = std::mem::replace(
            &mut m.body,
            Block { stmts: Vec::new(), tail: None },
        );
        m.body = rewrite_block(body, ctx);
    }
    // Property accessor bodies need rewriting too (their bodies
    // can contain calls to overloaded fns/methods). Properties
    // themselves aren't overloaded — no name change.
    for prop in &mut c.properties {
        if let Some(g) = prop.getter.as_mut() {
            let body = std::mem::replace(
                &mut g.body,
                Block { stmts: Vec::new(), tail: None },
            );
            g.body = rewrite_block(body, ctx);
        }
        if let Some(s) = prop.setter.as_mut() {
            let body = std::mem::replace(
                &mut s.body,
                Block { stmts: Vec::new(), tail: None },
            );
            s.body = rewrite_block(body, ctx);
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
        ExprKind::Call { callee, args } => {
            let new_callee = if ctx.overloaded.contains(&callee) {
                if let Some((name, idx)) = ctx.picks.get(&span) {
                    if name == &callee {
                        ctx.new_names
                            .get(&(callee.clone(), *idx))
                            .cloned()
                            .unwrap_or(callee)
                    } else {
                        callee
                    }
                } else {
                    callee
                }
            } else {
                callee
            };
            let mut new_args: Vec<Expr> =
                Vec::from(args).into_iter().map(|a| rewrite_expr(a, ctx)).collect();
            if let Some(fills) = ctx.default_fills.get(&span) {
                for d in fills {
                    new_args.push(rewrite_expr(d.clone(), ctx));
                }
            }
            ExprKind::Call {
                callee: new_callee,
                args: new_args.into(),
            }
        }
        // Mechanical recursion through every other expression shape.
        ExprKind::Int(n) => ExprKind::Int(n),
        ExprKind::Float(x) => ExprKind::Float(x),
        ExprKind::Bool(b) => ExprKind::Bool(b),
        ExprKind::Str(s) => ExprKind::Str(s),
        ExprKind::Var(n) => ExprKind::Var(n),
        ExprKind::This => ExprKind::This,
        ExprKind::None => ExprKind::None,
        ExprKind::Break(opt) => ExprKind::Break(opt.map(|e| Box::new(rewrite_expr(*e, ctx)))),
        ExprKind::Continue => ExprKind::Continue,
        ExprKind::Some(x) => ExprKind::Some(Box::new(rewrite_expr(*x, ctx))),
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
        ExprKind::Field { obj, name } => ExprKind::Field {
            obj: Box::new(rewrite_expr(*obj, ctx)),
            name,
        },
        ExprKind::MethodCall { obj, method, args } => {
            // Look up the typechecker's chosen overload for this site;
            // if the method is overloaded, rename to the mangled form.
            let new_method = if let Some((cls, m, idx)) = ctx.method_picks.get(&span) {
                if m == &method && ctx.overloaded_methods.contains(&(cls.clone(), m.clone())) {
                    ctx.new_method_names
                        .get(&(cls.clone(), m.clone(), *idx))
                        .cloned()
                        .unwrap_or(method)
                } else {
                    method
                }
            } else {
                method
            };
            let mut new_args: Vec<Expr> =
                Vec::from(args).into_iter().map(|a| rewrite_expr(a, ctx)).collect();
            if let Some(fills) = ctx.default_fills.get(&span) {
                for d in fills {
                    new_args.push(rewrite_expr(d.clone(), ctx));
                }
            }
            ExprKind::MethodCall {
                obj: Box::new(rewrite_expr(*obj, ctx)),
                method: new_method,
                args: new_args.into(),
            }
        }
        ExprKind::New { class, type_args, args, init_method: existing } => {
            // If `init` for this class is overloaded, set init_method
            // to the mangled name the typechecker selected. Otherwise
            // preserve any existing value (None for fresh AST).
            let new_init = if let Some((cls, m, idx)) = ctx.method_picks.get(&span) {
                if m == "init" && ctx.overloaded_methods.contains(&(cls.clone(), "init".into())) {
                    ctx.new_method_names
                        .get(&(cls.clone(), "init".into(), *idx))
                        .cloned()
                        .or(existing)
                } else {
                    existing
                }
            } else {
                existing
            };
            let mut new_args: Vec<Expr> =
                Vec::from(args).into_iter().map(|a| rewrite_expr(a, ctx)).collect();
            if let Some(fills) = ctx.default_fills.get(&span) {
                for d in fills {
                    new_args.push(rewrite_expr(d.clone(), ctx));
                }
            }
            ExprKind::New {
                class,
                type_args,
                args: new_args.into(),
                init_method: new_init,
            }
        }
        ExprKind::Block(b) => ExprKind::Block(rewrite_block(b, ctx)),
        ExprKind::If { cond, then_branch, else_branch } => ExprKind::If {
            cond: Box::new(rewrite_expr(*cond, ctx)),
            then_branch: rewrite_block(then_branch, ctx),
            else_branch: else_branch.map(|e| Box::new(rewrite_expr(*e, ctx))),
        },
        ExprKind::IfLet { name, expr, then_branch, else_branch } => ExprKind::IfLet {
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
        ExprKind::SuperCall { method, args } => ExprKind::SuperCall {
            method,
            args: Vec::from(args).into_iter().map(|a| rewrite_expr(a, ctx)).collect(),
        },
        ExprKind::Closure { fn_name, captures } => {
            ExprKind::Closure { fn_name, captures }
        }
        ExprKind::Return(opt) => ExprKind::Return(
            opt.map(|e| Box::new(rewrite_expr(*e, ctx))),
        ),
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
        ExprKind::Array(items) => ExprKind::Array(
            Vec::from(items).into_iter().map(|e| rewrite_expr(e, ctx)).collect(),
        ),
        ExprKind::Tuple(items) => ExprKind::Tuple(
            Vec::from(items).into_iter().map(|e| rewrite_expr(e, ctx)).collect(),
        ),
        ExprKind::StructLit { class, fields } => ExprKind::StructLit {
            class,
            fields: fields
                .into_iter()
                .map(|(n, e)| (n, rewrite_expr(e, ctx)))
                .collect(),
        },
        ExprKind::MapLit(entries) => ExprKind::MapLit(
            entries
                .into_iter()
                .map(|(k, v)| (
                    rewrite_expr(k, ctx),
                    rewrite_expr(v, ctx),
                ))
                .collect(),
        ),
        ExprKind::Index { obj, index } => ExprKind::Index {
            obj: Box::new(rewrite_expr(*obj, ctx)),
            index: Box::new(rewrite_expr(*index, ctx)),
        },
        ExprKind::EnumCtor { enum_name, variant, args } => ExprKind::EnumCtor {
            enum_name,
            variant,
            args: match args {
                ilang_ast::CtorArgs::Unit => ilang_ast::CtorArgs::Unit,
                ilang_ast::CtorArgs::Tuple(es) => ilang_ast::CtorArgs::Tuple(
                    Vec::from(es).into_iter().map(|e| rewrite_expr(e, ctx)).collect(),
                ),
                ilang_ast::CtorArgs::Struct(fs) => ilang_ast::CtorArgs::Struct(
                    fs.into_iter().map(|(n, e)| (n, rewrite_expr(e, ctx))).collect(),
                ),
            },
        },
        ExprKind::Match { scrutinee, arms } => ExprKind::Match {
            scrutinee: Box::new(rewrite_expr(*scrutinee, ctx)),
            arms: arms
                .into_iter()
                .map(|arm| ilang_ast::MatchArm {
                    pattern: arm.pattern,
                    body: rewrite_expr(arm.body, ctx),
                    span: arm.span,
                })
                .collect(),
        },
    };
    Expr { kind, span }
}

// Unused imports for now — keep for the inevitable extension to
// methods/struct rewrites.
#[allow(dead_code)]
type _Unused = (ClassDecl, FieldDecl, Param, Variant, VariantPayload);
