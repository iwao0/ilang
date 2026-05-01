use ilang_ast::{
    AttrArg, Attribute, BinOp, Block, Expr, FnDecl, Item, Param, Program, Stmt, Type, UnOp,
};
use ilang_lexer::{Token, TokenKind};
use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum ParseError {
    #[error("unexpected token {found:?} at line {line}, col {col} (expected {expected})")]
    Unexpected {
        found: TokenKind,
        expected: String,
        line: u32,
        col: u32,
    },
    #[error("unknown type {name:?} at line {line}, col {col}")]
    UnknownType { name: String, line: u32, col: u32 },
}

pub fn parse(tokens: &[Token]) -> Result<Program, ParseError> {
    let mut p = Parser { tokens, pos: 0 };
    p.parse_program()
}

/// Parse a single expression — used by tests that want to inspect expression
/// trees directly without wrapping in a program.
pub fn parse_expr_only(tokens: &[Token]) -> Result<Expr, ParseError> {
    let mut p = Parser { tokens, pos: 0 };
    let e = p.parse_expr(0)?;
    if !matches!(p.peek().kind, TokenKind::Eof) {
        let t = p.peek();
        return Err(ParseError::Unexpected {
            found: t.kind.clone(),
            expected: "end of input".into(),
            line: t.span.line,
            col: t.span.col,
        });
    }
    Ok(e)
}

struct Parser<'a> {
    tokens: &'a [Token],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> &'a Token {
        &self.tokens[self.pos]
    }

    fn bump(&mut self) -> &'a Token {
        let t = &self.tokens[self.pos];
        if !matches!(t.kind, TokenKind::Eof) {
            self.pos += 1;
        }
        t
    }

    fn expect(&mut self, expected: &TokenKind, label: &str) -> Result<(), ParseError> {
        let t = self.peek();
        if std::mem::discriminant(&t.kind) == std::mem::discriminant(expected) {
            self.bump();
            Ok(())
        } else {
            Err(ParseError::Unexpected {
                found: t.kind.clone(),
                expected: label.into(),
                line: t.span.line,
                col: t.span.col,
            })
        }
    }

    fn expect_ident(&mut self, label: &str) -> Result<String, ParseError> {
        let t = self.peek().clone();
        if let TokenKind::Ident(n) = t.kind {
            self.bump();
            Ok(n)
        } else {
            Err(ParseError::Unexpected {
                found: t.kind,
                expected: label.into(),
                line: t.span.line,
                col: t.span.col,
            })
        }
    }

    // ---------- program ----------

    fn parse_program(&mut self) -> Result<Program, ParseError> {
        let mut prog = Program::default();
        loop {
            match &self.peek().kind {
                TokenKind::Eof => break,
                TokenKind::Hash | TokenKind::Fn => {
                    let item = self.parse_item()?;
                    prog.items.push(item);
                }
                TokenKind::Let => {
                    let s = self.parse_let_stmt()?;
                    prog.stmts.push(s);
                }
                _ => {
                    let e = self.parse_expr(0)?;
                    if matches!(self.peek().kind, TokenKind::Semicolon) {
                        self.bump();
                        prog.stmts.push(Stmt::Expr(e));
                    } else {
                        // tail expression: must be at end
                        if !matches!(self.peek().kind, TokenKind::Eof) {
                            let t = self.peek();
                            return Err(ParseError::Unexpected {
                                found: t.kind.clone(),
                                expected: "';' or end of input".into(),
                                line: t.span.line,
                                col: t.span.col,
                            });
                        }
                        prog.tail = Some(e);
                        break;
                    }
                }
            }
        }
        Ok(prog)
    }

    // ---------- item / fn / attribute ----------

    fn parse_item(&mut self) -> Result<Item, ParseError> {
        let attrs = self.parse_attributes()?;
        // After attributes, only `fn` is supported in phase 2.
        match self.peek().kind {
            TokenKind::Fn => {
                let fn_decl = self.parse_fn_decl(attrs)?;
                Ok(Item::Fn(fn_decl))
            }
            _ => {
                let t = self.peek();
                Err(ParseError::Unexpected {
                    found: t.kind.clone(),
                    expected: "'fn' after attributes".into(),
                    line: t.span.line,
                    col: t.span.col,
                })
            }
        }
    }

    fn parse_attributes(&mut self) -> Result<Vec<Attribute>, ParseError> {
        let mut out = Vec::new();
        while matches!(self.peek().kind, TokenKind::Hash) {
            self.bump();
            self.expect(&TokenKind::LBracket, "'['")?;
            let name = self.expect_ident("attribute name")?;
            self.expect(&TokenKind::LParen, "'('")?;
            let mut args = Vec::new();
            if !matches!(self.peek().kind, TokenKind::RParen) {
                loop {
                    let path = self.parse_attr_path()?;
                    args.push(AttrArg::Path(path));
                    if matches!(self.peek().kind, TokenKind::Comma) {
                        self.bump();
                    } else {
                        break;
                    }
                }
            }
            self.expect(&TokenKind::RParen, "')'")?;
            self.expect(&TokenKind::RBracket, "']'")?;
            out.push(Attribute { name, args });
        }
        Ok(out)
    }

    fn parse_attr_path(&mut self) -> Result<Vec<String>, ParseError> {
        let mut parts = vec![self.expect_ident("capability name")?];
        while matches!(self.peek().kind, TokenKind::ColonColon) {
            self.bump();
            parts.push(self.expect_ident("capability segment")?);
        }
        Ok(parts)
    }

    fn parse_fn_decl(&mut self, attrs: Vec<Attribute>) -> Result<FnDecl, ParseError> {
        self.expect(&TokenKind::Fn, "'fn'")?;
        let name = self.expect_ident("function name")?;
        self.expect(&TokenKind::LParen, "'('")?;
        let mut params = Vec::new();
        if !matches!(self.peek().kind, TokenKind::RParen) {
            loop {
                let pname = self.expect_ident("parameter name")?;
                self.expect(&TokenKind::Colon, "':'")?;
                let pty = self.parse_type()?;
                params.push(Param {
                    name: pname,
                    ty: pty,
                });
                if matches!(self.peek().kind, TokenKind::Comma) {
                    self.bump();
                } else {
                    break;
                }
            }
        }
        self.expect(&TokenKind::RParen, "')'")?;
        let ret = if matches!(self.peek().kind, TokenKind::Arrow) {
            self.bump();
            Some(self.parse_type()?)
        } else {
            None
        };
        let body = self.parse_block()?;
        Ok(FnDecl {
            attrs,
            name,
            params,
            ret,
            body,
        })
    }

    fn parse_type(&mut self) -> Result<Type, ParseError> {
        let t = self.peek().clone();
        match t.kind {
            TokenKind::Ident(n) => {
                self.bump();
                match n.as_str() {
                    "i64" => Ok(Type::I64),
                    "f64" => Ok(Type::F64),
                    _ => Err(ParseError::UnknownType {
                        name: n,
                        line: t.span.line,
                        col: t.span.col,
                    }),
                }
            }
            other => Err(ParseError::Unexpected {
                found: other,
                expected: "type name".into(),
                line: t.span.line,
                col: t.span.col,
            }),
        }
    }

    fn parse_block(&mut self) -> Result<Block, ParseError> {
        self.expect(&TokenKind::LBrace, "'{'")?;
        let mut stmts = Vec::new();
        let mut tail = None;
        loop {
            match &self.peek().kind {
                TokenKind::RBrace => break,
                TokenKind::Let => {
                    let s = self.parse_let_stmt()?;
                    stmts.push(s);
                }
                _ => {
                    let e = self.parse_expr(0)?;
                    if matches!(self.peek().kind, TokenKind::Semicolon) {
                        self.bump();
                        stmts.push(Stmt::Expr(e));
                    } else {
                        tail = Some(Box::new(e));
                        break;
                    }
                }
            }
        }
        self.expect(&TokenKind::RBrace, "'}'")?;
        Ok(Block { stmts, tail })
    }

    fn parse_let_stmt(&mut self) -> Result<Stmt, ParseError> {
        self.expect(&TokenKind::Let, "'let'")?;
        let name = self.expect_ident("variable name")?;
        let ty = if matches!(self.peek().kind, TokenKind::Colon) {
            self.bump();
            Some(self.parse_type()?)
        } else {
            None
        };
        self.expect(&TokenKind::Equals, "'='")?;
        let value = self.parse_expr(0)?;
        self.expect(&TokenKind::Semicolon, "';'")?;
        Ok(Stmt::Let { name, ty, value })
    }

    // ---------- expression (Pratt) ----------

    fn parse_expr(&mut self, min_bp: u8) -> Result<Expr, ParseError> {
        let mut lhs = self.parse_prefix()?;
        loop {
            let (op, l_bp, r_bp) = match &self.peek().kind {
                TokenKind::Plus => (BinOp::Add, 10, 11),
                TokenKind::Minus => (BinOp::Sub, 10, 11),
                TokenKind::Star => (BinOp::Mul, 20, 21),
                TokenKind::Slash => (BinOp::Div, 20, 21),
                TokenKind::Percent => (BinOp::Rem, 20, 21),
                _ => break,
            };
            if l_bp < min_bp {
                break;
            }
            self.bump();
            let rhs = self.parse_expr(r_bp)?;
            lhs = Expr::Binary {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            };
        }
        Ok(lhs)
    }

    fn parse_prefix(&mut self) -> Result<Expr, ParseError> {
        let t = self.peek().clone();
        match t.kind {
            TokenKind::Int(n) => {
                self.bump();
                Ok(Expr::Int(n))
            }
            TokenKind::Float(f) => {
                self.bump();
                Ok(Expr::Float(f))
            }
            TokenKind::Ident(name) => {
                self.bump();
                if matches!(self.peek().kind, TokenKind::LParen) {
                    self.bump();
                    let mut args = Vec::new();
                    if !matches!(self.peek().kind, TokenKind::RParen) {
                        loop {
                            args.push(self.parse_expr(0)?);
                            if matches!(self.peek().kind, TokenKind::Comma) {
                                self.bump();
                            } else {
                                break;
                            }
                        }
                    }
                    self.expect(&TokenKind::RParen, "')'")?;
                    Ok(Expr::Call { callee: name, args })
                } else {
                    Ok(Expr::Var(name))
                }
            }
            TokenKind::Minus => {
                self.bump();
                let e = self.parse_expr(30)?;
                Ok(Expr::Unary {
                    op: UnOp::Neg,
                    expr: Box::new(e),
                })
            }
            TokenKind::Plus => {
                self.bump();
                let e = self.parse_expr(30)?;
                Ok(Expr::Unary {
                    op: UnOp::Pos,
                    expr: Box::new(e),
                })
            }
            TokenKind::LParen => {
                self.bump();
                let e = self.parse_expr(0)?;
                self.expect(&TokenKind::RParen, "')'")?;
                Ok(e)
            }
            TokenKind::LBrace => {
                let block = self.parse_block()?;
                Ok(Expr::Block(block))
            }
            other => Err(ParseError::Unexpected {
                found: other,
                expected: "number, identifier, '-', '+' or '('".into(),
                line: t.span.line,
                col: t.span.col,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ilang_lexer::tokenize;

    fn parse_str(src: &str) -> Program {
        let toks = tokenize(src).unwrap();
        parse(&toks).unwrap()
    }

    fn parse_expr_str(src: &str) -> Expr {
        let toks = tokenize(src).unwrap();
        parse_expr_only(&toks).unwrap()
    }

    #[test]
    fn precedence() {
        let e = parse_expr_str("1 + 2 * 3");
        assert_eq!(
            e,
            Expr::Binary {
                op: BinOp::Add,
                lhs: Box::new(Expr::Int(1)),
                rhs: Box::new(Expr::Binary {
                    op: BinOp::Mul,
                    lhs: Box::new(Expr::Int(2)),
                    rhs: Box::new(Expr::Int(3)),
                }),
            }
        );
    }

    #[test]
    fn let_stmt_then_tail() {
        let p = parse_str("let x = 1 + 2; x * 3");
        assert_eq!(p.stmts.len(), 1);
        assert!(matches!(&p.stmts[0], Stmt::Let { name, .. } if name == "x"));
        assert_eq!(
            p.tail,
            Some(Expr::Binary {
                op: BinOp::Mul,
                lhs: Box::new(Expr::Var("x".into())),
                rhs: Box::new(Expr::Int(3)),
            })
        );
    }

    #[test]
    fn let_with_type() {
        let p = parse_str("let x: i64 = 7;");
        match &p.stmts[0] {
            Stmt::Let { name, ty, value } => {
                assert_eq!(name, "x");
                assert_eq!(*ty, Some(Type::I64));
                assert_eq!(*value, Expr::Int(7));
            }
            _ => panic!(),
        }
    }

    #[test]
    fn fn_decl_basic() {
        let p = parse_str("fn add(a: i64, b: i64) -> i64 { a + b }");
        assert_eq!(p.items.len(), 1);
        let Item::Fn(f) = &p.items[0];
        assert_eq!(f.name, "add");
        assert_eq!(f.params.len(), 2);
        assert_eq!(f.ret, Some(Type::I64));
        assert!(f.body.tail.is_some());
    }

    #[test]
    fn fn_call() {
        let p = parse_str("fn id(x: i64) -> i64 { x } id(5)");
        assert_eq!(p.items.len(), 1);
        assert_eq!(
            p.tail,
            Some(Expr::Call {
                callee: "id".into(),
                args: vec![Expr::Int(5)],
            })
        );
    }

    #[test]
    fn fn_with_attribute() {
        let p = parse_str("#[requires(net, file::read)] fn fetch() -> i64 { 1 }");
        let Item::Fn(f) = &p.items[0];
        assert_eq!(f.attrs.len(), 1);
        assert_eq!(f.attrs[0].name, "requires");
        assert_eq!(
            f.attrs[0].args,
            vec![
                AttrArg::Path(vec!["net".into()]),
                AttrArg::Path(vec!["file".into(), "read".into()]),
            ]
        );
    }

    #[test]
    fn trailing_error() {
        let toks = tokenize("1 2").unwrap();
        assert!(parse(&toks).is_err());
    }
}
