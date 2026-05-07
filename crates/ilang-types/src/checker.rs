use std::collections::{HashMap, HashSet};

use ilang_ast::{
    Block, ClassDecl, CtorArgs, EnumDecl, Expr, ExprKind, FieldDecl, FnDecl, Item, Param,
    PatternBindings, PatternKind, Program, Span, Stmt, StmtKind, Symbol, Type, UnOp,
    VariantPayload,
};

use crate::error::TypeError;
use crate::ops::{assignable, bin_result, int_literal_fits};

/// Check whether a value expression can be assigned to a binding of type
/// `target`. In addition to the normal `assignable` rule, an integer
/// literal (or its unary negation) infers into any integer type whose
/// range it fits — this is what lets `let x: u8 = 5` work even though
/// the literal's natural type is i64.
/// `if` の枝合流専用の判定: 値式が **素の数値リテラル** (整数/浮動小数、
/// 任意で単項 `-`) で、`target` 型に収まるかどうか。`assignable` を経由
/// しないので i64 値→f64 のような暗黙拡張は通さない。
fn numeric_literal_fits(value: &Expr, target: &Type) -> bool {
    match &value.kind {
        ExprKind::Int(n) => {
            if target.is_int() {
                int_literal_fits(*n, target)
            } else {
                target.is_float()
            }
        }
        ExprKind::Unary { op: ilang_ast::UnOp::Neg, expr: inner } => {
            if let ExprKind::Int(n) = &inner.kind {
                if target.is_int() {
                    n.checked_neg().is_some_and(|v| int_literal_fits(v, target))
                } else {
                    target.is_float()
                }
            } else if matches!(inner.kind, ExprKind::Float(_)) {
                target.is_float()
            } else {
                false
            }
        }
        ExprKind::Float(_) => target.is_float(),
        _ => false,
    }
}

fn literal_assignable(value: &Expr, vt: &Type, target: &Type) -> bool {
    if assignable(vt, target) {
        return true;
    }
    // `let x: T? = literal` — auto-wrap. The literal is assignable to T?
    // iff it's assignable to the inner T (with literal coercions).
    if let Type::Optional(inner) = target {
        // `none` itself: vt = Optional<Any>, handled by `assignable`. For
        // `some(x)`, vt = Optional<U>, also handled there. The remaining
        // case is a bare literal that should coerce to the inner.
        if matches!(vt, Type::Optional(_)) {
            return false; // already handled above
        }
        return literal_assignable(value, vt, inner);
    }
    // Array literal → array type. Lets `let a: i32[] = [1, 2, 3]` work
    // even though the literal's natural element type is i64, and lets
    // `let a: i32[3] = [1, 2, 3]` match a fixed-length annotation.
    if let (
        ExprKind::Array(elements),
        Type::Array {
            elem: target_elem,
            fixed: target_fixed,
        },
    ) = (&value.kind, target)
    {
        if let Some(n) = target_fixed {
            if elements.len() != *n {
                return false;
            }
        }
        // Empty literal: element type doesn't matter, accept whatever the
        // annotation asks for (subject to the length check above).
        if elements.is_empty() {
            return true;
        }
        let vt_elem = match vt {
            Type::Array { elem, .. } => elem.clone(),
            _ => return false,
        };
        return elements
            .iter()
            .all(|e| literal_assignable(e, &vt_elem, target_elem));
    }
    // Tuple literal → tuple type. Pairwise check so element-level
    // literal coercions (int width narrowing, fixed → dynamic array)
    // apply inside tuples just like they do at the top level.
    if let (ExprKind::Tuple(elements), Type::Tuple(target_elems)) = (&value.kind, target) {
        if elements.len() != target_elems.len() {
            return false;
        }
        let vt_elems = match vt {
            Type::Tuple(es) => es,
            _ => return false,
        };
        return elements
            .iter()
            .zip(vt_elems.iter())
            .zip(target_elems.iter())
            .all(|((e, vt_e), tt_e)| literal_assignable(e, vt_e, tt_e));
    }
    if let ExprKind::Int(n) = &value.kind {
        if target.is_int() {
            return int_literal_fits(*n, target);
        }
        if target.is_float() {
            return true;
        }
    }
    if let ExprKind::Unary { op: ilang_ast::UnOp::Neg, expr: inner } = &value.kind {
        if let ExprKind::Int(n) = &inner.kind {
            if target.is_int() {
                return n.checked_neg().is_some_and(|v| int_literal_fits(v, target));
            }
            if target.is_float() {
                return true;
            }
        }
    }
    if let ExprKind::Float(_) = &value.kind {
        if target.is_float() {
            return true;
        }
    }
    false
}

#[derive(Debug, Clone)]
struct Signature {
    params: Vec<Type>,
    ret: Type,
    /// `true` for built-ins like `console.log` that accept any number of
    /// arguments (each typed as `Any`). User-defined variadics are not
    /// yet supported (parser doesn't accept `...args`).
    variadic: bool,
    /// Generic type parameters declared on the fn (e.g. `<T, U>`).
    /// Empty for non-generic fns. `params` / `ret` may reference these
    /// as `Type::TypeVar(name)`; concrete types are inferred from the
    /// arg expression types at each call site.
    type_params: Vec<Symbol>,
    /// Span of the original `FnDecl` this signature came from. Used by
    /// the post-typecheck mangler to find the right declaration when
    /// rewriting overloaded fn names. `Span::dummy()` for built-ins.
    #[allow(dead_code)]
    decl_span: Span,
    /// Default-value expressions for each parameter (`None` when the
    /// parameter has no default). Used at call sites to fill in
    /// missing trailing arguments. Always empty for built-ins and for
    /// the indirect-call path (no FnDecl behind it).
    defaults: Vec<Option<Expr>>,
}

#[derive(Debug, Clone, Default)]
struct ClassSig {
    /// Names of generic type parameters on the class. Empty for
    /// non-generic classes. Field/method types may reference these as
    /// `Type::TypeVar(name)`; instantiation substitutes them.
    type_params: Vec<Symbol>,
    fields: HashMap<Symbol, Type>,
    /// Methods grouped by source name, allowing overloads. Resolution
    /// at each MethodCall (and `new C(args)` for `init`) site picks
    /// the best match the same way top-level fn overloads do.
    methods: HashMap<Symbol, Vec<Signature>>,
    /// `get` / `set` accessors. `obj.x` reads dispatch through the
    /// getter, `obj.x = v` writes through the setter (when present).
    properties: HashMap<Symbol, PropertySig>,
    /// `static` methods — single sig per name (no overloading yet).
    /// Resolved at `ClassName.method(args)` call sites.
    static_methods: HashMap<Symbol, Signature>,
    /// `static` fields — class-level mutable storage. Read/write
    /// dispatched at `ClassName.field` field expressions.
    static_fields: HashMap<Symbol, Type>,
    /// Subset of `static_fields` declared with `const` (immutable —
    /// reassignment is rejected at type-check time).
    static_const_fields: HashSet<Symbol>,
    /// `extends Parent` — single-inheritance parent. None for root
    /// classes (or built-ins). Used by `is_subclass`, super
    /// resolution, and vtable layout.
    parent: Option<Symbol>,
    /// Per-method vtable slot index. Inherited methods keep the
    /// parent's slot; overrides reuse the same slot; new methods
    /// added in this class get fresh slots after the parent's last
    /// slot. The JIT reads this to lay out vtables.
    method_slots: HashMap<Symbol, usize>,
    /// Total number of vtable slots (= max slot index + 1, or 0).
    /// Equals parent's `vtable_len` plus this class's newly-added
    /// methods.
    vtable_len: usize,
    /// `Some(libname)` for `@extern("lib") class Foo {}` — the type
    /// is an opaque handle whose values come from native extern fns.
    /// `new`, fields, methods are all rejected on these.
    extern_lib: Option<Symbol>,
    /// `true` for `@extern(C) struct Foo { ... }`. Field-type validation
    /// (primitives + repr_c only) and embedded-struct layout depend
    /// on this flag.
    is_repr_c: bool,
    /// `true` when the class ends in a C99 flexible array member
    /// (`T[]` last field). `new ClassName(n)` accepts a single i64
    /// arg (the trailing element count) for these.
    has_fam: bool,
}

#[derive(Debug, Clone)]
struct PropertySig {
    ty: Type,
    has_get: bool,
    has_set: bool,
}

/// Type-checker view of an enum. Variants preserve declaration order so
/// the JIT can use the same indices as ordinal tags.
#[derive(Debug, Clone)]
struct EnumSig {
    /// Generic type parameters declared on the enum (mirrors
    /// `ClassSig.type_params`). Empty for non-generic enums.
    /// Variant payloads may reference these as `Type::TypeVar`.
    type_params: Vec<Symbol>,
    variants: Vec<EnumVariantSig>,
    /// `@flags` enum — supports `|` `&` `^` `~` and a `has` method.
    flags: bool,
}

#[derive(Debug, Clone)]
struct EnumVariantSig {
    name: Symbol,
    payload: VariantPayloadSig,
}

#[derive(Debug, Clone)]
enum VariantPayloadSig {
    Unit,
    Tuple(Vec<Type>),
    Struct(Vec<(Symbol, Type)>),
}

type Vars = HashMap<Symbol, Type>;

#[derive(Debug, Default)]
pub struct TypeChecker {
    /// Top-level functions, keyed by source name. A name maps to a
    /// non-empty vec because user code may define multiple
    /// overloads (`fn print(n: i64)` + `fn print(s: string)`). At each
    /// call site we pick the best match by arg-type scoring; if a name
    /// has just one entry we still go through the same path.
    fns: HashMap<Symbol, Vec<Signature>>,
    classes: HashMap<Symbol, ClassSig>,
    enums: HashMap<Symbol, EnumSig>,
    vars: HashMap<Symbol, Type>,
    /// Inferred type-argument vector for each generic-fn call site,
    /// keyed by the call expression's span. Populated during checking;
    /// consumed by the JIT's monomorphization pass. Values may contain
    /// `Type::TypeVar` when the call sits inside another generic
    /// context — the monomorphizer substitutes those at expansion time.
    /// Wrapped in `RefCell` because `check_expr` takes `&self`.
    fn_call_type_args: std::cell::RefCell<HashMap<Span, (Symbol, Vec<Type>)>>,
    /// Inferred type-arg vector for each generic-enum-ctor call site.
    /// Same shape as `fn_call_type_args`; consumed by the JIT's
    /// enum-monomorphization pass.
    enum_ctor_type_args: std::cell::RefCell<HashMap<Span, (Symbol, Vec<Type>)>>,
    /// Per-call-site choice when the callee is overloaded:
    /// `(name, index_into_self.fns[name])`. Used by the post-typecheck
    /// mangler to rewrite `Call.callee` to the per-overload mangled
    /// name when the name has more than one overload.
    fn_overload_pick: std::cell::RefCell<HashMap<Span, (Symbol, usize)>>,
    /// Per-call-site method overload pick. Same idea as
    /// `fn_overload_pick` but keyed for class methods. The triple is
    /// `(class_name, method_name, sig_idx)`. Includes both regular
    /// MethodCall sites and the `init` resolved at `new C(args)`.
    method_overload_pick: std::cell::RefCell<HashMap<Span, (Symbol, Symbol, usize)>>,
    /// Stack of currently-open loops, with the kind that controls
    /// whether `break v` is allowed and the accumulated break-value
    /// type so a `loop { ... break v }` expression can take the type of
    /// `v`. `LoopKind::Loop` collects break types; `LoopKind::Other`
    /// (while / for) rejects `break v` outright.
    loop_stack: std::cell::RefCell<Vec<LoopFrame>>,
    /// `true` while validating types or bodies inside an
    /// `@extern(C) { ... }` block. Allows raw C pointer / `void` /
    /// `char` / `size_t` / `ssize_t` types to appear; outside the
    /// block these types are rejected.
    in_extern_c: std::cell::RefCell<bool>,
    /// Per-`loop` expression: the unified break-value type that the
    /// loop evaluates to. Unit means no `break v` was seen. Consumed
    /// by the JIT lowering so it can allocate the right Cranelift
    /// `Variable` for the loop result.
    loop_break_type: std::cell::RefCell<HashMap<Span, Type>>,
    /// Per-`FnExpr` span: the list of (name, type) free variables
    /// the body captures from the enclosing scope. The JIT's hoist
    /// pass reads this to lay out closure environments. Order is
    /// stable (insertion order); the JIT uses it as the offset
    /// order in the closure struct.
    fn_expr_captures: std::cell::RefCell<HashMap<Span, Vec<(Symbol, Type)>>>,
    /// Used by the JIT's post-hoist re-typecheck: for each
    /// closure wrapper FnDecl, the body's "free vars" actually
    /// resolve to captured values. Pre-populating the body's
    /// scope with these makes the second-pass check pass without
    /// special-casing in the type checker proper.
    pub closure_wrapper_captures: HashMap<Symbol, Vec<(Symbol, Type)>>,
    /// Per-call-site default-arg fills: the trailing default
    /// expressions (already type-checked) that the post-typecheck
    /// pass must append to the Call's `args`. Keyed by the call
    /// expression's span.
    call_default_fills: std::cell::RefCell<HashMap<Span, Vec<Expr>>>,
}

#[derive(Debug)]
enum LoopFrame {
    /// `loop { ... }` — `break v` allowed; `Option<Type>` is the
    /// (unified) value type recorded so far, `None` until first break.
    Loop(Option<Type>),
    /// `while` / `for` — only bare `break` allowed.
    Other,
}

impl TypeChecker {
    pub fn new() -> Self {
        let mut tc = Self::default();
        tc.install_builtins();
        tc
    }

    /// Map of generic-fn call site → (callee name, inferred type args).
    /// Filled in during `check`; consumed by the JIT monomorphizer.
    pub fn fn_call_type_args(&self) -> HashMap<Span, (Symbol, Vec<Type>)> {
        self.fn_call_type_args.borrow().clone()
    }

    /// Map of generic-enum-ctor call site → (enum name, inferred type
    /// args). Same purpose as `fn_call_type_args` but for `Box.full(42)`
    /// style constructors.
    pub fn enum_ctor_type_args(&self) -> HashMap<Span, (Symbol, Vec<Type>)> {
        self.enum_ctor_type_args.borrow().clone()
    }

    /// Per-call-site overload pick: `(callee_name, sig_idx)`. Consumed
    /// by the post-typecheck `mangle_overloads` pass so it knows which
    /// of N same-name decls each call should resolve to.
    pub fn fn_overload_picks(&self) -> HashMap<Span, (Symbol, usize)> {
        self.fn_overload_pick.borrow().clone()
    }

    /// Per-call-site default-arg fills. Each entry is the (already
    /// type-checked) trailing default expressions to append to the
    /// Call's `args`. The post-typecheck mangler walks this to
    /// rewrite the AST so downstream passes see fully-positional
    /// calls.
    pub fn call_default_fills(&self) -> HashMap<Span, Vec<Expr>> {
        self.call_default_fills.borrow().clone()
    }

    /// Per-call-site method overload pick:
    /// `(class_name, method_name, sig_idx)`. Used by the mangler to
    /// rewrite `MethodCall.method` and `New.init_method`.
    pub fn method_overload_picks(&self) -> HashMap<Span, (Symbol, Symbol, usize)> {
        self.method_overload_pick.borrow().clone()
    }

    /// Map of `loop` expression span → the loop's result type (the
    /// unified `break v` value type, or `Unit` if no `break v`).
    /// Consumed by the JIT lowering.
    pub fn loop_break_types(&self) -> HashMap<Span, Type> {
        self.loop_break_type.borrow().clone()
    }

    /// Per-`FnExpr` span → captured (name, type) list. Empty list
    /// when the closure is purely top-level / no locals captured.
    pub fn fn_expr_captures(&self) -> HashMap<Span, Vec<(Symbol, Type)>> {
        self.fn_expr_captures.borrow().clone()
    }

    /// `(class, slot) -> method_name` for every class — used by the
    /// JIT to lay out per-class vtables. Empty for root classes
    /// without methods.
    pub fn class_method_slots(&self) -> HashMap<Symbol, HashMap<Symbol, usize>> {
        self.classes
            .iter()
            .map(|(n, sig)| (n.clone(), sig.method_slots.clone()))
            .collect()
    }

    /// `class -> vtable size` (max slot index + 1). Used by the JIT
    /// when allocating vtables.
    pub fn class_vtable_lens(&self) -> HashMap<Symbol, usize> {
        self.classes
            .iter()
            .map(|(n, sig)| (n.clone(), sig.vtable_len))
            .collect()
    }

    /// `class -> parent` (single-inheritance only). Empty for root
    /// classes. The JIT walks this for super-call resolution and
    /// vtable inheritance.
    pub fn class_parents(&self) -> HashMap<Symbol, Symbol> {
        self.classes
            .iter()
            .filter_map(|(n, sig)| sig.parent.map(|p| (*n, p)))
            .collect()
    }

    /// True iff `child` is `parent` or transitively descends from
    /// `parent` via `extends` chains. False if either name is
    /// unknown.
    fn is_subclass(&self, child: Symbol, parent: Symbol) -> bool {
        if child == parent {
            return true;
        }
        let mut cur = self.classes.get(&child).and_then(|c| c.parent);
        while let Some(name) = cur {
            if name == parent {
                return true;
            }
            cur = self.classes.get(&name).and_then(|c| c.parent);
        }
        false
    }

    /// Object-aware extension of `assignable`: returns true if the
    /// plain assignable check passes OR `from` is an object whose
    /// class is a (transitive) subclass of `to`'s class.
    fn assignable_obj(&self, from: &Type, to: &Type) -> bool {
        if assignable(from, to) {
            return true;
        }
        if let (Type::Object(c), Type::Object(p)) = (from, to) {
            return self.is_subclass(*c, *p);
        }
        false
    }

    /// When an EnumCtor's inferred type-args contain `Type::Any` (because
    /// only some of T/E were resolvable from the args alone), use the
    /// surrounding context's expected type to fill in the holes. This
    /// runs at let / return / tail positions so the JIT monomorphizer
    /// sees a fully concrete instantiation.
    fn refine_enum_ctor_args(&self, expr: &Expr, target: &Type) {
        let target_args = match target {
            Type::Generic(g) => Some((g.base.clone(), g.args.to_vec())),
            _ => None,
        };
        match &expr.kind {
            ExprKind::EnumCtor { enum_name, .. } => {
                if let Some((tbase, targs)) = &target_args {
                    if tbase == enum_name {
                        let mut tbl = self.enum_ctor_type_args.borrow_mut();
                        if let Some((_, recorded)) = tbl.get_mut(&expr.span) {
                            for (i, slot) in recorded.iter_mut().enumerate() {
                                if matches!(slot, Type::Any) {
                                    if let Some(t) = targs.get(i) {
                                        *slot = t.clone();
                                    }
                                }
                            }
                        }
                    }
                }
            }
            ExprKind::If { then_branch, else_branch, .. } => {
                self.refine_enum_ctor_args_in_block(then_branch, target);
                if let Some(e) = else_branch {
                    self.refine_enum_ctor_args(e, target);
                }
            }
            ExprKind::IfLet { then_branch, else_branch, .. } => {
                self.refine_enum_ctor_args_in_block(then_branch, target);
                if let Some(e) = else_branch {
                    self.refine_enum_ctor_args(e, target);
                }
            }
            ExprKind::Block(b) => self.refine_enum_ctor_args_in_block(b, target),
            ExprKind::Match { arms, .. } => {
                for arm in arms {
                    self.refine_enum_ctor_args(&arm.body, target);
                }
            }
            ExprKind::Return(Some(inner)) => self.refine_enum_ctor_args(inner, target),
            _ => {}
        }
    }

    fn refine_enum_ctor_args_in_block(&self, b: &ilang_ast::Block, target: &Type) {
        // Tail produces the block's value — refine against target.
        if let Some(t) = &b.tail {
            self.refine_enum_ctor_args(t, target);
        }
        // Return statements anywhere in the block also produce the
        // function's return value — refine those too.
        for s in &b.stmts {
            if let StmtKind::Expr(e) = &s.kind {
                refine_returns(self, e, target);
            }
        }
    }

    /// Pre-register the built-in `Console` class and the `console`
    /// singleton so `console.log(x)` type-checks for any `x`. Kept in one
    /// place so it's easy to grow with `console.error`, `console.warn`, etc.
    fn install_builtins(&mut self) {
        let mut methods = HashMap::new();
        methods.insert(
            "log".into(),
            vec![Signature {
                // No fixed prefix — variadic with arity 0+. Any
                // arg flows through unchecked.
                params: vec![],
                ret: Type::Unit,
                variadic: true, decl_span: Span::dummy(), type_params: Vec::new(), defaults: Vec::new(),
            }],
        );
        self.classes.insert(
            "Console".into(),
            ClassSig {
                type_params: Vec::new(),
                fields: HashMap::new(),
                methods,
                properties: HashMap::new(),
                static_methods: HashMap::new(),
                static_fields: HashMap::new(),
                static_const_fields: HashSet::new(),
                parent: None,
                method_slots: HashMap::new(),
                vtable_len: 0,
                extern_lib: None,
                is_repr_c: false,
                has_fam: false,
            },
        );
        self.vars
            .insert("console".into(), Type::Object("Console".into()));

        // Built-in `Map<K, V>` — generic class with no fields. Methods
        // are intercepted in the interpreter; the signatures here are
        // what the type checker enforces. Indexing (`m[k]` / `m[k] = v`)
        // is handled in the Index/AssignIndex arms by recognizing
        // `Type::Generic { Map, [K, V] }` receivers.
        let k = || Type::TypeVar("K".into());
        let v = || Type::TypeVar("V".into());
        let mut map_methods = HashMap::new();
        map_methods.insert(
            "init".into(),
            vec![Signature { params: vec![], ret: Type::Unit, variadic: false, decl_span: Span::dummy(), type_params: Vec::new(), defaults: Vec::new() }],
        );
        map_methods.insert(
            "get".into(),
            vec![Signature {
                params: vec![k()],
                ret: Type::Optional(Box::new(v())),
                variadic: false, decl_span: Span::dummy(), type_params: Vec::new(), defaults: Vec::new(),
            }],
        );
        map_methods.insert(
            "set".into(),
            vec![Signature { params: vec![k(), v()], ret: Type::Unit, variadic: false, decl_span: Span::dummy(), type_params: Vec::new(), defaults: Vec::new() }],
        );
        map_methods.insert(
            "has".into(),
            vec![Signature { params: vec![k()], ret: Type::Bool, variadic: false, decl_span: Span::dummy(), type_params: Vec::new(), defaults: Vec::new() }],
        );
        map_methods.insert(
            "delete".into(),
            vec![Signature { params: vec![k()], ret: Type::Bool, variadic: false, decl_span: Span::dummy(), type_params: Vec::new(), defaults: Vec::new() }],
        );
        map_methods.insert(
            "size".into(),
            vec![Signature { params: vec![], ret: Type::I64, variadic: false, decl_span: Span::dummy(), type_params: Vec::new(), defaults: Vec::new() }],
        );
        map_methods.insert(
            "keys".into(),
            vec![Signature {
                params: vec![],
                ret: Type::Array { elem: Box::new(k()), fixed: None },
                variadic: false, decl_span: Span::dummy(), type_params: Vec::new(), defaults: Vec::new(),
            }],
        );
        map_methods.insert(
            "values".into(),
            vec![Signature {
                params: vec![],
                ret: Type::Array { elem: Box::new(v()), fixed: None },
                variadic: false, decl_span: Span::dummy(), type_params: Vec::new(), defaults: Vec::new(),
            }],
        );
        self.classes.insert(
            "Map".into(),
            ClassSig {
                type_params: vec!["K".into(), "V".into()],
                fields: HashMap::new(),
                methods: map_methods,
                properties: HashMap::new(),
                static_methods: HashMap::new(),
                static_fields: HashMap::new(),
                static_const_fields: HashSet::new(),
                parent: None,
                method_slots: HashMap::new(),
                vtable_len: 0,
                extern_lib: None,
                is_repr_c: false,
                has_fam: false,
            },
        );

        // Built-in helpers callable inside `@extern(C) { ... }` blocks
        // for converting between raw C ABI values and ilang values.
        // Registered as top-level fns; their signatures use raw
        // pointer types so they're effectively only callable from
        // inside the block (outside the block, the user can't
        // construct a `*const char` to pass).
        let raw_const_char =
            Type::RawPtr { is_const: true, inner: Box::new(Type::CChar) };
        let raw_char = Type::RawPtr { is_const: false, inner: Box::new(Type::CChar) };
        let raw_const_void =
            Type::RawPtr { is_const: true, inner: Box::new(Type::CVoid) };
        let raw_void =
            Type::RawPtr { is_const: false, inner: Box::new(Type::CVoid) };
        let raw_const_const_char = Type::RawPtr {
            is_const: true,
            inner: Box::new(raw_const_char.clone()),
        };
        let mk_sig = |params: Vec<Type>, ret: Type, type_params: Vec<Symbol>| Signature {
            params,
            ret,
            variadic: false,
            decl_span: Span::dummy(),
            type_params,
            defaults: Vec::new(),
        };
        // stringFromCstr(p: *const char): string
        self.fns.insert(
            "stringFromCstr".into(),
            vec![mk_sig(vec![raw_const_char.clone()], Type::Str, Vec::new())],
        );
        // cstrFromString(s: string): *char
        self.fns.insert(
            "cstrFromString".into(),
            vec![mk_sig(vec![Type::Str], raw_char.clone(), Vec::new())],
        );
        // freeCstr(p: *char)
        self.fns.insert(
            "freeCstr".into(),
            vec![mk_sig(vec![raw_char.clone()], Type::Unit, Vec::new())],
        );
        // bytesFromBuffer(p: *const void, n: size_t): u8[]
        self.fns.insert(
            "bytesFromBuffer".into(),
            vec![mk_sig(
                vec![raw_const_void.clone(), Type::Size],
                Type::Array { elem: Box::new(Type::U8), fixed: None },
                Vec::new(),
            )],
        );
        // read{IN,UN,FN}(p: *const void, offset: i64): TN — alloc-free
        // primitive load at `p + offset` (offset is in BYTES). Mirrors
        // C99-style `*(TN*)((char*)p + offset)`. Caller is responsible
        // for alignment.
        for (name, ty) in [
            ("readI8", Type::I8),
            ("readI16", Type::I16),
            ("readI32", Type::I32),
            ("readI64", Type::I64),
            ("readU8", Type::U8),
            ("readU16", Type::U16),
            ("readU32", Type::U32),
            ("readU64", Type::U64),
            ("readF32", Type::F32),
            ("readF64", Type::F64),
        ] {
            self.fns.insert(
                name.into(),
                vec![mk_sig(
                    vec![raw_const_void.clone(), Type::I64],
                    ty,
                    Vec::new(),
                )],
            );
        }
        // write{IN,UN,FN}(p: *void, offset: i64, value: TN) — companion
        // store at `p + offset`. Same alignment caveat as the readers.
        for (name, ty) in [
            ("writeI8", Type::I8),
            ("writeI16", Type::I16),
            ("writeI32", Type::I32),
            ("writeI64", Type::I64),
            ("writeU8", Type::U8),
            ("writeU16", Type::U16),
            ("writeU32", Type::U32),
            ("writeU64", Type::U64),
            ("writeF32", Type::F32),
            ("writeF64", Type::F64),
        ] {
            self.fns.insert(
                name.into(),
                vec![mk_sig(
                    vec![raw_void.clone(), Type::I64, ty],
                    Type::Unit,
                    Vec::new(),
                )],
            );
        }
        // fnAddr(f): i64 — code-pointer of an ilang fn, suitable
        // for passing into C as a callback (e.g. SDL_AddTimer).
        // The callback's signature must already be C-ABI compatible
        // (numeric primitives + raw pointers); the caller is
        // responsible for that. Argument is type-checked as any
        // type via a free `F`; the JIT lowering enforces that the
        // expression is a bare fn name.
        self.fns.insert(
            "fnAddr".into(),
            vec![mk_sig(
                vec![Type::TypeVar("F".into())],
                Type::I64,
                vec!["F".into()],
            )],
        );
        // arrayFromCArray<T>(p: *const T, n: size_t): T[]
        // T is constrained to numeric primitive / bool at the call
        // site (the JIT lowering rejects other Ts since it would
        // need element-wise marshalling we don't ship).
        let t_var = Type::TypeVar("T".into());
        self.fns.insert(
            "arrayFromCArray".into(),
            vec![mk_sig(
                vec![
                    Type::RawPtr {
                        is_const: true,
                        inner: Box::new(t_var.clone()),
                    },
                    Type::Size,
                ],
                Type::Array { elem: Box::new(t_var), fixed: None },
                vec!["T".into()],
            )],
        );
        // cstrArrayToStrings(p: *const *const char): string[]
        self.fns.insert(
            "cstrArrayToStrings".into(),
            vec![mk_sig(
                vec![raw_const_const_char],
                Type::Array { elem: Box::new(Type::Str), fixed: None },
                Vec::new(),
            )],
        );
        // errnoCheck(rc: i32): i32?     — POSIX -1-on-failure, success branch
        // errnoCheckI64(rc: i64): i64?  — same shape for ssize_t-style
        self.fns.insert(
            "errnoCheck".into(),
            vec![mk_sig(
                vec![Type::I32],
                Type::Optional(Box::new(Type::I32)),
                Vec::new(),
            )],
        );
        self.fns.insert(
            "errnoCheckI64".into(),
            vec![mk_sig(
                vec![Type::I64],
                Type::Optional(Box::new(Type::I64)),
                Vec::new(),
            )],
        );

        // Built-in `Result<T, E>` — generic enum with `Ok(T)` and
        // `Err(E)` variants. Constructed via `Result::Ok(v)` /
        // `Result::Err(e)` and matched like any other enum.
        self.enums.insert(
            "Result".into(),
            EnumSig {
                type_params: vec!["T".into(), "E".into()],
                variants: vec![
                    EnumVariantSig {
                        name: "ok".into(),
                        payload: VariantPayloadSig::Tuple(vec![Type::TypeVar("T".into())]),
                    },
                    EnumVariantSig {
                        name: "err".into(),
                        payload: VariantPayloadSig::Tuple(vec![Type::TypeVar("E".into())]),
                    },
                ],
                flags: false,
            },
        );

        // Built-in RTTI: `Type` (returned by `typeof(x)`) plus the
        // `TypeKind` enum it exposes. Both are introspection-only and
        // user code can't construct or extend them.
        self.enums.insert(
            "TypeKind".into(),
            EnumSig {
                type_params: vec![],
                variants: vec![
                    EnumVariantSig { name: "primitive".into(), payload: VariantPayloadSig::Unit },
                    EnumVariantSig { name: "class".into(), payload: VariantPayloadSig::Unit },
                    EnumVariantSig { name: "enum".into(), payload: VariantPayloadSig::Unit },
                    EnumVariantSig { name: "optional".into(), payload: VariantPayloadSig::Unit },
                    EnumVariantSig { name: "array".into(), payload: VariantPayloadSig::Unit },
                    EnumVariantSig { name: "fn".into(), payload: VariantPayloadSig::Unit },
                    EnumVariantSig { name: "tuple".into(), payload: VariantPayloadSig::Unit },
                    EnumVariantSig { name: "string".into(), payload: VariantPayloadSig::Unit },
                    EnumVariantSig { name: "unit".into(), payload: VariantPayloadSig::Unit },
                ],
                flags: false,
            },
        );
        self.classes.insert(
            "Type".into(),
            ClassSig {
                type_params: Vec::new(),
                fields: HashMap::new(),
                methods: HashMap::new(),
                properties: HashMap::new(),
                static_methods: HashMap::new(),
                static_fields: HashMap::new(),
                static_const_fields: HashSet::new(),
                parent: None,
                method_slots: HashMap::new(),
                vtable_len: 0,
                extern_lib: None,
                is_repr_c: false,
                has_fam: false,
            },
        );

        // `typeof(x): Type` — global builtin. Polymorphic in arg type;
        // we register the variadic flag and special-case the call site
        // in check_expr to relax the param-type check.
        self.fns.insert(
            "typeof".into(),
            vec![Signature {
                params: vec![Type::Object("Type".into())], // placeholder; arg type is any
                ret: Type::Object("Type".into()),
                variadic: false,
                decl_span: Span::dummy(),
                type_params: Vec::new(),
                defaults: Vec::new(),
            }],
        );
    }

    pub fn check(&mut self, prog: &Program) -> Result<Type, TypeError> {
        // Pass 0: refuse to redefine built-in names. Otherwise a user
        // `class Console { ... }` would silently overwrite the built-in
        // signature and `console.log` would call the user code.
        for item in &prog.items {
            match item {
                Item::Class(c) if is_reserved_class(c.name.as_str()) => {
                    return Err(TypeError::ReservedName {
                        name: c.name.clone(),
                        span: c.span,
                    });
                }
                Item::Enum(e) if is_reserved_class(e.name.as_str()) => {
                    return Err(TypeError::ReservedName {
                        name: e.name.clone(),
                        span: e.span,
                    });
                }
                _ => {}
            }
        }
        for item in &prog.items {
            match item {
                Item::Fn(f) => {
                    let sig = signature_of(f);
                    let entry = self.fns.entry(f.name.clone()).or_default();
                    // Reject (a) generic + non-generic same name and
                    // (b) two overloads with identical param types.
                    // (a) keeps overload resolution simple — generic
                    // resolution is already its own special path, so we
                    // require a name to be EITHER one generic fn OR
                    // a set of non-generic overloads.
                    let any_generic = !sig.type_params.is_empty()
                        || entry.iter().any(|s| !s.type_params.is_empty());
                    if any_generic && !entry.is_empty() {
                        return Err(TypeError::Unsupported {
                            what: format!(
                                "fn {:?} mixes a generic declaration with another overload — \
                                 generic functions cannot share a name with other fns",
                                f.name
                            ),
                            span: f.span,
                        });
                    }
                    if entry.iter().any(|s| s.params == sig.params) {
                        return Err(TypeError::Unsupported {
                            what: format!(
                                "fn {:?} has a duplicate overload (same parameter types as a \
                                 previous declaration)",
                                f.name
                            ),
                            span: f.span,
                        });
                    }
                    entry.push(sig);
                }
                Item::Class(c) => {
                    // Resolve parent (must be already registered).
                    let parent_sig = if let Some(pname) = &c.parent {
                        Some(self.classes.get(&pname).cloned().ok_or_else(|| {
                            TypeError::UndefinedClass {
                                name: pname.clone(),
                                span: c.span,
                            }
                        })?)
                    } else {
                        None
                    };
                    let sig = class_signature(c, parent_sig.as_ref())?;
                    self.classes.insert(c.name.clone(), sig);
                }
                Item::Enum(e) => {
                    let sig = enum_signature(e);
                    self.enums.insert(e.name.clone(), sig);
                }
                // The loader replaces Use items with their resolved
                // contents before type checking; any Use that survives
                // here was emitted by something that bypassed the
                // loader, and silently ignoring it is fine — there's
                // nothing to check.
                Item::Use(_) => {}
                // Const items are inlined by the loader's substitution
                // pass — they shouldn't appear here in the normal
                // pipeline. Skip if any survives.
                Item::Const(_) => {}
                Item::ExternStatic(s) => {
                    // Restrict to numeric / bool. The dlsym address
                    // gives back a raw pointer; the JIT loads/stores
                    // a fixed-width value through it. Strings, arrays,
                    // structs would need marshalling we don't ship
                    // for globals yet.
                    let ok = matches!(
                        &s.ty,
                        Type::I8 | Type::I16 | Type::I32 | Type::I64
                        | Type::U8 | Type::U16 | Type::U32 | Type::U64
                        | Type::F32 | Type::F64
                        | Type::Bool
                    );
                    if !ok {
                        return Err(TypeError::Unsupported {
                            what: format!(
                                "@extern static {:?}: type {} not supported \
                                 (allowed: numeric primitives or bool)",
                                s.name, s.ty
                            ),
                            span: s.span,
                        });
                    }
                    // Register as a typed global so `errno = 5` and
                    // `let x = errno` resolve.
                    self.vars.insert(s.name.clone(), s.ty.clone());
                }
                Item::ExternC(block) => {
                    // Walk the block's items in extern_c context so
                    // raw pointer / C-only types are accepted.
                    *self.in_extern_c.borrow_mut() = true;
                    let result = self.collect_extern_c_signatures(block);
                    *self.in_extern_c.borrow_mut() = false;
                    result?;
                }
            }
        }
        for item in &prog.items {
            match item {
                Item::Fn(f) => self.check_fn(f, None)?,
                Item::Class(c) => self.check_class(c)?,
                Item::Enum(e) => self.check_enum(e)?,
                Item::Use(_) | Item::Const(_) | Item::ExternStatic(_) => {}
                Item::ExternC(block) => {
                    *self.in_extern_c.borrow_mut() = true;
                    let result = self.check_extern_c_bodies(block);
                    *self.in_extern_c.borrow_mut() = false;
                    result?;
                }
            }
        }

        let mut env: Vars = self.vars.clone();
        let mut last = Type::Unit;
        for s in &prog.stmts {
            // Refuse to redefine built-in globals at top level so a
            // wayward `let console = ...` cannot disable `console.log`.
            // Inner-scope shadowing is still allowed.
            if let StmtKind::Let { name, .. } = &s.kind {
                if is_reserved_global(name.as_str()) {
                    return Err(TypeError::ReservedName {
                        name: name.clone(),
                        span: s.span,
                    });
                }
            }
            last = self.check_stmt(s, &mut env, None, None, 0)?;
        }
        if let Some(t) = &prog.tail {
            last = self.check_expr(t, &env, None, None, 0)?;
        }
        self.vars = env;
        Ok(last)
    }

    /// Walk an `@extern(C) { ... }` block during signature collection.
    /// Each inner item registers into the same tables `Item::Class` /
    /// `Item::Fn` would write to, but with the C-ABI flags pre-set.
    /// Caller has already set `self.in_extern_c = true`.
    fn collect_extern_c_signatures(
        &mut self,
        block: &ilang_ast::ExternCBlock,
    ) -> Result<(), TypeError> {
        for item in &block.items {
            match item {
                ilang_ast::ExternCItem::Struct {
                    name,
                    fields,
                    is_packed,
                    span,
                } => {
                    let synth = ClassDecl {
                        name: name.clone(),
                        type_params: Box::new([]),
                        parent: None,
                        fields: fields.clone(),
                        methods: Box::new([]),
                        static_methods: Box::new([]),
                        static_fields: Box::new([]),
                        properties: Box::new([]),
                        extern_lib: None,
                        is_repr_c: true,
                        is_packed: *is_packed,
                        is_union: false,
                        span: *span,
                    };
                    let sig = class_signature(&synth, None)?;
                    self.classes.insert(name.clone(), sig);
                }
                ilang_ast::ExternCItem::Union { name, fields, span } => {
                    let synth = ClassDecl {
                        name: name.clone(),
                        type_params: Box::new([]),
                        parent: None,
                        fields: fields.clone(),
                        methods: Box::new([]),
                        static_methods: Box::new([]),
                        static_fields: Box::new([]),
                        properties: Box::new([]),
                        extern_lib: None,
                        is_repr_c: true,
                        is_packed: false,
                        is_union: true,
                        span: *span,
                    };
                    let sig = class_signature(&synth, None)?;
                    self.classes.insert(name.clone(), sig);
                }
                ilang_ast::ExternCItem::FnDecl { name, params, ret, variadic, span, .. } => {
                    // Build a synthetic FnDecl with @extern attribute
                    // so downstream pipeline (loader, JIT) treats it
                    // like an existing top-level extern fn.
                    let mut extern_args = vec![ilang_ast::AttrArg::Path(Box::new([Symbol::intern("C")]))];
                    if *variadic {
                        extern_args.push(ilang_ast::AttrArg::Path(Box::new([Symbol::intern("variadic")])));
                    }
                    let attrs = vec![ilang_ast::Attribute {
                        name: "extern".into(),
                        args: extern_args.into(),
                    }];
                    let synth = FnDecl {
                        attrs: attrs.into(),
                        name: name.clone(),
                        type_params: Box::new([]),
                        params: params.clone(),
                        ret: ret.clone(),
                        body: ilang_ast::Block { stmts: Vec::new(), tail: None },
                        span: *span,
                        is_override: false,
                    };
                    let sig = signature_of(&synth);
                    self.fns.entry(name.clone()).or_default().push(sig);
                }
                ilang_ast::ExternCItem::FnDef(f) => {
                    let sig = signature_of(f);
                    self.fns.entry(f.name.clone()).or_default().push(sig);
                }
                ilang_ast::ExternCItem::Static { name, ty, span, .. } => {
                    let ok = matches!(
                        ty,
                        Type::I8 | Type::I16 | Type::I32 | Type::I64
                        | Type::U8 | Type::U16 | Type::U32 | Type::U64
                        | Type::F32 | Type::F64
                        | Type::Bool
                    );
                    if !ok {
                        return Err(TypeError::Unsupported {
                            what: format!(
                                "@extern(C) static {:?}: type {} not supported \
                                 (allowed: numeric primitives or bool)",
                                name, ty
                            ),
                            span: *span,
                        });
                    }
                    self.vars.insert(name.clone(), ty.clone());
                }
                ilang_ast::ExternCItem::Class(c) => {
                    let sig = class_signature(c, None)?;
                    self.classes.insert(c.name.clone(), sig);
                }
            }
        }
        Ok(())
    }

    /// Type-check `match` over a primitive scrutinee (integer /
    /// bool / string). Each arm's pattern must be a literal of the
    /// same shape, the wildcard `_`, or — for bool scrutinees —
    /// a `Variant` pattern whose name parses as `true` / `false`.
    /// A wildcard arm is mandatory: literal patterns can never be
    /// proven exhaustive over a primitive.
    fn check_match_primitive(
        &self,
        st: &Type,
        arms: &[ilang_ast::MatchArm],
        match_span: Span,
        env: &Vars,
        ret_ty: Option<&Type>,
        in_class: Option<Symbol>,
        loop_depth: u32,
    ) -> Result<Type, TypeError> {
        let mut has_wildcard = false;
        let mut bool_true_covered = false;
        let mut bool_false_covered = false;
        let mut result_ty: Option<Type> = None;
        for arm in arms {
            if has_wildcard {
                return Err(TypeError::Unsupported {
                    what: "match arm after wildcard `_` is unreachable".into(),
                    span: arm.span,
                });
            }
            let pspan = arm.pattern.span;
            let ok = match &arm.pattern.kind {
                PatternKind::Wildcard => {
                    has_wildcard = true;
                    true
                }
                PatternKind::IntLit(_) => st.is_numeric(),
                PatternKind::IntRange { low, high, inclusive } => {
                    if !st.is_numeric() {
                        false
                    } else if *low > *high || (!*inclusive && *low == *high) {
                        return Err(TypeError::Unsupported {
                            what: format!(
                                "empty integer range pattern `{low}{}{high}`",
                                if *inclusive { "..=" } else { ".." }
                            ),
                            span: pspan,
                        });
                    } else {
                        true
                    }
                }
                PatternKind::BoolLit(p) => {
                    if *st == Type::Bool {
                        if *p { bool_true_covered = true; } else { bool_false_covered = true; }
                        true
                    } else { false }
                }
                PatternKind::StrLit(_) => *st == Type::Str,
                // Bare `true` / `false` arrive from the parser as a
                // unit `Variant{name:"true"|"false"}`. Accept them
                // when matching a bool scrutinee.
                PatternKind::Variant { enum_name: None, variant, bindings: ilang_ast::PatternBindings::Unit }
                    if *st == Type::Bool && (variant == "true" || variant == "false") => {
                    if variant == "true" { bool_true_covered = true; } else { bool_false_covered = true; }
                    true
                }
                _ => false,
            };
            if !ok {
                return Err(TypeError::Unsupported {
                    what: format!(
                        "pattern type doesn't match scrutinee `{st}`"
                    ),
                    span: pspan,
                });
            }
            let bt = self.check_expr(&arm.body, env, ret_ty, in_class, loop_depth)?;
            result_ty = Some(match result_ty {
                None => bt,
                Some(prev) => unify_branch(prev, bt, arm.body.span)?,
            });
        }
        // Bool is the only primitive whose value space is enumerable
        // — `true` + `false` together count as exhaustive, no `_`
        // arm needed.
        let bool_exhaustive =
            *st == Type::Bool && bool_true_covered && bool_false_covered;
        if !has_wildcard && !bool_exhaustive {
            return Err(TypeError::Unsupported {
                what: format!(
                    "non-exhaustive match on `{st}`: literal patterns require a `_` wildcard arm"
                ),
                span: match_span,
            });
        }
        Ok(result_ty.unwrap_or(Type::Unit))
    }

    /// Type-check fn bodies inside an `@extern(C) { ... }` block.
    /// Caller has already set `self.in_extern_c = true`.
    fn check_extern_c_bodies(
        &mut self,
        block: &ilang_ast::ExternCBlock,
    ) -> Result<(), TypeError> {
        for item in &block.items {
            match item {
                ilang_ast::ExternCItem::FnDef(f) => {
                    self.reject_pointer_in_signature(
                        &format!("fn {:?}", f.name),
                        f.params.iter().map(|p| &p.ty),
                        f.ret.as_ref(),
                        f.span,
                    )?;
                    self.check_fn(f, None)?;
                }
                ilang_ast::ExternCItem::Class(c) => {
                    for m in &c.methods {
                        self.reject_pointer_in_signature(
                            &format!("method {:?}.{:?}", c.name, m.name),
                            m.params.iter().map(|p| &p.ty),
                            m.ret.as_ref(),
                            m.span,
                        )?;
                    }
                    for m in &c.static_methods {
                        self.reject_pointer_in_signature(
                            &format!("static {:?}.{:?}", c.name, m.name),
                            m.params.iter().map(|p| &p.ty),
                            m.ret.as_ref(),
                            m.span,
                        )?;
                    }
                    self.check_class(c)?;
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Walks `params` + `ret` of an ilang-side fn declared inside an
    /// `@extern(C) { ... }` block (i.e. no `@lib(...)`) and rejects
    /// any raw-pointer type — directly or via a `@extern(C) struct`
    /// field that contains one. Raw pointers are meant to stay
    /// inside the FFI block; if a wrapper exposes them, ilang user
    /// code outside the block has no safe way to handle the value.
    fn reject_pointer_in_signature<'a>(
        &self,
        what: &str,
        params: impl IntoIterator<Item = &'a Type>,
        ret: Option<&Type>,
        span: Span,
    ) -> Result<(), TypeError> {
        let mut visiting: HashSet<Symbol> = HashSet::new();
        for p in params {
            if let Some(reason) = self.find_raw_pointer(p, &mut visiting) {
                return Err(TypeError::Unsupported {
                    what: format!(
                        "{what}: parameter of type `{p}` exposes a raw pointer ({reason}). \
                         Raw pointers are not allowed in ilang-side wrappers — keep them \
                         inside @lib(...) declarations."
                    ),
                    span,
                });
            }
        }
        if let Some(r) = ret {
            if let Some(reason) = self.find_raw_pointer(r, &mut visiting) {
                return Err(TypeError::Unsupported {
                    what: format!(
                        "{what}: return type `{r}` exposes a raw pointer ({reason}). \
                         Raw pointers are not allowed in ilang-side wrappers — keep them \
                         inside @lib(...) declarations."
                    ),
                    span,
                });
            }
        }
        Ok(())
    }

    /// Returns `Some(reason)` if `t` is a raw pointer or transitively
    /// references one through a `@extern(C) struct` field. `visiting`
    /// breaks cycles in mutually-referencing structs.
    fn find_raw_pointer(
        &self,
        t: &Type,
        visiting: &mut HashSet<Symbol>,
    ) -> Option<String> {
        match t {
            Type::RawPtr { .. } => Some(format!("`{t}`")),
            Type::Array { elem, .. } => self.find_raw_pointer(elem, visiting),
            Type::Optional(inner) | Type::Weak(inner) => {
                self.find_raw_pointer(inner, visiting)
            }
            Type::Tuple(items) => items
                .iter()
                .find_map(|x| self.find_raw_pointer(x, visiting)),
            Type::Generic(g) => g
                .args
                .iter()
                .find_map(|a| self.find_raw_pointer(a, visiting)),
            Type::Fn(ft) => ft
                .params
                .iter()
                .find_map(|p| self.find_raw_pointer(p, visiting))
                .or_else(|| self.find_raw_pointer(&ft.ret, visiting)),
            Type::Object(name) => {
                if !visiting.insert(name.clone()) {
                    return None;
                }
                let res = self.classes.get(&name).and_then(|cs| {
                    if !cs.is_repr_c {
                        return None;
                    }
                    cs.fields.iter().find_map(|(fname, fty)| {
                        self.find_raw_pointer(fty, visiting).map(|inner| {
                            format!("{name}.{fname}: {inner}")
                        })
                    })
                });
                visiting.remove(name);
                res
            }
            _ => None,
        }
    }

    fn check_enum(&self, e: &EnumDecl) -> Result<(), TypeError> {
        // Validate every payload type now that all class/enum names are
        // known. Duplicate variant names are rejected — they'd make
        // pattern matching ambiguous.
        let mut seen = std::collections::HashSet::new();
        for v in &e.variants {
            if !seen.insert(v.name.clone()) {
                return Err(TypeError::Unsupported {
                    what: format!("duplicate variant {:?} in enum {:?}", v.name, e.name),
                    span: v.span,
                });
            }
            match &v.payload {
                VariantPayload::Unit => {}
                VariantPayload::Tuple(tys) => {
                    for t in tys {
                        self.validate_type(t, v.span, &e.type_params)?;
                    }
                }
                VariantPayload::Struct(fields) => {
                    let mut fseen = std::collections::HashSet::new();
                    for f in fields {
                        if !fseen.insert(f.name.clone()) {
                            return Err(TypeError::Unsupported {
                                what: format!(
                                    "duplicate field {:?} in {}::{}",
                                    f.name, e.name, v.name
                                ),
                                span: f.span,
                            });
                        }
                        self.validate_type(&f.ty, f.span, &e.type_params)?;
                    }
                }
            }
        }
        Ok(())
    }

    fn check_class(&self, c: &ClassDecl) -> Result<(), TypeError> {
        for FieldDecl { ty, span, .. } in &c.fields {
            self.validate_type(ty, *span, &c.type_params)?;
        }
        // `@extern(C) struct`es must have C-compatible field types so
        // the in-memory bytes line up with what native code expects.
        // Allowed: numeric primitives, bool, and other `@extern(C) struct`
        // classes (which embed inline). Reject ARC types, regular
        // classes (heap-managed), arrays, optional, etc.
        if !c.is_repr_c {
            for f in &c.fields {
                if f.bits.is_some() {
                    return Err(TypeError::Unsupported {
                        what: format!(
                            "@bits on field {:?} of class {:?}: bitfields are \
                             only supported inside `@extern(C) struct`es",
                            f.name, c.name
                        ),
                        span: f.span,
                    });
                }
            }
        }
        if c.is_repr_c {
            // `@extern(C) union` extra restrictions: every field
            // shares offset 0 so writing one overwrites the others.
            // Heap fields (string / object / array) would leak or
            // dangle when the storage is reused, so reject them.
            // FAM / bitfields don't make sense for unions.
            if c.is_union {
                if c.fields.is_empty() {
                    return Err(TypeError::Unsupported {
                        what: format!(
                            "@extern(C) union {:?}: union must have at \
                             least one field",
                            c.name
                        ),
                        span: c.span,
                    });
                }
                for f in &c.fields {
                    if f.bits.is_some() {
                        return Err(TypeError::Unsupported {
                            what: format!(
                                "@bits on union field {:?}: bitfields aren't \
                                 supported inside `@extern(C) union` classes",
                                f.name
                            ),
                            span: f.span,
                        });
                    }
                    let union_ok = matches!(
                        &f.ty,
                        Type::I8 | Type::I16 | Type::I32 | Type::I64
                        | Type::U8 | Type::U16 | Type::U32 | Type::U64
                        | Type::F32 | Type::F64
                        | Type::Bool
                    ) || matches!(&f.ty, Type::Array { elem, fixed: Some(_) }
                        if matches!(elem.as_ref(),
                            Type::I8 | Type::I16 | Type::I32 | Type::I64
                            | Type::U8 | Type::U16 | Type::U32 | Type::U64
                            | Type::F32 | Type::F64 | Type::Bool));
                    if !union_ok {
                        return Err(TypeError::Unsupported {
                            what: format!(
                                "@extern(C) union {:?} field {:?}: type {} \
                                 not supported (allowed inside a union: numeric \
                                 primitives / bool / fixed-length numeric array \
                                 `T[N]`. Heap types and nested aggregates aren't \
                                 safe under shared storage)",
                                c.name, f.name, f.ty
                            ),
                            span: f.span,
                        });
                    }
                }
            }
            for (i, f) in c.fields.iter().enumerate() {
                let is_last = i + 1 == c.fields.len();
                let primitive_ok = |t: &Type| {
                    matches!(
                        t,
                        Type::I8 | Type::I16 | Type::I32 | Type::I64
                        | Type::U8 | Type::U16 | Type::U32 | Type::U64
                        | Type::F32 | Type::F64
                        | Type::Bool
                    )
                };
                let ok = match &f.ty {
                    t if primitive_ok(t) => true,
                    Type::Object(name) => self
                        .classes
                        .get(name)
                        .map(|cs| cs.is_repr_c)
                        .unwrap_or(false),
                    // Fixed-length numeric arrays — `u8[64]`,
                    // `i32[4]` etc — are laid out inline (no
                    // heap allocation, no ARC).
                    Type::Array { elem, fixed: Some(_) } if primitive_ok(elem) => true,
                    // C99 flexible array member: `T[]` (no length) as
                    // the **last** field. Allocation size is set by
                    // `new ClassName(n)`. Bounds checks are skipped
                    // (the user maintains the count, just like in C).
                    Type::Array { elem, fixed: None } if is_last && primitive_ok(elem) => true,
                    // Owned C-string slot (`char *`) — class manages
                    // the malloc'd buffer on assign / drop.
                    Type::Str => true,
                    _ => false,
                };
                if let Some(bits) = f.bits {
                    let max = match &f.ty {
                        Type::U8 => 8u32,
                        Type::U16 => 16,
                        Type::U32 => 32,
                        Type::U64 => 64,
                        _ => {
                            return Err(TypeError::Unsupported {
                                what: format!(
                                    "@bits on field {:?} of class {:?}: bitfields are \
                                     only supported on unsigned integer types \
                                     (u8/u16/u32/u64), got {}",
                                    f.name, c.name, f.ty
                                ),
                                span: f.span,
                            });
                        }
                    };
                    if bits == 0 || bits > max {
                        return Err(TypeError::Unsupported {
                            what: format!(
                                "@bits({}) on field {:?} of class {:?}: width must be \
                                 in 1..={} for {}",
                                bits, f.name, c.name, max, f.ty
                            ),
                            span: f.span,
                        });
                    }
                }
                if !ok {
                    return Err(TypeError::Unsupported {
                        what: format!(
                            "@extern(C) struct {:?} field {:?}: type {} not supported \
                             (allowed: numeric primitives / bool / str (owned C-string) / \
                             other @extern(C) struct / fixed-length primitive array `T[N]`)",
                            c.name, f.name, f.ty
                        ),
                        span: f.span,
                    });
                }
            }
        }
        for m in &c.methods {
            // `deinit` is the destructor: zero params, no return value (or
            // explicit Unit). Anything else would be a footgun since the
            // runtime calls it with no arguments and discards the result.
            if m.name == "deinit"
                && (!m.params.is_empty()
                    || matches!(&m.ret, Some(t) if *t != Type::Unit))
            {
                return Err(TypeError::BadDeinitSignature { span: m.span });
            }
            self.check_fn(m, Some(c.name))?;
        }
        for prop in &c.properties {
            self.validate_type(&prop.ty, prop.span, &c.type_params)?;
            if let Some(g) = &prop.getter {
                self.check_fn(g, Some(c.name))?;
            }
            if let Some(s) = &prop.setter {
                self.check_fn(s, Some(c.name))?;
            }
        }
        // Static methods don't have `this` — pass `in_class=None` so
        // their bodies fail to resolve `this` / implicit field refs.
        for m in &c.static_methods {
            self.check_fn(m, None)?;
        }
        // Static field initializers were already folded to literals
        // by the loader. Just verify each one's type matches.
        let env: Vars = HashMap::new();
        for sf in &c.static_fields {
            self.validate_type(&sf.ty, sf.span, &c.type_params)?;
            let vt = self.check_expr(&sf.value, &env, None, None, 0)?;
            if !literal_assignable(&sf.value, &vt, &sf.ty) && !self.assignable_obj(&vt, &sf.ty) {
                return Err(TypeError::Mismatch {
                    expected: sf.ty.clone(),
                    got: vt,
                    span: sf.value.span,
                });
            }
        }
        Ok(())
    }

    fn check_fn(&self, f: &FnDecl, in_class: Option<Symbol>) -> Result<(), TypeError> {
        // Type parameters in scope: the class's (if we're inside a
        // generic class) plus the fn's own `<T, U>`.
        let mut params_in_scope: Vec<Symbol> = in_class
            .and_then(|n| self.classes.get(&n))
            .map(|c| c.type_params.clone())
            .unwrap_or_default();
        params_in_scope.extend(f.type_params.iter().cloned());
        let class_params = params_in_scope;
        for Param { ty, span, .. } in &f.params {
            self.validate_type(ty, *span, &class_params)?;
        }
        if let Some(ret) = &f.ret {
            self.validate_type(ret, f.span, &class_params)?;
        }
        // `@extern` fns have no body — the runtime supplies the
        // implementation. Skip the body check; the signature is the
        // contract and the runtime is responsible for honoring it.
        if f.attrs.iter().any(|a| a.name == "extern") {
            return Ok(());
        }
        // Seed the body env with module-level globals (the built-in
        // `console` singleton plus any `static` declarations from
        // `@extern(C) {}` blocks). Top-level `let` bindings are NOT
        // here yet at fn-body-check time — they get checked after
        // all fn bodies, matching most module systems.
        let mut env: Vars = self.vars.clone();
        // Closure wrapper: the body's "free" vars actually resolve
        // to captured values. Pre-populate the env with their
        // declared types so the body type-checks. Used by the
        // JIT's post-hoist re-typecheck.
        if let Some(captures) = self.closure_wrapper_captures.get(&f.name) {
            for (n, t) in captures {
                env.insert(n.clone(), t.clone());
            }
        }
        for Param { name, ty, .. } in &f.params {
            // Rewrite Object(T) → TypeVar(T) so the body checker treats
            // references to T as the type variable (not an unknown class).
            env.insert(name.clone(), rewrite_type_params(ty, &class_params));
        }
        let expected = rewrite_type_params(
            &f.ret.clone().unwrap_or(Type::Unit),
            &class_params,
        );
        // Function bodies start a fresh loop-stack: a `break` inside a
        // closure / nested fn body never refers to an outer loop.
        let saved_loops = std::mem::take(&mut *self.loop_stack.borrow_mut());
        let body_res = self.check_block(&f.body, &env, Some(&expected), in_class, 0);
        *self.loop_stack.borrow_mut() = saved_loops;
        let body_ty = body_res?;
        if !assignable(&body_ty, &expected) && !self.assignable_obj(&body_ty, &expected) {
            return Err(TypeError::BadReturn {
                name: f.name.clone(),
                expected,
                got: body_ty,
                span: f.span,
            });
        }
        // Refine enum-ctor entries in tail / Return sites against the
        // declared return type. See the equivalent in `check_stmt`'s
        // Let arm.
        self.refine_enum_ctor_args_in_block(&f.body, &expected);
        Ok(())
    }

    fn validate_type(
        &self,
        t: &Type,
        span: Span,
        type_params_in_scope: &[Symbol],
    ) -> Result<(), TypeError> {
        match t {
            Type::Object(name) => {
                // An identifier may refer to either a class, an enum,
                // or — when checking a generic class body — one of the
                // class's type parameters. `Type::Enum` only exists
                // when the checker resolved it explicitly (currently
                // unused — the parser produces `Object(name)` for both
                // classes and enums).
                if self.classes.contains_key(name)
                    || self.enums.contains_key(name)
                    || type_params_in_scope.iter().any(|p| p == name)
                {
                    // ok
                } else {
                    return Err(TypeError::UndefinedClass {
                        name: name.clone(),
                        span,
                    });
                }
            }
            Type::Enum(name) => {
                if !self.enums.contains_key(name) {
                    return Err(TypeError::UndefinedClass {
                        name: name.clone(),
                        span,
                    });
                }
            }
            Type::Array { elem, .. } => {
                self.validate_type(elem, span, type_params_in_scope)?;
            }
            Type::Optional(inner) => {
                self.validate_type(inner, span, type_params_in_scope)?;
            }
            Type::Weak(inner) => {
                // Weak is meaningful only for class instances. Reject
                // `string.weak`, `i64.weak`, etc. up front.
                if !matches!(inner.as_ref(), Type::Object(_)) {
                    return Err(TypeError::Unsupported {
                        what: format!("weak reference of {inner} (only class types allowed)"),
                        span,
                    });
                }
                self.validate_type(inner, span, type_params_in_scope)?;
            }
            // Raw C pointer / void / char / size_t / ssize_t — only
            // nameable inside an `@extern(C) { ... }` block.
            Type::RawPtr { inner, .. } => {
                if !*self.in_extern_c.borrow() {
                    return Err(TypeError::Unsupported {
                        what: format!(
                            "{t} (raw C pointer types are only nameable inside an @extern(C) {{ ... }} block)"
                        ),
                        span,
                    });
                }
                self.validate_type(inner, span, type_params_in_scope)?;
            }
            Type::CVoid | Type::CChar | Type::Size | Type::SSize => {
                if !*self.in_extern_c.borrow() {
                    return Err(TypeError::Unsupported {
                        what: format!(
                            "{t} (C-only type, nameable only inside an @extern(C) {{ ... }} block)"
                        ),
                        span,
                    });
                }
            }
            Type::Tuple(elems) => {
                for e in elems {
                    self.validate_type(e, span, type_params_in_scope)?;
                }
            }
            Type::Fn(ft) => {
                for p in &ft.params {
                    self.validate_type(p, span, type_params_in_scope)?;
                }
                self.validate_type(&ft.ret, span, type_params_in_scope)?;
            }
            Type::Generic(g) => {
                for a in &g.args {
                    self.validate_type(a, span, type_params_in_scope)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn check_block(
        &self,
        block: &Block,
        outer: &Vars,
        ret_ty: Option<&Type>,
        in_class: Option<Symbol>,
        loop_depth: u32,
    ) -> Result<Type, TypeError> {
        let mut env = outer.clone();
        let mut last = Type::Unit;
        for s in &block.stmts {
            last = self.check_stmt(s, &mut env, ret_ty, in_class, loop_depth)?;
        }
        if let Some(t) = &block.tail {
            last = self.check_expr(t, &env, ret_ty, in_class, loop_depth)?;
        }
        Ok(last)
    }

    fn check_stmt(
        &self,
        stmt: &Stmt,
        env: &mut Vars,
        ret_ty: Option<&Type>,
        in_class: Option<Symbol>,
        loop_depth: u32,
    ) -> Result<Type, TypeError> {
        match &stmt.kind {
            StmtKind::Let { name, ty, value } => {
                // `let a = []` cannot pick an element type. Force the
                // user to annotate before we lose the chance to do so.
                if ty.is_none() {
                    if let ExprKind::Array(elements) = &value.kind {
                        if elements.is_empty() {
                            return Err(TypeError::EmptyArrayNeedsAnnotation {
                                span: value.span,
                            });
                        }
                    }
                }
                let vt = self.check_expr(value, env, ret_ty, in_class, loop_depth)?;
                let bind = match ty {
                    Some(ann) => {
                        self.validate_type(ann, stmt.span, &[])?;
                        if !literal_assignable(value, &vt, ann) {
                            return Err(TypeError::Mismatch {
                                expected: ann.clone(),
                                got: vt,
                                span: value.span,
                            });
                        }
                        // Refine any enum-ctor side-table entries inside
                        // `value` whose inferred args contain `Any`,
                        // using the let annotation as the target. This
                        // is what lets the JIT monomorphizer pick a
                        // single concrete enum instantiation when an
                        // EnumCtor only provides args for some of T/E.
                        self.refine_enum_ctor_args(value, ann);
                        ann.clone()
                    }
                    None => vt,
                };
                // Outside `@extern(C) {}`, raw C pointer / `char` /
                // `void` / `size_t` / `ssize_t` values cannot be
                // bound to a name — that would let them escape into
                // user code, defeating the encapsulation. Wrap the
                // FFI call in an `@extern(C)` fn instead.
                if !*self.in_extern_c.borrow() {
                    if let Some(c_only) = first_c_only_type(&bind) {
                        return Err(TypeError::Unsupported {
                            what: format!(
                                "value of type {bind} cannot be bound outside an \
                                 @extern(C) {{ ... }} block (contains the C-only type \
                                 {c_only}); wrap the FFI call in an @extern(C) fn"
                            ),
                            span: value.span,
                        });
                    }
                }
                env.insert(name.clone(), bind);
                Ok(Type::Unit)
            }
            StmtKind::Expr(e) => self.check_expr(e, env, ret_ty, in_class, loop_depth),
        }
    }

    fn check_expr(
        &self,
        expr: &Expr,
        env: &Vars,
        ret_ty: Option<&Type>,
        in_class: Option<Symbol>,
        loop_depth: u32,
    ) -> Result<Type, TypeError> {
        let t = self.check_expr_inner(expr, env, ret_ty, in_class, loop_depth)?;
        // Outside `@extern(C) {}`, no expression may evaluate to a
        // raw C pointer / `char` / `void` / `size_t` / `ssize_t`.
        // This rejects helper calls like `cstrFromString(...)` and
        // FFI fn calls returning C-only types from user code, forcing
        // the user to wrap them in an `@extern(C)` fn that yields a
        // regular ilang type.
        if !*self.in_extern_c.borrow() {
            if let Some(c_only) = first_c_only_type(&t) {
                return Err(TypeError::Unsupported {
                    what: format!(
                        "expression of type {t} is only allowed inside an \
                         @extern(C) {{ ... }} block (contains the C-only type \
                         {c_only})"
                    ),
                    span: expr.span,
                });
            }
        }
        Ok(t)
    }

    fn check_expr_inner(
        &self,
        expr: &Expr,
        env: &Vars,
        ret_ty: Option<&Type>,
        in_class: Option<Symbol>,
        loop_depth: u32,
    ) -> Result<Type, TypeError> {
        let span = expr.span;
        match &expr.kind {
            // The parser produces `StructLit`, but normalize desugars
            // it into `{ let __sl = new Foo(); __sl.f = v; ...; __sl }`
            // before type checking runs — reaching here means a
            // pipeline shortcut bypassed normalize.
            ExprKind::StructLit { .. } => Err(TypeError::Unsupported {
                what: "internal: struct literal reached type checker (normalize was skipped)".into(),
                span,
            }),
            ExprKind::Int(_) => Ok(Type::I64),
            ExprKind::Float(_) => Ok(Type::F64),
            ExprKind::Bool(_) => Ok(Type::Bool),
            ExprKind::Str(_) => Ok(Type::Str),
            ExprKind::Closure { fn_name, .. } => {
                // The JIT runs a second typecheck on the post-hoist
                // program; by then FnExpr has been replaced with
                // Closure and the wrapper FnDecl is in self.fns.
                // The wrapper's first param is the synthetic
                // `__env: i64`; the user-facing fn type is the rest.
                let sigs = self.fns.get(fn_name).cloned().ok_or_else(|| {
                    TypeError::UndefinedVariable {
                        name: fn_name.clone(),
                        span,
                    }
                })?;
                let sig = sigs.into_iter().next().expect("registered fn has sig");
                let user_params: Vec<Type> = sig.params.iter().skip(1).cloned().collect();
                Ok(Type::func(user_params, sig.ret))
            }
            ExprKind::This => match in_class {
                Some(name) => Ok(Type::Object(name.into())),
                None => Err(TypeError::ThisOutsideMethod { span }),
            },
            ExprKind::SuperCall { method, args } => {
                let class_name = in_class.ok_or_else(|| TypeError::Unsupported {
                    what: "`super` is only valid inside a class method".into(),
                    span,
                })?;
                let parent_name = self
                    .classes
                    .get(&class_name)
                    .and_then(|c| c.parent)
                    .ok_or_else(|| TypeError::Unsupported {
                        what: format!(
                            "`super` used in class {:?}, which has no parent",
                            class_name
                        ),
                        span,
                    })?;
                let parent_sig = self.classes.get(&parent_name).cloned().expect("parent registered");
                let lookup: Symbol = method.unwrap_or_else(|| "init".into());
                let sigs = parent_sig.methods.get(&lookup).cloned().ok_or_else(|| {
                    TypeError::UnknownMethod {
                        class: parent_name,
                        method: lookup,
                        span,
                    }
                })?;
                if sigs.len() != 1 {
                    return Err(TypeError::Unsupported {
                        what: format!(
                            "super.{lookup}: parent's method is overloaded; \
                             super-calls require a single signature"
                        ),
                        span,
                    });
                }
                let sig = sigs.into_iter().next().unwrap();
                self.check_args(lookup, &sig, args, env, ret_ty, in_class, loop_depth, span)?;
                Ok(sig.ret)
            }
            ExprKind::Var(n) => {
                if let Some(t) = env.get(n) {
                    return Ok(t.clone());
                }
                if let Some(class_name) = in_class {
                    if let Some(cls) = self.classes.get(&class_name) {
                        if let Some(t) = cls.fields.get(n) {
                            return Ok(t.clone());
                        }
                    }
                }
                // First-class function: a bare reference to a top-level
                // `fn` becomes a function value (`Type::Fn(...)`). For
                // overloaded names this is ambiguous — disambiguation
                // would need an explicit type annotation we don't have
                // syntax for, so reject.
                if let Some(sigs) = self.fns.get(n) {
                    if sigs.len() != 1 {
                        return Err(TypeError::Unsupported {
                            what: format!(
                                "fn {n:?} has {} overloads — bare references to overloaded \
                                 fns are ambiguous; call them directly with arguments",
                                sigs.len()
                            ),
                            span,
                        });
                    }
                    let sig = &sigs[0];
                    return Ok(Type::func(sig.params.clone(), sig.ret.clone()));
                }
                Err(TypeError::UndefinedVariable {
                    name: n.clone(),
                    span,
                })
            }
            ExprKind::Unary { op, expr: inner } => {
                let t = self.check_expr(inner, env, ret_ty, in_class, loop_depth)?;
                match op {
                    // Unary `-` is only meaningful on signed numerics.
                    UnOp::Neg if t.is_signed_int() || t.is_float() => Ok(t),
                    // Unary `+` is identity on any numeric.
                    UnOp::Pos if t.is_numeric() => Ok(t),
                    UnOp::Not if t == Type::Bool => Ok(t),
                    // Bit-not on any int (signed or unsigned).
                    UnOp::BitNot if t.is_int() => Ok(t),
                    // `~Flag.x` on a @flags enum yields a Flag value.
                    UnOp::BitNot
                        if matches!(&t, Type::Object(n) if self.enums.get(n).map(|s| s.flags).unwrap_or(false)) =>
                    {
                        Ok(t)
                    }
                    _ => Err(TypeError::BadUnary { ty: t, span }),
                }
            }
            ExprKind::Binary { op, lhs, rhs } => {
                let l = self.check_expr(lhs, env, ret_ty, in_class, loop_depth)?;
                let r = self.check_expr(rhs, env, ret_ty, in_class, loop_depth)?;
                // `@flags` enum: `Flag op Flag` for `|` `&` `^` returns Flag.
                if matches!(op, ilang_ast::BinOp::BitOr | ilang_ast::BinOp::BitAnd | ilang_ast::BinOp::BitXor) {
                    if let (Type::Object(ln), Type::Object(rn)) = (&l, &r) {
                        if ln == rn
                            && self.enums.get(ln).map(|s| s.flags).unwrap_or(false)
                        {
                            return Ok(l);
                        }
                    }
                }
                // Literal-side adoption: when one operand is a
                // numeric literal whose value fits the other's
                // integer type, treat the literal as that type.
                // Lets `u32_var < 5000` and `u32_var != 0` work
                // without a manual `as u32`.
                let (l, r) = if l.is_int() && r.is_int()
                    && l.is_signed_int() != r.is_signed_int()
                {
                    if numeric_literal_fits(rhs, &l) {
                        (l.clone(), l)
                    } else if numeric_literal_fits(lhs, &r) {
                        (r.clone(), r)
                    } else {
                        (l, r)
                    }
                } else {
                    (l, r)
                };
                bin_result(*op, &l, &r).map_err(|e| attach_span(e, span))
            }
            ExprKind::Logical { op: _, lhs, rhs } => {
                let l = self.check_expr(lhs, env, ret_ty, in_class, loop_depth)?;
                let r = self.check_expr(rhs, env, ret_ty, in_class, loop_depth)?;
                if l != Type::Bool || r != Type::Bool {
                    return Err(TypeError::BadBinary {
                        lhs: l,
                        rhs: r,
                        span,
                    });
                }
                Ok(Type::Bool)
            }
            ExprKind::Call { callee, args } => {
                if callee == "deinit" {
                    return Err(TypeError::CannotCallDeinit { span });
                }
                // Built-in `typeof(x): Type` — RTTI introspection.
                // Accepts any single value; the JIT / interpreter
                // synthesise the right Type metadata at runtime.
                if callee == "typeof" {
                    if args.len() != 1 {
                        return Err(TypeError::ArityMismatch {
                            name: callee.clone(),
                            expected: 1,
                            got: args.len(),
                            span,
                        });
                    }
                    self.check_expr(&args[0], env, ret_ty, in_class, loop_depth)?;
                    return Ok(Type::Object("Type".into()));
                }
                // FFI marshalling helpers are only callable inside an
                // `@extern(C) {}` block — they exist to bridge raw C
                // values to ilang-native ones, which only matters at
                // the FFI boundary.
                if !*self.in_extern_c.borrow()
                    && FFI_HELPERS.contains(&callee.as_str())
                {
                    return Err(TypeError::Unsupported {
                        what: format!(
                            "{callee}: FFI marshalling helper, only \
                             callable inside an @extern(C) {{ ... }} block"
                        ),
                        span,
                    });
                }
                // Indirect call through a function-typed local: shadows
                // both methods and top-level fns, mirroring how a local
                // `let print = ...` shadows an outer name.
                if let Some(Type::Fn(ft)) = env.get(callee).cloned() {
                    let sig = Signature {
                        params: ft.params.to_vec(),
                        ret: ft.ret.clone(),
                        variadic: false, decl_span: Span::dummy(), type_params: Vec::new(),
                        defaults: Vec::new(),
                    };
                    self.check_args(*callee, &sig, args, env, ret_ty, in_class, loop_depth, span)?;
                    return Ok(sig.ret);
                }
                if let Some(class_name) = in_class {
                    if let Some(cls) = self.classes.get(&class_name) {
                        if let Some(sigs) = cls.methods.get(callee).cloned() {
                            // Implicit-this method call. Resolve overload
                            // exactly like a top-level fn call.
                            let chosen = self.resolve_method_call(
                                class_name, *callee, &sigs, args, env, ret_ty, in_class, loop_depth, span,
                            )?;
                            return Ok(chosen.ret);
                        }
                    }
                }
                let sigs = self.fns.get(callee).cloned().ok_or_else(|| {
                    TypeError::UndefinedFunction {
                        name: callee.clone(),
                        span,
                    }
                })?;
                // Generic fns can't share a name with overloads (we
                // reject that at registration time), so a generic slot
                // is always exactly one signature. Fall through to the
                // existing generic-inference path below.
                let is_generic_slot = sigs.len() == 1 && !sigs[0].type_params.is_empty();
                if !is_generic_slot {
                    if sigs.len() == 1 {
                        // Single non-generic overload: keep the existing
                        // arity / per-arg validation so error variants
                        // (Mismatch / ArityMismatch) stay precise.
                        let sig = sigs.into_iter().next().unwrap();
                        self.fn_overload_pick
                            .borrow_mut()
                            .insert(span, (callee.clone(), 0));
                        self.check_args(*callee, &sig, args, env, ret_ty, in_class, loop_depth, span)?;
                        return Ok(sig.ret);
                    }
                    // Multiple overloads — score each viable signature
                    // and pick the best match.
                    let mut arg_tys: Vec<Type> = Vec::with_capacity(args.len());
                    for a in args {
                        arg_tys.push(self.check_expr(a, env, ret_ty, in_class, loop_depth)?);
                    }
                    let chosen = resolve_overload(*callee, &sigs, &arg_tys, args, span)?;
                    let chosen_sig = sigs[chosen].clone();
                    self.fn_overload_pick
                        .borrow_mut()
                        .insert(span, (callee.clone(), chosen));
                    self.check_args(*callee, &chosen_sig, args, env, ret_ty, in_class, loop_depth, span)?;
                    return Ok(chosen_sig.ret);
                }
                let sig = sigs.into_iter().next().unwrap();
                // Generic fn — see below; we also stash the inferred
                // type-args vector keyed by call span so the JIT's
                // monomorphization pass can find it later.
                // Generic fn: infer type-arg bindings from the (parametric
                // param type, arg type) pairs, then validate arg-by-arg
                // against the substituted param types and return the
                // substituted return type. Mirrors enum-ctor inference.
                if sig.params.len() != args.len() {
                    return Err(TypeError::ArityMismatch {
                        name: callee.clone(),
                        expected: sig.params.len(),
                        got: args.len(),
                        span,
                    });
                }
                let mut bindings: HashMap<Symbol, Type> = HashMap::new();
                let mut arg_tys: Vec<Type> = Vec::with_capacity(args.len());
                for (param_ty, arg) in sig.params.iter().zip(args.iter()) {
                    let at = self.check_expr(arg, env, ret_ty, in_class, loop_depth)?;
                    collect_type_var_bindings(param_ty, &at, &mut bindings);
                    arg_tys.push(at);
                }
                let inferred_args: Vec<Type> = sig
                    .type_params
                    .iter()
                    .map(|p| bindings.get(p).cloned().unwrap_or(Type::Any))
                    .collect();
                // Stash for the JIT monomorphizer. Args may still
                // contain TypeVars when the call is inside another
                // generic context — that's resolved at expansion time.
                self.fn_call_type_args
                    .borrow_mut()
                    .insert(span, (callee.clone(), inferred_args.clone()));
                for ((param_ty, arg), at) in sig.params.iter().zip(args.iter()).zip(arg_tys.iter()) {
                    let actual = subst_type(param_ty, &sig.type_params, &inferred_args);
                    if !literal_assignable(arg, at, &actual) && !self.assignable_obj(at, &actual) {
                        return Err(TypeError::Mismatch {
                            expected: actual,
                            got: at.clone(),
                            span: arg.span,
                        });
                    }
                }
                Ok(subst_type(&sig.ret, &sig.type_params, &inferred_args))
            }
            ExprKind::Field { obj, name } => {
                // Static field read: `ClassName.field` when there's
                // no shadowing local and the class declares a
                // static field by that name.
                if let ExprKind::Var(rname) = &obj.kind {
                    let is_local_shadow = env.contains_key(rname) || self.vars.contains_key(rname);
                    if !is_local_shadow {
                        if let Some(cls) = self.classes.get(&rname) {
                            if let Some(t) = cls.static_fields.get(name) {
                                return Ok(t.clone());
                            }
                        }
                    }
                }
                let ot = self.check_expr(obj, env, ret_ty, in_class, loop_depth)?;
                // Built-in property: every array exposes `length: i64`.
                if matches!(ot, Type::Array { .. }) && name == "length" {
                    return Ok(Type::I64);
                }
                // Built-in property: strings expose `length: i64` (Unicode
                // code-point count, JS-style).
                if matches!(ot, Type::Str) && name == "length" {
                    return Ok(Type::I64);
                }
                // Built-in Optional properties: `isSome` / `isNone`.
                if matches!(ot, Type::Optional(_))
                    && (name == "isSome" || name == "isNone")
                {
                    return Ok(Type::Bool);
                }
                // Built-in Result properties: `isOk` / `isErr`.
                if (name == "isOk" || name == "isErr") && is_result_type(&ot) {
                    return Ok(Type::Bool);
                }
                // Built-in RTTI: `Type.name` / `Type.kind` / `Type.parent`.
                if matches!(&ot, Type::Object(n) if n.as_str() == "Type") {
                    if name == "name" {
                        return Ok(Type::Str);
                    }
                    if name == "kind" {
                        return Ok(Type::Object("TypeKind".into()));
                    }
                    if name == "parent" {
                        return Ok(Type::Optional(Box::new(Type::Object("Type".into()))));
                    }
                    if name == "fields" || name == "methods" {
                        return Ok(Type::Array { elem: Box::new(Type::Str), fixed: None });
                    }
                    if name == "typeArgs" {
                        return Ok(Type::Array {
                            elem: Box::new(Type::Object("Type".into())),
                            fixed: None,
                        });
                    }
                }
                let class_name = expect_object(&ot, span)?;
                let cls = self.classes.get(&class_name).ok_or_else(|| {
                    TypeError::UndefinedClass {
                        name: class_name.into(),
                        span,
                    }
                })?;
                // Property `get` takes precedence over field lookup —
                // the parser disallows declaring a property and a
                // same-named field on one class, but checking properties
                // first keeps the resolution explicit.
                if let Some(p) = cls.properties.get(name) {
                    if !p.has_get {
                        return Err(TypeError::Unsupported {
                            what: format!(
                                "property {:?}.{} has no getter (write-only)",
                                class_name, name
                            ),
                            span,
                        });
                    }
                    return Ok(subst_type(
                        &p.ty,
                        &cls.type_params,
                        type_args_of(&ot),
                    ));
                }
                let raw = cls.fields.get(name).cloned().ok_or_else(|| {
                    TypeError::UnknownField {
                        class: class_name.into(),
                        field: name.clone(),
                        span,
                    }
                })?;
                Ok(subst_type(&raw, &cls.type_params, type_args_of(&ot)))
            }
            ExprKind::MethodCall { obj, method, args } => {
                if method == "deinit" {
                    return Err(TypeError::CannotCallDeinit { span });
                }
                // Static method dispatch: `ClassName.method(args)` —
                // the receiver is a Var matching a known class name
                // that has a static method by that name, and there's
                // no shadowing local of the same name.
                if let ExprKind::Var(name) = &obj.kind {
                    let is_local_shadow = env.contains_key(name) || self.vars.contains_key(name);
                    if !is_local_shadow {
                        if let Some(cls) = self.classes.get(&name) {
                            if let Some(sig) = cls.static_methods.get(method).cloned() {
                                self.check_args(
                                    *method, &sig, args, env, ret_ty, in_class, loop_depth, span,
                                )?;
                                return Ok(sig.ret);
                            }
                        }
                    }
                }
                let ot = self.check_expr(obj, env, ret_ty, in_class, loop_depth)?;
                // Built-in `Type` introspection methods.
                if matches!(&ot, Type::Object(n) if n.as_str() == "Type") {
                    let target = match method.as_str() {
                        "fieldType" | "methodReturn" => Some((1, Type::Optional(Box::new(
                            Type::Object("Type".into()),
                        )))),
                        "methodParams" => Some((1, Type::Optional(Box::new(Type::Array {
                            elem: Box::new(Type::Object("Type".into())),
                            fixed: None,
                        })))),
                        _ => None,
                    };
                    if let Some((arity, ret)) = target {
                        if args.len() != arity {
                            return Err(TypeError::ArityMismatch {
                                name: method.clone(),
                                expected: arity,
                                got: args.len(),
                                span,
                            });
                        }
                        let at = self.check_expr(&args[0], env, ret_ty, in_class, loop_depth)?;
                        if at != Type::Str {
                            return Err(TypeError::Mismatch {
                                expected: Type::Str,
                                got: at,
                                span: args[0].span,
                            });
                        }
                        return Ok(ret);
                    }
                }
                // Built-in Weak method: get(): T?.
                if let Type::Weak(inner) = &ot {
                    if method == "get" {
                        if !args.is_empty() {
                            return Err(TypeError::ArityMismatch {
                                name: "get".into(),
                                expected: 0,
                                got: args.len(),
                                span,
                            });
                        }
                        return Ok(Type::Optional(inner.clone()));
                    }
                    return Err(TypeError::UnknownMethod {
                        class: Symbol::intern(&format!("{ot}")),
                        method: method.clone(),
                        span,
                    });
                }
                // Built-in Optional methods: unwrap. (`isSome` / `isNone`
                // are properties — see ExprKind::Field.)
                if let Type::Optional(inner) = &ot {
                    match method.as_str() {
                        "unwrap" => {
                            if !args.is_empty() {
                                return Err(TypeError::ArityMismatch {
                                    name: method.clone(),
                                    expected: 0,
                                    got: args.len(),
                                    span,
                                });
                            }
                            return Ok((**inner).clone());
                        }
                        _ => {
                            return Err(TypeError::UnknownMethod {
                                class: Symbol::intern(&format!("{ot}")),
                                method: method.clone(),
                                span,
                            });
                        }
                    }
                }
                // Built-in string methods (JS-style camelCase).
                if matches!(ot, Type::Str) {
                    let arity_check = |expected: usize| -> Result<(), TypeError> {
                        if args.len() != expected {
                            Err(TypeError::ArityMismatch {
                                name: method.clone(),
                                expected,
                                got: args.len(),
                                span,
                            })
                        } else {
                            Ok(())
                        }
                    };
                    match method.as_str() {
                        "charAt" => {
                            arity_check(1)?;
                            let at = self.check_expr(&args[0], env, ret_ty, in_class, loop_depth)?;
                            if !matches!(at, Type::I64 | Type::I32 | Type::I16 | Type::I8 | Type::U64 | Type::U32 | Type::U16 | Type::U8) {
                                return Err(TypeError::Mismatch {
                                    expected: Type::I64,
                                    got: at,
                                    span: args[0].span,
                                });
                            }
                            return Ok(Type::Str);
                        }
                        "includes" | "startsWith" | "endsWith" => {
                            arity_check(1)?;
                            let at = self.check_expr(&args[0], env, ret_ty, in_class, loop_depth)?;
                            if !matches!(at, Type::Str) {
                                return Err(TypeError::Mismatch {
                                    expected: Type::Str,
                                    got: at,
                                    span: args[0].span,
                                });
                            }
                            return Ok(Type::Bool);
                        }
                        "toUpper" | "toLower" | "trim" => {
                            arity_check(0)?;
                            return Ok(Type::Str);
                        }
                        "replace" => {
                            arity_check(2)?;
                            for a in args {
                                let at = self.check_expr(a, env, ret_ty, in_class, loop_depth)?;
                                if !matches!(at, Type::Str) {
                                    return Err(TypeError::Mismatch {
                                        expected: Type::Str,
                                        got: at,
                                        span: a.span,
                                    });
                                }
                            }
                            return Ok(Type::Str);
                        }
                        "split" => {
                            arity_check(1)?;
                            let at = self.check_expr(&args[0], env, ret_ty, in_class, loop_depth)?;
                            if !matches!(at, Type::Str) {
                                return Err(TypeError::Mismatch {
                                    expected: Type::Str,
                                    got: at,
                                    span: args[0].span,
                                });
                            }
                            return Ok(Type::Array { elem: Box::new(Type::Str), fixed: None });
                        }
                        "slice" => {
                            arity_check(2)?;
                            for a in args {
                                let at = self.check_expr(a, env, ret_ty, in_class, loop_depth)?;
                                if !matches!(at, Type::I64 | Type::I32 | Type::I16 | Type::I8 | Type::U64 | Type::U32 | Type::U16 | Type::U8) {
                                    return Err(TypeError::Mismatch {
                                        expected: Type::I64,
                                        got: at,
                                        span: a.span,
                                    });
                                }
                            }
                            return Ok(Type::Str);
                        }
                        _ => {
                            return Err(TypeError::UnknownMethod {
                                class: "string".into(),
                                method: method.clone(),
                                span,
                            });
                        }
                    }
                }
                // Built-in array methods.
                if let Type::Array { elem, fixed } = &ot {
                    if method == "push" {
                        if fixed.is_some() {
                            return Err(TypeError::Mismatch {
                                expected: Type::Array {
                                    elem: elem.clone(),
                                    fixed: None,
                                },
                                got: ot.clone(),
                                span,
                            });
                        }
                        if args.len() != 1 {
                            return Err(TypeError::ArityMismatch {
                                name: "push".into(),
                                expected: 1,
                                got: args.len(),
                                span,
                            });
                        }
                        let at = self.check_expr(&args[0], env, ret_ty, in_class, loop_depth)?;
                        if !literal_assignable(&args[0], &at, elem) {
                            return Err(TypeError::Mismatch {
                                expected: (**elem).clone(),
                                got: at,
                                span: args[0].span,
                            });
                        }
                        return Ok(Type::Unit);
                    }
                    if method == "pop" {
                        if fixed.is_some() {
                            return Err(TypeError::Mismatch {
                                expected: Type::Array {
                                    elem: elem.clone(),
                                    fixed: None,
                                },
                                got: ot.clone(),
                                span,
                            });
                        }
                        if !args.is_empty() {
                            return Err(TypeError::ArityMismatch {
                                name: "pop".into(),
                                expected: 0,
                                got: args.len(),
                                span,
                            });
                        }
                        return Ok(Type::Optional(elem.clone()));
                    }
                    if method == "indexOf" || method == "includes" {
                        if args.len() != 1 {
                            return Err(TypeError::ArityMismatch {
                                name: method.clone(),
                                expected: 1,
                                got: args.len(),
                                span,
                            });
                        }
                        let at = self.check_expr(&args[0], env, ret_ty, in_class, loop_depth)?;
                        if !literal_assignable(&args[0], &at, elem) {
                            return Err(TypeError::Mismatch {
                                expected: (**elem).clone(),
                                got: at,
                                span: args[0].span,
                            });
                        }
                        return Ok(if method == "indexOf" {
                            Type::I64
                        } else {
                            Type::Bool
                        });
                    }
                    if method == "slice" {
                        // slice(start: i64, end: i64): T[]
                        if args.len() != 2 {
                            return Err(TypeError::ArityMismatch {
                                name: "slice".into(),
                                expected: 2,
                                got: args.len(),
                                span,
                            });
                        }
                        for a in args {
                            let at = self.check_expr(a, env, ret_ty, in_class, loop_depth)?;
                            if !literal_assignable(a, &at, &Type::I64) && !self.assignable_obj(&at, &Type::I64) {
                                return Err(TypeError::Mismatch {
                                    expected: Type::I64,
                                    got: at,
                                    span: a.span,
                                });
                            }
                        }
                        return Ok(Type::Array { elem: elem.clone(), fixed: None });
                    }
                    if method == "map" || method == "filter" || method == "forEach" {
                        if args.len() != 1 {
                            return Err(TypeError::ArityMismatch {
                                name: method.clone(),
                                expected: 1,
                                got: args.len(),
                                span,
                            });
                        }
                        let ft = self.check_expr(&args[0], env, ret_ty, in_class, loop_depth)?;
                        let (params, ret) = match &ft {
                            Type::Fn(fty) => (fty.params.clone(), fty.ret.clone()),
                            _ => return Err(TypeError::Mismatch {
                                expected: Type::func(vec![(**elem).clone()], Type::Any),
                                got: ft,
                                span: args[0].span,
                            }),
                        };
                        if params.len() != 1 || !assignable(elem, &params[0]) && !self.assignable_obj(elem, &params[0]) {
                            return Err(TypeError::Mismatch {
                                expected: Type::func(vec![(**elem).clone()], Type::Any),
                                got: Type::func(params.to_vec(), ret.clone()),
                                span: args[0].span,
                            });
                        }
                        return Ok(match method.as_str() {
                            "map" => Type::Array { elem: Box::new(ret), fixed: None },
                            "filter" => {
                                if !matches!(ret, Type::Bool) {
                                    return Err(TypeError::Mismatch {
                                        expected: Type::Bool,
                                        got: ret,
                                        span: args[0].span,
                                    });
                                }
                                Type::Array { elem: elem.clone(), fixed: None }
                            }
                            "forEach" => Type::Unit,
                            _ => unreachable!(),
                        });
                    }
                    return Err(TypeError::UnknownMethod {
                        class: Symbol::intern(&format!("{ot}")),
                        method: method.clone(),
                        span,
                    });
                }
                // `@flags` enum: `f.has(other)` is a synthetic method
                // returning bool, equivalent to `(f & other) == other`.
                if let Type::Object(ename) = &ot {
                    if let Some(sig) = self.enums.get(ename).cloned() {
                        if sig.flags && method == "has" {
                            if args.len() != 1 {
                                return Err(TypeError::ArityMismatch {
                                    name: "has".into(),
                                    expected: 1,
                                    got: args.len(),
                                    span,
                                });
                            }
                            let at = self.check_expr(
                                &args[0], env, ret_ty, in_class, loop_depth,
                            )?;
                            if at != ot {
                                return Err(TypeError::Mismatch {
                                    expected: ot.clone(),
                                    got: at,
                                    span: args[0].span,
                                });
                            }
                            return Ok(Type::Bool);
                        }
                    }
                }
                let class_name = expect_object(&ot, span)?;
                let cls = self.classes.get(&class_name).ok_or_else(|| {
                    TypeError::UndefinedClass {
                        name: class_name.into(),
                        span,
                    }
                })?;
                let raw_sigs = cls.methods.get(method).cloned().ok_or_else(|| {
                    TypeError::UnknownMethod {
                        class: class_name.into(),
                        method: method.clone(),
                        span,
                    }
                })?;
                let class_params = cls.type_params.clone();
                let inst_args: Vec<Type> = type_args_of(&ot).to_vec();
                // Substitute generic type args once per overload, then
                // resolve which overload matches the call.
                let class_name_owned = class_name.into();
                let substituted: Vec<Signature> = raw_sigs
                    .iter()
                    .map(|raw| Signature {
                        params: raw
                            .params
                            .iter()
                            .map(|t| subst_type(t, &class_params, &inst_args))
                            .collect(),
                        ret: subst_type(&raw.ret, &class_params, &inst_args),
                        variadic: raw.variadic,
                        type_params: Vec::new(),
                        decl_span: raw.decl_span,
                        defaults: raw.defaults.clone(),
                    })
                    .collect();
                let chosen = self.resolve_method_call(
                    class_name_owned, *method, &substituted, args, env, ret_ty, in_class, loop_depth, span,
                )?;
                Ok(chosen.ret)
            }
            ExprKind::New { class, type_args, args, init_method } => {
                let cls = self.classes.get(&class).ok_or_else(|| TypeError::UndefinedClass {
                    name: class.clone(),
                    span,
                })?;
                if cls.extern_lib.is_some() {
                    return Err(TypeError::Unsupported {
                        what: format!(
                            "cannot construct opaque extern class {class:?} with `new` — \
                             values come from native extern fn return values"
                        ),
                        span,
                    });
                }
                let class_params = cls.type_params.clone();
                // After the mangling pass, an overloaded `init` is renamed
                // to e.g. `init__i64`; New.init_method records which one
                // was picked. Look that up first so a re-typecheck on the
                // mangled program still resolves correctly.
                let init_lookup: Symbol = init_method.unwrap_or_else(|| "init".into());
                let init_raw = cls.methods.get(&init_lookup).cloned();
                // Generic instantiation: arity check on type args.
                if !class_params.is_empty() && type_args.len() != class_params.len() {
                    return Err(TypeError::ArityMismatch {
                        name: Symbol::intern(&format!("{class}::<type args>")),
                        expected: class_params.len(),
                        got: type_args.len(),
                        span,
                    });
                }
                // Non-generic class with explicit `<...>` args is an error.
                if class_params.is_empty() && !type_args.is_empty() {
                    return Err(TypeError::ArityMismatch {
                        name: Symbol::intern(&format!("{class}::<type args>")),
                        expected: 0,
                        got: type_args.len(),
                        span,
                    });
                }
                let inst_args: Vec<Type> = type_args.to_vec();
                if let Some(init_overloads) = init_raw {
                    // Substitute generic type-args once per init
                    // overload, then resolve which init to call.
                    let substituted: Vec<Signature> = init_overloads
                        .iter()
                        .map(|init| Signature {
                            params: init
                                .params
                                .iter()
                                .map(|t| subst_type(t, &class_params, &inst_args))
                                .collect(),
                            ret: subst_type(&init.ret, &class_params, &inst_args),
                            variadic: init.variadic,
                            type_params: Vec::new(),
                            decl_span: init.decl_span,
                            defaults: init.defaults.clone(),
                        })
                        .collect();
                    self.resolve_method_call(
                        *class, "init".into(), &substituted, args, env, ret_ty, in_class, loop_depth, span,
                    )?;
                } else if !args.is_empty() {
                    // C99 flexible array member: `@extern(C) struct`
                    // ending in `T[]` accepts exactly one i64 arg
                    // (the trailing element count) at construction.
                    if cls.has_fam && args.len() == 1 {
                        let t = self.check_expr(
                            &args[0], env, ret_ty, in_class, loop_depth,
                        )?;
                        if !matches!(
                            t,
                            Type::I8 | Type::I16 | Type::I32 | Type::I64
                            | Type::U8 | Type::U16 | Type::U32 | Type::U64
                        ) {
                            return Err(TypeError::Mismatch {
                                expected: Type::I64,
                                got: t,
                                span: args[0].span,
                            });
                        }
                    } else {
                        return Err(TypeError::ArityMismatch {
                            name: Symbol::intern(&format!("{class}::init")),
                            expected: 0,
                            got: args.len(),
                            span,
                        });
                    }
                }
                Ok(if class_params.is_empty() {
                    Type::Object(class.clone())
                } else {
                    Type::generic(class.clone(), inst_args)
                })
            }
            ExprKind::Block(b) => self.check_block(b, env, ret_ty, in_class, loop_depth),
            ExprKind::If {
                cond,
                then_branch,
                else_branch,
            } => {
                let c = self.check_expr(cond, env, ret_ty, in_class, loop_depth)?;
                if c != Type::Bool {
                    return Err(TypeError::Mismatch {
                        expected: Type::Bool,
                        got: c,
                        span: cond.span,
                    });
                }
                let then_ty = self.check_block(then_branch, env, ret_ty, in_class, loop_depth)?;
                match else_branch {
                    None => {
                        // No else: the expression evaluates to () regardless
                        // of the then-branch's type (any value would be
                        // discarded). Mirrors `if let some(...)` and matches
                        // the JS-style intent of "do this conditionally".
                        Ok(Type::Unit)
                    }
                    Some(else_e) => {
                        let else_ty = self.check_expr(else_e, env, ret_ty, in_class, loop_depth)?;
                        if then_ty == else_ty {
                            return Ok(then_ty);
                        }
                        // Generic types (e.g. `Result<T, E>`) where each
                        // arm fixed a different type parameter need to
                        // be merged into the more specific shape — e.g.
                        // `Result<i64, Any>` and `Result<Any, string>`
                        // unify to `Result<i64, string>`.
                        if let Some(merged) = merge_generic_with_holes(&then_ty, &else_ty) {
                            return Ok(merged);
                        }
                        // Rust 流: 暗黙の数値拡張は禁止 (i64 と f64 を
                        // ぶつけたらエラー)。例外として、片方のアームの末尾式
                        // が「素の数値リテラル」 (整数/浮動小数、単項マイナス
                        // 込み) で、もう一方の型に収まるときだけ受け入れる。
                        let then_tail = then_branch.tail.as_deref();
                        if let Some(t) = then_tail {
                            if numeric_literal_fits(t, &else_ty) {
                                return Ok(else_ty);
                            }
                        }
                        if numeric_literal_fits(else_e, &then_ty) {
                            return Ok(then_ty);
                        }
                        Err(TypeError::Mismatch {
                            expected: then_ty,
                            got: else_ty,
                            span: else_e.span,
                        })
                    }
                }
            }
            ExprKind::While { cond, body } => {
                let c = self.check_expr(cond, env, ret_ty, in_class, loop_depth)?;
                if c != Type::Bool {
                    return Err(TypeError::Mismatch {
                        expected: Type::Bool,
                        got: c,
                        span: cond.span,
                    });
                }
                self.loop_stack.borrow_mut().push(LoopFrame::Other);
                let body_res = self.check_block(body, env, ret_ty, in_class, loop_depth + 1);
                self.loop_stack.borrow_mut().pop();
                let body_ty = body_res?;
                if body_ty != Type::Unit {
                    return Err(TypeError::Mismatch {
                        expected: Type::Unit,
                        got: body_ty,
                        span,
                    });
                }
                Ok(Type::Unit)
            }
            ExprKind::Loop { body } => {
                self.loop_stack.borrow_mut().push(LoopFrame::Loop(None));
                let body_res = self.check_block(body, env, ret_ty, in_class, loop_depth + 1);
                let frame = self.loop_stack.borrow_mut().pop();
                let body_ty = body_res?;
                if body_ty != Type::Unit {
                    return Err(TypeError::Mismatch {
                        expected: Type::Unit,
                        got: body_ty,
                        span,
                    });
                }
                // The loop's own type is the unified break-value type, or
                // Unit if no `break v` was seen.
                let break_ty = match frame {
                    Some(LoopFrame::Loop(Some(t))) => t,
                    _ => Type::Unit,
                };
                self.loop_break_type
                    .borrow_mut()
                    .insert(span, break_ty.clone());
                Ok(break_ty)
            }
            ExprKind::ForIn { var, iter, body } => {
                // Range iter: check both endpoints are integer types of
                // a single common int type, bind `var` to that type.
                let elem = if let ExprKind::Range { start, end, .. } = &iter.kind {
                    let st = self.check_expr(start, env, ret_ty, in_class, loop_depth)?;
                    let et = self.check_expr(end, env, ret_ty, in_class, loop_depth)?;
                    if !st.is_int() {
                        return Err(TypeError::Mismatch {
                            expected: Type::I64,
                            got: st,
                            span: start.span,
                        });
                    }
                    if !et.is_int() {
                        return Err(TypeError::Mismatch {
                            expected: st.clone(),
                            got: et,
                            span: end.span,
                        });
                    }
                    if st != et {
                        // Same literal-coercion rule as if-branches and
                        // let bindings: a bare numeric literal endpoint
                        // adopts the other side's int type when it fits.
                        if numeric_literal_fits(start, &et) {
                            et
                        } else if numeric_literal_fits(end, &st) {
                            st
                        } else {
                            return Err(TypeError::Mismatch {
                                expected: st,
                                got: et,
                                span: end.span,
                            });
                        }
                    } else {
                        st
                    }
                } else {
                    let it = self.check_expr(iter, env, ret_ty, in_class, loop_depth)?;
                    match &it {
                        Type::Array { elem, .. } => (**elem).clone(),
                        other => {
                            return Err(TypeError::Mismatch {
                                expected: Type::Array {
                                    elem: Box::new(Type::Any),
                                    fixed: None,
                                },
                                got: other.clone(),
                                span: iter.span,
                            });
                        }
                    }
                };
                let mut inner = env.clone();
                inner.insert(var.clone(), elem);
                self.loop_stack.borrow_mut().push(LoopFrame::Other);
                let body_res =
                    self.check_block(body, &inner, ret_ty, in_class, loop_depth + 1);
                self.loop_stack.borrow_mut().pop();
                let body_ty = body_res?;
                if body_ty != Type::Unit {
                    return Err(TypeError::Mismatch {
                        expected: Type::Unit,
                        got: body_ty,
                        span,
                    });
                }
                Ok(Type::Unit)
            }
            ExprKind::Break(value) => {
                if loop_depth == 0 {
                    return Err(TypeError::BreakOutsideLoop { span });
                }
                let val_ty = match value {
                    Some(e) => Some((self.check_expr(e, env, ret_ty, in_class, loop_depth)?, e.span)),
                    None => None,
                };
                // The innermost loop frame governs whether `break v` is
                // allowed and (for `loop`) collects its value type.
                let mut stack = self.loop_stack.borrow_mut();
                let frame = stack.last_mut().expect("loop_depth > 0 implies non-empty stack");
                match frame {
                    LoopFrame::Other => {
                        if value.is_some() {
                            return Err(TypeError::Unsupported {
                                what: "`break value` is only allowed inside `loop` (not `while` / `for`)".into(),
                                span,
                            });
                        }
                    }
                    LoopFrame::Loop(acc) => {
                        let new_ty = val_ty.as_ref().map(|(t, _)| t.clone()).unwrap_or(Type::Unit);
                        match acc {
                            None => *acc = Some(new_ty),
                            Some(prev) => {
                                if *prev != new_ty {
                                    // Same fallback as if/else branch
                                    // unification: try to merge generic
                                    // holes, then accept a bare numeric
                                    // literal that fits the prior type.
                                    if let Some(merged) =
                                        merge_generic_with_holes(prev, &new_ty)
                                    {
                                        *prev = merged;
                                    } else if value
                                        .as_deref()
                                        .is_some_and(|e| numeric_literal_fits(e, prev))
                                    {
                                        // keep `prev` — current break's
                                        // literal coerces into it.
                                    } else {
                                        return Err(TypeError::Mismatch {
                                            expected: prev.clone(),
                                            got: new_ty,
                                            span: val_ty.map(|(_, s)| s).unwrap_or(span),
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
                Ok(Type::Unit)
            }
            ExprKind::Continue => {
                if loop_depth == 0 {
                    return Err(TypeError::ContinueOutsideLoop { span });
                }
                Ok(Type::Unit)
            }
            ExprKind::Range { .. } => Err(TypeError::Unsupported {
                what: "range expression `a..b` is only valid as a `for-in` iterator".into(),
                span,
            }),
            ExprKind::Return(value) => {
                // Top-level `return` is allowed as an early-exit
                // from the program. Carrying a value is rejected
                // there — the program's value is its tail expr,
                // not a `return value`.
                let Some(expected) = ret_ty.cloned() else {
                    if value.is_some() {
                        return Err(TypeError::Unsupported {
                            what: "top-level `return` cannot carry a value (the program's value is its tail expression)".into(),
                            span,
                        });
                    }
                    return Ok(Type::Unit);
                };
                match value {
                    Some(v) => {
                        let vt = self.check_expr(v, env, ret_ty, in_class, loop_depth)?;
                        if !literal_assignable(v, &vt, &expected) && !self.assignable_obj(&vt, &expected) {
                            return Err(TypeError::Mismatch {
                                expected,
                                got: vt,
                                span: v.span,
                            });
                        }
                    }
                    None => {
                        if !matches!(expected, Type::Unit) {
                            return Err(TypeError::Mismatch {
                                expected,
                                got: Type::Unit,
                                span,
                            });
                        }
                    }
                }
                // `return` diverges — control never continues past it.
                // We pretend the expression has the function's return
                // type so a body ending in `return X` and a non-else
                // `if cond { return X }` both type-check without
                // needing a separate Never type.
                Ok(expected)
            }
            ExprKind::Assign { target, value } => {
                if let Some(var_ty) = env.get(target).cloned() {
                    let v_ty = self.check_expr(value, env, ret_ty, in_class, loop_depth)?;
                    if !literal_assignable(value, &v_ty, &var_ty) && !self.assignable_obj(&v_ty, &var_ty) {
                        return Err(TypeError::Mismatch {
                            expected: var_ty,
                            got: v_ty,
                            span: value.span,
                        });
                    }
                    return Ok(Type::Unit);
                }
                if let Some(class_name) = in_class {
                    if let Some(cls) = self.classes.get(&class_name) {
                        if let Some(field_ty) = cls.fields.get(target).cloned() {
                            let v_ty = self.check_expr(value, env, ret_ty, in_class, loop_depth)?;
                            if !literal_assignable(value, &v_ty, &field_ty) && !self.assignable_obj(&v_ty, &field_ty) {
                                return Err(TypeError::Mismatch {
                                    expected: field_ty,
                                    got: v_ty,
                                    span: value.span,
                                });
                            }
                            return Ok(Type::Unit);
                        }
                    }
                }
                Err(TypeError::UndefinedVariable {
                    name: target.clone(),
                    span,
                })
            }
            ExprKind::Array(elements) => {
                if elements.is_empty() {
                    // Element type is unknown; surface a marker
                    // (`Any`-element array) and let `literal_assignable`
                    // accept it when the let / param annotation pins the
                    // type. Bare `let a = []` falls through to the
                    // EmptyArrayNeedsAnnotation error in `check_stmt`.
                    return Ok(Type::Array {
                        elem: Box::new(Type::Any),
                        fixed: Some(0),
                    });
                }
                let first_ty = self.check_expr(&elements[0], env, ret_ty, in_class, loop_depth)?;
                for e in &elements[1..] {
                    let et = self.check_expr(e, env, ret_ty, in_class, loop_depth)?;
                    if !literal_assignable(e, &et, &first_ty) && !self.assignable_obj(&et, &first_ty) {
                        return Err(TypeError::Mismatch {
                            expected: first_ty.clone(),
                            got: et,
                            span: e.span,
                        });
                    }
                }
                Ok(Type::Array {
                    elem: Box::new(first_ty),
                    fixed: Some(elements.len()),
                })
            }
            ExprKind::Tuple(elements) => {
                let mut tys = Vec::with_capacity(elements.len());
                for e in elements {
                    tys.push(self.check_expr(e, env, ret_ty, in_class, loop_depth)?);
                }
                Ok(Type::Tuple(tys.into()))
            }
            ExprKind::MapLit(entries) => {
                // The parser only ever emits MapLit when there's at least
                // one `key: value` entry; `{}` parses as an empty block.
                let (k0, v0) = &entries[0];
                let k_ty = self.check_expr(k0, env, ret_ty, in_class, loop_depth)?;
                if !is_valid_map_key_type(&k_ty) {
                    return Err(TypeError::Unsupported {
                        what: format!(
                            "map key type {k_ty} (only string / int / bool keys are supported)"
                        ),
                        span: k0.span,
                    });
                }
                let v_ty = self.check_expr(v0, env, ret_ty, in_class, loop_depth)?;
                for (k, v) in &entries[1..] {
                    let kt = self.check_expr(k, env, ret_ty, in_class, loop_depth)?;
                    if !literal_assignable(k, &kt, &k_ty) && !self.assignable_obj(&kt, &k_ty) {
                        return Err(TypeError::Mismatch {
                            expected: k_ty.clone(),
                            got: kt,
                            span: k.span,
                        });
                    }
                    let vt = self.check_expr(v, env, ret_ty, in_class, loop_depth)?;
                    if !literal_assignable(v, &vt, &v_ty) && !self.assignable_obj(&vt, &v_ty) {
                        return Err(TypeError::Mismatch {
                            expected: v_ty.clone(),
                            got: vt,
                            span: v.span,
                        });
                    }
                }
                Ok(Type::generic("Map", vec![k_ty, v_ty]))
            }
            ExprKind::Index { obj, index } => {
                let ot = self.check_expr(obj, env, ret_ty, in_class, loop_depth)?;
                let it = self.check_expr(index, env, ret_ty, in_class, loop_depth)?;
                // Map<K, V> indexing: `m[k]` returns V (panics at runtime
                // if missing — use `.get(k)` for `V?`).
                if let Type::Generic(g) = &ot {
                    if g.base == "Map" && g.args.len() == 2 {
                        if !literal_assignable(index, &it, &g.args[0]) && !self.assignable_obj(&it, &g.args[0]) {
                            return Err(TypeError::Mismatch {
                                expected: g.args[0].clone(),
                                got: it,
                                span: index.span,
                            });
                        }
                        return Ok(g.args[1].clone());
                    }
                }
                // Tuple indexing: index must be a non-negative integer
                // literal so the element type is statically known.
                if let Type::Tuple(elems) = &ot {
                    let n = match &index.kind {
                        ExprKind::Int(n) if *n >= 0 => *n as usize,
                        _ => {
                            return Err(TypeError::Unsupported {
                                what: "tuple index must be a non-negative integer literal".into(),
                                span: index.span,
                            });
                        }
                    };
                    if n >= elems.len() {
                        return Err(TypeError::Unsupported {
                            what: format!(
                                "tuple index {n} out of bounds for {ot}"
                            ),
                            span: index.span,
                        });
                    }
                    return Ok(elems[n].clone());
                }
                if !it.is_int() {
                    return Err(TypeError::Mismatch {
                        expected: Type::I64,
                        got: it,
                        span: index.span,
                    });
                }
                match ot {
                    Type::Array { elem, .. } => Ok((*elem).clone()),
                    other => Err(TypeError::Mismatch {
                        expected: Type::Array {
                            elem: Box::new(Type::Any),
                            fixed: None,
                        },
                        got: other,
                        span: obj.span,
                    }),
                }
            }
            ExprKind::AssignIndex { obj, index, value } => {
                let ot = self.check_expr(obj, env, ret_ty, in_class, loop_depth)?;
                let it = self.check_expr(index, env, ret_ty, in_class, loop_depth)?;
                // Map<K, V>: `m[k] = v` desugars to `set(k, v)`.
                if let Type::Generic(g) = &ot {
                    if g.base == "Map" && g.args.len() == 2 {
                        if !literal_assignable(index, &it, &g.args[0]) && !self.assignable_obj(&it, &g.args[0]) {
                            return Err(TypeError::Mismatch {
                                expected: g.args[0].clone(),
                                got: it,
                                span: index.span,
                            });
                        }
                        let vt = self.check_expr(value, env, ret_ty, in_class, loop_depth)?;
                        if !literal_assignable(value, &vt, &g.args[1]) && !self.assignable_obj(&vt, &g.args[1]) {
                            return Err(TypeError::Mismatch {
                                expected: g.args[1].clone(),
                                got: vt,
                                span: value.span,
                            });
                        }
                        return Ok(Type::Unit);
                    }
                }
                if !it.is_int() {
                    return Err(TypeError::Mismatch {
                        expected: Type::I64,
                        got: it,
                        span: index.span,
                    });
                }
                let elem_ty = match &ot {
                    Type::Array { elem, .. } => (**elem).clone(),
                    other => {
                        return Err(TypeError::Mismatch {
                            expected: Type::Array {
                                elem: Box::new(Type::Any),
                                fixed: None,
                            },
                            got: other.clone(),
                            span: obj.span,
                        });
                    }
                };
                let vt = self.check_expr(value, env, ret_ty, in_class, loop_depth)?;
                if !literal_assignable(value, &vt, &elem_ty) && !self.assignable_obj(&vt, &elem_ty) {
                    return Err(TypeError::Mismatch {
                        expected: elem_ty,
                        got: vt,
                        span: value.span,
                    });
                }
                Ok(Type::Unit)
            }
            ExprKind::FnExpr { params, ret, body } => {
                for Param { ty, span: pspan, .. } in params {
                    self.validate_type(ty, *pspan, &[])?;
                }
                if let Some(r) = ret {
                    self.validate_type(r, span, &[])?;
                }
                // Closures capture outer locals by value. The body's
                // local env starts from the outer env so free vars
                // resolve, then params overlay.
                let mut inner: Vars = env.clone();
                for Param { name, ty, .. } in params {
                    inner.insert(name.clone(), ty.clone());
                }
                // Compute captures: free vars in the body that come
                // from the OUTER `env` (not the closure's own params,
                // not top-level fns/classes/enums). Order is
                // first-encountered for stable JIT layout.
                let mut bound: std::collections::HashSet<Symbol> =
                    params.iter().map(|p| p.name.clone()).collect();
                let mut frees: Vec<Symbol> = Vec::new();
                let mut seen: std::collections::HashSet<Symbol> = Default::default();
                collect_fn_expr_free_vars(body, &mut bound, &mut frees, &mut seen);
                let captures: Vec<(Symbol, Type)> = frees
                    .into_iter()
                    .filter_map(|n| env.get(&n).map(|t| (n, t.clone())))
                    .collect();
                self.fn_expr_captures
                    .borrow_mut()
                    .insert(span, captures);
                let expected = ret.clone().unwrap_or(Type::Unit);
                let body_ty =
                    self.check_block(body, &inner, Some(&expected), in_class, 0)?;
                if !assignable(&body_ty, &expected) && !self.assignable_obj(&body_ty, &expected) {
                    return Err(TypeError::BadReturn {
                        name: "<closure>".into(),
                        expected,
                        got: body_ty,
                        span,
                    });
                }
                Ok(Type::func(
                    params.iter().map(|p| p.ty.clone()).collect(),
                    ret.clone().unwrap_or(Type::Unit),
                ))
            }
            ExprKind::Cast { expr: inner, ty } => {
                let from = self.check_expr(inner, env, ret_ty, in_class, loop_depth)?;
                self.validate_type(ty, span, &[])?;
                // Numeric → numeric (any width) and `bool → int`
                // (0/1 conversion) are the regular path.
                let from_ok = from.is_numeric() || from == Type::Bool;
                let to_ok = ty.is_numeric();
                if from_ok && to_ok {
                    return Ok(ty.clone());
                }
                // Enum → numeric: hand back the variant's
                // discriminant value as a primitive integer.
                // Mainly useful for fieldless enums with an
                // explicit `: u32` repr (bitflag-style usage —
                // `Flag.audio as u32 | Flag.video as u32`).
                if matches!(from, Type::Object(ref n) if self.enums.contains_key(n)) && ty.is_numeric() {
                    return Ok(ty.clone());
                }
                // Numeric → enum: reinterpret an integer as one of
                // the enum's discriminants. Only allowed for
                // fieldless (unit-variant-only) enums; payloaded
                // enums have no integer representation. Lets C-side
                // out values (`SDL_GetKeyFromScancode(...) as
                // Keycode`) round-trip into the typed enum.
                if from.is_numeric() {
                    if let Type::Object(n) = ty {
                        if let Some(sig) = self.enums.get(n) {
                            let fieldless = sig.variants.iter().all(|v| {
                                matches!(v.payload, VariantPayloadSig::Unit)
                            });
                            if fieldless {
                                return Ok(ty.clone());
                            }
                        }
                    }
                }
                // FFI escape hatch — `i64 ↔ opaque-extern class
                // (without deinit)`. Lets out-pointer slots from C
                // be reinterpreted as an opaque handle and vice
                // versa. Restricted to the deinit-less form so the
                // user never accidentally constructs a phantom ARC
                // box. Cast direction must come from the user (no
                // implicit conversion elsewhere) so it stays
                // explicit at every call site.
                let opaque_no_deinit = |t: &Type| match t {
                    Type::Object(name) => self
                        .classes
                        .get(name)
                        .map(|cs| {
                            cs.extern_lib.is_some() && !cs.methods.contains_key(&"deinit".into())
                        })
                        .unwrap_or(false),
                    _ => false,
                };
                if (from == Type::I64 && opaque_no_deinit(ty))
                    || (opaque_no_deinit(&from) && *ty == Type::I64)
                {
                    return Ok(ty.clone());
                }
                // Raw C pointer ↔ i64 escape hatch — pointers are
                // bit-equivalent to a 64-bit address. Lets out-pointer
                // patterns work (read an opaque address from i64[],
                // hand it back to a `*Foo` parameter).
                let is_raw_ptr = |t: &Type| matches!(t, Type::RawPtr { .. });
                if (is_raw_ptr(&from) && *ty == Type::I64)
                    || (from == Type::I64 && is_raw_ptr(ty))
                {
                    return Ok(ty.clone());
                }
                // Raw pointer ↔ raw pointer — type-punning at the
                // C boundary (`*const u8` → `*const void`,
                // `*const char` → `*u8`, etc.). All raw pointers are
                // i64-sized at the ABI; this just reinterprets the
                // pointee type. Restricted to inside `@extern(C) {}`
                // since raw pointer values aren't supposed to surface
                // outside the block in the first place.
                if is_raw_ptr(&from) && is_raw_ptr(ty) && *self.in_extern_c.borrow() {
                    return Ok(ty.clone());
                }
                Err(TypeError::Mismatch {
                    expected: ty.clone(),
                    got: from,
                    span,
                })
            }
            ExprKind::TypeTest { expr: inner, ty } => {
                self.check_expr(inner, env, ret_ty, in_class, loop_depth)?;
                self.validate_type(ty, span, &[])?;
                Ok(Type::Bool)
            }
            ExprKind::TypeDowncast { expr: inner, ty } => {
                self.check_expr(inner, env, ret_ty, in_class, loop_depth)?;
                self.validate_type(ty, span, &[])?;
                Ok(Type::Optional(Box::new(ty.clone())))
            }
            ExprKind::AssignField { obj, field, value } => {
                // Static field write: `ClassName.field = v`.
                if let ExprKind::Var(rname) = &obj.kind {
                    let is_local_shadow = env.contains_key(rname) || self.vars.contains_key(rname);
                    if !is_local_shadow {
                        if let Some(cls) = self.classes.get(&rname) {
                            if let Some(ft) = cls.static_fields.get(field).cloned() {
                                if cls.static_const_fields.contains(field) {
                                    return Err(TypeError::Unsupported {
                                        what: format!(
                                            "cannot assign to const static field {:?}.{:?}",
                                            rname, field
                                        ),
                                        span,
                                    });
                                }
                                let vt =
                                    self.check_expr(value, env, ret_ty, in_class, loop_depth)?;
                                if !literal_assignable(value, &vt, &ft) && !self.assignable_obj(&vt, &ft) {
                                    return Err(TypeError::Mismatch {
                                        expected: ft,
                                        got: vt,
                                        span: value.span,
                                    });
                                }
                                return Ok(Type::Unit);
                            }
                        }
                    }
                }
                let ot = self.check_expr(obj, env, ret_ty, in_class, loop_depth)?;
                let class_name = expect_object(&ot, obj.span)?;
                let cls = self.classes.get(&class_name).ok_or_else(|| {
                    TypeError::UndefinedClass {
                        name: class_name.into(),
                        span: obj.span,
                    }
                })?;
                // Property `set` precedes field lookup. Read-only
                // properties (no setter) reject the assignment.
                if let Some(p) = cls.properties.get(field) {
                    if !p.has_set {
                        return Err(TypeError::Unsupported {
                            what: format!(
                                "property {:?}.{} has no setter (read-only)",
                                class_name, field
                            ),
                            span,
                        });
                    }
                    let prop_ty =
                        subst_type(&p.ty, &cls.type_params, type_args_of(&ot));
                    let v_ty =
                        self.check_expr(value, env, ret_ty, in_class, loop_depth)?;
                    if !literal_assignable(value, &v_ty, &prop_ty) && !self.assignable_obj(&v_ty, &prop_ty) {
                        return Err(TypeError::Mismatch {
                            expected: prop_ty,
                            got: v_ty,
                            span: value.span,
                        });
                    }
                    return Ok(Type::Unit);
                }
                let raw_field_ty = cls.fields.get(field).cloned().ok_or_else(|| {
                    TypeError::UnknownField {
                        class: class_name.into(),
                        field: field.clone(),
                        span,
                    }
                })?;
                // Substitute the receiver's generic type args so a
                // `Box<i64>.x = 100` check sees `i64` for `x: T`.
                // Mirrors the substitution done by the Field read path.
                let field_ty = subst_type(&raw_field_ty, &cls.type_params, type_args_of(&ot));
                let v_ty = self.check_expr(value, env, ret_ty, in_class, loop_depth)?;
                if !literal_assignable(value, &v_ty, &field_ty) && !self.assignable_obj(&v_ty, &field_ty) {
                    return Err(TypeError::Mismatch {
                        expected: field_ty,
                        got: v_ty,
                        span: value.span,
                    });
                }
                Ok(Type::Unit)
            }
            ExprKind::None => Ok(Type::Optional(Box::new(Type::Any))),
            ExprKind::Some(inner) => {
                let it = self.check_expr(inner, env, ret_ty, in_class, loop_depth)?;
                Ok(Type::Optional(Box::new(it)))
            }
            ExprKind::IfLet {
                name,
                expr,
                then_branch,
                else_branch,
            } => {
                let scrut_ty = self.check_expr(expr, env, ret_ty, in_class, loop_depth)?;
                let inner = match &scrut_ty {
                    Type::Optional(t) => (**t).clone(),
                    _ => {
                        return Err(TypeError::Mismatch {
                            expected: Type::Optional(Box::new(Type::Any)),
                            got: scrut_ty,
                            span: expr.span,
                        });
                    }
                };
                // Inner must be concrete for the binding to be useful;
                // we reject `if let some(x) = none` because the type of
                // x would be `Any`.
                if matches!(inner, Type::Any) {
                    return Err(TypeError::Mismatch {
                        expected: Type::Optional(Box::new(Type::Any)),
                        got: scrut_ty,
                        span: expr.span,
                    });
                }
                let mut then_env = env.clone();
                then_env.insert(name.clone(), inner);
                let then_ty = self.check_block(then_branch, &then_env, ret_ty, in_class, loop_depth)?;
                if let Some(eb) = else_branch {
                    let else_ty = self.check_expr(eb, env, ret_ty, in_class, loop_depth)?;
                    // Pick the unifying type: if either branch is Unit, the
                    // overall expr is Unit (statement-style); otherwise the
                    // two branches must agree.
                    if matches!(then_ty, Type::Unit) || matches!(else_ty, Type::Unit) {
                        Ok(Type::Unit)
                    } else if assignable(&else_ty, &then_ty) {
                        Ok(then_ty)
                    } else if assignable(&then_ty, &else_ty) {
                        Ok(else_ty)
                    } else if let Some(merged) = merge_generic_with_holes(&then_ty, &else_ty) {
                        // Each branch fixed a different generic hole
                        // (e.g. `Result<i64, Any>` and `Result<Any, string>`)
                        // — merge to the more specific shape. Mirrors the
                        // regular if/else path.
                        Ok(merged)
                    } else if let Some(t) = then_branch.tail.as_deref() {
                        if numeric_literal_fits(t, &else_ty) {
                            Ok(else_ty)
                        } else if numeric_literal_fits(eb, &then_ty) {
                            Ok(then_ty)
                        } else {
                            Err(TypeError::Mismatch {
                                expected: then_ty,
                                got: else_ty,
                                span,
                            })
                        }
                    } else if numeric_literal_fits(eb, &then_ty) {
                        Ok(then_ty)
                    } else {
                        Err(TypeError::Mismatch {
                            expected: then_ty,
                            got: else_ty,
                            span,
                        })
                    }
                } else {
                    // No else: the result is Unit even if then has a value.
                    Ok(Type::Unit)
                }
            }
            ExprKind::EnumCtor {
                enum_name,
                variant,
                args,
            } => {
                let sig = self.enums.get(enum_name).cloned().ok_or_else(|| {
                    TypeError::UndefinedClass {
                        name: enum_name.clone(),
                        span,
                    }
                })?;
                let v = sig.variants.iter().find(|v| v.name == *variant).ok_or_else(|| {
                    TypeError::Unsupported {
                        what: format!("enum {enum_name:?} has no variant {variant:?}"),
                        span,
                    }
                })?;
                let type_params = sig.type_params.clone();
                // First pass: gather arg types, infer type-parameter
                // bindings from the (parametric payload type, arg type)
                // pairs. Bindings absent here stay as `Any`, to be
                // refined by an outer annotation.
                let mut bindings: HashMap<Symbol, Type> = HashMap::new();
                let mut arg_tys_tuple: Vec<Type> = Vec::new();
                let mut arg_tys_struct: Vec<(Symbol, Type)> = Vec::new();
                match (&v.payload, args) {
                    (VariantPayloadSig::Unit, CtorArgs::Unit) => {}
                    (VariantPayloadSig::Tuple(tys), CtorArgs::Tuple(elems)) => {
                        if tys.len() != elems.len() {
                            return Err(TypeError::ArityMismatch {
                                name: Symbol::intern(&format!("{enum_name}::{variant}")),
                                expected: tys.len(),
                                got: elems.len(),
                                span,
                            });
                        }
                        for (e, t) in elems.iter().zip(tys.iter()) {
                            let et = self.check_expr(e, env, ret_ty, in_class, loop_depth)?;
                            collect_type_var_bindings(t, &et, &mut bindings);
                            arg_tys_tuple.push(et);
                        }
                    }
                    (VariantPayloadSig::Struct(fields), CtorArgs::Struct(provided)) => {
                        if provided.len() != fields.len() {
                            return Err(TypeError::ArityMismatch {
                                name: Symbol::intern(&format!("{enum_name}::{variant}")),
                                expected: fields.len(),
                                got: provided.len(),
                                span,
                            });
                        }
                        for (fname, fty) in fields {
                            let supplied = provided.iter().find(|(n, _)| n == fname).ok_or_else(
                                || TypeError::UnknownField {
                                    class: Symbol::intern(&format!("{enum_name}::{variant}")),
                                    field: fname.clone(),
                                    span,
                                },
                            )?;
                            let st = self.check_expr(
                                &supplied.1,
                                env,
                                ret_ty,
                                in_class,
                                loop_depth,
                            )?;
                            collect_type_var_bindings(fty, &st, &mut bindings);
                            arg_tys_struct.push((fname.clone(), st));
                        }
                    }
                    _ => {
                        return Err(TypeError::Unsupported {
                            what: format!(
                                "constructor shape for {enum_name}::{variant} doesn't match its declaration"
                            ),
                            span,
                        });
                    }
                }
                // Build inferred type-arg vector (Any for unsolved).
                let inferred_args: Vec<Type> = type_params
                    .iter()
                    .map(|p| bindings.get(p).cloned().unwrap_or(Type::Any))
                    .collect();
                // Stash for the JIT enum-monomorphization pass. Args
                // may still contain TypeVars when the call sits inside
                // another generic context — that's resolved at
                // expansion time. Always recorded (even for non-generic
                // enums) since the cost is trivial.
                if !type_params.is_empty() {
                    self.enum_ctor_type_args
                        .borrow_mut()
                        .insert(span, (enum_name.clone(), inferred_args.clone()));
                }
                // Validate each arg against the substituted payload type.
                match (&v.payload, args) {
                    (VariantPayloadSig::Unit, _) => {}
                    (VariantPayloadSig::Tuple(tys), CtorArgs::Tuple(elems)) => {
                        for ((e, t), et) in elems.iter().zip(tys.iter()).zip(arg_tys_tuple.iter()) {
                            let actual = subst_type(t, &type_params, &inferred_args);
                            if !literal_assignable(e, et, &actual) && !self.assignable_obj(et, &actual) {
                                return Err(TypeError::Mismatch {
                                    expected: actual,
                                    got: et.clone(),
                                    span: e.span,
                                });
                            }
                        }
                    }
                    (VariantPayloadSig::Struct(fields), CtorArgs::Struct(provided)) => {
                        for (fname, fty) in fields {
                            let supplied = provided.iter().find(|(n, _)| n == fname).unwrap();
                            let st = arg_tys_struct
                                .iter()
                                .find(|(n, _)| n == fname)
                                .map(|(_, t)| t.clone())
                                .unwrap();
                            let actual = subst_type(fty, &type_params, &inferred_args);
                            if !literal_assignable(&supplied.1, &st, &actual) && !self.assignable_obj(&st, &actual) {
                                return Err(TypeError::Mismatch {
                                    expected: actual,
                                    got: st,
                                    span: supplied.1.span,
                                });
                            }
                        }
                    }
                    _ => {}
                }
                Ok(if type_params.is_empty() {
                    Type::Object(enum_name.clone())
                } else {
                    Type::generic(enum_name.clone(), inferred_args)
                })
            }
            ExprKind::Match { scrutinee, arms } => {
                let st = self.check_expr(scrutinee, env, ret_ty, in_class, loop_depth)?;
                // Match on a primitive (integer / bool / string)
                // is allowed, with `IntLit` / `BoolLit` / `StrLit`
                // patterns. Bool literals appear as
                // `Variant{name: "true"|"false"}` from the parser,
                // which we treat as `BoolLit` here.
                if st.is_numeric() || st == Type::Bool || st == Type::Str {
                    return self.check_match_primitive(&st, arms, span, env, ret_ty, in_class, loop_depth);
                }
                let (enum_name, scrut_args) = match &st {
                    Type::Object(name) if self.enums.contains_key(name) => {
                        (name.clone(), Vec::<Type>::new())
                    }
                    Type::Generic(g) if self.enums.contains_key(&g.base) => {
                        (g.base.clone(), g.args.to_vec())
                    }
                    _ => {
                        return Err(TypeError::Mismatch {
                            expected: Type::Object("<enum>".into()),
                            got: st,
                            span: scrutinee.span,
                        });
                    }
                };
                let sig = self.enums[&enum_name].clone();
                let enum_params = sig.type_params.clone();
                let mut covered: std::collections::HashSet<Symbol> =
                    std::collections::HashSet::new();
                let mut has_wildcard = false;
                let mut result_ty: Option<Type> = None;
                for arm in arms {
                    if has_wildcard {
                        return Err(TypeError::Unsupported {
                            what: "match arm after wildcard `_` is unreachable".into(),
                            span: arm.span,
                        });
                    }
                    let mut arm_env = env.clone();
                    let arm_kind_span = arm.pattern.span;
                    match &arm.pattern.kind {
                        PatternKind::Wildcard => {
                            has_wildcard = true;
                        }
                        PatternKind::IntLit(_)
                        | PatternKind::IntRange { .. }
                        | PatternKind::BoolLit(_)
                        | PatternKind::StrLit(_) => {
                            return Err(TypeError::Unsupported {
                                what: format!(
                                    "literal pattern not allowed when matching enum {enum_name:?}"
                                ),
                                span: arm_kind_span,
                            });
                        }
                        PatternKind::Variant {
                            enum_name: pat_enum,
                            variant,
                            bindings,
                        } => {
                            // Short form (`Variant ...` without `Enum::`)
                            // borrows the scrutinee's enum name. Long
                            // form must match it exactly.
                            if let Some(pe) = pat_enum {
                                if pe != &enum_name {
                                    return Err(TypeError::Mismatch {
                                        expected: Type::Object(enum_name.clone()),
                                        got: Type::Object(pe.clone()),
                                        span: arm_kind_span,
                                    });
                                }
                            }
                            let v = sig
                                .variants
                                .iter()
                                .find(|v| v.name == *variant)
                                .ok_or_else(|| TypeError::Unsupported {
                                    what: format!(
                                        "enum {enum_name:?} has no variant {variant:?}"
                                    ),
                                    span: arm_kind_span,
                                })?;
                            if !covered.insert(variant.clone()) {
                                return Err(TypeError::Unsupported {
                                    what: format!("duplicate match arm for {variant:?}"),
                                    span: arm_kind_span,
                                });
                            }
                            // Check binding shape matches and add bindings.
                            // Generic enums: substitute the scrutinee's
                            // concrete type args into each parametric
                            // payload type before binding the name.
                            match (&v.payload, bindings) {
                                (VariantPayloadSig::Unit, PatternBindings::Unit) => {}
                                (
                                    VariantPayloadSig::Tuple(tys),
                                    PatternBindings::Tuple(names),
                                ) => {
                                    if tys.len() != names.len() {
                                        return Err(TypeError::ArityMismatch {
                                            name: Symbol::intern(&format!("{enum_name}::{variant}")),
                                            expected: tys.len(),
                                            got: names.len(),
                                            span: arm_kind_span,
                                        });
                                    }
                                    for (n, t) in names.iter().zip(tys.iter()) {
                                        if n != "_" {
                                            let bind_ty =
                                                subst_type(t, &enum_params, &scrut_args);
                                            arm_env.insert(n.clone(), bind_ty);
                                        }
                                    }
                                }
                                (
                                    VariantPayloadSig::Struct(fields),
                                    PatternBindings::Struct(pairs),
                                ) => {
                                    for (fname, bname) in pairs {
                                        let fty = fields
                                            .iter()
                                            .find(|(n, _)| n == fname)
                                            .map(|(_, t)| t.clone())
                                            .ok_or_else(|| TypeError::UnknownField {
                                                class: Symbol::intern(&format!("{enum_name}::{variant}")),
                                                field: fname.clone(),
                                                span: arm_kind_span,
                                            })?;
                                        if bname != "_" {
                                            let bind_ty =
                                                subst_type(&fty, &enum_params, &scrut_args);
                                            arm_env.insert(bname.clone(), bind_ty);
                                        }
                                    }
                                }
                                _ => {
                                    return Err(TypeError::Unsupported {
                                        what: format!(
                                            "pattern shape for {enum_name}::{variant} doesn't match its declaration"
                                        ),
                                        span: arm_kind_span,
                                    });
                                }
                            }
                        }
                    }
                    let bt = self.check_expr(&arm.body, &arm_env, ret_ty, in_class, loop_depth)?;
                    result_ty = Some(match result_ty {
                        None => bt,
                        Some(prev) => unify_branch(prev, bt, arm.body.span)?,
                    });
                }
                if !has_wildcard {
                    let total = sig.variants.len();
                    if covered.len() != total {
                        let missing: Vec<_> = sig
                            .variants
                            .iter()
                            .filter(|v| !covered.contains(&v.name))
                            .map(|v| v.name.as_str())
                            .collect::<Vec<_>>();
                        return Err(TypeError::Unsupported {
                            what: format!(
                                "non-exhaustive match on {enum_name}: missing {}",
                                missing.join(", ")
                            ),
                            span,
                        });
                    }
                }
                Ok(result_ty.unwrap_or(Type::Unit))
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    /// Resolve which method overload (or init) a call site invokes.
    /// Returns the chosen Signature; records the pick in the side
    /// table so the post-typecheck mangler can rewrite the call.
    #[allow(clippy::too_many_arguments)]
    fn resolve_method_call(
        &self,
        class_name: Symbol,
        method: Symbol,
        sigs: &[Signature],
        args: &[Expr],
        env: &Vars,
        ret_ty: Option<&Type>,
        in_class: Option<Symbol>,
        loop_depth: u32,
        span: Span,
    ) -> Result<Signature, TypeError> {
        // Single overload: keep the precise check_args path so error
        // variants (Mismatch / ArityMismatch) match what users expect.
        if sigs.len() == 1 {
            self.method_overload_pick
                .borrow_mut()
                .insert(span, (class_name.into(), method.into(), 0));
            self.check_args(method, &sigs[0], args, env, ret_ty, in_class, loop_depth, span)?;
            return Ok(sigs[0].clone());
        }
        // Multiple overloads: score and pick.
        let mut arg_tys: Vec<Type> = Vec::with_capacity(args.len());
        for a in args {
            arg_tys.push(self.check_expr(a, env, ret_ty, in_class, loop_depth)?);
        }
        let chosen = resolve_overload(method, sigs, &arg_tys, args, span)?;
        let cs = sigs[chosen].clone();
        self.method_overload_pick
            .borrow_mut()
            .insert(span, (class_name.into(), method.into(), chosen));
        self.check_args(method, &cs, args, env, ret_ty, in_class, loop_depth, span)?;
        Ok(cs)
    }

    fn check_args(
        &self,
        name: Symbol,
        sig: &Signature,
        args: &[Expr],
        env: &Vars,
        ret_ty: Option<&Type>,
        in_class: Option<Symbol>,
        loop_depth: u32,
        call_span: Span,
    ) -> Result<(), TypeError> {
        if sig.variadic {
            // Variadic: the declared params are the **fixed prefix**;
            // each arg before that index is type-checked against the
            // declared type (so e.g. `printf`'s `fmt: string` is
            // enforced). Extra args after the prefix are checked
            // permissively — they flow through to the C side at
            // their actual JIT-time types.
            if args.len() < sig.params.len() {
                return Err(TypeError::ArityMismatch {
                    name: name.into(),
                    expected: sig.params.len(),
                    got: args.len(),
                    span: call_span,
                });
            }
            for (i, arg) in args.iter().enumerate() {
                let at = self.check_expr(arg, env, ret_ty, in_class, loop_depth)?;
                if i < sig.params.len() {
                    let p = &sig.params[i];
                    if !matches!(p, Type::Any)
                        && !literal_assignable(arg, &at, p)
                        && !self.assignable_obj(&at, p)
                    {
                        return Err(TypeError::Mismatch {
                            expected: p.clone(),
                            got: at,
                            span: arg.span,
                        });
                    }
                }
            }
            return Ok(());
        }
        // Default-arg fill: when args are short, try to append the
        // trailing defaults stored on the signature. If any required
        // (no-default) trailing slot is missing, fall through to the
        // normal arity-mismatch error below.
        let filled: Vec<Expr> = if args.len() < sig.params.len() {
            let missing = sig.params.len() - args.len();
            let mut appended: Vec<Expr> = Vec::with_capacity(missing);
            let mut ok = true;
            for d in sig.defaults.iter().skip(args.len()).take(missing) {
                match d {
                    Some(e) => appended.push(e.clone()),
                    None => {
                        ok = false;
                        break;
                    }
                }
            }
            if ok {
                self.call_default_fills
                    .borrow_mut()
                    .insert(call_span, appended.clone());
                args.iter().cloned().chain(appended.into_iter()).collect()
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };
        let effective: &[Expr] = if filled.is_empty() { args } else { &filled };
        if sig.params.len() != effective.len() {
            return Err(TypeError::ArityMismatch {
                name: name.into(),
                expected: sig.params.len(),
                got: args.len(),
                span: call_span,
            });
        }
        for (param_ty, arg) in sig.params.iter().zip(effective.iter()) {
            let at = self.check_expr(arg, env, ret_ty, in_class, loop_depth)?;
            if !literal_assignable(arg, &at, param_ty)
                && !self.assignable_obj(&at, param_ty)
            {
                return Err(TypeError::Mismatch {
                    expected: param_ty.clone(),
                    got: at,
                    span: arg.span,
                });
            }
        }
        Ok(())
    }
}

/// FFI marshalling helpers — only callable inside an `@extern(C) {}`
/// block. Listed here so the call-site check can fire even on the
/// helpers whose signatures don't reference any C-only type
/// (`errnoCheck` / `errnoCheckI64`), which would otherwise sneak
/// past the C-only-types rule.
const FFI_HELPERS: &[&str] = &[
    "stringFromCstr",
    "cstrFromString",
    "freeCstr",
    "bytesFromBuffer",
    "readI8",
    "readI16",
    "readI32",
    "readI64",
    "readU8",
    "readU16",
    "readU32",
    "readU64",
    "readF32",
    "readF64",
    "writeI8",
    "writeI16",
    "writeI32",
    "writeI64",
    "writeU8",
    "writeU16",
    "writeU32",
    "writeU64",
    "writeF32",
    "writeF64",
    "fnAddr",
    "arrayFromCArray",
    "cstrArrayToStrings",
    "errnoCheck",
    "errnoCheckI64",
];

/// Return the first C-only type encountered in `t` (raw pointer,
/// `char`, `void`, `size_t`, `ssize_t`), recursing through composite
/// shapes. `None` if `t` is fully ilang-native.
fn first_c_only_type(t: &Type) -> Option<&Type> {
    match t {
        Type::RawPtr { .. } | Type::CVoid | Type::CChar | Type::Size | Type::SSize => Some(t),
        Type::Array { elem, .. } => first_c_only_type(elem),
        Type::Optional(inner) | Type::Weak(inner) => first_c_only_type(inner),
        Type::Generic(g) => g.args.iter().find_map(first_c_only_type),
        Type::Fn(ft) => ft.params
            .iter()
            .find_map(first_c_only_type)
            .or_else(|| first_c_only_type(&ft.ret)),
        Type::Tuple(elems) => elems.iter().find_map(first_c_only_type),
        _ => None,
    }
}

/// Walk a parametric payload type alongside a concrete arg type and
/// record bindings for each `TypeVar` encountered. Used by the enum
/// constructor checker to infer type arguments from call args.
/// First-found binding wins for any given TypeVar.
fn collect_type_var_bindings(
    payload: &Type,
    arg: &Type,
    bindings: &mut HashMap<Symbol, Type>,
) {
    match (payload, arg) {
        (Type::TypeVar(name), other) => {
            bindings.entry(name.clone()).or_insert_with(|| other.clone());
        }
        (Type::Array { elem: pe, .. }, Type::Array { elem: ae, .. }) => {
            collect_type_var_bindings(pe, ae, bindings);
        }
        (Type::Optional(p), Type::Optional(a)) => {
            collect_type_var_bindings(p, a, bindings);
        }
        (Type::Weak(p), Type::Weak(a)) => {
            collect_type_var_bindings(p, a, bindings);
        }
        (Type::Generic(pg), Type::Generic(ag)) => {
            for (p, a) in pg.args.iter().zip(ag.args.iter()) {
                collect_type_var_bindings(p, a, bindings);
            }
        }
        (Type::RawPtr { inner: pi, .. }, Type::RawPtr { inner: ai, .. }) => {
            collect_type_var_bindings(pi, ai, bindings);
        }
        (Type::Tuple(pe), Type::Tuple(ae)) => {
            for (p, a) in pe.iter().zip(ae.iter()) {
                collect_type_var_bindings(p, a, bindings);
            }
        }
        (Type::Fn(pf), Type::Fn(af)) => {
            for (p, a) in pf.params.iter().zip(af.params.iter()) {
                collect_type_var_bindings(p, a, bindings);
            }
            collect_type_var_bindings(&pf.ret, &af.ret, bindings);
        }
        _ => {}
    }
}

/// Map keys are constrained to types with stable structural equality.
/// Floats are excluded (NaN), as are heap objects and arrays.
fn is_valid_map_key_type(t: &Type) -> bool {
    matches!(
        t,
        Type::Str | Type::Bool
            | Type::I8 | Type::I16 | Type::I32 | Type::I64
            | Type::U8 | Type::U16 | Type::U32 | Type::U64
    )
}

fn is_reserved_class(name: &str) -> bool {
    matches!(name, "Console" | "Map" | "Result" | "Type" | "TypeKind")
}

fn is_reserved_global(name: &str) -> bool {
    matches!(name, "console" | "typeof")
}

/// Walk `e` and call `tc.refine_enum_ctor_args(inner, target)` on
/// every `return inner` we encounter. Used by `check_fn` to propagate
/// the declared return type into early-return enum-ctor sites.
fn refine_returns(tc: &TypeChecker, e: &Expr, target: &Type) {
    if let ExprKind::Return(Some(inner)) = &e.kind {
        tc.refine_enum_ctor_args(inner, target);
    }
    walk_children(e, &mut |c| refine_returns(tc, c, target));
}

/// Visit every direct child Expr of `e`. A small structural walk used
/// only by `refine_returns`; not optimized.
fn walk_children(e: &Expr, f: &mut dyn FnMut(&Expr)) {
    match &e.kind {
        ExprKind::Some(x) | ExprKind::Unary { expr: x, .. } => f(x),
        ExprKind::Binary { lhs, rhs, .. } | ExprKind::Logical { lhs, rhs, .. } => {
            f(lhs);
            f(rhs);
        }
        ExprKind::Cast { expr, .. }
        | ExprKind::TypeTest { expr, .. }
        | ExprKind::TypeDowncast { expr, .. } => f(expr),
        ExprKind::Call { args, .. } => {
            for a in args {
                f(a);
            }
        }
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
        ExprKind::If { cond, then_branch, else_branch } => {
            f(cond);
            walk_block_children(then_branch, f);
            if let Some(e) = else_branch {
                f(e);
            }
        }
        ExprKind::IfLet { expr, then_branch, else_branch, .. } => {
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
            f(start);
            f(end);
        }
        ExprKind::Return(Some(x)) => f(x),
        ExprKind::Assign { value, .. }
        | ExprKind::AssignField { value, .. }
        | ExprKind::AssignIndex { value, .. } => f(value),
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
        _ => {}
    }
}

fn walk_block_children(b: &ilang_ast::Block, f: &mut dyn FnMut(&Expr)) {
    for s in &b.stmts {
        match &s.kind {
            StmtKind::Let { value, .. } => f(value),
            StmtKind::Expr(e) => f(e),
        }
    }
    if let Some(t) = &b.tail {
        f(t);
    }
}

// ─── overload resolution ──────────────────────────────────────────────

/// Score how well an actual arg type fits a parameter type. `None`
/// means the conversion isn't allowed at all; lower numbers mean a
/// closer match. Used to rank overloads when multiple are viable.
fn score_arg(arg: &Expr, arg_ty: &Type, param_ty: &Type) -> Option<u32> {
    if arg_ty == param_ty {
        return Some(0);
    }
    // `Type::Any` (e.g. inside `console.log(x)` — used elsewhere)
    // matches every concrete type with cost 1 so concrete overloads win.
    if matches!(arg_ty, Type::Any) || matches!(param_ty, Type::Any) {
        return Some(1);
    }
    // Same-sign integer widening / narrowing — implicit per syntax.md §2.
    if arg_ty.is_int() && param_ty.is_int() {
        let same_sign = arg_ty.is_signed_int() == param_ty.is_signed_int();
        if same_sign {
            return Some(1);
        }
        // Differing signs need an explicit `as` cast — not viable here.
        return None;
    }
    // Int → float (also widening between f32 / f64) — implicit.
    if arg_ty.is_int() && param_ty.is_float() {
        return Some(2);
    }
    if matches!((arg_ty, param_ty), (Type::F32, Type::F64) | (Type::F64, Type::F32)) {
        return Some(1);
    }
    // T → T? auto-wrap.
    if let Type::Optional(inner) = param_ty {
        if let Some(inner_score) = score_arg(arg, arg_ty, inner) {
            return Some(inner_score + 3);
        }
    }
    // Object → Weak downgrade (same class).
    if let (Type::Object(a), Type::Weak(b_inner)) = (arg_ty, param_ty) {
        if let Type::Object(b) = b_inner.as_ref() {
            if a == b {
                return Some(4);
            }
        }
    }
    // Fall back to literal_assignable: catches int-literal widening
    // into smaller widths (`1` into `i8`) and similar.
    if literal_assignable(arg, arg_ty, param_ty) {
        return Some(2);
    }
    None
}

/// Pick the best matching overload from `sigs`. Returns the index of
/// the chosen signature, or a TypeError if none is viable / multiple
/// tie for best score.
fn resolve_overload(
    name: Symbol,
    sigs: &[Signature],
    arg_tys: &[Type],
    args: &[Expr],
    span: Span,
) -> Result<usize, TypeError> {
    // Variadic built-ins live in this slot too — accept the first that
    // matches arity (which for variadics means "any arg count").
    let mut viable: Vec<(usize, u32)> = Vec::new();
    for (i, sig) in sigs.iter().enumerate() {
        if sig.variadic {
            // Variadic: any arity, no per-arg scoring needed.
            viable.push((i, 0));
            continue;
        }
        if sig.params.len() < arg_tys.len() {
            continue;
        }
        // Default-arg fill: a sig with more params than args is
        // viable iff every unfilled trailing slot has a default.
        // Each filled-by-default slot adds a flat penalty so an
        // exact-arity overload always beats a default-filled one.
        let missing = sig.params.len() - arg_tys.len();
        if missing > 0 {
            let have_defaults = sig
                .defaults
                .iter()
                .skip(arg_tys.len())
                .take(missing)
                .all(|d| d.is_some());
            if !have_defaults {
                continue;
            }
        }
        let mut total = 0u32;
        let mut all_ok = true;
        for ((expected, actual), arg) in sig.params.iter().zip(arg_tys.iter()).zip(args.iter()) {
            match score_arg(arg, actual, expected) {
                Some(s) => total += s,
                None => {
                    all_ok = false;
                    break;
                }
            }
        }
        if all_ok {
            // Penalty: each defaulted slot costs 1000, dwarfing any
            // implicit-conversion delta so an exact-arity match wins
            // first.
            total += (missing as u32) * 1000;
            viable.push((i, total));
        }
    }
    if viable.is_empty() {
        return Err(TypeError::Unsupported {
            what: format!(
                "no matching overload for `{name}` with arg types ({})",
                arg_tys.iter().map(|t| format!("{t}")).collect::<Vec<_>>().join(", "),
            ),
            span,
        });
    }
    // Pick lowest score; tie → ambiguous.
    viable.sort_by_key(|(_, s)| *s);
    let best = viable[0].1;
    let tied: Vec<usize> = viable.iter().take_while(|(_, s)| *s == best).map(|(i, _)| *i).collect();
    if tied.len() > 1 {
        return Err(TypeError::Unsupported {
            what: format!(
                "ambiguous call to `{name}` — multiple overloads match equally well \
                 ({} candidates)",
                tied.len()
            ),
            span,
        });
    }
    Ok(tied[0])
}

fn signature_of(f: &FnDecl) -> Signature {
    // Rewrite the fn's own `<T, U>` type parameters from `Object(T)` to
    // `TypeVar(T)` so call-site inference (which substitutes for
    // `TypeVar`) fires. Methods rewrite the *class's* type params on top
    // of this in `class_signature`.
    let params: Vec<Type> = f
        .params
        .iter()
        .map(|p| rewrite_type_params(&p.ty, &f.type_params))
        .collect();
    let ret = rewrite_type_params(
        &f.ret.clone().unwrap_or(Type::Unit),
        &f.type_params,
    );
    // `@extern("...", variadic)` propagates to the signature so the
    // type checker accepts trailing args of any type at call sites.
    let is_variadic = f.attrs.iter().any(|a| {
        a.name == "extern"
            && a.args
                .iter()
                .any(|x| matches!(x, ilang_ast::AttrArg::Path(p) if p.iter().map(|s| s.as_str()).collect::<Vec<_>>() == ["variadic"]))
    });
    Signature {
        params,
        ret,
        variadic: is_variadic,
        decl_span: f.span,
        type_params: Vec::from(f.type_params.clone()),
        defaults: f.params.iter().map(|p| p.default.clone()).collect(),
    }
}

fn class_signature(
    c: &ClassDecl,
    parent: Option<&ClassSig>,
) -> Result<ClassSig, TypeError> {
    // Inheritance restrictions: the parent must not be generic
    // (we don't substitute type params across the boundary), and
    // the child can't add type params either if it inherits.
    if let Some(p) = parent {
        if !p.type_params.is_empty() || !c.type_params.is_empty() {
            return Err(TypeError::Unsupported {
                what: format!(
                    "class {:?}: inheritance with generic classes is not supported",
                    c.name
                ),
                span: c.span,
            });
        }
    }
    // Start from the parent's tables and overlay this class's
    // declarations. Fields and methods are inherited; same-named
    // child decl overrides (must be explicitly marked `override`
    // for methods).
    let mut fields: HashMap<Symbol, Type> = parent
        .map(|p| p.fields.clone())
        .unwrap_or_default();
    for f in &c.fields {
        if fields.contains_key(&f.name) {
            return Err(TypeError::Unsupported {
                what: format!(
                    "class {:?}: field {:?} shadows an inherited field of the same name",
                    c.name, f.name
                ),
                span: f.span,
            });
        }
        fields.insert(f.name.clone(), rewrite_type_params(&f.ty, &c.type_params));
    }
    let mut methods: HashMap<Symbol, Vec<Signature>> = parent
        .map(|p| p.methods.clone())
        .unwrap_or_default();
    let mut method_slots: HashMap<Symbol, usize> = parent
        .map(|p| p.method_slots.clone())
        .unwrap_or_default();
    let mut vtable_len: usize = parent.map(|p| p.vtable_len).unwrap_or(0);
    let has_parent = parent.is_some();
    // Track which init/deinit names this child has declared this
    // pass — needed because `methods` starts with parent entries
    // already populated, so a "first child decl overwrites parent"
    // is legitimate but a second one is a duplicate.
    let mut child_special_seen: HashSet<Symbol> = HashSet::new();
    // Pass 1: handle inheritance interactions (override / hiding / no-overload).
    for m in &c.methods {
        // `init` and `deinit` are per-class — they're NOT inherited
        // in the override sense. Pass 1 just overwrites whatever the
        // parent had (without requiring `override`); pass 2 skips
        // them since `has_parent` is true.
        if m.name == "init" || m.name == "deinit" {
            if has_parent {
                // Inheritance disallows overloading, including for
                // init/deinit. The root-class dup check below only
                // runs when there's no parent, so catch duplicates
                // here.
                if !child_special_seen.insert(m.name.clone()) {
                    return Err(TypeError::Unsupported {
                        what: format!(
                            "class {:?} declares `{}` more than once",
                            c.name, m.name
                        ),
                        span: m.span,
                    });
                }
                let mut sig = signature_of(m);
                for p in sig.params.iter_mut() {
                    *p = rewrite_type_params(p, &c.type_params);
                }
                sig.ret = rewrite_type_params(&sig.ret, &c.type_params);
                methods.insert(m.name.clone(), vec![sig]);
            }
            continue;
        }
        let inherited = parent
            .map(|p| p.methods.contains_key(&m.name))
            .unwrap_or(false);
        if m.is_override && !inherited {
            return Err(TypeError::Unsupported {
                what: format!(
                    "method {:?} in class {:?} is `override` but no parent \
                     declares a method by that name",
                    m.name, c.name
                ),
                span: m.span,
            });
        }
        if inherited && !m.is_override {
            return Err(TypeError::Unsupported {
                what: format!(
                    "method {:?} in class {:?} hides a parent method without \
                     the `override` keyword",
                    m.name, c.name
                ),
                span: m.span,
            });
        }
        if inherited {
            // Override: replace parent's entry, reuse parent's slot.
            let parent_sigs = parent.unwrap().methods.get(&m.name).unwrap();
            if parent_sigs.len() != 1 {
                return Err(TypeError::Unsupported {
                    what: format!(
                        "method {:?} in parent of class {:?} is overloaded; \
                         cannot be overridden",
                        m.name, c.name
                    ),
                    span: m.span,
                });
            }
            let parent_sig = &parent_sigs[0];
            let mut sig = signature_of(m);
            for p in sig.params.iter_mut() {
                *p = rewrite_type_params(p, &c.type_params);
            }
            sig.ret = rewrite_type_params(&sig.ret, &c.type_params);
            if sig.params != parent_sig.params || sig.ret != parent_sig.ret {
                return Err(TypeError::Unsupported {
                    what: format!(
                        "override of method {:?} in class {:?} has a different \
                         signature than the parent's declaration",
                        m.name, c.name
                    ),
                    span: m.span,
                });
            }
            methods.insert(m.name.clone(), vec![sig]);
            continue;
        }
        // Not inherited. With a parent, single-sig only.
        if has_parent {
            if methods.contains_key(&m.name) {
                return Err(TypeError::Unsupported {
                    what: format!(
                        "method {:?} in class {:?} cannot be overloaded \
                         (overloading is not supported in inheritance hierarchies)",
                        m.name, c.name
                    ),
                    span: m.span,
                });
            }
            let mut sig = signature_of(m);
            for p in sig.params.iter_mut() {
                *p = rewrite_type_params(p, &c.type_params);
            }
            sig.ret = rewrite_type_params(&sig.ret, &c.type_params);
            methods.insert(m.name.clone(), vec![sig]);
            if m.name != "init" && m.name != "deinit" {
                method_slots.insert(m.name.clone(), vtable_len);
                vtable_len += 1;
            }
        }
    }
    // Pass 2: legacy overload-aware loop for root classes only.
    for m in &c.methods {
        if has_parent {
            continue;
        }
        let mut sig = signature_of(m);
        for p in sig.params.iter_mut() {
            *p = rewrite_type_params(p, &c.type_params);
        }
        sig.ret = rewrite_type_params(&sig.ret, &c.type_params);
        let entry = methods.entry(m.name.clone()).or_default();
        // `deinit` can't be overloaded — it's always called by the
        // runtime with no args. Reject any second decl.
        if m.name == "deinit" && !entry.is_empty() {
            return Err(TypeError::Unsupported {
                what: format!("class {:?} declares `deinit` more than once", c.name),
                span: m.span,
            });
        }
        // Generic + non-generic same name: forbidden (same rule as
        // top-level fns).
        let any_generic = !sig.type_params.is_empty()
            || entry.iter().any(|s| !s.type_params.is_empty());
        if any_generic && !entry.is_empty() {
            return Err(TypeError::Unsupported {
                what: format!(
                    "method {:?} in class {:?} mixes a generic declaration with another \
                     overload — generic methods cannot share a name with other methods",
                    m.name, c.name
                ),
                span: m.span,
            });
        }
        if entry.iter().any(|s| s.params == sig.params) {
            return Err(TypeError::Unsupported {
                what: format!(
                    "method {:?} in class {:?} has a duplicate overload (same parameter \
                     types as a previous declaration)",
                    m.name, c.name
                ),
                span: m.span,
            });
        }
        // Generic class + method overload: forbidden. Mono and overload
        // resolution paths are kept separate to avoid having to score
        // overloads after type-param substitution per instantiation.
        if !c.type_params.is_empty() && !entry.is_empty() {
            return Err(TypeError::Unsupported {
                what: format!(
                    "method {:?} in generic class {:?} cannot be overloaded \
                     (generic classes do not support method overloading)",
                    m.name, c.name
                ),
                span: m.span,
            });
        }
        entry.push(sig);
        // Slot for the first sig of each method name. Overloaded
        // methods reuse the same slot — but they can't be overridden
        // anyway (forbidden in inheriting classes), so the slot is
        // effectively unused for them. `init` / `deinit` skip slots.
        if m.name != "init"
            && m.name != "deinit"
            && !method_slots.contains_key(&m.name)
        {
            method_slots.insert(m.name.clone(), vtable_len);
            vtable_len += 1;
        }
    }
    let mut properties: HashMap<Symbol, PropertySig> = HashMap::new();
    for prop in &c.properties {
        // Reject name collisions with fields and methods.
        if fields.contains_key(&prop.name) {
            return Err(TypeError::Unsupported {
                what: format!(
                    "property {:?} in class {:?} collides with a field of the same name",
                    prop.name, c.name
                ),
                span: prop.span,
            });
        }
        if methods.contains_key(&prop.name) {
            return Err(TypeError::Unsupported {
                what: format!(
                    "property {:?} in class {:?} collides with a method of the same name",
                    prop.name, c.name
                ),
                span: prop.span,
            });
        }
        let prop_ty = rewrite_type_params(&prop.ty, &c.type_params);
        // Validate getter / setter signatures match the property type.
        if let Some(g) = &prop.getter {
            let ret = g
                .ret
                .as_ref()
                .map(|t| rewrite_type_params(t, &c.type_params))
                .unwrap_or(Type::Unit);
            if ret != prop_ty {
                return Err(TypeError::Mismatch {
                    expected: prop_ty.clone(),
                    got: ret,
                    span: g.span,
                });
            }
        }
        if let Some(s) = &prop.setter {
            let param = rewrite_type_params(&s.params[0].ty, &c.type_params);
            if param != prop_ty {
                return Err(TypeError::Mismatch {
                    expected: prop_ty.clone(),
                    got: param,
                    span: s.span,
                });
            }
        }
        properties.insert(
            prop.name.clone(),
            PropertySig {
                ty: prop_ty,
                has_get: prop.getter.is_some(),
                has_set: prop.setter.is_some(),
            },
        );
    }
    let mut static_methods: HashMap<Symbol, Signature> = HashMap::new();
    if !c.type_params.is_empty()
        && (!c.static_methods.is_empty() || !c.static_fields.is_empty())
    {
        return Err(TypeError::Unsupported {
            what: format!(
                "class {:?}: static members on generic classes are not supported",
                c.name
            ),
            span: c.span,
        });
    }
    for m in &c.static_methods {
        if static_methods.contains_key(&m.name) {
            return Err(TypeError::Unsupported {
                what: format!(
                    "static method {:?} in class {:?} is declared more than once \
                     (static methods cannot be overloaded)",
                    m.name, c.name
                ),
                span: m.span,
            });
        }
        // No name collisions with instance fields / methods / properties.
        if fields.contains_key(&m.name)
            || methods.contains_key(&m.name)
            || properties.contains_key(&m.name)
        {
            return Err(TypeError::Unsupported {
                what: format!(
                    "static method {:?} in class {:?} collides with an instance \
                     field / method / property of the same name",
                    m.name, c.name
                ),
                span: m.span,
            });
        }
        let mut sig = signature_of(m);
        for p in sig.params.iter_mut() {
            *p = rewrite_type_params(p, &c.type_params);
        }
        sig.ret = rewrite_type_params(&sig.ret, &c.type_params);
        static_methods.insert(m.name.clone(), sig);
    }
    let mut static_fields: HashMap<Symbol, Type> = HashMap::new();
    let mut static_const_fields: HashSet<Symbol> = HashSet::new();
    for sf in &c.static_fields {
        if static_fields.contains_key(&sf.name)
            || fields.contains_key(&sf.name)
            || methods.contains_key(&sf.name)
            || properties.contains_key(&sf.name)
            || static_methods.contains_key(&sf.name)
        {
            return Err(TypeError::Unsupported {
                what: format!(
                    "static field {:?} in class {:?} collides with a field / \
                     method / property / static method of the same name",
                    sf.name, c.name
                ),
                span: sf.span,
            });
        }
        // Allowed static-field types: numeric primitives (any
        // width) + bool, and dynamic arrays of those primitives
        // (the ARC retain/release on the slot uses the same
        // helpers as instance fields). Heap types beyond arrays
        // (objects, strings, optionals, …) still need a slot-init
        // phase; reject those for now with a clearer message.
        let prim_ok = matches!(
            sf.ty,
            Type::I8 | Type::I16 | Type::I32 | Type::I64
            | Type::U8 | Type::U16 | Type::U32 | Type::U64
            | Type::F32 | Type::F64 | Type::Bool
        );
        let array_of_prim_ok = matches!(
            &sf.ty,
            Type::Array { elem, fixed: None } if matches!(
                elem.as_ref(),
                Type::I8 | Type::I16 | Type::I32 | Type::I64
                | Type::U8 | Type::U16 | Type::U32 | Type::U64
                | Type::F32 | Type::F64 | Type::Bool
            )
        );
        if !prim_ok && !array_of_prim_ok {
            return Err(TypeError::Unsupported {
                what: format!(
                    "static field {:?} in class {:?}: type {} not yet \
                     supported (allowed: numeric primitives, bool, or \
                     dynamic arrays of those)",
                    sf.name, c.name, sf.ty
                ),
                span: sf.span,
            });
        }
        static_fields.insert(sf.name.clone(), sf.ty.clone());
        if sf.is_const {
            static_const_fields.insert(sf.name.clone());
        }
    }
    Ok(ClassSig {
        type_params: Vec::from(c.type_params.clone()),
        fields,
        methods,
        properties,
        static_methods,
        static_fields,
        static_const_fields,
        parent: c.parent.clone(),
        method_slots,
        vtable_len,
        extern_lib: c.extern_lib.clone(),
        is_repr_c: c.is_repr_c,
        has_fam: c.is_repr_c
            && c.fields.last().map_or(false, |f| matches!(
                &f.ty, Type::Array { fixed: None, .. }
            )),
    })
}

/// The parser produces `Type::Object(name)` for any user-defined type
/// reference. Inside a generic class body, references that match the
/// class's type-parameter names are actually type variables — convert
/// them to `Type::TypeVar` so the checker can substitute later.
fn rewrite_type_params(t: &Type, params: &[Symbol]) -> Type {
    match t {
        Type::Object(name) if params.iter().any(|p| p == name) => {
            Type::TypeVar(name.clone())
        }
        Type::Array { elem, fixed } => Type::Array {
            elem: Box::new(rewrite_type_params(elem, params)),
            fixed: *fixed,
        },
        Type::Optional(inner) => {
            Type::Optional(Box::new(rewrite_type_params(inner, params)))
        }
        Type::Weak(inner) => Type::Weak(Box::new(rewrite_type_params(inner, params))),
        Type::Generic(g) => Type::generic(
            g.base.clone(),
            g.args.iter().map(|a| rewrite_type_params(a, params)).collect(),
        ),
        Type::Tuple(elems) => Type::Tuple(
            elems.iter().map(|e| rewrite_type_params(e, params)).collect(),
        ),
        Type::Fn(ft) => Type::func(
            ft.params.iter().map(|p| rewrite_type_params(p, params)).collect(),
            rewrite_type_params(&ft.ret, params),
        ),
        Type::RawPtr { is_const, inner } => Type::RawPtr {
            is_const: *is_const,
            inner: Box::new(rewrite_type_params(inner, params)),
        },
        _ => t.clone(),
    }
}

/// Substitute concrete types for type variables. Used when a generic
/// class is instantiated: each `TypeVar(P)` is replaced with the i-th
/// concrete arg from the matching position in `params`.
fn subst_type(t: &Type, params: &[Symbol], args: &[Type]) -> Type {
    match t {
        Type::TypeVar(name) => params
            .iter()
            .position(|p| p == name)
            .and_then(|i| args.get(i).cloned())
            .unwrap_or_else(|| t.clone()),
        Type::Array { elem, fixed } => Type::Array {
            elem: Box::new(subst_type(elem, params, args)),
            fixed: *fixed,
        },
        Type::Optional(inner) => Type::Optional(Box::new(subst_type(inner, params, args))),
        Type::Weak(inner) => Type::Weak(Box::new(subst_type(inner, params, args))),
        Type::Generic(g) => Type::generic(
            g.base.clone(),
            g.args.iter().map(|a| subst_type(a, params, args)).collect(),
        ),
        Type::RawPtr { is_const, inner } => Type::RawPtr {
            is_const: *is_const,
            inner: Box::new(subst_type(inner, params, args)),
        },
        Type::Tuple(elems) => Type::Tuple(
            elems.iter().map(|e| subst_type(e, params, args)).collect(),
        ),
        Type::Fn(ft) => Type::func(
            ft.params.iter().map(|p| subst_type(p, params, args)).collect(),
            subst_type(&ft.ret, params, args),
        ),
        _ => t.clone(),
    }
}

/// Pick the unifying type between two branches of an `if`/`match`. If
/// either side is assignable to the other, the wider one wins; otherwise
/// surface a type-mismatch.
fn unify_branch(a: Type, b: Type, span: Span) -> Result<Type, TypeError> {
    if a == b {
        return Ok(a);
    }
    // Generic with `Any` placeholders (from enum-ctor inference) — try
    // to merge the two sides into the more specific type.
    if let Some(merged) = merge_generic_with_holes(&a, &b) {
        return Ok(merged);
    }
    if assignable(&a, &b) {
        return Ok(b);
    }
    if assignable(&b, &a) {
        return Ok(a);
    }
    Err(TypeError::Mismatch {
        expected: a,
        got: b,
        span,
    })
}

/// When two arms each produced a `Type::Generic` with the same base
/// but different concrete args (commonly with `Any` placeholders left
/// over from constructor-type inference, e.g. `Result<i64, Any>` on
/// one side and `Result<Any, string>` on the other), merge them by
/// taking the non-`Any` side at each position. Returns `None` if the
/// bases differ, the arities differ, or any position has two
/// incompatible non-`Any` types.
fn merge_generic_with_holes(a: &Type, b: &Type) -> Option<Type> {
    let (Type::Generic(ga), Type::Generic(gb)) = (a, b) else {
        return None;
    };
    if ga.base != gb.base || ga.args.len() != gb.args.len() {
        return None;
    }
    let mut merged = Vec::with_capacity(ga.args.len());
    for (x, y) in ga.args.iter().zip(gb.args.iter()) {
        if x == y {
            merged.push(x.clone());
        } else if matches!(x, Type::Any) {
            merged.push(y.clone());
        } else if matches!(y, Type::Any) {
            merged.push(x.clone());
        } else if let Some(inner) = merge_generic_with_holes(x, y) {
            merged.push(inner);
        } else {
            return None;
        }
    }
    Some(Type::generic(ga.base.clone(), merged))
}

fn enum_signature(e: &EnumDecl) -> EnumSig {
    let params = &e.type_params;
    let variants = e
        .variants
        .iter()
        .map(|v| EnumVariantSig {
            name: v.name.clone(),
            payload: match &v.payload {
                VariantPayload::Unit => VariantPayloadSig::Unit,
                VariantPayload::Tuple(tys) => VariantPayloadSig::Tuple(
                    tys.iter().map(|t| rewrite_type_params(t, params)).collect(),
                ),
                VariantPayload::Struct(fs) => VariantPayloadSig::Struct(
                    fs.iter()
                        .map(|f| (f.name.clone(), rewrite_type_params(&f.ty, params)))
                        .collect(),
                ),
            },
        })
        .collect();
    EnumSig {
        type_params: Vec::from(e.type_params.clone()),
        variants,
        flags: e.flags,
    }
}

fn is_result_type(t: &Type) -> bool {
    // Matches both the pre-monomorphization names (`Result` /
    // `Result<T, E>`) and the post-monomorphization mangled object
    // names like `Result<i64, string>` that the JIT emits.
    let name = match t {
        Type::Object(name) => *name,
        Type::Generic(g) => g.base,
        _ => return false,
    };
    let s = name.as_str();
    s == "Result" || s.starts_with("Result<")
}

fn expect_object(t: &Type, span: Span) -> Result<Symbol, TypeError> {
    match t {
        Type::Object(name) => Ok(*name),
        Type::Generic(g) => Ok(g.base),
        _ => Err(TypeError::Mismatch {
            expected: Type::Object("<object>".into()),
            got: t.clone(),
            span,
        }),
    }
}

/// Extract the concrete type arguments from an object-typed value, if
/// any. Non-generic objects return an empty slice.
fn type_args_of(t: &Type) -> &[Type] {
    if let Type::Generic(g) = t {
        &g.args
    } else {
        &[]
    }
}

/// Helper for `bin_result`-style spanless errors (the ops module returns
/// `BadBinary`/`BadUnary` without knowing the source position; we attach
/// the surrounding expression's span here).
fn attach_span(e: TypeError, span: Span) -> TypeError {
    match e {
        TypeError::BadBinary { lhs, rhs, .. } => TypeError::BadBinary { lhs, rhs, span },
        TypeError::BadUnary { ty, .. } => TypeError::BadUnary { ty, span },
        TypeError::MixedSignedness { lhs, rhs, .. } => {
            TypeError::MixedSignedness { lhs, rhs, span }
        }
        other => other,
    }
}

// ─── Closure capture analysis (used by JIT closure lowering) ──────────

fn collect_fn_expr_free_vars(
    b: &ilang_ast::Block,
    bound: &mut std::collections::HashSet<Symbol>,
    frees: &mut Vec<Symbol>,
    seen: &mut std::collections::HashSet<Symbol>,
) {
    let snapshot = bound.clone();
    for s in &b.stmts {
        match &s.kind {
            ilang_ast::StmtKind::Let { name, value, .. } => {
                cfev_expr(value, bound, frees, seen);
                bound.insert(name.clone());
            }
            ilang_ast::StmtKind::Expr(e) => cfev_expr(e, bound, frees, seen),
        }
    }
    if let Some(t) = &b.tail {
        cfev_expr(t, bound, frees, seen);
    }
    *bound = snapshot;
}

fn cfev_expr(
    e: &ilang_ast::Expr,
    bound: &mut std::collections::HashSet<Symbol>,
    frees: &mut Vec<Symbol>,
    seen: &mut std::collections::HashSet<Symbol>,
) {
    use ilang_ast::ExprKind;
    match &e.kind {
        ExprKind::Var(n) => {
            if !bound.contains(n) && !seen.contains(n) {
                seen.insert(n.clone());
                frees.push(n.clone());
            }
        }
        ExprKind::Int(_) | ExprKind::Float(_) | ExprKind::Bool(_) | ExprKind::Str(_)
        | ExprKind::This | ExprKind::None | ExprKind::Continue => {}
        ExprKind::Break(opt) | ExprKind::Return(opt) => {
            if let Some(x) = opt { cfev_expr(x, bound, frees, seen); }
        }
        ExprKind::Some(inner) => cfev_expr(inner, bound, frees, seen),
        ExprKind::Unary { expr, .. } => cfev_expr(expr, bound, frees, seen),
        ExprKind::Binary { lhs, rhs, .. } | ExprKind::Logical { lhs, rhs, .. } => {
            cfev_expr(lhs, bound, frees, seen);
            cfev_expr(rhs, bound, frees, seen);
        }
        ExprKind::Cast { expr, .. }
        | ExprKind::TypeTest { expr, .. }
        | ExprKind::TypeDowncast { expr, .. } => cfev_expr(expr, bound, frees, seen),
        ExprKind::Call { args, .. }
        | ExprKind::SuperCall { args, .. } => {
            for a in args { cfev_expr(a, bound, frees, seen); }
        }
        ExprKind::Field { obj, .. } => cfev_expr(obj, bound, frees, seen),
        ExprKind::MethodCall { obj, args, .. } => {
            cfev_expr(obj, bound, frees, seen);
            for a in args { cfev_expr(a, bound, frees, seen); }
        }
        ExprKind::New { args, .. } => {
            for a in args { cfev_expr(a, bound, frees, seen); }
        }
        ExprKind::Block(b) => collect_fn_expr_free_vars(b, bound, frees, seen),
        ExprKind::If { cond, then_branch, else_branch } => {
            cfev_expr(cond, bound, frees, seen);
            collect_fn_expr_free_vars(then_branch, bound, frees, seen);
            if let Some(x) = else_branch { cfev_expr(x, bound, frees, seen); }
        }
        ExprKind::IfLet { name, expr, then_branch, else_branch } => {
            cfev_expr(expr, bound, frees, seen);
            let snap = bound.clone();
            bound.insert(name.clone());
            collect_fn_expr_free_vars(then_branch, bound, frees, seen);
            *bound = snap;
            if let Some(x) = else_branch { cfev_expr(x, bound, frees, seen); }
        }
        ExprKind::While { cond, body } => {
            cfev_expr(cond, bound, frees, seen);
            collect_fn_expr_free_vars(body, bound, frees, seen);
        }
        ExprKind::Loop { body } => collect_fn_expr_free_vars(body, bound, frees, seen),
        ExprKind::ForIn { var, iter, body } => {
            cfev_expr(iter, bound, frees, seen);
            let snap = bound.clone();
            bound.insert(var.clone());
            collect_fn_expr_free_vars(body, bound, frees, seen);
            *bound = snap;
        }
        ExprKind::Range { start, end, .. } => {
            cfev_expr(start, bound, frees, seen);
            cfev_expr(end, bound, frees, seen);
        }
        ExprKind::Assign { target, value } => {
            if !bound.contains(target) && !seen.contains(target) {
                seen.insert(target.clone());
                frees.push(target.clone());
            }
            cfev_expr(value, bound, frees, seen);
        }
        ExprKind::AssignField { obj, value, .. } => {
            cfev_expr(obj, bound, frees, seen);
            cfev_expr(value, bound, frees, seen);
        }
        ExprKind::AssignIndex { obj, index, value } => {
            cfev_expr(obj, bound, frees, seen);
            cfev_expr(index, bound, frees, seen);
            cfev_expr(value, bound, frees, seen);
        }
        ExprKind::Array(items) => for i in items { cfev_expr(i, bound, frees, seen); },
        ExprKind::Tuple(items) => for i in items { cfev_expr(i, bound, frees, seen); },
        ExprKind::StructLit { fields, .. } => {
            for (_, e) in fields { cfev_expr(e, bound, frees, seen); }
        }
        ExprKind::MapLit(entries) => for (k, v) in entries {
            cfev_expr(k, bound, frees, seen);
            cfev_expr(v, bound, frees, seen);
        },
        ExprKind::Index { obj, index } => {
            cfev_expr(obj, bound, frees, seen);
            cfev_expr(index, bound, frees, seen);
        }
        ExprKind::EnumCtor { args, .. } => match args {
            ilang_ast::CtorArgs::Unit => {}
            ilang_ast::CtorArgs::Tuple(es) => for e in es { cfev_expr(e, bound, frees, seen); },
            ilang_ast::CtorArgs::Struct(fs) => for (_, e) in fs { cfev_expr(e, bound, frees, seen); },
        },
        ExprKind::Match { scrutinee, arms } => {
            cfev_expr(scrutinee, bound, frees, seen);
            for arm in arms {
                let snap = bound.clone();
                cfev_pattern_binds(&arm.pattern, bound);
                cfev_expr(&arm.body, bound, frees, seen);
                *bound = snap;
            }
        }
        ExprKind::FnExpr { params, body, .. } => {
            // Inner closure: its own params shadow, but its captures
            // become OUR captures (the frees the outer closure must
            // pass through). Recurse with extended bound set.
            let snap = bound.clone();
            for p in params { bound.insert(p.name.clone()); }
            collect_fn_expr_free_vars(body, bound, frees, seen);
            *bound = snap;
        }
        ExprKind::Closure { .. } => {} // hoist hasn't run yet
    }
}

fn cfev_pattern_binds(p: &ilang_ast::Pattern, bound: &mut std::collections::HashSet<Symbol>) {
    use ilang_ast::{PatternBindings, PatternKind};
    if let PatternKind::Variant { bindings, .. } = &p.kind {
        match bindings {
            PatternBindings::Unit => {}
            PatternBindings::Tuple(names) => for n in names {
                if n != "_" { bound.insert(n.clone()); }
            },
            PatternBindings::Struct(fs) => for (_, n) in fs {
                if n != "_" { bound.insert(n.clone()); }
            },
        }
    }
}
