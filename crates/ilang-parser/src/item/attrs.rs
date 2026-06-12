//! Attribute parsing — `@name(args, ...)` decorators that prefix
//! items, methods, and fields. Hosts the tiny `parse_attr_path` and
//! `parse_dotted_ident` helpers used by attribute argument lists and
//! by `parse_class_decl`'s `: Parent` base list.

use ilang_ast::{AttrArg, Attribute, Symbol};
use ilang_lexer::TokenKind;

use crate::error::ParseError;
use crate::parser::Parser;

/// The canonical set of user-writable attributes. Anything else is
/// a parse error — a typo'd attribute (`@derieve`) used to pass
/// through silently and simply have no effect. Compiler-SYNTHESISED
/// attributes (`sel`, `byValue`, `variadic` — minted by the
/// `@extern` loaders after parsing) never pass through here and
/// intentionally aren't listed.
const KNOWN_ATTRIBUTES: &[&str] = &[
    "bits",       // bitfield width on @extern(C) struct fields
    "com",        // COM interface marker
    "deprecated", // deprecation note surfaced by the checker / LSP
    "derive",     // @derive(Eq, Hash)
    "embed",      // @embed("file") const — compile-time file embed
    "extern",     // @extern / @extern(C) / @extern("lib")
    "flags",      // bitflags enum
    "handle",     // opaque pointer-sized handle struct
    "intrinsic",  // runtime-supplied fn
    "lib",        // dynamic library for an extern decl
    "objc",       // ObjC binding marker / selector
    "optional",   // optional ObjC protocol method
    "packed",     // @extern(C) packed struct layout
    "requires",   // capability list (parsed, not yet enforced)
    "since",      // availability note (ObjC bindings)
    "symbol",     // explicit C symbol name
    "target",     // platform gate (@target("macos") / not "...")
];

impl<'a> Parser<'a> {
    /// Parse a sequence of `@name(args)` attributes (TS / Java / Python
    /// decorator style). Each `@` introduces one attribute; chain them
    /// for multiple. The argument list is required for now — bare `@x`
    /// without parens is a parse error so the syntax stays predictable.
    pub(in crate::item) fn parse_attributes(&mut self) -> Result<Vec<Attribute>, ParseError> {
        let mut out = Vec::new();
        while matches!(self.peek().kind, TokenKind::At) {
            let at_span = self.peek().span;
            self.bump();
            let name = self.expect_ident("attribute name")?;
            if !KNOWN_ATTRIBUTES.contains(&name.as_str()) {
                return Err(ParseError::Generic {
                    msg: format!(
                        "unknown attribute @{name} (known attributes: {})",
                        KNOWN_ATTRIBUTES
                            .iter()
                            .map(|a| format!("@{a}"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                    span: at_span,
                });
            }
            // Argument list is optional. `@extern` (no parens) and
            // `@requires(net, file.read)` are both valid.
            let args = if matches!(self.peek().kind, TokenKind::LParen) {
                self.bump();
                let mut args = Vec::new();
                if !matches!(self.peek().kind, TokenKind::RParen) {
                    loop {
                        // String literal arg (`@extern("libm")`) or a
                        // capability path (`@requires(net)`).
                        // `not "X"` — negated string form, only valid
                        // when the identifier `not` is followed by a
                        // string literal directly (no comma). The
                        // semantics layer (`@target`) decides whether
                        // to accept this; other attrs reject NotStr.
                        let is_not_str = matches!(
                            &self.peek().kind,
                            TokenKind::Ident(s) if s.as_str() == "not"
                        ) && matches!(
                            self.peek_n(1).map(|t| &t.kind),
                            Some(TokenKind::Str(_))
                        );
                        if is_not_str {
                            self.bump();
                            if let TokenKind::Str(s) = self.peek().kind.clone() {
                                self.bump();
                                args.push(AttrArg::NotStr(s));
                            }
                        } else if let TokenKind::Str(s) = &self.peek().kind {
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

    pub(in crate::item) fn parse_attr_path(&mut self) -> Result<Vec<Symbol>, ParseError> {
        let mut parts = vec![self.expect_ident("capability name")?];
        while matches!(self.peek().kind, TokenKind::Dot) {
            self.bump();
            parts.push(self.expect_ident("capability segment")?);
        }
        Ok(parts)
    }

    pub(in crate::item) fn parse_dotted_ident(&mut self, expected: &str) -> Result<Symbol, ParseError> {
        let mut name = self.expect_ident(expected)?.as_str().to_string();
        while matches!(self.peek().kind, TokenKind::Dot) {
            self.bump();
            let segment = self.expect_ident(expected)?;
            name.push('.');
            name.push_str(segment.as_str());
        }
        Ok(Symbol::intern(&name))
    }
}
