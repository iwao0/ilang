//! Extracted from `monomorphize/mod.rs`.

use std::collections::{HashMap, HashSet};

use ilang_ast::{
    Block, ClassDecl, Expr, ExprKind, FnDecl, Item, Param, Program, Span, Symbol, Type,
};

use super::*;

/// Run the pass. Returns a new `Program` where every reference to a
/// generic class has been replaced by a concrete monomorphized
/// instantiation. Non-generic items pass through unchanged.
pub fn monomorphize(
    prog: &Program,
    enum_ctor_type_args: &HashMap<Span, (Symbol, Vec<Type>)>,
) -> Program {
    monomorphize_with_requests(prog, &[], enum_ctor_type_args)
}

/// Like [`monomorphize`], with extra instantiation requests that
/// don't appear anywhere in the program's own text. The REPL passes
/// its slot types here: a previous chunk's `let b = new Box<i64>(..)`
/// persists as a host slot of type `Box<i64>`, and a later chunk
/// that only READS `b` contains no instantiation site of its own —
/// without the seed, the specialized class wouldn't be synthesized
/// and the slot silently failed to resolve.
pub fn monomorphize_with_requests(
    prog: &Program,
    extra_request_types: &[Type],
    enum_ctor_type_args: &HashMap<Span, (Symbol, Vec<Type>)>,
) -> Program {
    // Stash the enum-ctor type-arg table so `subst_expr` can re-mangle a
    // generic-enum ctor inside a specialized generic-class method body.
    ENUM_CTOR_TYPE_ARGS.with(|tbl| {
        *tbl.borrow_mut() = enum_ctor_type_args.clone();
    });
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
    // for generic instantiations. Pre-populate `needed` with the
    // names of existing concrete (non-generic) classes so the
    // worklist skips re-synthesizing them when a previous
    // monomorphize round already produced them — important when
    // monomorphize runs more than once (e.g. after fn
    // specialization surfaces new class refs).
    let mut needed: HashSet<Symbol> = prog
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Class(c) if c.type_params.is_empty() => Some(c.name.clone()),
            _ => None,
        })
        .collect();
    let mut worklist: Vec<InstKey> = Vec::new();
    let seed = |t: &Type, needed: &mut HashSet<Symbol>, work: &mut Vec<InstKey>| {
        collect_instantiations(t, needed, work);
    };
    for t in extra_request_types {
        seed(t, &mut needed, &mut worklist);
    }
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
                    // A generic method's body references its own type
                    // params (`new Box<U>` in `remap<U>: Box<U>`); those
                    // aren't substituted yet, so scanning here would
                    // synthesize a bogus `Box<U>` class. Skip generic
                    // methods — `monomorphize_methods` specializes them
                    // and the concrete body is scanned in the next round.
                    if m.type_params.is_empty() {
                        scan_fn(m, &mut needed, &mut worklist);
                    }
                }
                for m in &c.static_methods {
                    if m.type_params.is_empty() {
                        scan_fn(m, &mut needed, &mut worklist);
                    }
                }
                for p in &c.properties {
                    seed(&p.ty, &mut needed, &mut worklist);
                    if let Some(g) = &p.getter {
                        scan_fn(g, &mut needed, &mut worklist);
                    }
                    if let Some(s) = &p.setter {
                        scan_fn(s, &mut needed, &mut worklist);
                    }
                }
            }
            // Skip seeding from generic fn bodies: they reference
            // their own type params as `Object("T")`, and
            // `contains_type_var` (which only treats `TypeVar` as
            // a type variable) would wrongly consider them
            // concrete, queueing fake instantiations like
            // `Box<T>`. monomorphize_fns specializes such bodies
            // per call site; a second pass over the program then
            // picks up the resulting concrete `Box<i64>` refs.
            Item::Fn(f) if f.type_params.is_empty() => {
                scan_fn(f, &mut needed, &mut worklist)
            }
            Item::Fn(_) => {}
            Item::Enum(_) | Item::Use(_) | Item::Const(_)  | Item::ExternC(_) => {}
            Item::Interface(_) => {}
        }
    }
    super::walk::walk_types_in_top_stmts(&prog.stmts, &mut |t| {
        scan_type(t, &mut needed, &mut worklist)
    });
    super::walk::walk_top_stmts(&prog.stmts, prog.tail.as_ref(), &mut |e| {
        scan_expr(e, &mut needed, &mut worklist)
    });

    // Iteratively monomorphize each pending instantiation. As we
    // substitute T → concrete in method bodies, new generic refs may
    // appear (e.g. `class Wrap<T> { f(): Box<T> { ... } }` instantiated
    // with T=i64 yields a `Box<i64>` ref) — those go back on the
    // worklist.
    let mut synthesized: HashMap<Symbol, ClassDecl> = HashMap::new();
    // Guard against non-terminating monomorphization: a generic class
    // whose method body / return type names a STRICTLY DEEPER
    // instantiation of itself (`class Wrap<T> { f(): Wrap<Wrap<T>> { ...
    // } }`) expands forever — each level is a distinct mangled name the
    // `synthesized` dedup can't collapse, so the worklist never drains.
    // The substitution is eager (every method of an instantiated class is
    // scanned even if never called), so just constructing `new Wrap<i64>`
    // triggers it. Bail with a clear message instead of hanging. The cap
    // is far above any realistic program's instantiation count.
    const MONO_CLASS_LIMIT: usize = 1_000;
    while let Some(key) = worklist.pop() {
        if synthesized.len() >= MONO_CLASS_LIMIT {
            panic!(
                "monomorphization limit exceeded ({MONO_CLASS_LIMIT} generic class \
                 instantiations) — this usually means a generic class is recursively \
                 instantiated at ever-deeper type arguments (e.g. a method returning \
                 `Wrap<Wrap<T>>`), which can never terminate"
            );
        }
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
            // Skip generic methods — see the template-scan note above.
            if m.type_params.is_empty() {
                scan_fn(m, &mut needed, &mut worklist);
            }
        }
        for m in &new_class.static_methods {
            if m.type_params.is_empty() {
                scan_fn(m, &mut needed, &mut worklist);
            }
        }
        for p in &new_class.properties {
            scan_type(&p.ty, &mut needed, &mut worklist);
            if let Some(g) = &p.getter {
                scan_fn(g, &mut needed, &mut worklist);
            }
            if let Some(s) = &p.setter {
                scan_fn(s, &mut needed, &mut worklist);
            }
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
    let pseudo = Block {
        stmts: prog.stmts.clone(),
        tail: prog.tail.as_ref().map(|e| Box::new(e.clone())),
    };
    let rewritten = super::walk::map_block_children(
        &pseudo,
        &mut rewrite_expr,
        &mut rewrite_type,
    );
    let stmts = rewritten.stmts;
    let tail = rewritten.tail.map(|b| *b);
    Program {
        items: out_items,
        stmts,
        tail,
    }
}

pub(super) fn scan_fn(f: &FnDecl, needed: &mut HashSet<Symbol>, work: &mut Vec<InstKey>) {
    for Param { ty, .. } in &f.params {
        scan_type(ty, needed, work);
    }
    if let Some(t) = &f.ret {
        scan_type(t, needed, work);
    }
    scan_block(&f.body, needed, work);
}

pub(super) fn scan_block(b: &Block, needed: &mut HashSet<Symbol>, work: &mut Vec<InstKey>) {
    super::walk::walk_types_in_block(b, &mut |t| scan_type(t, needed, work));
    super::walk::walk_block_children(b, &mut |e| scan_expr(e, needed, work));
}

pub(super) fn scan_expr(e: &Expr, needed: &mut HashSet<Symbol>, work: &mut Vec<InstKey>) {
    super::walk::walk_types_in_expr(e, &mut |t| scan_type(t, needed, work));
    // `new G<Ts>(...)` is itself a seed for the worklist, on top of
    // the type args already visited above.
    if let ExprKind::New { type_args, class, .. } = &e.kind {
        if !type_args.is_empty() {
            push_inst(class.clone(), type_args.to_vec(), needed, work);
        }
    }
    super::walk::walk_expr_children(e, &mut |c| scan_expr(c, needed, work));
}

pub(super) fn scan_type(t: &Type, needed: &mut HashSet<Symbol>, work: &mut Vec<InstKey>) {
    collect_instantiations(t, needed, work);
}

pub(super) fn collect_instantiations(
    t: &Type,
    needed: &mut HashSet<Symbol>,
    work: &mut Vec<InstKey>,
) {
    super::walk::walk_types_pre(t, &mut |ty| {
        // Only enqueue concrete instantiations (no remaining type
        // variables). A `Box<T>` reference inside `class Wrap<T>`'s
        // body is left as-is here; substitute_class produces the
        // concrete `Box<i64>` later, which seeds the worklist on
        // the next round.
        if let Type::Generic(g) = ty {
            if !contains_type_var(ty) {
                push_inst(g.base.clone(), g.args.to_vec(), needed, work);
            }
        }
    });
}

pub(super) fn push_inst(
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

pub(super) fn contains_type_var(t: &Type) -> bool {
    match t {
        Type::TypeVar(_) => true,
        Type::Array { elem, .. } => contains_type_var(elem),
        Type::Optional(inner) | Type::Weak(inner) => contains_type_var(inner),
        Type::Generic(g) => g.args.iter().any(contains_type_var),
        Type::Tuple(elems) => elems.iter().any(contains_type_var),
        Type::Fn(ft) => {
            ft.params.iter().any(contains_type_var) || contains_type_var(&ft.ret)
        }
        _ => false,
    }
}

pub(super) fn specialize_class(c: &ClassDecl, args: &[Type], mangled: &str) -> ClassDecl {
    let params = c.type_params.clone();
    // Concrete generic args (e.g. T = Box<i64>) need to be collapsed
    // to their mangled `Object("Box<i64>")` form before substitution,
    // otherwise nested instantiations leak through as `Type::Generic`.
    let args: Vec<Type> = args.iter().map(rewrite_type).collect();
    let args = &args[..];
    let mut map_expr = |e: &Expr| subst_expr(e, &params, args);
    let mut map_block = |b: &Block| subst_block(b, &params, args);
    let mut map_type = |t: &Type| subst_type(t, &params, args);
    let mut out = super::walk::map_class_decl(c, &mut map_expr, &mut map_block, &mut map_type);
    // A GENERIC method (`remap<U>: Box<U>`) references its own type
    // params, which the uniform substitution above leaves as
    // `Object("U")`. `subst_type` then mangles `Box<U>` into a phantom
    // `Object("Box<U>")` because `contains_type_var` doesn't see
    // `Object("U")` as a variable, and `monomorphize_methods` can't fix
    // a baked class name. Re-substitute each generic method from the
    // template with the class params AND the method's own params mapped
    // to `TypeVar` — that keeps `Box<U>` un-mangled (a `TypeVar` IS
    // caught), so method specialization later substitutes U → concrete
    // and mangles the real form.
    for (i, m) in c.methods.iter().enumerate() {
        if !m.type_params.is_empty() {
            out.methods[i] = specialize_generic_method(m, &params, args);
        }
    }
    for (i, m) in c.static_methods.iter().enumerate() {
        if !m.type_params.is_empty() {
            out.static_methods[i] = specialize_generic_method(m, &params, args);
        }
    }
    // map_class_decl preserves the generic class's type_params and
    // drops `is_pub`; specialize emits a concrete non-generic class
    // bound to `mangled` instead.
    out.name = mangled.into();
    out.type_params = Box::new([]);
    out
}

/// Substitute the class params into a still-generic method, mapping the
/// method's OWN type params to `TypeVar` so generic types referencing
/// them (`Box<U>`) stay un-mangled until method specialization fills U.
fn specialize_generic_method(m: &FnDecl, class_params: &[Symbol], class_args: &[Type]) -> FnDecl {
    let mut params: Vec<Symbol> = class_params.to_vec();
    let mut args: Vec<Type> = class_args.to_vec();
    for p in m.type_params.iter() {
        params.push(*p);
        args.push(Type::TypeVar(*p));
    }
    let mut out = specialize_fn(m, &params, &args);
    // specialize_fn strips type_params; this method is still generic.
    out.type_params = m.type_params.clone();
    out
}

pub(super) fn specialize_fn(f: &FnDecl, params: &[Symbol], args: &[Type]) -> FnDecl {
    let mut map_expr = |e: &Expr| subst_expr(e, params, args);
    let mut map_block = |b: &Block| subst_block(b, params, args);
    let mut map_type = |t: &Type| subst_type(t, params, args);
    let mut out = super::walk::map_fn_decl(f, &mut map_expr, &mut map_block, &mut map_type);
    // Specialized fns shed their generic params — the type checker
    // already pinned every TypeVar in `subst_type`.
    out.type_params = Box::new([]);
    out
}

pub(super) fn subst_block(b: &Block, params: &[Symbol], args: &[Type]) -> Block {
    super::walk::map_block_children(
        b,
        &mut |e| subst_expr(e, params, args),
        &mut |t: &Type| subst_type(t, params, args),
    )
}

pub(super) fn subst_expr(e: &Expr, params: &[Symbol], args: &[Type]) -> Expr {
    // A generic-enum ctor inside a specialized method body keeps its bare
    // `enum_name` ("Maybe" / "Result") unless re-mangled to the concrete
    // instantiation. Mirror `monomorphize_fns`: look the ctor's recorded
    // type args up by span, substitute this class's T -> concrete, and
    // mangle when fully concrete. (The args are still substituted via the
    // recursive walk below.)
    if let ExprKind::EnumCtor { enum_name, variant, args: ctor_args } = &e.kind {
        let new_name = remangle_generic_enum_ctor(enum_name, e.span, params, args);
        if new_name != *enum_name {
            let new_args = match ctor_args {
                ilang_ast::CtorArgs::Unit => ilang_ast::CtorArgs::Unit,
                ilang_ast::CtorArgs::Tuple(es) => ilang_ast::CtorArgs::Tuple(
                    es.iter().map(|x| subst_expr(x, params, args)).collect(),
                ),
                ilang_ast::CtorArgs::Struct(fs) => ilang_ast::CtorArgs::Struct(
                    fs.iter().map(|(n, x)| (n.clone(), subst_expr(x, params, args))).collect(),
                ),
            };
            return Expr {
                kind: ExprKind::EnumCtor { enum_name: new_name, variant: variant.clone(), args: new_args },
                span: e.span,
            };
        }
    }
    let kind = super::walk::map_expr_children(
        e,
        &mut |c| subst_expr(c, params, args),
        &mut |t: &Type| subst_type(t, params, args),
    );
    Expr { kind, span: e.span }
}

/// Re-mangle a generic-enum ctor's `enum_name` from its span-keyed
/// recorded type args, substituting this class's type params -> concrete.
/// Returns the bare name unchanged when the enum isn't generic, has no
/// recorded args, or the args don't fully concretize (a later
/// monomorphize round picks those up).
fn remangle_generic_enum_ctor(
    enum_name: &Symbol,
    span: Span,
    params: &[Symbol],
    args: &[Type],
) -> Symbol {
    if !is_generic_enum(enum_name) {
        return enum_name.clone();
    }
    ENUM_CTOR_TYPE_ARGS.with(|tbl| {
        let tbl = tbl.borrow();
        if let Some((tn, raw_args)) = tbl.get(&span) {
            if tn == enum_name {
                let concrete: Vec<Type> =
                    raw_args.iter().map(|t| subst_type(t, params, args)).collect();
                if !concrete.iter().any(contains_type_var) {
                    return super::enums::mangle_enum(enum_name.as_str(), &concrete);
                }
            }
        }
        enum_name.clone()
    })
}

pub(super) fn subst_type(t: &Type, params: &[Symbol], args: &[Type]) -> Type {
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
            // Built-in generic classes (`Map`) and generic enums
            // are NOT monomorphized into per-instantiation
            // copies — the JIT routes them through dedicated
            // codegen, so they stay as `Type::Generic`. Without
            // this guard, `class Bag<T> { items: Map<string, T[]>
            // }` substituted with `T = i32` would emit
            // `Object("Map<string, i32[]>")` and resolve_ty's
            // `Type::Object` arm would error with
            // "unknown type: Map<string, i32[]>".
            let gen_ty = Type::generic(g.base.clone(), new_args.clone());
            if is_generic_enum(&g.base) || is_builtin_generic_class(g.base.as_str()) {
                gen_ty
            } else if !contains_type_var(&gen_ty) {
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
        // A tuple type carries its element types structurally — recurse
        // so a type parameter nested in a tuple (`(T, T)` as a fn / method
        // signature or field type) is substituted. Without this arm the
        // tuple fell through to the catch-all `clone()` and its inner
        // `Object("T")` survived monomorphization, surfacing as
        // "unknown type: T" at lowering.
        Type::Tuple(elems) => {
            Type::Tuple(elems.iter().map(|e| subst_type(e, params, args)).collect())
        }
        Type::Fn(ft) => Type::func(
            ft.params.iter().map(|p| subst_type(p, params, args)).collect(),
            subst_type(&ft.ret, params, args),
        ),
        _ => t.clone(),
    }
}

pub(super) fn rewrite_item(item: &Item) -> Item {
    match item {
        Item::Class(c) => {
            let mut rewritten = super::walk::map_class_decl(
                c,
                &mut rewrite_expr,
                &mut rewrite_block,
                &mut rewrite_type,
            );
            // Preserve generic methods un-rewritten (like the generic-fn
            // skip below): their bodies reference their own type params
            // (`new Box<U>`), which the rewrite would mangle into a
            // phantom `Box<U>` class. `monomorphize_methods` specializes
            // them per call site, and a later fixed-point round mangles
            // the resulting concrete bodies.
            for (i, m) in c.methods.iter().enumerate() {
                if !m.type_params.is_empty() {
                    rewritten.methods[i] = m.clone();
                }
            }
            for (i, m) in c.static_methods.iter().enumerate() {
                if !m.type_params.is_empty() {
                    rewritten.static_methods[i] = m.clone();
                }
            }
            Item::Class(rewritten)
        }
        Item::Fn(f) => {
            // Skip rewrite for generic fns — their bodies reference
            // their own type params (as `Object("T")`), and
            // `rewrite_type` doesn't know to preserve those, so it
            // would mangle `new Box<T>(...)` to `Object("Box<T>")`
            // and surface a phantom "Box<T>" class. Let
            // `monomorphize_fns` handle them per call site (where
            // T is concrete), and re-running `monomorphize` after
            // that pass picks up any class / enum instantiations
            // in the specialized bodies.
            if !f.type_params.is_empty() {
                Item::Fn(f.clone())
            } else {
                Item::Fn(rewrite_fn(f))
            }
        }
        Item::Enum(e) => Item::Enum(e.clone()),
        Item::Use(u) => Item::Use(u.clone()),
        Item::Const(c) => Item::Const(c.clone()),
        Item::ExternC(b) => Item::ExternC(b.clone()),
        
        Item::Interface(i) => Item::Interface(i.clone()),
    }
}

pub(super) fn rewrite_fn(f: &FnDecl) -> FnDecl {
    super::walk::map_fn_decl(f, &mut rewrite_expr, &mut rewrite_block, &mut rewrite_type)
}

pub(super) fn rewrite_block(b: &Block) -> Block {
    super::walk::map_block_children(b, &mut rewrite_expr, &mut rewrite_type)
}

pub(super) fn rewrite_expr(e: &Expr) -> Expr {
    let kind = match &e.kind {
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
                    args: new_args.into(),
                    init_method: init_method.clone(),
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
                    args: new_args.into(),
                    init_method: init_method.clone(),
                }
            }
        }
        _ => super::walk::map_expr_children(e, &mut rewrite_expr, &mut rewrite_type),
    };
    Expr {
        kind,
        span: e.span,
    }
}

pub(super) fn is_generic_enum(name: &Symbol) -> bool {
    GENERIC_ENUM_NAMES.with(|set| set.borrow().contains(name))
}

/// Built-in generic classes whose `Type::Generic { base, args }` should
/// flow through to the JIT verbatim (NOT mangled into a synthetic
/// `Type::Object` like user generic classes). The JIT recognizes these
/// names and produces dedicated `JitTy` variants for them.
pub(super) fn is_builtin_generic_class(name: &str) -> bool {
    name == "Map" || name == "Set" || name == "Promise" || name == "ObjCBlock"
}

/// Collapse `Type::Generic { Box, [i64] }` to `Type::Object("Box<i64>")`
/// so the JIT pipeline (which only knows `Object`) routes to the
/// monomorphized class. Recurses through Array/Optional/Weak.
pub(super) fn rewrite_type(t: &Type) -> Type {
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
            if is_generic_enum(&g.base) || is_builtin_generic_class(g.base.as_str()) {
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
        // Recurse into tuple elements so a concrete generic
        // instantiation nested in a tuple (e.g. an `(Inner<i64>, i64)`
        // field type) gets its mangled Object name. Without this arm
        // the tuple was cloned unchanged and the still-`Generic`
        // `Inner<i64>` reached lowering as "unsupported in M1:
        // user-defined generic types".
        Type::Tuple(elems) => Type::Tuple(elems.iter().map(rewrite_type).collect()),
        Type::Fn(ft) => Type::func(
            ft.params.iter().map(rewrite_type).collect(),
            rewrite_type(&ft.ret),
        ),
        _ => t.clone(),
    }
}

pub(super) fn mangle_fn_name(name: &str, args: &[Type]) -> Symbol {
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

