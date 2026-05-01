use ilang_ast::{AttrArg, BinOp, Expr, Item, LogicalOp, Program, Stmt, Type};
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
    let p = parse_str("fn add(a: i64, b: i64): i64 { a + b }");
    assert_eq!(p.items.len(), 1);
    let Item::Fn(f) = &p.items[0];
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
        Some(Expr::Call {
            callee: "id".into(),
            args: vec![Expr::Int(5)],
        })
    );
}

#[test]
fn fn_with_attribute() {
    let p = parse_str("#[requires(net, file::read)] fn fetch(): i64 { 1 }");
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
    let e = parse_expr_str("1 + 2 < 3 + 4");
    assert!(matches!(e, Expr::Binary { op: BinOp::Lt, .. }));
}

#[test]
fn logical_short_circuit_shape() {
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
    assert!(matches!(
        parse(&toks),
        Err(ParseError::InvalidAssignTarget { .. })
    ));
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
    let p = parse_str("let n = 0; while false { } n");
    assert_eq!(p.stmts.len(), 2);
    assert_eq!(p.tail, Some(Expr::Var("n".into())));
}
