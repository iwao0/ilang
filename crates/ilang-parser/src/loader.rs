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
    BinOp, Block, Expr, ExprKind, Item, LogicalOp, MatchArm, Program, Span, Stmt, StmtKind,
    Type, UnOp, UseDecl,
};

use crate::ParseError;

/// Modules whose source is shipped inside the compiler. `use math`
/// resolves here before consulting the filesystem.
fn builtin_module_source(name: &str) -> Option<&'static str> {
    match name {
        "math" => Some(include_str!("stdlib/math.il")),
        "test" => Some(include_str!("stdlib/test.il")),
        "os" => Some(include_str!("stdlib/os.il")),
        _ => None,
    }
}

/// A path-shaped key for built-in modules so the rest of the loader
/// can treat them uniformly with on-disk files.
fn builtin_path(name: &str) -> PathBuf {
    PathBuf::from(format!("<builtin>/{name}.il"))
}

fn is_builtin_path(p: &Path) -> Option<&str> {
    let s = p.to_str()?;
    s.strip_prefix("<builtin>/")
        .and_then(|rest| rest.strip_suffix(".il"))
}

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
    /// `const X = expr` where `expr` couldn't be folded to a literal.
    /// Carries a human-readable reason and the offending span.
    BadConst {
        name: String,
        reason: String,
        span: ilang_ast::Span,
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
            LoadError::BadConst { name, reason, span } => {
                write!(f, "{span}: `const {name}` is not a constant expression: {reason}")
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
                None,
                &entry_canon,
                &mut loaded,
                &mut merged,
                &mut whole_imports,
            )?,
            other => merged.items.push(other),
        }
    }
    // Re-normalize the merged program. Each file was normalized in
    // isolation, so an entry-file reference like `lib.Color.green`
    // collapses to `Field(Var("lib.Color"), "green")` — at parse time
    // `lib.Color` wasn't a known enum (it lives in another file). Now
    // that the loader has merged the prefixed `lib.Color` enum decl
    // into `merged.items`, a second normalize pass picks it up and
    // converts the field-access into an `EnumCtor`.
    let merged = crate::normalize::normalize(merged);
    // Inline `const` declarations: collect every Item::Const in the
    // merged Program, then walk all expressions replacing
    // `Var(const_name)` with the literal value. Item::Const entries
    // are removed afterwards. Downstream stages (type checker /
    // interpreter / JIT) never see consts.
    inline_constants(merged)
}

fn canonicalize(p: &Path) -> Result<PathBuf, LoadError> {
    p.canonicalize().map_err(|e| LoadError::ReadError {
        path: p.to_path_buf(),
        message: e.to_string(),
    })
}

/// Resolve a `use module` to either an on-disk canonicalized path
/// or a virtual `<builtin>/module.il` path for shipped stdlib modules.
fn resolve_module(module: &str, dir: &Path) -> Result<PathBuf, LoadError> {
    if builtin_module_source(module).is_some() {
        return Ok(builtin_path(module));
    }
    let path = dir.join(format!("{module}.il"));
    canonicalize(&path)
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
    // Recurse into use items. Built-in modules don't have a dir, so
    // pass through the importer's dir for any nested non-builtin
    // resolutions (built-ins themselves don't import other modules
    // for now, but the path stays correct if they ever do).
    let dir = file.parent().unwrap_or(base_dir).to_path_buf();
    for item in &prog.items {
        if let Item::Use(u) = item {
            let canon = resolve_module(&u.module, &dir)?;
            load_recursive(&canon, &dir, visiting, chain, loaded)?;
        }
    }
    loaded.insert(file.to_path_buf(), prog);
    visiting.remove(file);
    chain.pop();
    Ok(())
}

fn parse_file(file: &Path) -> Result<Program, LoadError> {
    let src = if let Some(name) = is_builtin_path(file) {
        builtin_module_source(name)
            .expect("builtin path checked")
            .to_string()
    } else {
        std::fs::read_to_string(file).map_err(|e| LoadError::ReadError {
            path: file.to_path_buf(),
            message: e.to_string(),
        })?
    };
    let toks = ilang_lexer::tokenize(&src)
        .map_err(|e| LoadError::LexError(e.to_string()))?;
    crate::parse(&toks).map_err(LoadError::ParseError)
}

fn apply_use(
    u: UseDecl,
    // When `Some(p)`, items from `u`'s module merge under prefix `p`
    // instead of `u.module`. Used by `@export use M` so M's items
    // appear under the re-exporting module's namespace. `None` at
    // the entry-point and on regular nested uses.
    prefix_override: Option<&str>,
    importer_canon: &Path,
    loaded: &mut HashMap<PathBuf, Program>,
    merged: &mut Program,
    _whole_imports: &mut HashSet<String>,
) -> Result<(), LoadError> {
    let importer_dir = importer_canon
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    let canon = resolve_module(&u.module, &importer_dir)?;
    // Clone instead of remove — the same module may legitimately be
    // applied multiple times (e.g. once via @export to publish under
    // an umbrella prefix, and once directly so a sibling module that
    // `use`s it sees the items under the original prefix). Each
    // application targets a distinct effective prefix, so the
    // resulting items don't shadow each other.
    let mut module_prog = loaded
        .get(&canon)
        .cloned()
        .expect("loaded before via load_recursive");
    let effective_prefix: String = prefix_override
        .map(str::to_string)
        .unwrap_or_else(|| u.module.clone());
    // Recursively expand the module's own use items first, into the
    // module_prog's namespace. `@export use N` propagates the
    // current module's effective prefix to N so its items also land
    // under the re-exporting namespace.
    let mut nested_uses = Vec::new();
    let mut local_items = Vec::new();
    for item in module_prog.items {
        match item {
            Item::Use(nu) => nested_uses.push(nu),
            other => local_items.push(other),
        }
    }
    module_prog.items = local_items;
    for nu in nested_uses {
        let nested_override: Option<&str> = if nu.re_export {
            Some(effective_prefix.as_str())
        } else {
            None
        };
        apply_use(nu, nested_override, &canon, loaded, merged, _whole_imports)?;
    }

    match u.selective {
        None => {
            // Whole-module import: prefix every item with the
            // effective prefix (the override when re-exporting,
            // otherwise the module's own name).
            for item in module_prog.items {
                merged.items.push(prefix_item(item, &effective_prefix));
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
        Item::Const(c) => Some(c.name.clone()),
        Item::ExternStatic(s) => Some(s.name.clone()),
        Item::ExternC(_) => None,
        Item::Use(_) => None,
    }
}

fn prefix_class_decl(c: &mut ilang_ast::ClassDecl, prefix: &str) {
    c.name = format!("{prefix}.{}", c.name);
    for m in c.methods.iter_mut().chain(c.static_methods.iter_mut()) {
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
                default: p.default.clone(),
            })
            .collect();
        m.ret = m.ret.as_ref().map(|t| prefix_type(t, prefix));
    }
    for f in &mut c.fields {
        f.ty = prefix_type(&f.ty, prefix);
    }
    for prop in &mut c.properties {
        prop.ty = prefix_type(&prop.ty, prefix);
        if let Some(g) = prop.getter.as_mut() {
            let body = std::mem::replace(
                &mut g.body,
                Block { stmts: Vec::new(), tail: None },
            );
            g.body = prefix_block_calls(body, prefix);
            g.ret = g.ret.as_ref().map(|t| prefix_type(t, prefix));
        }
        if let Some(s) = prop.setter.as_mut() {
            let body = std::mem::replace(
                &mut s.body,
                Block { stmts: Vec::new(), tail: None },
            );
            s.body = prefix_block_calls(body, prefix);
            s.params = s
                .params
                .iter()
                .map(|p| ilang_ast::Param {
                    name: p.name.clone(),
                    ty: prefix_type(&p.ty, prefix),
                    span: p.span,
                    default: p.default.clone(),
                })
                .collect();
        }
    }
}

fn prefix_item(item: Item, prefix: &str) -> Item {
    match item {
        Item::Fn(mut f) => {
            f.name = format!("{prefix}.{}", f.name);
            f.params = f
                .params
                .iter()
                .map(|p| ilang_ast::Param {
                    name: p.name.clone(),
                    ty: prefix_type(&p.ty, prefix),
                    span: p.span,
                    default: p.default.clone(),
                })
                .collect();
            f.ret = f.ret.as_ref().map(|t| prefix_type(t, prefix));
            f.body = prefix_block_calls(f.body, prefix);
            Item::Fn(f)
        }
        Item::Class(mut c) => {
            prefix_class_decl(&mut c, prefix);
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
        Item::Const(mut c) => {
            c.name = format!("{prefix}.{}", c.name);
            // The value is a literal — no inner refs to rewrite.
            Item::Const(c)
        }
        Item::ExternStatic(mut s) => {
            // Module-prefix the ilang-side name (so `use mymod` lets
            // callers write `mymod.errno`). The C symbol resolved
            // via dlsym uses the original unprefixed name, so we
            // stash that for the dlsym pass to find.
            s.name = format!("{prefix}.{}", s.name);
            Item::ExternStatic(s)
        }
        Item::ExternC(mut b) => {
            // Prefix the ilang-side names of the block's items so
            // callers can write `module.fn` etc. For library-form
            // (@lib) FnDecls, preserve the original C symbol name in
            // `c_symbol` so dlsym still finds it after the ilang name
            // has been rewritten to the prefixed form. Host-form fns
            // (no @lib) keep using the prefixed name as the symbol —
            // host registration code uses the prefixed name to match.
            //
            // Field / param / ret / static types also get prefixed so
            // intra-block references (e.g. `*SDL_Window` returning
            // from a fn that declared the struct) keep resolving.
            for inner in &mut b.items {
                match inner {
                    ilang_ast::ExternCItem::Struct { name, fields, .. }
                    | ilang_ast::ExternCItem::Union { name, fields, .. } => {
                        *name = format!("{prefix}.{name}");
                        for f in fields {
                            f.ty = prefix_type(&f.ty, prefix);
                        }
                    }
                    ilang_ast::ExternCItem::Static { name, ty, .. } => {
                        *name = format!("{prefix}.{name}");
                        *ty = prefix_type(ty, prefix);
                    }
                    ilang_ast::ExternCItem::FnDecl {
                        name, libs, c_symbol, params, ret, ..
                    } => {
                        if !libs.is_empty() && c_symbol.is_none() {
                            *c_symbol = Some(name.clone());
                        }
                        *name = format!("{prefix}.{name}");
                        for p in params.iter_mut() {
                            p.ty = prefix_type(&p.ty, prefix);
                        }
                        if let Some(rt) = ret.as_mut() {
                            *rt = prefix_type(rt, prefix);
                        }
                    }
                    ilang_ast::ExternCItem::FnDef(f) => {
                        f.name = format!("{prefix}.{}", f.name);
                        for p in f.params.iter_mut() {
                            p.ty = prefix_type(&p.ty, prefix);
                        }
                        if let Some(rt) = f.ret.as_mut() {
                            *rt = prefix_type(rt, prefix);
                        }
                        let body = std::mem::replace(
                            &mut f.body,
                            Block { stmts: Vec::new(), tail: None },
                        );
                        f.body = prefix_block_calls(body, prefix);
                    }
                    ilang_ast::ExternCItem::Class(c) => {
                        prefix_class_decl(c, prefix);
                    }
                }
            }
            Item::ExternC(b)
        }
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
        // prefixed form. Built-ins (FFI marshalling helpers,
        // already-qualified `module.fn` shapes that get parsed as
        // MethodCall, etc.) are skipped.
        ExprKind::Call { callee, args } => {
            // Skip rewriting when:
            //   - the callee is a built-in (FFI helper, console.log, …)
            //   - the callee is already module-qualified (contains a
            //     `.`) — that means an earlier normalize pass already
            //     turned `module.fn(args)` into a `Call`, and adding
            //     the current module's prefix again would produce
            //     `current.module.fn` and break resolution.
            let new_callee = if is_builtin_callee(&callee) || callee.contains('.') {
                callee
            } else {
                format!("{prefix}.{}", callee)
            };
            ExprKind::Call {
                callee: new_callee,
                args: args.into_iter().map(|a| prefix_expr(a, prefix)).collect(),
            }
        }
        ExprKind::New { class, type_args, args, init_method } => ExprKind::New {
            class: format!("{prefix}.{}", class),
            type_args: type_args.into_iter().map(|t| prefix_type(&t, prefix)).collect(),
            args: args.into_iter().map(|a| prefix_expr(a, prefix)).collect(),
            init_method,
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
                    default: p.default,
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
        ExprKind::Range { start, end, inclusive } => ExprKind::Range {
            start: Box::new(prefix_expr(*start, prefix)),
            end: Box::new(prefix_expr(*end, prefix)),
            inclusive,
        },
        ExprKind::Closure { fn_name, captures } => {
            ExprKind::Closure { fn_name, captures }
        }
        ExprKind::SuperCall { method, args } => ExprKind::SuperCall {
            method,
            args: args.into_iter().map(|a| prefix_expr(a, prefix)).collect(),
        },
        ExprKind::Return(opt) => ExprKind::Return(opt.map(|e| Box::new(prefix_expr(*e, prefix)))),
        ExprKind::Break(opt) => ExprKind::Break(opt.map(|e| Box::new(prefix_expr(*e, prefix)))),
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
        ExprKind::Tuple(items) => {
            ExprKind::Tuple(items.into_iter().map(|e| prefix_expr(e, prefix)).collect())
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
        | ExprKind::Continue) => other,
        // Struct literals are desugared by `normalize` before the
        // loader walks anything; reaching this arm means a module
        // skipped that pass.
        ExprKind::StructLit { class, fields } => ExprKind::StructLit {
            class,
            fields: fields
                .into_iter()
                .map(|(n, e)| (n, prefix_expr(e, prefix)))
                .collect(),
        },
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
        Type::RawPtr { is_const, inner } => Type::RawPtr {
            is_const: *is_const,
            inner: Box::new(prefix_type(inner, prefix)),
        },
        _ => t.clone(),
    }
}

/// Names that should never get module-prefixed at Call sites — the
/// FFI marshalling helpers shipped by the type checker (mirrors the
/// `FFI_HELPERS` list in `ilang-types`).
fn is_builtin_callee(name: &str) -> bool {
    matches!(
        name,
        "stringFromCstr"
            | "cstrFromString"
            | "freeCstr"
            | "bytesFromBuffer"
            | "arrayFromCArray"
            | "cstrArrayToStrings"
            | "errnoCheck"
            | "errnoCheckI64"
    )
}

fn is_builtin_type(name: &str) -> bool {
    // Built-in classes/enums that should never get prefixed even
    // when referenced inside a module body.
    matches!(name, "Console" | "Map" | "Result")
}

// ─── const substitution ────────────────────────────────────────────────

/// Walk the merged program collecting every `Item::Const`, then
/// replace `Var(const_name)` references everywhere with the literal
/// RHS. Removes the Item::Const entries from the output. Consts are
/// allowed to reference module-prefixed names (e.g. `math.pi` after
/// the loader's mangling) since the substitution happens by exact
/// name match.
fn inline_constants(prog: Program) -> Result<Program, LoadError> {
    // Walk items in declaration order and fold each `const`'s RHS to a
    // literal, using already-folded consts as known bindings. The
    // result becomes the substitution value for every `Var(name)`
    // reference in the rest of the program.
    let mut consts: HashMap<String, Expr> = HashMap::new();
    // Annotated types — looked up at substitution time so each
    // substituted reference carries the const's declared type via
    // a wrapping `Cast`. Unannotated consts (`const N = 5`) leave
    // their entry absent and substitute as the bare literal (i64
    // for ints, the natural literal type otherwise).
    let mut const_types: HashMap<String, ilang_ast::Type> = HashMap::new();
    let mut items_no_const: Vec<Item> = Vec::new();
    for item in prog.items {
        match item {
            Item::Const(c) => {
                let folded = fold_const_expr(&c.value, &consts).map_err(|reason| {
                    LoadError::BadConst {
                        name: c.name.clone(),
                        reason,
                        span: c.value.span,
                    }
                })?;
                if let Some(ty) = &c.ty {
                    // Don't wrap string / bool literals — those have
                    // a single natural type and casting them would
                    // be invalid. For numeric types, the wrap kicks
                    // in at substitution time so call sites get
                    // `<value> as <ty>` automatically.
                    let wrappable = matches!(
                        &folded.kind,
                        ExprKind::Int(_) | ExprKind::Float(_)
                    );
                    if wrappable {
                        const_types.insert(c.name.clone(), ty.clone());
                    }
                }
                consts.insert(c.name, folded);
            }
            other => items_no_const.push(other),
        }
    }
    // Fold each class's static-field initializers using the same
    // rules. The folded literal sits on the AST until the
    // interpreter / JIT pulls it for storage init.
    for item in items_no_const.iter_mut() {
        if let Item::Class(c) = item {
            for sf in c.static_fields.iter_mut() {
                let folded = fold_const_expr(&sf.value, &consts).map_err(|reason| {
                    LoadError::BadConst {
                        name: format!("{}.{}", c.name, sf.name),
                        reason,
                        span: sf.value.span,
                    }
                })?;
                sf.value = folded;
            }
        }
    }
    if consts.is_empty() {
        return Ok(Program {
            items: items_no_const,
            stmts: prog.stmts,
            tail: prog.tail,
        });
    }
    let ctx = SubstCtx { consts: &consts, types: &const_types };
    Ok(Program {
        items: items_no_const
            .into_iter()
            .map(|i| subst_const_item(i, &ctx))
            .collect(),
        stmts: prog
            .stmts
            .into_iter()
            .map(|s| subst_const_stmt(s, &ctx))
            .collect(),
        tail: prog.tail.map(|e| subst_const_expr(e, &ctx)),
    })
}

struct SubstCtx<'a> {
    consts: &'a HashMap<String, Expr>,
    types: &'a HashMap<String, ilang_ast::Type>,
}

/// Constant folder. Reduces `e` to a literal `Expr` (Int / Float /
/// Bool / Str), or returns a human-readable failure reason.
/// Supported: literals, references to other consts, unary `- ! ~`,
/// binary arithmetic / comparison / bitwise / logical, `as` casts
/// between numeric types, string `+` (concat) and `==` / `!=`.
fn fold_const_expr(e: &Expr, consts: &HashMap<String, Expr>) -> Result<Expr, String> {
    let span = e.span;
    let lit = |k: ExprKind| Expr { kind: k, span };
    match &e.kind {
        ExprKind::Int(_) | ExprKind::Float(_) | ExprKind::Bool(_) | ExprKind::Str(_) => {
            Ok(e.clone())
        }
        ExprKind::Var(name) => consts
            .get(name)
            .cloned()
            .ok_or_else(|| format!("unknown identifier `{name}` in const expression")),
        ExprKind::Unary { op, expr } => {
            let v = fold_const_expr(expr, consts)?;
            match (op, &v.kind) {
                (UnOp::Neg, ExprKind::Int(n)) => Ok(lit(ExprKind::Int(-n))),
                (UnOp::Neg, ExprKind::Float(x)) => Ok(lit(ExprKind::Float(-x))),
                (UnOp::Not, ExprKind::Bool(b)) => Ok(lit(ExprKind::Bool(!b))),
                (UnOp::BitNot, ExprKind::Int(n)) => Ok(lit(ExprKind::Int(!n))),
                _ => Err(format!("unary {op:?} not supported in const expression")),
            }
        }
        ExprKind::Binary { op, lhs, rhs } => {
            let l = fold_const_expr(lhs, consts)?;
            let r = fold_const_expr(rhs, consts)?;
            fold_binary(*op, &l, &r, span)
        }
        ExprKind::Logical { op, lhs, rhs } => {
            let l = fold_const_expr(lhs, consts)?;
            let lb = match l.kind {
                ExprKind::Bool(b) => b,
                _ => return Err("logical operands must be bool".into()),
            };
            // Short-circuit, like the runtime would.
            match op {
                LogicalOp::And if !lb => Ok(lit(ExprKind::Bool(false))),
                LogicalOp::Or if lb => Ok(lit(ExprKind::Bool(true))),
                _ => {
                    let r = fold_const_expr(rhs, consts)?;
                    match r.kind {
                        ExprKind::Bool(b) => Ok(lit(ExprKind::Bool(b))),
                        _ => Err("logical operands must be bool".into()),
                    }
                }
            }
        }
        ExprKind::Cast { expr, ty } => {
            let v = fold_const_expr(expr, consts)?;
            cast_const(&v, ty, span)
        }
        // Anything else (calls, fields, control flow, ...) is not a
        // constant expression. Be specific in the error so the user
        // knows what to fix.
        other => Err(format!(
            "expression {} is not allowed in `const`",
            describe_expr_kind(other)
        )),
    }
}

fn fold_binary(op: BinOp, l: &Expr, r: &Expr, span: Span) -> Result<Expr, String> {
    let lit = |k: ExprKind| Expr { kind: k, span };
    use ExprKind::*;
    match (&l.kind, &r.kind) {
        (Int(a), Int(b)) => Ok(lit(match op {
            BinOp::Add => Int(a.wrapping_add(*b)),
            BinOp::Sub => Int(a.wrapping_sub(*b)),
            BinOp::Mul => Int(a.wrapping_mul(*b)),
            BinOp::Div => {
                if *b == 0 {
                    return Err("division by zero in const expression".into());
                }
                Int(a / b)
            }
            BinOp::Rem => {
                if *b == 0 {
                    return Err("modulo by zero in const expression".into());
                }
                Int(a % b)
            }
            BinOp::BitAnd => Int(a & b),
            BinOp::BitOr => Int(a | b),
            BinOp::BitXor => Int(a ^ b),
            BinOp::Shl => Int(a.wrapping_shl(*b as u32)),
            BinOp::Shr => Int(a.wrapping_shr(*b as u32)),
            BinOp::Eq => Bool(a == b),
            BinOp::Ne => Bool(a != b),
            BinOp::Lt => Bool(a < b),
            BinOp::Le => Bool(a <= b),
            BinOp::Gt => Bool(a > b),
            BinOp::Ge => Bool(a >= b),
        })),
        (Float(a), Float(b)) => Ok(lit(match op {
            BinOp::Add => Float(a + b),
            BinOp::Sub => Float(a - b),
            BinOp::Mul => Float(a * b),
            BinOp::Div => Float(a / b),
            BinOp::Eq => Bool(a == b),
            BinOp::Ne => Bool(a != b),
            BinOp::Lt => Bool(a < b),
            BinOp::Le => Bool(a <= b),
            BinOp::Gt => Bool(a > b),
            BinOp::Ge => Bool(a >= b),
            _ => return Err(format!("operator {op:?} not supported on float in const")),
        })),
        (Str(a), Str(b)) => Ok(lit(match op {
            BinOp::Add => Str(format!("{a}{b}")),
            BinOp::Eq => Bool(a == b),
            BinOp::Ne => Bool(a != b),
            _ => return Err(format!("operator {op:?} not supported on string in const")),
        })),
        (Bool(a), Bool(b)) => Ok(lit(match op {
            BinOp::Eq => Bool(a == b),
            BinOp::Ne => Bool(a != b),
            BinOp::BitAnd => Bool(a & b),
            BinOp::BitOr => Bool(a | b),
            BinOp::BitXor => Bool(a ^ b),
            _ => return Err(format!("operator {op:?} not supported on bool in const")),
        })),
        _ => Err(format!(
            "type mismatch in const binary {op:?} ({} vs {})",
            describe_expr_kind(&l.kind),
            describe_expr_kind(&r.kind)
        )),
    }
}

fn cast_const(v: &Expr, ty: &Type, span: Span) -> Result<Expr, String> {
    let lit = |k: ExprKind| Expr { kind: k, span };
    use ExprKind::*;
    match (&v.kind, ty) {
        // int → int / int → float / float → int / float → float / bool → int.
        (Int(n), Type::I8 | Type::I16 | Type::I32 | Type::I64
            | Type::U8 | Type::U16 | Type::U32 | Type::U64) => Ok(lit(Int(*n))),
        (Int(n), Type::F32 | Type::F64) => Ok(lit(Float(*n as f64))),
        (Float(x), Type::F32 | Type::F64) => Ok(lit(Float(*x))),
        (Float(x), Type::I8 | Type::I16 | Type::I32 | Type::I64
            | Type::U8 | Type::U16 | Type::U32 | Type::U64) => Ok(lit(Int(*x as i64))),
        (Bool(b), Type::I8 | Type::I16 | Type::I32 | Type::I64
            | Type::U8 | Type::U16 | Type::U32 | Type::U64) => {
            Ok(lit(Int(if *b { 1 } else { 0 })))
        }
        _ => Err(format!("cast to {ty} not supported in const expression")),
    }
}

fn describe_expr_kind(k: &ExprKind) -> &'static str {
    match k {
        ExprKind::Int(_) => "int literal",
        ExprKind::Float(_) => "float literal",
        ExprKind::Bool(_) => "bool literal",
        ExprKind::Str(_) => "string literal",
        ExprKind::Var(_) => "identifier",
        ExprKind::Call { .. } => "function call",
        ExprKind::MethodCall { .. } => "method call",
        ExprKind::New { .. } => "object construction",
        ExprKind::Field { .. } => "field access",
        ExprKind::Index { .. } => "index",
        ExprKind::Array(_) => "array literal",
        ExprKind::MapLit(_) => "map literal",
        ExprKind::If { .. } => "if expression",
        ExprKind::IfLet { .. } => "if-let expression",
        ExprKind::Match { .. } => "match",
        ExprKind::Block(_) => "block",
        ExprKind::While { .. } | ExprKind::Loop { .. } | ExprKind::ForIn { .. } => "loop",
        ExprKind::Range { .. } => "range",
        _ => "non-constant expression",
    }
}

fn subst_const_item(item: Item, ctx: &SubstCtx<'_>) -> Item {
    match item {
        Item::Fn(mut f) => {
            f.body = subst_const_block(f.body, ctx);
            Item::Fn(f)
        }
        Item::Class(mut c) => {
            for m in c.methods.iter_mut().chain(c.static_methods.iter_mut()) {
                let body = std::mem::replace(
                    &mut m.body,
                    Block { stmts: Vec::new(), tail: None },
                );
                m.body = subst_const_block(body, ctx);
            }
            for prop in &mut c.properties {
                if let Some(g) = prop.getter.as_mut() {
                    let body = std::mem::replace(
                        &mut g.body,
                        Block { stmts: Vec::new(), tail: None },
                    );
                    g.body = subst_const_block(body, ctx);
                }
                if let Some(s) = prop.setter.as_mut() {
                    let body = std::mem::replace(
                        &mut s.body,
                        Block { stmts: Vec::new(), tail: None },
                    );
                    s.body = subst_const_block(body, ctx);
                }
            }
            Item::Class(c)
        }
        other => other,
    }
}

fn subst_const_block(b: Block, ctx: &SubstCtx<'_>) -> Block {
    Block {
        stmts: b
            .stmts
            .into_iter()
            .map(|s| subst_const_stmt(s, ctx))
            .collect(),
        tail: b.tail.map(|e| Box::new(subst_const_expr(*e, ctx))),
    }
}

fn subst_const_stmt(s: Stmt, ctx: &SubstCtx<'_>) -> Stmt {
    let kind = match s.kind {
        StmtKind::Let { name, ty, value } => StmtKind::Let {
            name,
            ty,
            value: subst_const_expr(value, ctx),
        },
        StmtKind::Expr(e) => StmtKind::Expr(subst_const_expr(e, ctx)),
    };
    Stmt { kind, span: s.span }
}

fn subst_const_expr(e: Expr, ctx: &SubstCtx<'_>) -> Expr {
    let span = e.span;
    let kind = match e.kind {
        // The substitution itself: `Var(name)` whose name is a const.
        // Re-apply the const's span to the literal so error messages
        // point at the use site, not the declaration site.
        ExprKind::Var(ref name) => {
            if let Some(lit) = ctx.consts.get(name) {
                let mut new_lit = lit.clone();
                new_lit.span = span;
                // If the const had an annotated type, wrap the
                // literal in a Cast so the substituted reference
                // carries that type. This lets `const N: u32 = 16`
                // be used in `i32 < N` style sites without a manual
                // `as u32` at every call.
                if let Some(ty) = ctx.types.get(name) {
                    return Expr::new(
                        ExprKind::Cast {
                            expr: Box::new(new_lit),
                            ty: ty.clone(),
                        },
                        span,
                    );
                }
                return new_lit;
            }
            ExprKind::Var(name.clone())
        }
        // Mechanical recursion through every other shape.
        ExprKind::Unary { op, expr } => ExprKind::Unary {
            op,
            expr: Box::new(subst_const_expr(*expr, ctx)),
        },
        ExprKind::Binary { op, lhs, rhs } => ExprKind::Binary {
            op,
            lhs: Box::new(subst_const_expr(*lhs, ctx)),
            rhs: Box::new(subst_const_expr(*rhs, ctx)),
        },
        ExprKind::Logical { op, lhs, rhs } => ExprKind::Logical {
            op,
            lhs: Box::new(subst_const_expr(*lhs, ctx)),
            rhs: Box::new(subst_const_expr(*rhs, ctx)),
        },
        ExprKind::Cast { expr, ty } => ExprKind::Cast {
            expr: Box::new(subst_const_expr(*expr, ctx)),
            ty,
        },
        ExprKind::FnExpr { params, ret, body } => ExprKind::FnExpr {
            params,
            ret,
            body: subst_const_block(body, ctx),
        },
        ExprKind::Call { callee, args } => ExprKind::Call {
            callee,
            args: args.into_iter().map(|a| subst_const_expr(a, ctx)).collect(),
        },
        ExprKind::Field { obj, name } => ExprKind::Field {
            obj: Box::new(subst_const_expr(*obj, ctx)),
            name,
        },
        ExprKind::MethodCall { obj, method, args } => ExprKind::MethodCall {
            obj: Box::new(subst_const_expr(*obj, ctx)),
            method,
            args: args.into_iter().map(|a| subst_const_expr(a, ctx)).collect(),
        },
        ExprKind::New { class, type_args, args, init_method } => ExprKind::New {
            class,
            type_args,
            args: args.into_iter().map(|a| subst_const_expr(a, ctx)).collect(),
            init_method,
        },
        ExprKind::Block(b) => ExprKind::Block(subst_const_block(b, ctx)),
        ExprKind::If {
            cond,
            then_branch,
            else_branch,
        } => ExprKind::If {
            cond: Box::new(subst_const_expr(*cond, ctx)),
            then_branch: subst_const_block(then_branch, ctx),
            else_branch: else_branch.map(|e| Box::new(subst_const_expr(*e, ctx))),
        },
        ExprKind::IfLet {
            name,
            expr,
            then_branch,
            else_branch,
        } => ExprKind::IfLet {
            name,
            expr: Box::new(subst_const_expr(*expr, ctx)),
            then_branch: subst_const_block(then_branch, ctx),
            else_branch: else_branch.map(|e| Box::new(subst_const_expr(*e, ctx))),
        },
        ExprKind::While { cond, body } => ExprKind::While {
            cond: Box::new(subst_const_expr(*cond, ctx)),
            body: subst_const_block(body, ctx),
        },
        ExprKind::Loop { body } => ExprKind::Loop {
            body: subst_const_block(body, ctx),
        },
        ExprKind::ForIn { var, iter, body } => ExprKind::ForIn {
            var,
            iter: Box::new(subst_const_expr(*iter, ctx)),
            body: subst_const_block(body, ctx),
        },
        ExprKind::Range { start, end, inclusive } => ExprKind::Range {
            start: Box::new(subst_const_expr(*start, ctx)),
            end: Box::new(subst_const_expr(*end, ctx)),
            inclusive,
        },
        ExprKind::Closure { fn_name, captures } => {
            ExprKind::Closure { fn_name, captures }
        }
        ExprKind::SuperCall { method, args } => ExprKind::SuperCall {
            method,
            args: args.into_iter().map(|a| subst_const_expr(a, ctx)).collect(),
        },
        ExprKind::Return(opt) => {
            ExprKind::Return(opt.map(|e| Box::new(subst_const_expr(*e, ctx))))
        }
        ExprKind::Break(opt) => {
            ExprKind::Break(opt.map(|e| Box::new(subst_const_expr(*e, ctx))))
        }
        ExprKind::Assign { target, value } => ExprKind::Assign {
            target,
            value: Box::new(subst_const_expr(*value, ctx)),
        },
        ExprKind::AssignField { obj, field, value } => ExprKind::AssignField {
            obj: Box::new(subst_const_expr(*obj, ctx)),
            field,
            value: Box::new(subst_const_expr(*value, ctx)),
        },
        ExprKind::AssignIndex { obj, index, value } => ExprKind::AssignIndex {
            obj: Box::new(subst_const_expr(*obj, ctx)),
            index: Box::new(subst_const_expr(*index, ctx)),
            value: Box::new(subst_const_expr(*value, ctx)),
        },
        ExprKind::Array(items) => ExprKind::Array(
            items.into_iter().map(|e| subst_const_expr(e, ctx)).collect(),
        ),
        ExprKind::Tuple(items) => ExprKind::Tuple(
            items.into_iter().map(|e| subst_const_expr(e, ctx)).collect(),
        ),
        ExprKind::MapLit(entries) => ExprKind::MapLit(
            entries
                .into_iter()
                .map(|(k, v)| (subst_const_expr(k, ctx), subst_const_expr(v, ctx)))
                .collect(),
        ),
        ExprKind::Index { obj, index } => ExprKind::Index {
            obj: Box::new(subst_const_expr(*obj, ctx)),
            index: Box::new(subst_const_expr(*index, ctx)),
        },
        ExprKind::Some(inner) => ExprKind::Some(Box::new(subst_const_expr(*inner, ctx))),
        ExprKind::EnumCtor {
            enum_name,
            variant,
            args,
        } => ExprKind::EnumCtor {
            enum_name,
            variant,
            args: match args {
                ilang_ast::CtorArgs::Unit => ilang_ast::CtorArgs::Unit,
                ilang_ast::CtorArgs::Tuple(es) => ilang_ast::CtorArgs::Tuple(
                    es.into_iter().map(|e| subst_const_expr(e, ctx)).collect(),
                ),
                ilang_ast::CtorArgs::Struct(fs) => ilang_ast::CtorArgs::Struct(
                    fs.into_iter()
                        .map(|(n, e)| (n, subst_const_expr(e, ctx)))
                        .collect(),
                ),
            },
        },
        ExprKind::Match { scrutinee, arms } => ExprKind::Match {
            scrutinee: Box::new(subst_const_expr(*scrutinee, ctx)),
            arms: arms
                .into_iter()
                .map(|arm| ilang_ast::MatchArm {
                    pattern: arm.pattern,
                    body: subst_const_expr(arm.body, ctx),
                    span: arm.span,
                })
                .collect(),
        },
        // Trivial nodes pass through.
        other @ (ExprKind::Int(_)
        | ExprKind::Float(_)
        | ExprKind::Bool(_)
        | ExprKind::Str(_)
        | ExprKind::This
        | ExprKind::None
        | ExprKind::Continue) => other,
        ExprKind::StructLit { class, fields } => ExprKind::StructLit {
            class,
            fields: fields
                .into_iter()
                .map(|(n, e)| (n, subst_const_expr(e, ctx)))
                .collect(),
        },
    };
    Expr { kind, span }
}
