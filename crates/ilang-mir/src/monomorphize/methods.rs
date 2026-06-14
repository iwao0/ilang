//! AST monomorphization for generic class methods (instance and
//! static). Mirrors `monomorphize_fns` but specializes methods
//! *inside* their owning class instead of producing top-level fns.
//!
//! For each generic method actually used in the program, this pass:
//!
//! 1. Synthesises a concrete (non-generic) clone of the method with
//!    its `<T, U, ...>` substituted by the inferred type args. The
//!    clone's name is `mangle_fn_name(method, args)` —
//!    `instCount<i64>`, etc.
//! 2. Drops the original generic method template from the class.
//! 3. Rewrites every `MethodCall.method` whose recorded type args
//!    are fully concrete to the mangled name.
//!
//! Only call sites whose recorded args contain no `TypeVar` are
//! rewritten — call sites inside another generic context (which
//! shouldn't exist in practice once class monomorphization runs
//! first) leave a dangling reference, surfaced later as
//! "unknown method".

use std::collections::{HashMap, HashSet};

use ilang_ast::{Block, ClassDecl, Expr, ExprKind, FnDecl, Item, Program, Span, Symbol, Type};

use super::class::{contains_type_var, mangle_fn_name, specialize_fn, subst_type};
use super::walk::{map_block_children, map_expr_children, walk_block_children, walk_expr_children, walk_top_stmts};

pub fn monomorphize_methods(
    prog: &Program,
    method_call_type_args: &HashMap<Span, (Symbol, Symbol, Vec<Type>)>,
) -> Program {
    // Index every generic method (instance + static) by
    // `(class_name, method_name)`. We need this for two things:
    // deciding whether to mangle a recorded call site, and looking
    // up the template body when synthesizing a specialization.
    let mut generic_methods: HashMap<(Symbol, Symbol), (FnDecl, bool)> = HashMap::new();
    for item in &prog.items {
        if let Item::Class(c) = item {
            for m in c.methods.iter() {
                if !m.type_params.is_empty() {
                    generic_methods.insert((c.name, m.name), (m.clone(), false));
                }
            }
            for m in c.static_methods.iter() {
                if !m.type_params.is_empty() {
                    generic_methods.insert((c.name, m.name), (m.clone(), true));
                }
            }
        }
    }
    if generic_methods.is_empty() {
        return prog.clone();
    }

    // Worklist of (class, method, concrete-args) tuples. Dedup by
    // mangled method name within the class.
    let mut requested: HashSet<(Symbol, Symbol)> = HashSet::new();
    let mut worklist: Vec<(Symbol, Symbol, Symbol, Vec<Type>)> = Vec::new(); // (class, original_method, mangled_method, args)

    let enqueue = |class: Symbol,
                       method: Symbol,
                       args: &[Type],
                       wl: &mut Vec<(Symbol, Symbol, Symbol, Vec<Type>)>,
                       req: &mut HashSet<(Symbol, Symbol)>| {
        if !generic_methods.contains_key(&(class, method)) {
            return;
        }
        if args.iter().any(contains_type_var) {
            return;
        }
        let mangled = mangle_fn_name(method.as_str(), args);
        if req.insert((class, mangled)) {
            wl.push((class, method, mangled, args.to_vec()));
        }
    };

    // Seed the worklist from every recorded method call site.
    let empty_params: Vec<Symbol> = Vec::new();
    let empty_args: Vec<Type> = Vec::new();
    for item in &prog.items {
        seed_in_item(
            item,
            method_call_type_args,
            &empty_params,
            &empty_args,
            &mut |class, method, args| enqueue(class, method, args, &mut worklist, &mut requested),
        );
    }
    walk_top_stmts(&prog.stmts, prog.tail.as_ref(), &mut |e| {
        seed_in_expr(
            e,
            method_call_type_args,
            &empty_params,
            &empty_args,
            None,
            &mut |class, method, args| enqueue(class, method, args, &mut worklist, &mut requested),
        );
    });

    // Drain the worklist. Each specialization may discover further
    // generic-method calls in its (substituted) body; those go back on.
    // Map: class → list of (mangled_name, specialized FnDecl, is_static).
    let mut specializations: HashMap<Symbol, Vec<(Symbol, FnDecl, bool)>> = HashMap::new();
    let mut seen: HashSet<(Symbol, Symbol)> = HashSet::new();
    while let Some((class, method, mangled, args)) = worklist.pop() {
        if !seen.insert((class, mangled)) {
            continue;
        }
        let (template, is_static) = generic_methods.get(&(class, method)).unwrap().clone();
        let outer_params: Vec<Symbol> = template.type_params.iter().copied().collect();
        let outer_args = args.clone();

        let mut new_method = specialize_fn(&template, &outer_params, &outer_args);
        new_method.name = mangled;
        new_method.type_params = Box::new([]);
        // Re-scan the substituted body for further generic calls FIRST,
        // while the inner calls still carry their recorded method names —
        // the rewrite below renames them to the mangled callee, which
        // would then no longer match `recorded_method` in the seed.
        seed_in_block(
            &new_method.body,
            method_call_type_args,
            &outer_params,
            &outer_args,
            Some(class),
            &mut |c, m, a| enqueue(c, m, a, &mut worklist, &mut requested),
        );

        // Now rewrite generic-method calls in the specialized body so the
        // inner call refers to the mangled callee.
        new_method.body = rewrite_method_calls_in_block(
            &new_method.body,
            method_call_type_args,
            &outer_params,
            &outer_args,
            Some(class),
            &generic_methods,
        );

        specializations
            .entry(class)
            .or_default()
            .push((mangled, new_method, is_static));
    }

    // Build the rewritten program: drop generic methods from each
    // class, append the new specializations, rewrite every method
    // call site (anywhere in the program) to the mangled name.
    let out_items: Vec<Item> = prog
        .items
        .iter()
        .map(|item| match item {
            Item::Class(c) => Item::Class(rewrite_class(
                c,
                &specializations,
                method_call_type_args,
                &empty_params,
                &empty_args,
                &generic_methods,
            )),
            Item::Fn(f) => Item::Fn(rewrite_fn(
                f,
                method_call_type_args,
                &empty_params,
                &empty_args,
                None,
                &generic_methods,
            )),
            other => other.clone(),
        })
        .collect();
    let pseudo = Block {
        stmts: prog.stmts.clone(),
        tail: prog.tail.as_ref().map(|e| Box::new(e.clone())),
    };
    let rewritten = map_block_children(
        &pseudo,
        &mut |e| {
            rewrite_method_calls_in_expr(
                e,
                method_call_type_args,
                &empty_params,
                &empty_args,
                None,
                &generic_methods,
            )
        },
        &mut |t: &Type| t.clone(),
    );
    Program {
        items: out_items,
        stmts: rewritten.stmts,
        tail: rewritten.tail.map(|b| *b),
    }
}

fn rewrite_class(
    c: &ClassDecl,
    specializations: &HashMap<Symbol, Vec<(Symbol, FnDecl, bool)>>,
    table: &HashMap<Span, (Symbol, Symbol, Vec<Type>)>,
    outer_params: &[Symbol],
    outer_args: &[Type],
    generic_methods: &HashMap<(Symbol, Symbol), (FnDecl, bool)>,
) -> ClassDecl {
    let class_specs = specializations.get(&c.name);
    // Drop generic templates from methods / static_methods; rewrite
    // bodies of the remaining concrete methods so any internal
    // generic-method calls get the mangled callee.
    let mut new_methods: Vec<FnDecl> = c
        .methods
        .iter()
        .filter(|m| m.type_params.is_empty())
        .map(|m| rewrite_fn(m, table, outer_params, outer_args, Some(c.name), generic_methods))
        .collect();
    let mut new_static_methods: Vec<FnDecl> = c
        .static_methods
        .iter()
        .filter(|m| m.type_params.is_empty())
        .map(|m| rewrite_fn(m, table, outer_params, outer_args, Some(c.name), generic_methods))
        .collect();
    if let Some(specs) = class_specs {
        for (_mangled, fn_decl, is_static) in specs.iter() {
            if *is_static {
                new_static_methods.push(fn_decl.clone());
            } else {
                new_methods.push(fn_decl.clone());
            }
        }
    }
    // Rewrite property getter / setter bodies too — property bodies
    // are FnDecls and may invoke generic methods.
    let new_properties: Vec<ilang_ast::PropertyDecl> = c
        .properties
        .iter()
        .map(|p| ilang_ast::PropertyDecl {
            is_static: p.is_static,
            is_pub: p.is_pub,
            name: p.name,
            ty: p.ty.clone(),
            getter: p
                .getter
                .as_ref()
                .map(|g| rewrite_fn(g, table, outer_params, outer_args, Some(c.name), generic_methods)),
            setter: p
                .setter
                .as_ref()
                .map(|s| rewrite_fn(s, table, outer_params, outer_args, Some(c.name), generic_methods)),
            span: p.span,
        })
        .collect();
    ClassDecl {
        is_pub: c.is_pub,
        extern_lib: c.extern_lib.clone(),
        is_repr_c: c.is_repr_c,
        is_packed: c.is_packed,
        is_handle: c.is_handle,
        is_union: c.is_union,
        name: c.name,
        parent: c.parent,
        interfaces: c.interfaces.clone(),
        type_params: c.type_params.clone(),
        fields: c.fields.clone(),
        methods: new_methods.into(),
        static_methods: new_static_methods.into(),
        static_fields: c.static_fields.clone(),
        properties: new_properties.into(),
        attrs: c.attrs.clone(),
        span: c.span,
    }
}

fn rewrite_fn(
    f: &FnDecl,
    table: &HashMap<Span, (Symbol, Symbol, Vec<Type>)>,
    outer_params: &[Symbol],
    outer_args: &[Type],
    enclosing: Option<Symbol>,
    generic_methods: &HashMap<(Symbol, Symbol), (FnDecl, bool)>,
) -> FnDecl {
    let body =
        rewrite_method_calls_in_block(&f.body, table, outer_params, outer_args, enclosing, generic_methods);
    FnDecl {
        is_pub: f.is_pub,
        attrs: f.attrs.clone(),
        name: f.name,
        type_params: f.type_params.clone(),
        params: f.params.clone(),
        ret: f.ret.clone(),
        body,
        span: f.span,
        is_override: f.is_override,
        is_async: f.is_async,
        intrinsic_name: f.intrinsic_name,
    }
}

fn rewrite_method_calls_in_block(
    b: &Block,
    table: &HashMap<Span, (Symbol, Symbol, Vec<Type>)>,
    outer_params: &[Symbol],
    outer_args: &[Type],
    enclosing: Option<Symbol>,
    generic_methods: &HashMap<(Symbol, Symbol), (FnDecl, bool)>,
) -> Block {
    map_block_children(
        b,
        &mut |e| {
            rewrite_method_calls_in_expr(e, table, outer_params, outer_args, enclosing, generic_methods)
        },
        &mut |t: &Type| t.clone(),
    )
}

fn rewrite_method_calls_in_expr(
    e: &Expr,
    table: &HashMap<Span, (Symbol, Symbol, Vec<Type>)>,
    outer_params: &[Symbol],
    outer_args: &[Type],
    enclosing: Option<Symbol>,
    generic_methods: &HashMap<(Symbol, Symbol), (FnDecl, bool)>,
) -> Expr {
    let recurse = |c: &Expr| rewrite_method_calls_in_expr(c, table, outer_params, outer_args, enclosing, generic_methods);
    let kind = match &e.kind {
        ExprKind::MethodCall { obj, method, args } => {
            let new_obj = Box::new(recurse(obj));
            let new_args: Box<[Expr]> = args.iter().map(&recurse).collect();
            let new_method = match table.get(&e.span) {
                Some((class, recorded_method, raw_args)) if recorded_method == method => {
                    let cls = self_class(*class, enclosing);
                    if generic_methods.contains_key(&(cls, *method)) {
                        let concrete: Vec<Type> = raw_args
                            .iter()
                            .map(|t| subst_type(t, outer_params, outer_args))
                            .collect();
                        if !concrete.iter().any(contains_type_var) {
                            mangle_fn_name(method.as_str(), &concrete)
                        } else {
                            *method
                        }
                    } else {
                        *method
                    }
                }
                _ => *method,
            };
            ExprKind::MethodCall {
                obj: new_obj,
                method: new_method,
                args: new_args,
            }
        }
        _ => map_expr_children(
            e,
            &mut |c| recurse(c),
            &mut |t: &Type| t.clone(),
        ),
    };
    Expr { kind, span: e.span }
}

/// A `this`-call inside a specialized class method is recorded under the
/// GENERIC base name (`Box`) — at check time the class was still generic.
/// Remap it to the specialized class (`Box<i64>`) currently being
/// processed so the generic-method lookup matches.
fn self_class(recorded: Symbol, enclosing: Option<Symbol>) -> Symbol {
    if let Some(enc) = enclosing {
        let es = enc.as_str();
        let base = es.split('<').next().unwrap_or(es);
        if recorded.as_str() == base {
            return enc;
        }
    }
    recorded
}

fn seed_in_item(
    item: &Item,
    table: &HashMap<Span, (Symbol, Symbol, Vec<Type>)>,
    outer_params: &[Symbol],
    outer_args: &[Type],
    visit: &mut dyn FnMut(Symbol, Symbol, &[Type]),
) {
    match item {
        Item::Fn(f) => seed_in_block(&f.body, table, outer_params, outer_args, None, visit),
        Item::Class(c) => {
            let enc = Some(c.name);
            for m in c.methods.iter() {
                seed_in_block(&m.body, table, outer_params, outer_args, enc, visit);
            }
            for m in c.static_methods.iter() {
                seed_in_block(&m.body, table, outer_params, outer_args, enc, visit);
            }
            for p in c.properties.iter() {
                if let Some(g) = &p.getter {
                    seed_in_block(&g.body, table, outer_params, outer_args, enc, visit);
                }
                if let Some(s) = &p.setter {
                    seed_in_block(&s.body, table, outer_params, outer_args, enc, visit);
                }
            }
        }
        _ => {}
    }
}

fn seed_in_block(
    b: &Block,
    table: &HashMap<Span, (Symbol, Symbol, Vec<Type>)>,
    outer_params: &[Symbol],
    outer_args: &[Type],
    enclosing: Option<Symbol>,
    visit: &mut dyn FnMut(Symbol, Symbol, &[Type]),
) {
    walk_block_children(b, &mut |e| {
        seed_in_expr(e, table, outer_params, outer_args, enclosing, visit)
    });
}

fn seed_in_expr(
    e: &Expr,
    table: &HashMap<Span, (Symbol, Symbol, Vec<Type>)>,
    outer_params: &[Symbol],
    outer_args: &[Type],
    enclosing: Option<Symbol>,
    visit: &mut dyn FnMut(Symbol, Symbol, &[Type]),
) {
    if let ExprKind::MethodCall { method, .. } = &e.kind {
        if let Some((class, recorded_method, raw_args)) = table.get(&e.span) {
            if recorded_method == method {
                let concrete: Vec<Type> = raw_args
                    .iter()
                    .map(|t| subst_type(t, outer_params, outer_args))
                    .collect();
                visit(self_class(*class, enclosing), *method, &concrete);
            }
        }
    }
    walk_expr_children(e, &mut |c| {
        seed_in_expr(c, table, outer_params, outer_args, enclosing, visit)
    });
}
