//! AST-level dead-code elimination.
//!
//! Runs after `monomorphize_with_template_reattach` and before MIR
//! lowering. Drops top-level `Item::Fn` / `Item::Class` / `Item::Enum`
//! / `Item::Interface` declarations that no reachable code references,
//! so the downstream pipeline (mir lower + mir opts + cranelift) only
//! processes what `__main` (and other roots) can actually reach.
//!
//! Roots:
//!   - the program-level statements + tail (the synthetic `__main` body),
//!   - every `Item::ExternC` block (the ilang-side `FnDef` callbacks
//!     in a block may be invoked from C with no static call site we
//!     can see),
//!   - every `Item::Interface` (referenced through dynamic dispatch
//!     that can't be resolved statically without full type info).
//!
//! Walk: for every newly-live item, scan its body / signature for
//! - `Call.callee` / `Var(name)` → mark a free fn,
//! - `New.class` / `StructLit.class` / `LetStruct.class` → mark a class,
//! - `EnumCtor.enum_name` / `Pattern::Variant.enum_name` → mark an enum,
//! - every `Type::Object(s)` / `Type::Enum(s)` / `Type::Generic(g)` →
//!   mark `s` / `g.base` (post-monomorph there shouldn't be any
//!   `Type::Generic` left, but recurse anyway).
//!
//! For each newly-live class, also walk every method body, static
//! method body, field type, parent name, and interface list. For each
//! newly-live enum, walk every variant payload type.
//!
//! Conservative cases this pass deliberately leaves alone:
//!   - `MethodCall` / `super.method()` — we don't try to map a method
//!     name back to a specific class. The receiver's type drags the
//!     correct class in via type walks elsewhere; all methods of a
//!     reached class stay live regardless of which one is called.
//!   - Class hierarchies — when a class is live, every method stays;
//!     no per-method DCE here (MIR-level `dce_fn` does that).

use std::collections::{HashMap, VecDeque};

use ilang_ast::{
    Block, ClassDecl, EnumDecl, Expr, ExprKind, ExternCBlock, ExternCItem, FnDecl,
    InterfaceDecl, Item, Param, PatternKind, Program as AstProgram, Stmt, StmtKind, Symbol,
    Type,
};

#[derive(Debug, Default, Clone, Copy)]
pub struct Stats {
    pub items_removed: usize,
}

pub fn run(prog: &mut AstProgram) -> Stats {
    let n = prog.items.len();
    if n == 0 {
        return Stats::default();
    }

    // name -> item index. Item names are unique post-mangle /
    // post-monomorph (the type checker rejects duplicates), so first
    // match wins; conflicts here would already have errored upstream.
    let mut name_idx: HashMap<Symbol, usize> = HashMap::new();
    for (i, item) in prog.items.iter().enumerate() {
        let name = match item {
            Item::Fn(f) => Some(f.name),
            Item::Class(c) => Some(c.name),
            Item::Enum(e) => Some(e.name),
            Item::Interface(d) => Some(d.name),
            Item::ExternC(_) | Item::Use(_) | Item::Const(_) => None,
        };
        if let Some(nm) = name {
            name_idx.entry(nm).or_insert(i);
        }
    }

    let mut tracker = Tracker {
        name_idx: &name_idx,
        live: vec![false; n],
        wq: VecDeque::new(),
    };

    // Conservative roots: every ExternC block stays; every Interface
    // stays (dynamic dispatch on a `@com interface` etc. uses an
    // interface vtable layout we can't resolve statically here).
    for (i, item) in prog.items.iter().enumerate() {
        match item {
            Item::ExternC(_) | Item::Interface(_) => tracker.mark_idx(i),
            _ => {}
        }
    }

    // Walk the program-level stmts + tail (the `__main` body).
    for s in &prog.stmts {
        tracker.walk_stmt(s);
    }
    if let Some(t) = &prog.tail {
        tracker.walk_expr(t);
    }

    // Drain: each newly-live item gets its body / signature scanned
    // for further refs.
    while let Some(idx) = tracker.wq.pop_front() {
        match &prog.items[idx] {
            Item::Fn(f) => tracker.walk_fn_decl(f),
            Item::Class(c) => tracker.walk_class_decl(c),
            Item::Enum(e) => tracker.walk_enum_decl(e),
            Item::Interface(d) => tracker.walk_interface_decl(d),
            Item::ExternC(b) => tracker.walk_extern_c_block(b),
            Item::Use(_) | Item::Const(_) => {}
        }
    }

    let kept: usize = tracker.live.iter().filter(|b| **b).count();
    let removed = n - kept;
    if removed == 0 {
        return Stats::default();
    }

    let old_items = std::mem::take(&mut prog.items);
    let mut new_items = Vec::with_capacity(kept);
    for (i, item) in old_items.into_iter().enumerate() {
        if tracker.live[i] {
            new_items.push(item);
        }
    }
    prog.items = new_items;

    Stats { items_removed: removed }
}

struct Tracker<'a> {
    name_idx: &'a HashMap<Symbol, usize>,
    live: Vec<bool>,
    wq: VecDeque<usize>,
}

impl<'a> Tracker<'a> {
    fn mark_idx(&mut self, i: usize) {
        if !self.live[i] {
            self.live[i] = true;
            self.wq.push_back(i);
        }
    }

    fn mark_name(&mut self, name: Symbol) {
        if let Some(&i) = self.name_idx.get(&name) {
            self.mark_idx(i);
        }
    }

    fn walk_type(&mut self, ty: &Type) {
        match ty {
            Type::Object(name) | Type::Enum(name) => self.mark_name(*name),
            Type::Generic(g) => {
                self.mark_name(g.base);
                for arg in g.args.iter() {
                    self.walk_type(arg);
                }
            }
            Type::Array { elem, .. } => self.walk_type(elem),
            Type::Tuple(elems) => {
                for t in elems.iter() {
                    self.walk_type(t);
                }
            }
            Type::Optional(inner)
            | Type::Weak(inner)
            | Type::RawPtr { inner, .. } => self.walk_type(inner),
            Type::Fn(ft) => {
                for p in ft.params.iter() {
                    self.walk_type(p);
                }
                self.walk_type(&ft.ret);
            }
            // Primitive / unresolved / FFI scalars carry no Symbol
            // references to a top-level item.
            Type::I8
            | Type::I16
            | Type::I32
            | Type::I64
            | Type::U8
            | Type::U16
            | Type::U32
            | Type::U64
            | Type::F32
            | Type::F64
            | Type::Bool
            | Type::Str
            | Type::Unit
            | Type::Any
            | Type::Error
            | Type::TypeVar(_)
            | Type::CVoid
            | Type::CChar
            | Type::Size
            | Type::SSize
            | Type::Simd { .. } => {}
        }
    }

    fn walk_param(&mut self, p: &Param) {
        self.walk_type(&p.ty);
        if let Some(d) = &p.default {
            self.walk_expr(d);
        }
    }

    fn walk_fn_decl(&mut self, f: &FnDecl) {
        for p in f.params.iter() {
            self.walk_param(p);
        }
        if let Some(r) = &f.ret {
            self.walk_type(r);
        }
        self.walk_block(&f.body);
    }

    fn walk_class_decl(&mut self, c: &ClassDecl) {
        if let Some(parent) = c.parent {
            self.mark_name(parent);
        }
        for iface in c.interfaces.iter() {
            self.mark_name(*iface);
        }
        for fld in c.fields.iter() {
            self.walk_type(&fld.ty);
        }
        for sf in c.static_fields.iter() {
            self.walk_type(&sf.ty);
            self.walk_expr(&sf.value);
        }
        for m in c.methods.iter() {
            self.walk_fn_decl(m);
        }
        for sm in c.static_methods.iter() {
            self.walk_fn_decl(sm);
        }
        for prop in c.properties.iter() {
            self.walk_type(&prop.ty);
            if let Some(g) = &prop.getter {
                self.walk_fn_decl(g);
            }
            if let Some(s) = &prop.setter {
                self.walk_fn_decl(s);
            }
        }
    }

    fn walk_enum_decl(&mut self, e: &EnumDecl) {
        if let Some(repr) = &e.repr_ty {
            self.walk_type(repr);
        }
        for v in e.variants.iter() {
            match &v.payload {
                ilang_ast::VariantPayload::Unit => {}
                ilang_ast::VariantPayload::Tuple(tys) => {
                    for t in tys.iter() {
                        self.walk_type(t);
                    }
                }
                ilang_ast::VariantPayload::Struct(fields) => {
                    for fld in fields.iter() {
                        self.walk_type(&fld.ty);
                    }
                }
            }
        }
    }

    fn walk_interface_decl(&mut self, d: &InterfaceDecl) {
        if let Some(parent) = d.parent {
            self.mark_name(parent);
        }
        for m in d.methods.iter() {
            for p in m.params.iter() {
                self.walk_param(p);
            }
            if let Some(r) = &m.ret {
                self.walk_type(r);
            }
        }
    }

    fn walk_extern_c_block(&mut self, b: &ExternCBlock) {
        for inner in b.items.iter() {
            match inner {
                ExternCItem::Struct { fields, .. } | ExternCItem::Union { fields, .. } => {
                    for fld in fields.iter() {
                        self.walk_type(&fld.ty);
                    }
                }
                ExternCItem::FnDecl { params, ret, .. } => {
                    for p in params.iter() {
                        self.walk_param(p);
                    }
                    if let Some(r) = ret {
                        self.walk_type(r);
                    }
                }
                ExternCItem::FnDef(f) => self.walk_fn_decl(f),
                ExternCItem::Class(c) => self.walk_class_decl(c),
            }
        }
        for iface in b.interfaces.iter() {
            self.walk_interface_decl(iface);
        }
    }

    fn walk_stmt(&mut self, s: &Stmt) {
        match &s.kind {
            StmtKind::Let { ty, value, .. } => {
                if let Some(t) = ty {
                    self.walk_type(t);
                }
                self.walk_expr(value);
            }
            StmtKind::LetTuple { value, .. } => self.walk_expr(value),
            StmtKind::LetStruct { class, value, .. } => {
                self.mark_name(*class);
                self.walk_expr(value);
            }
            StmtKind::Expr(e) => self.walk_expr(e),
        }
    }

    fn walk_block(&mut self, b: &Block) {
        for s in &b.stmts {
            self.walk_stmt(s);
        }
        if let Some(t) = &b.tail {
            self.walk_expr(t);
        }
    }

    fn walk_expr(&mut self, e: &Expr) {
        use ExprKind as E;
        match &e.kind {
            E::Int(_)
            | E::Float(_)
            | E::Bool(_)
            | E::Str(_)
            | E::This
            | E::None
            | E::Continue => {}
            E::Var(name) => {
                // Top-level fn used as a first-class value
                // (`let f = my_fn`) — mark the target if it exists.
                self.mark_name(*name);
            }
            E::Closure { fn_name, captures } => {
                // The hoisted wrapper fn is referenced here by name.
                self.mark_name(*fn_name);
                for (_, ty) in captures.iter() {
                    self.walk_type(ty);
                }
            }
            E::Call { callee, args } => {
                self.mark_name(*callee);
                for a in args.iter() {
                    self.walk_expr(a);
                }
            }
            E::SuperCall { args, .. } => {
                for a in args.iter() {
                    self.walk_expr(a);
                }
            }
            E::New { class, type_args, args, .. } => {
                self.mark_name(*class);
                for ty in type_args.iter() {
                    self.walk_type(ty);
                }
                for a in args.iter() {
                    self.walk_expr(a);
                }
            }
            E::MethodCall { obj, args, .. } => {
                self.walk_expr(obj);
                for a in args.iter() {
                    self.walk_expr(a);
                }
            }
            E::Field { obj, .. } => self.walk_expr(obj),
            E::Unary { expr, .. } | E::Some(expr) | E::Await(expr) => self.walk_expr(expr),
            E::Binary { lhs, rhs, .. } | E::Logical { lhs, rhs, .. } => {
                self.walk_expr(lhs);
                self.walk_expr(rhs);
            }
            E::Cast { expr, ty }
            | E::TypeTest { expr, ty }
            | E::TypeDowncast { expr, ty } => {
                self.walk_expr(expr);
                self.walk_type(ty);
            }
            E::FnExpr { params, body, ret } => {
                for p in params.iter() {
                    self.walk_param(p);
                }
                if let Some(r) = ret {
                    self.walk_type(r);
                }
                self.walk_block(body);
            }
            E::Block(b) => self.walk_block(b),
            E::If { cond, then_branch, else_branch } => {
                self.walk_expr(cond);
                self.walk_block(then_branch);
                if let Some(e2) = else_branch {
                    self.walk_expr(e2);
                }
            }
            E::While { cond, body } => {
                self.walk_expr(cond);
                self.walk_block(body);
            }
            E::Loop { body } => self.walk_block(body),
            E::ForIn { iter, body, .. } => {
                self.walk_expr(iter);
                self.walk_block(body);
            }
            E::IfLet { expr, then_branch, else_branch, .. } => {
                self.walk_expr(expr);
                self.walk_block(then_branch);
                if let Some(e2) = else_branch {
                    self.walk_expr(e2);
                }
            }
            E::Match { scrutinee, arms } => {
                self.walk_expr(scrutinee);
                for arm in arms.iter() {
                    if let PatternKind::Variant { enum_name, .. } = &arm.pattern.kind {
                        if let Some(n) = enum_name {
                            self.mark_name(*n);
                        }
                    }
                    // Bindings inside a pattern are locals — no item refs.
                    self.walk_expr(&arm.body);
                }
            }
            E::Range { start, end, .. } => {
                if let Some(s) = start {
                    self.walk_expr(s);
                }
                if let Some(e2) = end {
                    self.walk_expr(e2);
                }
            }
            E::Break(v) | E::Return(v) => {
                if let Some(x) = v {
                    self.walk_expr(x);
                }
            }
            E::Array(items) | E::Tuple(items) => {
                for x in items.iter() {
                    self.walk_expr(x);
                }
            }
            E::Index { obj, index } => {
                self.walk_expr(obj);
                self.walk_expr(index);
            }
            E::Assign { value, .. } => self.walk_expr(value),
            E::AssignField { obj, value, .. } => {
                self.walk_expr(obj);
                self.walk_expr(value);
            }
            E::AssignIndex { obj, index, value } => {
                self.walk_expr(obj);
                self.walk_expr(index);
                self.walk_expr(value);
            }
            E::StructLit { class, fields, .. } => {
                self.mark_name(*class);
                for (_, v) in fields.iter() {
                    self.walk_expr(v);
                }
            }
            E::MapLit(entries) => {
                for (k, v) in entries.iter() {
                    self.walk_expr(k);
                    self.walk_expr(v);
                }
            }
            E::EnumCtor { enum_name, args, .. } => {
                self.mark_name(*enum_name);
                match args {
                    ilang_ast::CtorArgs::Unit => {}
                    ilang_ast::CtorArgs::Tuple(es) => {
                        for e2 in es.iter() {
                            self.walk_expr(e2);
                        }
                    }
                    ilang_ast::CtorArgs::Struct(fs) => {
                        for (_, e2) in fs.iter() {
                            self.walk_expr(e2);
                        }
                    }
                }
            }
            E::Template { parts } => {
                for p in parts.iter() {
                    if let ilang_ast::TemplatePart::Expr(e2) = p {
                        self.walk_expr(e2);
                    }
                }
            }
        }
    }
}

