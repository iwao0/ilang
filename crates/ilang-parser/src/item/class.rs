//! Class / interface body parsing. `parse_class_decl` walks a
//! `class Name { ... }` body, classifying each member (field /
//! method / static / property accessor / interface base list)
//! and reusing the per-member helpers below. `parse_interface_decl`
//! handles the simpler interface body: just method signatures, no
//! bodies. The per-member helpers (`parse_field`, `parse_method*`,
//! `parse_static_field`, `parse_property_accessor*`) live here too
//! so the dispatch stays close to its callees.

use ilang_ast::{
    Attribute, ClassDecl, FieldDecl, FnDecl, PropertyDecl, StaticFieldDecl, Symbol,
};
use ilang_lexer::TokenKind;

use crate::error::ParseError;
use crate::parser::Parser;
use crate::stmt::parse_block;

impl<'a> Parser<'a> {
    pub(in crate::item) fn parse_class_decl(&mut self) -> Result<ClassDecl, ParseError> {
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
                    // Allow `@attr pub method(...)` — top-level items
                    // accept attrs before `pub`, mirror that order
                    // inside class bodies so users don't have to
                    // remember a different rule for members.
                    let attr_pub = if matches!(self.peek().kind, TokenKind::Pub) {
                        let t = self.peek();
                        if member_is_pub {
                            return Err(ParseError::Unexpected {
                                found: TokenKind::Pub,
                                expected: "single `pub` modifier — \
                                    don't write `pub` both before and \
                                    after the attribute list".into(),
                                span: t.span,
                            });
                        }
                        self.bump();
                        true
                    } else {
                        false
                    };
                    let member_is_pub = member_is_pub || attr_pub;
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
                            Some(TokenKind::LParen) | Some(TokenKind::Lt) => {
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
                        TokenKind::LParen | TokenKind::Lt => {
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
        is_handle: false,
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

    pub(in crate::item) fn parse_interface_decl(&mut self, is_objc: bool) -> Result<ilang_ast::InterfaceDecl, ParseError> {
        let span = self.peek().span;
        self.expect(&TokenKind::Interface, "'interface'")?;
        let name = self.expect_ident("interface name")?;
        // Optional `: Parent` — single-interface inheritance.
        // Used by `@com interface ID3D12Device : IUnknown { ... }` to
        // chain vtable slots so the parent's methods occupy the
        // leading slots (matching the COM ABI). Plain (non-@com)
        // interfaces accept the parse for forward-compat but the
        // checker / MIR ignore it for now.
        let parent = if matches!(self.peek().kind, TokenKind::Colon) {
            self.bump();
            Some(self.expect_ident("parent interface name")?)
        } else {
            None
        };
        self.expect(&TokenKind::LBrace, "'{'")?;
        let mut methods: Vec<ilang_ast::InterfaceMethod> = Vec::new();
        while !matches!(self.peek().kind, TokenKind::RBrace) {
            // Attributes on interface methods:
            //   `@objc("selector:")`    — explicit Objective-C
            //                             selector for an @objc
            //                             interface method.
            // Optional methods are expressed with a trailing `?` on
            // the method name (e.g. `foo?(x: i64)`) — the legacy
            // `@optional` attribute is rejected.
            let attr_span = self.peek().span;
            let m_attrs = self.parse_attributes()?;
            let mut objc_selector: Option<Symbol> = None;
            for a in m_attrs.iter() {
                match a.name.as_str() {
                    "objc" => {
                        if !is_objc {
                            return Err(ParseError::Unexpected {
                                found: TokenKind::Ident(a.name.to_string()),
                                expected: "@objc(\"selector:\") is only allowed on methods of @objc interfaces".into(),
                                span: attr_span,
                            });
                        }
                        match &a.args[..] {
                            [ilang_ast::AttrArg::Str(s)] => {
                                objc_selector = Some(Symbol::intern(s));
                            }
                            _ => {
                                let t = self.peek();
                                return Err(ParseError::Unexpected {
                                    found: t.kind.clone(),
                                    expected: "@objc(\"selector:\") takes exactly one string argument".into(),
                                    span: t.span,
                                });
                            }
                        }
                    }
                    "optional" => {
                        let expected = if is_objc {
                            "the `@optional` attribute has been removed — write a trailing `?` on the method name instead (e.g. `foo?(x: i64)`)".into()
                        } else {
                            "optional interface methods are only allowed inside `@objc` interfaces — remove `@optional`".into()
                        };
                        return Err(ParseError::Unexpected {
                            found: TokenKind::Ident(a.name.to_string()),
                            expected,
                            span: attr_span,
                        });
                    }
                    _ => {
                        return Err(ParseError::Unexpected {
                            found: TokenKind::Ident(a.name.to_string()),
                            expected: "only `@objc(\"selector:\")` is supported as an interface-method attribute".into(),
                            span: attr_span,
                        });
                    }
                }
            }
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
            // Trailing `?` after the method name marks an optional
            // method (Objective-C `@optional` protocol method). Only
            // legal inside `@objc` interfaces; rejected elsewhere so
            // plain interfaces keep a strict conformance contract.
            let is_optional = if matches!(self.peek().kind, TokenKind::Question) {
                let q_span = self.peek().span;
                if !is_objc {
                    return Err(ParseError::Unexpected {
                        found: TokenKind::Question,
                        expected: "optional interface methods (trailing `?`) are only allowed inside `@objc` interfaces".into(),
                        span: q_span,
                    });
                }
                self.bump();
                true
            } else {
                false
            };
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
                objc_selector,
                span: m_span,
            });
        }
        self.expect(&TokenKind::RBrace, "'}'")?;
        Ok(ilang_ast::InterfaceDecl {
            is_pub: false,
            name,
            methods: methods.into(),
            is_objc: false,
            is_com: false,
            parent,
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
        // Optional `<T, U>` method-level type parameters. Stack on top
        // of any class-level params; the checker merges both lists into
        // `current_type_params` when checking the body.
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
            intrinsic_name: None,
        })
    }
}
