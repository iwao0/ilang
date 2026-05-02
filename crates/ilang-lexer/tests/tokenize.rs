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
        kinds("{},;:::@[]."),
        vec![
            TokenKind::LBrace,
            TokenKind::RBrace,
            TokenKind::Comma,
            TokenKind::Semicolon,
            TokenKind::ColonColon,
            TokenKind::Colon,
            TokenKind::At,
            TokenKind::LBracket,
            TokenKind::RBracket,
            TokenKind::Dot,
            TokenKind::Eof,
        ]
    );
}

#[test]
fn class_keywords() {
    assert_eq!(
        kinds("class new this"),
        vec![TokenKind::Class, TokenKind::New, TokenKind::This, TokenKind::Eof]
    );
}

#[test]
fn bad_exponent() {
    assert!(matches!(
        tokenize("1e"),
        Err(LexError::InvalidNumber { .. })
    ));
}

#[test]
fn loop_break_continue_keywords() {
    let toks = tokenize("loop break continue").unwrap();
    let kinds: Vec<_> = toks.into_iter().map(|t| t.kind).collect();
    assert_eq!(
        kinds,
        vec![
            TokenKind::Loop,
            TokenKind::Break,
            TokenKind::Continue,
            TokenKind::Eof,
        ]
    );
}

#[test]
fn lex_error_format_starts_with_span() {
    let err = tokenize("$").unwrap_err();
    let s = format!("{err}");
    assert!(s.starts_with("[1:1]:"), "got: {s}");
}

#[test]
fn line_comment_skipped() {
    assert_eq!(
        kinds("1 // a comment\n2"),
        vec![TokenKind::Int(1), TokenKind::Int(2), TokenKind::Eof],
    );
}

#[test]
fn line_comment_at_eof() {
    assert_eq!(
        kinds("1 // trailing"),
        vec![TokenKind::Int(1), TokenKind::Eof],
    );
}

#[test]
fn line_comment_preserves_newline_for_asi() {
    // The newline after the comment must still set leading_newline on the
    // following token, otherwise `let x = 1 // foo\nlet y = 2` would parse
    // as `let x = 1 let y = 2` and fail.
    let toks = tokenize("1 // foo\n2").unwrap();
    let two = toks.iter().find(|t| matches!(t.kind, TokenKind::Int(2))).unwrap();
    assert!(two.leading_newline);
}

#[test]
fn block_comment_skipped_inline() {
    assert_eq!(
        kinds("1 /* hi */ + 2"),
        vec![TokenKind::Int(1), TokenKind::Plus, TokenKind::Int(2), TokenKind::Eof],
    );
}

#[test]
fn block_comment_nested() {
    // A `/* ... */` inside another doesn't end the outer one.
    assert_eq!(
        kinds("1 /* outer /* inner */ still */ 2"),
        vec![TokenKind::Int(1), TokenKind::Int(2), TokenKind::Eof],
    );
}

#[test]
fn block_comment_with_newline_preserves_asi() {
    // A multi-line block comment should still set leading_newline on
    // the next token so JS-style stmt termination keeps working.
    let toks = tokenize("1\n/* foo\nbar */ 2").unwrap();
    let two = toks.iter().find(|t| matches!(t.kind, TokenKind::Int(2))).unwrap();
    assert!(two.leading_newline);
}

#[test]
fn slash_division_still_works() {
    assert_eq!(
        kinds("10 / 2"),
        vec![
            TokenKind::Int(10),
            TokenKind::Slash,
            TokenKind::Int(2),
            TokenKind::Eof,
        ],
    );
}

#[test]
fn hex_literal() {
    assert_eq!(kinds("0xff"), vec![TokenKind::Int(255), TokenKind::Eof]);
    assert_eq!(kinds("0xFF"), vec![TokenKind::Int(255), TokenKind::Eof]);
    assert_eq!(kinds("0X10"), vec![TokenKind::Int(16), TokenKind::Eof]);
}

#[test]
fn binary_literal() {
    assert_eq!(kinds("0b1010"), vec![TokenKind::Int(10), TokenKind::Eof]);
    assert_eq!(kinds("0B11"), vec![TokenKind::Int(3), TokenKind::Eof]);
}

#[test]
fn underscore_separators() {
    assert_eq!(
        kinds("1_000_000"),
        vec![TokenKind::Int(1_000_000), TokenKind::Eof]
    );
    assert_eq!(
        kinds("0xff_ff"),
        vec![TokenKind::Int(0xffff), TokenKind::Eof]
    );
    assert_eq!(
        kinds("0b1010_0011"),
        vec![TokenKind::Int(0b1010_0011), TokenKind::Eof]
    );
    // Float: separators allowed in integer, fractional, and exponent parts.
    assert_eq!(
        kinds("1_2.3_4e1_0"),
        vec![TokenKind::Float(1_2.3_4e1_0), TokenKind::Eof]
    );
}

#[test]
fn empty_radix_literal_errors() {
    assert!(matches!(
        tokenize("0x"),
        Err(LexError::InvalidNumber { .. })
    ));
    assert!(matches!(
        tokenize("0b"),
        Err(LexError::InvalidNumber { .. })
    ));
}

#[test]
fn string_literal_basic() {
    assert_eq!(
        kinds(r#""hello""#),
        vec![TokenKind::Str("hello".into()), TokenKind::Eof]
    );
}

#[test]
fn string_escapes() {
    let toks = tokenize(r#""a\nb\tc\\\"end""#).unwrap();
    let kind = toks.into_iter().next().unwrap().kind;
    assert_eq!(kind, TokenKind::Str("a\nb\tc\\\"end".into()));
}

#[test]
fn unterminated_string_errors() {
    assert!(matches!(
        tokenize("\"abc"),
        Err(LexError::UnterminatedString { .. })
    ));
}

#[test]
fn bad_escape_errors() {
    assert!(matches!(
        tokenize(r#""bad\zescape""#),
        Err(LexError::BadEscape { .. })
    ));
}

#[test]
fn numeric_suffix_attached_to_token() {
    let toks = tokenize("1_i32 + 2u8").unwrap();
    assert_eq!(toks[0].kind, TokenKind::Int(1));
    assert_eq!(toks[0].numeric_suffix, Some(ilang_ast::Type::I32));
    assert_eq!(toks[2].kind, TokenKind::Int(2));
    assert_eq!(toks[2].numeric_suffix, Some(ilang_ast::Type::U8));
}

#[test]
fn unknown_suffix_does_not_consume() {
    // `1_foo` should leave `foo` as a separate identifier (the `_` is
    // eaten by the number's own separator rule, but the rest rolls back).
    let toks = tokenize("1_foo").unwrap();
    assert_eq!(toks[0].kind, TokenKind::Int(1));
    assert_eq!(toks[0].numeric_suffix, None);
    assert!(matches!(toks[1].kind, TokenKind::Ident(ref n) if n == "foo"));
}
