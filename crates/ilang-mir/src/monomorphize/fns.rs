//! Extracted from `monomorphize/mod.rs`.

use std::collections::{HashMap, HashSet};

use ilang_ast::{Block, Expr, ExprKind, FnDecl, Item, Program, Span, Symbol, Type};

use super::walk::{map_expr_children, walk_expr_children};
use super::class::*;

pub fn monomorphize_fns(
    prog: &Program,
    call_type_args: &HashMap<Span, (Symbol, Vec<Type>)>,
    enum_ctor_type_args: &HashMap<Span, (Symbol, Vec<Type>)>,
) -> Program {
    // Collect generic enum decls so the post-specialize EnumCtor
    // rewrite (below) knows which enum_names to mangle. Built-in
    // `Result<T, E>` has no source decl but is mangled per instantiation
    // like any generic enum — without it, a `Result.ok(x)` inside a
    // specialized generic fn body keeps `enum_name="Result"` and MIR
    // lower fails with "unknown enum Result" (the `monomorphize_enums`
    // pass adds the same entry for the same reason).
    let mut generic_enums: HashMap<Symbol, ilang_ast::EnumDecl> = prog
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Enum(e) if !e.type_params.is_empty() => Some((e.name.clone(), e.clone())),
            _ => None,
        })
        .collect();
    generic_enums
        .entry("Result".into())
        .or_insert_with(super::enums::result_template);
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
    // Worklist entries carry the already-computed mangled name
    // alongside `(name, args)` so the drain below doesn't re-run
    // `mangle_fn_name` (which stringifies every type arg) a second
    // time — `enqueue` already computed it for the dedup key.
    let mut worklist: Vec<(Symbol, Symbol, Vec<Type>)> = Vec::new();

    let enqueue = |name: &str,
                   args: &[Type],
                   wl: &mut Vec<(Symbol, Symbol, Vec<Type>)>,
                   req: &mut HashSet<Symbol>| {
        if !generic_fns.contains_key(&Symbol::intern(name)) {
            return;
        }
        if args.iter().any(contains_type_var) {
            return; // call site sits in another generic context — skip
        }
        let key = mangle_fn_name(name, args);
        if req.insert(key) {
            wl.push((key, name.into(), args.to_vec()));
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
    while let Some((mangled, name, args)) = worklist.pop() {
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

        // 3. Rewrite, in a single body walk, both generic-fn calls
        //    (mangle the callee) and generic-enum references (mangle
        //    each `EnumCtor.enum_name` + strip now-mangled match
        //    patterns + mangle enum types). The two transforms touch
        //    disjoint node kinds (`Call` vs `EnumCtor` / `Match`), so
        //    one traversal is equivalent to the former two separate
        //    passes and saves a full clone of the body. Without the
        //    enum half, `MyOpt.some(v)` inside a specialized `wrap_i64`
        //    body would keep `enum_name="MyOpt"` and MIR lower couldn't
        //    find the (already-dropped) generic `MyOpt` template.
        new_fn.body = rewrite_calls_and_enums_in_block(
            &new_fn.body,
            call_type_args,
            enum_ctor_type_args,
            &outer_params,
            &outer_args,
            &generic_fns,
            &generic_enums,
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
    // Wrap top-level stmts + tail as a Block so we can reuse
    // map_block_children. Unwraps the Boxed tail on the way out.
    let pseudo = Block {
        stmts: prog.stmts.clone(),
        tail: prog.tail.as_ref().map(|e| Box::new(e.clone())),
    };
    let rewritten = super::walk::map_block_children(
        &pseudo,
        &mut |e| rewrite_calls_in_expr(e, call_type_args, &empty_params, &empty_args, &generic_fns),
        &mut |t: &Type| t.clone(),
    );
    let stmts = rewritten.stmts;
    let tail = rewritten.tail.map(|b| *b);
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
    let mut map_expr =
        |e: &Expr| rewrite_calls_in_expr(e, table, outer_params, outer_args, generic_fns);
    let mut map_block =
        |b: &Block| rewrite_calls_in_block(b, table, outer_params, outer_args, generic_fns);
    let mut keep_type = |t: &Type| t.clone();
    match item {
        Item::Fn(f) => Item::Fn(super::walk::map_fn_decl(
            f,
            &mut map_expr,
            &mut map_block,
            &mut keep_type,
        )),
        Item::Class(c) => Item::Class(super::walk::map_class_decl(
            c,
            &mut map_expr,
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
    // `rewrite_calls` only touches `Call` callee names; nothing
    // type-bearing changes shape.
    super::walk::map_block_children(
        b,
        &mut |e| rewrite_calls_in_expr(e, table, outer_params, outer_args, generic_fns),
        &mut |t: &Type| t.clone(),
    )
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
            // `rewrite_calls` doesn't touch types — pass an
            // identity type mapper.
            map_expr_children(
                e,
                &mut |c| rewrite_calls_in_expr(c, table, outer_params, outer_args, generic_fns),
                &mut |t: &Type| t.clone(),
            )
        }
    };
    Expr { kind, span: e.span }
}

/// Combined per-specialization body rewrite: mangle generic-fn call
/// callees (`call_table`) AND generic-enum `EnumCtor.enum_name` /
/// match-pattern / enum types (`enum_table` + `generic_enums`) in a
/// single traversal. `Call` and `EnumCtor` / `Match` are disjoint node
/// kinds, so doing both here is equivalent to the former two separate
/// `rewrite_calls_in_block` + `rewrite_enum_refs_in_block` passes while
/// cloning the body only once.
pub(super) fn rewrite_calls_and_enums_in_block(
    b: &Block,
    call_table: &HashMap<Span, (Symbol, Vec<Type>)>,
    enum_table: &HashMap<Span, (Symbol, Vec<Type>)>,
    outer_params: &[Symbol],
    outer_args: &[Type],
    generic_fns: &HashMap<Symbol, FnDecl>,
    generic_enums: &HashMap<Symbol, ilang_ast::EnumDecl>,
) -> Block {
    super::walk::map_block_children(
        b,
        &mut |e| {
            rewrite_calls_and_enums_in_expr(
                e, call_table, enum_table, outer_params, outer_args, generic_fns, generic_enums,
            )
        },
        &mut |t: &Type| super::enums::rewrite_enum_refs_in_type(t, generic_enums),
    )
}

fn rewrite_calls_and_enums_in_expr(
    e: &Expr,
    call_table: &HashMap<Span, (Symbol, Vec<Type>)>,
    enum_table: &HashMap<Span, (Symbol, Vec<Type>)>,
    outer_params: &[Symbol],
    outer_args: &[Type],
    generic_fns: &HashMap<Symbol, FnDecl>,
    generic_enums: &HashMap<Symbol, ilang_ast::EnumDecl>,
) -> Expr {
    let recurse = |c: &Expr| {
        rewrite_calls_and_enums_in_expr(
            c, call_table, enum_table, outer_params, outer_args, generic_fns, generic_enums,
        )
    };
    let kind = match &e.kind {
        ExprKind::Call { callee, args } => {
            let new_args: Vec<Expr> = args.iter().map(&recurse).collect();
            let new_callee = if generic_fns.contains_key(callee) {
                if let Some((cname, raw_args)) = call_table.get(&e.span) {
                    if cname == callee {
                        let concrete: Vec<Type> = raw_args
                            .iter()
                            .map(|t| subst_type(t, outer_params, outer_args))
                            .collect();
                        if !concrete.iter().any(contains_type_var) {
                            mangle_fn_name(callee.as_str(), &concrete)
                        } else {
                            callee.clone()
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
            ExprKind::Call { callee: new_callee, args: new_args.into() }
        }
        ExprKind::EnumCtor { enum_name, variant, args } => {
            let new_args = match args {
                ilang_ast::CtorArgs::Unit => ilang_ast::CtorArgs::Unit,
                ilang_ast::CtorArgs::Tuple(es) => {
                    ilang_ast::CtorArgs::Tuple(es.iter().map(&recurse).collect())
                }
                ilang_ast::CtorArgs::Struct(fs) => ilang_ast::CtorArgs::Struct(
                    fs.iter().map(|(n, x)| (n.clone(), recurse(x))).collect(),
                ),
            };
            let new_name = if generic_enums.contains_key(enum_name) {
                if let Some((tn, raw_args)) = enum_table.get(&e.span) {
                    if tn == enum_name {
                        let concrete: Vec<Type> = raw_args
                            .iter()
                            .map(|t| subst_type(t, outer_params, outer_args))
                            .collect();
                        if !concrete.iter().any(contains_type_var) {
                            super::enums::mangle_enum(enum_name.as_str(), &concrete)
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
        ExprKind::Match { scrutinee, arms } => {
            let new_scrut = recurse(scrutinee);
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
                        body: recurse(&arm.body),
                        span: arm.span,
                    }
                })
                .collect();
            ExprKind::Match { scrutinee: Box::new(new_scrut), arms: new_arms }
        }
        _ => map_expr_children(
            e,
            &mut |c| recurse(c),
            &mut |t: &Type| super::enums::rewrite_enum_refs_in_type(t, generic_enums),
        ),
    };
    Expr { kind, span: e.span }
}

