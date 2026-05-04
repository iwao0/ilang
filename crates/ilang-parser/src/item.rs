use ilang_ast::{
    AttrArg, Attribute, ClassDecl, EnumDecl, FieldDecl, FnDecl, Item, Param,
    PropertyDecl, StaticFieldDecl, Type, Variant, VariantPayload,
};
use ilang_lexer::TokenKind;

use crate::error::ParseError;
use crate::parser::Parser;
use crate::stmt::parse_block;

/// True if `e` is a value-only literal — what `const` accepts as its
/// RHS. Numeric / bool / string literals, optionally with a unary
/// `-` on numerics. No identifiers, no calls, no expressions.

impl<'a> Parser<'a> {
    pub(crate) fn parse_item(&mut self) -> Result<Item, ParseError> {
        let attrs = self.parse_attributes()?;
        match self.peek().kind {
            TokenKind::Fn => {
                let fn_decl = self.parse_fn_decl(attrs)?;
                Ok(Item::Fn(fn_decl))
            }
            TokenKind::Class => {
                // Class-level attributes:
                //   `@extern("libname")` — opaque handle type
                //   `@repr(C)` — C-compatible struct layout for FFI
                let mut extern_lib: Option<String> = None;
                let mut is_repr_c = false;
                let mut is_packed = false;
                let mut is_union = false;
                for a in &attrs {
                    match (a.name.as_str(), a.args.as_slice()) {
                        ("extern", [ilang_ast::AttrArg::Str(s)]) => {
                            extern_lib = Some(s.clone());
                        }
                        ("repr", args) => {
                            // `@repr(C)` or `@repr(C, packed)`. Each
                            // arg must be a single-segment path; `C`
                            // is required and the only other accepted
                            // modifier today is `packed`.
                            let mut saw_c = false;
                            for arg in args {
                                match arg {
                                    ilang_ast::AttrArg::Path(p)
                                        if p.as_slice() == ["C"] =>
                                    {
                                        saw_c = true;
                                    }
                                    ilang_ast::AttrArg::Path(p)
                                        if p.as_slice() == ["packed"] =>
                                    {
                                        is_packed = true;
                                    }
                                    ilang_ast::AttrArg::Path(p)
                                        if p.as_slice() == ["union"] =>
                                    {
                                        is_union = true;
                                    }
                                    _ => {
                                        let t = self.peek();
                                        return Err(ParseError::Unexpected {
                                            found: t.kind.clone(),
                                            expected: "@repr(C), @repr(C, packed), or @repr(C, union) (other repr modifiers are not supported)".into(),
                                            span: t.span,
                                        });
                                    }
                                }
                            }
                            if !saw_c {
                                let t = self.peek();
                                return Err(ParseError::Unexpected {
                                    found: t.kind.clone(),
                                    expected: "@repr(C) — bare @repr or @repr(packed) without C is not supported".into(),
                                    span: t.span,
                                });
                            }
                            is_repr_c = true;
                        }
                        _ => {
                            let t = self.peek();
                            return Err(ParseError::Unexpected {
                                found: t.kind.clone(),
                                expected:
                                    "@extern(\"libname\") or @repr(C[, packed]) (other attributes are not supported on classes)"
                                        .into(),
                                span: t.span,
                            });
                        }
                    }
                }
                let mut c = self.parse_class_decl()?;
                if is_repr_c {
                    // C-compat struct: only fields, no methods/init/
                    // properties/inheritance. New (no args) zero-
                    // initializes the storage.
                    let has_disallowed = !c.methods.is_empty()
                        || !c.static_methods.is_empty()
                        || !c.static_fields.is_empty()
                        || !c.properties.is_empty()
                        || c.parent.is_some()
                        || !c.type_params.is_empty();
                    if has_disallowed {
                        return Err(ParseError::Unexpected {
                            found: TokenKind::Class,
                            expected: "fields only — `@repr(C) class Foo` cannot declare init, methods, parent, type parameters, properties, or static members".into(),
                            span: c.span,
                        });
                    }
                    c.is_repr_c = true;
                    c.is_packed = is_packed;
                    c.is_union = is_union;
                    if is_packed && is_union {
                        return Err(ParseError::Unexpected {
                            found: TokenKind::Class,
                            expected: "`packed` and `union` cannot be combined — packed implies struct semantics".into(),
                            span: c.span,
                        });
                    }
                } else if is_packed || is_union {
                    let t = self.peek();
                    return Err(ParseError::Unexpected {
                        found: t.kind.clone(),
                        expected: "`packed` / `union` require `C` — use `@repr(C, packed)` / `@repr(C, union)`".into(),
                        span: t.span,
                    });
                }
                if extern_lib.is_some() {
                    // Opaque handle classes carry no user state and
                    // can declare at most one method: `deinit`. The
                    // deinit body runs when the last reference is
                    // dropped (RAII auto-close).
                    let only_deinit_methods = c
                        .methods
                        .iter()
                        .all(|m| m.name == "deinit" && m.params.is_empty());
                    let has_disallowed = !c.fields.is_empty()
                        || !only_deinit_methods
                        || !c.static_methods.is_empty()
                        || !c.static_fields.is_empty()
                        || !c.properties.is_empty()
                        || c.parent.is_some()
                        || !c.type_params.is_empty();
                    if has_disallowed {
                        return Err(ParseError::Unexpected {
                            found: TokenKind::Class,
                            expected: "an empty body or only `deinit { ... }` — `@extern(\"lib\") class Foo` cannot declare fields, init, parent, type parameters, or non-deinit methods".into(),
                            span: c.span,
                        });
                    }
                    c.extern_lib = extern_lib;
                }
                Ok(Item::Class(c))
            }
            TokenKind::Enum => {
                if !attrs.is_empty() {
                    let t = self.peek();
                    return Err(ParseError::Unexpected {
                        found: t.kind.clone(),
                        expected: "'fn' (attributes on enums are not supported)".into(),
                        span: t.span,
                    });
                }
                let e = self.parse_enum_decl()?;
                Ok(Item::Enum(e))
            }
            TokenKind::Use => {
                if !attrs.is_empty() {
                    let t = self.peek();
                    return Err(ParseError::Unexpected {
                        found: t.kind.clone(),
                        expected: "'fn' (attributes on `use` are not supported)".into(),
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
                let c = self.parse_const_decl()?;
                Ok(Item::Const(c))
            }
            TokenKind::Ident(ref name) if name == "static" && !attrs.is_empty() => {
                // `@extern[(\"lib\")] static <name>: <ty>` — read/
                // write reference to a C global resolved via dlsym.
                let span = self.peek().span;
                self.bump(); // consume `static`
                let s_name = self.expect_ident("static name")?;
                self.expect(&TokenKind::Colon, "':'")?;
                let ty = self.parse_type()?;
                self.consume_stmt_terminator()?;
                let mut lib: Option<String> = None;
                let mut saw_extern = false;
                for a in &attrs {
                    match (a.name.as_str(), a.args.as_slice()) {
                        ("extern", []) => {
                            saw_extern = true;
                        }
                        ("extern", [AttrArg::Str(s)]) => {
                            saw_extern = true;
                            lib = Some(s.clone());
                        }
                        _ => {
                            return Err(ParseError::Unexpected {
                                found: TokenKind::At,
                                expected: "@extern or @extern(\"libname\") on a top-level static (no other attributes are recognised)".into(),
                                span,
                            });
                        }
                    }
                }
                if !saw_extern {
                    return Err(ParseError::Unexpected {
                        found: TokenKind::Ident("static".into()),
                        expected: "top-level `static` requires `@extern` (only extern globals are supported)".into(),
                        span,
                    });
                }
                Ok(Item::ExternStatic(ilang_ast::ExternStaticDecl {
                    name: s_name,
                    ty,
                    lib,
                    span,
                }))
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
        self.expect(&TokenKind::Equals, "'='")?;
        let value = self.parse_expr(0)?;
        // The parser accepts any expression here; the loader's
        // `inline_constants` pass folds it to a literal (or errors).
        Ok(ilang_ast::ConstDecl {
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
        Ok(ilang_ast::UseDecl {
            module,
            selective,
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
        // `extends Parent` (single inheritance, optional).
        let parent = if matches!(self.peek().kind, TokenKind::Extends) {
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
            match self.peek().kind {
                TokenKind::RBrace => break,
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
                                let bits = match a.args.as_slice() {
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
                        fields.push(f);
                    } else {
                        let m = self.parse_method(attrs)?;
                        methods.push(m);
                    }
                }
                TokenKind::Override => {
                    self.bump(); // consume `override`
                    let mut m = self.parse_method(Vec::new())?;
                    m.is_override = true;
                    methods.push(m);
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
                        self.parse_property_accessor(&mut properties)?;
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
                                let m = self.parse_method(Vec::new())?;
                                static_methods.push(m);
                                continue;
                            }
                            Some(TokenKind::Colon) => {
                                self.bump(); // consume `static`
                                let f = self.parse_static_field()?;
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
                            let f = self.parse_field()?;
                            fields.push(f);
                        }
                        TokenKind::LParen => {
                            let m = self.parse_method(Vec::new())?;
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
            extern_lib: None,
            is_repr_c: false,
            is_packed: false,
            is_union: false,
            name,
            parent,
            type_params,
            fields,
            methods,
            static_methods,
            static_fields,
            properties,
            span,
        })
    }

    /// `static <name>: <type> = <expr>` — caller has already
    /// consumed the `static` keyword; we start at the field name.
    fn parse_static_field(&mut self) -> Result<StaticFieldDecl, ParseError> {
        let span = self.peek().span;
        let name = self.expect_ident("static field name")?;
        self.expect(&TokenKind::Colon, "':'")?;
        let ty = self.parse_type()?;
        self.expect(&TokenKind::Equals, "'='")?;
        let value = self.parse_expr(0)?;
        Ok(StaticFieldDecl { name, ty, value, span })
    }

    /// Parse one `get name(): T { body }` or `set name(v: T) { body }`
    /// accessor and merge it into the running properties list. Two
    /// accessors with the same name share a single PropertyDecl. The
    /// type checker validates getter ret == setter param later.
    fn parse_property_accessor(
        &mut self,
        properties: &mut Vec<PropertyDecl>,
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
        // Find existing entry, or create a new one.
        if let Some(existing) = properties.iter_mut().find(|p| p.name == prop_name) {
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
                name: prop_name,
                ty: prop_ty,
                getter,
                setter,
                span: kw_span,
            });
        }
        Ok(())
    }

    fn parse_enum_decl(&mut self) -> Result<EnumDecl, ParseError> {
        let span = self.peek().span;
        self.expect(&TokenKind::Enum, "'enum'")?;
        let name = self.expect_ident("enum name")?;
        // Optional `<T, U>` type parameters — same shape as classes.
        let type_params = if matches!(self.peek().kind, TokenKind::Lt) {
            self.parse_type_param_list()?
        } else {
            Vec::new()
        };
        self.expect(&TokenKind::LBrace, "'{'")?;
        let mut variants = Vec::new();
        while !matches!(self.peek().kind, TokenKind::RBrace) {
            let v_span = self.peek().span;
            let v_name = self.expect_ident("variant name")?;
            // Payload is introduced by `:` — either `: (Ty, ...)` for
            // tuple or `: { name: Ty, ... }` for struct. Without a `:`
            // the variant is a unit (no payload).
            let payload = if matches!(self.peek().kind, TokenKind::Colon) {
                self.bump();
                match self.peek().kind {
                    TokenKind::LParen => {
                        self.bump();
                        let mut tys = Vec::new();
                        if !matches!(self.peek().kind, TokenKind::RParen) {
                            loop {
                                tys.push(self.parse_type()?);
                                if matches!(self.peek().kind, TokenKind::Comma) {
                                    self.bump();
                                } else {
                                    break;
                                }
                            }
                        }
                        self.expect(&TokenKind::RParen, "')'")?;
                        VariantPayload::Tuple(tys)
                    }
                    TokenKind::LBrace => {
                        self.bump();
                        let mut fields = Vec::new();
                        while !matches!(self.peek().kind, TokenKind::RBrace) {
                            let f_span = self.peek().span;
                            let f_name = self.expect_ident("field name")?;
                            self.expect(&TokenKind::Colon, "':'")?;
                            let f_ty = self.parse_type()?;
                            fields.push(FieldDecl {
                                name: f_name,
                                ty: f_ty,
                                span: f_span,
                                bits: None,
                            });
                            if matches!(self.peek().kind, TokenKind::Comma) {
                                self.bump();
                            } else if !matches!(self.peek().kind, TokenKind::RBrace)
                                && !self.peek().leading_newline
                            {
                                let p = self.peek();
                                return Err(ParseError::Unexpected {
                                    found: p.kind.clone(),
                                    expected: "',' or newline between struct fields".into(),
                                    span: p.span,
                                });
                            }
                        }
                        self.expect(&TokenKind::RBrace, "'}'")?;
                        VariantPayload::Struct(fields)
                    }
                    _ => {
                        let p = self.peek();
                        return Err(ParseError::Unexpected {
                            found: p.kind.clone(),
                            expected: "'(' (tuple payload) or '{' (struct payload) after ':'"
                                .into(),
                            span: p.span,
                        });
                    }
                }
            } else {
                VariantPayload::Unit
            };
            variants.push(Variant {
                name: v_name,
                payload,
                span: v_span,
            });
            // Variants separated by commas or newlines.
            if matches!(self.peek().kind, TokenKind::Comma) {
                self.bump();
            } else if !matches!(self.peek().kind, TokenKind::RBrace)
                && !self.peek().leading_newline
            {
                let p = self.peek();
                return Err(ParseError::Unexpected {
                    found: p.kind.clone(),
                    expected: "',' or newline between variants".into(),
                    span: p.span,
                });
            }
        }
        self.expect(&TokenKind::RBrace, "'}'")?;
        Ok(EnumDecl {
            name,
            type_params,
            variants,
            span,
        })
    }

    fn parse_field(&mut self) -> Result<FieldDecl, ParseError> {
        let span = self.peek().span;
        let name = self.expect_ident("field name")?;
        self.expect(&TokenKind::Colon, "':'")?;
        let ty = self.parse_type()?;
        self.consume_stmt_terminator()?;
        Ok(FieldDecl { name, ty, span, bits: None })
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
        name: String,
        span: ilang_ast::Span,
    ) -> Result<FnDecl, ParseError> {
        self.parse_method_after_name_with_attrs(name, span, Vec::new())
    }

    fn parse_method_after_name_with_attrs(
        &mut self,
        name: String,
        span: ilang_ast::Span,
        attrs: Vec<Attribute>,
    ) -> Result<FnDecl, ParseError> {
        // Methods don't take their own type params — they inherit
        // the class's. Always empty here.
        let type_params: Vec<String> = Vec::new();
        self.expect(&TokenKind::LParen, "'('")?;
        let params = self.parse_param_list()?;
        self.expect(&TokenKind::RParen, "')'")?;
        let ret = if matches!(self.peek().kind, TokenKind::Colon) {
            self.bump();
            Some(self.parse_type()?)
        } else {
            None
        };
        // `@extern` fns have no body — the runtime supplies the
        // implementation via a name-based registry.
        let body = if attrs.iter().any(|a| a.name == "extern") {
            ilang_ast::Block { stmts: Vec::new(), tail: None }
        } else {
            parse_block(self)?
        };
        Ok(FnDecl {
            attrs,
            name,
            type_params,
            params,
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
                            args.push(AttrArg::Path(path));
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
            out.push(Attribute { name, args });
        }
        Ok(out)
    }

    fn parse_attr_path(&mut self) -> Result<Vec<String>, ParseError> {
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
        // `@extern` fns have no body — the runtime supplies the
        // implementation via a name-based registry.
        let body = if attrs.iter().any(|a| a.name == "extern") {
            ilang_ast::Block { stmts: Vec::new(), tail: None }
        } else {
            parse_block(self)?
        };
        Ok(FnDecl {
            attrs,
            name,
            type_params,
            params,
            ret,
            body,
            span,
            is_override: false,
        })
    }

    /// Parse `<T, U, ...>` after a class name in declaration position.
    /// Returns the bare identifier names; uniqueness is checked downstream.
    fn parse_type_param_list(&mut self) -> Result<Vec<String>, ParseError> {
        self.expect(&TokenKind::Lt, "'<'")?;
        let mut names = Vec::new();
        loop {
            names.push(self.expect_ident("type parameter name")?);
            if matches!(self.peek().kind, TokenKind::Comma) {
                self.bump();
            } else {
                break;
            }
        }
        self.expect_close_gt()?;
        Ok(names)
    }

    /// Parse a comma-separated parameter list `name: T` or
    /// `name: T = default_expr`. The opening `(` and closing `)` are
    /// expected to be handled by the caller. Validates that defaults
    /// only appear on trailing parameters (once one parameter has a
    /// default, every later one must too).
    pub(crate) fn parse_param_list(&mut self) -> Result<Vec<Param>, ParseError> {
        let mut params = Vec::new();
        if matches!(self.peek().kind, TokenKind::RParen) {
            return Ok(params);
        }
        let mut seen_default_at: Option<ilang_ast::Span> = None;
        loop {
            let pspan = self.peek().span;
            let pname = self.expect_ident("parameter name")?;
            self.expect(&TokenKind::Colon, "':'")?;
            let pty = self.parse_type()?;
            let default = if matches!(self.peek().kind, TokenKind::Equals) {
                self.bump();
                let expr = self.parse_expr(0)?;
                seen_default_at = Some(pspan);
                Some(expr)
            } else {
                if let Some(_first) = seen_default_at {
                    return Err(ParseError::Unexpected {
                        found: self.peek().kind.clone(),
                        expected: "'=' (parameter without default cannot follow one with a default)"
                            .into(),
                        span: pspan,
                    });
                }
                None
            };
            params.push(Param {
                name: pname,
                ty: pty,
                span: pspan,
                default,
            });
            if matches!(self.peek().kind, TokenKind::Comma) {
                self.bump();
            } else {
                break;
            }
        }
        Ok(params)
    }

    /// Parse `<T, U, ...>` of concrete type arguments (used in generic
    /// type references and `new Foo<T>(args)`).
    pub(crate) fn parse_type_args(&mut self) -> Result<Vec<Type>, ParseError> {
        self.expect(&TokenKind::Lt, "'<'")?;
        let mut args = Vec::new();
        loop {
            args.push(self.parse_type()?);
            if matches!(self.peek().kind, TokenKind::Comma) {
                self.bump();
            } else {
                break;
            }
        }
        self.expect_close_gt()?;
        Ok(args)
    }

    /// Consume a closing `>` for a generic. Handles the `>>` case by
    /// splitting it: the inner generic registers one "virtual" `>` via
    /// `pending_close_gt` so the outer can close without re-tokenizing.
    fn expect_close_gt(&mut self) -> Result<(), ParseError> {
        // Outer close after a previously-split `>>`.
        if self.pending_close_gt > 0 {
            self.pending_close_gt -= 1;
            self.bump(); // consume the `>>` token now that both halves used
            return Ok(());
        }
        let peeked = self.peek().clone();
        match peeked.kind {
            TokenKind::Gt => {
                self.bump();
                Ok(())
            }
            TokenKind::GtGt => {
                // Take the first `>` here; leave the token in place so the
                // surrounding generic's close picks up the second.
                self.pending_close_gt += 1;
                Ok(())
            }
            other => Err(ParseError::Unexpected {
                found: other,
                expected: "'>'".into(),
                span: peeked.span,
            }),
        }
    }

    pub(crate) fn parse_type(&mut self) -> Result<Type, ParseError> {
        let t = self.peek().clone();
        // Function type: `fn(T1, T2): R` (or `fn(): R` / `fn(T)` for unit ret).
        if matches!(t.kind, TokenKind::Fn) {
            self.bump();
            self.expect(&TokenKind::LParen, "'('")?;
            let mut params = Vec::new();
            if !matches!(self.peek().kind, TokenKind::RParen) {
                loop {
                    params.push(self.parse_type()?);
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
                self.parse_type()?
            } else {
                Type::Unit
            };
            return Ok(Type::Fn {
                params,
                ret: Box::new(ret),
            });
        }
        // Tuple type: `(T1, T2, ...)`. A single `(T)` is grouping and
        // returns `T` itself; `()` would be unit but is not currently
        // emitted by the type parser.
        if matches!(t.kind, TokenKind::LParen) {
            self.bump();
            let first = self.parse_type()?;
            if matches!(self.peek().kind, TokenKind::Comma) {
                let mut elems = vec![first];
                while matches!(self.peek().kind, TokenKind::Comma) {
                    self.bump();
                    if matches!(self.peek().kind, TokenKind::RParen) {
                        break;
                    }
                    elems.push(self.parse_type()?);
                }
                self.expect(&TokenKind::RParen, "')'")?;
                return Ok(Type::Tuple(elems));
            }
            self.expect(&TokenKind::RParen, "')'")?;
            return Ok(first);
        }
        let mut ty = match t.kind {
            TokenKind::Ident(n) => {
                self.bump();
                match n.as_str() {
                    "i8" => Type::I8,
                    "i16" => Type::I16,
                    "i32" => Type::I32,
                    "i64" => Type::I64,
                    "u8" => Type::U8,
                    "u16" => Type::U16,
                    "u32" => Type::U32,
                    "u64" => Type::U64,
                    "f32" => Type::F32,
                    "f64" => Type::F64,
                    "bool" => Type::Bool,
                    "string" => Type::Str,
                    _ => {
                        // After a class-like name, accept optional
                        // `<T, U>` for generic instantiations:
                        //   Box<i64>          → Generic { Box, [i64] }
                        //   Pair<string, i64> → Generic { Pair, [Str, I64] }
                        if matches!(self.peek().kind, TokenKind::Lt) {
                            let args = self.parse_type_args()?;
                            Type::Generic { base: n, args }
                        } else {
                            Type::Object(n)
                        }
                    }
                }
            }
            other => {
                return Err(ParseError::Unexpected {
                    found: other,
                    expected: "type name".into(),
                    span: t.span,
                });
            }
        };
        // Postfix modifiers: array `T[]` / `T[N]` and optional `T?`.
        // Both can chain (`T[]?`, `T?[]`, `T??` though redundant).
        loop {
            match self.peek().kind {
                TokenKind::LBracket => {
                    self.bump();
                    let fixed = match self.peek().kind {
                        TokenKind::RBracket => None,
                        TokenKind::Int(n) if n >= 0 => {
                            self.bump();
                            Some(n as usize)
                        }
                        _ => {
                            let p = self.peek();
                            return Err(ParseError::Unexpected {
                                found: p.kind.clone(),
                                expected: "']' or non-negative integer literal".into(),
                                span: p.span,
                            });
                        }
                    };
                    self.expect(&TokenKind::RBracket, "']'")?;
                    ty = Type::Array {
                        elem: Box::new(ty),
                        fixed,
                    };
                }
                TokenKind::Question => {
                    self.bump();
                    ty = Type::Optional(Box::new(ty));
                }
                TokenKind::Dot => {
                    // `.weak` postfix — only valid form at the moment.
                    // We snapshot the position so an unrelated dot
                    // sequence after a type wouldn't accidentally be
                    // consumed (no such case today, but safe-guarded).
                    if matches!(
                        self.peek_n(1).map(|t| &t.kind),
                        Some(TokenKind::Ident(n)) if n == "weak"
                    ) {
                        self.bump(); // .
                        self.bump(); // weak
                        ty = Type::Weak(Box::new(ty));
                    } else {
                        break;
                    }
                }
                _ => break,
            }
        }
        Ok(ty)
    }
}
