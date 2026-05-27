//! Per-block selector cache + the cstring / `sel_registerName`
//! helpers every builder uses to mint dispatch sites. The cache
//! gives each unique selector a `pub static __sel_<n>: i64 = 0` slot
//! on a synthesised `<tag>_sel_cache` class so the first call to a
//! wrapper interns the SEL through libobjc once and every later
//! call reads the slot directly.

use std::cell::RefCell;
use std::collections::BTreeMap;

use ilang_ast::{Block, Expr, ExprKind, Span, Stmt, StmtKind, Symbol, Type};

/// Tracks the per-block selector cache: each unique selector seen
/// during the block's desugar is assigned a static-field slot on
/// the synthetic `<tag>_sel_cache` class so the first call to a
/// wrapper fetches the SEL from libobjc once, stores it in the
/// slot, and every subsequent call reads the slot directly.
pub(super) struct SelectorCache {
    /// Synthesised class name (`<tag>_sel_cache`) hosting the
    /// per-selector static i64 slots.
    pub(super) class_name: Symbol,
    /// `selector → slot_field_name`. BTreeMap so the emitted class
    /// fields are deterministic regardless of selector encounter
    /// order (important for reproducible AST snapshots / tests).
    pub(super) entries: RefCell<BTreeMap<String, Symbol>>,
}

impl SelectorCache {
    pub(super) fn new(tag: &str) -> Self {
        Self {
            class_name: Symbol::intern(&format!("{tag}_sel_cache")),
            entries: RefCell::new(BTreeMap::new()),
        }
    }
    /// Returns the static-field name allocated for `selector`. Each
    /// distinct selector gets a fresh `__sel_<index>` slot the first
    /// time it's requested; repeat lookups return the same slot.
    pub(super) fn slot_for(&self, selector: &str) -> Symbol {
        let mut m = self.entries.borrow_mut();
        if let Some(&sym) = m.get(selector) {
            return sym;
        }
        let idx = m.len();
        let slot = Symbol::intern(&format!("__sel_{idx}"));
        m.insert(selector.to_string(), slot);
        slot
    }
}

pub(super) fn build_cstr(s: &str, span: Span) -> Expr {
    // Synthesised @objc desugar needs to convert ilang `string`s into
    // NUL-terminated C-string pointers (for selector / class lookups
    // and class-pair registration). User code reaches this via
    // `use std.ffi { cstrFromString }`, but the desugar runs at parse
    // time before any `use` resolution and can't rely on the importer
    // having pulled the helper in. Calling the runtime symbol
    // directly with the `$ffi.` prefix sidesteps the use-import
    // machinery — the `$` character isn't a legal ilang identifier
    // start (lex rejects it), so this name can only originate from
    // parser-synthesised code and the bare-name collision risk that
    // motivated the migration off compiler-magic dispatch doesn't
    // apply.
    Expr::new(
        ExprKind::Call {
            callee: Symbol::intern("$ffi.cstrFromString"),
            args: Box::new([Expr::new(ExprKind::Str(s.to_string()), span)]),
        },
        span,
    )
}

pub(super) fn build_sel_register_call(selector: &str, sel_register: Symbol, span: Span) -> Expr {
    Expr::new(
        ExprKind::Call {
            callee: sel_register,
            args: Box::new([build_cstr(selector, span)]),
        },
        span,
    )
}

/// Build the cached selector-lookup expression for the given
/// selector. Lazily fills the per-block `<tag>_sel_cache` slot on
/// first call, then returns the stored SEL on every subsequent
/// call. Equivalent to (in source):
///
/// ```ignore
/// {
///     let __cached_sel = <cache_class>.<slot>
///     if __cached_sel == 0 {
///         __cached_sel = sel_registerName(cstrFromString("X"))
///         <cache_class>.<slot> = __cached_sel
///     }
///     __cached_sel
/// }
/// ```
///
/// `__cached_sel` is a fresh local each call, so distinct wrappers
/// don't clash even though they share the desugar machinery.
pub(super) fn build_cached_sel_call(
    selector: &str,
    sel_register: Symbol,
    sel_struct: Symbol,
    cache: &SelectorCache,
    span: Span,
) -> Expr {
    let slot = cache.slot_for(selector);
    let local = Symbol::intern("__cached_sel");
    let cache_var = Expr::new(ExprKind::Var(cache.class_name), span);
    let slot_read = Expr::new(
        ExprKind::Field {
            obj: Box::new(cache_var.clone()),
            name: slot,
        },
        span,
    );
    // SEL static-field slot is `i64` (static-field types are
    // restricted to plain i64 / f64 / bool); the sel_register alias
    // returns `*sel_t`. Cast bidirectionally so the block's tail
    // delivers a `*sel_t` to the surrounding `__sel` binding.
    let sel_ptr_ty = Type::RawPtr {
        is_const: false,
        inner: Box::new(Type::Object(sel_struct)),
    };
    // let __cached_sel = <cache>.slot          // i64
    let let_stmt = Stmt::new(
        StmtKind::Let {
            is_pub: false,
            is_const: false,
            name: local,
            ty: None,
            value: slot_read,
        },
        span,
    );
    // if __cached_sel == 0 { ... }
    let cond = Expr::new(
        ExprKind::Binary {
            op: ilang_ast::BinOp::Eq,
            lhs: Box::new(Expr::new(ExprKind::Var(local), span)),
            rhs: Box::new(Expr::new(ExprKind::Int(0), span)),
        },
        span,
    );
    // __cached_sel = sel_register(cstrFromString("X")) as i64
    let register_expr = build_sel_register_call(selector, sel_register, span);
    let register_as_i64 = Expr::new(
        ExprKind::Cast {
            expr: Box::new(register_expr),
            ty: Type::I64,
        },
        span,
    );
    let assign_local = Stmt::new(
        StmtKind::Expr(Expr::new(
            ExprKind::Assign {
                target: local,
                value: Box::new(register_as_i64),
            },
            span,
        )),
        span,
    );
    // <cache>.slot = __cached_sel
    let store_slot = Stmt::new(
        StmtKind::Expr(Expr::new(
            ExprKind::AssignField {
                obj: Box::new(cache_var),
                field: slot,
                value: Box::new(Expr::new(ExprKind::Var(local), span)),
                is_init: false,
            },
            span,
        )),
        span,
    );
    let then_branch = Block {
        stmts: vec![assign_local, store_slot],
        tail: None,
    };
    let if_stmt = Stmt::new(
        StmtKind::Expr(Expr::new(
            ExprKind::If {
                cond: Box::new(cond),
                then_branch,
                else_branch: None,
            },
            span,
        )),
        span,
    );
    // Tail: __cached_sel as *sel_t — hand the surrounding wrapper
    // body a `*sel_t` so the alias call's signature matches.
    let tail = Expr::new(
        ExprKind::Cast {
            expr: Box::new(Expr::new(ExprKind::Var(local), span)),
            ty: sel_ptr_ty,
        },
        span,
    );
    Expr::new(
        ExprKind::Block(Block {
            stmts: vec![let_stmt, if_stmt],
            tail: Some(Box::new(tail)),
        }),
        span,
    )
}

/// Build the synthetic `<tag>_sel_cache` class that hosts one
/// `pub static __sel_<n>: i64 = 0` slot per selector the block
/// referenced. Emitted as the last item in the block so all wrapper
/// bodies (which reference its fields) parse before it.
pub(super) fn build_sel_cache_class(cache: &SelectorCache, span: Span) -> ilang_ast::ExternCItem {
    let entries = cache.entries.borrow();
    let static_fields: Vec<ilang_ast::StaticFieldDecl> = entries
        .values()
        .map(|slot| ilang_ast::StaticFieldDecl {
            is_pub: true,
            name: *slot,
            ty: Type::I64,
            value: Expr::new(ExprKind::Int(0), span),
            is_const: false,
            span,
        })
        .collect();
    ilang_ast::ExternCItem::Class(ilang_ast::ClassDecl {
        extern_lib: None,
        is_repr_c: false,
        is_packed: false,
        is_handle: false,
        is_union: false,
        is_pub: false,
        name: cache.class_name,
        parent: None,
        interfaces: Box::new([]),
        type_params: Box::new([]),
        fields: Box::new([]),
        methods: Box::new([]),
        static_methods: Box::new([]),
        static_fields: static_fields.into_boxed_slice(),
        properties: Box::new([]),
        attrs: Box::new([]),
        span,
    })
}
