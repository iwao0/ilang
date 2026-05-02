//! Module loader: resolve `use module` and `use module { name1, name2 }`
//! by reading `<module>.il` adjacent to the importing file, parsing
//! it, and merging its top-level items into the entry program.
//!
//! Loading is recursive (a module's `use` items are followed too),
//! with cycle detection. Items get mangled as follows:
//!   - whole-module import (`use utils`):
//!       - `fn foo` in utils.il      → `utils.foo` in the merged program
//!       - `class Counter`           → `utils.Counter`
//!       - `enum Color`              → `utils.Color`
//!     Callers reference them as `utils.foo(args)`, `new utils.Counter()`,
//!     `utils.Color.red`, etc. The normalize pass + parser already
//!     understand these dotted forms.
//!   - selective import (`use utils { foo, bar }`):
//!       - imported items keep their bare names (`foo`, `bar`).
//!     Anything in utils.il that isn't named in the selective list is
//!     not visible.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use ilang_ast::{
    Block, Expr, ExprKind, Item, MatchArm, Program, Stmt, StmtKind, Type, UseDecl,
};

use crate::ParseError;

#[derive(Debug)]
pub enum LoadError {
    ReadError {
        path: PathBuf,
        message: String,
    },
    LexError(String),
    ParseError(ParseError),
    CircularImport {
        chain: Vec<String>,
    },
    UnknownImport {
        module: String,
        name: String,
    },
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::ReadError { path, message } => {
                write!(f, "cannot read {path:?}: {message}")
            }
            LoadError::LexError(s) => write!(f, "lex error: {s}"),
            LoadError::ParseError(e) => write!(f, "parse error: {e}"),
            LoadError::CircularImport { chain } => {
                write!(f, "circular import: {}", chain.join(" → "))
            }
            LoadError::UnknownImport { module, name } => {
                write!(f, "module `{module}` doesn't export `{name}`")
            }
        }
    }
}

/// Load `entry`, recursively resolve every `use`, merge all items
/// into one Program, and return it. Removes all `Item::Use` from the
/// final program.
pub fn load_program(entry: &Path) -> Result<Program, LoadError> {
    let mut visiting: HashSet<PathBuf> = HashSet::new();
    let mut chain: Vec<String> = Vec::new();
    let mut loaded: HashMap<PathBuf, Program> = HashMap::new();
    let entry_dir = entry.parent().unwrap_or_else(|| Path::new(".")).to_path_buf();
    let entry_canon = canonicalize(entry)?;

    // Recursively load entry + its use'd modules. The entry's items
    // are kept under their original names; imported modules' items
    // get processed per the use's mode (whole/selective).
    load_recursive(&entry_canon, &entry_dir, &mut visiting, &mut chain, &mut loaded)?;

    let entry_prog = loaded.remove(&entry_canon).expect("entry just loaded");
    // Process the entry's use items into actual merged content.
    let mut merged = Program {
        items: Vec::new(),
        stmts: entry_prog.stmts,
        tail: entry_prog.tail,
    };
    let mut whole_imports: HashSet<String> = HashSet::new();
    for item in entry_prog.items {
        match item {
            Item::Use(u) => apply_use(
                u,
                &entry_canon,
                &mut loaded,
                &mut merged,
                &mut whole_imports,
            )?,
            other => merged.items.push(other),
        }
    }
    Ok(merged)
}

fn canonicalize(p: &Path) -> Result<PathBuf, LoadError> {
    p.canonicalize().map_err(|e| LoadError::ReadError {
        path: p.to_path_buf(),
        message: e.to_string(),
    })
}

fn load_recursive(
    file: &Path,
    base_dir: &Path,
    visiting: &mut HashSet<PathBuf>,
    chain: &mut Vec<String>,
    loaded: &mut HashMap<PathBuf, Program>,
) -> Result<(), LoadError> {
    if loaded.contains_key(file) {
        return Ok(());
    }
    if !visiting.insert(file.to_path_buf()) {
        chain.push(file.display().to_string());
        return Err(LoadError::CircularImport { chain: chain.clone() });
    }
    chain.push(file.display().to_string());
    let prog = parse_file(file)?;
    // Recurse into use items.
    let dir = file.parent().unwrap_or(base_dir).to_path_buf();
    for item in &prog.items {
        if let Item::Use(u) = item {
            let path = dir.join(format!("{}.il", u.module));
            let canon = canonicalize(&path)?;
            load_recursive(&canon, &dir, visiting, chain, loaded)?;
        }
    }
    loaded.insert(file.to_path_buf(), prog);
    visiting.remove(file);
    chain.pop();
    Ok(())
}

fn parse_file(file: &Path) -> Result<Program, LoadError> {
    let src = std::fs::read_to_string(file).map_err(|e| LoadError::ReadError {
        path: file.to_path_buf(),
        message: e.to_string(),
    })?;
    let toks = ilang_lexer::tokenize(&src)
        .map_err(|e| LoadError::LexError(e.to_string()))?;
    crate::parse(&toks).map_err(LoadError::ParseError)
}

fn apply_use(
    u: UseDecl,
    importer_canon: &Path,
    loaded: &mut HashMap<PathBuf, Program>,
    merged: &mut Program,
    _whole_imports: &mut HashSet<String>,
) -> Result<(), LoadError> {
    let importer_dir = importer_canon
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    let path = importer_dir.join(format!("{}.il", u.module));
    let canon = canonicalize(&path)?;
    let mut module_prog = loaded
        .remove(&canon)
        .expect("loaded before via load_recursive");
    // Recursively expand the module's own use items first, into the
    // module_prog's namespace. (Nested whole-module imports inside a
    // module would carry their full prefix; for MVP we just inline
    // the items as-is, treating each module as a flat list.)
    let mut nested_uses = Vec::new();
    let mut local_items = Vec::new();
    for item in module_prog.items {
        match item {
            Item::Use(nu) => nested_uses.push(nu),
            other => local_items.push(other),
        }
    }
    module_prog.items = local_items;
    // Process nested uses. They expand into the same merged program
    // as the entry's. Items nested-imported keep their natural names
    // (i.e., a module's nested `use foo { bar }` makes `bar` callable
    // from within that module — which after merging becomes `bar` in
    // the merged program). This is a simple model; not module-private.
    for nu in nested_uses {
        apply_use(nu, &canon, loaded, merged, _whole_imports)?;
    }

    match u.selective {
        None => {
            // Whole-module import: prefix every item with `module.`.
            for item in module_prog.items {
                merged.items.push(prefix_item(item, &u.module));
            }
        }
        Some(names) => {
            // Selective import: pull in just the listed items, keeping
            // their bare names.
            let selected: HashSet<&str> = names.iter().map(|s| s.as_str()).collect();
            let mut found: HashSet<String> = HashSet::new();
            for item in module_prog.items {
                let item_name = item_name_of(&item);
                if let Some(name) = item_name {
                    if selected.contains(name.as_str()) {
                        found.insert(name);
                        merged.items.push(item);
                    }
                }
            }
            for name in &names {
                if !found.contains(name) {
                    return Err(LoadError::UnknownImport {
                        module: u.module.clone(),
                        name: name.clone(),
                    });
                }
            }
        }
    }
    Ok(())
}

fn item_name_of(item: &Item) -> Option<String> {
    match item {
        Item::Fn(f) => Some(f.name.clone()),
        Item::Class(c) => Some(c.name.clone()),
        Item::Enum(e) => Some(e.name.clone()),
        Item::Use(_) => None,
    }
}

fn prefix_item(item: Item, prefix: &str) -> Item {
    match item {
        Item::Fn(mut f) => {
            f.name = format!("{prefix}.{}", f.name);
            f.body = prefix_block_calls(f.body, prefix);
            Item::Fn(f)
        }
        Item::Class(mut c) => {
            c.name = format!("{prefix}.{}", c.name);
            for m in &mut c.methods {
                let body = std::mem::replace(
                    &mut m.body,
                    Block { stmts: Vec::new(), tail: None },
                );
                m.body = prefix_block_calls(body, prefix);
                m.params = m
                    .params
                    .iter()
                    .map(|p| ilang_ast::Param {
                        name: p.name.clone(),
                        ty: prefix_type(&p.ty, prefix),
                        span: p.span,
                    })
                    .collect();
                m.ret = m.ret.as_ref().map(|t| prefix_type(t, prefix));
            }
            for f in &mut c.fields {
                f.ty = prefix_type(&f.ty, prefix);
            }
            Item::Class(c)
        }
        Item::Enum(mut e) => {
            e.name = format!("{prefix}.{}", e.name);
            for v in &mut e.variants {
                v.payload = match std::mem::replace(&mut v.payload, ilang_ast::VariantPayload::Unit) {
                    ilang_ast::VariantPayload::Unit => ilang_ast::VariantPayload::Unit,
                    ilang_ast::VariantPayload::Tuple(tys) => ilang_ast::VariantPayload::Tuple(
                        tys.into_iter().map(|t| prefix_type(&t, prefix)).collect(),
                    ),
                    ilang_ast::VariantPayload::Struct(fs) => {
                        ilang_ast::VariantPayload::Struct(
                            fs.into_iter()
                                .map(|mut fd| {
                                    fd.ty = prefix_type(&fd.ty, prefix);
                                    fd
                                })
                                .collect(),
                        )
                    }
                };
            }
            Item::Enum(e)
        }
        Item::Use(u) => Item::Use(u),
    }
}

/// Within a prefixed item, references to other top-level items from
/// the same module should also resolve to their prefixed names. We
/// don't have full symbol info here, so we use a heuristic: rewrite
/// bare `Call { callee: name }` and bare `Type::Object(name)` /
/// `Type::Generic { base, .. }` only when the name is *not* already
/// in the prefixed form. This is intentionally conservative — for
/// MVP we only rewrite Calls. Other forms (class refs from inside)
/// stay bare and can be cross-resolved by the type checker.
fn prefix_block_calls(b: Block, prefix: &str) -> Block {
    Block {
        stmts: b.stmts.into_iter().map(|s| prefix_stmt(s, prefix)).collect(),
        tail: b.tail.map(|e| Box::new(prefix_expr(*e, prefix))),
    }
}

fn prefix_stmt(s: Stmt, prefix: &str) -> Stmt {
    let kind = match s.kind {
        StmtKind::Let { name, ty, value } => StmtKind::Let {
            name,
            ty: ty.map(|t| prefix_type(&t, prefix)),
            value: prefix_expr(value, prefix),
        },
        StmtKind::Expr(e) => StmtKind::Expr(prefix_expr(e, prefix)),
    };
    Stmt { kind, span: s.span }
}

fn prefix_expr(e: Expr, prefix: &str) -> Expr {
    let span = e.span;
    let kind = match e.kind {
        // Function calls within a module: a bare `helper(x)` could
        // refer to the module's own `helper`. We rewrite these to the
        // prefixed form. Built-ins (console.log not a Call here) and
        // local fn-value calls are unaffected by this since they go
        // through other AST shapes.
        ExprKind::Call { callee, args } => ExprKind::Call {
            callee: format!("{prefix}.{}", callee),
            args: args.into_iter().map(|a| prefix_expr(a, prefix)).collect(),
        },
        ExprKind::New {
            class,
            type_args,
            args,
        } => ExprKind::New {
            class: format!("{prefix}.{}", class),
            type_args: type_args.into_iter().map(|t| prefix_type(&t, prefix)).collect(),
            args: args.into_iter().map(|a| prefix_expr(a, prefix)).collect(),
        },
        ExprKind::EnumCtor {
            enum_name,
            variant,
            args,
        } => ExprKind::EnumCtor {
            enum_name: format!("{prefix}.{}", enum_name),
            variant,
            args: match args {
                ilang_ast::CtorArgs::Unit => ilang_ast::CtorArgs::Unit,
                ilang_ast::CtorArgs::Tuple(es) => ilang_ast::CtorArgs::Tuple(
                    es.into_iter().map(|e| prefix_expr(e, prefix)).collect(),
                ),
                ilang_ast::CtorArgs::Struct(fs) => ilang_ast::CtorArgs::Struct(
                    fs.into_iter()
                        .map(|(n, e)| (n, prefix_expr(e, prefix)))
                        .collect(),
                ),
            },
        },
        ExprKind::Cast { expr, ty } => ExprKind::Cast {
            expr: Box::new(prefix_expr(*expr, prefix)),
            ty: prefix_type(&ty, prefix),
        },
        ExprKind::FnExpr { params, ret, body } => ExprKind::FnExpr {
            params: params
                .into_iter()
                .map(|p| ilang_ast::Param {
                    name: p.name,
                    ty: prefix_type(&p.ty, prefix),
                    span: p.span,
                })
                .collect(),
            ret: ret.map(|t| prefix_type(&t, prefix)),
            body: prefix_block_calls(body, prefix),
        },
        // Recurse mechanically through everything else.
        ExprKind::Unary { op, expr } => ExprKind::Unary {
            op,
            expr: Box::new(prefix_expr(*expr, prefix)),
        },
        ExprKind::Binary { op, lhs, rhs } => ExprKind::Binary {
            op,
            lhs: Box::new(prefix_expr(*lhs, prefix)),
            rhs: Box::new(prefix_expr(*rhs, prefix)),
        },
        ExprKind::Logical { op, lhs, rhs } => ExprKind::Logical {
            op,
            lhs: Box::new(prefix_expr(*lhs, prefix)),
            rhs: Box::new(prefix_expr(*rhs, prefix)),
        },
        ExprKind::Field { obj, name } => ExprKind::Field {
            obj: Box::new(prefix_expr(*obj, prefix)),
            name,
        },
        ExprKind::MethodCall { obj, method, args } => ExprKind::MethodCall {
            obj: Box::new(prefix_expr(*obj, prefix)),
            method,
            args: args.into_iter().map(|a| prefix_expr(a, prefix)).collect(),
        },
        ExprKind::Block(b) => ExprKind::Block(prefix_block_calls(b, prefix)),
        ExprKind::If {
            cond,
            then_branch,
            else_branch,
        } => ExprKind::If {
            cond: Box::new(prefix_expr(*cond, prefix)),
            then_branch: prefix_block_calls(then_branch, prefix),
            else_branch: else_branch.map(|e| Box::new(prefix_expr(*e, prefix))),
        },
        ExprKind::IfLet {
            name,
            expr,
            then_branch,
            else_branch,
        } => ExprKind::IfLet {
            name,
            expr: Box::new(prefix_expr(*expr, prefix)),
            then_branch: prefix_block_calls(then_branch, prefix),
            else_branch: else_branch.map(|e| Box::new(prefix_expr(*e, prefix))),
        },
        ExprKind::While { cond, body } => ExprKind::While {
            cond: Box::new(prefix_expr(*cond, prefix)),
            body: prefix_block_calls(body, prefix),
        },
        ExprKind::Loop { body } => ExprKind::Loop {
            body: prefix_block_calls(body, prefix),
        },
        ExprKind::ForIn { var, iter, body } => ExprKind::ForIn {
            var,
            iter: Box::new(prefix_expr(*iter, prefix)),
            body: prefix_block_calls(body, prefix),
        },
        ExprKind::Return(opt) => ExprKind::Return(opt.map(|e| Box::new(prefix_expr(*e, prefix)))),
        ExprKind::Assign { target, value } => ExprKind::Assign {
            target,
            value: Box::new(prefix_expr(*value, prefix)),
        },
        ExprKind::AssignField { obj, field, value } => ExprKind::AssignField {
            obj: Box::new(prefix_expr(*obj, prefix)),
            field,
            value: Box::new(prefix_expr(*value, prefix)),
        },
        ExprKind::AssignIndex { obj, index, value } => ExprKind::AssignIndex {
            obj: Box::new(prefix_expr(*obj, prefix)),
            index: Box::new(prefix_expr(*index, prefix)),
            value: Box::new(prefix_expr(*value, prefix)),
        },
        ExprKind::Array(items) => {
            ExprKind::Array(items.into_iter().map(|e| prefix_expr(e, prefix)).collect())
        }
        ExprKind::MapLit(entries) => ExprKind::MapLit(
            entries
                .into_iter()
                .map(|(k, v)| (prefix_expr(k, prefix), prefix_expr(v, prefix)))
                .collect(),
        ),
        ExprKind::Index { obj, index } => ExprKind::Index {
            obj: Box::new(prefix_expr(*obj, prefix)),
            index: Box::new(prefix_expr(*index, prefix)),
        },
        ExprKind::Some(inner) => ExprKind::Some(Box::new(prefix_expr(*inner, prefix))),
        ExprKind::Match { scrutinee, arms } => ExprKind::Match {
            scrutinee: Box::new(prefix_expr(*scrutinee, prefix)),
            arms: arms
                .into_iter()
                .map(|arm: MatchArm| MatchArm {
                    pattern: arm.pattern,
                    body: prefix_expr(arm.body, prefix),
                    span: arm.span,
                })
                .collect(),
        },
        // Trivial nodes pass through.
        other @ (ExprKind::Int(_)
        | ExprKind::Float(_)
        | ExprKind::Bool(_)
        | ExprKind::Str(_)
        | ExprKind::Var(_)
        | ExprKind::This
        | ExprKind::None
        | ExprKind::Break
        | ExprKind::Continue) => other,
    };
    Expr { kind, span }
}

fn prefix_type(t: &Type, prefix: &str) -> Type {
    match t {
        Type::Object(name) if !name.contains('.') && !is_builtin_type(name) => {
            Type::Object(format!("{prefix}.{name}"))
        }
        Type::Array { elem, fixed } => Type::Array {
            elem: Box::new(prefix_type(elem, prefix)),
            fixed: *fixed,
        },
        Type::Optional(inner) => Type::Optional(Box::new(prefix_type(inner, prefix))),
        Type::Weak(inner) => Type::Weak(Box::new(prefix_type(inner, prefix))),
        Type::Generic { base, args } => Type::Generic {
            base: if !base.contains('.') && !is_builtin_type(base) {
                format!("{prefix}.{base}")
            } else {
                base.clone()
            },
            args: args.iter().map(|a| prefix_type(a, prefix)).collect(),
        },
        Type::Fn { params, ret } => Type::Fn {
            params: params.iter().map(|p| prefix_type(p, prefix)).collect(),
            ret: Box::new(prefix_type(ret, prefix)),
        },
        _ => t.clone(),
    }
}

fn is_builtin_type(name: &str) -> bool {
    // Built-in classes/enums that should never get prefixed even
    // when referenced inside a module body.
    matches!(name, "Console" | "Map" | "Result")
}
