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
fn octal_literal() {
    assert_eq!(kinds("0o17"), vec![TokenKind::Int(15), TokenKind::Eof]);
    assert_eq!(kinds("0O755"), vec![TokenKind::Int(0o755), TokenKind::Eof]);
    assert_eq!(
        kinds("0o1_234"),
        vec![TokenKind::Int(0o1234), TokenKind::Eof]
    );
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
    assert!(matches!(
        tokenize("0o"),
        Err(LexError::InvalidNumber { .. })
    ));
    // 0o8 / 0o9 are not valid octal digits.
    assert!(matches!(
        tokenize("0o9"),
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
    // `_` only acts as a separator strictly between digits, so the `_`
    // in `1_foo` is left for the identifier rather than being silently
    // swallowed (which would let `_` substitute for whitespace).
    let toks = tokenize("1_foo").unwrap();
    assert_eq!(toks[0].kind, TokenKind::Int(1));
    assert_eq!(toks[0].numeric_suffix, None);
    assert!(matches!(toks[1].kind, TokenKind::Ident(ref n) if n == "_foo"));
}

#[test]
fn underscore_only_between_digits() {
    // `1__2` should not collapse to `Int(12)` — the second `_` isn't
    // followed by a digit, so the first `_` doesn't see a digit on its
    // right either and the run stops at `1`.
    let toks = tokenize("1__2").unwrap();
    assert_eq!(toks[0].kind, TokenKind::Int(1));
    assert!(matches!(toks[1].kind, TokenKind::Ident(ref n) if n == "__2"));
}

#[test]
fn unterminated_block_comment_errors() {
    assert!(matches!(
        tokenize("/* unterminated"),
        Err(LexError::UnterminatedBlockComment { .. })
    ));
    // Nested: outer never closes.
    assert!(matches!(
        tokenize("/* outer /* inner */ "),
        Err(LexError::UnterminatedBlockComment { .. })
    ));
}

#[test]
fn u64_max_decimal_literal() {
    // 18446744073709551615 is u64::MAX — outside i64 but inside u64.
    // It should lex via the u64 fallback (reinterpreted as i64).
    let toks = tokenize("18446744073709551615_u64").unwrap();
    assert_eq!(toks[0].kind, TokenKind::Int(-1));
    assert_eq!(toks[0].numeric_suffix, Some(ilang_ast::Type::U64));
}

#[test]
fn leading_dot_float() {
    assert_eq!(kinds(".5"), vec![TokenKind::Float(0.5), TokenKind::Eof]);
    assert_eq!(
        kinds(".25e2"),
        vec![TokenKind::Float(25.0), TokenKind::Eof]
    );
    // `.foo` is still field access, not a float.
    assert_eq!(
        kinds(".foo"),
        vec![TokenKind::Dot, TokenKind::Ident("foo".into()), TokenKind::Eof]
    );
    // `..` and `..=` keep their range meaning.
    assert!(matches!(kinds("..").as_slice(), [TokenKind::DotDot, TokenKind::Eof]));
}

#[test]
fn invalid_numeric_suffix_errors() {
    // `1foo` looks like a typo'd suffix — surface as error rather than
    // silently splitting into `Int(1)` + `Ident("foo")`.
    assert!(matches!(
        tokenize("1foo"),
        Err(LexError::InvalidNumericSuffix { ref name, .. }) if name == "foo"
    ));
    assert!(matches!(
        tokenize("1u8x"),
        Err(LexError::InvalidNumericSuffix { ref name, .. }) if name == "u8x"
    ));
}

#[test]
fn bom_is_skipped() {
    let toks = tokenize("\u{FEFF}let x = 1").unwrap();
    assert!(matches!(toks[0].kind, TokenKind::Let));
}

#[test]
fn extended_string_escapes() {
    let toks = tokenize(r#""\a\b\f\v""#).unwrap();
    assert_eq!(toks[0].kind, TokenKind::Str("\x07\x08\x0c\x0b".into()));

    let toks = tokenize(r#""\x41\x7F""#).unwrap();
    assert_eq!(toks[0].kind, TokenKind::Str("A\x7F".into()));

    let toks = tokenize(r#""\u{1F600}""#).unwrap();
    assert_eq!(toks[0].kind, TokenKind::Str("\u{1F600}".into()));
}

#[test]
fn bad_extended_escapes_error() {
    // \x exceeds 0x7F
    assert!(matches!(tokenize(r#""\x80""#), Err(LexError::BadEscape { .. })));
    // \x with non-hex
    assert!(matches!(tokenize(r#""\xZZ""#), Err(LexError::BadEscape { .. })));
    // \u missing braces
    assert!(matches!(tokenize(r#""\u41""#), Err(LexError::BadEscape { .. })));
    // \u with surrogate
    assert!(matches!(tokenize(r#""\u{D800}""#), Err(LexError::BadEscape { .. })));
    // \u{} empty
    assert!(matches!(tokenize(r#""\u{}""#), Err(LexError::BadEscape { .. })));
}

#[test]
fn lone_cr_triggers_asi() {
    // A single `\r` (old-Mac line ending) should mark the next token as
    // following a newline, just like `\n` and `\r\n`.
    let toks = tokenize("1\r2").unwrap();
    let two = toks.iter().find(|t| matches!(t.kind, TokenKind::Int(2))).unwrap();
    assert!(two.leading_newline);
    // `\r\n` shouldn't double-count: line should be 2, not 3.
    let toks = tokenize("a\r\nb").unwrap();
    let b = toks.iter().find(|t| matches!(&t.kind, TokenKind::Ident(n) if n == "b")).unwrap();
    assert_eq!(b.span.line, 2);
}

#[test]
fn unicode_identifiers() {
    let toks = tokenize("let 名前 = 1").unwrap();
    assert!(matches!(toks[0].kind, TokenKind::Let));
    assert!(matches!(&toks[1].kind, TokenKind::Ident(n) if n == "名前"));
    // Continue chars include Unicode digits + alphabetics.
    let toks = tokenize("café_2").unwrap();
    assert!(matches!(&toks[0].kind, TokenKind::Ident(n) if n == "café_2"));
}

#[test]
fn integer_with_float_suffix_promotes_to_float() {
    // `1_f32` is shorthand for `1.0_f32` — token kind reflects that.
    let toks = tokenize("1_f32").unwrap();
    assert_eq!(toks[0].kind, TokenKind::Float(1.0));
    assert_eq!(toks[0].numeric_suffix, Some(ilang_ast::Type::F32));

    let toks = tokenize("42f64").unwrap();
    assert_eq!(toks[0].kind, TokenKind::Float(42.0));
    assert_eq!(toks[0].numeric_suffix, Some(ilang_ast::Type::F64));
}
