use ilang_lexer::{tokenize, LexError, TokenKind};

fn kinds(src: &str) -> Vec<TokenKind> {
    tokenize(src).unwrap().into_iter().map(|t| t.kind).collect()
}

#[test]
fn simple_arithmetic() {
    assert_eq!(
        kinds("1 + 2.5"),
        vec![
            TokenKind::Int(1),
            TokenKind::Plus,
            TokenKind::Float(2.5),
            TokenKind::Eof,
        ]
    );
}

#[test]
fn all_operators() {
    assert_eq!(
        kinds("+-*/%()"),
        vec![
            TokenKind::Plus,
            TokenKind::Minus,
            TokenKind::Star,
            TokenKind::Slash,
            TokenKind::Percent,
            TokenKind::LParen,
            TokenKind::RParen,
            TokenKind::Eof,
        ]
    );
}

#[test]
fn float_with_exponent() {
    assert_eq!(
        kinds("2.5e-3"),
        vec![TokenKind::Float(2.5e-3), TokenKind::Eof]
    );
    assert_eq!(kinds("1E2"), vec![TokenKind::Float(100.0), TokenKind::Eof]);
}

#[test]
fn unexpected_char() {
    assert!(matches!(
        tokenize("1 $ 2"),
        Err(LexError::UnexpectedChar { ch: '$', .. })
    ));
}

#[test]
fn control_flow_tokens() {
    assert_eq!(
        kinds("if else while true false == != <= >= < > && || !"),
        vec![
            TokenKind::If,
            TokenKind::Else,
            TokenKind::While,
            TokenKind::True,
            TokenKind::False,
            TokenKind::EqEq,
            TokenKind::BangEq,
            TokenKind::LtEq,
            TokenKind::GtEq,
            TokenKind::Lt,
            TokenKind::Gt,
            TokenKind::AmpAmp,
            TokenKind::PipePipe,
            TokenKind::Bang,
            TokenKind::Eof,
        ]
    );
}

#[test]
fn keywords_and_ident() {
    assert_eq!(
        kinds("let fn x_1 i64"),
        vec![
            TokenKind::Let,
            TokenKind::Fn,
            TokenKind::Ident("x_1".into()),
            TokenKind::Ident("i64".into()),
            TokenKind::Eof,
        ]
    );
}

#[test]
fn punctuation() {
    assert_eq!(
        kinds("{},;:::->#[]"),
        vec![
            TokenKind::LBrace,
            TokenKind::RBrace,
            TokenKind::Comma,
            TokenKind::Semicolon,
            TokenKind::ColonColon,
            TokenKind::Colon,
            TokenKind::Arrow,
            TokenKind::Hash,
            TokenKind::LBracket,
            TokenKind::RBracket,
            TokenKind::Eof,
        ]
    );
}

#[test]
fn bad_exponent() {
    assert!(matches!(
        tokenize("1e"),
        Err(LexError::InvalidNumber { .. })
    ));
}
