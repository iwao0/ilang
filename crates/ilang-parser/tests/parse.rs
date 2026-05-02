use ilang_ast::{
    AttrArg, BinOp, Expr, ExprKind, Item, LogicalOp, Program, Span, Stmt, StmtKind, Type,
};
use ilang_lexer::tokenize;
use ilang_parser::{parse, parse_expr_only, ParseError};

fn parse_str(src: &str) -> Program {
    let toks = tokenize(src).unwrap();
    parse(&toks).unwrap()
}

fn parse_expr_str(src: &str) -> Expr {
    let toks = tokenize(src).unwrap();
    parse_expr_only(&toks).unwrap()
}

/// Wrap an `ExprKind` in an `Expr` with a dummy span so test fixtures can be
/// compared against parsed trees (PartialEq on `Expr` ignores spans).
fn e(kind: ExprKind) -> Expr {
    Expr::new(kind, Span::dummy())
}

fn boxed(kind: ExprKind) -> Box<Expr> {
    Box::new(e(kind))
}

#[test]
fn precedence() {
    let parsed = parse_expr_str("1 + 2 * 3");
    assert_eq!(
        parsed,
        e(ExprKind::Binary {
            op: BinOp::Add,
            lhs: boxed(ExprKind::Int(1)),
            rhs: boxed(ExprKind::Binary {
                op: BinOp::Mul,
                lhs: boxed(ExprKind::Int(2)),
                rhs: boxed(ExprKind::Int(3)),
            }),
        })
    );
}

#[test]
fn let_stmt_then_tail() {
    let p = parse_str("let x = 1 + 2; x * 3");
    assert_eq!(p.stmts.len(), 1);
    assert!(matches!(&p.stmts[0].kind, StmtKind::Let { name, .. } if name == "x"));
    assert_eq!(
        p.tail,
        Some(e(ExprKind::Binary {
            op: BinOp::Mul,
            lhs: boxed(ExprKind::Var("x".into())),
            rhs: boxed(ExprKind::Int(3)),
        }))
    );
}

#[test]
fn let_with_type() {
    let p = parse_str("let x: i64 = 7;");
    match &p.stmts[0].kind {
        StmtKind::Let { name, ty, value } => {
            assert_eq!(name, "x");
            assert_eq!(*ty, Some(Type::I64));
            assert_eq!(*value, e(ExprKind::Int(7)));
        }
        _ => panic!(),
    }
}

#[test]
fn fn_decl_basic() {
    let p = parse_str("fn add(a: i64, b: i64): i64 { a + b }");
    assert_eq!(p.items.len(), 1);
    let Item::Fn(f) = &p.items[0] else { panic!("expected fn") };
    assert_eq!(f.name, "add");
    assert_eq!(f.params.len(), 2);
    assert_eq!(f.ret, Some(Type::I64));
    assert!(f.body.tail.is_some());
}

#[test]
fn fn_call() {
    let p = parse_str("fn id(x: i64): i64 { x } id(5)");
    assert_eq!(p.items.len(), 1);
    assert_eq!(
        p.tail,
        Some(e(ExprKind::Call {
            callee: "id".into(),
            args: vec![e(ExprKind::Int(5))],
        }))
    );
}

#[test]
fn fn_with_attribute() {
    let p = parse_str("@requires(net, file.read) fn fetch(): i64 { 1 }");
    let Item::Fn(f) = &p.items[0] else { panic!("expected fn") };
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
    let parsed = parse_expr_str("1 + 2 < 3 + 4");
    assert!(matches!(
        parsed.kind,
        ExprKind::Binary { op: BinOp::Lt, .. }
    ));
}

#[test]
fn logical_short_circuit_shape() {
    let parsed = parse_expr_str("true && false || true");
    match parsed.kind {
        ExprKind::Logical {
            op: LogicalOp::Or,
            lhs,
            ..
        } => {
            assert!(matches!(
                lhs.kind,
                ExprKind::Logical {
                    op: LogicalOp::And,
                    ..
                }
            ));
        }
        _ => panic!("expected ||"),
    }
}

#[test]
fn assignment_right_assoc() {
    let p = parse_str("x = y = 1;");
    match &p.stmts[0].kind {
        StmtKind::Expr(expr) => match &expr.kind {
            ExprKind::Assign { target, value } => {
                assert_eq!(target, "x");
                assert!(matches!(
                    &value.kind,
                    ExprKind::Assign { target: t, .. } if t == "y"
                ));
            }
            _ => panic!(),
        },
        _ => panic!(),
    }
}

#[test]
fn invalid_assign_target() {
    let toks = tokenize("1 = 2;").unwrap();
    assert!(matches!(
        parse(&toks),
        Err(ParseError::InvalidAssignTarget { .. })
    ));
}

#[test]
fn if_expression_with_elif() {
    let p = parse_str("if true { 1 } elif false { 2 } else { 3 }");
    match p.tail.map(|t| t.kind) {
        Some(ExprKind::If {
            else_branch: Some(eb),
            ..
        }) => {
            assert!(matches!(eb.kind, ExprKind::If { .. }));
        }
        _ => panic!(),
    }
}

#[test]
fn while_then_more_stmts() {
    let p = parse_str("let n = 0; while false { } n");
    assert_eq!(p.stmts.len(), 2);
    assert_eq!(p.tail, Some(e(ExprKind::Var("n".into()))));
}

// Catch-all to keep the unused `Stmt` import alive; equivalent to using
// the symbol directly elsewhere.
#[allow(dead_code)]
fn _stmt_marker(_s: Stmt) {}

#[test]
fn parse_error_format_starts_with_span() {
    let toks = tokenize("let").unwrap();
    let err = parse(&toks).unwrap_err();
    let s = format!("{err}");
    assert!(s.starts_with("[1:4]:"), "got: {s}");
}
