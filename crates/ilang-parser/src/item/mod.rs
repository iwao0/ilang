use ilang_ast::{
    AttrArg, Attribute, ClassDecl, FieldDecl, FnDecl, Item, PropertyDecl, StaticFieldDecl, Symbol,
};
use ilang_lexer::TokenKind;

use crate::error::ParseError;
use crate::parser::Parser;
use crate::stmt::parse_block;

mod enum_;
mod extern_c;
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
            if !matches!(u.alias, ilang_ast::UseAlias::Default) {
                return Err(ParseError::Unexpected {
                    found: TokenKind::As,
                    expected: "`pub use` does not support `as <alias>`".into(),
                    span: u.span,
                });
            }
            u.re_export = true;
            return Ok(Item::Use(u));
        }
        match self.peek().kind {
            TokenKind::Fn => {
                let mut fn_decl = self.parse_fn_decl(attrs)?;
                fn_decl.is_pub = is_pub;
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
                if !attrs.is_empty() {
                    let t = self.peek();
                    return Err(ParseError::Unexpected {
                        found: t.kind.clone(),
                        expected: "'fn' (attributes on `const` are not supported)".into(),
                        span: t.span,
                    });
                }
                let mut c = self.parse_const_decl()?;
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
    fn parse_const_decl(&mut self) -> Result<ilang_ast::ConstDecl, ParseError> {
        let span = self.peek().span;
        self.expect(&TokenKind::Const, "'const'")?;
        let name = self.expect_ident("constant name")?;
        let ty = if matches!(self.peek().kind, TokenKind::Colon) {
            self.bump();
            Some(self.parse_type()?)
        } else {
            None
        };
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
            span,
        })
    }

    /// `use module` (whole-module import) or
    /// `use module { name1, name2, ... }` (selective).
    fn parse_use_decl(&mut self) -> Result<ilang_ast::UseDecl, ParseError> {
        let span = self.peek().span;
        self.expect(&TokenKind::Use, "'use'")?;
        let module = self.expect_ident("module name")?;
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
        let selective = if matches!(self.peek().kind, TokenKind::LBrace) {
            self.bump();
            let mut names = Vec::new();
            if !matches!(self.peek().kind, TokenKind::RBrace) {
                loop {
                    names.push(self.expect_ident("imported name")?);
                    if matches!(self.peek().kind, TokenKind::Comma) {
                        self.bump();
                    } else {
                        break;
                    }
                }
            }
            self.expect(&TokenKind::RBrace, "'}'")?;
            Some(names)
        } else {
            None
        };
        // `use M as _` without a selective list has no observable
        // effect (the namespace is suppressed and no bare names are
        // imported). Reject it so the user catches the typo at parse
        // time.
        if matches!(alias, ilang_ast::UseAlias::Discard) && selective.is_none() {
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
        // `: Parent` (single inheritance, optional). Unambiguous in
        // class-decl position — the only other thing that can follow
        // the name (or its `<...>` type params) is `{`.
        let parent = if matches!(self.peek().kind, TokenKind::Colon) {
            self.bump();
            Some(self.expect_ident("parent class name")?)
        } else {
            None
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
                    if name == "static"
                        && matches!(
                            self.tokens.get(self.pos + 1).map(|t| &t.kind),
                            Some(TokenKind::Ident(_))
                        )
                    {
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
            type_params: type_params.into(),
            fields: fields.into(),
            methods: methods.into(),
            static_methods: static_methods.into(),
            static_fields: static_fields.into(),
            properties: properties.into(),
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
        })
    }
}
