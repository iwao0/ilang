//! Extracted from `monomorphize/mod.rs`.

use std::collections::{HashMap, HashSet};

use ilang_ast::{
    Block, Expr, ExprKind, FnDecl, Item, Program, Span,
    Stmt, StmtKind, Symbol, Type,
};

use super::walk::{map_expr_children, walk_expr_children};
use super::class::*;

pub fn monomorphize_fns(
    prog: &Program,
    call_type_args: &HashMap<Span, (Symbol, Vec<Type>)>,
    enum_ctor_type_args: &HashMap<Span, (Symbol, Vec<Type>)>,
) -> Program {
    // Collect generic enum decls so the post-specialize EnumCtor
    // rewrite (below) knows which enum_names to mangle.
    let generic_enums: HashMap<Symbol, ilang_ast::EnumDecl> = prog
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Enum(e) if !e.type_params.is_empty() => Some((e.name.clone(), e.clone())),
            _ => None,
        })
        .collect();
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
    super::walk::walk_top_stmts(&prog.stmts, prog.tail.as_ref(), &mut |e| {
        seed_calls_in_expr(
            e,
            call_type_args,
            &empty_params,
            &empty_args,
            &mut |name, args| enqueue(name, args, &mut worklist, &mut requested),
        );
    });

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

        // 4. Rewrite EnumCtors in the substituted body so refs to
        //    generic enums get their `enum_name` mangled with the
        //    now-concrete args. Without this, `MyOpt.some(v)` inside
        //    a specialized `wrap_i64` body keeps `enum_name="MyOpt"`,
        //    and MIR lower can't find the (already-dropped) generic
        //    `MyOpt` template.
        new_fn.body = super::enums::rewrite_enum_refs_in_block(
            &new_fn.body,
            &generic_enums,
            enum_ctor_type_args,
            &outer_params,
            &outer_args,
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

pub(super) fn seed_calls_in_item(
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
            for m in &c.static_methods {
                seed_calls_in_block(&m.body, table, outer_params, outer_args, visit);
            }
            for p in &c.properties {
                if let Some(g) = &p.getter {
                    seed_calls_in_block(&g.body, table, outer_params, outer_args, visit);
                }
                if let Some(s) = &p.setter {
                    seed_calls_in_block(&s.body, table, outer_params, outer_args, visit);
                }
            }
        }
        Item::Enum(_) | Item::Use(_) | Item::Const(_)  | Item::ExternC(_) => {}
        Item::Interface(_) => {}
    }
}

pub(super) fn seed_calls_in_block(
    b: &Block,
    table: &HashMap<Span, (Symbol, Vec<Type>)>,
    outer_params: &[Symbol],
    outer_args: &[Type],
    visit: &mut dyn FnMut(&str, &[Type]),
) {
    super::walk::walk_block_children(b, &mut |e| {
        seed_calls_in_expr(e, table, outer_params, outer_args, visit)
    });
}

pub(super) fn seed_calls_in_expr(
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

pub(super) fn rewrite_calls_in_item(
    item: &Item,
    table: &HashMap<Span, (Symbol, Vec<Type>)>,
    outer_params: &[Symbol],
    outer_args: &[Type],
    generic_fns: &HashMap<Symbol, FnDecl>,
) -> Item {
    let mut map_block =
        |b: &Block| rewrite_calls_in_block(b, table, outer_params, outer_args, generic_fns);
    let mut keep_type = |t: &Type| t.clone();
    match item {
        Item::Fn(f) => Item::Fn(super::walk::map_fn_decl(f, &mut map_block, &mut keep_type)),
        Item::Class(c) => Item::Class(super::walk::map_class_decl(
            c,
            &mut map_block,
            &mut keep_type,
        )),
        Item::Enum(e) => Item::Enum(e.clone()),
        Item::Use(u) => Item::Use(u.clone()),
        Item::Const(c) => Item::Const(c.clone()),
        Item::ExternC(b) => Item::ExternC(b.clone()),
        Item::Interface(i) => Item::Interface(i.clone()),
    }
}

pub(super) fn rewrite_calls_in_block(
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

pub(super) fn rewrite_calls_in_stmt(
    s: &Stmt,
    table: &HashMap<Span, (Symbol, Vec<Type>)>,
    outer_params: &[Symbol],
    outer_args: &[Type],
    generic_fns: &HashMap<Symbol, FnDecl>,
) -> Stmt {
    let kind = match &s.kind {
        StmtKind::Let { name, ty, value, .. } => StmtKind::Let {
            is_pub: false,
                is_const: false,
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
    Stmt { kind, span: s.span, source_module: s.source_module.clone() }
}

pub(super) fn rewrite_calls_in_expr(
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

