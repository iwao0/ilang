//! `const NAME [: T] = expr` — top-level immutable binding. The
//! parser accepts an arbitrary expression; the loader's
//! `inline_constants` pass tries to fold it to a literal and
//! inline at each reference, falling back to a once-evaluated
//! runtime initializer (`Stmt::Let { is_const: true, ... }`)
//! when the RHS can't be folded.

use ilang_lexer::TokenKind;

use crate::error::ParseError;
use crate::parser::Parser;

impl<'a> Parser<'a> {
    /// `const NAME [: T] = expr` — top-level immutable binding.
    /// The parser accepts an arbitrary expression; the loader's
    /// `inline_constants` pass tries to fold it to a literal and
    /// inline at each reference, falling back to a once-evaluated
    /// runtime initializer (`Stmt::Let { is_const: true, ... }`)
    /// when the RHS can't be folded.
    ///
    /// When `embed_path` is `Some`, the const is being initialised
    /// from a file via `@embed("path")` — `=` must be absent and the
    /// type annotation must be present (we can't infer `string` vs
    /// `u8[]` from the file alone). A placeholder `value` is stored
    /// so downstream passes have a well-typed `ConstDecl`; the
    /// loader replaces it once the file is read.
    pub(in crate::item) fn parse_const_decl(
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
                in_extern_c: false,
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
        // `inline_constants` pass tries to fold it to a literal and,
        // when that fails, demotes the decl to a once-evaluated
        // runtime initializer (`Stmt::Let { is_const: true, ... }`).
        Ok(ilang_ast::ConstDecl {
            is_pub: false,
            name,
            ty,
            value,
            embed_path: None,
            in_extern_c: false,
            span,
        })
    }
}
