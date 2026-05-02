//! AST monomorphization pass: turn each generic class instantiation
//! (`Box<i64>`) into a concrete non-generic class (`Box<i64>` mangled
//! into a unique class name) by cloning the declaration and
//! substituting the type parameters throughout fields, method
//! signatures, and method bodies.
//!
//! After this pass runs, the program contains zero `Type::Generic`,
//! `Type::TypeVar`, or `ExprKind::New { type_args: !empty }` nodes —
//! the JIT pipeline can then proceed unchanged.
//!
//! Strategy: walk the program collecting `(class_name, [arg types])`
//! instantiation seeds, iteratively expand by substituting and
//! re-walking the cloned method bodies until a fixed point is reached
//! (a method body may reference further generic types). Replace the
//! original generic class declarations with the synthesized concrete
//! ones.

use std::collections::{HashMap, HashSet};

use ilang_ast::{
    Block, ClassDecl, Expr, ExprKind, FieldDecl, FnDecl, Item, Param, Program, Stmt,
    StmtKind, Type,
};

/// The unique key of a monomorphization request: class name + concrete
/// type arguments. We don't derive Hash on `Type`, so the worklist
/// uses the rendered mangled string for dedup; the args are kept
/// separately for substitution.
#[derive(Clone, Debug)]
struct InstKey {
    class: String,
    args: Vec<Type>,
}

fn mangle(class: &str, args: &[Type]) -> String {
    // Embed the concrete args in the class name. The result is not a
    // valid identifier (contains `<`, `,`, `>`), but class names live
    // as opaque strings throughout the JIT — we never re-parse them —
    // so this is safe and easy to debug.
    let mut s = class.to_string();
    s.push('<');
    for (i, a) in args.iter().enumerate() {
        if i > 0 {
            s.push_str(", ");
        }
        s.push_str(&a.to_string());
    }
    s.push('>');
    s
}

impl InstKey {
    fn mangled(&self) -> String {
        mangle(&self.class, &self.args)
    }
}

/// Hoist anonymous-function expressions out to top-level synthetic
/// fns. Each `fn(...) { ... }` becomes a fresh `Item::Fn` with a
/// generated name like `__anon_fn_0`, and the original `FnExpr` is
/// replaced with a `Var(synth_name)` reference. The JIT then sees
/// only named functions — call sites turn into ordinary indirect
/// calls (or direct calls when the var is shadowed by a `let`).
pub(crate) fn hoist_anon_fns(prog: &Program) -> Program {
    let mut counter: u32 = 0;
    let mut hoisted: Vec<Item> = Vec::new();
    let new_items: Vec<Item> = prog
        .items
        .iter()
        .map(|i| hoist_in_item(i, &mut counter, &mut hoisted))
        .collect();
    let new_stmts: Vec<Stmt> = prog
        .stmts
        .iter()
        .map(|s| hoist_in_stmt(s, &mut counter, &mut hoisted))
        .collect();
    let new_tail = prog
        .tail
        .as_ref()
        .map(|e| hoist_in_expr(e, &mut counter, &mut hoisted));
    let mut items = new_items;
    items.extend(hoisted);
    Program {
        items,
        stmts: new_stmts,
        tail: new_tail,
    }
}

fn fresh_anon_name(counter: &mut u32) -> String {
    let n = *counter;
    *counter += 1;
    format!("__anon_fn_{n}")
}

fn hoist_in_item(item: &Item, counter: &mut u32, hoisted: &mut Vec<Item>) -> Item {
    match item {
        Item::Fn(f) => Item::Fn(FnDecl {
            attrs: f.attrs.clone(),
            name: f.name.clone(),
            params: f.params.clone(),
            ret: f.ret.clone(),
            body: hoist_in_block(&f.body, counter, hoisted),
            span: f.span,
        }),
        Item::Class(c) => Item::Class(ClassDecl {
            name: c.name.clone(),
            type_params: c.type_params.clone(),
            fields: c.fields.clone(),
            methods: c
                .methods
                .iter()
                .map(|m| FnDecl {
                    attrs: m.attrs.clone(),
                    name: m.name.clone(),
                    params: m.params.clone(),
                    ret: m.ret.clone(),
                    body: hoist_in_block(&m.body, counter, hoisted),
                    span: m.span,
                })
                .collect(),
            span: c.span,
        }),
        Item::Enum(e) => Item::Enum(e.clone()),
        Item::Use(u) => Item::Use(u.clone()),
    }
}

fn hoist_in_block(b: &Block, counter: &mut u32, hoisted: &mut Vec<Item>) -> Block {
    Block {
        stmts: b
            .stmts
            .iter()
            .map(|s| hoist_in_stmt(s, counter, hoisted))
            .collect(),
        tail: b
            .tail
            .as_ref()
            .map(|e| Box::new(hoist_in_expr(e, counter, hoisted))),
    }
}

fn hoist_in_stmt(s: &Stmt, counter: &mut u32, hoisted: &mut Vec<Item>) -> Stmt {
    let kind = match &s.kind {
        StmtKind::Let { name, ty, value } => StmtKind::Let {
            name: name.clone(),
            ty: ty.clone(),
            value: hoist_in_expr(value, counter, hoisted),
        },
        StmtKind::Expr(e) => StmtKind::Expr(hoist_in_expr(e, counter, hoisted)),
    };
    Stmt {
        kind,
        span: s.span,
    }
}

fn hoist_in_expr(e: &Expr, counter: &mut u32, hoisted: &mut Vec<Item>) -> Expr {
    let kind = match &e.kind {
        ExprKind::FnExpr { params, ret, body } => {
            // First hoist any nested anon fns inside this body.
            let body = hoist_in_block(body, counter, hoisted);
            let name = fresh_anon_name(counter);
            hoisted.push(Item::Fn(FnDecl {
                attrs: Vec::new(),
                name: name.clone(),
                params: params.clone(),
                ret: ret.clone(),
                body,
                span: e.span,
            }));
            ExprKind::Var(name)
        }
        // Recurse through anything that might contain expressions.
        ExprKind::Some(inner) => {
            ExprKind::Some(Box::new(hoist_in_expr(inner, counter, hoisted)))
        }
        ExprKind::Unary { op, expr } => ExprKind::Unary {
            op: *op,
            expr: Box::new(hoist_in_expr(expr, counter, hoisted)),
        },
        ExprKind::Binary { op, lhs, rhs } => ExprKind::Binary {
            op: *op,
            lhs: Box::new(hoist_in_expr(lhs, counter, hoisted)),
            rhs: Box::new(hoist_in_expr(rhs, counter, hoisted)),
        },
        ExprKind::Logical { op, lhs, rhs } => ExprKind::Logical {
            op: *op,
            lhs: Box::new(hoist_in_expr(lhs, counter, hoisted)),
            rhs: Box::new(hoist_in_expr(rhs, counter, hoisted)),
        },
        ExprKind::Cast { expr, ty } => ExprKind::Cast {
            expr: Box::new(hoist_in_expr(expr, counter, hoisted)),
            ty: ty.clone(),
        },
        ExprKind::Call { callee, args } => ExprKind::Call {
            callee: callee.clone(),
            args: args.iter().map(|a| hoist_in_expr(a, counter, hoisted)).collect(),
        },
        ExprKind::Field { obj, name } => ExprKind::Field {
            obj: Box::new(hoist_in_expr(obj, counter, hoisted)),
            name: name.clone(),
        },
        ExprKind::MethodCall { obj, method, args } => ExprKind::MethodCall {
            obj: Box::new(hoist_in_expr(obj, counter, hoisted)),
            method: method.clone(),
            args: args.iter().map(|a| hoist_in_expr(a, counter, hoisted)).collect(),
        },
        ExprKind::New {
            class,
            type_args,
            args,
        } => ExprKind::New {
            class: class.clone(),
            type_args: type_args.clone(),
            args: args.iter().map(|a| hoist_in_expr(a, counter, hoisted)).collect(),
        },
        ExprKind::Block(b) => ExprKind::Block(hoist_in_block(b, counter, hoisted)),
        ExprKind::If {
            cond,
            then_branch,
            else_branch,
        } => ExprKind::If {
            cond: Box::new(hoist_in_expr(cond, counter, hoisted)),
            then_branch: hoist_in_block(then_branch, counter, hoisted),
            else_branch: else_branch
                .as_ref()
                .map(|e| Box::new(hoist_in_expr(e, counter, hoisted))),
        },
        ExprKind::IfLet {
            name,
            expr,
            then_branch,
            else_branch,
        } => ExprKind::IfLet {
            name: name.clone(),
            expr: Box::new(hoist_in_expr(expr, counter, hoisted)),
            then_branch: hoist_in_block(then_branch, counter, hoisted),
            else_branch: else_branch
                .as_ref()
                .map(|e| Box::new(hoist_in_expr(e, counter, hoisted))),
        },
        ExprKind::While { cond, body } => ExprKind::While {
            cond: Box::new(hoist_in_expr(cond, counter, hoisted)),
            body: hoist_in_block(body, counter, hoisted),
        },
        ExprKind::Loop { body } => ExprKind::Loop {
            body: hoist_in_block(body, counter, hoisted),
        },
        ExprKind::ForIn { var, iter, body } => ExprKind::ForIn {
            var: var.clone(),
            iter: Box::new(hoist_in_expr(iter, counter, hoisted)),
            body: hoist_in_block(body, counter, hoisted),
        },
        ExprKind::Return(opt) => ExprKind::Return(
            opt.as_ref().map(|e| Box::new(hoist_in_expr(e, counter, hoisted))),
        ),
        ExprKind::Assign { target, value } => ExprKind::Assign {
            target: target.clone(),
            value: Box::new(hoist_in_expr(value, counter, hoisted)),
        },
        ExprKind::AssignField { obj, field, value } => ExprKind::AssignField {
            obj: obj.clone(),
            field: field.clone(),
            value: Box::new(hoist_in_expr(value, counter, hoisted)),
        },
        ExprKind::AssignIndex { obj, index, value } => ExprKind::AssignIndex {
            obj: obj.clone(),
            index: index.clone(),
            value: Box::new(hoist_in_expr(value, counter, hoisted)),
        },
        ExprKind::Array(items) => ExprKind::Array(
            items.iter().map(|i| hoist_in_expr(i, counter, hoisted)).collect(),
        ),
        ExprKind::MapLit(entries) => ExprKind::MapLit(
            entries
                .iter()
                .map(|(k, v)| {
                    (
                        hoist_in_expr(k, counter, hoisted),
                        hoist_in_expr(v, counter, hoisted),
                    )
                })
                .collect(),
        ),
        ExprKind::Index { obj, index } => ExprKind::Index {
            obj: Box::new(hoist_in_expr(obj, counter, hoisted)),
            index: Box::new(hoist_in_expr(index, counter, hoisted)),
        },
        ExprKind::EnumCtor {
            enum_name,
            variant,
            args,
        } => ExprKind::EnumCtor {
            enum_name: enum_name.clone(),
            variant: variant.clone(),
            args: match args {
                ilang_ast::CtorArgs::Unit => ilang_ast::CtorArgs::Unit,
                ilang_ast::CtorArgs::Tuple(es) => ilang_ast::CtorArgs::Tuple(
                    es.iter().map(|e| hoist_in_expr(e, counter, hoisted)).collect(),
                ),
                ilang_ast::CtorArgs::Struct(fs) => ilang_ast::CtorArgs::Struct(
                    fs.iter()
                        .map(|(n, e)| (n.clone(), hoist_in_expr(e, counter, hoisted)))
                        .collect(),
                ),
            },
        },
        ExprKind::Match { scrutinee, arms } => ExprKind::Match {
            scrutinee: Box::new(hoist_in_expr(scrutinee, counter, hoisted)),
            arms: arms
                .iter()
                .map(|arm| ilang_ast::MatchArm {
                    pattern: arm.pattern.clone(),
                    body: hoist_in_expr(&arm.body, counter, hoisted),
                    span: arm.span,
                })
                .collect(),
        },
        ExprKind::Int(n) => ExprKind::Int(*n),
        ExprKind::Float(f) => ExprKind::Float(*f),
        ExprKind::Bool(b) => ExprKind::Bool(*b),
        ExprKind::Str(s) => ExprKind::Str(s.clone()),
        ExprKind::Var(n) => ExprKind::Var(n.clone()),
        ExprKind::This => ExprKind::This,
        ExprKind::None => ExprKind::None,
        ExprKind::Break => ExprKind::Break,
        ExprKind::Continue => ExprKind::Continue,
    };
    Expr {
        kind,
        span: e.span,
    }
}

/// Run the pass. Returns a new `Program` where every reference to a
/// generic class has been replaced by a concrete monomorphized
/// instantiation. Non-generic items pass through unchanged.
pub(crate) fn monomorphize(prog: &Program) -> Program {
    // Index original (generic) class decls by name so we can clone +
    // substitute on demand.
    let generic_classes: HashMap<String, &ClassDecl> = prog
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Class(c) if !c.type_params.is_empty() => Some((c.name.clone(), c)),
            _ => None,
        })
        .collect();

    // Generic-enum names are tracked too — we DON'T monomorphize
    // them (per-instantiation enum layouts would need significant
    // JIT infrastructure). Instead the rewrite step leaves their
    // `Type::Generic` references alone so the JIT errors out with a
    // clear "generic enum + JIT" message via UnsupportedType.
    let generic_enum_names: HashSet<String> = prog
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Enum(e) if !e.type_params.is_empty() => Some(e.name.clone()),
            _ => None,
        })
        .collect();
    // Built-in generic enums (Result) are also enum names.
    let mut generic_enum_names = generic_enum_names;
    generic_enum_names.insert("Result".into());
    GENERIC_ENUM_NAMES.with(|set| {
        *set.borrow_mut() = generic_enum_names.clone();
    });

    // Seed the worklist by scanning the entire (untransformed) program
    // for generic instantiations.
    let mut needed: HashSet<String> = HashSet::new();
    let mut worklist: Vec<InstKey> = Vec::new();
    let seed = |t: &Type, needed: &mut HashSet<String>, work: &mut Vec<InstKey>| {
        collect_instantiations(t, needed, work);
    };
    for item in &prog.items {
        match item {
            Item::Class(c) => {
                // Generic-class bodies still get scanned with the
                // class's params left as TypeVars; only concrete refs
                // (no TypeVar) seed the worklist. The substitution
                // pass will expand them properly later.
                for f in &c.fields {
                    seed(&f.ty, &mut needed, &mut worklist);
                }
                for m in &c.methods {
                    scan_fn(m, &mut needed, &mut worklist);
                }
            }
            Item::Fn(f) => scan_fn(f, &mut needed, &mut worklist),
            Item::Enum(_) | Item::Use(_) => {}
        }
    }
    for s in &prog.stmts {
        scan_stmt(s, &mut needed, &mut worklist);
    }
    if let Some(t) = &prog.tail {
        scan_expr(t, &mut needed, &mut worklist);
    }

    // Iteratively monomorphize each pending instantiation. As we
    // substitute T → concrete in method bodies, new generic refs may
    // appear (e.g. `class Wrap<T> { f(): Box<T> { ... } }` instantiated
    // with T=i64 yields a `Box<i64>` ref) — those go back on the
    // worklist.
    let mut synthesized: HashMap<String, ClassDecl> = HashMap::new();
    while let Some(key) = worklist.pop() {
        let mangled = key.mangled();
        if synthesized.contains_key(&mangled) {
            continue;
        }
        let template = match generic_classes.get(&key.class) {
            Some(c) => *c,
            None => continue, // Concrete or undefined — let the type checker have caught it.
        };
        if template.type_params.len() != key.args.len() {
            continue; // arity mismatch — type checker should have rejected
        }
        let new_class = specialize_class(template, &key.args, &mangled);
        // Walk the new class's substituted bodies for further generic refs.
        for f in &new_class.fields {
            scan_type(&f.ty, &mut needed, &mut worklist);
        }
        for m in &new_class.methods {
            scan_fn(m, &mut needed, &mut worklist);
        }
        synthesized.insert(mangled, new_class);
    }

    // Build the output program: drop the generic class definitions,
    // keep everything else, and append the synthesized concrete classes.
    // Then rewrite Type::Generic → Type::Object(mangled) and
    // New.type_args → empty + class = mangled, throughout every
    // remaining node.
    let mut out_items: Vec<Item> = Vec::new();
    for item in &prog.items {
        match item {
            Item::Class(c) if !c.type_params.is_empty() => { /* drop */ }
            other => out_items.push(rewrite_item(other)),
        }
    }
    for (_, c) in synthesized {
        out_items.push(Item::Class(c));
    }
    let stmts: Vec<Stmt> = prog.stmts.iter().map(rewrite_stmt).collect();
    let tail = prog.tail.as_ref().map(rewrite_expr);
    Program {
        items: out_items,
        stmts,
        tail,
    }
}

// ─── seed-collection helpers (no substitution, just observe) ─────────

fn scan_fn(f: &FnDecl, needed: &mut HashSet<String>, work: &mut Vec<InstKey>) {
    for Param { ty, .. } in &f.params {
        scan_type(ty, needed, work);
    }
    if let Some(t) = &f.ret {
        scan_type(t, needed, work);
    }
    scan_block(&f.body, needed, work);
}

fn scan_block(b: &Block, needed: &mut HashSet<String>, work: &mut Vec<InstKey>) {
    for s in &b.stmts {
        scan_stmt(s, needed, work);
    }
    if let Some(t) = &b.tail {
        scan_expr(t, needed, work);
    }
}

fn scan_stmt(s: &Stmt, needed: &mut HashSet<String>, work: &mut Vec<InstKey>) {
    match &s.kind {
        StmtKind::Let { value, ty, .. } => {
            if let Some(t) = ty {
                scan_type(t, needed, work);
            }
            scan_expr(value, needed, work);
        }
        StmtKind::Expr(e) => scan_expr(e, needed, work),
    }
}

fn scan_expr(e: &Expr, needed: &mut HashSet<String>, work: &mut Vec<InstKey>) {
    match &e.kind {
        ExprKind::Int(_)
        | ExprKind::Float(_)
        | ExprKind::Bool(_)
        | ExprKind::Str(_)
        | ExprKind::Var(_)
        | ExprKind::This
        | ExprKind::None
        | ExprKind::Break
        | ExprKind::Continue => {}
        ExprKind::Some(inner) => scan_expr(inner, needed, work),
        ExprKind::Unary { expr, .. } => scan_expr(expr, needed, work),
        ExprKind::Binary { lhs, rhs, .. } | ExprKind::Logical { lhs, rhs, .. } => {
            scan_expr(lhs, needed, work);
            scan_expr(rhs, needed, work);
        }
        ExprKind::Cast { expr, ty } => {
            scan_expr(expr, needed, work);
            scan_type(ty, needed, work);
        }
        ExprKind::FnExpr { params, ret, body } => {
            for p in params {
                scan_type(&p.ty, needed, work);
            }
            if let Some(t) = ret {
                scan_type(t, needed, work);
            }
            scan_block(body, needed, work);
        }
        ExprKind::Call { args, .. } => {
            for a in args {
                scan_expr(a, needed, work);
            }
        }
        ExprKind::Field { obj, .. } => scan_expr(obj, needed, work),
        ExprKind::MethodCall { obj, args, .. } => {
            scan_expr(obj, needed, work);
            for a in args {
                scan_expr(a, needed, work);
            }
        }
        ExprKind::New { type_args, args, class } => {
            for t in type_args {
                scan_type(t, needed, work);
            }
            for a in args {
                scan_expr(a, needed, work);
            }
            // The `new` itself is also an instantiation seed.
            if !type_args.is_empty() {
                push_inst(class.clone(), type_args.clone(), needed, work);
            }
        }
        ExprKind::Block(b) => scan_block(b, needed, work),
        ExprKind::If {
            cond,
            then_branch,
            else_branch,
        } => {
            scan_expr(cond, needed, work);
            scan_block(then_branch, needed, work);
            if let Some(e) = else_branch {
                scan_expr(e, needed, work);
            }
        }
        ExprKind::IfLet {
            expr,
            then_branch,
            else_branch,
            ..
        } => {
            scan_expr(expr, needed, work);
            scan_block(then_branch, needed, work);
            if let Some(e) = else_branch {
                scan_expr(e, needed, work);
            }
        }
        ExprKind::While { cond, body } => {
            scan_expr(cond, needed, work);
            scan_block(body, needed, work);
        }
        ExprKind::Loop { body } => scan_block(body, needed, work),
        ExprKind::ForIn { iter, body, .. } => {
            scan_expr(iter, needed, work);
            scan_block(body, needed, work);
        }
        ExprKind::Return(opt) => {
            if let Some(e) = opt {
                scan_expr(e, needed, work);
            }
        }
        ExprKind::Assign { value, .. } => scan_expr(value, needed, work),
        ExprKind::AssignField { obj, value, .. } => {
            scan_expr(obj, needed, work);
            scan_expr(value, needed, work);
        }
        ExprKind::AssignIndex { obj, index, value } => {
            scan_expr(obj, needed, work);
            scan_expr(index, needed, work);
            scan_expr(value, needed, work);
        }
        ExprKind::Array(items) => {
            for i in items {
                scan_expr(i, needed, work);
            }
        }
        ExprKind::MapLit(entries) => {
            for (k, v) in entries {
                scan_expr(k, needed, work);
                scan_expr(v, needed, work);
            }
        }
        ExprKind::Index { obj, index } => {
            scan_expr(obj, needed, work);
            scan_expr(index, needed, work);
        }
        ExprKind::EnumCtor { args, .. } => {
            if let ilang_ast::CtorArgs::Tuple(es) = args {
                for e in es {
                    scan_expr(e, needed, work);
                }
            } else if let ilang_ast::CtorArgs::Struct(fs) = args {
                for (_, e) in fs {
                    scan_expr(e, needed, work);
                }
            }
        }
        ExprKind::Match { scrutinee, arms } => {
            scan_expr(scrutinee, needed, work);
            for arm in arms {
                scan_expr(&arm.body, needed, work);
            }
        }
    }
}

fn scan_type(t: &Type, needed: &mut HashSet<String>, work: &mut Vec<InstKey>) {
    collect_instantiations(t, needed, work);
}

fn collect_instantiations(
    t: &Type,
    needed: &mut HashSet<String>,
    work: &mut Vec<InstKey>,
) {
    match t {
        Type::Generic { base, args } => {
            // Only enqueue concrete instantiations (no remaining type
            // variables). A `Box<T>` reference inside `class Wrap<T>`'s
            // body is left as-is here; substitute_class produces the
            // concrete `Box<i64>` later, which seeds the worklist on
            // the next round.
            if !contains_type_var(t) {
                push_inst(base.clone(), args.clone(), needed, work);
            }
            for a in args {
                collect_instantiations(a, needed, work);
            }
        }
        Type::Array { elem, .. } => collect_instantiations(elem, needed, work),
        Type::Optional(inner) | Type::Weak(inner) => {
            collect_instantiations(inner, needed, work)
        }
        Type::Fn { params, ret } => {
            for p in params {
                collect_instantiations(p, needed, work);
            }
            collect_instantiations(ret, needed, work);
        }
        _ => {}
    }
}

fn push_inst(
    class: String,
    args: Vec<Type>,
    needed: &mut HashSet<String>,
    work: &mut Vec<InstKey>,
) {
    let key = InstKey { class, args };
    if needed.insert(key.mangled()) {
        work.push(key);
    }
}

fn contains_type_var(t: &Type) -> bool {
    match t {
        Type::TypeVar(_) => true,
        Type::Array { elem, .. } => contains_type_var(elem),
        Type::Optional(inner) | Type::Weak(inner) => contains_type_var(inner),
        Type::Generic { args, .. } => args.iter().any(contains_type_var),
        Type::Fn { params, ret } => {
            params.iter().any(contains_type_var) || contains_type_var(ret)
        }
        _ => false,
    }
}

// ─── specialization: clone a generic class with substituted types ────

fn specialize_class(c: &ClassDecl, args: &[Type], mangled: &str) -> ClassDecl {
    let params = c.type_params.clone();
    // Concrete generic args (e.g. T = Box<i64>) need to be collapsed
    // to their mangled `Object("Box<i64>")` form before substitution,
    // otherwise nested instantiations leak through as `Type::Generic`.
    let args: Vec<Type> = args.iter().map(rewrite_type).collect();
    let args = &args[..];
    let fields = c
        .fields
        .iter()
        .map(|f| FieldDecl {
            name: f.name.clone(),
            ty: subst_type(&f.ty, &params, args),
            span: f.span,
        })
        .collect();
    let methods = c
        .methods
        .iter()
        .map(|m| specialize_fn(m, &params, args))
        .collect();
    ClassDecl {
        name: mangled.to_string(),
        type_params: Vec::new(),
        fields,
        methods,
        span: c.span,
    }
}

fn specialize_fn(f: &FnDecl, params: &[String], args: &[Type]) -> FnDecl {
    FnDecl {
        name: f.name.clone(),
        params: f
            .params
            .iter()
            .map(|p| Param {
                name: p.name.clone(),
                ty: subst_type(&p.ty, params, args),
                span: p.span,
            })
            .collect(),
        ret: f.ret.as_ref().map(|t| subst_type(t, params, args)),
        body: subst_block(&f.body, params, args),
        attrs: f.attrs.clone(),
        span: f.span,
    }
}

fn subst_block(b: &Block, params: &[String], args: &[Type]) -> Block {
    Block {
        stmts: b.stmts.iter().map(|s| subst_stmt(s, params, args)).collect(),
        tail: b.tail.as_ref().map(|e| Box::new(subst_expr(e, params, args))),
    }
}

fn subst_stmt(s: &Stmt, params: &[String], args: &[Type]) -> Stmt {
    let kind = match &s.kind {
        StmtKind::Let { name, ty, value } => StmtKind::Let {
            name: name.clone(),
            ty: ty.as_ref().map(|t| subst_type(t, params, args)),
            value: subst_expr(value, params, args),
        },
        StmtKind::Expr(e) => StmtKind::Expr(subst_expr(e, params, args)),
    };
    Stmt {
        kind,
        span: s.span,
    }
}

fn subst_expr(e: &Expr, params: &[String], args: &[Type]) -> Expr {
    let kind = match &e.kind {
        ExprKind::Int(n) => ExprKind::Int(*n),
        ExprKind::Float(f) => ExprKind::Float(*f),
        ExprKind::Bool(b) => ExprKind::Bool(*b),
        ExprKind::Str(s) => ExprKind::Str(s.clone()),
        ExprKind::Var(n) => ExprKind::Var(n.clone()),
        ExprKind::This => ExprKind::This,
        ExprKind::None => ExprKind::None,
        ExprKind::Some(inner) => ExprKind::Some(Box::new(subst_expr(inner, params, args))),
        ExprKind::Break => ExprKind::Break,
        ExprKind::Continue => ExprKind::Continue,
        ExprKind::Unary { op, expr } => ExprKind::Unary {
            op: *op,
            expr: Box::new(subst_expr(expr, params, args)),
        },
        ExprKind::Binary { op, lhs, rhs } => ExprKind::Binary {
            op: *op,
            lhs: Box::new(subst_expr(lhs, params, args)),
            rhs: Box::new(subst_expr(rhs, params, args)),
        },
        ExprKind::Logical { op, lhs, rhs } => ExprKind::Logical {
            op: *op,
            lhs: Box::new(subst_expr(lhs, params, args)),
            rhs: Box::new(subst_expr(rhs, params, args)),
        },
        ExprKind::Cast { expr, ty } => ExprKind::Cast {
            expr: Box::new(subst_expr(expr, params, args)),
            ty: subst_type(ty, params, args),
        },
        ExprKind::FnExpr {
            params: ps,
            ret,
            body,
        } => ExprKind::FnExpr {
            params: ps
                .iter()
                .map(|p| ilang_ast::Param {
                    name: p.name.clone(),
                    ty: subst_type(&p.ty, params, args),
                    span: p.span,
                })
                .collect(),
            ret: ret.as_ref().map(|t| subst_type(t, params, args)),
            body: subst_block(body, params, args),
        },
        ExprKind::Call { callee, args: a } => ExprKind::Call {
            callee: callee.clone(),
            args: a.iter().map(|x| subst_expr(x, params, args)).collect(),
        },
        ExprKind::Field { obj, name } => ExprKind::Field {
            obj: Box::new(subst_expr(obj, params, args)),
            name: name.clone(),
        },
        ExprKind::MethodCall { obj, method, args: a } => ExprKind::MethodCall {
            obj: Box::new(subst_expr(obj, params, args)),
            method: method.clone(),
            args: a.iter().map(|x| subst_expr(x, params, args)).collect(),
        },
        ExprKind::New {
            class,
            type_args,
            args: a,
        } => ExprKind::New {
            class: class.clone(),
            type_args: type_args.iter().map(|t| subst_type(t, params, args)).collect(),
            args: a.iter().map(|x| subst_expr(x, params, args)).collect(),
        },
        ExprKind::Block(b) => ExprKind::Block(subst_block(b, params, args)),
        ExprKind::If {
            cond,
            then_branch,
            else_branch,
        } => ExprKind::If {
            cond: Box::new(subst_expr(cond, params, args)),
            then_branch: subst_block(then_branch, params, args),
            else_branch: else_branch.as_ref().map(|e| Box::new(subst_expr(e, params, args))),
        },
        ExprKind::IfLet {
            name,
            expr,
            then_branch,
            else_branch,
        } => ExprKind::IfLet {
            name: name.clone(),
            expr: Box::new(subst_expr(expr, params, args)),
            then_branch: subst_block(then_branch, params, args),
            else_branch: else_branch.as_ref().map(|e| Box::new(subst_expr(e, params, args))),
        },
        ExprKind::While { cond, body } => ExprKind::While {
            cond: Box::new(subst_expr(cond, params, args)),
            body: subst_block(body, params, args),
        },
        ExprKind::Loop { body } => ExprKind::Loop {
            body: subst_block(body, params, args),
        },
        ExprKind::ForIn { var, iter, body } => ExprKind::ForIn {
            var: var.clone(),
            iter: Box::new(subst_expr(iter, params, args)),
            body: subst_block(body, params, args),
        },
        ExprKind::Return(opt) => ExprKind::Return(
            opt.as_ref().map(|e| Box::new(subst_expr(e, params, args))),
        ),
        ExprKind::Assign { target, value } => ExprKind::Assign {
            target: target.clone(),
            value: Box::new(subst_expr(value, params, args)),
        },
        ExprKind::AssignField { obj, field, value } => ExprKind::AssignField {
            obj: obj.clone(),
            field: field.clone(),
            value: Box::new(subst_expr(value, params, args)),
        },
        ExprKind::AssignIndex { obj, index, value } => ExprKind::AssignIndex {
            obj: obj.clone(),
            index: index.clone(),
            value: Box::new(subst_expr(value, params, args)),
        },
        ExprKind::Array(items) => ExprKind::Array(
            items.iter().map(|e| subst_expr(e, params, args)).collect(),
        ),
        ExprKind::MapLit(entries) => ExprKind::MapLit(
            entries
                .iter()
                .map(|(k, v)| (subst_expr(k, params, args), subst_expr(v, params, args)))
                .collect(),
        ),
        ExprKind::Index { obj, index } => ExprKind::Index {
            obj: Box::new(subst_expr(obj, params, args)),
            index: Box::new(subst_expr(index, params, args)),
        },
        ExprKind::EnumCtor {
            enum_name,
            variant,
            args: a,
        } => ExprKind::EnumCtor {
            enum_name: enum_name.clone(),
            variant: variant.clone(),
            args: match a {
                ilang_ast::CtorArgs::Unit => ilang_ast::CtorArgs::Unit,
                ilang_ast::CtorArgs::Tuple(es) => ilang_ast::CtorArgs::Tuple(
                    es.iter().map(|e| subst_expr(e, params, args)).collect(),
                ),
                ilang_ast::CtorArgs::Struct(fs) => ilang_ast::CtorArgs::Struct(
                    fs.iter()
                        .map(|(n, e)| (n.clone(), subst_expr(e, params, args)))
                        .collect(),
                ),
            },
        },
        ExprKind::Match { scrutinee, arms } => ExprKind::Match {
            scrutinee: Box::new(subst_expr(scrutinee, params, args)),
            arms: arms
                .iter()
                .map(|arm| ilang_ast::MatchArm {
                    pattern: arm.pattern.clone(),
                    body: subst_expr(&arm.body, params, args),
                    span: arm.span,
                })
                .collect(),
        },
    };
    Expr {
        kind,
        span: e.span,
    }
}

fn subst_type(t: &Type, params: &[String], args: &[Type]) -> Type {
    match t {
        // The parser emits `Type::Object(name)` for any user-named type.
        // Inside a generic class body, references to the class's own
        // type parameters arrive here as `Object("T")` — treat those
        // as type variables and substitute. The type checker already
        // performs the same conceptual rewrite via `rewrite_type_params`.
        Type::Object(name) | Type::TypeVar(name) => params
            .iter()
            .position(|p| p == name)
            .and_then(|i| args.get(i).cloned())
            .unwrap_or_else(|| t.clone()),
        Type::Generic { base, args: targs } => {
            let new_args: Vec<Type> =
                targs.iter().map(|a| subst_type(a, params, args)).collect();
            // Once concrete (no TypeVar left), collapse to Object(mangled).
            let g = Type::Generic {
                base: base.clone(),
                args: new_args.clone(),
            };
            if !contains_type_var(&g) {
                Type::Object(
                    InstKey {
                        class: base.clone(),
                        args: new_args,
                    }
                    .mangled(),
                )
            } else {
                g
            }
        }
        Type::Array { elem, fixed } => Type::Array {
            elem: Box::new(subst_type(elem, params, args)),
            fixed: *fixed,
        },
        Type::Optional(inner) => Type::Optional(Box::new(subst_type(inner, params, args))),
        Type::Weak(inner) => Type::Weak(Box::new(subst_type(inner, params, args))),
        Type::Fn { params: ps, ret } => Type::Fn {
            params: ps.iter().map(|p| subst_type(p, params, args)).collect(),
            ret: Box::new(subst_type(ret, params, args)),
        },
        _ => t.clone(),
    }
}

// ─── rewrite pass: collapse Generic refs in non-generic items ────────

fn rewrite_item(item: &Item) -> Item {
    match item {
        Item::Class(c) => Item::Class(ClassDecl {
            name: c.name.clone(),
            type_params: c.type_params.clone(),
            fields: c
                .fields
                .iter()
                .map(|f| FieldDecl {
                    name: f.name.clone(),
                    ty: rewrite_type(&f.ty),
                    span: f.span,
                })
                .collect(),
            methods: c.methods.iter().map(rewrite_fn).collect(),
            span: c.span,
        }),
        Item::Fn(f) => Item::Fn(rewrite_fn(f)),
        Item::Enum(e) => Item::Enum(e.clone()),
        Item::Use(u) => Item::Use(u.clone()),
    }
}

fn rewrite_fn(f: &FnDecl) -> FnDecl {
    FnDecl {
        name: f.name.clone(),
        params: f
            .params
            .iter()
            .map(|p| Param {
                name: p.name.clone(),
                ty: rewrite_type(&p.ty),
                span: p.span,
            })
            .collect(),
        ret: f.ret.as_ref().map(rewrite_type),
        body: rewrite_block(&f.body),
        attrs: f.attrs.clone(),
        span: f.span,
    }
}

fn rewrite_block(b: &Block) -> Block {
    Block {
        stmts: b.stmts.iter().map(rewrite_stmt).collect(),
        tail: b.tail.as_ref().map(|e| Box::new(rewrite_expr(e))),
    }
}

fn rewrite_stmt(s: &Stmt) -> Stmt {
    let kind = match &s.kind {
        StmtKind::Let { name, ty, value } => StmtKind::Let {
            name: name.clone(),
            ty: ty.as_ref().map(rewrite_type),
            value: rewrite_expr(value),
        },
        StmtKind::Expr(e) => StmtKind::Expr(rewrite_expr(e)),
    };
    Stmt {
        kind,
        span: s.span,
    }
}

fn rewrite_expr(e: &Expr) -> Expr {
    let kind = match &e.kind {
        ExprKind::Cast { expr, ty } => ExprKind::Cast {
            expr: Box::new(rewrite_expr(expr)),
            ty: rewrite_type(ty),
        },
        ExprKind::FnExpr { params, ret, body } => ExprKind::FnExpr {
            params: params
                .iter()
                .map(|p| ilang_ast::Param {
                    name: p.name.clone(),
                    ty: rewrite_type(&p.ty),
                    span: p.span,
                })
                .collect(),
            ret: ret.as_ref().map(rewrite_type),
            body: rewrite_block(body),
        },
        ExprKind::New {
            class,
            type_args,
            args,
        } => {
            // Concrete generic instantiation → call into the
            // monomorphized class by its mangled name.
            let new_args: Vec<Expr> = args.iter().map(rewrite_expr).collect();
            if type_args.is_empty() {
                ExprKind::New {
                    class: class.clone(),
                    type_args: Vec::new(),
                    args: new_args,
                }
            } else {
                let mangled = InstKey {
                    class: class.clone(),
                    args: type_args.clone(),
                }
                .mangled();
                ExprKind::New {
                    class: mangled,
                    type_args: Vec::new(),
                    args: new_args,
                }
            }
        }
        // Mechanical recursion through the rest. We could derive this
        // if we had a Visitor trait, but the AST is small enough that
        // an explicit walk is the cheapest thing to read.
        ExprKind::Unary { op, expr } => ExprKind::Unary {
            op: *op,
            expr: Box::new(rewrite_expr(expr)),
        },
        ExprKind::Binary { op, lhs, rhs } => ExprKind::Binary {
            op: *op,
            lhs: Box::new(rewrite_expr(lhs)),
            rhs: Box::new(rewrite_expr(rhs)),
        },
        ExprKind::Logical { op, lhs, rhs } => ExprKind::Logical {
            op: *op,
            lhs: Box::new(rewrite_expr(lhs)),
            rhs: Box::new(rewrite_expr(rhs)),
        },
        ExprKind::Call { callee, args } => ExprKind::Call {
            callee: callee.clone(),
            args: args.iter().map(rewrite_expr).collect(),
        },
        ExprKind::Field { obj, name } => ExprKind::Field {
            obj: Box::new(rewrite_expr(obj)),
            name: name.clone(),
        },
        ExprKind::MethodCall { obj, method, args } => ExprKind::MethodCall {
            obj: Box::new(rewrite_expr(obj)),
            method: method.clone(),
            args: args.iter().map(rewrite_expr).collect(),
        },
        ExprKind::Block(b) => ExprKind::Block(rewrite_block(b)),
        ExprKind::If {
            cond,
            then_branch,
            else_branch,
        } => ExprKind::If {
            cond: Box::new(rewrite_expr(cond)),
            then_branch: rewrite_block(then_branch),
            else_branch: else_branch.as_ref().map(|e| Box::new(rewrite_expr(e))),
        },
        ExprKind::IfLet {
            name,
            expr,
            then_branch,
            else_branch,
        } => ExprKind::IfLet {
            name: name.clone(),
            expr: Box::new(rewrite_expr(expr)),
            then_branch: rewrite_block(then_branch),
            else_branch: else_branch.as_ref().map(|e| Box::new(rewrite_expr(e))),
        },
        ExprKind::While { cond, body } => ExprKind::While {
            cond: Box::new(rewrite_expr(cond)),
            body: rewrite_block(body),
        },
        ExprKind::Loop { body } => ExprKind::Loop {
            body: rewrite_block(body),
        },
        ExprKind::ForIn { var, iter, body } => ExprKind::ForIn {
            var: var.clone(),
            iter: Box::new(rewrite_expr(iter)),
            body: rewrite_block(body),
        },
        ExprKind::Return(opt) => {
            ExprKind::Return(opt.as_ref().map(|e| Box::new(rewrite_expr(e))))
        }
        ExprKind::Assign { target, value } => ExprKind::Assign {
            target: target.clone(),
            value: Box::new(rewrite_expr(value)),
        },
        ExprKind::AssignField { obj, field, value } => ExprKind::AssignField {
            obj: obj.clone(),
            field: field.clone(),
            value: Box::new(rewrite_expr(value)),
        },
        ExprKind::AssignIndex { obj, index, value } => ExprKind::AssignIndex {
            obj: obj.clone(),
            index: index.clone(),
            value: Box::new(rewrite_expr(value)),
        },
        ExprKind::Array(items) => {
            ExprKind::Array(items.iter().map(rewrite_expr).collect())
        }
        ExprKind::MapLit(entries) => ExprKind::MapLit(
            entries
                .iter()
                .map(|(k, v)| (rewrite_expr(k), rewrite_expr(v)))
                .collect(),
        ),
        ExprKind::Index { obj, index } => ExprKind::Index {
            obj: Box::new(rewrite_expr(obj)),
            index: Box::new(rewrite_expr(index)),
        },
        ExprKind::Some(inner) => ExprKind::Some(Box::new(rewrite_expr(inner))),
        ExprKind::EnumCtor {
            enum_name,
            variant,
            args,
        } => ExprKind::EnumCtor {
            enum_name: enum_name.clone(),
            variant: variant.clone(),
            args: match args {
                ilang_ast::CtorArgs::Unit => ilang_ast::CtorArgs::Unit,
                ilang_ast::CtorArgs::Tuple(es) => {
                    ilang_ast::CtorArgs::Tuple(es.iter().map(rewrite_expr).collect())
                }
                ilang_ast::CtorArgs::Struct(fs) => ilang_ast::CtorArgs::Struct(
                    fs.iter().map(|(n, e)| (n.clone(), rewrite_expr(e))).collect(),
                ),
            },
        },
        ExprKind::Match { scrutinee, arms } => ExprKind::Match {
            scrutinee: Box::new(rewrite_expr(scrutinee)),
            arms: arms
                .iter()
                .map(|arm| ilang_ast::MatchArm {
                    pattern: arm.pattern.clone(),
                    body: rewrite_expr(&arm.body),
                    span: arm.span,
                })
                .collect(),
        },
        // Trivial nodes pass through.
        ExprKind::Int(n) => ExprKind::Int(*n),
        ExprKind::Float(f) => ExprKind::Float(*f),
        ExprKind::Bool(b) => ExprKind::Bool(*b),
        ExprKind::Str(s) => ExprKind::Str(s.clone()),
        ExprKind::Var(n) => ExprKind::Var(n.clone()),
        ExprKind::This => ExprKind::This,
        ExprKind::None => ExprKind::None,
        ExprKind::Break => ExprKind::Break,
        ExprKind::Continue => ExprKind::Continue,
    };
    Expr {
        kind,
        span: e.span,
    }
}

// Thread-local set of generic-enum names. Populated at the top of
// `monomorphize()`; consulted by `rewrite_type` to decide whether a
// `Type::Generic { base, args }` should be collapsed to a mangled
// `Object` (class case) or left as-is (enum case — JIT errors out
// later with a clear "generic enum + JIT unsupported" message).
thread_local! {
    static GENERIC_ENUM_NAMES: std::cell::RefCell<HashSet<String>> =
        std::cell::RefCell::new(HashSet::new());
}

fn is_generic_enum(name: &str) -> bool {
    GENERIC_ENUM_NAMES.with(|set| set.borrow().contains(name))
}

/// Collapse `Type::Generic { Box, [i64] }` to `Type::Object("Box<i64>")`
/// so the JIT pipeline (which only knows `Object`) routes to the
/// monomorphized class. Recurses through Array/Optional/Weak.
fn rewrite_type(t: &Type) -> Type {
    match t {
        Type::Generic { base, args } => {
            let new_args: Vec<Type> = args.iter().map(rewrite_type).collect();
            // Generic enums aren't monomorphized — leave them as
            // `Type::Generic` so the JIT's `from_ast` errors with a
            // clear UnsupportedType. Classes get the mangled name.
            if is_generic_enum(base) {
                Type::Generic {
                    base: base.clone(),
                    args: new_args,
                }
            } else {
                Type::Object(
                    InstKey {
                        class: base.clone(),
                        args: new_args,
                    }
                    .mangled(),
                )
            }
        }
        Type::Array { elem, fixed } => Type::Array {
            elem: Box::new(rewrite_type(elem)),
            fixed: *fixed,
        },
        Type::Optional(inner) => Type::Optional(Box::new(rewrite_type(inner))),
        Type::Weak(inner) => Type::Weak(Box::new(rewrite_type(inner))),
        Type::Fn { params, ret } => Type::Fn {
            params: params.iter().map(rewrite_type).collect(),
            ret: Box::new(rewrite_type(ret)),
        },
        _ => t.clone(),
    }
}
