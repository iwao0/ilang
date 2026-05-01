use ilang_ast::{
    AttrArg, Attribute, BinOp, Block, Expr, FnDecl, Item, LogicalOp, Param, Program, Stmt, Type,
    UnOp,
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
    #[error("invalid assignment target at line {line}, col {col}")]
    InvalidAssignTarget { line: u32, col: u32 },
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

enum ExprEnd {
    Stmt,
    Tail,
}

fn is_block_like(e: &Expr) -> bool {
    matches!(e, Expr::Block(_) | Expr::If { .. } | Expr::While { .. })
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
                    match self.classify_expr_end(&e, TokenKind::Eof)? {
                        ExprEnd::Stmt => prog.stmts.push(Stmt::Expr(e)),
                        ExprEnd::Tail => {
                            prog.tail = Some(e);
                            break;
                        }
                    }
                }
            }
        }
        Ok(prog)
    }

    /// After parsing an expression in a statement-position, decide whether it
    /// becomes a statement (followed by `;`, or block-like and more tokens
    /// follow) or the trailing expression (at end of program/block).
    fn classify_expr_end(
        &mut self,
        expr: &Expr,
        end: TokenKind,
    ) -> Result<ExprEnd, ParseError> {
        if matches!(self.peek().kind, TokenKind::Semicolon) {
            self.bump();
            return Ok(ExprEnd::Stmt);
        }
        if std::mem::discriminant(&self.peek().kind) == std::mem::discriminant(&end) {
            return Ok(ExprEnd::Tail);
        }
        if is_block_like(expr) {
            return Ok(ExprEnd::Stmt);
        }
        let t = self.peek();
        Err(ParseError::Unexpected {
            found: t.kind.clone(),
            expected: "';' or end of block".into(),
            line: t.span.line,
            col: t.span.col,
        })
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
                    "bool" => Ok(Type::Bool),
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
                    match self.classify_expr_end(&e, TokenKind::RBrace)? {
                        ExprEnd::Stmt => stmts.push(Stmt::Expr(e)),
                        ExprEnd::Tail => {
                            tail = Some(Box::new(e));
                            break;
                        }
                    }
                }
            }
        }
        self.expect(&TokenKind::RBrace, "'}'")?;
        Ok(Block { stmts, tail })
    }

    fn parse_if(&mut self) -> Result<Expr, ParseError> {
        self.expect(&TokenKind::If, "'if'")?;
        let cond = self.parse_expr(0)?;
        let then_branch = self.parse_block()?;
        let else_branch = if matches!(self.peek().kind, TokenKind::Else) {
            self.bump();
            // `else if` chains: parse another If expression directly so the
            // structure stays an If with an Else branch that is itself an If.
            if matches!(self.peek().kind, TokenKind::If) {
                let inner = self.parse_if()?;
                Some(Box::new(inner))
            } else {
                let block = self.parse_block()?;
                Some(Box::new(Expr::Block(block)))
            }
        } else {
            None
        };
        Ok(Expr::If {
            cond: Box::new(cond),
            then_branch,
            else_branch,
        })
    }

    fn parse_while(&mut self) -> Result<Expr, ParseError> {
        self.expect(&TokenKind::While, "'while'")?;
        let cond = self.parse_expr(0)?;
        let body = self.parse_block()?;
        Ok(Expr::While {
            cond: Box::new(cond),
            body,
        })
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
    //
    // Precedence (low → high):
    //   `=`              : 2 / 1 (right-assoc, lhs must be a Var)
    //   `||`             : 3 / 4
    //   `&&`             : 5 / 6
    //   `==` `!=`        : 7 / 8
    //   `<` `<=` `>` `>=`: 9 / 10
    //   `+` `-`          : 10 / 11
    //   `*` `/` `%`      : 20 / 21
    //   prefix unary     : 30
    //
    // The 9/10 and 10/11 split is intentional: `1 + 2 < 3` parses as
    // `(1 + 2) < 3` because `+`'s l_bp (10) ≥ `<`'s r_bp (10), letting `+`
    // bind on the rhs of `<`, while `<` itself doesn't bind tighter than `+`
    // because its l_bp (9) < `+`'s r_bp (11).

    fn parse_expr(&mut self, min_bp: u8) -> Result<Expr, ParseError> {
        let mut lhs = self.parse_prefix()?;
        loop {
            // Assignment is right-associative: lhs must be a Var.
            if matches!(self.peek().kind, TokenKind::Equals) {
                let l_bp = 2u8;
                let r_bp = 1u8;
                if l_bp < min_bp {
                    break;
                }
                let eq_tok = self.peek().clone();
                self.bump();
                let value = self.parse_expr(r_bp)?;
                let target = match lhs {
                    Expr::Var(name) => name,
                    _ => {
                        return Err(ParseError::InvalidAssignTarget {
                            line: eq_tok.span.line,
                            col: eq_tok.span.col,
                        });
                    }
                };
                lhs = Expr::Assign {
                    target,
                    value: Box::new(value),
                };
                continue;
            }

            // Short-circuit logical operators.
            if let Some((logop, l_bp, r_bp)) = match self.peek().kind {
                TokenKind::PipePipe => Some((LogicalOp::Or, 3u8, 4u8)),
                TokenKind::AmpAmp => Some((LogicalOp::And, 5u8, 6u8)),
                _ => None,
            } {
                if l_bp < min_bp {
                    break;
                }
                self.bump();
                let rhs = self.parse_expr(r_bp)?;
                lhs = Expr::Logical {
                    op: logop,
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                };
                continue;
            }

            // Regular binary operators.
            let (op, l_bp, r_bp) = match &self.peek().kind {
                TokenKind::EqEq => (BinOp::Eq, 7, 8),
                TokenKind::BangEq => (BinOp::Ne, 7, 8),
                TokenKind::Lt => (BinOp::Lt, 9, 10),
                TokenKind::LtEq => (BinOp::Le, 9, 10),
                TokenKind::Gt => (BinOp::Gt, 9, 10),
                TokenKind::GtEq => (BinOp::Ge, 9, 10),
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
            TokenKind::True => {
                self.bump();
                Ok(Expr::Bool(true))
            }
            TokenKind::False => {
                self.bump();
                Ok(Expr::Bool(false))
            }
            TokenKind::If => self.parse_if(),
            TokenKind::While => self.parse_while(),
            TokenKind::Bang => {
                self.bump();
                let e = self.parse_expr(30)?;
                Ok(Expr::Unary {
                    op: UnOp::Not,
                    expr: Box::new(e),
                })
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

    #[test]
    fn comparison_precedence() {
        // 1 + 2 < 3 + 4   →   (1+2) < (3+4)
        let e = parse_expr_str("1 + 2 < 3 + 4");
        assert!(matches!(
            e,
            Expr::Binary { op: BinOp::Lt, .. }
        ));
    }

    #[test]
    fn logical_short_circuit_shape() {
        // a && b || c   →   (a && b) || c
        let e = parse_expr_str("true && false || true");
        match e {
            Expr::Logical { op: LogicalOp::Or, lhs, .. } => {
                assert!(matches!(*lhs, Expr::Logical { op: LogicalOp::And, .. }));
            }
            _ => panic!("expected ||"),
        }
    }

    #[test]
    fn assignment_right_assoc() {
        // x = y = 1   →   x = (y = 1)
        let p = parse_str("x = y = 1;");
        match &p.stmts[0] {
            Stmt::Expr(Expr::Assign { target, value }) => {
                assert_eq!(target, "x");
                assert!(matches!(value.as_ref(), Expr::Assign { target: t, .. } if t == "y"));
            }
            _ => panic!(),
        }
    }

    #[test]
    fn invalid_assign_target() {
        let toks = tokenize("1 = 2;").unwrap();
        assert!(matches!(parse(&toks), Err(ParseError::InvalidAssignTarget { .. })));
    }

    #[test]
    fn if_expression_with_else_if() {
        let p = parse_str("if true { 1 } else if false { 2 } else { 3 }");
        match p.tail {
            Some(Expr::If { else_branch: Some(eb), .. }) => {
                assert!(matches!(*eb, Expr::If { .. }));
            }
            _ => panic!(),
        }
    }

    #[test]
    fn while_then_more_stmts() {
        // while loop without ; followed by an expression
        let p = parse_str("let n = 0; while false { } n");
        assert_eq!(p.stmts.len(), 2);
        assert_eq!(p.tail, Some(Expr::Var("n".into())));
    }
}
