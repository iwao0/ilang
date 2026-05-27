//! `@extern(ObjC) { ... }` block parsing and desugar orchestrator.
//!
//! Two shapes desugar to plain `@extern(C)` items at parse time:
//!
//!   1. **Top-level @objc fn** — a typed `objc_msgSend` alias plus a
//!      thin wrapper that interns the selector and forwards. The
//!      L1 alias path on `objc_msgSend` makes multiple shapes share
//!      one C symbol.
//!
//!   2. **@objc class** — an ilang class with a single `handle: i64`
//!      field plus an `init(h: i64)` constructor. Each declared
//!      instance / static method becomes an ilang method that
//!      extracts handles from arg classes, calls the corresponding
//!      `objc_msgSend` alias, and wraps the result back into an
//!      ilang class instance when the return type names another
//!      `@objc class` from the same block.
//!
//! The block's source position is woven into every synthesised name
//! so multiple `@extern(ObjC)` blocks in the same file coexist.
//!
//! This module is the orchestrator. The mechanical passes live in
//! siblings — `parse` (the `Parser` impl), `model` (shared types +
//! tiny predicates), `selector` (the per-block selector cache +
//! cstring helpers), `build_class` (class / method / free-fn
//! builders), `super_call` (`super.x()` rewrite + super-helper
//! synthesis), and `imp` (subclass IMP + `register()`).

use std::collections::{HashMap, HashSet};

use ilang_ast::{AttrArg, Attribute, ClassDecl, InterfaceDecl, Symbol, Type};

mod build_class;
mod imp;
mod model;
mod parse;
mod selector;
mod super_call;

use build_class::{build_freefn_dispatch, build_objc_class};
use model::{ObjcClass, ObjcMethod};
use selector::{build_sel_cache_class, SelectorCache};

use model::ObjcCtx;

/// Apply the @extern(ObjC) desugar phase to the parsed contents
/// of a block (or to a synthetic block assembled post-parse).
/// Generates libobjc helper externs, runs `build_objc_class` for
/// each Objective-C class, builds dispatch wrappers for top-level
/// `@objc("…") fn` aliases, attaches the per-block selector
/// cache, and returns the finished `ExternCBlock`. Pulled out of
/// `Parser::parse_extern_objc_block` so the auto-lift pass can
/// reuse the same machinery for top-level classes that implement
/// an `@objc interface`.
pub(in crate::item::extern_objc) fn finalize_objc_block(
    mut items: Vec<ilang_ast::ExternCItem>,
    objc_fns: Vec<ObjcMethod>,
    objc_classes: Vec<ObjcClass>,
    objc_interfaces: Vec<ilang_ast::InterfaceDecl>,
    block_libs: Vec<Symbol>,
    block_span: ilang_ast::Span,
    external_objc_classes: &HashSet<Symbol>,
) -> ilang_ast::ExternCBlock {
    // Process-wide unique sequence number per finalized
    // `@extern(ObjC) { ... }` block so two blocks at the same
    // line/col in different sibling files (e.g.
    // `foundation/io.il` and `foundation/system.il` both opening
    // at line 26 column 77) don't synthesize colliding
    // `_sel_register` / `_get_class` helper names that the loader
    // would later merge into a single module namespace and the
    // type checker would reject as overloaded. `block_span` is
    // still embedded in the tag for human debuggability — the
    // sequence number just disambiguates collisions.
    use std::sync::atomic::{AtomicU64, Ordering};
    static BLOCK_COUNTER: AtomicU64 = AtomicU64::new(0);
    let block_seq = BLOCK_COUNTER.fetch_add(1, Ordering::Relaxed);
    let tag = format!(
        "__objc_b{}c{}n{:x}",
        block_span.line, block_span.col, block_seq
    );
    let sel_struct_name: Symbol = format!("{tag}_sel_t").into();
    let sel_register_name: Symbol = format!("{tag}_sel_register").into();
    let class_struct_name: Symbol = format!("{tag}_class_t").into();
    let get_class_name: Symbol = format!("{tag}_get_class").into();
    let object_struct_name: Symbol = format!("{tag}_object_t").into();
    let any_objc = !objc_fns.is_empty() || !objc_classes.is_empty();
    let any_static = objc_classes
        .iter()
        .any(|c| c.methods.iter().any(|m| m.is_static));
    let any_class = !objc_classes.is_empty();
    // ilang-defined subclasses (parent set) need the ObjC
    // class-registration helpers — objc_allocateClassPair /
    // objc_registerClassPair — plus objc_getClass for the
    // idempotency check inside `register()`.
    // "Real" subclass = has a parent AND adds at least one
    // method with a body. Plain `: Parent` inheritance with
    // no bodies is just an ilang type-system relationship
    // (no ObjC-runtime registration / IMPs needed); skip the
    // libobjc class-helper extern decls and the per-class
    // `register()` static for those.
    let any_subclass = objc_classes
        .iter()
        .any(|c| c.parent.is_some() && c.methods.iter().any(|m| m.body.is_some()));
    let allocate_pair_name: Symbol = format!("{tag}_allocate_class_pair").into();
    let register_pair_name: Symbol = format!("{tag}_register_class_pair").into();
    let class_add_method_name: Symbol = format!("{tag}_class_add_method").into();
    let dlsym_name: Symbol = format!("{tag}_dlsym").into();
    let retain_name: Symbol = format!("{tag}_objc_retain").into();
    let release_name: Symbol = format!("{tag}_objc_release").into();

    if any_objc {
        // Selector type + sel_registerName alias.
        items.insert(
            0,
            ilang_ast::ExternCItem::Struct {
                is_pub: false,
                name: sel_struct_name,
                fields: Box::new([]),
                is_packed: false,
                is_handle: false,
                restrict_c_types: false,
                span: block_span,
            },
        );
        items.insert(
            1,
            ilang_ast::ExternCItem::FnDecl {
                is_pub: false,
                name: sel_register_name,
                type_params: Box::new([]),
                params: Box::new([ilang_ast::Param {
                    name: Symbol::intern("name"),
                    ty: Type::RawPtr {
                        is_const: true,
                        inner: Box::new(Type::CChar),
                    },
                    span: block_span,
                    default: None,
                }]),
                ret: Some(Type::RawPtr {
                    is_const: false,
                    inner: Box::new(Type::Object(sel_struct_name)),
                }),
                libs: Box::new([Symbol::intern("objc")]),
                optional: false,
                c_symbol: Some(Symbol::intern("sel_registerName")),
                intrinsic_name: None,
                variadic: false,
                span: block_span,
            },
        );
        // Opaque ObjC `id` placeholder — used as the receiver
        // type on instance-method aliases and as the value of
        // `arg.handle as *...` casts. Only injected when the
        // block actually declares an @objc class (top-level
        // @objc fns already use the user-named opaque types
        // in their declared signatures).
        if any_class {
            items.insert(
                2,
                ilang_ast::ExternCItem::Struct {
                    is_pub: false,
                    name: object_struct_name,
                    fields: Box::new([]),
                    is_packed: false,
                    is_handle: false,
                    restrict_c_types: false,
                    span: block_span,
                },
            );
        }
        // Retain / release helpers — used by the auto-generated
        // deinit on root @objc classes and by the dispatch
        // wrappers' retain-on-autoreleased-return rule.
        if any_class {
            items.push(ilang_ast::ExternCItem::FnDecl {
                is_pub: false,
                name: retain_name,
                type_params: Box::new([]),
                params: Box::new([ilang_ast::Param {
                    name: Symbol::intern("obj"),
                    ty: Type::RawPtr {
                        is_const: false,
                        inner: Box::new(Type::Object(object_struct_name)),
                    },
                    span: block_span,
                    default: None,
                }]),
                ret: Some(Type::RawPtr {
                    is_const: false,
                    inner: Box::new(Type::Object(object_struct_name)),
                }),
                libs: Box::new([Symbol::intern("objc")]),
                optional: false,
                c_symbol: Some(Symbol::intern("objc_retain")),
                intrinsic_name: None,
                variadic: false,
                span: block_span,
            });
            items.push(ilang_ast::ExternCItem::FnDecl {
                is_pub: false,
                name: release_name,
                type_params: Box::new([]),
                params: Box::new([ilang_ast::Param {
                    name: Symbol::intern("obj"),
                    ty: Type::RawPtr {
                        is_const: false,
                        inner: Box::new(Type::Object(object_struct_name)),
                    },
                    span: block_span,
                    default: None,
                }]),
                ret: None,
                libs: Box::new([Symbol::intern("objc")]),
                optional: false,
                c_symbol: Some(Symbol::intern("objc_release")),
                intrinsic_name: None,
                variadic: false,
                span: block_span,
            });
        }
        // Class lookup helpers are only injected when at least
        // one class uses a static method (only static dispatch
        // needs `objc_getClass`). Subclass registration also
        // requires objc_getClass for the idempotency check
        // (avoid re-registering on second call).
        if any_static || any_subclass {
            items.insert(
                2,
                ilang_ast::ExternCItem::Struct {
                    is_pub: false,
                    name: class_struct_name,
                    fields: Box::new([]),
                    is_packed: false,
                    is_handle: false,
                    restrict_c_types: false,
                    span: block_span,
                },
            );
            items.insert(
                3,
                ilang_ast::ExternCItem::FnDecl {
                    is_pub: false,
                    name: get_class_name,
                    type_params: Box::new([]),
                    params: Box::new([ilang_ast::Param {
                        name: Symbol::intern("name"),
                        ty: Type::RawPtr {
                            is_const: true,
                            inner: Box::new(Type::CChar),
                        },
                        span: block_span,
                        default: None,
                    }]),
                    ret: Some(Type::RawPtr {
                        is_const: false,
                        inner: Box::new(Type::Object(class_struct_name)),
                    }),
                    libs: Box::new([Symbol::intern("objc")]),
                    optional: false,
                    c_symbol: Some(Symbol::intern("objc_getClass")),
                    intrinsic_name: None,
                    variadic: false,
                    span: block_span,
                },
            );
        }
        // libobjc class-registration helpers — only needed
        // when at least one declared @objc class is an
        // ilang-defined subclass (has a parent set).
        if any_subclass {
            items.push(ilang_ast::ExternCItem::FnDecl {
                is_pub: false,
                name: allocate_pair_name,
                type_params: Box::new([]),
                params: Box::new([
                    ilang_ast::Param {
                        name: Symbol::intern("parent"),
                        ty: Type::RawPtr {
                            is_const: false,
                            inner: Box::new(Type::Object(class_struct_name)),
                        },
                        span: block_span,
                        default: None,
                    },
                    ilang_ast::Param {
                        name: Symbol::intern("name"),
                        ty: Type::RawPtr {
                            is_const: true,
                            inner: Box::new(Type::CChar),
                        },
                        span: block_span,
                        default: None,
                    },
                    ilang_ast::Param {
                        name: Symbol::intern("extra_bytes"),
                        ty: Type::Size,
                        span: block_span,
                        default: None,
                    },
                ]),
                ret: Some(Type::RawPtr {
                    is_const: false,
                    inner: Box::new(Type::Object(class_struct_name)),
                }),
                libs: Box::new([Symbol::intern("objc")]),
                optional: false,
                c_symbol: Some(Symbol::intern("objc_allocateClassPair")),
                intrinsic_name: None,
                variadic: false,
                span: block_span,
            });
            items.push(ilang_ast::ExternCItem::FnDecl {
                is_pub: false,
                name: register_pair_name,
                type_params: Box::new([]),
                params: Box::new([ilang_ast::Param {
                    name: Symbol::intern("cls"),
                    ty: Type::RawPtr {
                        is_const: false,
                        inner: Box::new(Type::Object(class_struct_name)),
                    },
                    span: block_span,
                    default: None,
                }]),
                ret: None,
                libs: Box::new([Symbol::intern("objc")]),
                optional: false,
                c_symbol: Some(Symbol::intern("objc_registerClassPair")),
                intrinsic_name: None,
                variadic: false,
                span: block_span,
            });
            // `class_addMethod(cls, sel, imp, type_encoding)`.
            items.push(ilang_ast::ExternCItem::FnDecl {
                is_pub: false,
                name: class_add_method_name,
                type_params: Box::new([]),
                params: Box::new([
                    ilang_ast::Param {
                        name: Symbol::intern("cls"),
                        ty: Type::RawPtr {
                            is_const: false,
                            inner: Box::new(Type::Object(class_struct_name)),
                        },
                        span: block_span,
                        default: None,
                    },
                    ilang_ast::Param {
                        name: Symbol::intern("sel"),
                        ty: Type::RawPtr {
                            is_const: false,
                            inner: Box::new(Type::Object(sel_struct_name)),
                        },
                        span: block_span,
                        default: None,
                    },
                    ilang_ast::Param {
                        name: Symbol::intern("imp"),
                        ty: Type::RawPtr {
                            is_const: false,
                            inner: Box::new(Type::CVoid),
                        },
                        span: block_span,
                        default: None,
                    },
                    ilang_ast::Param {
                        name: Symbol::intern("types"),
                        ty: Type::RawPtr {
                            is_const: true,
                            inner: Box::new(Type::CChar),
                        },
                        span: block_span,
                        default: None,
                    },
                ]),
                ret: Some(Type::I8),
                libs: Box::new([Symbol::intern("objc")]),
                optional: false,
                c_symbol: Some(Symbol::intern("class_addMethod")),
                intrinsic_name: None,
                variadic: false,
                span: block_span,
            });
            // IMP address lookup. AOT links our subclass IMPs
            // with `Linkage::Export` so `dlsym(RTLD_DEFAULT)`
            // would find them; the JIT can't be reached the
            // same way, so we go through an ilang-runtime
            // helper (`__ilang_objc_imp_lookup`) that checks a
            // JIT-populated table first and falls back to
            // dlsym for the AOT path. The first `handle`
            // argument is kept to preserve the dlsym call
            // shape but is ignored by the helper.
            items.push(ilang_ast::ExternCItem::FnDecl {
                is_pub: false,
                name: dlsym_name,
                type_params: Box::new([]),
                params: Box::new([
                    ilang_ast::Param {
                        name: Symbol::intern("handle"),
                        ty: Type::RawPtr {
                            is_const: false,
                            inner: Box::new(Type::CVoid),
                        },
                        span: block_span,
                        default: None,
                    },
                    ilang_ast::Param {
                        name: Symbol::intern("name"),
                        ty: Type::RawPtr {
                            is_const: true,
                            inner: Box::new(Type::CChar),
                        },
                        span: block_span,
                        default: None,
                    },
                ]),
                ret: Some(Type::RawPtr {
                    is_const: false,
                    inner: Box::new(Type::CVoid),
                }),
                libs: Box::new([Symbol::intern("c")]),
                optional: false,
                c_symbol: Some(Symbol::intern("$ilang.objcImpLookup")),
                intrinsic_name: None,
                variadic: false,
                span: block_span,
            });
        }
    }

    // Per-block selector cache. Each unique selector encountered
    // by the wrapper builders gets a `pub static __sel_<n>: i64`
    // slot on the synthesised `<tag>_sel_cache` class; the cache
    // class itself is emitted at the end of the block once
    // every selector has been registered.
    let sel_cache = SelectorCache::new(&tag);

    // Top-level @objc fns — same expansion as before.
    for m in objc_fns {
        let (alias, wrapper) =
            build_freefn_dispatch(&m, &tag, sel_struct_name, sel_register_name, &sel_cache);
        items.push(alias);
        items.push(wrapper);
    }

    // Names of @objc classes the method-body desugar should
    // treat as "wrapped" — those whose arg/return slots need
    // `.handle` extraction and result re-wrapping. Includes:
    //   1. Classes declared in this block.
    //   2. `@objc class` names imported from already-loaded
    //      dependency modules (populated by the loader). This is
    //      what lets `NSWindow.setTitle(t: NSString)` in
    //      `appkit.il` correctly unwrap a `foundation.NSString`
    //      argument — without (2), the desugar would pass the
    //      ilang wrapper pointer to `objc_msgSend` and crash.
    // An @objc interface, when used as a parameter type on an
    // @objc method, needs the same `arg.handle as i64`
    // marshalling as an @objc class: the value at the call site
    // is an ilang wrapper instance (the implementing class),
    // and objc_msgSend wants the raw `id`. Fold interface
    // names into `class_names` so `is_objc_class_ty` matches
    // them too.
    let class_names: HashSet<Symbol> = objc_classes
        .iter()
        .map(|c| c.name)
        .chain(external_objc_classes.iter().copied())
        .chain(objc_interfaces.iter().map(|i| i.name))
        .collect();

    for c in objc_classes {
        let ctx = ObjcCtx {
            tag: &tag,
            sel_struct: sel_struct_name,
            sel_register: sel_register_name,
            sel_cache: &sel_cache,
            class_struct: class_struct_name,
            get_class: get_class_name,
            object_struct: object_struct_name,
            allocate_pair: allocate_pair_name,
            register_pair: register_pair_name,
            class_add_method: class_add_method_name,
            dlsym: dlsym_name,
            retain: retain_name,
            release: release_name,
            class_names: &class_names,
        };
        let (class_item, aliases) = build_objc_class(c, &ctx);
        items.push(class_item);
        items.extend(aliases);
    }

    // Block-level `@extern(ObjC, "path", ...)` library handling.
    // The paths trigger an eager `dlopen` at JIT init so the
    // @objc classes inside resolve via libobjc's class
    // registry. A plain `pub fn` declared in the block must
    // mark itself with bare `@lib` (no args) to opt into
    // dlsym-from-the-block-path; that's handled in
    // `parse_extern_c_fn_with_default_libs` and not here. To
    // keep the dlopen firing when the block declares zero C
    // fns, synthesise a one-off optional loader fn whose only
    // purpose is to carry the `libs` field through to the JIT
    // startup walk.
    if !block_libs.is_empty() {
        let loader_name: Symbol = format!("{tag}_load").into();
        items.push(ilang_ast::ExternCItem::FnDecl {
            is_pub: false,
            name: loader_name,
            type_params: Box::new([]),
            params: Box::new([]),
            ret: None,
            libs: block_libs.into_boxed_slice(),
            optional: true,
            c_symbol: Some(Symbol::intern("$objc.blockLoadPhantom")),
            intrinsic_name: None,
            variadic: false,
            span: block_span,
        });
    }

    // Emit the per-block selector cache class. Skipped when no
    // selector was registered (a block with only C decls and no
    // @objc items).
    if !sel_cache.entries.borrow().is_empty() {
        items.push(build_sel_cache_class(&sel_cache, block_span));
    }

    ilang_ast::ExternCBlock {
        items: items.into(),
        interfaces: objc_interfaces.into(),
        consts: Box::new([]),
        span: block_span,
    }
}

/// Convert a user-written top-level `class C: SomeObjcInterface { … }`
/// declaration into a synthesized `@extern(ObjC)` block whose only
/// content is the equivalent `@objc class` form. Selectors for each
/// method are inherited from the matching `@objc interface` method;
/// methods that don't appear on any implemented @objc interface get
/// an auto-derived selector (`name` + `:` × paramCount). `@objc("…")`
/// on a class method overrides the inherited / derived value.
///
/// Auto-injects bindings for the standard `alloc` / `init` ObjC
/// selectors so the user only writes the method bodies the interface
/// demands. If the user wrote their own `alloc` / `init` (with or
/// without `@objc("…")`) the injection is skipped for that name.
///
/// The Objective-C parent class defaults to `NSObject` unless the
/// declared parent name (the parser-level `cd.parent` slot) is in
/// `objc_class_names`, in which case it's kept verbatim (used when
/// the user wrote e.g. `class MyView: NSView, …`).
pub(crate) fn lift_class_to_objc_block(
    cd: ClassDecl,
    objc_ifaces: &HashMap<Symbol, InterfaceDecl>,
    objc_class_names: &HashSet<Symbol>,
) -> ilang_ast::ExternCBlock {
    let span = cd.span;
    let class_name = cd.name;
    let is_pub = cd.is_pub;

    // Untangle the base list. The parser puts the FIRST base name
    // in `cd.parent` regardless of whether it's a class or
    // interface. Sort each name into one of three buckets so the
    // ObjcClass we build has a real ObjC parent + the right
    // interface list.
    let mut parent_class: Option<Symbol> = None;
    let mut iface_bases: Vec<Symbol> = Vec::new();
    let bases: Vec<Symbol> = cd
        .parent
        .iter()
        .copied()
        .chain(cd.interfaces.iter().copied())
        .collect();
    for b in bases {
        if objc_class_names.contains(&b) {
            // Use the first @objc class as the parent; further @objc
            // classes are unusual (no multiple inheritance) and
            // ignored here — the type checker will complain later
            // through normal channels.
            if parent_class.is_none() {
                parent_class = Some(b);
            }
        } else if objc_ifaces.contains_key(&b) {
            iface_bases.push(b);
        } else {
            // Unknown name in base list — leave it as an interface,
            // the type checker will surface the error.
            iface_bases.push(b);
        }
    }
    // Default Objective-C parent — pick the canonical NSObject
    // from the post-merge registry so type checks find the right
    // class. `build_register_class_fn` strips the module prefix
    // when generating the `objc_getClass(<parent>)` call so the
    // ObjC-runtime side sees the bare class name.
    let parent = parent_class.unwrap_or_else(|| {
        objc_class_names
            .iter()
            .copied()
            .find(|n| {
                let s = n.as_str();
                s == "NSObject" || s.ends_with(".NSObject")
            })
            .unwrap_or_else(|| Symbol::intern("NSObject"))
    });

    // Build a method-name → selector map from every @objc interface
    // we implement. Earlier entries win on collision (first interface
    // in the base list takes precedence).
    let mut selector_by_method: HashMap<Symbol, String> = HashMap::new();
    for iface_name in iface_bases.iter() {
        let Some(iface) = objc_ifaces.get(iface_name) else { continue };
        for m in iface.methods.iter() {
            if let Some(sel) = m.objc_selector {
                selector_by_method
                    .entry(m.name)
                    .or_insert_with(|| sel.as_str().to_string());
            }
        }
    }

    // Convert each user method to an `ObjcMethod`.
    let mut objc_methods: Vec<ObjcMethod> = Vec::new();
    let mut have_alloc = false;
    let mut have_init = false;
    for m in cd.methods.iter() {
        // Instance methods named "init" claim the ObjC init slot.
        if m.name.as_str() == "init" {
            have_init = true;
        }
        let explicit_sel = m
            .attrs
            .iter()
            .find(|a| a.name.as_str() == "objc")
            .and_then(|a| match &a.args[..] {
                [AttrArg::Str(s)] => Some(s.clone()),
                _ => None,
            });
        let selector = explicit_sel
            .or_else(|| selector_by_method.get(&m.name).cloned())
            .unwrap_or_else(|| {
                let mut s = m.name.as_str().to_string();
                for _ in 0..m.params.len() {
                    s.push(':');
                }
                s
            });
        let extra_attrs: Vec<Attribute> = m
            .attrs
            .iter()
            .filter(|a| a.name.as_str() != "objc")
            .cloned()
            .collect();
        let body_is_empty =
            m.body.stmts.is_empty() && m.body.tail.is_none();
        let body = if body_is_empty { None } else { Some(m.body.clone()) };
        objc_methods.push(ObjcMethod {
            name: m.name,
            selector,
            params: m.params.clone(),
            ret: m.ret.clone(),
            body,
            span: m.span,
            is_pub: m.is_pub,
            is_static: false,
            is_override: m.is_override,
            extra_attrs,
            accessor: None,
        });
    }
    // Static methods sit in a separate field on ClassDecl; route
    // them through the same selector-resolution logic.
    for m in cd.static_methods.iter() {
        if m.name.as_str() == "alloc" {
            have_alloc = true;
        }
        let explicit_sel = m
            .attrs
            .iter()
            .find(|a| a.name.as_str() == "objc")
            .and_then(|a| match &a.args[..] {
                [AttrArg::Str(s)] => Some(s.clone()),
                _ => None,
            });
        let selector = explicit_sel.unwrap_or_else(|| {
            let mut s = m.name.as_str().to_string();
            for _ in 0..m.params.len() {
                s.push(':');
            }
            s
        });
        let extra_attrs: Vec<Attribute> = m
            .attrs
            .iter()
            .filter(|a| a.name.as_str() != "objc")
            .cloned()
            .collect();
        let body_is_empty =
            m.body.stmts.is_empty() && m.body.tail.is_none();
        let body = if body_is_empty { None } else { Some(m.body.clone()) };
        objc_methods.push(ObjcMethod {
            name: m.name,
            selector,
            params: m.params.clone(),
            ret: m.ret.clone(),
            body,
            span: m.span,
            is_pub: m.is_pub,
            is_static: true,
            is_override: m.is_override,
            extra_attrs,
            accessor: None,
        });
    }

    // Auto-inject `@objc("alloc") static alloc(): Self` and
    // `@objc("init") init(): Self` when missing. Lets users write
    //   class MyApp: NSAppDel { … method bodies only … }
    // and still have a usable `MyApp.alloc().init()` chain.
    if !have_alloc {
        objc_methods.push(ObjcMethod {
            name: Symbol::intern("alloc"),
            selector: "alloc".into(),
            params: Box::new([]),
            ret: Some(Type::Object(class_name)),
            body: None,
            span,
            is_pub: true,
            is_static: true,
            is_override: false,
            extra_attrs: Vec::new(),
            accessor: None,
        });
    }
    if !have_init {
        objc_methods.push(ObjcMethod {
            name: Symbol::intern("init"),
            selector: "init".into(),
            params: Box::new([]),
            ret: Some(Type::Object(class_name)),
            body: None,
            span,
            is_pub: true,
            is_static: false,
            is_override: false,
            extra_attrs: Vec::new(),
            accessor: None,
        });
    }

    let objc_class = ObjcClass {
        name: class_name,
        is_pub,
        parent: Some(parent),
        interfaces: iface_bases,
        methods: objc_methods,
        span,
    };

    finalize_objc_block(
        Vec::new(),
        Vec::new(),
        vec![objc_class],
        Vec::new(),
        Vec::new(),
        span,
        objc_class_names,
    )
}
