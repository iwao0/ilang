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
    Block, ClassDecl, EnumDecl, Expr, ExprKind, FieldDecl, FnDecl, Item, Param, Program, Span,
    Stmt, StmtKind, Symbol, Type, Variant, VariantPayload,
};

/// The unique key of a monomorphization request: class name + concrete
/// type arguments. We don't derive Hash on `Type`, so the worklist
/// uses the rendered mangled string for dedup; the args are kept
/// separately for substitution.
#[derive(Clone, Debug)]
struct InstKey {
    class: Symbol,
    args: Vec<Type>,
}

fn mangle(class: &str, args: &[Type]) -> Symbol {
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
    Symbol::intern(&s)
}

impl InstKey {
    fn mangled(&self) -> Symbol {
        mangle(self.class.as_str(), &self.args)
    }
}

/// Hoist anonymous-function expressions out to top-level synthetic
/// fns. Each `fn(...) { ... }` becomes a fresh `Item::Fn` with a
/// generated name like `__anon_fn_0`, and the original `FnExpr` is
/// replaced with a `Var(synth_name)` reference. The JIT then sees
/// only named functions — call sites turn into ordinary indirect
/// calls (or direct calls when the var is shadowed by a `let`).
/// Per-closure-wrapper metadata produced by the hoist pass and
/// consumed by the JIT compiler to lay out closure structs.
#[derive(Debug, Clone)]
pub(crate) struct ClosureMetaIn {
    pub user_param_tys: Vec<ilang_ast::Type>,
    pub ret_ty: Option<ilang_ast::Type>,
    pub captures: Vec<(Symbol, ilang_ast::Type)>,
    pub span: ilang_ast::Span,
    /// Lexical class when the wrapper was hoisted from inside a
    /// class method body. Used by the JIT to restore
    /// `lc.current_class` at wrapper lower time and by the second
    /// type-check pass to allow `this` / `super` references in the
    /// wrapper body.
    pub this_class: Option<Symbol>,
}

/// Bundle of state threaded through the hoist walkers.
pub(crate) struct HoistCtx<'a> {
    pub counter: &'a mut u32,
    pub hoisted: &'a mut Vec<Item>,
    /// FnExpr span → captured (name, type) list (from typechecker).
    pub captures_map: &'a std::collections::HashMap<
        ilang_ast::Span,
        Vec<(Symbol, ilang_ast::Type)>,
    >,
    /// FnExpr span → enclosing class symbol when the body uses `this`.
    /// Drives the synthetic `this` capture and `ExprKind::This` →
    /// `Var("this")` rewrite below.
    pub this_class_map: &'a std::collections::HashMap<ilang_ast::Span, Symbol>,
    /// wrapper_name → metadata. Filled in as we hoist FnExprs.
    pub closure_meta:
        &'a mut std::collections::HashMap<Symbol, ClosureMetaIn>,
}

pub(crate) fn hoist_anon_fns(
    prog: &Program,
    fn_expr_captures: &std::collections::HashMap<
        ilang_ast::Span,
        Vec<(Symbol, ilang_ast::Type)>,
    >,
    fn_expr_this_class: &std::collections::HashMap<ilang_ast::Span, Symbol>,
) -> (Program, std::collections::HashMap<Symbol, ClosureMetaIn>) {
    let mut counter: u32 = 0;
    let mut hoisted: Vec<Item> = Vec::new();
    let mut closure_meta: std::collections::HashMap<Symbol, ClosureMetaIn> =
        std::collections::HashMap::new();
    let mut ctx = HoistCtx {
        counter: &mut counter,
        hoisted: &mut hoisted,
        captures_map: fn_expr_captures,
        this_class_map: fn_expr_this_class,
        closure_meta: &mut closure_meta,
    };
    let new_items: Vec<Item> = prog
        .items
        .iter()
        .map(|i| hoist_in_item(i, &mut ctx))
        .collect();
    let new_stmts: Vec<Stmt> = prog
        .stmts
        .iter()
        .map(|s| hoist_in_stmt(s, &mut ctx))
        .collect();
    let new_tail = prog
        .tail
        .as_ref()
        .map(|e| hoist_in_expr(e, &mut ctx));
    let mut items = new_items;
    items.extend(hoisted);
    (
        Program {
            items,
            stmts: new_stmts.into(),
            tail: new_tail,
        },
        closure_meta,
    )
}

fn fresh_anon_name(counter: &mut u32) -> Symbol {
    let n = *counter;
    *counter += 1;
    Symbol::intern(&format!("__anon_fn_{n}"))
}

fn hoist_in_item(item: &Item, ctx: &mut HoistCtx) -> Item {
    match item {
        Item::Fn(f) => Item::Fn(FnDecl {
            attrs: f.attrs.clone(),
            name: f.name.clone(),
            type_params: f.type_params.clone(),
            params: f.params.clone(),
            ret: f.ret.clone(),
            body: hoist_in_block(&f.body, ctx),
            span: f.span,
        is_override: f.is_override,
        }),
        Item::Class(c) => Item::Class(ClassDecl {
            extern_lib: c.extern_lib.clone(),
            is_repr_c: c.is_repr_c,
            is_packed: c.is_packed,
            is_union: c.is_union,
            name: c.name.clone(),
            parent: c.parent.clone(),
            type_params: c.type_params.clone(),
            fields: c.fields.clone(),
            methods: c
                .methods
                .iter()
                .map(|m| FnDecl {
                    attrs: m.attrs.clone(),
                    name: m.name.clone(),
                    type_params: m.type_params.clone(),
                    params: m.params.clone(),
                    ret: m.ret.clone(),
                    body: hoist_in_block(&m.body, ctx),
                    span: m.span,
                is_override: m.is_override,
                })
                .collect(),
            static_methods: c
                .static_methods
                .iter()
                .map(|m| FnDecl {
                    attrs: m.attrs.clone(),
                    name: m.name.clone(),
                    type_params: m.type_params.clone(),
                    params: m.params.clone(),
                    ret: m.ret.clone(),
                    body: hoist_in_block(&m.body, ctx),
                    span: m.span,
                is_override: m.is_override,
                })
                .collect(),
            static_fields: c.static_fields.clone(),
            properties: c
                .properties
                .iter()
                .map(|p| ilang_ast::PropertyDecl {
                    name: p.name.clone(),
                    ty: p.ty.clone(),
                    getter: p.getter.as_ref().map(|g| FnDecl {
                        attrs: g.attrs.clone(),
                        name: g.name.clone(),
                        type_params: g.type_params.clone(),
                        params: g.params.clone(),
                        ret: g.ret.clone(),
                        body: hoist_in_block(&g.body, ctx),
                        span: g.span,
                    is_override: g.is_override,
                    }),
                    setter: p.setter.as_ref().map(|s| FnDecl {
                        attrs: s.attrs.clone(),
                        name: s.name.clone(),
                        type_params: s.type_params.clone(),
                        params: s.params.clone(),
                        ret: s.ret.clone(),
                        body: hoist_in_block(&s.body, ctx),
                        span: s.span,
                    is_override: s.is_override,
                    }),
                    span: p.span,
                })
                .collect(),
            span: c.span,
        }),
        Item::Enum(e) => Item::Enum(e.clone()),
        Item::Use(u) => Item::Use(u.clone()),
        Item::Const(c) => Item::Const(c.clone()),
        Item::ExternC(b) => Item::ExternC(b.clone()),
        Item::ExternStatic(s) => Item::ExternStatic(s.clone()),
    }
}

fn hoist_in_block(b: &Block, ctx: &mut HoistCtx) -> Block {
    Block {
        stmts: b
            .stmts
            .iter()
            .map(|s| hoist_in_stmt(s, ctx))
            .collect(),
        tail: b
            .tail
            .as_ref()
            .map(|e| Box::new(hoist_in_expr(e, ctx))),
    }
}

fn hoist_in_stmt(s: &Stmt, ctx: &mut HoistCtx) -> Stmt {
    let kind = match &s.kind {
        StmtKind::Let { name, ty, value } => StmtKind::Let {
            name: name.clone(),
            ty: ty.clone(),
            value: hoist_in_expr(value, ctx),
        },
        StmtKind::LetTuple { elems, value } => StmtKind::LetTuple {
            elems: elems.clone(),
            value: hoist_in_expr(value, ctx),
        },
        StmtKind::LetStruct { class, fields, value } => StmtKind::LetStruct {
            class: class.clone(),
            fields: fields.clone(),
            value: hoist_in_expr(value, ctx),
        },
        StmtKind::Expr(e) => StmtKind::Expr(hoist_in_expr(e, ctx)),
    };
    Stmt {
        kind,
        span: s.span,
    }
}

fn hoist_in_expr(e: &Expr, ctx: &mut HoistCtx) -> Expr {
    let kind = match &e.kind {
        ExprKind::FnExpr { params, ret, body } => {
            // First hoist any nested anon fns inside this body.
            let body = hoist_in_block(body, ctx);
            let name = fresh_anon_name(ctx.counter);
            // Look up captures recorded by the typechecker for this
            // FnExpr (keyed by the FnExpr's source span).
            let mut captures = ctx
                .captures_map
                .get(&e.span)
                .cloned()
                .unwrap_or_default();
            // If this closure was built inside a class method body
            // and refers to `this`, prepend a synthetic `this`
            // capture so codegen has somewhere to read the
            // pointer from. The wrapper itself is lowered with
            // `lc.this` populated from this capture (compiler.rs),
            // which makes both `ExprKind::This` and
            // `super.method(...)` work transparently in the body —
            // no AST rewrite needed. The class symbol is also
            // stashed on the closure meta so the second TC and the
            // SuperCall lowerer can restore the lexical class.
            let this_class = ctx.this_class_map.get(&e.span).cloned();
            if let Some(class_name) = this_class.clone() {
                let this_sym: Symbol = "this".into();
                if !captures.iter().any(|(n, _)| *n == this_sym) {
                    captures.insert(
                        0,
                        (this_sym, ilang_ast::Type::Object(class_name)),
                    );
                }
            }
            // Wrapper takes a hidden env_ptr first param so the same
            // calling convention applies to capture-free anon fns and
            // closures alike. Inside the body, captured Var(name)
            // references are resolved to env loads at lower-time
            // (see lower_expr's Var handler).
            let mut wrapper_params = Vec::with_capacity(params.len() + 1);
            wrapper_params.push(ilang_ast::Param {
                name: "__env".into(),
                ty: ilang_ast::Type::I64,
                span: e.span,
                default: None,
            });
            wrapper_params.extend(params.iter().cloned());
            ctx.hoisted.push(Item::Fn(FnDecl {
                attrs: Box::new([]),
                name: name.clone(),
                type_params: Box::new([]),
                params: wrapper_params.into(),
                ret: ret.clone(),
                body,
                span: e.span,
                is_override: false,
            }));
            ctx.closure_meta.insert(
                name.clone(),
                ClosureMetaIn {
                    user_param_tys: params.iter().map(|p| p.ty.clone()).collect(),
                    ret_ty: ret.clone(),
                    captures: captures.clone(),
                    span: e.span,
                    this_class,
                },
            );
            ExprKind::Closure { fn_name: name, captures: captures.into() }
        }
        // Recurse through anything that might contain expressions.
        ExprKind::Some(inner) => {
            ExprKind::Some(Box::new(hoist_in_expr(inner, ctx)))
        }
        ExprKind::Unary { op, expr } => ExprKind::Unary {
            op: *op,
            expr: Box::new(hoist_in_expr(expr, ctx)),
        },
        ExprKind::Binary { op, lhs, rhs } => ExprKind::Binary {
            op: *op,
            lhs: Box::new(hoist_in_expr(lhs, ctx)),
            rhs: Box::new(hoist_in_expr(rhs, ctx)),
        },
        ExprKind::Logical { op, lhs, rhs } => ExprKind::Logical {
            op: *op,
            lhs: Box::new(hoist_in_expr(lhs, ctx)),
            rhs: Box::new(hoist_in_expr(rhs, ctx)),
        },
        ExprKind::Cast { expr, ty } => ExprKind::Cast {
            expr: Box::new(hoist_in_expr(expr, ctx)),
            ty: ty.clone(),
        },
        ExprKind::TypeTest { expr, ty } => ExprKind::TypeTest {
            expr: Box::new(hoist_in_expr(expr, ctx)),
            ty: ty.clone(),
        },
        ExprKind::TypeDowncast { expr, ty } => ExprKind::TypeDowncast {
            expr: Box::new(hoist_in_expr(expr, ctx)),
            ty: ty.clone(),
        },
        ExprKind::Call { callee, args } => ExprKind::Call {
            callee: callee.clone(),
            args: args.iter().map(|a| hoist_in_expr(a, ctx)).collect(),
        },
        ExprKind::SuperCall { method, args } => ExprKind::SuperCall {
            method: method.clone(),
            args: args.iter().map(|a| hoist_in_expr(a, ctx)).collect(),
        },
        ExprKind::Closure { fn_name, captures } => ExprKind::Closure {
            fn_name: fn_name.clone(),
            captures: captures.clone(),
        },
        ExprKind::Field { obj, name } => ExprKind::Field {
            obj: Box::new(hoist_in_expr(obj, ctx)),
            name: name.clone(),
        },
        ExprKind::MethodCall { obj, method, args } => ExprKind::MethodCall {
            obj: Box::new(hoist_in_expr(obj, ctx)),
            method: method.clone(),
            args: args.iter().map(|a| hoist_in_expr(a, ctx)).collect(),
        },
        ExprKind::New { class, type_args, args, init_method } => ExprKind::New {
            class: class.clone(),
            type_args: type_args.clone(),
            args: args.iter().map(|a| hoist_in_expr(a, ctx)).collect(), init_method: init_method.clone(),
        },
        ExprKind::Block(b) => ExprKind::Block(hoist_in_block(b, ctx)),
        ExprKind::If {
            cond,
            then_branch,
            else_branch,
        } => ExprKind::If {
            cond: Box::new(hoist_in_expr(cond, ctx)),
            then_branch: hoist_in_block(then_branch, ctx),
            else_branch: else_branch
                .as_ref()
                .map(|e| Box::new(hoist_in_expr(e, ctx))),
        },
        ExprKind::IfLet {
            name,
            expr,
            then_branch,
            else_branch,
        } => ExprKind::IfLet {
            name: name.clone(),
            expr: Box::new(hoist_in_expr(expr, ctx)),
            then_branch: hoist_in_block(then_branch, ctx),
            else_branch: else_branch
                .as_ref()
                .map(|e| Box::new(hoist_in_expr(e, ctx))),
        },
        ExprKind::While { cond, body } => ExprKind::While {
            cond: Box::new(hoist_in_expr(cond, ctx)),
            body: hoist_in_block(body, ctx),
        },
        ExprKind::Loop { body } => ExprKind::Loop {
            body: hoist_in_block(body, ctx),
        },
        ExprKind::ForIn { var, iter, body } => ExprKind::ForIn {
            var: var.clone(),
            iter: Box::new(hoist_in_expr(iter, ctx)),
            body: hoist_in_block(body, ctx),
        },
        ExprKind::Range { start, end, inclusive } => ExprKind::Range {
            start: start.as_ref().map(|s| Box::new(hoist_in_expr(s, ctx))),
            end: end.as_ref().map(|e| Box::new(hoist_in_expr(e, ctx))),
            inclusive: *inclusive,
        },
        ExprKind::Return(opt) => ExprKind::Return(
            opt.as_ref().map(|e| Box::new(hoist_in_expr(e, ctx))),
        ),
        ExprKind::Break(opt) => ExprKind::Break(
            opt.as_ref().map(|e| Box::new(hoist_in_expr(e, ctx))),
        ),
        ExprKind::Assign { target, value } => ExprKind::Assign {
            target: target.clone(),
            value: Box::new(hoist_in_expr(value, ctx)),
        },
        ExprKind::AssignField { obj, field, value } => ExprKind::AssignField {
            obj: obj.clone(),
            field: field.clone(),
            value: Box::new(hoist_in_expr(value, ctx)),
        },
        ExprKind::AssignIndex { obj, index, value } => ExprKind::AssignIndex {
            obj: obj.clone(),
            index: index.clone(),
            value: Box::new(hoist_in_expr(value, ctx)),
        },
        ExprKind::Array(items) => ExprKind::Array(
            items.iter().map(|i| hoist_in_expr(i, ctx)).collect(),
        ),
        ExprKind::Tuple(items) => ExprKind::Tuple(
            items.iter().map(|i| hoist_in_expr(i, ctx)).collect(),
        ),
        ExprKind::StructLit { class, fields } => ExprKind::StructLit {
            class: class.clone(),
            fields: fields
                .iter()
                .map(|(n, e)| (n.clone(), hoist_in_expr(e, ctx)))
                .collect(),
        },
        ExprKind::MapLit(entries) => ExprKind::MapLit(
            entries
                .iter()
                .map(|(k, v)| {
                    (
                        hoist_in_expr(k, ctx),
                        hoist_in_expr(v, ctx),
                    )
                })
                .collect(),
        ),
        ExprKind::Index { obj, index } => ExprKind::Index {
            obj: Box::new(hoist_in_expr(obj, ctx)),
            index: Box::new(hoist_in_expr(index, ctx)),
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
                    es.iter().map(|e| hoist_in_expr(e, ctx)).collect(),
                ),
                ilang_ast::CtorArgs::Struct(fs) => ilang_ast::CtorArgs::Struct(
                    fs.iter()
                        .map(|(n, e)| (n.clone(), hoist_in_expr(e, ctx)))
                        .collect(),
                ),
            },
        },
        ExprKind::Match { scrutinee, arms } => ExprKind::Match {
            scrutinee: Box::new(hoist_in_expr(scrutinee, ctx)),
            arms: arms
                .iter()
                .map(|arm| ilang_ast::MatchArm {
                    pattern: arm.pattern.clone(),
                    body: hoist_in_expr(&arm.body, ctx),
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
    let generic_classes: HashMap<Symbol, &ClassDecl> = prog
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
    let generic_enum_names: HashSet<Symbol> = prog
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
    let mut needed: HashSet<Symbol> = HashSet::new();
    let mut worklist: Vec<InstKey> = Vec::new();
    let seed = |t: &Type, needed: &mut HashSet<Symbol>, work: &mut Vec<InstKey>| {
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
            Item::Enum(_) | Item::Use(_) | Item::Const(_) | Item::ExternStatic(_) | Item::ExternC(_) => {}
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
    let mut synthesized: HashMap<Symbol, ClassDecl> = HashMap::new();
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
        let new_class = specialize_class(template, &key.args, mangled.as_str());
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

fn scan_fn(f: &FnDecl, needed: &mut HashSet<Symbol>, work: &mut Vec<InstKey>) {
    for Param { ty, .. } in &f.params {
        scan_type(ty, needed, work);
    }
    if let Some(t) = &f.ret {
        scan_type(t, needed, work);
    }
    scan_block(&f.body, needed, work);
}

fn scan_block(b: &Block, needed: &mut HashSet<Symbol>, work: &mut Vec<InstKey>) {
    for s in &b.stmts {
        scan_stmt(s, needed, work);
    }
    if let Some(t) = &b.tail {
        scan_expr(t, needed, work);
    }
}

fn scan_stmt(s: &Stmt, needed: &mut HashSet<Symbol>, work: &mut Vec<InstKey>) {
    match &s.kind {
        StmtKind::Let { value, ty, .. } => {
            if let Some(t) = ty {
                scan_type(t, needed, work);
            }
            scan_expr(value, needed, work);
        }
        StmtKind::LetTuple { value, .. }
        | StmtKind::LetStruct { value, .. } => scan_expr(value, needed, work),
        StmtKind::Expr(e) => scan_expr(e, needed, work),
    }
}

fn scan_expr(e: &Expr, needed: &mut HashSet<Symbol>, work: &mut Vec<InstKey>) {
    match &e.kind {
        ExprKind::Int(_)
        | ExprKind::Float(_)
        | ExprKind::Bool(_)
        | ExprKind::Str(_)
        | ExprKind::Var(_)
        | ExprKind::This
        | ExprKind::None
        | ExprKind::Continue => {}
        ExprKind::Break(opt) => {
            if let Some(e) = opt {
                scan_expr(e, needed, work);
            }
        }
        ExprKind::Some(inner) => scan_expr(inner, needed, work),
        ExprKind::Unary { expr, .. } => scan_expr(expr, needed, work),
        ExprKind::Binary { lhs, rhs, .. } | ExprKind::Logical { lhs, rhs, .. } => {
            scan_expr(lhs, needed, work);
            scan_expr(rhs, needed, work);
        }
        ExprKind::Cast { expr, ty }
        | ExprKind::TypeTest { expr, ty }
        | ExprKind::TypeDowncast { expr, ty } => {
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
        ExprKind::SuperCall { args, .. } => {
            for a in args {
                scan_expr(a, needed, work);
            }
        }
        ExprKind::Closure { .. } => {}
        ExprKind::Field { obj, .. } => scan_expr(obj, needed, work),
        ExprKind::MethodCall { obj, args, .. } => {
            scan_expr(obj, needed, work);
            for a in args {
                scan_expr(a, needed, work);
            }
        }
        ExprKind::New { type_args, args, class, init_method: _ } => {
            for t in type_args {
                scan_type(t, needed, work);
            }
            for a in args {
                scan_expr(a, needed, work);
            }
            // The `new` itself is also an instantiation seed.
            if !type_args.is_empty() {
                push_inst(class.clone(), type_args.to_vec(), needed, work);
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
        ExprKind::Range { start, end, .. } => {
            if let Some(s) = start {
                scan_expr(s, needed, work);
            }
            if let Some(e) = end {
                scan_expr(e, needed, work);
            }
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
        ExprKind::Tuple(items) => {
            for i in items {
                scan_expr(i, needed, work);
            }
        }
        ExprKind::StructLit { fields, .. } => {
            for (_, e) in fields {
                scan_expr(e, needed, work);
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

fn scan_type(t: &Type, needed: &mut HashSet<Symbol>, work: &mut Vec<InstKey>) {
    collect_instantiations(t, needed, work);
}

fn collect_instantiations(
    t: &Type,
    needed: &mut HashSet<Symbol>,
    work: &mut Vec<InstKey>,
) {
    match t {
        Type::Generic(g) => {
            // Only enqueue concrete instantiations (no remaining type
            // variables). A `Box<T>` reference inside `class Wrap<T>`'s
            // body is left as-is here; substitute_class produces the
            // concrete `Box<i64>` later, which seeds the worklist on
            // the next round.
            if !contains_type_var(t) {
                push_inst(g.base.clone(), g.args.to_vec(), needed, work);
            }
            for a in &g.args {
                collect_instantiations(a, needed, work);
            }
        }
        Type::Array { elem, .. } => collect_instantiations(elem, needed, work),
        Type::Optional(inner) | Type::Weak(inner) => {
            collect_instantiations(inner, needed, work)
        }
        Type::Fn(ft) => {
            for p in &ft.params {
                collect_instantiations(p, needed, work);
            }
            collect_instantiations(&ft.ret, needed, work);
        }
        _ => {}
    }
}

fn push_inst(
    class: Symbol,
    args: Vec<Type>,
    needed: &mut HashSet<Symbol>,
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
        Type::Generic(g) => g.args.iter().any(contains_type_var),
        Type::Fn(ft) => {
            ft.params.iter().any(contains_type_var) || contains_type_var(&ft.ret)
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
            span: f.span, bits: f.bits,
        })
        .collect();
    let methods = c
        .methods
        .iter()
        .map(|m| specialize_fn(m, &params, args))
        .collect();
    let static_methods = c
        .static_methods
        .iter()
        .map(|m| specialize_fn(m, &params, args))
        .collect();
    let properties = c
        .properties
        .iter()
        .map(|p| ilang_ast::PropertyDecl {
            name: p.name.clone(),
            ty: subst_type(&p.ty, &params, args),
            getter: p.getter.as_ref().map(|g| specialize_fn(g, &params, args)),
            setter: p.setter.as_ref().map(|s| specialize_fn(s, &params, args)),
            span: p.span,
        })
        .collect();
    ClassDecl {
        extern_lib: c.extern_lib.clone(),
        is_repr_c: c.is_repr_c,
            is_packed: c.is_packed,
            is_union: c.is_union,
        name: mangled.into(),
        type_params: Box::new([]),
        parent: c.parent.clone(),
        fields,
        properties,
        methods,
        static_methods,
        // Generic class + static fields shouldn't reach here (the
        // type checker forbids static members on generic classes for
        // now), but pass them through verbatim for completeness.
        static_fields: c.static_fields.clone(),
        span: c.span,
    }
}

fn specialize_fn(f: &FnDecl, params: &[Symbol], args: &[Type]) -> FnDecl {
    FnDecl {
        name: f.name.clone(),
        type_params: Box::new([]),
        params: f
            .params
            .iter()
            .map(|p| Param {
                name: p.name.clone(),
                ty: subst_type(&p.ty, params, args),
                span: p.span,
                default: p.default.clone(),
            })
            .collect(),
        ret: f.ret.as_ref().map(|t| subst_type(t, params, args)),
        body: subst_block(&f.body, params, args),
        attrs: f.attrs.clone(),
        span: f.span,
        is_override: f.is_override,
    }
}

fn subst_block(b: &Block, params: &[Symbol], args: &[Type]) -> Block {
    Block {
        stmts: b.stmts.iter().map(|s| subst_stmt(s, params, args)).collect(),
        tail: b.tail.as_ref().map(|e| Box::new(subst_expr(e, params, args))),
    }
}

fn subst_stmt(s: &Stmt, params: &[Symbol], args: &[Type]) -> Stmt {
    let kind = match &s.kind {
        StmtKind::Let { name, ty, value } => StmtKind::Let {
            name: name.clone(),
            ty: ty.as_ref().map(|t| subst_type(t, params, args)),
            value: subst_expr(value, params, args),
        },
        StmtKind::LetTuple { elems, value } => StmtKind::LetTuple {
            elems: elems.clone(),
            value: subst_expr(value, params, args),
        },
        StmtKind::LetStruct { class, fields, value } => StmtKind::LetStruct {
            class: class.clone(),
            fields: fields.clone(),
            value: subst_expr(value, params, args),
        },
        StmtKind::Expr(e) => StmtKind::Expr(subst_expr(e, params, args)),
    };
    Stmt {
        kind,
        span: s.span,
    }
}

fn subst_expr(e: &Expr, params: &[Symbol], args: &[Type]) -> Expr {
    let kind = match &e.kind {
        ExprKind::Int(n) => ExprKind::Int(*n),
        ExprKind::Float(f) => ExprKind::Float(*f),
        ExprKind::Bool(b) => ExprKind::Bool(*b),
        ExprKind::Str(s) => ExprKind::Str(s.clone()),
        ExprKind::Var(n) => ExprKind::Var(n.clone()),
        ExprKind::This => ExprKind::This,
        ExprKind::None => ExprKind::None,
        ExprKind::Some(inner) => ExprKind::Some(Box::new(subst_expr(inner, params, args))),
        ExprKind::Break(opt) => ExprKind::Break(opt.as_ref().map(|e| Box::new(subst_expr(e, params, args)))),
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
        ExprKind::TypeTest { expr, ty } => ExprKind::TypeTest {
            expr: Box::new(subst_expr(expr, params, args)),
            ty: subst_type(ty, params, args),
        },
        ExprKind::TypeDowncast { expr, ty } => ExprKind::TypeDowncast {
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
                    default: p.default.clone(),
                })
                .collect(),
            ret: ret.as_ref().map(|t| subst_type(t, params, args)),
            body: subst_block(body, params, args),
        },
        ExprKind::Call { callee, args: a } => ExprKind::Call {
            callee: callee.clone(),
            args: a.iter().map(|x| subst_expr(x, params, args)).collect(),
        },
        ExprKind::SuperCall { method, args: a } => ExprKind::SuperCall {
            method: method.clone(),
            args: a.iter().map(|x| subst_expr(x, params, args)).collect(),
        },
        ExprKind::Closure { fn_name, captures } => ExprKind::Closure {
            fn_name: fn_name.clone(),
            captures: captures.clone(),
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
            init_method,
        } => ExprKind::New {
            class: class.clone(),
            type_args: type_args.iter().map(|t| subst_type(t, params, args)).collect(),
            args: a.iter().map(|x| subst_expr(x, params, args)).collect(),
            init_method: init_method.clone(),
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
        ExprKind::Range { start, end, inclusive } => ExprKind::Range {
            start: start.as_ref().map(|s| Box::new(subst_expr(s, params, args))),
            end: end.as_ref().map(|e| Box::new(subst_expr(e, params, args))),
            inclusive: *inclusive,
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
        ExprKind::Tuple(items) => ExprKind::Tuple(
            items.iter().map(|e| subst_expr(e, params, args)).collect(),
        ),
        ExprKind::StructLit { class, fields } => ExprKind::StructLit {
            class: class.clone(),
            fields: fields
                .iter()
                .map(|(n, e)| (n.clone(), subst_expr(e, params, args)))
                .collect(),
        },
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

fn subst_type(t: &Type, params: &[Symbol], args: &[Type]) -> Type {
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
        Type::Generic(g) => {
            let new_args: Vec<Type> =
                g.args.iter().map(|a| subst_type(a, params, args)).collect();
            // Once concrete (no TypeVar left), collapse to Object(mangled).
            let gen_ty = Type::generic(g.base.clone(), new_args.clone());
            if !contains_type_var(&gen_ty) {
                Type::Object(
                    InstKey {
                        class: g.base.clone(),
                        args: new_args.into(),
                    }
                    .mangled(),
                )
            } else {
                gen_ty
            }
        }
        Type::Array { elem, fixed } => Type::Array {
            elem: Box::new(subst_type(elem, params, args)),
            fixed: *fixed,
        },
        Type::Optional(inner) => Type::Optional(Box::new(subst_type(inner, params, args))),
        Type::Weak(inner) => Type::Weak(Box::new(subst_type(inner, params, args))),
        Type::Fn(ft) => Type::func(
            ft.params.iter().map(|p| subst_type(p, params, args)).collect(),
            subst_type(&ft.ret, params, args),
        ),
        _ => t.clone(),
    }
}

// ─── rewrite pass: collapse Generic refs in non-generic items ────────

fn rewrite_item(item: &Item) -> Item {
    match item {
        Item::Class(c) => Item::Class(ClassDecl {
            extern_lib: c.extern_lib.clone(),
            is_repr_c: c.is_repr_c,
            is_packed: c.is_packed,
            is_union: c.is_union,
            name: c.name.clone(),
            parent: c.parent.clone(),
            type_params: c.type_params.clone(),
            fields: c
                .fields
                .iter()
                .map(|f| FieldDecl {
                    name: f.name.clone(),
                    ty: rewrite_type(&f.ty),
                    span: f.span, bits: f.bits,
                })
                .collect(),
            methods: c.methods.iter().map(rewrite_fn).collect(),
            static_methods: c.static_methods.iter().map(rewrite_fn).collect(),
            static_fields: c.static_fields.clone(),
            properties: c
                .properties
                .iter()
                .map(|p| ilang_ast::PropertyDecl {
                    name: p.name.clone(),
                    ty: rewrite_type(&p.ty),
                    getter: p.getter.as_ref().map(rewrite_fn),
                    setter: p.setter.as_ref().map(rewrite_fn),
                    span: p.span,
                })
                .collect(),
            span: c.span,
        }),
        Item::Fn(f) => Item::Fn(rewrite_fn(f)),
        Item::Enum(e) => Item::Enum(e.clone()),
        Item::Use(u) => Item::Use(u.clone()),
        Item::Const(c) => Item::Const(c.clone()),
        Item::ExternC(b) => Item::ExternC(b.clone()),
        Item::ExternStatic(s) => Item::ExternStatic(s.clone()),
    }
}

fn rewrite_fn(f: &FnDecl) -> FnDecl {
    FnDecl {
        name: f.name.clone(),
        type_params: f.type_params.clone(),
        params: f
            .params
            .iter()
            .map(|p| Param {
                name: p.name.clone(),
                ty: rewrite_type(&p.ty),
                span: p.span,
                default: p.default.clone(),
            })
            .collect(),
        ret: f.ret.as_ref().map(rewrite_type),
        body: rewrite_block(&f.body),
        attrs: f.attrs.clone(),
        span: f.span,
        is_override: f.is_override,
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
        StmtKind::LetTuple { elems, value } => StmtKind::LetTuple {
            elems: elems.clone(),
            value: rewrite_expr(value),
        },
        StmtKind::LetStruct { class, fields, value } => StmtKind::LetStruct {
            class: class.clone(),
            fields: fields.clone(),
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
        ExprKind::TypeTest { expr, ty } => ExprKind::TypeTest {
            expr: Box::new(rewrite_expr(expr)),
            ty: rewrite_type(ty),
        },
        ExprKind::TypeDowncast { expr, ty } => ExprKind::TypeDowncast {
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
                    default: p.default.clone(),
                })
                .collect(),
            ret: ret.as_ref().map(rewrite_type),
            body: rewrite_block(body),
        },
        ExprKind::New { class, type_args, args, init_method } => {
            // Concrete generic instantiation → call into the
            // monomorphized class by its mangled name. Built-in generic
            // classes (Map) skip mangling — the JIT lowers `new Map<..>()`
            // by recognizing the class name + type_args directly.
            let new_args: Vec<Expr> = args.iter().map(rewrite_expr).collect();
            let new_type_args: Vec<Type> = type_args.iter().map(rewrite_type).collect();
            if type_args.is_empty() || is_builtin_generic_class(class.as_str()) {
                ExprKind::New {
                    class: class.clone(),
                    type_args: new_type_args.into(),
                    args: new_args.into(), init_method: init_method.clone(),
                }
            } else {
                let mangled = InstKey {
                    class: class.clone(),
                    args: new_type_args,
                }
                .mangled();
                ExprKind::New {
                    class: mangled,
                    type_args: Box::new([]),
                    args: new_args.into(), init_method: init_method.clone(),
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
        ExprKind::SuperCall { method, args } => ExprKind::SuperCall {
            method: method.clone(),
            args: args.iter().map(rewrite_expr).collect(),
        },
        ExprKind::Closure { fn_name, captures } => ExprKind::Closure {
            fn_name: fn_name.clone(),
            captures: captures.clone(),
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
        ExprKind::Range { start, end, inclusive } => ExprKind::Range {
            start: start.as_ref().map(|s| Box::new(rewrite_expr(s))),
            end: end.as_ref().map(|e| Box::new(rewrite_expr(e))),
            inclusive: *inclusive,
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
        ExprKind::Tuple(items) => {
            ExprKind::Tuple(items.iter().map(rewrite_expr).collect())
        }
        ExprKind::StructLit { class, fields } => ExprKind::StructLit {
            class: class.clone(),
            fields: fields.iter().map(|(n, e)| (n.clone(), rewrite_expr(e))).collect(),
        },
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
        ExprKind::Break(opt) => ExprKind::Break(opt.as_ref().map(|e| Box::new(rewrite_expr(e)))),
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
    static GENERIC_ENUM_NAMES: std::cell::RefCell<HashSet<Symbol>> =
        std::cell::RefCell::new(HashSet::new());
}

fn is_generic_enum(name: &str) -> bool {
    GENERIC_ENUM_NAMES.with(|set| set.borrow().contains(&Symbol::intern(name)))
}

/// Built-in generic classes whose `Type::Generic { base, args }` should
/// flow through to the JIT verbatim (NOT mangled into a synthetic
/// `Type::Object` like user generic classes). The JIT recognizes these
/// names and produces dedicated `JitTy` variants for them.
fn is_builtin_generic_class(name: &str) -> bool {
    name == "Map"
}

/// Collapse `Type::Generic { Box, [i64] }` to `Type::Object("Box<i64>")`
/// so the JIT pipeline (which only knows `Object`) routes to the
/// monomorphized class. Recurses through Array/Optional/Weak.
fn rewrite_type(t: &Type) -> Type {
    match t {
        Type::Generic(g) => {
            let new_args: Vec<Type> = g.args.iter().map(rewrite_type).collect();
            // Generic enums aren't monomorphized — leave them as
            // `Type::Generic` so the JIT's `from_ast` errors with a
            // clear UnsupportedType. Built-in generic classes (Map)
            // are also kept as Generic — the JIT handles them
            // specially. User generic classes get the mangled name.
            // Built-in generic classes (Map) are kept as Generic so
            // the JIT handles them specially. Generic enums are also
            // left intact here — the separate `monomorphize_enums` pass
            // converts them to `Type::Object(mangled)` after this pass.
            // User generic classes get the mangled Object name now.
            if is_generic_enum(g.base.as_str()) || is_builtin_generic_class(g.base.as_str()) {
                Type::generic(g.base.clone(), new_args)
            } else {
                Type::Object(
                    InstKey {
                        class: g.base.clone(),
                        args: new_args.into(),
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
        Type::Fn(ft) => Type::func(
            ft.params.iter().map(rewrite_type).collect(),
            rewrite_type(&ft.ret),
        ),
        _ => t.clone(),
    }
}

// ─── generic-fn monomorphization ─────────────────────────────────────
//
// Generic fns don't carry explicit `<T>` syntax at call sites — the
// type checker infers them from the arg types and stashes the result
// in `call_type_args` keyed by the call expression's span. This pass
// consumes that side table to:
//
// 1. Synthesize one concrete `FnDecl` per (generic_fn, concrete args)
//    pair actually used in the program.
// 2. Rewrite each Call's callee from the generic name to the mangled
//    concrete name.
// 3. Drop the generic templates from the output.
//
// **Limitation**: only call sites whose recorded type args are fully
// concrete (no `TypeVar`) get rewritten. A generic fn called from
// inside another generic context (e.g. a still-generic class method
// that survived class monomorphization for some reason) leaves a
// dangling reference; the JIT then errors with "unknown function".
// In practice class monomorphization runs first so all class-method
// bodies are concrete by the time we get here.

fn mangle_fn_name(name: &str, args: &[Type]) -> Symbol {
    let mut s = name.to_string();
    s.push('<');
    for (i, a) in args.iter().enumerate() {
        if i > 0 {
            s.push_str(", ");
        }
        s.push_str(&a.to_string());
    }
    s.push('>');
    Symbol::intern(&s)
}

pub(crate) fn monomorphize_fns(
    prog: &Program,
    call_type_args: &HashMap<Span, (Symbol, Vec<Type>)>,
) -> Program {
    // Catalog generic fns. After class monomorphization every fn is a
    // top-level `Item::Fn` (methods live inside their class's items),
    // so we don't need to look at class methods here.
    let generic_fns: HashMap<Symbol, FnDecl> = prog
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Fn(f) if !f.type_params.is_empty() => Some((f.name.clone(), f.clone())),
            _ => None,
        })
        .collect();
    if generic_fns.is_empty() {
        return prog.clone();
    }

    // Worklist of concrete instantiations to synthesize. Dedup by
    // mangled name; keep the (name, args) pair around for substitution.
    let mut requested: HashSet<Symbol> = HashSet::new();
    let mut worklist: Vec<(Symbol, Vec<Type>)> = Vec::new();

    let enqueue = |name: &str,
                   args: &[Type],
                   wl: &mut Vec<(Symbol, Vec<Type>)>,
                   req: &mut HashSet<Symbol>| {
        if !generic_fns.contains_key(&Symbol::intern(name)) {
            return;
        }
        if args.iter().any(contains_type_var) {
            return; // call site sits in another generic context — skip
        }
        let key = mangle_fn_name(name, args);
        if req.insert(key) {
            wl.push((name.into(), args.to_vec()));
        }
    };

    // Seed: scan every call in the program. Outer substitution is
    // empty (we're at the top level / inside non-generic items).
    let empty_params: Vec<Symbol> = Vec::new();
    let empty_args: Vec<Type> = Vec::new();
    for item in &prog.items {
        seed_calls_in_item(
            item,
            call_type_args,
            &empty_params,
            &empty_args,
            &mut |name, args| enqueue(name, args, &mut worklist, &mut requested),
        );
    }
    for s in &prog.stmts {
        seed_calls_in_stmt(
            s,
            call_type_args,
            &empty_params,
            &empty_args,
            &mut |name, args| enqueue(name, args, &mut worklist, &mut requested),
        );
    }
    if let Some(t) = &prog.tail {
        seed_calls_in_expr(
            t,
            call_type_args,
            &empty_params,
            &empty_args,
            &mut |name, args| enqueue(name, args, &mut worklist, &mut requested),
        );
    }

    // Drain the worklist. Each specialization may discover further
    // generic-fn calls in its (substituted) body; those go back on.
    let mut synthesized: HashMap<Symbol, FnDecl> = HashMap::new();
    while let Some((name, args)) = worklist.pop() {
        let mangled = mangle_fn_name(name.as_str(), &args);
        if synthesized.contains_key(&mangled) {
            continue;
        }
        let template = generic_fns.get(&name).unwrap().clone();
        let outer_params = template.type_params.clone();
        let outer_args = args.clone();

        // 1. Substitute T → concrete throughout sig + body.
        let mut new_fn = specialize_fn(&template, &outer_params, &outer_args);
        new_fn.name = mangled.clone();
        new_fn.type_params = Box::new([]);

        // 2. Discover & enqueue further generic-fn calls inside the
        //    substituted body (substituting outer T → concrete in the
        //    recorded args first).
        seed_calls_in_block(
            &new_fn.body,
            call_type_args,
            &outer_params,
            &outer_args,
            &mut |inner_name, inner_args| {
                enqueue(inner_name, inner_args, &mut worklist, &mut requested);
            },
        );

        // 3. Rewrite generic-fn calls in the substituted body to use
        //    their mangled names.
        new_fn.body = rewrite_calls_in_block(
            &new_fn.body,
            call_type_args,
            &outer_params,
            &outer_args,
            &generic_fns,
        );

        synthesized.insert(mangled, new_fn);
    }

    // Build output: drop generic-fn templates, rewrite calls in
    // everything else, append synthesized concrete fns.
    let mut out_items: Vec<Item> = Vec::new();
    for item in &prog.items {
        match item {
            Item::Fn(f) if !f.type_params.is_empty() => { /* drop */ }
            other => out_items.push(rewrite_calls_in_item(
                other,
                call_type_args,
                &empty_params,
                &empty_args,
                &generic_fns,
            )),
        }
    }
    for (_, f) in synthesized {
        out_items.push(Item::Fn(f));
    }
    let stmts: Vec<Stmt> = prog
        .stmts
        .iter()
        .map(|s| {
            rewrite_calls_in_stmt(s, call_type_args, &empty_params, &empty_args, &generic_fns)
        })
        .collect();
    let tail = prog.tail.as_ref().map(|e| {
        rewrite_calls_in_expr(e, call_type_args, &empty_params, &empty_args, &generic_fns)
    });
    Program {
        items: out_items,
        stmts,
        tail,
    }
}

// ─── seed helpers: walk the AST and visit every Call ─────────────────

fn seed_calls_in_item(
    item: &Item,
    table: &HashMap<Span, (Symbol, Vec<Type>)>,
    outer_params: &[Symbol],
    outer_args: &[Type],
    visit: &mut dyn FnMut(&str, &[Type]),
) {
    match item {
        Item::Fn(f) => seed_calls_in_block(&f.body, table, outer_params, outer_args, visit),
        Item::Class(c) => {
            for m in &c.methods {
                seed_calls_in_block(&m.body, table, outer_params, outer_args, visit);
            }
        }
        Item::Enum(_) | Item::Use(_) | Item::Const(_) | Item::ExternStatic(_) | Item::ExternC(_) => {}
    }
}

fn seed_calls_in_block(
    b: &Block,
    table: &HashMap<Span, (Symbol, Vec<Type>)>,
    outer_params: &[Symbol],
    outer_args: &[Type],
    visit: &mut dyn FnMut(&str, &[Type]),
) {
    for s in &b.stmts {
        seed_calls_in_stmt(s, table, outer_params, outer_args, visit);
    }
    if let Some(t) = &b.tail {
        seed_calls_in_expr(t, table, outer_params, outer_args, visit);
    }
}

fn seed_calls_in_stmt(
    s: &Stmt,
    table: &HashMap<Span, (Symbol, Vec<Type>)>,
    outer_params: &[Symbol],
    outer_args: &[Type],
    visit: &mut dyn FnMut(&str, &[Type]),
) {
    match &s.kind {
        StmtKind::Let { value, .. }
        | StmtKind::LetTuple { value, .. }
        | StmtKind::LetStruct { value, .. } => {
            seed_calls_in_expr(value, table, outer_params, outer_args, visit)
        }
        StmtKind::Expr(e) => seed_calls_in_expr(e, table, outer_params, outer_args, visit),
    }
}

fn seed_calls_in_expr(
    e: &Expr,
    table: &HashMap<Span, (Symbol, Vec<Type>)>,
    outer_params: &[Symbol],
    outer_args: &[Type],
    visit: &mut dyn FnMut(&str, &[Type]),
) {
    if let ExprKind::Call { callee, .. } = &e.kind {
        if let Some((cname, raw_args)) = table.get(&e.span) {
            if cname == callee {
                let concrete: Vec<Type> = raw_args
                    .iter()
                    .map(|t| subst_type(t, outer_params, outer_args))
                    .collect();
                visit(callee.as_str(), &concrete);
            }
        }
    }
    walk_expr_children(e, &mut |c| {
        seed_calls_in_expr(c, table, outer_params, outer_args, visit)
    });
}

// ─── rewrite helpers: same shape, but rename Call.callee ─────────────

fn rewrite_calls_in_item(
    item: &Item,
    table: &HashMap<Span, (Symbol, Vec<Type>)>,
    outer_params: &[Symbol],
    outer_args: &[Type],
    generic_fns: &HashMap<Symbol, FnDecl>,
) -> Item {
    match item {
        Item::Fn(f) => Item::Fn(FnDecl {
            attrs: f.attrs.clone(),
            name: f.name.clone(),
            type_params: f.type_params.clone(),
            params: f.params.clone(),
            ret: f.ret.clone(),
            body: rewrite_calls_in_block(&f.body, table, outer_params, outer_args, generic_fns),
            span: f.span,
        is_override: f.is_override,
        }),
        Item::Class(c) => Item::Class(ClassDecl {
            extern_lib: c.extern_lib.clone(),
            is_repr_c: c.is_repr_c,
            is_packed: c.is_packed,
            is_union: c.is_union,
            name: c.name.clone(),
            parent: c.parent.clone(),
            type_params: c.type_params.clone(),
            fields: c.fields.clone(),
            methods: c
                .methods
                .iter()
                .map(|m| FnDecl {
                    attrs: m.attrs.clone(),
                    name: m.name.clone(),
                    type_params: m.type_params.clone(),
                    params: m.params.clone(),
                    ret: m.ret.clone(),
                    body: rewrite_calls_in_block(
                        &m.body,
                        table,
                        outer_params,
                        outer_args,
                        generic_fns,
                    ),
                    span: m.span,
                is_override: m.is_override,
                })
                .collect(),
            static_methods: c
                .static_methods
                .iter()
                .map(|m| FnDecl {
                    attrs: m.attrs.clone(),
                    name: m.name.clone(),
                    type_params: m.type_params.clone(),
                    params: m.params.clone(),
                    ret: m.ret.clone(),
                    body: rewrite_calls_in_block(
                        &m.body,
                        table,
                        outer_params,
                        outer_args,
                        generic_fns,
                    ),
                    span: m.span,
                is_override: m.is_override,
                })
                .collect(),
            static_fields: c.static_fields.clone(),
            properties: c
                .properties
                .iter()
                .map(|p| ilang_ast::PropertyDecl {
                    name: p.name.clone(),
                    ty: p.ty.clone(),
                    getter: p.getter.as_ref().map(|g| FnDecl {
                        attrs: g.attrs.clone(),
                        name: g.name.clone(),
                        type_params: g.type_params.clone(),
                        params: g.params.clone(),
                        ret: g.ret.clone(),
                        body: rewrite_calls_in_block(
                            &g.body,
                            table,
                            outer_params,
                            outer_args,
                            generic_fns,
                        ),
                        span: g.span,
                    is_override: g.is_override,
                    }),
                    setter: p.setter.as_ref().map(|s| FnDecl {
                        attrs: s.attrs.clone(),
                        name: s.name.clone(),
                        type_params: s.type_params.clone(),
                        params: s.params.clone(),
                        ret: s.ret.clone(),
                        body: rewrite_calls_in_block(
                            &s.body,
                            table,
                            outer_params,
                            outer_args,
                            generic_fns,
                        ),
                        span: s.span,
                    is_override: s.is_override,
                    }),
                    span: p.span,
                })
                .collect(),
            span: c.span,
        }),
        Item::Enum(e) => Item::Enum(e.clone()),
        Item::Use(u) => Item::Use(u.clone()),
        Item::Const(c) => Item::Const(c.clone()),
        Item::ExternC(b) => Item::ExternC(b.clone()),
        Item::ExternStatic(s) => Item::ExternStatic(s.clone()),
    }
}

fn rewrite_calls_in_block(
    b: &Block,
    table: &HashMap<Span, (Symbol, Vec<Type>)>,
    outer_params: &[Symbol],
    outer_args: &[Type],
    generic_fns: &HashMap<Symbol, FnDecl>,
) -> Block {
    Block {
        stmts: b
            .stmts
            .iter()
            .map(|s| rewrite_calls_in_stmt(s, table, outer_params, outer_args, generic_fns))
            .collect(),
        tail: b.tail.as_ref().map(|e| {
            Box::new(rewrite_calls_in_expr(
                e,
                table,
                outer_params,
                outer_args,
                generic_fns,
            ))
        }),
    }
}

fn rewrite_calls_in_stmt(
    s: &Stmt,
    table: &HashMap<Span, (Symbol, Vec<Type>)>,
    outer_params: &[Symbol],
    outer_args: &[Type],
    generic_fns: &HashMap<Symbol, FnDecl>,
) -> Stmt {
    let kind = match &s.kind {
        StmtKind::Let { name, ty, value } => StmtKind::Let {
            name: name.clone(),
            ty: ty.clone(),
            value: rewrite_calls_in_expr(value, table, outer_params, outer_args, generic_fns),
        },
        StmtKind::LetTuple { elems, value } => StmtKind::LetTuple {
            elems: elems.clone(),
            value: rewrite_calls_in_expr(value, table, outer_params, outer_args, generic_fns),
        },
        StmtKind::LetStruct { class, fields, value } => StmtKind::LetStruct {
            class: class.clone(),
            fields: fields.clone(),
            value: rewrite_calls_in_expr(value, table, outer_params, outer_args, generic_fns),
        },
        StmtKind::Expr(e) => StmtKind::Expr(rewrite_calls_in_expr(
            e,
            table,
            outer_params,
            outer_args,
            generic_fns,
        )),
    };
    Stmt { kind, span: s.span }
}

fn rewrite_calls_in_expr(
    e: &Expr,
    table: &HashMap<Span, (Symbol, Vec<Type>)>,
    outer_params: &[Symbol],
    outer_args: &[Type],
    generic_fns: &HashMap<Symbol, FnDecl>,
) -> Expr {
    let kind = match &e.kind {
        ExprKind::Call { callee, args } => {
            // Recurse into args first.
            let new_args: Vec<Expr> = args
                .iter()
                .map(|a| rewrite_calls_in_expr(a, table, outer_params, outer_args, generic_fns))
                .collect();
            // Decide the callee's final name.
            let new_callee = if generic_fns.contains_key(callee) {
                if let Some((cname, raw_args)) = table.get(&e.span) {
                    if cname == callee {
                        let concrete: Vec<Type> = raw_args
                            .iter()
                            .map(|t| subst_type(t, outer_params, outer_args))
                            .collect();
                        if !concrete.iter().any(contains_type_var) {
                            mangle_fn_name(callee.as_str(), &concrete)
                        } else {
                            callee.clone() // dangling — JIT will error
                        }
                    } else {
                        callee.clone()
                    }
                } else {
                    callee.clone()
                }
            } else {
                callee.clone()
            };
            ExprKind::Call {
                callee: new_callee,
                args: new_args.into(),
            }
        }
        _ => {
            // Recurse through other expression shapes structurally.
            map_expr_children(e, &mut |c| {
                rewrite_calls_in_expr(c, table, outer_params, outer_args, generic_fns)
            })
        }
    };
    Expr { kind, span: e.span }
}

// ─── generic AST walk helpers ────────────────────────────────────────

/// Visit every direct child Expr of `e`. Used by seed_calls_in_expr to
/// avoid duplicating the whole match by hand.
fn walk_expr_children(e: &Expr, f: &mut dyn FnMut(&Expr)) {
    match &e.kind {
        ExprKind::Int(_)
        | ExprKind::Float(_)
        | ExprKind::Bool(_)
        | ExprKind::Str(_)
        | ExprKind::Var(_)
        | ExprKind::This
        | ExprKind::None
        | ExprKind::Continue => {}
        ExprKind::Break(opt) => {
            if let Some(x) = opt {
                f(x);
            }
        }
        ExprKind::Some(x) | ExprKind::Unary { expr: x, .. } => f(x),
        ExprKind::Binary { lhs, rhs, .. } | ExprKind::Logical { lhs, rhs, .. } => {
            f(lhs);
            f(rhs);
        }
        ExprKind::Cast { expr, .. }
        | ExprKind::TypeTest { expr, .. }
        | ExprKind::TypeDowncast { expr, .. } => f(expr),
        ExprKind::FnExpr { .. } => {
            // Anonymous fns are hoisted out before this pass; nothing to do.
        }
        ExprKind::Call { args, .. } => {
            for a in args {
                f(a);
            }
        }
        ExprKind::SuperCall { args, .. } => {
            for a in args {
                f(a);
            }
        }
        ExprKind::Closure { .. } => {}
        ExprKind::Field { obj, .. } => f(obj),
        ExprKind::MethodCall { obj, args, .. } => {
            f(obj);
            for a in args {
                f(a);
            }
        }
        ExprKind::New { args, .. } => {
            for a in args {
                f(a);
            }
        }
        ExprKind::Block(b) => walk_block_children(b, f),
        ExprKind::If {
            cond,
            then_branch,
            else_branch,
        } => {
            f(cond);
            walk_block_children(then_branch, f);
            if let Some(e) = else_branch {
                f(e);
            }
        }
        ExprKind::IfLet {
            expr,
            then_branch,
            else_branch,
            ..
        } => {
            f(expr);
            walk_block_children(then_branch, f);
            if let Some(e) = else_branch {
                f(e);
            }
        }
        ExprKind::While { cond, body } => {
            f(cond);
            walk_block_children(body, f);
        }
        ExprKind::Loop { body } => walk_block_children(body, f),
        ExprKind::ForIn { iter, body, .. } => {
            f(iter);
            walk_block_children(body, f);
        }
        ExprKind::Range { start, end, .. } => {
            if let Some(s) = start {
                f(s);
            }
            if let Some(e) = end {
                f(e);
            }
        }
        ExprKind::Return(opt) => {
            if let Some(e) = opt {
                f(e);
            }
        }
        ExprKind::Assign { value, .. } => f(value),
        ExprKind::AssignField { value, .. } => f(value),
        ExprKind::AssignIndex { value, .. } => f(value),
        ExprKind::Array(items) => {
            for i in items {
                f(i);
            }
        }
        ExprKind::Tuple(items) => {
            for i in items {
                f(i);
            }
        }
        ExprKind::StructLit { fields, .. } => {
            for (_, e) in fields {
                f(e);
            }
        }
        ExprKind::MapLit(entries) => {
            for (k, v) in entries {
                f(k);
                f(v);
            }
        }
        ExprKind::Index { obj, index } => {
            f(obj);
            f(index);
        }
        ExprKind::EnumCtor { args, .. } => match args {
            ilang_ast::CtorArgs::Unit => {}
            ilang_ast::CtorArgs::Tuple(es) => {
                for x in es {
                    f(x);
                }
            }
            ilang_ast::CtorArgs::Struct(fs) => {
                for (_, x) in fs {
                    f(x);
                }
            }
        },
        ExprKind::Match { scrutinee, arms } => {
            f(scrutinee);
            for arm in arms {
                f(&arm.body);
            }
        }
    }
}

fn walk_block_children(b: &Block, f: &mut dyn FnMut(&Expr)) {
    for s in &b.stmts {
        match &s.kind {
            StmtKind::Let { value, .. }
            | StmtKind::LetTuple { value, .. }
            | StmtKind::LetStruct { value, .. } => f(value),
            StmtKind::Expr(e) => f(e),
        }
    }
    if let Some(t) = &b.tail {
        f(t);
    }
}

/// Map every direct child of `e` through `f` and rebuild the Expr's
/// kind. Used by rewrite_calls_in_expr's catch-all arm so we don't
/// have to enumerate every variant by hand.
fn map_expr_children(e: &Expr, f: &mut dyn FnMut(&Expr) -> Expr) -> ExprKind {
    match &e.kind {
        ExprKind::Int(n) => ExprKind::Int(*n),
        ExprKind::Float(x) => ExprKind::Float(*x),
        ExprKind::Bool(b) => ExprKind::Bool(*b),
        ExprKind::Str(s) => ExprKind::Str(s.clone()),
        ExprKind::Var(n) => ExprKind::Var(n.clone()),
        ExprKind::This => ExprKind::This,
        ExprKind::None => ExprKind::None,
        ExprKind::Break(opt) => ExprKind::Break(opt.as_ref().map(|e| Box::new(f(e)))),
        ExprKind::Continue => ExprKind::Continue,
        ExprKind::Some(x) => ExprKind::Some(Box::new(f(x))),
        ExprKind::Unary { op, expr } => ExprKind::Unary {
            op: *op,
            expr: Box::new(f(expr)),
        },
        ExprKind::Binary { op, lhs, rhs } => ExprKind::Binary {
            op: *op,
            lhs: Box::new(f(lhs)),
            rhs: Box::new(f(rhs)),
        },
        ExprKind::Logical { op, lhs, rhs } => ExprKind::Logical {
            op: *op,
            lhs: Box::new(f(lhs)),
            rhs: Box::new(f(rhs)),
        },
        ExprKind::Cast { expr, ty } => ExprKind::Cast {
            expr: Box::new(f(expr)),
            ty: ty.clone(),
        },
        ExprKind::TypeTest { expr, ty } => ExprKind::TypeTest {
            expr: Box::new(f(expr)),
            ty: ty.clone(),
        },
        ExprKind::TypeDowncast { expr, ty } => ExprKind::TypeDowncast {
            expr: Box::new(f(expr)),
            ty: ty.clone(),
        },
        ExprKind::FnExpr { params, ret, body } => ExprKind::FnExpr {
            params: params.clone(),
            ret: ret.clone(),
            body: map_block_children(body, f),
        },
        ExprKind::Call { callee, args } => ExprKind::Call {
            callee: callee.clone(),
            args: args.iter().map(|a| f(a)).collect(),
        },
        ExprKind::SuperCall { method, args } => ExprKind::SuperCall {
            method: method.clone(),
            args: args.iter().map(|a| f(a)).collect(),
        },
        ExprKind::Closure { fn_name, captures } => ExprKind::Closure {
            fn_name: fn_name.clone(),
            captures: captures.clone(),
        },
        ExprKind::Field { obj, name } => ExprKind::Field {
            obj: Box::new(f(obj)),
            name: name.clone(),
        },
        ExprKind::MethodCall { obj, method, args } => ExprKind::MethodCall {
            obj: Box::new(f(obj)),
            method: method.clone(),
            args: args.iter().map(|a| f(a)).collect(),
        },
        ExprKind::New { class, type_args, args, init_method } => ExprKind::New {
            class: class.clone(),
            type_args: type_args.clone(),
            args: args.iter().map(|a| f(a)).collect(), init_method: init_method.clone(),
        },
        ExprKind::Block(b) => ExprKind::Block(map_block_children(b, f)),
        ExprKind::If {
            cond,
            then_branch,
            else_branch,
        } => ExprKind::If {
            cond: Box::new(f(cond)),
            then_branch: map_block_children(then_branch, f),
            else_branch: else_branch.as_ref().map(|e| Box::new(f(e))),
        },
        ExprKind::IfLet {
            name,
            expr,
            then_branch,
            else_branch,
        } => ExprKind::IfLet {
            name: name.clone(),
            expr: Box::new(f(expr)),
            then_branch: map_block_children(then_branch, f),
            else_branch: else_branch.as_ref().map(|e| Box::new(f(e))),
        },
        ExprKind::While { cond, body } => ExprKind::While {
            cond: Box::new(f(cond)),
            body: map_block_children(body, f),
        },
        ExprKind::Loop { body } => ExprKind::Loop {
            body: map_block_children(body, f),
        },
        ExprKind::ForIn { var, iter, body } => ExprKind::ForIn {
            var: var.clone(),
            iter: Box::new(f(iter)),
            body: map_block_children(body, f),
        },
        ExprKind::Range { start, end, inclusive } => ExprKind::Range {
            start: start.as_ref().map(|s| Box::new(f(s))),
            end: end.as_ref().map(|e| Box::new(f(e))),
            inclusive: *inclusive,
        },
        ExprKind::Return(opt) => ExprKind::Return(opt.as_ref().map(|e| Box::new(f(e)))),
        ExprKind::Assign { target, value } => ExprKind::Assign {
            target: target.clone(),
            value: Box::new(f(value)),
        },
        ExprKind::AssignField { obj, field, value } => ExprKind::AssignField {
            obj: obj.clone(),
            field: field.clone(),
            value: Box::new(f(value)),
        },
        ExprKind::AssignIndex { obj, index, value } => ExprKind::AssignIndex {
            obj: obj.clone(),
            index: index.clone(),
            value: Box::new(f(value)),
        },
        ExprKind::Array(items) => ExprKind::Array(items.iter().map(|e| f(e)).collect()),
        ExprKind::Tuple(items) => ExprKind::Tuple(items.iter().map(|e| f(e)).collect()),
        ExprKind::StructLit { class, fields } => ExprKind::StructLit {
            class: class.clone(),
            fields: fields.iter().map(|(n, e)| (n.clone(), f(e))).collect(),
        },
        ExprKind::MapLit(entries) => ExprKind::MapLit(
            entries.iter().map(|(k, v)| (f(k), f(v))).collect(),
        ),
        ExprKind::Index { obj, index } => ExprKind::Index {
            obj: Box::new(f(obj)),
            index: Box::new(f(index)),
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
                ilang_ast::CtorArgs::Tuple(es) => {
                    ilang_ast::CtorArgs::Tuple(es.iter().map(|e| f(e)).collect())
                }
                ilang_ast::CtorArgs::Struct(fs) => ilang_ast::CtorArgs::Struct(
                    fs.iter().map(|(n, e)| (n.clone(), f(e))).collect(),
                ),
            },
        },
        ExprKind::Match { scrutinee, arms } => ExprKind::Match {
            scrutinee: Box::new(f(scrutinee)),
            arms: arms
                .iter()
                .map(|arm| ilang_ast::MatchArm {
                    pattern: arm.pattern.clone(),
                    body: f(&arm.body),
                    span: arm.span,
                })
                .collect(),
        },
    }
}

fn map_block_children(b: &Block, f: &mut dyn FnMut(&Expr) -> Expr) -> Block {
    Block {
        stmts: b
            .stmts
            .iter()
            .map(|s| {
                let kind = match &s.kind {
                    StmtKind::Let { name, ty, value } => StmtKind::Let {
                        name: name.clone(),
                        ty: ty.clone(),
                        value: f(value),
                    },
                    StmtKind::LetTuple { elems, value } => StmtKind::LetTuple {
                        elems: elems.clone(),
                        value: f(value),
                    },
                    StmtKind::LetStruct { class, fields, value } => StmtKind::LetStruct {
                        class: class.clone(),
                        fields: fields.clone(),
                        value: f(value),
                    },
                    StmtKind::Expr(e) => StmtKind::Expr(f(e)),
                };
                Stmt { kind, span: s.span }
            })
            .collect(),
        tail: b.tail.as_ref().map(|e| Box::new(f(e))),
    }
}

// ─── generic-enum monomorphization ───────────────────────────────────
//
// Runs after `monomorphize` (which handles classes). Generic enums
// require a per-(name, args) concrete `EnumDecl` so the JIT can pin
// down each variant's payload size. The class pass deliberately
// leaves `Type::Generic { Enum, [..] }` alone; this pass:
//
// 1. Catalogs generic enums (user-defined + the built-in `Result`).
// 2. Seeds a worklist from every concrete instantiation it sees —
//    both `Type::Generic` refs in field/param/return slots AND
//    `EnumCtor` calls (looked up via the type-checker's side table).
// 3. Synthesizes concrete `EnumDecl`s by substituting each variant's
//    payload types.
// 4. Rewrites the rest of the program: `Type::Generic { Enum, ... }`
//    → `Type::Object(mangled)`, `EnumCtor.enum_name` → mangled.
// 5. Drops the original generic enum declarations from the output.

fn result_template() -> EnumDecl {
    let span = Span::dummy();
    EnumDecl {
        name: "Result".into(),
        type_params: Box::new(["T".into(), "E".into()]),
        repr_ty: None,
        flags: false,
        variants: Box::new([
            Variant {
                name: "ok".into(),
                payload: VariantPayload::Tuple(Box::new([Type::Object("T".into())])),
                discriminant: None,
                span,
            },
            Variant {
                name: "err".into(),
                payload: VariantPayload::Tuple(Box::new([Type::Object("E".into())])),
                discriminant: None,
                span,
            },
        ]),
        span,
    }
}

pub(crate) fn monomorphize_enums(
    prog: &Program,
    enum_ctor_type_args: &HashMap<Span, (Symbol, Vec<Type>)>,
) -> Program {
    // Catalog generic enums. Result is always available (built-in).
    let mut generic_enums: HashMap<Symbol, EnumDecl> = prog
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Enum(e) if !e.type_params.is_empty() => Some((e.name.clone(), e.clone())),
            _ => None,
        })
        .collect();
    generic_enums.entry("Result".into()).or_insert_with(result_template);

    if generic_enums.is_empty() {
        return prog.clone();
    }

    let mut requested: HashSet<Symbol> = HashSet::new();
    let mut worklist: Vec<InstKey> = Vec::new();

    // Closure that classifies an instantiation as either an enum
    // (worklist-bound) or anything else (skipped).
    let enqueue_enum =
        |name: &str, args: &[Type], wl: &mut Vec<InstKey>, req: &mut HashSet<Symbol>| {
            if !generic_enums.contains_key(&Symbol::intern(name)) {
                return;
            }
            if args.iter().any(contains_type_var) {
                return;
            }
            let key = InstKey { class: name.into(), args: args.to_vec() };
            if req.insert(key.mangled()) {
                wl.push(key);
            }
        };

    // Seed pass A: walk every Type slot for `Type::Generic { Enum, ... }`.
    for item in &prog.items {
        seed_enums_in_item(item, &mut |n, a| enqueue_enum(n, a, &mut worklist, &mut requested));
    }
    for s in &prog.stmts {
        seed_enums_in_stmt(s, &mut |n, a| enqueue_enum(n, a, &mut worklist, &mut requested));
    }
    if let Some(t) = &prog.tail {
        seed_enums_in_expr(t, &mut |n, a| enqueue_enum(n, a, &mut worklist, &mut requested));
    }

    // Seed pass B: walk every EnumCtor with the side-table.
    let empty_params: Vec<Symbol> = Vec::new();
    let empty_args: Vec<Type> = Vec::new();
    for item in &prog.items {
        seed_enum_ctors_in_item(
            item,
            enum_ctor_type_args,
            &empty_params,
            &empty_args,
            &mut |n, a| enqueue_enum(n, a, &mut worklist, &mut requested),
        );
    }
    for s in &prog.stmts {
        seed_enum_ctors_in_stmt(
            s,
            enum_ctor_type_args,
            &empty_params,
            &empty_args,
            &mut |n, a| enqueue_enum(n, a, &mut worklist, &mut requested),
        );
    }
    if let Some(t) = &prog.tail {
        seed_enum_ctors_in_expr(
            t,
            enum_ctor_type_args,
            &empty_params,
            &empty_args,
            &mut |n, a| enqueue_enum(n, a, &mut worklist, &mut requested),
        );
    }

    // Drain worklist. Each specialization's payload types may
    // themselves contain `Type::Generic { Enum, ... }` refs (e.g.
    // `Result<Box<i64>, string>`); those go back on the worklist.
    let mut synthesized: HashMap<Symbol, EnumDecl> = HashMap::new();
    while let Some(key) = worklist.pop() {
        let mangled = key.mangled();
        if synthesized.contains_key(&mangled) {
            continue;
        }
        let template = match generic_enums.get(&key.class) {
            Some(e) => e.clone(),
            None => continue,
        };
        if template.type_params.len() != key.args.len() {
            continue;
        }
        let new_enum = specialize_enum(&template, &key.args, mangled.as_str());
        // Walk new variants for further generic enum refs.
        for v in &new_enum.variants {
            match &v.payload {
                VariantPayload::Unit => {}
                VariantPayload::Tuple(tys) => {
                    for t in tys {
                        seed_enums_in_type(t, &mut |n, a| {
                            enqueue_enum(n, a, &mut worklist, &mut requested)
                        });
                    }
                }
                VariantPayload::Struct(fields) => {
                    for f in fields {
                        seed_enums_in_type(&f.ty, &mut |n, a| {
                            enqueue_enum(n, a, &mut worklist, &mut requested)
                        });
                    }
                }
            }
        }
        synthesized.insert(mangled, new_enum);
    }

    // Build output: drop generic-enum templates, rewrite the rest.
    let mut out_items: Vec<Item> = Vec::new();
    for item in &prog.items {
        match item {
            Item::Enum(e) if !e.type_params.is_empty() => { /* drop */ }
            other => out_items.push(rewrite_enum_refs_in_item(
                other,
                &generic_enums,
                enum_ctor_type_args,
                &empty_params,
                &empty_args,
            )),
        }
    }
    for (_, e) in synthesized {
        out_items.push(Item::Enum(e));
    }
    let stmts: Vec<Stmt> = prog
        .stmts
        .iter()
        .map(|s| {
            rewrite_enum_refs_in_stmt(
                s,
                &generic_enums,
                enum_ctor_type_args,
                &empty_params,
                &empty_args,
            )
        })
        .collect();
    let tail = prog.tail.as_ref().map(|e| {
        rewrite_enum_refs_in_expr(
            e,
            &generic_enums,
            enum_ctor_type_args,
            &empty_params,
            &empty_args,
        )
    });
    Program {
        items: out_items,
        stmts,
        tail,
    }
}

fn specialize_enum(e: &EnumDecl, args: &[Type], mangled: &str) -> EnumDecl {
    let params = e.type_params.clone();
    // Recursively rewrite nested concrete generics (so a payload
    // type `Box<T>` with T=i64 collapses straight to `Box<i64>`
    // mangled instead of leaking back as Type::Generic).
    let args: Vec<Type> = args.iter().map(rewrite_type).collect();
    EnumDecl {
        name: mangled.into(),
        type_params: Box::new([]),
        repr_ty: e.repr_ty.clone(),
        flags: e.flags,
        variants: e
            .variants
            .iter()
            .map(|v| Variant {
                name: v.name.clone(),
                discriminant: v.discriminant,
                payload: match &v.payload {
                    VariantPayload::Unit => VariantPayload::Unit,
                    VariantPayload::Tuple(tys) => VariantPayload::Tuple(
                        tys.iter().map(|t| subst_type(t, &params, &args)).collect(),
                    ),
                    VariantPayload::Struct(fields) => VariantPayload::Struct(
                        fields
                            .iter()
                            .map(|f| FieldDecl {
                                name: f.name.clone(),
                                ty: subst_type(&f.ty, &params, &args),
                                span: f.span, bits: f.bits,
                            })
                            .collect(),
                    ),
                },
                span: v.span,
            })
            .collect(),
        span: e.span,
    }
}

// ─── seed: walk Type slots for Generic{Enum, ...} ───────────────────

fn seed_enums_in_item(item: &Item, visit: &mut dyn FnMut(&str, &[Type])) {
    match item {
        Item::Class(c) => {
            for f in &c.fields {
                seed_enums_in_type(&f.ty, visit);
            }
            for m in &c.methods {
                seed_enums_in_fn(m, visit);
            }
        }
        Item::Fn(f) => seed_enums_in_fn(f, visit),
        Item::Enum(e) => {
            for v in &e.variants {
                match &v.payload {
                    VariantPayload::Unit => {}
                    VariantPayload::Tuple(tys) => {
                        for t in tys {
                            seed_enums_in_type(t, visit);
                        }
                    }
                    VariantPayload::Struct(fields) => {
                        for f in fields {
                            seed_enums_in_type(&f.ty, visit);
                        }
                    }
                }
            }
        }
        Item::Use(_) | Item::Const(_) | Item::ExternStatic(_) | Item::ExternC(_) => {}
    }
}

fn seed_enums_in_fn(f: &FnDecl, visit: &mut dyn FnMut(&str, &[Type])) {
    for p in &f.params {
        seed_enums_in_type(&p.ty, visit);
    }
    if let Some(t) = &f.ret {
        seed_enums_in_type(t, visit);
    }
    seed_enums_in_block(&f.body, visit);
}

fn seed_enums_in_block(b: &Block, visit: &mut dyn FnMut(&str, &[Type])) {
    for s in &b.stmts {
        seed_enums_in_stmt(s, visit);
    }
    if let Some(t) = &b.tail {
        seed_enums_in_expr(t, visit);
    }
}

fn seed_enums_in_stmt(s: &Stmt, visit: &mut dyn FnMut(&str, &[Type])) {
    match &s.kind {
        StmtKind::Let { value, ty, .. } => {
            if let Some(t) = ty {
                seed_enums_in_type(t, visit);
            }
            seed_enums_in_expr(value, visit);
        }
        StmtKind::LetTuple { value, .. }
        | StmtKind::LetStruct { value, .. } => seed_enums_in_expr(value, visit),
        StmtKind::Expr(e) => seed_enums_in_expr(e, visit),
    }
}

fn seed_enums_in_expr(e: &Expr, visit: &mut dyn FnMut(&str, &[Type])) {
    if let ExprKind::Cast { ty, .. } = &e.kind {
        seed_enums_in_type(ty, visit);
    }
    walk_expr_children(e, &mut |c| seed_enums_in_expr(c, visit));
}

fn seed_enums_in_type(t: &Type, visit: &mut dyn FnMut(&str, &[Type])) {
    match t {
        Type::Generic(g) => {
            visit(g.base.as_str(), &g.args);
            for a in &g.args {
                seed_enums_in_type(a, visit);
            }
        }
        Type::Array { elem, .. } => seed_enums_in_type(elem, visit),
        Type::Optional(inner) | Type::Weak(inner) => seed_enums_in_type(inner, visit),
        Type::Fn(ft) => {
            for p in &ft.params {
                seed_enums_in_type(p, visit);
            }
            seed_enums_in_type(&ft.ret, visit);
        }
        _ => {}
    }
}

// ─── seed: walk EnumCtor sites with the type-checker side table ─────

fn seed_enum_ctors_in_item(
    item: &Item,
    table: &HashMap<Span, (Symbol, Vec<Type>)>,
    outer_params: &[Symbol],
    outer_args: &[Type],
    visit: &mut dyn FnMut(&str, &[Type]),
) {
    match item {
        Item::Fn(f) => seed_enum_ctors_in_block(&f.body, table, outer_params, outer_args, visit),
        Item::Class(c) => {
            for m in &c.methods {
                seed_enum_ctors_in_block(&m.body, table, outer_params, outer_args, visit);
            }
        }
        Item::Enum(_) | Item::Use(_) | Item::Const(_) | Item::ExternStatic(_) | Item::ExternC(_) => {}
    }
}

fn seed_enum_ctors_in_block(
    b: &Block,
    table: &HashMap<Span, (Symbol, Vec<Type>)>,
    outer_params: &[Symbol],
    outer_args: &[Type],
    visit: &mut dyn FnMut(&str, &[Type]),
) {
    for s in &b.stmts {
        seed_enum_ctors_in_stmt(s, table, outer_params, outer_args, visit);
    }
    if let Some(t) = &b.tail {
        seed_enum_ctors_in_expr(t, table, outer_params, outer_args, visit);
    }
}

fn seed_enum_ctors_in_stmt(
    s: &Stmt,
    table: &HashMap<Span, (Symbol, Vec<Type>)>,
    outer_params: &[Symbol],
    outer_args: &[Type],
    visit: &mut dyn FnMut(&str, &[Type]),
) {
    match &s.kind {
        StmtKind::Let { value, .. }
        | StmtKind::LetTuple { value, .. }
        | StmtKind::LetStruct { value, .. } => {
            seed_enum_ctors_in_expr(value, table, outer_params, outer_args, visit)
        }
        StmtKind::Expr(e) => seed_enum_ctors_in_expr(e, table, outer_params, outer_args, visit),
    }
}

fn seed_enum_ctors_in_expr(
    e: &Expr,
    table: &HashMap<Span, (Symbol, Vec<Type>)>,
    outer_params: &[Symbol],
    outer_args: &[Type],
    visit: &mut dyn FnMut(&str, &[Type]),
) {
    if let ExprKind::EnumCtor { enum_name, .. } = &e.kind {
        if let Some((name, raw_args)) = table.get(&e.span) {
            if name == enum_name {
                let concrete: Vec<Type> = raw_args
                    .iter()
                    .map(|t| subst_type(t, outer_params, outer_args))
                    .collect();
                visit(enum_name.as_str(), &concrete);
            }
        }
    }
    walk_expr_children(e, &mut |c| {
        seed_enum_ctors_in_expr(c, table, outer_params, outer_args, visit)
    });
}

// ─── rewrite: mangle generic-enum refs in types + EnumCtor.enum_name ─

fn rewrite_enum_refs_in_item(
    item: &Item,
    generic_enums: &HashMap<Symbol, EnumDecl>,
    table: &HashMap<Span, (Symbol, Vec<Type>)>,
    outer_params: &[Symbol],
    outer_args: &[Type],
) -> Item {
    match item {
        Item::Fn(f) => Item::Fn(FnDecl {
            attrs: f.attrs.clone(),
            name: f.name.clone(),
            type_params: f.type_params.clone(),
            params: f
                .params
                .iter()
                .map(|p| Param {
                    name: p.name.clone(),
                    ty: rewrite_enum_refs_in_type(&p.ty, generic_enums),
                    span: p.span,
                    default: p.default.clone(),
                })
                .collect(),
            ret: f.ret.as_ref().map(|t| rewrite_enum_refs_in_type(t, generic_enums)),
            body: rewrite_enum_refs_in_block(
                &f.body, generic_enums, table, outer_params, outer_args,
            ),
            span: f.span,
        is_override: f.is_override,
        }),
        Item::Class(c) => Item::Class(ClassDecl {
            extern_lib: c.extern_lib.clone(),
            is_repr_c: c.is_repr_c,
            is_packed: c.is_packed,
            is_union: c.is_union,
            name: c.name.clone(),
            parent: c.parent.clone(),
            type_params: c.type_params.clone(),
            fields: c
                .fields
                .iter()
                .map(|f| FieldDecl {
                    name: f.name.clone(),
                    ty: rewrite_enum_refs_in_type(&f.ty, generic_enums),
                    span: f.span, bits: f.bits,
                })
                .collect(),
            methods: c
                .methods
                .iter()
                .map(|m| FnDecl {
                    attrs: m.attrs.clone(),
                    name: m.name.clone(),
                    type_params: m.type_params.clone(),
                    params: m
                        .params
                        .iter()
                        .map(|p| Param {
                            name: p.name.clone(),
                            ty: rewrite_enum_refs_in_type(&p.ty, generic_enums),
                            span: p.span,
                            default: p.default.clone(),
                        })
                        .collect(),
                    ret: m.ret.as_ref().map(|t| rewrite_enum_refs_in_type(t, generic_enums)),
                    body: rewrite_enum_refs_in_block(
                        &m.body, generic_enums, table, outer_params, outer_args,
                    ),
                    span: m.span,
                is_override: m.is_override,
                })
                .collect(),
            static_methods: c
                .static_methods
                .iter()
                .map(|m| FnDecl {
                    attrs: m.attrs.clone(),
                    name: m.name.clone(),
                    type_params: m.type_params.clone(),
                    params: m.params.iter().map(|p| Param {
                        name: p.name.clone(),
                        ty: rewrite_enum_refs_in_type(&p.ty, generic_enums),
                        span: p.span,
                        default: p.default.clone(),
                    }).collect(),
                    ret: m.ret.as_ref().map(|t| rewrite_enum_refs_in_type(t, generic_enums)),
                    body: rewrite_enum_refs_in_block(
                        &m.body, generic_enums, table, outer_params, outer_args,
                    ),
                    span: m.span,
                is_override: m.is_override,
                })
                .collect(),
            static_fields: c.static_fields.clone(),
            properties: c
                .properties
                .iter()
                .map(|p| ilang_ast::PropertyDecl {
                    name: p.name.clone(),
                    ty: rewrite_enum_refs_in_type(&p.ty, generic_enums),
                    getter: p.getter.as_ref().map(|g| FnDecl {
                        attrs: g.attrs.clone(),
                        name: g.name.clone(),
                        type_params: g.type_params.clone(),
                        params: g.params.iter().map(|q| Param {
                            name: q.name.clone(),
                            ty: rewrite_enum_refs_in_type(&q.ty, generic_enums),
                            span: q.span,
                            default: q.default.clone(),
                        }).collect(),
                        ret: g.ret.as_ref().map(|t| rewrite_enum_refs_in_type(t, generic_enums)),
                        body: rewrite_enum_refs_in_block(&g.body, generic_enums, table, outer_params, outer_args),
                        span: g.span,
                    is_override: g.is_override,
                    }),
                    setter: p.setter.as_ref().map(|s| FnDecl {
                        attrs: s.attrs.clone(),
                        name: s.name.clone(),
                        type_params: s.type_params.clone(),
                        params: s.params.iter().map(|q| Param {
                            name: q.name.clone(),
                            ty: rewrite_enum_refs_in_type(&q.ty, generic_enums),
                            span: q.span,
                            default: q.default.clone(),
                        }).collect(),
                        ret: s.ret.as_ref().map(|t| rewrite_enum_refs_in_type(t, generic_enums)),
                        body: rewrite_enum_refs_in_block(&s.body, generic_enums, table, outer_params, outer_args),
                        span: s.span,
                    is_override: s.is_override,
                    }),
                    span: p.span,
                })
                .collect(),
            span: c.span,
        }),
        Item::Enum(e) => Item::Enum(EnumDecl {
            name: e.name.clone(),
            type_params: e.type_params.clone(),
            repr_ty: e.repr_ty.clone(),
            flags: e.flags,
            variants: e
                .variants
                .iter()
                .map(|v| Variant {
                    name: v.name.clone(),
                    discriminant: v.discriminant,
                    payload: match &v.payload {
                        VariantPayload::Unit => VariantPayload::Unit,
                        VariantPayload::Tuple(tys) => VariantPayload::Tuple(
                            tys.iter()
                                .map(|t| rewrite_enum_refs_in_type(t, generic_enums))
                                .collect(),
                        ),
                        VariantPayload::Struct(fields) => VariantPayload::Struct(
                            fields
                                .iter()
                                .map(|f| FieldDecl {
                                    name: f.name.clone(),
                                    ty: rewrite_enum_refs_in_type(&f.ty, generic_enums),
                                    span: f.span, bits: f.bits,
                                })
                                .collect(),
                        ),
                    },
                    span: v.span,
                })
                .collect(),
            span: e.span,
        }),
        Item::Use(u) => Item::Use(u.clone()),
        Item::Const(c) => Item::Const(c.clone()),
        Item::ExternC(b) => Item::ExternC(b.clone()),
        Item::ExternStatic(s) => Item::ExternStatic(s.clone()),
    }
}

fn rewrite_enum_refs_in_block(
    b: &Block,
    generic_enums: &HashMap<Symbol, EnumDecl>,
    table: &HashMap<Span, (Symbol, Vec<Type>)>,
    outer_params: &[Symbol],
    outer_args: &[Type],
) -> Block {
    Block {
        stmts: b
            .stmts
            .iter()
            .map(|s| {
                rewrite_enum_refs_in_stmt(s, generic_enums, table, outer_params, outer_args)
            })
            .collect(),
        tail: b.tail.as_ref().map(|e| {
            Box::new(rewrite_enum_refs_in_expr(
                e,
                generic_enums,
                table,
                outer_params,
                outer_args,
            ))
        }),
    }
}

fn rewrite_enum_refs_in_stmt(
    s: &Stmt,
    generic_enums: &HashMap<Symbol, EnumDecl>,
    table: &HashMap<Span, (Symbol, Vec<Type>)>,
    outer_params: &[Symbol],
    outer_args: &[Type],
) -> Stmt {
    let kind = match &s.kind {
        StmtKind::Let { name, ty, value } => StmtKind::Let {
            name: name.clone(),
            ty: ty.as_ref().map(|t| rewrite_enum_refs_in_type(t, generic_enums)),
            value: rewrite_enum_refs_in_expr(
                value,
                generic_enums,
                table,
                outer_params,
                outer_args,
            ),
        },
        StmtKind::LetTuple { elems, value } => StmtKind::LetTuple {
            elems: elems.clone(),
            value: rewrite_enum_refs_in_expr(
                value,
                generic_enums,
                table,
                outer_params,
                outer_args,
            ),
        },
        StmtKind::LetStruct { class, fields, value } => StmtKind::LetStruct {
            class: class.clone(),
            fields: fields.clone(),
            value: rewrite_enum_refs_in_expr(
                value,
                generic_enums,
                table,
                outer_params,
                outer_args,
            ),
        },
        StmtKind::Expr(e) => StmtKind::Expr(rewrite_enum_refs_in_expr(
            e,
            generic_enums,
            table,
            outer_params,
            outer_args,
        )),
    };
    Stmt { kind, span: s.span }
}

fn rewrite_enum_refs_in_expr(
    e: &Expr,
    generic_enums: &HashMap<Symbol, EnumDecl>,
    table: &HashMap<Span, (Symbol, Vec<Type>)>,
    outer_params: &[Symbol],
    outer_args: &[Type],
) -> Expr {
    let kind = match &e.kind {
        ExprKind::EnumCtor { enum_name, variant, args } => {
            let new_args = match args {
                ilang_ast::CtorArgs::Unit => ilang_ast::CtorArgs::Unit,
                ilang_ast::CtorArgs::Tuple(es) => ilang_ast::CtorArgs::Tuple(
                    es.iter()
                        .map(|x| {
                            rewrite_enum_refs_in_expr(
                                x, generic_enums, table, outer_params, outer_args,
                            )
                        })
                        .collect(),
                ),
                ilang_ast::CtorArgs::Struct(fs) => ilang_ast::CtorArgs::Struct(
                    fs.iter()
                        .map(|(n, x)| {
                            (
                                n.clone(),
                                rewrite_enum_refs_in_expr(
                                    x, generic_enums, table, outer_params, outer_args,
                                ),
                            )
                        })
                        .collect(),
                ),
            };
            let new_name = if generic_enums.contains_key(enum_name) {
                if let Some((tn, raw_args)) = table.get(&e.span) {
                    if tn == enum_name {
                        let concrete: Vec<Type> = raw_args
                            .iter()
                            .map(|t| subst_type(t, outer_params, outer_args))
                            .collect();
                        if !concrete.iter().any(contains_type_var) {
                            mangle_enum(enum_name.as_str(), &concrete)
                        } else {
                            enum_name.clone()
                        }
                    } else {
                        enum_name.clone()
                    }
                } else {
                    enum_name.clone()
                }
            } else {
                enum_name.clone()
            };
            ExprKind::EnumCtor {
                enum_name: new_name,
                variant: variant.clone(),
                args: new_args.into(),
            }
        }
        ExprKind::Cast { expr, ty } => ExprKind::Cast {
            expr: Box::new(rewrite_enum_refs_in_expr(
                expr, generic_enums, table, outer_params, outer_args,
            )),
            ty: rewrite_enum_refs_in_type(ty, generic_enums),
        },
        ExprKind::Match { scrutinee, arms } => {
            // Patterns may carry an explicit `enum_name` (long form
            // `Result.ok(v)`); strip it when it names a now-mangled
            // generic enum so the JIT's match lowering resolves
            // variants from the (already-mangled) scrutinee type.
            let new_scrut = rewrite_enum_refs_in_expr(
                scrutinee, generic_enums, table, outer_params, outer_args,
            );
            let new_arms = arms
                .iter()
                .map(|arm| {
                    let new_pat = match &arm.pattern.kind {
                        ilang_ast::PatternKind::Variant { enum_name, variant, bindings } => {
                            let stripped = enum_name
                                .as_ref()
                                .filter(|n| !generic_enums.contains_key(n))
                                .cloned();
                            ilang_ast::Pattern {
                                kind: ilang_ast::PatternKind::Variant {
                                    enum_name: stripped,
                                    variant: variant.clone(),
                                    bindings: bindings.clone(),
                                },
                                span: arm.pattern.span,
                            }
                        }
                        other => ilang_ast::Pattern {
                            kind: other.clone(),
                            span: arm.pattern.span,
                        },
                    };
                    ilang_ast::MatchArm {
                        pattern: new_pat,
                        body: rewrite_enum_refs_in_expr(
                            &arm.body, generic_enums, table, outer_params, outer_args,
                        ),
                        span: arm.span,
                    }
                })
                .collect();
            ExprKind::Match { scrutinee: Box::new(new_scrut), arms: new_arms }
        }
        _ => map_expr_children(e, &mut |c| {
            rewrite_enum_refs_in_expr(c, generic_enums, table, outer_params, outer_args)
        }),
    };
    Expr { kind, span: e.span }
}

fn rewrite_enum_refs_in_type(
    t: &Type,
    generic_enums: &HashMap<Symbol, EnumDecl>,
) -> Type {
    match t {
        Type::Generic(g) => {
            let new_args: Vec<Type> =
                g.args.iter().map(|a| rewrite_enum_refs_in_type(a, generic_enums)).collect();
            if generic_enums.contains_key(&g.base) && !new_args.iter().any(contains_type_var) {
                Type::Object(mangle_enum(g.base.as_str(), &new_args))
            } else {
                Type::generic(g.base.clone(), new_args)
            }
        }
        Type::Array { elem, fixed } => Type::Array {
            elem: Box::new(rewrite_enum_refs_in_type(elem, generic_enums)),
            fixed: *fixed,
        },
        Type::Optional(inner) => {
            Type::Optional(Box::new(rewrite_enum_refs_in_type(inner, generic_enums)))
        }
        Type::Weak(inner) => Type::Weak(Box::new(rewrite_enum_refs_in_type(inner, generic_enums))),
        Type::Fn(ft) => Type::func(
            ft.params.iter().map(|p| rewrite_enum_refs_in_type(p, generic_enums)).collect(),
            rewrite_enum_refs_in_type(&ft.ret, generic_enums),
        ),
        _ => t.clone(),
    }
}

fn mangle_enum(name: &str, args: &[Type]) -> Symbol {
    InstKey { class: name.into(), args: args.to_vec() }.mangled()
}

