//! Top-level item parsing — `fn` / `class` / `interface` / `enum` /
//! `use` / `const` / `struct` / `union` / `@extern(C) { ... }` /
//! `@extern(ObjC) { ... }`. The dispatch in `parse_item` reads any
//! leading attributes + the optional `pub`, then routes to one of
//! the per-shape parsers living in the sibling sub-modules.

use ilang_ast::{AttrArg, Attribute, Item, Symbol};
use ilang_lexer::TokenKind;

use crate::error::ParseError;
use crate::parser::Parser;

mod attrs;
mod class;
mod const_;
mod enum_;
mod extern_c;
pub(crate) mod extern_objc;
mod fn_;
mod types;
mod use_;

/// Extract the runtime symbol argument from `@intrinsic("symbol")`.
/// Returns `Ok(Some(sym))` when the attribute is present, `Ok(None)`
/// when it's absent, and an error when present but malformed (wrong
/// argument shape, repeated, etc.).
fn extract_intrinsic_arg(attrs: &[Attribute]) -> Result<Option<Symbol>, ParseError> {
    let mut found: Option<Symbol> = None;
    for a in attrs {
        if a.name.as_str() != "intrinsic" {
            continue;
        }
        if found.is_some() {
            return Err(ParseError::Generic {
                msg: "duplicate @intrinsic attribute".into(),
                span: ilang_ast::Span::dummy(),
            });
        }
        match a.args.as_ref() {
            [AttrArg::Str(s)] if !s.is_empty() => {
                found = Some(Symbol::intern(s));
            }
            _ => {
                return Err(ParseError::Generic {
                    msg: "@intrinsic requires exactly one non-empty string argument, e.g. @intrinsic(\"regex.compile\")".into(),
                    span: ilang_ast::Span::dummy(),
                });
            }
        }
    }
    Ok(found)
}

impl<'a> Parser<'a> {
    pub(crate) fn parse_item(&mut self) -> Result<Item, ParseError> {
        let attrs = self.parse_attributes()?;
        // `pub` modifier — accepted before `use`/`fn`/`class`/`enum`/
        // `const`, and after any leading attributes (`@flags pub enum`,
        // `@extern("...") pub fn`, etc.). Without `pub`, the item is
        // module-private and only visible within its declaring file.
        // `pub use M` is the only form where `pub` toggles re-export
        // instead of visibility.
        let is_pub = if matches!(self.peek().kind, TokenKind::Pub) {
            self.bump();
            true
        } else {
            false
        };
        // `pub use M` short-circuits — re-export and we're done.
        if is_pub && matches!(self.peek().kind, TokenKind::Use) {
            if !attrs.is_empty() {
                let t = self.peek();
                return Err(ParseError::Unexpected {
                    found: t.kind.clone(),
                    expected: "no attributes are supported on `pub use`".into(),
                    span: t.span,
                });
            }
            let mut u = self.parse_use_decl()?;
            // Two re-export shapes:
            //   `pub use M`              — namespaced; items live at
            //                              `<umbrella>.M.X`.
            //   `pub use M as _ { * }`   — flattened; items live at
            //                              `<umbrella>.X`.
            // Any other combination of alias / selective on `pub use`
            // is intentionally rejected.
            let is_namespaced = matches!(u.alias, ilang_ast::UseAlias::Default)
                && u.selective.is_none()
                && !u.wildcard;
            let is_flattened = matches!(u.alias, ilang_ast::UseAlias::Discard)
                && u.selective.is_none()
                && u.wildcard;
            if !is_namespaced && !is_flattened {
                return Err(ParseError::Unexpected {
                    found: TokenKind::As,
                    expected:
                        "`pub use M` (namespaced) or `pub use M as _ { * }` (flattened) only"
                            .into(),
                    span: u.span,
                });
            }
            u.re_export = true;
            return Ok(Item::Use(u));
        }
        // Top-level `struct` / `union` (`Ident` tokens, not keywords).
        // They reuse the inside-`@extern(C)` parsing path but get
        // wrapped into a single-item `ExternCBlock` for downstream
        // pipelines, with `restrict_c_types: true` so the validator
        // later rejects C-only field types.
        if let TokenKind::Ident(ref n) = self.peek().kind {
            if n == "struct" || n == "union" {
                let is_struct = n == "struct";
                if !attrs.is_empty() {
                    let t = self.peek();
                    return Err(ParseError::Unexpected {
                        found: t.kind.clone(),
                        expected: format!(
                            "no attributes are supported on top-level `{kw}` (use `@extern(C) {{ {kw} ... }}` if you need `@packed` / C interop)",
                            kw = if is_struct { "struct" } else { "union" }
                        ),
                        span: t.span,
                    });
                }
                let span = self.peek().span;
                let mut item = if is_struct {
                    self.parse_struct_decl(Vec::new(), true)?
                } else {
                    self.parse_union_decl(true)?
                };
                match &mut item {
                    ilang_ast::ExternCItem::Struct { is_pub: p, .. }
                    | ilang_ast::ExternCItem::Union { is_pub: p, .. } => *p = is_pub,
                    _ => unreachable!(),
                }
                return Ok(Item::ExternC(ilang_ast::ExternCBlock {
                    items: Box::new([item]),
                    interfaces: Box::new([]),
                    consts: Box::new([]),
                    span,
                }));
            }
        }
        // `@intrinsic("runtime.symbol") [pub] fn name(...): T` —
        // body-less binding for a runtime-provided implementation. We
        // desugar to an `@extern(C) { fn name(...): T }` block with
        // `c_symbol` set to the attribute's argument so the existing
        // FFI lowering path routes the call to the named symbol.
        if let Some(symbol) = extract_intrinsic_arg(&attrs)? {
            return self.parse_intrinsic_fn(is_pub, symbol, &attrs);
        }
        // `async fn ...` — strip the `async` token, set `is_async`
        // on the parsed FnDecl. The desugar pass picks this up and
        // wraps the body in a `Promise<T>` chain.
        let is_async = if matches!(self.peek().kind, TokenKind::Async) {
            self.bump();
            true
        } else {
            false
        };
        match self.peek().kind {
            TokenKind::Fn => {
                let mut fn_decl = self.parse_fn_decl(attrs)?;
                fn_decl.is_pub = is_pub;
                fn_decl.is_async = is_async;
                Ok(Item::Fn(fn_decl))
            }
            TokenKind::Class => {
                // `@derive(Eq, Hash)` is the one attribute classes accept
                // today (loader's `expand_derives` pass synthesises
                // matching methods). Anything else still surfaces the
                // FFI-pointer hint.
                let non_derive: Vec<&ilang_ast::Attribute> = attrs
                    .iter()
                    .filter(|a| a.name.as_str() != "derive")
                    .collect();
                if !non_derive.is_empty() {
                    let t = self.peek();
                    return Err(ParseError::Unexpected {
                        found: t.kind.clone(),
                        expected: "only `@derive(...)` is supported on classes — for FFI types use `@extern(C) { struct Name { ... } }` instead".into(),
                        span: t.span,
                    });
                }
                let mut c = self.parse_class_decl()?;
                c.is_pub = is_pub;
                c.attrs = attrs.into();
                Ok(Item::Class(c))
            }
            TokenKind::Interface => {
                if !attrs.is_empty() {
                    // `@com interface` must live inside `@extern(C) { ... }`
                    // so its method signatures can reference raw pointers
                    // / C-only types under the same scope rule as fn /
                    // struct decls. Other attributes aren't recognised
                    // on top-level interfaces.
                    let t = self.peek();
                    return Err(ParseError::Unexpected {
                        found: t.kind.clone(),
                        expected:
                            "no attributes are supported on top-level interfaces — \
                             wrap `@com interface` inside an `@extern(C) { ... }` block"
                                .into(),
                        span: t.span,
                    });
                }
                let mut i = self.parse_interface_decl(false)?;
                i.is_pub = is_pub;
                Ok(Item::Interface(i))
            }
            TokenKind::Enum => {
                let mut flags = false;
                for a in &attrs {
                    match a.name.as_str() {
                        "flags" if a.args.is_empty() => {
                            flags = true;
                        }
                        _ => {
                            let t = self.peek();
                            return Err(ParseError::Unexpected {
                                found: t.kind.clone(),
                                expected: "'fn' (only @flags is supported on enums)".into(),
                                span: t.span,
                            });
                        }
                    }
                }
                let mut e = self.parse_enum_decl()?;
                e.flags = flags;
                e.is_pub = is_pub;
                // `@flags` defaults to `u64` repr when no explicit
                // `: <type>` is given — matches the language's default
                // integer literal type.
                if e.flags && e.repr_ty.is_none() {
                    e.repr_ty = Some(ilang_ast::Type::U64);
                }
                Ok(Item::Enum(e))
            }
            TokenKind::Use => {
                // Plain `use module`. The re-export form (`pub use ...`)
                // is handled above before this match.
                if is_pub {
                    let t = self.peek();
                    return Err(ParseError::Unexpected {
                        found: t.kind.clone(),
                        expected: "`pub use` is the re-export form (handled above) — bare `use` cannot be `pub`".into(),
                        span: t.span,
                    });
                }
                if !attrs.is_empty() {
                    let t = self.peek();
                    return Err(ParseError::Unexpected {
                        found: t.kind.clone(),
                        expected: "no attributes are supported on `use` (use `pub use` to re-export)".into(),
                        span: t.span,
                    });
                }
                let u = self.parse_use_decl()?;
                Ok(Item::Use(u))
            }
            TokenKind::Const => {
                // The only attribute supported on `const` is
                // `@embed("path")`, which initialises the constant
                // from a file at compile time (see `parse_const_decl`
                // for the per-form rules).
                let mut embed_path: Option<ilang_ast::Symbol> = None;
                for a in &attrs {
                    match a.name.as_str() {
                        "embed" => {
                            let bad = ParseError::Unexpected {
                                found: TokenKind::At,
                                expected: "@embed(\"path/to/file\") — exactly one string argument".into(),
                                span: self.peek().span,
                            };
                            if a.args.len() != 1 {
                                return Err(bad);
                            }
                            match &a.args[0] {
                                ilang_ast::AttrArg::Str(s) => {
                                    embed_path = Some(s.as_str().into());
                                }
                                _ => return Err(bad),
                            }
                        }
                        _ => {
                            let t = self.peek();
                            return Err(ParseError::Unexpected {
                                found: t.kind.clone(),
                                expected: "unknown attribute on `const` (only `@embed(\"path\")` is supported)".into(),
                                span: t.span,
                            });
                        }
                    }
                }
                let mut c = self.parse_const_decl(embed_path)?;
                c.is_pub = is_pub;
                Ok(Item::Const(c))
            }
            TokenKind::LBrace
                if attrs.iter().any(|a| {
                    a.name == "extern"
                        && !a.args.is_empty()
                        && matches!(
                            &a.args[0],
                            ilang_ast::AttrArg::Path(p) if p.iter().map(|s| s.as_str()).collect::<Vec<_>>() == ["C"]
                        )
                }) =>
            {
                // `@extern(C) { ... }` — C ABI block. Optional
                // trailing string args after `C` are default
                // library names that any inner `pub fn` can opt
                // into by writing a bare `@lib` (no args) instead
                // of repeating `@lib("name")` on every decl. Same
                // treatment as `@extern(ObjC, "path", ...)`.
                //
                //   @extern(C, "SDL2") {
                //       @lib pub fn SDL_Init(flags: u32): i32
                //   }
                if is_pub {
                    let t = self.peek();
                    return Err(ParseError::Unexpected {
                        found: TokenKind::Pub,
                        expected: "`pub` on the block as a whole isn't supported — mark individual items inside `@extern(C) { ... }` instead".into(),
                        span: t.span,
                    });
                }
                if attrs.len() != 1 {
                    let t = self.peek();
                    return Err(ParseError::Unexpected {
                        found: t.kind.clone(),
                        expected:
                            "@extern(C) cannot be combined with other attributes on the block"
                                .into(),
                        span: t.span,
                    });
                }
                let mut block_libs: Vec<ilang_ast::Symbol> = Vec::new();
                let attr = &attrs[0];
                for arg in attr.args.iter().skip(1) {
                    match arg {
                        ilang_ast::AttrArg::Str(s) => {
                            block_libs.push(ilang_ast::Symbol::intern(s));
                        }
                        _ => {
                            let t = self.peek();
                            return Err(ParseError::Unexpected {
                                found: t.kind.clone(),
                                expected: "string library names after `C` in @extern(C, \"name\", ...)".into(),
                                span: t.span,
                            });
                        }
                    }
                }
                let block = self.parse_extern_c_block_with_default_libs(&block_libs)?;
                Ok(Item::ExternC(block))
            }
            TokenKind::LBrace
                if attrs.iter().any(|a| {
                    a.name == "extern"
                        && !a.args.is_empty()
                        && matches!(
                            &a.args[0],
                            ilang_ast::AttrArg::Path(p) if p.iter().map(|s| s.as_str()).collect::<Vec<_>>() == ["ObjC"]
                        )
                }) =>
            {
                // `@extern(ObjC) { ... }` — Objective-C dispatch
                // block. The parser desugars each `@objc("selector:")
                // fn` into a typed `objc_msgSend` alias plus a thin
                // wrapper that interns the selector and forwards.
                // The output is an ordinary `ExternCBlock` so the
                // rest of the compiler sees no new construct.
                //
                // Optional trailing string args after `ObjC` are
                // dylib / framework paths to dlopen at JIT init so
                // the @objc classes inside resolve via libobjc's
                // global registry. They also become the default
                // `@lib(...)` for any plain `pub fn` declared in
                // the block (the C fn doesn't need its own @lib).
                //
                //   @extern(ObjC, "/System/.../AppKit.framework/AppKit") {
                //       pub fn NSApplicationLoad(): bool        // dlsym'd from path
                //       @objc pub class NSWindow : NSObject { ... }
                //   }
                if is_pub {
                    let t = self.peek();
                    return Err(ParseError::Unexpected {
                        found: TokenKind::Pub,
                        expected: "`pub` on the block as a whole isn't supported — mark individual items inside `@extern(ObjC) { ... }` instead".into(),
                        span: t.span,
                    });
                }
                if attrs.len() != 1 {
                    let t = self.peek();
                    return Err(ParseError::Unexpected {
                        found: t.kind.clone(),
                        expected:
                            "@extern(ObjC) cannot be combined with other attributes on the block"
                                .into(),
                        span: t.span,
                    });
                }
                // Extract dylib paths from `@extern(ObjC, "p1", "p2", ...)`.
                let mut block_libs: Vec<ilang_ast::Symbol> = Vec::new();
                let attr = &attrs[0];
                for arg in attr.args.iter().skip(1) {
                    match arg {
                        ilang_ast::AttrArg::Str(s) => {
                            block_libs.push(ilang_ast::Symbol::intern(s));
                        }
                        _ => {
                            let t = self.peek();
                            return Err(ParseError::Unexpected {
                                found: t.kind.clone(),
                                expected: "string library paths after `ObjC` in @extern(ObjC, \"path\", ...)".into(),
                                span: t.span,
                            });
                        }
                    }
                }
                let block = self.parse_extern_objc_block(block_libs)?;
                Ok(Item::ExternC(block))
            }
            _ => {
                let t = self.peek();
                Err(ParseError::Unexpected {
                    found: t.kind.clone(),
                    expected: "'fn', 'class', 'enum', or 'use' after attributes".into(),
                    span: t.span,
                })
            }
        }
    }
}
