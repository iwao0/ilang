mod analyse;
mod backend;
mod builtins;
mod code_actions;
mod completion;
mod diag;
mod external;
mod formatter;
mod handlers;
mod helpers;
mod imports;
mod project;
mod symbols;
mod text;
mod text_utils;
mod types;
mod walker;

use analyse::*;
use backend::*;
use diag::*;
use external::*;
use helpers::*;
use symbols::*;
use types::*;
use walker::*;

use code_actions::{fill_match_arms_at, generate_init_at};
use completion::{
    at_attribute_position, at_type_position, attribute_completions, brace_depth_at, call_snippet,
    global_completions, in_extern_c_block, literal_token_at, preceding_kw_introduces_binder,
    push_extern_c_keywords, push_ffi_helper_completions, trigger_sig_help_command,
    type_completions,
};
use imports::organize_imports;
use text_utils::{byte_range_to_lsp_range, byte_to_position};

use std::collections::HashMap;
use std::time::Duration;

use ilang_ast::{
    Symbol as AstSymbol, UnOp,
};
use ilang_lexer::tokenize;
use ilang_parser::parse;
use ilang_types::TypeChecker;
use tower_lsp::{LspService, Server};

use builtins::{
    array_method_names, array_method_sig, ffi_helper_signature, string_method_names,
    string_method_sig,
};
use project::{collect_dep_paths, find_project_file, find_umbrella};
use text::{
    call_context_at, locate_dot_name, locate_let_name, locate_let_name_with_kw,
    locate_property_name, locate_selective_name, locate_type_after_colon,
    parameter_offsets, receiver_before_dot, span_full_to_range, span_to_range,
    word_at,
};


type ExternalSources = HashMap<AstSymbol, ExternalLoc>;


// ─── Index building ────────────────────────────────────────────────────────


// ─── Scope walker ──────────────────────────────────────────────────────────


#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let (service, socket) = LspService::new(Backend::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}

#[cfg(test)]
mod organize_imports_tests {
    use super::organize_imports;
    use ilang_lexer::tokenize;
    use ilang_parser::parse;

    pub(crate) fn run(src: &str) -> Option<String> {
        let toks = tokenize(src).ok()?;
        let prog = parse(&toks).ok()?;
        let (s, e, new) = organize_imports(src, &prog)?;
        let mut out = src.to_string();
        out.replace_range(s..e, &new);
        Some(out)
    }

    #[test]
    pub(crate) fn already_sorted_is_no_op() {
        let src = "use math\nuse test\n";
        assert!(run(src).is_none() || run(src).as_deref() == Some(src));
    }

    #[test]
    pub(crate) fn sorts_modules_alphabetically() {
        let src = "use test\nuse math\n";
        let want = "use math\nuse test\n";
        assert_eq!(run(src).unwrap(), want);
    }

    #[test]
    pub(crate) fn dedupes_whole_module() {
        let src = "use math\nuse math\nuse test\n";
        let want = "use math\nuse test\n";
        assert_eq!(run(src).unwrap(), want);
    }

    #[test]
    pub(crate) fn merges_selective_names_alphabetically() {
        let src = "use math { sin }\nuse math { cos, abs }\n";
        let want = "use math { abs, cos, sin }\n";
        assert_eq!(run(src).unwrap(), want);
    }

    #[test]
    pub(crate) fn whole_and_selective_coexist() {
        // sdl_breakout/main.il has both `use sdl` and `use sdl { ... }`.
        let src = "use sdl { InitFlag }\nuse sdl\n";
        let want = "use sdl\nuse sdl { InitFlag }\n";
        assert_eq!(run(src).unwrap(), want);
    }

    #[test]
    pub(crate) fn re_export_grouped_separately() {
        let src = "pub use beta\nuse alpha\n";
        // Non-export comes first (re_export = false sorts before true).
        let want = "use alpha\npub use beta\n";
        assert_eq!(run(src).unwrap(), want);
    }

    #[test]
    pub(crate) fn leaves_non_use_items_alone() {
        // Disordered leading uses should sort, but the trailing
        // `use later` after the `fn` stays put — only the leading
        // contiguous block is reorganised.
        let src = "use test\nuse math\nfn foo() {}\nuse later\n";
        let out = run(src).unwrap();
        assert!(
            out.starts_with("use math\nuse test\nfn foo() {}\nuse later\n"),
            "out:\n{out}"
        );
    }
}

#[cfg(test)]
mod discriminant_literal_text_tests {
    use super::*;
    use ilang_lexer::tokenize;
    use ilang_parser::parse;

    pub(crate) fn span_of_first_variant(src: &str) -> Span {
        let toks = tokenize(src).expect("lex");
        let prog = parse(&toks).expect("parse");
        for it in &prog.items {
            if let Item::Enum(e) = it {
                return e.variants[0].span;
            }
        }
        panic!("no enum");
    }

    #[test]
    pub(crate) fn integer_literal() {
        let src = "enum X: i32 { foo = 0x10 }";
        let span = span_of_first_variant(src);
        assert_eq!(discriminant_literal_text(src, span).unwrap(), "0x10");
    }

    #[test]
    pub(crate) fn integer_underscore_separator() {
        let src = "enum X: i64 { foo = 1_000 }";
        let span = span_of_first_variant(src);
        assert_eq!(discriminant_literal_text(src, span).unwrap(), "1_000");
    }

    #[test]
    pub(crate) fn negative_integer() {
        let src = "enum X: i32 { foo = -1 }";
        let span = span_of_first_variant(src);
        assert_eq!(discriminant_literal_text(src, span).unwrap(), "-1");
    }

    #[test]
    pub(crate) fn string_literal() {
        let src = "enum X: string { foo = \"SDL_HINT_AUDIO\" }";
        let span = span_of_first_variant(src);
        assert_eq!(
            discriminant_literal_text(src, span).unwrap(),
            "\"SDL_HINT_AUDIO\""
        );
    }

    #[test]
    pub(crate) fn string_literal_with_long_alignment_spaces() {
        let src = "enum X: string {\n    audioResamplingMode               = \"SDL_AUDIO_RESAMPLING_MODE\"\n}\n";
        let span = span_of_first_variant(src);
        assert_eq!(
            discriminant_literal_text(src, span).unwrap(),
            "\"SDL_AUDIO_RESAMPLING_MODE\""
        );
    }

    #[test]
    pub(crate) fn no_explicit_discriminant() {
        let src = "enum X { foo, bar }";
        let span = span_of_first_variant(src);
        assert_eq!(discriminant_literal_text(src, span), None);
    }
}

#[cfg(test)]
mod fill_match_arms_tests {
    use super::*;
    use ilang_lexer::tokenize;
    use ilang_parser::parse;

    pub(crate) fn run(src: &str, cursor: Position) -> Option<String> {
        let toks = tokenize(src).ok()?;
        let prog = parse(&toks).ok()?;
        // Re-derive var_types the way `analyse` would for `let x: Foo = ...`
        // bindings — minimal pass good enough for these unit tests.
        let mut var_types: HashMap<AstSymbol, Type> = HashMap::new();
        for it in &prog.items {
            if let Item::Fn(f) = it {
                for p in f.params.iter() {
                    var_types.insert(p.name.clone(), p.ty.clone());
                }
                collect_let_types(&f.body, &mut var_types);
            }
        }
        let (insert, new_text, _) =
            fill_match_arms_at(src, &prog, &var_types, cursor)?;
        let mut out = src.to_string();
        out.insert_str(insert, &new_text);
        Some(out)
    }

    pub(crate) fn collect_let_types(b: &Block, out: &mut HashMap<AstSymbol, Type>) {
        for s in &b.stmts {
            if let StmtKind::Let { name, ty: Some(t), .. } = &s.kind {
                out.insert(name.clone(), t.clone());
            }
        }
    }

    pub(crate) fn pos(line: u32, col: u32) -> Position {
        Position { line, character: col }
    }

    #[test]
    pub(crate) fn unit_variants_filled() {
        let src = "\
enum Color { Red, Green, Blue }
fn f(c: Color) {
    match c {
        Color.Red { 1 }
    }
}
";
        // cursor inside the match block (line 3 in 0-based).
        let out = run(src, pos(3, 8)).unwrap();
        assert!(out.contains("Color.Green { todo() }"), "out:\n{out}");
        assert!(out.contains("Color.Blue { todo() }"), "out:\n{out}");
    }

    #[test]
    pub(crate) fn wildcard_arm_skips_completion() {
        let src = "\
enum Color { Red, Green, Blue }
fn f(c: Color) {
    match c {
        Color.Red { 1 }
        _ { 0 }
    }
}
";
        assert!(run(src, pos(3, 8)).is_none());
    }

    #[test]
    pub(crate) fn fully_covered_skips_completion() {
        let src = "\
enum Color { Red, Green, Blue }
fn f(c: Color) {
    match c {
        Color.Red { 1 }
        Color.Green { 2 }
        Color.Blue { 3 }
    }
}
";
        assert!(run(src, pos(3, 8)).is_none());
    }

    #[test]
    pub(crate) fn cursor_outside_match_returns_none() {
        let src = "\
enum Color { Red, Green, Blue }
fn f(c: Color) {
    match c {
        Color.Red { 1 }
    }
}
";
        // line 0 is the enum decl — outside any match.
        assert!(run(src, pos(0, 5)).is_none());
    }

    #[test]
    pub(crate) fn tuple_variant_uses_underscore_placeholders() {
        let src = "\
enum Shape { Circle: (f64), Square: (f64, f64) }
fn area(s: Shape): f64 {
    match s {
        Shape.Circle(r) { r }
    }
}
";
        let out = run(src, pos(3, 8)).unwrap();
        assert!(
            out.contains("Shape.Square(_, _) { todo() }"),
            "out:\n{out}"
        );
    }

    pub(crate) fn run_init(src: &str, cursor: Position) -> Option<String> {
        let toks = ilang_lexer::tokenize(src).ok()?;
        let prog = ilang_parser::parse(&toks).ok()?;
        let (insert, new_text) = generate_init_at(src, &prog, cursor)?;
        let mut out = src.to_string();
        out.insert_str(insert, &new_text);
        Some(out)
    }

    #[test]
    pub(crate) fn init_generated_from_fields() {
        let src = "\
class Point {
    x: f64
    y: f64
}
";
        // cursor inside class body (line 1, anywhere).
        let out = run_init(src, pos(1, 4)).unwrap();
        assert!(out.contains("init(x: f64, y: f64) {"), "out:\n{out}");
        assert!(out.contains("this.x = x"), "out:\n{out}");
        assert!(out.contains("this.y = y"), "out:\n{out}");
    }

    #[test]
    pub(crate) fn init_skipped_when_already_defined() {
        let src = "\
class Point {
    x: f64
    init(x: f64) { this.x = x }
}
";
        assert!(run_init(src, pos(1, 4)).is_none());
    }

    #[test]
    pub(crate) fn init_skipped_for_empty_class() {
        let src = "\
class Empty {
}
";
        assert!(run_init(src, pos(1, 0)).is_none());
    }

    #[test]
    pub(crate) fn init_outside_class_returns_none() {
        let src = "\
class Point {
    x: f64
}
fn f() {}
";
        // cursor is on the `fn` line — outside the class.
        assert!(run_init(src, pos(3, 0)).is_none());
    }

    #[test]
    pub(crate) fn init_renders_complex_field_types() {
        let src = "\
class Bag {
    items: i32[]
    label: string
    count: i64
}
";
        let out = run_init(src, pos(1, 4)).unwrap();
        assert!(
            out.contains("init(items: i32[], label: string, count: i64)"),
            "out:\n{out}"
        );
    }

    #[test]
    pub(crate) fn struct_variant_emits_field_names() {
        let src = "\
enum Msg { Ping, Move: { x: f64, y: f64 } }
fn h(m: Msg) {
    match m {
        Msg.Ping { 1 }
    }
}
";
        let out = run(src, pos(3, 8)).unwrap();
        assert!(
            out.contains("Msg.Move { x, y } { todo() }"),
            "out:\n{out}"
        );
    }
}
