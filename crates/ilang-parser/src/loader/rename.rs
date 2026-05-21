//! Selective-import rename pass.
//!
//! Selective imports that resolve through `pub use` chains record
//! a `bare → umbrella.bare` rename rule (see `apply_use`). After the
//! loader has merged every imported module, this pass walks the
//! Program and rewrites bare references in the entry's items / stmts
//! / tail to the umbrella-qualified form that the umbrella's nested
//! `pub use` already merged. The rewrite is name-keyed (not
//! prefix-keyed like `prefix_expr`), so it only fires on the specific
//! names the user imported.
//!
//! Only bare names (no `.`) can collide; sub-module items merged via
//! `prefix_item` already have dotted names and pass through these
//! walkers untouched.

use std::collections::HashMap;

use ilang_ast::{Block, ClassDecl, Expr, ExprKind, Item, Program, Stmt, StmtKind, Symbol, Type};

fn rename_sym(name: &Symbol, rules: &HashMap<Symbol, Symbol>) -> Option<Symbol> {
    rules.get(name).cloned()
}

/// `true` if `name` is a sub-module item that was merged in via a
/// previous `prefix_item` pass — those names contain a `.` because the
/// loader rewrote them as `module.original`. Such items already went
/// through their own module's `module_rename_rules`, and their bodies'
/// bare references legitimately point at FFI builtins or in-module
/// symbols that share a name with an umbrella-level export (e.g.
/// `cocoa.writeU8`). The entry's rules must not touch them.
fn is_submodule_name(name: &Symbol) -> bool {
    name.as_str().contains('.')
}

pub(super) fn rename_in_program(prog: &mut Program, rules: &HashMap<Symbol, Symbol>) {
    for item in prog.items.iter_mut() {
        rename_in_item(item, rules);
    }
    for s in prog.stmts.iter_mut() {
        rename_in_stmt(s, rules);
    }
    if let Some(t) = prog.tail.as_mut() {
        rename_in_expr(t, rules);
    }
}

pub(super) fn rename_in_item(item: &mut Item, rules: &HashMap<Symbol, Symbol>) {
    match item {
        Item::Fn(f) => {
            if is_submodule_name(&f.name) {
                return;
            }
            for p in f.params.iter_mut() {
                rename_in_type(&mut p.ty, rules);
                if let Some(d) = p.default.as_mut() {
                    rename_in_expr(d, rules);
                }
            }
            if let Some(t) = f.ret.as_mut() {
                rename_in_type(t, rules);
            }
            rename_in_block(&mut f.body, rules);
        }
        Item::Class(c) => {
            if is_submodule_name(&c.name) {
                return;
            }
            rename_in_class(c, rules);
        }
        Item::Enum(_) => {}
        Item::Use(_) => {}
        Item::Const(c) => {
            if is_submodule_name(&c.name) {
                return;
            }
            if let Some(t) = c.ty.as_mut() {
                rename_in_type(t, rules);
            }
            rename_in_expr(&mut c.value, rules);
        }
        Item::ExternC(b) => {
            for inner in b.items.iter_mut() {
                match inner {
                    ilang_ast::ExternCItem::FnDef(f) => {
                        if is_submodule_name(&f.name) {
                            continue;
                        }
                        for p in f.params.iter_mut() {
                            rename_in_type(&mut p.ty, rules);
                            if let Some(d) = p.default.as_mut() {
                                rename_in_expr(d, rules);
                            }
                        }
                        if let Some(t) = f.ret.as_mut() {
                            rename_in_type(t, rules);
                        }
                        rename_in_block(&mut f.body, rules);
                    }
                    ilang_ast::ExternCItem::FnDecl { name, params, ret, .. } => {
                        if is_submodule_name(name) {
                            continue;
                        }
                        for p in params.iter_mut() {
                            rename_in_type(&mut p.ty, rules);
                            if let Some(d) = p.default.as_mut() {
                                rename_in_expr(d, rules);
                            }
                        }
                        if let Some(t) = ret.as_mut() {
                            rename_in_type(t, rules);
                        }
                    }
                    ilang_ast::ExternCItem::Struct { name, fields, .. }
                    | ilang_ast::ExternCItem::Union { name, fields, .. } => {
                        if is_submodule_name(name) {
                            continue;
                        }
                        for f in fields.iter_mut() {
                            rename_in_type(&mut f.ty, rules);
                        }
                    }
                    ilang_ast::ExternCItem::Class(c) => {
                        if is_submodule_name(&c.name) {
                            continue;
                        }
                        rename_in_class(c, rules);
                    }
                }
            }
            // @objc / @com interfaces declared alongside the FFI items
            // — rewrite their parent reference and method param /
            // return types so cross-module references stay consistent.
            // Without the parent rewrite a `@com pub interface X : Y`
            // can't inherit from a `Y` brought in via
            // `use M { Y }`: the bare `Y` parent stays unqualified
            // while the imported `Y` is renamed to `M.Y`, so the
            // post-merge `prefix_item` would prepend the wrong
            // module prefix and the inherited methods stop resolving.
            for iface in b.interfaces.iter_mut() {
                if is_submodule_name(&iface.name) {
                    continue;
                }
                if let Some(parent) = iface.parent.as_mut() {
                    if let Some(new_name) = rename_sym(parent, rules) {
                        *parent = new_name;
                    }
                }
                for m in iface.methods.iter_mut() {
                    for p in m.params.iter_mut() {
                        rename_in_type(&mut p.ty, rules);
                    }
                    if let Some(t) = m.ret.as_mut() {
                        rename_in_type(t, rules);
                    }
                }
            }
        }
        Item::Interface(_) => {}
    }
}

fn rename_in_class(c: &mut ClassDecl, rules: &HashMap<Symbol, Symbol>) {
    if let Some(parent) = c.parent.as_mut() {
        if let Some(new_name) = rename_sym(parent, rules) {
            *parent = new_name;
        }
    }
    for ifn in c.interfaces.iter_mut() {
        if let Some(new_name) = rename_sym(ifn, rules) {
            *ifn = new_name;
        }
    }
    for f in c.fields.iter_mut() {
        rename_in_type(&mut f.ty, rules);
    }
    for sf in c.static_fields.iter_mut() {
        rename_in_type(&mut sf.ty, rules);
        rename_in_expr(&mut sf.value, rules);
    }
    for m in c.methods.iter_mut().chain(c.static_methods.iter_mut()) {
        for p in m.params.iter_mut() {
            rename_in_type(&mut p.ty, rules);
            if let Some(d) = p.default.as_mut() {
                rename_in_expr(d, rules);
            }
        }
        if let Some(t) = m.ret.as_mut() {
            rename_in_type(t, rules);
        }
        rename_in_block(&mut m.body, rules);
    }
    for prop in c.properties.iter_mut() {
        rename_in_type(&mut prop.ty, rules);
        if let Some(g) = prop.getter.as_mut() {
            for p in g.params.iter_mut() {
                rename_in_type(&mut p.ty, rules);
            }
            if let Some(t) = g.ret.as_mut() {
                rename_in_type(t, rules);
            }
            rename_in_block(&mut g.body, rules);
        }
        if let Some(s) = prop.setter.as_mut() {
            for p in s.params.iter_mut() {
                rename_in_type(&mut p.ty, rules);
            }
            if let Some(t) = s.ret.as_mut() {
                rename_in_type(t, rules);
            }
            rename_in_block(&mut s.body, rules);
        }
    }
}

fn rename_in_block(b: &mut Block, rules: &HashMap<Symbol, Symbol>) {
    for s in b.stmts.iter_mut() {
        rename_in_stmt(s, rules);
    }
    if let Some(t) = b.tail.as_mut() {
        rename_in_expr(t, rules);
    }
}

pub(super) fn rename_in_stmt(s: &mut Stmt, rules: &HashMap<Symbol, Symbol>) {
    match &mut s.kind {
        StmtKind::Let { value, ty, .. } => {
            if let Some(t) = ty.as_mut() {
                rename_in_type(t, rules);
            }
            rename_in_expr(value, rules);
        }
        StmtKind::LetTuple { value, .. }
        | StmtKind::LetStruct { value, .. } => rename_in_expr(value, rules),
        StmtKind::Expr(e) => rename_in_expr(e, rules),
    }
}

fn rename_in_expr(e: &mut Expr, rules: &HashMap<Symbol, Symbol>) {
    match &mut e.kind {
        ExprKind::Var(name) => {
            if let Some(new_name) = rename_sym(name, rules) {
                *name = new_name;
            }
        }
        ExprKind::Call { callee, args } => {
            if let Some(new_name) = rename_sym(callee, rules) {
                *callee = new_name;
            }
            for a in args.iter_mut() {
                rename_in_expr(a, rules);
            }
        }
        ExprKind::SuperCall { args, .. } => {
            for a in args.iter_mut() {
                rename_in_expr(a, rules);
            }
        }
        ExprKind::New { class, type_args, args, .. } => {
            if let Some(new_name) = rename_sym(class, rules) {
                *class = new_name;
            }
            for ta in type_args.iter_mut() {
                rename_in_type(ta, rules);
            }
            for a in args.iter_mut() {
                rename_in_expr(a, rules);
            }
        }
        ExprKind::EnumCtor { enum_name, args, .. } => {
            if let Some(new_name) = rename_sym(enum_name, rules) {
                *enum_name = new_name;
            }
            match args {
                ilang_ast::CtorArgs::Unit => {}
                ilang_ast::CtorArgs::Tuple(es) => {
                    for e in es.iter_mut() {
                        rename_in_expr(e, rules);
                    }
                }
                ilang_ast::CtorArgs::Struct(fs) => {
                    for (_, e) in fs.iter_mut() {
                        rename_in_expr(e, rules);
                    }
                }
            }
        }
        ExprKind::Field { obj, .. } => rename_in_expr(obj, rules),
        ExprKind::MethodCall { obj, args, .. } => {
            rename_in_expr(obj, rules);
            for a in args.iter_mut() {
                rename_in_expr(a, rules);
            }
        }
        ExprKind::Unary { expr, .. } => rename_in_expr(expr, rules),
        ExprKind::Binary { lhs, rhs, .. } => {
            rename_in_expr(lhs, rules);
            rename_in_expr(rhs, rules);
        }
        ExprKind::Logical { lhs, rhs, .. } => {
            rename_in_expr(lhs, rules);
            rename_in_expr(rhs, rules);
        }
        ExprKind::Cast { expr, ty }
        | ExprKind::TypeTest { expr, ty }
        | ExprKind::TypeDowncast { expr, ty } => {
            rename_in_expr(expr, rules);
            rename_in_type(ty, rules);
        }
        ExprKind::Block(b) => rename_in_block(b, rules),
        ExprKind::If { cond, then_branch, else_branch } => {
            rename_in_expr(cond, rules);
            rename_in_block(then_branch, rules);
            if let Some(e) = else_branch.as_mut() {
                rename_in_expr(e, rules);
            }
        }
        ExprKind::IfLet { expr, then_branch, else_branch, .. } => {
            rename_in_expr(expr, rules);
            rename_in_block(then_branch, rules);
            if let Some(e) = else_branch.as_mut() {
                rename_in_expr(e, rules);
            }
        }
        ExprKind::While { cond, body } => {
            rename_in_expr(cond, rules);
            rename_in_block(body, rules);
        }
        ExprKind::ForIn { iter, body, .. } => {
            rename_in_expr(iter, rules);
            rename_in_block(body, rules);
        }
        ExprKind::Range { start, end, .. } => {
            if let Some(s) = start.as_mut() {
                rename_in_expr(s, rules);
            }
            if let Some(e) = end.as_mut() {
                rename_in_expr(e, rules);
            }
        }
        ExprKind::Loop { body } => rename_in_block(body, rules),
        ExprKind::Break(opt) => {
            if let Some(e) = opt.as_mut() {
                rename_in_expr(e, rules);
            }
        }
        ExprKind::Return(opt) => {
            if let Some(e) = opt.as_mut() {
                rename_in_expr(e, rules);
            }
        }
        ExprKind::Assign { value, .. } => rename_in_expr(value, rules),
        ExprKind::AssignField { obj, value, .. } => {
            rename_in_expr(obj, rules);
            rename_in_expr(value, rules);
        }
        ExprKind::FnExpr { params, ret, body } => {
            for p in params.iter_mut() {
                rename_in_type(&mut p.ty, rules);
                if let Some(d) = p.default.as_mut() {
                    rename_in_expr(d, rules);
                }
            }
            if let Some(t) = ret.as_mut() {
                rename_in_type(t, rules);
            }
            rename_in_block(body, rules);
        }
        ExprKind::Array(es) | ExprKind::Tuple(es) => {
            for e in es.iter_mut() {
                rename_in_expr(e, rules);
            }
        }
        ExprKind::StructLit { class, fields, .. } => {
            if let Some(new_name) = rename_sym(class, rules) {
                *class = new_name;
            }
            for (_, e) in fields.iter_mut() {
                rename_in_expr(e, rules);
            }
        }
        ExprKind::MapLit(entries) => {
            for (k, v) in entries.iter_mut() {
                rename_in_expr(k, rules);
                rename_in_expr(v, rules);
            }
        }
        ExprKind::Index { obj, index } => {
            rename_in_expr(obj, rules);
            rename_in_expr(index, rules);
        }
        ExprKind::AssignIndex { obj, index, value } => {
            rename_in_expr(obj, rules);
            rename_in_expr(index, rules);
            rename_in_expr(value, rules);
        }
        ExprKind::Some(inner) => rename_in_expr(inner, rules),
        ExprKind::Await(inner) => rename_in_expr(inner, rules),
        ExprKind::Match { scrutinee, arms } => {
            rename_in_expr(scrutinee, rules);
            for arm in arms.iter_mut() {
                rename_in_pattern(&mut arm.pattern, rules);
                rename_in_expr(&mut arm.body, rules);
            }
        }
        ExprKind::Closure { .. }
        | ExprKind::This
        | ExprKind::None
        | ExprKind::Continue
        | ExprKind::Int(_)
        | ExprKind::Float(_)
        | ExprKind::Bool(_)
        | ExprKind::Str(_) => {}
    }
}

/// Apply use-rules to a match-arm pattern. Only `Variant`'s
/// `enum_name` carries a user-named symbol that may have been
/// brought in via `use M { Enum }`. Without this, the type
/// checker sees the scrutinee as the fully-qualified
/// `windows.WindowMessage` (renamed by the type/expr passes) but
/// the pattern's bare `WindowMessage` stays unrenamed and the arm
/// fails with `expected windows.WindowMessage, got WindowMessage`.
fn rename_in_pattern(p: &mut ilang_ast::Pattern, rules: &HashMap<Symbol, Symbol>) {
    if let ilang_ast::PatternKind::Variant { enum_name, .. } = &mut p.kind {
        if let Some(name) = enum_name {
            if let Some(new_name) = rename_sym(name, rules) {
                *name = new_name;
            }
        }
    }
}

fn rename_in_type(t: &mut Type, rules: &HashMap<Symbol, Symbol>) {
    match t {
        Type::Object(name) => {
            if let Some(new_name) = rename_sym(name, rules) {
                *name = new_name;
            }
        }
        Type::Array { elem, .. } => rename_in_type(elem, rules),
        Type::Optional(inner) => rename_in_type(inner, rules),
        Type::Weak(inner) => rename_in_type(inner, rules),
        Type::Generic(g) => {
            if let Some(new_name) = rename_sym(&g.base, rules) {
                g.base = new_name;
            }
            for a in g.args.iter_mut() {
                rename_in_type(a, rules);
            }
        }
        Type::Fn(ft) => {
            for p in ft.params.iter_mut() {
                rename_in_type(p, rules);
            }
            rename_in_type(&mut ft.ret, rules);
        }
        Type::RawPtr { inner, .. } => rename_in_type(inner, rules),
        Type::Tuple(elems) => {
            for e in elems.iter_mut() {
                rename_in_type(e, rules);
            }
        }
        _ => {}
    }
}
