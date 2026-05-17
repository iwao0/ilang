use ilang_ast::{
    AttrArg, Attribute, ClassDecl, FieldDecl, FnDecl, Item, PropertyDecl, StaticFieldDecl, Symbol,
};
use ilang_lexer::TokenKind;

use crate::error::ParseError;
use crate::parser::Parser;
use crate::stmt::parse_block;

mod enum_;
mod extern_c;
mod extern_objc;
mod types;

/// True if `e` is a value-only literal — what `const` accepts as its
/// RHS. Numeric / bool / string literals, optionally with a unary
/// `-` on numerics. No identifiers, no calls, no expressions.

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
                    span,
                }));
            }
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
                if !attrs.is_empty() {
                    let t = self.peek();
                    return Err(ParseError::Unexpected {
                        found: t.kind.clone(),
                        expected: "no attributes are supported on classes — for FFI types use `@extern(C) { struct Name { ... } }` instead".into(),
                        span: t.span,
                    });
                }
                let mut c = self.parse_class_decl()?;
                c.is_pub = is_pub;
                Ok(Item::Class(c))
            }
            TokenKind::Interface => {
                if !attrs.is_empty() {
                    let t = self.peek();
                    return Err(ParseError::Unexpected {
                        found: t.kind.clone(),
                        expected: "no attributes are supported on interfaces".into(),
                        span: t.span,
                    });
                }
                let mut i = self.parse_interface_decl()?;
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
                        && a.args.len() == 1
                        && matches!(
                            &a.args[0],
                            ilang_ast::AttrArg::Path(p) if p.iter().map(|s| s.as_str()).collect::<Vec<_>>() == ["C"]
                        )
                }) =>
            {
                // `@extern(C) { ... }` — C ABI block. Validate that
                // no other attributes were stacked, then parse the
                // block body.
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
                let block = self.parse_extern_c_block()?;
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

    /// `const NAME [: T] = literal` — top-level immutable binding.
    /// Restricted to literal RHS (numeric / bool / string, with
    /// optional unary minus on numerics). Anything more elaborate is
    /// rejected so the substitution pass stays trivial.
    ///
    /// When `embed_path` is `Some`, the const is being initialised
    /// from a file via `@embed("path")` — `=` must be absent and the
    /// type annotation must be present (we can't infer `string` vs
    /// `u8[]` from the file alone). A placeholder `value` is stored
    /// so downstream passes have a well-typed `ConstDecl`; the
    /// loader replaces it once the file is read.
    fn parse_const_decl(
        &mut self,
        embed_path: Option<ilang_ast::Symbol>,
    ) -> Result<ilang_ast::ConstDecl, ParseError> {
        let span = self.peek().span;
        self.expect(&TokenKind::Const, "'const'")?;
        let name = self.expect_ident("constant name")?;
        let ty = if matches!(self.peek().kind, TokenKind::Colon) {
            self.bump();
            Some(self.parse_type()?)
        } else {
            None
        };
        if embed_path.is_some() {
            // `@embed("...")` initialises the const from a file; the
            // user must not also write `= <expr>`. The type annotation
            // is mandatory so the loader knows whether to produce a
            // `string` (UTF-8 text) or a `u8[]` (raw bytes).
            if matches!(self.peek().kind, TokenKind::Equals) {
                let t = self.peek();
                return Err(ParseError::Unexpected {
                    found: t.kind.clone(),
                    expected: "no `= ...` on a `@embed(\"...\") const` — the value comes from the file".into(),
                    span: t.span,
                });
            }
            if ty.is_none() {
                return Err(ParseError::Unexpected {
                    found: TokenKind::Const,
                    expected: "type annotation required on `@embed(\"...\") const` (use `: string` or `: u8[]`)".into(),
                    span,
                });
            }
            // Placeholder value — replaced by the loader after the
            // file is read. Pick a literal shape that matches the
            // declared type so any pre-loader walker stays happy.
            let placeholder_kind = match &ty {
                Some(ilang_ast::Type::Str) => ilang_ast::ExprKind::Str(String::new()),
                _ => ilang_ast::ExprKind::Array(Box::new([])),
            };
            let value = ilang_ast::Expr {
                kind: placeholder_kind,
                span,
            };
            return Ok(ilang_ast::ConstDecl {
                is_pub: false,
                name,
                ty,
                value,
                embed_path,
                span,
            });
        }
        if !matches!(self.peek().kind, TokenKind::Equals) {
            let t = self.peek();
            return Err(ParseError::Unexpected {
                found: t.kind.clone(),
                expected: "`=` — `const` requires an initializer expression".into(),
                span: t.span,
            });
        }
        self.bump();
        let value = self.parse_expr(0)?;
        // The parser accepts any expression here; the loader's
        // `inline_constants` pass folds it to a literal (or errors).
        Ok(ilang_ast::ConstDecl {
            is_pub: false,
            name,
            ty,
            value,
            embed_path: None,
            span,
        })
    }

    /// `use module` (whole-module import) or
    /// `use module { name1, name2, ... }` (selective).
    fn parse_use_decl(&mut self) -> Result<ilang_ast::UseDecl, ParseError> {
        let span = self.peek().span;
        self.expect(&TokenKind::Use, "'use'")?;
        let module = self.expect_ident("module name")?;
        // `use M.*` / `use M.Name` — short forms for
        // `use M as _ { * }` / `use M as _ { Name }`. Both produce
        // the Discard-alias UseDecl shape the loader already handles
        // (wildcard branch for `.*`, selective branch for `.Name`),
        // so callers can stack one-liners — `use cocoa.NSObject` on
        // one line and `use cocoa.NSString` on another get merged the
        // same way the long-form selective imports do.
        if matches!(self.peek().kind, TokenKind::Dot) {
            self.bump();
            let next = self.peek().clone();
            match next.kind {
                TokenKind::Star => {
                    self.bump();
                    return Ok(ilang_ast::UseDecl {
                        module,
                        alias: ilang_ast::UseAlias::Discard,
                        selective: None,
                        wildcard: true,
                        re_export: false,
                        span,
                    });
                }
                TokenKind::Ident(name) => {
                    self.bump();
                    let sym = ilang_ast::Symbol::intern(&name);
                    return Ok(ilang_ast::UseDecl {
                        module,
                        alias: ilang_ast::UseAlias::Discard,
                        selective: Some(Box::new([sym])),
                        wildcard: false,
                        re_export: false,
                        span,
                    });
                }
                _ => {
                    return Err(ParseError::Unexpected {
                        found: next.kind,
                        expected: "`*` or an identifier after `.` in `use M.<name>`".into(),
                        span: next.span,
                    });
                }
            }
        }
        // Optional `as <ident>` / `as _` alias.
        let alias = if matches!(self.peek().kind, TokenKind::As) {
            self.bump();
            let t = self.peek().clone();
            match t.kind {
                TokenKind::Ident(name) => {
                    self.bump();
                    if name == "_" {
                        ilang_ast::UseAlias::Discard
                    } else {
                        ilang_ast::UseAlias::Named(ilang_ast::Symbol::intern(&name))
                    }
                }
                _ => {
                    return Err(ParseError::Unexpected {
                        found: t.kind,
                        expected: "alias name or `_`".into(),
                        span: t.span,
                    });
                }
            }
        } else {
            ilang_ast::UseAlias::Default
        };
        let mut wildcard = false;
        let selective = if matches!(self.peek().kind, TokenKind::LBrace) {
            self.bump();
            let mut names = Vec::new();
            if !matches!(self.peek().kind, TokenKind::RBrace) {
                // `{ * }` wildcard — only legal on `pub use M as _ { * }`
                // (re-export with flatten). Validated below.
                if matches!(self.peek().kind, TokenKind::Star) {
                    self.bump();
                    wildcard = true;
                } else {
                    // Comma- or newline-separated list. A trailing
                    // separator is allowed; a newline before the next
                    // ident also counts so multi-line bodies don't
                    // require explicit commas.
                    loop {
                        names.push(self.expect_ident("imported name")?);
                        if matches!(self.peek().kind, TokenKind::Comma) {
                            self.bump();
                            // Trailing comma before `}` ends the list.
                            if matches!(self.peek().kind, TokenKind::RBrace) {
                                break;
                            }
                            continue;
                        }
                        // Implicit newline separator: keep looping
                        // while the next token is an identifier on a
                        // new line. Anything else (RBrace, or a
                        // same-line non-ident token) ends the list
                        // and falls through to the closing `}` check.
                        let next = self.peek();
                        if next.leading_newline
                            && matches!(next.kind, TokenKind::Ident(_))
                        {
                            continue;
                        }
                        break;
                    }
                }
            }
            self.expect(&TokenKind::RBrace, "'}'")?;
            if wildcard { None } else { Some(names) }
        } else {
            None
        };
        // `use M as _` without a selective list has no observable
        // effect (the namespace is suppressed and no bare names are
        // imported). Reject it so the user catches the typo at parse
        // time.
        if matches!(alias, ilang_ast::UseAlias::Discard) && selective.is_none() && !wildcard {
            return Err(ParseError::Unexpected {
                found: TokenKind::Use,
                expected: "`use M as _` requires a `{ ... }` selective list".into(),
                span,
            });
        }
        Ok(ilang_ast::UseDecl {
            module,
            alias,
            selective: selective.map(Into::into),
            wildcard,
            re_export: false,
            span,
        })
    }

    fn parse_class_decl(&mut self) -> Result<ClassDecl, ParseError> {
        let span = self.peek().span;
        self.expect(&TokenKind::Class, "'class'")?;
        let name = self.expect_ident("class name")?;
        // Optional `<T, U>` type parameters. Always unambiguous after a
        // class name in declaration position.
        let type_params = if matches!(self.peek().kind, TokenKind::Lt) {
            self.parse_type_param_list()?
        } else {
            Vec::new()
        };
        // `: Parent, IFace1, IFace2 …`. Bases are comma-separated. We
        // can't tell class from interface at parse time, so the first
        // entry goes into `parent` and the rest into `interfaces`;
        // the type checker reclassifies if `parent` turns out to name
        // an interface (`parent` then becomes None and the name is
        // moved to the head of `interfaces`).
        let (parent, interfaces) = if matches!(self.peek().kind, TokenKind::Colon) {
            self.bump();
            let first = self.parse_dotted_ident("parent class or interface name")?;
            let mut rest: Vec<ilang_ast::Symbol> = Vec::new();
            while matches!(self.peek().kind, TokenKind::Comma) {
                self.bump();
                rest.push(self.parse_dotted_ident("interface name")?);
            }
            (Some(first), rest.into_boxed_slice())
        } else {
            (None, Box::new([]) as Box<[ilang_ast::Symbol]>)
        };
        self.expect(&TokenKind::LBrace, "'{'")?;
        let mut fields = Vec::new();
        let mut methods = Vec::new();
        let mut static_methods = Vec::new();
        let mut static_fields: Vec<StaticFieldDecl> = Vec::new();
        let mut properties: Vec<PropertyDecl> = Vec::new();
        loop {
            // Optional `pub` modifier on the next member. Without it
            // the member is private to the declaring module.
            let member_is_pub = if matches!(self.peek().kind, TokenKind::Pub) {
                self.bump();
                true
            } else {
                false
            };
            match self.peek().kind {
                TokenKind::RBrace => {
                    if member_is_pub {
                        let t = self.peek();
                        return Err(ParseError::Unexpected {
                            found: TokenKind::RBrace,
                            expected: "member declaration after `pub`".into(),
                            span: t.span,
                        });
                    }
                    break;
                }
                TokenKind::At => {
                    let attrs = self.parse_attributes()?;
                    // Attributes can apply to either a method or a
                    // field. Look two tokens ahead: `ident :` →
                    // field (with attrs), `ident (` → method.
                    let next_kind = self
                        .tokens
                        .get(self.pos + 1)
                        .map(|t| t.kind.clone());
                    if matches!(next_kind, Some(TokenKind::Colon)) {
                        let mut f = self.parse_field()?;
                        // `@bits(N)` is the only field attr today.
                        for a in &attrs {
                            if a.name == "bits" {
                                let bits = match &*a.args {
                                    [ilang_ast::AttrArg::Int(n)] if *n >= 1 && *n <= 64 => {
                                        *n as u32
                                    }
                                    _ => return Err(ParseError::Unexpected {
                                        found: TokenKind::At,
                                        expected: "@bits(N) with 1 ≤ N ≤ 64".into(),
                                        span: f.span,
                                    }),
                                };
                                f.bits = Some(bits);
                            } else {
                                return Err(ParseError::Unexpected {
                                    found: TokenKind::At,
                                    expected: format!(
                                        "unknown field attribute @{} (only @bits is recognised)",
                                        a.name
                                    ),
                                    span: f.span,
                                });
                            }
                        }
                        f.is_pub = member_is_pub;
                        fields.push(f);
                    } else {
                        let mut m = self.parse_method(attrs)?;
                        m.is_pub = member_is_pub;
                        methods.push(m);
                    }
                }
                TokenKind::Override => {
                    self.bump(); // consume `override`
                    let mut m = self.parse_method(Vec::new())?;
                    m.is_override = true;
                    m.is_pub = member_is_pub;
                    methods.push(m);
                }
                TokenKind::Async => {
                    self.bump(); // consume `async`
                    let mut m = self.parse_method(Vec::new())?;
                    m.is_async = true;
                    m.is_pub = member_is_pub;
                    methods.push(m);
                }
                TokenKind::Const => {
                    // `const name: T = expr` — class-level immutable
                    // static. Reassignment is rejected by the type
                    // checker. Reads are `ClassName.name` (same path
                    // as `static`).
                    self.bump(); // consume `const`
                    let mut f = self.parse_static_field(true)?;
                    f.is_pub = member_is_pub;
                    static_fields.push(f);
                }
                TokenKind::Ident(ref name) => {
                    // Contextual keywords `get` / `set` introduce a
                    // property accessor when followed by `<ident> (`.
                    // Anything else with that name is treated as a
                    // regular field/method declaration.
                    let is_accessor = (name == "get" || name == "set")
                        && matches!(
                            self.tokens
                                .get(self.pos + 1)
                                .map(|t| &t.kind),
                            Some(TokenKind::Ident(_))
                        )
                        && matches!(
                            self.tokens
                                .get(self.pos + 2)
                                .map(|t| &t.kind),
                            Some(TokenKind::LParen)
                        );
                    if is_accessor {
                        self.parse_property_accessor_pub(&mut properties, member_is_pub)?;
                        continue;
                    }
                    // `static <ident> (` → class-level method.
                    // `static <ident> :` → class-level (mutable) field.
                    // `static get <ident> (` / `static set <ident> (` →
                    //   class-level property accessor (`pub static get
                    //   black(): NSColor { ... }` etc.).
                    if name == "static"
                        && matches!(
                            self.tokens.get(self.pos + 1).map(|t| &t.kind),
                            Some(TokenKind::Ident(_))
                        )
                    {
                        // Lookahead for `static get/set <ident> (`.
                        let kw_after_static = match self.tokens.get(self.pos + 1).map(|t| &t.kind) {
                            Some(TokenKind::Ident(n)) if n == "get" || n == "set" => Some(n.clone()),
                            _ => None,
                        };
                        if kw_after_static.is_some()
                            && matches!(
                                self.tokens.get(self.pos + 2).map(|t| &t.kind),
                                Some(TokenKind::Ident(_))
                            )
                            && matches!(
                                self.tokens.get(self.pos + 3).map(|t| &t.kind),
                                Some(TokenKind::LParen)
                            )
                        {
                            self.bump(); // consume `static`
                            self.parse_property_accessor_pub_with_static(
                                &mut properties,
                                member_is_pub,
                                /*is_static*/ true,
                            )?;
                            continue;
                        }
                        match self.tokens.get(self.pos + 2).map(|t| &t.kind) {
                            Some(TokenKind::LParen) => {
                                self.bump(); // consume `static`
                                let mut m = self.parse_method(Vec::new())?;
                                m.is_pub = member_is_pub;
                                static_methods.push(m);
                                continue;
                            }
                            Some(TokenKind::Colon) => {
                                self.bump(); // consume `static`
                                let mut f = self.parse_static_field(false)?;
                                f.is_pub = member_is_pub;
                                static_fields.push(f);
                                continue;
                            }
                            _ => {}
                        }
                    }
                    let next_kind = self.tokens[(self.pos + 1).min(self.tokens.len() - 1)]
                        .kind
                        .clone();
                    match next_kind {
                        TokenKind::Colon => {
                            let mut f = self.parse_field()?;
                            f.is_pub = member_is_pub;
                            fields.push(f);
                        }
                        TokenKind::LParen => {
                            let mut m = self.parse_method(Vec::new())?;
                            m.is_pub = member_is_pub;
                            methods.push(m);
                        }
                        other => {
                            let t = self.peek();
                            return Err(ParseError::Unexpected {
                                found: other,
                                expected: "':' (field) or '(' (method)".into(),
                                span: t.span,
                            });
                        }
                    }
                }
                _ => {
                    let t = self.peek();
                    return Err(ParseError::Unexpected {
                        found: t.kind.clone(),
                        expected: "field, method, or '}'".into(),
                        span: t.span,
                    });
                }
            }
        }
        self.expect(&TokenKind::RBrace, "'}'")?;
        Ok(ClassDecl {
            is_pub: false,
            extern_lib: None,
            is_repr_c: false,
            is_packed: false,
            is_union: false,
            name,
            parent,
            interfaces,
            type_params: type_params.into(),
            fields: fields.into(),
            methods: methods.into(),
            static_methods: static_methods.into(),
            static_fields: static_fields.into(),
            properties: properties.into(),
            attrs: Box::new([]),
            span,
        })
    }

    fn parse_interface_decl(&mut self) -> Result<ilang_ast::InterfaceDecl, ParseError> {
        let span = self.peek().span;
        self.expect(&TokenKind::Interface, "'interface'")?;
        let name = self.expect_ident("interface name")?;
        self.expect(&TokenKind::LBrace, "'{'")?;
        let mut methods: Vec<ilang_ast::InterfaceMethod> = Vec::new();
        while !matches!(self.peek().kind, TokenKind::RBrace) {
            // Attributes on interface methods. Currently the only
            // attribute we recognise is `@optional`, which marks
            // the method as not-required-to-implement. Unknown
            // attributes are kept in the parsed list and ignored
            // here; later passes may complain.
            let m_attrs = self.parse_attributes()?;
            let is_optional = m_attrs
                .iter()
                .any(|a| a.name.as_str() == "optional");
            let m_span = self.peek().span;
            // Method declarations mirror the class-body shape:
            // `name(params): ret` — no leading `fn` keyword. A
            // stray `fn` is rejected with a targeted message
            // rather than the generic "expected method name".
            if matches!(self.peek().kind, TokenKind::Fn) {
                let t = self.peek();
                return Err(ParseError::Unexpected {
                    found: t.kind.clone(),
                    expected: "method name — interface bodies use the same `name(params): ret` shape as a class body (drop the leading `fn`)".into(),
                    span: t.span,
                });
            }
            let m_name = self.expect_ident("method name")?;
            self.expect(&TokenKind::LParen, "'('")?;
            let mut params: Vec<ilang_ast::Param> = Vec::new();
            if !matches!(self.peek().kind, TokenKind::RParen) {
                loop {
                    let p_span = self.peek().span;
                    let p_name = self.expect_ident("parameter name")?;
                    self.expect(&TokenKind::Colon, "':'")?;
                    let p_ty = self.parse_type()?;
                    params.push(ilang_ast::Param {
                        name: p_name,
                        ty: p_ty,
                        default: None,
                        span: p_span,
                    });
                    if matches!(self.peek().kind, TokenKind::Comma) {
                        self.bump();
                    } else {
                        break;
                    }
                }
            }
            self.expect(&TokenKind::RParen, "')'")?;
            let ret = if matches!(self.peek().kind, TokenKind::Colon) {
                self.bump();
                Some(self.parse_type()?)
            } else {
                None
            };
            // Interface methods have no body — they declare a contract
            // implementing classes must satisfy.
            self.consume_stmt_terminator()?;
            methods.push(ilang_ast::InterfaceMethod {
                name: m_name,
                params: params.into(),
                ret,
                is_optional,
                span: m_span,
            });
        }
        self.expect(&TokenKind::RBrace, "'}'")?;
        Ok(ilang_ast::InterfaceDecl {
            is_pub: false,
            name,
            methods: methods.into(),
            span,
        })
    }

    /// `static <name>: <type> = <expr>` — caller has already
    /// consumed the `static` keyword; we start at the field name.
    /// `is_const` distinguishes `static` (mutable) from `const`
    /// (immutable; reassignment is rejected by the type checker).
    fn parse_static_field(&mut self, is_const: bool) -> Result<StaticFieldDecl, ParseError> {
        let span = self.peek().span;
        let name = self.expect_ident("static field name")?;
        self.expect(&TokenKind::Colon, "':'")?;
        let ty = self.parse_type()?;
        self.expect(&TokenKind::Equals, "'='")?;
        let value = self.parse_expr(0)?;
        Ok(StaticFieldDecl { is_pub: false, name, ty, value, is_const, span })
    }

    /// Parse one `get name(): T { body }` or `set name(v: T) { body }`
    /// accessor and merge it into the running properties list. Two
    /// accessors with the same name share a single PropertyDecl. The
    /// type checker validates getter ret == setter param later.
    fn parse_property_accessor_pub(
        &mut self,
        properties: &mut Vec<PropertyDecl>,
        is_pub: bool,
    ) -> Result<(), ParseError> {
        self.parse_property_accessor_pub_with_static(properties, is_pub, /*is_static*/ false)
    }

    /// Same as `parse_property_accessor_pub`, but stamps the resulting
    /// (or pre-existing) `PropertyDecl` as static. Caller is
    /// responsible for already having consumed any `static` keyword.
    fn parse_property_accessor_pub_with_static(
        &mut self,
        properties: &mut Vec<PropertyDecl>,
        is_pub: bool,
        is_static: bool,
    ) -> Result<(), ParseError> {
        let kw_span = self.peek().span;
        let kw = match &self.peek().kind {
            TokenKind::Ident(s) => s.clone(),
            _ => unreachable!("caller verified kind is Ident(get|set)"),
        };
        self.bump(); // get / set
        let name_span = self.peek().span;
        let prop_name = self.expect_ident("property name")?;
        // Re-use the existing parse_method machinery starting from the
        // `(` — but the method's `name` should be the property name and
        // we need to inject it. parse_method assumes the name was just
        // consumed; we'll mimic the body of parse_method here.
        let fn_decl = self.parse_method_after_name(prop_name.clone(), kw_span)?;
        // Validate accessor shape eagerly so misuse errors point at the
        // `get` / `set` keyword rather than at the call site.
        if kw == "get" {
            if !fn_decl.params.is_empty() {
                return Err(ParseError::Unexpected {
                    found: TokenKind::LParen,
                    expected: "getter takes no parameters".into(),
                    span: name_span,
                });
            }
            if fn_decl.ret.is_none() {
                return Err(ParseError::Unexpected {
                    found: TokenKind::LBrace,
                    expected: "getter must declare a return type".into(),
                    span: name_span,
                });
            }
        } else {
            if fn_decl.params.len() != 1 {
                return Err(ParseError::Unexpected {
                    found: TokenKind::LParen,
                    expected: "setter takes exactly one parameter".into(),
                    span: name_span,
                });
            }
            if fn_decl.ret.is_some() {
                return Err(ParseError::Unexpected {
                    found: TokenKind::Colon,
                    expected: "setter must not declare a return type".into(),
                    span: name_span,
                });
            }
        }
        let prop_ty = if kw == "get" {
            fn_decl.ret.clone().expect("getter ret checked above")
        } else {
            fn_decl.params[0].ty.clone()
        };
        // Find existing entry, or create a new one. Once one accessor
        // sets `is_pub`, the merged property is `pub`.
        if let Some(existing) = properties.iter_mut().find(|p| p.name == prop_name) {
            if existing.is_static != is_static {
                return Err(ParseError::Unexpected {
                    found: TokenKind::Ident(if is_static { "static" } else { "get" }.into()),
                    expected: format!(
                        "`{prop_name}` accessor mixes static and instance forms; pick one"
                    ),
                    span: kw_span,
                });
            }
            if is_pub {
                existing.is_pub = true;
            }
            if kw == "get" {
                if existing.getter.is_some() {
                    return Err(ParseError::Unexpected {
                        found: TokenKind::Ident("get".into()),
                        expected: format!("duplicate `get {prop_name}` accessor"),
                        span: kw_span,
                    });
                }
                existing.getter = Some(fn_decl);
            } else {
                if existing.setter.is_some() {
                    return Err(ParseError::Unexpected {
                        found: TokenKind::Ident("set".into()),
                        expected: format!("duplicate `set {prop_name}` accessor"),
                        span: kw_span,
                    });
                }
                existing.setter = Some(fn_decl);
            }
        } else {
            let (getter, setter) = if kw == "get" {
                (Some(fn_decl), None)
            } else {
                (None, Some(fn_decl))
            };
            properties.push(PropertyDecl {
                is_pub,
                is_static,
                name: prop_name,
                ty: prop_ty,
                getter,
                setter,
                span: kw_span,
            });
        }
        Ok(())
    }

    fn parse_field(&mut self) -> Result<FieldDecl, ParseError> {
        let span = self.peek().span;
        let name = self.expect_ident("field name")?;
        self.expect(&TokenKind::Colon, "':'")?;
        let ty = self.parse_type()?;
        self.consume_stmt_terminator()?;
        Ok(FieldDecl { is_pub: false, name, ty, span, bits: None })
    }

    fn parse_method(&mut self, attrs: Vec<Attribute>) -> Result<FnDecl, ParseError> {
        let span = self.peek().span;
        let name = self.expect_ident("method name")?;
        self.parse_method_after_name_with_attrs(name, span, attrs)
    }

    /// Like `parse_method`, but called when the caller already consumed
    /// the method name (e.g. property accessors that consumed `get`/`set`
    /// + ident first). Used for `get/set` accessor parsing.
    fn parse_method_after_name(
        &mut self,
        name: Symbol,
        span: ilang_ast::Span,
    ) -> Result<FnDecl, ParseError> {
        self.parse_method_after_name_with_attrs(name, span, Vec::new())
    }

    fn parse_method_after_name_with_attrs(
        &mut self,
        name: Symbol,
        span: ilang_ast::Span,
        attrs: Vec<Attribute>,
    ) -> Result<FnDecl, ParseError> {
        // Methods don't take their own type params — they inherit
        // the class's. Always empty here.
        let type_params: Vec<Symbol> = Vec::new();
        self.expect(&TokenKind::LParen, "'('")?;
        let params = self.parse_param_list()?;
        self.expect(&TokenKind::RParen, "')'")?;
        let ret = if matches!(self.peek().kind, TokenKind::Colon) {
            self.bump();
            Some(self.parse_type()?)
        } else {
            None
        };
        let body = parse_block(self)?;
        Ok(FnDecl {
            is_pub: false,
            attrs: attrs.into(),
            name,
            type_params: type_params.into(),
            params: params.into(),
            ret,
            body,
            span,
            is_override: false,
            is_async: false,
        })
    }

    /// Parse a sequence of `@name(args)` attributes (TS / Java / Python
    /// decorator style). Each `@` introduces one attribute; chain them
    /// for multiple. The argument list is required for now — bare `@x`
    /// without parens is a parse error so the syntax stays predictable.
    fn parse_attributes(&mut self) -> Result<Vec<Attribute>, ParseError> {
        let mut out = Vec::new();
        while matches!(self.peek().kind, TokenKind::At) {
            self.bump();
            let name = self.expect_ident("attribute name")?;
            // Argument list is optional. `@extern` (no parens) and
            // `@requires(net, file.read)` are both valid.
            let args = if matches!(self.peek().kind, TokenKind::LParen) {
                self.bump();
                let mut args = Vec::new();
                if !matches!(self.peek().kind, TokenKind::RParen) {
                    loop {
                        // String literal arg (`@extern("libm")`) or a
                        // capability path (`@requires(net)`).
                        if let TokenKind::Str(s) = &self.peek().kind {
                            let s = s.clone();
                            self.bump();
                            args.push(AttrArg::Str(s));
                        } else if let TokenKind::Int(n) = &self.peek().kind {
                            let n = *n;
                            self.bump();
                            args.push(AttrArg::Int(n));
                        } else {
                            let path = self.parse_attr_path()?;
                            args.push(AttrArg::Path(path.into()));
                        }
                        if matches!(self.peek().kind, TokenKind::Comma) {
                            self.bump();
                        } else {
                            break;
                        }
                    }
                }
                self.expect(&TokenKind::RParen, "')'")?;
                args
            } else {
                Vec::new()
            };
            out.push(Attribute { name, args: args.into() });
        }
        Ok(out)
    }

    fn parse_attr_path(&mut self) -> Result<Vec<Symbol>, ParseError> {
        let mut parts = vec![self.expect_ident("capability name")?];
        while matches!(self.peek().kind, TokenKind::Dot) {
            self.bump();
            parts.push(self.expect_ident("capability segment")?);
        }
        Ok(parts)
    }

    fn parse_dotted_ident(&mut self, expected: &str) -> Result<Symbol, ParseError> {
        let mut name = self.expect_ident(expected)?.as_str().to_string();
        while matches!(self.peek().kind, TokenKind::Dot) {
            self.bump();
            let segment = self.expect_ident(expected)?;
            name.push('.');
            name.push_str(segment.as_str());
        }
        Ok(Symbol::intern(&name))
    }

    fn parse_fn_decl(&mut self, attrs: Vec<Attribute>) -> Result<FnDecl, ParseError> {
        let span = self.peek().span;
        self.expect(&TokenKind::Fn, "'fn'")?;
        let name = self.expect_ident("function name")?;
        // Optional `<T, U>` type parameters (same shape as classes).
        let type_params = if matches!(self.peek().kind, TokenKind::Lt) {
            self.parse_type_param_list()?
        } else {
            Vec::new()
        };
        self.expect(&TokenKind::LParen, "'('")?;
        let params = self.parse_param_list()?;
        self.expect(&TokenKind::RParen, "')'")?;
        let ret = if matches!(self.peek().kind, TokenKind::Colon) {
            self.bump();
            Some(self.parse_type()?)
        } else {
            None
        };
        let body = parse_block(self)?;
        Ok(FnDecl {
            is_pub: false,
            attrs: attrs.into(),
            name,
            type_params: type_params.into(),
            params: params.into(),
            ret,
            body,
            span,
            is_override: false,
            is_async: false,
        })
    }
}
