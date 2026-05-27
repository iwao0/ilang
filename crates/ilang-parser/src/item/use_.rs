//! `use module` (whole-module import) /
//! `use module { name1, name2, ... }` (selective) /
//! `use a.b.c.*` (wildcard) parsing. Visibility-toggling
//! `pub use M` is handled one level up in `parse_item` so it can
//! short-circuit before reaching this function.

use ilang_lexer::TokenKind;

use crate::error::ParseError;
use crate::parser::Parser;

impl<'a> Parser<'a> {
    /// `use module` (whole-module import) or
    /// `use module { name1, name2, ... }` (selective).
    pub(in crate::item) fn parse_use_decl(&mut self) -> Result<ilang_ast::UseDecl, ParseError> {
        let span = self.peek().span;
        self.expect(&TokenKind::Use, "'use'")?;
        // `use super.<...>` — count consecutive `super.` prefixes
        // so the loader can walk that many edges up the dep tree
        // when resolving the eventual module name.
        let mut super_count: u32 = 0;
        while matches!(self.peek().kind, TokenKind::Super) {
            self.bump();
            self.expect(&TokenKind::Dot, "'.' after `super`")?;
            super_count += 1;
        }
        let module = self.expect_ident("module name")?;
        // `use M.*` / `use M.Name` — short forms for
        // `use M as _ { * }` / `use M as _ { Name }`. The loop
        // accumulates intermediate `.Ident` segments into `subpath`
        // so `use M.a.b.*` / `use M.a.b.Name` walk `M`'s `a/b/`
        // subdirectories before applying the wildcard / selective
        // terminator. `use M.Name` (single dot) keeps the old
        // selective shorthand — `subpath` ends up empty.
        let mut subpath: Vec<ilang_ast::Symbol> = Vec::new();
        if matches!(self.peek().kind, TokenKind::Dot) {
            loop {
                self.bump(); // consume the Dot
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
                            super_count,
                            subpath: subpath.into_boxed_slice(),
                            span,
                        });
                    }
                    TokenKind::Ident(name) => {
                        self.bump();
                        let sym = ilang_ast::Symbol::intern(&name);
                        match self.peek().kind {
                            TokenKind::Dot => {
                                // Intermediate segment — keep walking.
                                subpath.push(sym);
                                continue;
                            }
                            TokenKind::LBrace => {
                                // `use a.b.c { X, Y }` — `c.il` is
                                // the deepest file; the braces are
                                // the selective list. Push `c` to
                                // subpath and fall through to the
                                // long-form `{...}` parser below.
                                subpath.push(sym);
                                break;
                            }
                            TokenKind::As => {
                                // `use a.b.c as alias` — push `c` onto
                                // subpath and fall through to the
                                // common `as <ident>` parsing below
                                // so the user-supplied alias wins
                                // over the implicit leaf-name default.
                                subpath.push(sym);
                                break;
                            }
                            _ => {
                                // Bare `use a.b.c` path-style import:
                                // load the deepest file (`c.il`) and
                                // bind the user-facing namespace to
                                // the full dotted path (`a.b.c`).
                                // Callers reach the items as
                                // `a.b.c.X`, mirroring what they
                                // wrote in the `use` declaration —
                                // the loader merges items under that
                                // same dotted prefix so the leaf-only
                                // form (`c.X`) no longer accidentally
                                // works. Aliasing (`use a.b.c as m`)
                                // is the only way to expose a
                                // shorter namespace.
                                subpath.push(sym);
                                return Ok(ilang_ast::UseDecl {
                                    module,
                                    alias: ilang_ast::UseAlias::Default,
                                    selective: None,
                                    wildcard: false,
                                    re_export: false,
                                    super_count,
                                    subpath: subpath.into_boxed_slice(),
                                    span,
                                });
                            }
                        }
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
            super_count,
            subpath: subpath.into_boxed_slice(),
            span,
        })
    }
}
