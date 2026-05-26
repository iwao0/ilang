//! Extracted from `checker/mod.rs`.

#![allow(unused_imports)]

use std::collections::{HashMap, HashSet};

use ilang_ast::{
    Block, ClassDecl, CtorArgs, EnumDecl, Expr, ExprKind, FieldDecl, FnDecl, Item, Param,
    PatternBindings, PatternKind, Program, Span, Stmt, StmtKind, Symbol, Type, UnOp,
    VariantPayload,
};

use crate::error::TypeError;
use crate::ops::{assignable, bin_result, int_literal_fits};

use super::*;

impl TypeChecker {
    /// Pre-register the built-in `Console` class and the `console`
    /// singleton so `console.log(x)` type-checks for any `x`. Kept in one
    /// place so it's easy to grow with `console.error`, `console.warn`, etc.
    pub(super) fn install_builtins(&mut self) {
        let mut methods = HashMap::new();
        methods.insert(
            "log".into(),
            vec![Signature {
                // No fixed prefix — variadic with arity 0+. Any
                // arg flows through unchecked.
                params: vec![],
                ret: Type::Unit,
                variadic: true, decl_span: Span::dummy(), type_params: Vec::new(), defaults: Vec::new(), is_pub: true, deprecated: None, lib_names: Vec::new() }],
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
                implements: Vec::new(),
                method_slots: HashMap::new(),
                vtable_len: 0,
                extern_lib: None,
                is_repr_c: false,
                is_handle: false,
                is_union: false,
                has_fam: false,
                field_pub: HashMap::new(),
                static_field_pub: HashMap::new(),
                module: String::new(),
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
            vec![Signature { params: vec![], ret: Type::Unit, variadic: false, decl_span: Span::dummy(), type_params: Vec::new(), defaults: Vec::new(), is_pub: true, deprecated: None, lib_names: Vec::new() }],
        );
        map_methods.insert(
            "get".into(),
            vec![Signature {
                params: vec![k()],
                ret: Type::Optional(Box::new(v())),
                variadic: false, decl_span: Span::dummy(), type_params: Vec::new(), defaults: Vec::new(), is_pub: true, deprecated: None, lib_names: Vec::new() }],
        );
        map_methods.insert(
            "set".into(),
            vec![Signature { params: vec![k(), v()], ret: Type::Unit, variadic: false, decl_span: Span::dummy(), type_params: Vec::new(), defaults: Vec::new(), is_pub: true, deprecated: None, lib_names: Vec::new() }],
        );
        map_methods.insert(
            "has".into(),
            vec![Signature { params: vec![k()], ret: Type::Bool, variadic: false, decl_span: Span::dummy(), type_params: Vec::new(), defaults: Vec::new(), is_pub: true, deprecated: None, lib_names: Vec::new() }],
        );
        map_methods.insert(
            "delete".into(),
            vec![Signature { params: vec![k()], ret: Type::Bool, variadic: false, decl_span: Span::dummy(), type_params: Vec::new(), defaults: Vec::new(), is_pub: true, deprecated: None, lib_names: Vec::new() }],
        );
        map_methods.insert(
            "size".into(),
            vec![Signature { params: vec![], ret: Type::I64, variadic: false, decl_span: Span::dummy(), type_params: Vec::new(), defaults: Vec::new(), is_pub: true, deprecated: None, lib_names: Vec::new() }],
        );
        map_methods.insert(
            "keys".into(),
            vec![Signature {
                params: vec![],
                ret: Type::Array { elem: Box::new(k()), fixed: None },
                variadic: false, decl_span: Span::dummy(), type_params: Vec::new(), defaults: Vec::new(), is_pub: true, deprecated: None, lib_names: Vec::new() }],
        );
        map_methods.insert(
            "values".into(),
            vec![Signature {
                params: vec![],
                ret: Type::Array { elem: Box::new(v()), fixed: None },
                variadic: false, decl_span: Span::dummy(), type_params: Vec::new(), defaults: Vec::new(), is_pub: true, deprecated: None, lib_names: Vec::new() }],
        );
        map_methods.insert(
            "clear".into(),
            vec![Signature {
                params: vec![],
                ret: Type::Unit,
                variadic: false, decl_span: Span::dummy(), type_params: Vec::new(), defaults: Vec::new(), is_pub: true, deprecated: None, lib_names: Vec::new() }],
        );
        // `entries(): (K, V)[]` — list of key/value tuples, in
        // arbitrary order (matches `keys` / `values`).
        map_methods.insert(
            "entries".into(),
            vec![Signature {
                params: vec![],
                ret: Type::Array {
                    elem: Box::new(Type::Tuple(vec![k(), v()].into())),
                    fixed: None,
                },
                variadic: false, decl_span: Span::dummy(), type_params: Vec::new(), defaults: Vec::new(), is_pub: true, deprecated: None, lib_names: Vec::new() }],
        );
        // `forEach(cb: fn(K, V): ())` — invoke `cb` once per entry.
        // Callback args are `(key, value)` to match `entries()`'s
        // tuple order.
        map_methods.insert(
            "forEach".into(),
            vec![Signature {
                params: vec![Type::func(vec![k(), v()], Type::Unit)],
                ret: Type::Unit,
                variadic: false, decl_span: Span::dummy(), type_params: Vec::new(), defaults: Vec::new(), is_pub: true, deprecated: None, lib_names: Vec::new() }],
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
                implements: Vec::new(),
                method_slots: HashMap::new(),
                vtable_len: 0,
                extern_lib: None,
                is_repr_c: false,
                is_handle: false,
                is_union: false,
                has_fam: false,
                field_pub: HashMap::new(),
                static_field_pub: HashMap::new(),
                module: String::new(),
            },
        );

        // Built-in `Set<T>` — generic class with no fields. Element
        // type constraints (string / integer / bool) are checked at
        // `new Set<T>()` use sites the same way Map's key type is.
        let t_set = || Type::TypeVar("T".into());
        let mut set_methods = HashMap::new();
        set_methods.insert(
            "init".into(),
            vec![Signature { params: vec![], ret: Type::Unit, variadic: false, decl_span: Span::dummy(), type_params: Vec::new(), defaults: Vec::new(), is_pub: true, deprecated: None, lib_names: Vec::new() }],
        );
        set_methods.insert(
            "add".into(),
            vec![Signature { params: vec![t_set()], ret: Type::Unit, variadic: false, decl_span: Span::dummy(), type_params: Vec::new(), defaults: Vec::new(), is_pub: true, deprecated: None, lib_names: Vec::new() }],
        );
        set_methods.insert(
            "has".into(),
            vec![Signature { params: vec![t_set()], ret: Type::Bool, variadic: false, decl_span: Span::dummy(), type_params: Vec::new(), defaults: Vec::new(), is_pub: true, deprecated: None, lib_names: Vec::new() }],
        );
        set_methods.insert(
            "delete".into(),
            vec![Signature { params: vec![t_set()], ret: Type::Bool, variadic: false, decl_span: Span::dummy(), type_params: Vec::new(), defaults: Vec::new(), is_pub: true, deprecated: None, lib_names: Vec::new() }],
        );
        set_methods.insert(
            "size".into(),
            vec![Signature { params: vec![], ret: Type::I64, variadic: false, decl_span: Span::dummy(), type_params: Vec::new(), defaults: Vec::new(), is_pub: true, deprecated: None, lib_names: Vec::new() }],
        );
        set_methods.insert(
            "clear".into(),
            vec![Signature { params: vec![], ret: Type::Unit, variadic: false, decl_span: Span::dummy(), type_params: Vec::new(), defaults: Vec::new(), is_pub: true, deprecated: None, lib_names: Vec::new() }],
        );
        set_methods.insert(
            "values".into(),
            vec![Signature {
                params: vec![],
                ret: Type::Array { elem: Box::new(t_set()), fixed: None },
                variadic: false, decl_span: Span::dummy(), type_params: Vec::new(), defaults: Vec::new(), is_pub: true, deprecated: None, lib_names: Vec::new() }],
        );
        set_methods.insert(
            "forEach".into(),
            vec![Signature {
                params: vec![Type::func(vec![t_set()], Type::Unit)],
                ret: Type::Unit,
                variadic: false, decl_span: Span::dummy(), type_params: Vec::new(), defaults: Vec::new(), is_pub: true, deprecated: None, lib_names: Vec::new() }],
        );
        // `set ∪ other` — returns a freshly-allocated set containing
        // every element present in either side. Element type must
        // match.
        set_methods.insert(
            "union".into(),
            vec![Signature {
                params: vec![Type::generic("Set", vec![t_set()])],
                ret: Type::generic("Set", vec![t_set()]),
                variadic: false, decl_span: Span::dummy(), type_params: Vec::new(), defaults: Vec::new(), is_pub: true, deprecated: None, lib_names: Vec::new() }],
        );
        set_methods.insert(
            "intersection".into(),
            vec![Signature {
                params: vec![Type::generic("Set", vec![t_set()])],
                ret: Type::generic("Set", vec![t_set()]),
                variadic: false, decl_span: Span::dummy(), type_params: Vec::new(), defaults: Vec::new(), is_pub: true, deprecated: None, lib_names: Vec::new() }],
        );
        set_methods.insert(
            "difference".into(),
            vec![Signature {
                params: vec![Type::generic("Set", vec![t_set()])],
                ret: Type::generic("Set", vec![t_set()]),
                variadic: false, decl_span: Span::dummy(), type_params: Vec::new(), defaults: Vec::new(), is_pub: true, deprecated: None, lib_names: Vec::new() }],
        );
        set_methods.insert(
            "isSubsetOf".into(),
            vec![Signature {
                params: vec![Type::generic("Set", vec![t_set()])],
                ret: Type::Bool,
                variadic: false, decl_span: Span::dummy(), type_params: Vec::new(), defaults: Vec::new(), is_pub: true, deprecated: None, lib_names: Vec::new() }],
        );
        set_methods.insert(
            "isSupersetOf".into(),
            vec![Signature {
                params: vec![Type::generic("Set", vec![t_set()])],
                ret: Type::Bool,
                variadic: false, decl_span: Span::dummy(), type_params: Vec::new(), defaults: Vec::new(), is_pub: true, deprecated: None, lib_names: Vec::new() }],
        );
        set_methods.insert(
            "isDisjointFrom".into(),
            vec![Signature {
                params: vec![Type::generic("Set", vec![t_set()])],
                ret: Type::Bool,
                variadic: false, decl_span: Span::dummy(), type_params: Vec::new(), defaults: Vec::new(), is_pub: true, deprecated: None, lib_names: Vec::new() }],
        );
        self.classes.insert(
            "Set".into(),
            ClassSig {
                type_params: vec!["T".into()],
                fields: HashMap::new(),
                methods: set_methods,
                properties: HashMap::new(),
                static_methods: HashMap::new(),
                static_fields: HashMap::new(),
                static_const_fields: HashSet::new(),
                parent: None,
                implements: Vec::new(),
                method_slots: HashMap::new(),
                vtable_len: 0,
                extern_lib: None,
                is_repr_c: false,
                is_handle: false,
                is_union: false,
                has_fam: false,
                field_pub: HashMap::new(),
                static_field_pub: HashMap::new(),
                module: String::new(),
            },
        );

        // Built-in `Promise<T>` — generic class with no fields.
        // Methods/static methods are intercepted in MIR lowering;
        // the signatures here are what the type checker enforces.
        //
        //   then<U>(cb: fn(T): U): Promise<U>
        //   catch(cb: fn(string): T): Promise<T>
        //   static resolve(v: T): Promise<T>
        //   static reject(msg: string): Promise<T>
        //
        // The constructor `new Promise<T>(executor: fn(fn(T), fn(string)))`
        // goes through the regular `init` slot.
        let t = || Type::TypeVar("T".into());
        let u = || Type::TypeVar("U".into());
        let promise_t = || Type::generic("Promise", vec![t()]);
        let promise_u = || Type::generic("Promise", vec![u()]);
        let mut promise_methods = HashMap::new();
        // init(executor: fn(fn(T), fn(string)))
        let executor_ty = Type::func(
            vec![
                Type::func(vec![t()], Type::Unit),
                Type::func(vec![Type::Str], Type::Unit),
            ],
            Type::Unit,
        );
        promise_methods.insert(
            "init".into(),
            vec![Signature {
                params: vec![executor_ty],
                ret: Type::Unit,
                variadic: false,
                decl_span: Span::dummy(),
                type_params: Vec::new(),
                defaults: Vec::new(),
                is_pub: true,
            deprecated: None,
                lib_names: Vec::new(),
            }],
        );
        promise_methods.insert(
            "then".into(),
            vec![Signature {
                params: vec![Type::func(vec![t()], u())],
                ret: promise_u(),
                variadic: false,
                decl_span: Span::dummy(),
                type_params: vec!["U".into()],
                defaults: Vec::new(),
                is_pub: true,
            deprecated: None,
                lib_names: Vec::new(),
            }],
        );
        promise_methods.insert(
            "catch".into(),
            vec![Signature {
                params: vec![Type::func(vec![Type::Str], t())],
                ret: promise_t(),
                variadic: false,
                decl_span: Span::dummy(),
                type_params: Vec::new(),
                defaults: Vec::new(),
                is_pub: true,
            deprecated: None,
                lib_names: Vec::new(),
            }],
        );
        let mut promise_statics = HashMap::new();
        promise_statics.insert(
            "resolve".into(),
            vec![Signature {
                params: vec![t()],
                ret: promise_t(),
                variadic: false,
                decl_span: Span::dummy(),
                type_params: vec!["T".into()],
                defaults: Vec::new(),
                is_pub: true,
            deprecated: None,
                lib_names: Vec::new(),
            }],
        );
        // `Promise.reject(msg)` returns `Promise<()>` since there's
        // nothing in the call to constrain T (the rejection has no
        // value, only a message). Common use is
        // `.catch(fn(s) { console.log(s) })` which type-checks
        // because the catch handler also returns `()`. Typed
        // rejections (where catch recovers to a specific T) need
        // the executor form: `new Promise<T>(fn(_, reject) { reject("...") })`.
        // `Promise.all<T>(ps: Promise<T>[]): Promise<T[]>` —
        // resolves with all values, rejects on first rejection.
        promise_statics.insert(
            "all".into(),
            vec![Signature {
                params: vec![Type::Array {
                    elem: Box::new(Type::generic("Promise", vec![t()])),
                    fixed: None,
                }],
                ret: Type::generic(
                    "Promise",
                    vec![Type::Array { elem: Box::new(t()), fixed: None }],
                ),
                variadic: false,
                decl_span: Span::dummy(),
                type_params: vec!["T".into()],
                defaults: Vec::new(),
                is_pub: true,
            deprecated: None,
                lib_names: Vec::new(),
            }],
        );
        // `Promise.race<T>(ps: Promise<T>[]): Promise<T>` —
        // settles with the first promise to settle.
        promise_statics.insert(
            "race".into(),
            vec![Signature {
                params: vec![Type::Array {
                    elem: Box::new(Type::generic("Promise", vec![t()])),
                    fixed: None,
                }],
                ret: promise_t(),
                variadic: false,
                decl_span: Span::dummy(),
                type_params: vec!["T".into()],
                defaults: Vec::new(),
                is_pub: true,
            deprecated: None,
                lib_names: Vec::new(),
            }],
        );
        promise_statics.insert(
            "reject".into(),
            vec![Signature {
                params: vec![Type::Str],
                ret: Type::generic("Promise", vec![Type::Unit]),
                variadic: false,
                decl_span: Span::dummy(),
                type_params: Vec::new(),
                defaults: Vec::new(),
                is_pub: true,
            deprecated: None,
                lib_names: Vec::new(),
            }],
        );
        // Internal: `Promise.__pending<T>(): Promise<T>` — allocates
        // a Pending promise. Used by the async/await state-machine
        // desugar's wrapper fn to create the result promise; not
        // intended for direct user consumption (the double-underscore
        // signals "compiler-internal", same convention as
        // `__mir_alloc` and friends).
        promise_statics.insert(
            "$promise.pending".into(),
            vec![Signature {
                params: vec![],
                ret: promise_t(),
                variadic: false,
                decl_span: Span::dummy(),
                type_params: vec!["T".into()],
                defaults: Vec::new(),
                is_pub: true,
            deprecated: None,
                lib_names: Vec::new(),
            }],
        );
        // Internal: `Promise.__settleResolve<T>(p: Promise<T>, v: T)` —
        // transitions a Pending promise to Resolved. Used by the
        // generated poll fn at the end of an async body.
        promise_statics.insert(
            "$promise.settleResolve".into(),
            vec![Signature {
                params: vec![promise_t(), t()],
                ret: Type::Unit,
                variadic: false,
                decl_span: Span::dummy(),
                type_params: vec!["T".into()],
                defaults: Vec::new(),
                is_pub: true,
            deprecated: None,
                lib_names: Vec::new(),
            }],
        );
        // Internal: `Promise.__settleReject(p: Promise<()>, msg: string)`.
        // The poll fn calls this when the async body wants to reject;
        // because we don't yet have `throw`, this is currently
        // emitted only for the trivial rejection paths inside the
        // desugar — exposed for completeness.
        promise_statics.insert(
            "$promise.settleReject".into(),
            vec![Signature {
                params: vec![
                    Type::generic("Promise", vec![Type::Unit]),
                    Type::Str,
                ],
                ret: Type::Unit,
                variadic: false,
                decl_span: Span::dummy(),
                type_params: Vec::new(),
                defaults: Vec::new(),
                is_pub: true,
            deprecated: None,
                lib_names: Vec::new(),
            }],
        );
        self.classes.insert(
            "Promise".into(),
            ClassSig {
                type_params: vec!["T".into()],
                fields: HashMap::new(),
                methods: promise_methods,
                properties: HashMap::new(),
                static_methods: promise_statics,
                static_fields: HashMap::new(),
                static_const_fields: HashSet::new(),
                parent: None,
                implements: Vec::new(),
                method_slots: HashMap::new(),
                vtable_len: 0,
                extern_lib: None,
                is_repr_c: false,
                is_handle: false,
                is_union: false,
                has_fam: false,
                field_pub: HashMap::new(),
                static_field_pub: HashMap::new(),
                module: String::new(),
            },
        );

        // Built-in `ObjCBlock<F>` — typed handle to an Objective-C
        // block whose `invoke` trampoline calls an ilang closure of
        // shape `F`. `new ObjCBlock(closure)` constructs one (lower
        // pass routes it to the matching `__ilang_make_*_block`
        // runtime helper); the user-visible class has a single
        // `init(body: F)` so the type checker accepts the standard
        // `new`-with-args form. `F` is intentionally an unconstrained
        // type variable here — the lower pass enforces "must be a
        // fn type" because that error reads better with the actual
        // ObjC ABI mismatch in hand.
        let f = || Type::TypeVar("F".into());
        let mut objc_block_methods = HashMap::new();
        objc_block_methods.insert(
            "init".into(),
            vec![Signature {
                params: vec![f()],
                ret: Type::Unit,
                variadic: false,
                decl_span: Span::dummy(),
                type_params: Vec::new(),
                defaults: Vec::new(),
                is_pub: true,
                deprecated: None,
                lib_names: Vec::new(),
            }],
        );
        self.classes.insert(
            "ObjCBlock".into(),
            ClassSig {
                type_params: vec!["F".into()],
                fields: HashMap::new(),
                methods: objc_block_methods,
                properties: HashMap::new(),
                static_methods: HashMap::new(),
                static_fields: HashMap::new(),
                static_const_fields: HashSet::new(),
                parent: None,
                implements: Vec::new(),
                method_slots: HashMap::new(),
                vtable_len: 0,
                extern_lib: None,
                is_repr_c: false,
                is_handle: false,
                is_union: false,
                has_fam: false,
                field_pub: HashMap::new(),
                static_field_pub: HashMap::new(),
                module: String::new(),
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
            is_pub: true,
            deprecated: None,
                lib_names: Vec::new(),
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
                repr: None,
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
                repr: None,
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
                implements: Vec::new(),
                method_slots: HashMap::new(),
                vtable_len: 0,
                extern_lib: None,
                is_repr_c: false,
                is_handle: false,
                is_union: false,
                has_fam: false,
                field_pub: HashMap::new(),
                static_field_pub: HashMap::new(),
                module: String::new(),
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
                defaults: Vec::new(), is_pub: true, deprecated: None, lib_names: Vec::new() }],
        );
    }

}
