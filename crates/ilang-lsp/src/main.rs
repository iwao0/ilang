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

use code_actions::{
    fill_match_arms_at, generate_init_at, implement_interface_methods_at,
    interface_method_stub_completions_textual,
};
#[cfg(test)]
use code_actions::interface_method_stub_completions_at;
use completion::{
    at_attribute_position, at_type_position, attribute_completions, brace_depth_at, call_snippet,
    enclosing_class, enclosing_use_module, global_completions, in_extern_c_block,
    literal_token_at, preceding_kw_introduces_binder, push_extern_c_keywords,
    push_ffi_helper_completions, trigger_sig_help_command, type_completions,
};
use imports::organize_imports;
use text_utils::{byte_range_to_lsp_range, byte_to_position};

use std::collections::HashMap;
use std::time::Duration;

use ilang_ast::{Symbol as AstSymbol, UnOp};
use ilang_lexer::tokenize;
use ilang_parser::parse;
use ilang_types::TypeChecker;
use tower_lsp::{LspService, Server};
#[cfg(test)]
use ilang_ast::{Block, Item, Span, StmtKind, Type};
#[cfg(test)]
use tower_lsp::lsp_types::Position;

use builtins::{
    array_method_doc, array_method_names, array_method_sig, ffi_helper_signature,
    map_method_doc, map_method_names, map_method_sig, string_method_doc, string_method_names,
    string_method_sig,
};
use project::{collect_dep_paths, find_project_file, find_umbrella};
use text::{
    call_context_at, generic_args_context_at, locate_class_base_name, locate_dot_name,
    locate_if_let_some_name, locate_let_name, locate_let_name_with_kw, locate_property_name,
    locate_selective_name, locate_type_after_colon, parameter_offsets, receiver_before_dot,
    span_full_to_range, span_to_range, word_at,
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

    pub(crate) fn run_iface(src: &str, cursor: Position) -> Option<String> {
        let toks = ilang_lexer::tokenize(src).ok()?;
        let prog = ilang_parser::parse(&toks).ok()?;
        let empty: std::collections::HashMap<AstSymbol, ilang_ast::InterfaceDecl> =
            std::collections::HashMap::new();
        let (insert, new_text, _) =
            implement_interface_methods_at(src, &prog, &empty, cursor)?;
        let mut out = src.to_string();
        out.insert_str(insert, &new_text);
        Some(out)
    }

    #[test]
    pub(crate) fn implement_stubs_inserted_for_missing_methods() {
        let src = "\
@extern(ObjC) {
    @objc interface Greeter {
        hello(name: i64)
        goodbye?(name: i64): bool
    }
}
class Eager : Greeter {
    pub init() {}
}
";
        // cursor inside the class body
        let out = run_iface(src, pos(7, 4)).unwrap();
        assert!(out.contains("pub hello(name: i64) {"), "out:\n{out}");
        assert!(out.contains("pub goodbye(name: i64): bool {"), "out:\n{out}");
        assert!(out.contains("optional"), "out:\n{out}");
        assert!(out.contains("false"), "out:\n{out}");
    }

    #[test]
    pub(crate) fn implement_skips_existing_methods() {
        let src = "\
interface Greet {
    hi(): string
    bye(): string
}
class C : Greet {
    pub init() {}
    pub hi(): string { \"hi\" }
}
";
        // cursor inside class body
        let out = run_iface(src, pos(5, 4)).unwrap();
        // `hi` already implemented — only `bye` should be added.
        assert!(out.contains("pub bye(): string {"), "out:\n{out}");
        // Should NOT add a second `pub hi`.
        assert_eq!(out.matches("pub hi(").count(), 1, "out:\n{out}");
    }

    #[test]
    pub(crate) fn implement_returns_none_when_all_methods_present() {
        let src = "\
interface I {
    f()
}
class D : I {
    pub init() {}
    pub f() {}
}
";
        assert!(run_iface(src, pos(4, 4)).is_none());
    }

    #[test]
    pub(crate) fn implement_returns_none_outside_class() {
        let src = "\
interface I { f() }
class D : I { pub init() {} pub f() {} }
fn outside() {}
";
        // cursor on the outside fn
        assert!(run_iface(src, pos(2, 0)).is_none());
    }

    #[test]
    pub(crate) fn at_type_position_recognises_class_base_list() {
        // `class C : ` — cursor right after the `:` (with trailing
        // space) should be a type position.
        let src = "class C : ";
        assert!(at_type_position(src, src.len()));

        // `class C : A, ` — cursor right after the comma + space
        // should also be a type position (additional interfaces).
        let src = "class C : A, ";
        assert!(at_type_position(src, src.len()));

        // `foo(a, ` — regular call argument list, NOT a type position.
        let src = "foo(a, ";
        assert!(!at_type_position(src, src.len()));

        // `(a, ` — bare tuple, NOT a type position.
        let src = "let t = (a, ";
        assert!(!at_type_position(src, src.len()));

        // `let a: Map<` — first generic argument slot is a type position.
        let src = "let a: Map<";
        assert!(at_type_position(src, src.len()));

        // `let a: Map<K, ` — subsequent generic argument slot is a type
        // position.
        let src = "let a: Map<K, ";
        assert!(at_type_position(src, src.len()));

        // `new Map<` — generic args inside a constructor are types.
        let src = "let a = new Map<";
        assert!(at_type_position(src, src.len()));

        // `new Map<i32, ` — subsequent constructor generic slot.
        let src = "let a = new Map<i32, ";
        assert!(at_type_position(src, src.len()));
    }

    #[test]
    pub(crate) fn synthesized_objc_helpers_excluded_from_symbols_and_completion() {
        // Source with one user @objc class triggers the desugar's
        // sel-cache helper class; `collect_symbols` should not
        // record it.
        let src = "\
@extern(ObjC) {
    @objc pub class NSObject {
        @objc(\"release\") release()
    }
    @objc pub class MyView : NSObject {
        @objc(\"alloc\") pub static alloc(): MyView
    }
}
";
        let toks = ilang_lexer::tokenize(src).unwrap();
        let prog = ilang_parser::parse(&toks).unwrap();
        let syms = collect_symbols(&prog, src);
        for key in syms.keys() {
            assert!(
                !key.as_str().contains("_sel_cache"),
                "synth helper leaked: {}",
                key.as_str()
            );
            assert!(
                !key.as_str().starts_with("__objc_"),
                "synth helper leaked: {}",
                key.as_str()
            );
        }
        // User-declared classes should still be present.
        assert!(syms.contains_key(&AstSymbol::intern("MyView")));
        assert!(syms.contains_key(&AstSymbol::intern("NSObject")));
    }

    #[test]
    pub(crate) fn interface_method_completion_emits_snippets() {
        // Cursor inside a class body that lists an interface as
        // its base — completion should produce one entry per
        // unimplemented interface method, with a snippet that
        // inserts the full signature + body.
        let src = "\
interface Greeter {
    hello(name: i64): bool
    bye()
}
class C : Greeter {
    pub init() {}

}
";
        let toks = ilang_lexer::tokenize(src).unwrap();
        let prog = ilang_parser::parse(&toks).unwrap();
        let empty: std::collections::HashMap<AstSymbol, ilang_ast::InterfaceDecl> =
            std::collections::HashMap::new();
        // Cursor inside the class body, just past `pub init() {}`
        // on the next line.
        let stubs =
            interface_method_stub_completions_at(src, &prog, &empty, pos(5, 18));
        // Two missing methods: `hello` and `bye`. `init` already
        // exists.
        assert_eq!(stubs.len(), 2, "stubs: {:?}", stubs);
        let labels: Vec<&str> = stubs.iter().map(|(l, _, _)| l.as_str()).collect();
        assert!(labels.contains(&"hello"), "labels: {labels:?}");
        assert!(labels.contains(&"bye"), "labels: {labels:?}");
        // Snippet for `hello(name: i64): bool` should include the
        // signature + a default `false` for the bool return.
        let (_, _, hello_snippet) = stubs.iter().find(|(l, _, _)| l == "hello").unwrap();
        assert!(hello_snippet.contains("pub hello(name: i64): bool"), "{}", hello_snippet);
        assert!(hello_snippet.contains("false"), "{}", hello_snippet);
    }

    #[test]
    pub(crate) fn interface_method_completion_textual_works_mid_edit() {
        // The user is mid-typing inside a class body whose buffer
        // doesn't yet parse cleanly. The text-based completion
        // path should still surface the missing-method stubs.
        let src = "\
interface Greeter {
    hello(name: i64): bool
    bye()
}
class C : Greeter {
    pub init() {}
    he
}
";
        let ext_src = "\
interface Greeter {
    hello(name: i64): bool
    bye()
}
";
        // Build local_interfaces map from a parse of the interface
        // declaration (the LSP would normally have this populated
        // from the last successful parse of the same buffer).
        let ext_toks = ilang_lexer::tokenize(ext_src).unwrap();
        let ext_prog = ilang_parser::parse(&ext_toks).unwrap();
        let mut locals: std::collections::HashMap<AstSymbol, ilang_ast::InterfaceDecl> =
            std::collections::HashMap::new();
        for it in &ext_prog.items {
            if let ilang_ast::Item::Interface(i) = it {
                locals.insert(i.name, i.clone());
            }
        }
        let empty: std::collections::HashMap<AstSymbol, ilang_ast::InterfaceDecl> =
            std::collections::HashMap::new();
        // Cursor at line 6 col 6, right after `he` partial ident.
        let off =
            crate::text::line_col_to_offset(src, 7, 7).expect("offset");
        let stubs =
            interface_method_stub_completions_textual(src, off, &locals, &empty);
        assert_eq!(stubs.len(), 2, "stubs: {:?}", stubs);
        let labels: Vec<&str> = stubs.iter().map(|(l, _, _)| l.as_str()).collect();
        assert!(labels.contains(&"hello"));
        assert!(labels.contains(&"bye"));
    }

    #[test]
    pub(crate) fn implement_works_for_inline_empty_class_body() {
        // `class MyApp : NSApplicationDelegate {}` on a single
        // line with empty body — same shape the user reports.
        let local_src = "class MyApp : MyDel {}\n";
        let toks = ilang_lexer::tokenize(local_src).unwrap();
        let local_prog = ilang_parser::parse(&toks).unwrap();

        let ext_src = "\
interface MyDel {
    notifyMe(name: i64)
}
";
        let ext_toks = ilang_lexer::tokenize(ext_src).unwrap();
        let ext_prog = ilang_parser::parse(&ext_toks).unwrap();
        let mut ext_ifaces: std::collections::HashMap<
            AstSymbol,
            ilang_ast::InterfaceDecl,
        > = std::collections::HashMap::new();
        for it in &ext_prog.items {
            if let ilang_ast::Item::Interface(i) = it {
                ext_ifaces.insert(i.name, i.clone());
            }
        }
        // Cursor between `{` and `}` on line 0 (column ~21).
        let res = implement_interface_methods_at(
            local_src,
            &local_prog,
            &ext_ifaces,
            pos(0, 21),
        );
        assert!(res.is_some(), "inline `{{}}` body should still trigger");
    }

    #[test]
    pub(crate) fn implement_uses_external_interfaces_for_cross_module() {
        // Simulate `use cocoa { MyDel }` where MyDel lives in
        // another file: the local buffer has no `interface MyDel`
        // visible, but `external_interfaces` carries the decl
        // populated by the loader. The code action should fall
        // back to that map.
        let local_src = "\
class MyApp : MyDel {
}
";
        let toks = ilang_lexer::tokenize(local_src).unwrap();
        let local_prog = ilang_parser::parse(&toks).unwrap();

        // Build a stand-in InterfaceDecl by parsing a separate
        // snippet that DOES declare it.
        let ext_src = "\
@extern(ObjC) {
    @objc interface MyDel {
        notifyMe(name: i64)
        cleanup?()
    }
}
";
        let ext_toks = ilang_lexer::tokenize(ext_src).unwrap();
        let ext_prog = ilang_parser::parse(&ext_toks).unwrap();
        let mut ext_ifaces: std::collections::HashMap<
            AstSymbol,
            ilang_ast::InterfaceDecl,
        > = std::collections::HashMap::new();
        for it in &ext_prog.items {
            if let ilang_ast::Item::ExternC(b) = it {
                for i in b.interfaces.iter() {
                    ext_ifaces.insert(i.name, i.clone());
                }
            }
        }

        let (insert, new_text, count) =
            implement_interface_methods_at(local_src, &local_prog, &ext_ifaces, pos(0, 22))
                .expect("should fire for cross-module interface");
        assert!(count >= 2, "missing count = {count}");
        let mut out = local_src.to_string();
        out.insert_str(insert, &new_text);
        assert!(out.contains("pub notifyMe(name: i64) {"), "out:\n{out}");
        assert!(out.contains("pub cleanup() {"), "out:\n{out}");
        assert!(out.contains("optional"), "out:\n{out}");
    }

    #[test]
    pub(crate) fn external_sources_track_subfolder_mod_il_definitions() {
        // After the `bindings/cocoa/foundation/` split, `NSString` /
        // `NSObject` live in `foundation/core.il`, re-exported by
        // `foundation/mod.il`. F12 from a sibling binding
        // (`bindings/cocoa/spritekit.il` does `use foundation
        // { NSString }`) must still land on the real declaration —
        // the harvest used to give up when `<dir>/foundation.il`
        // didn't exist and miss the `<dir>/foundation/mod.il`
        // fallback the loader now accepts.
        use std::path::PathBuf;
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.pop();
        path.pop();
        path.push("bindings/cocoa/spritekit/node.il");
        let doc = analyse::analyse_path_to_doc(&path)
            .expect("bindings/cocoa/spritekit/node.il must load");
        let ns_string_loc = doc
            .external_sources
            .get(&AstSymbol::intern("NSString"))
            .expect("F12 should resolve `NSString` through foundation/mod.il");
        // The target file must be the actual declaration site, not
        // the umbrella stub.
        let path_str = ns_string_loc.path.to_string_lossy();
        assert!(
            path_str.ends_with("foundation/core.il"),
            "expected F12 target inside foundation/core.il, got {path_str}"
        );
    }

    #[test]
    pub(crate) fn local_class_inheriting_nsobject_has_synth_alloc_init_types() {
        // `examples/macos/cocoa/main.il` declares
        //   class AppDelegate : NSApplicationDelegate { ... }
        //   class FormHandler : NSObject { ... }
        // The loader's auto-lift gives both classes synthesized
        // `alloc` / `init` / `register` methods. Confirm the LSP's
        // local parse (post-lift) sees them — without the lift on
        // the buffer-local path, `let appDel = AppDelegate.alloc().init()`
        // would infer no type and hover would come up blank.
        use std::path::PathBuf;
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.pop();
        path.pop();
        path.push("examples/macos/cocoa/main.il");
        let doc = analyse::analyse_path_to_doc(&path)
            .expect("examples/macos/cocoa/main.il must load");

        let app_del_key = AstSymbol::intern("AppDelegate");
        let info = doc
            .classes
            .get(&app_del_key)
            .expect("AppDelegate must be in doc.classes");
        assert!(
            info.methods.contains_key(&AstSymbol::intern("init")),
            "AppDelegate is missing the synth `init` method"
        );
        // `alloc` is a static method on @objc classes.
        let alloc_present = info
            .methods
            .get(&AstSymbol::intern("alloc"))
            .map(|m| m.is_static)
            .unwrap_or(false);
        assert!(
            alloc_present,
            "AppDelegate is missing the synth static `alloc` method"
        );

        // Likewise, AppDelegate.alloc().init() should be inferrable
        // as Object("AppDelegate"). The buffer binds the result to
        // `appDel`; var_types stores the walker-inferred type.
        let app_del_ty = doc.var_types.get(&AstSymbol::intern("appDel"));
        assert!(
            matches!(
                app_del_ty,
                Some(ilang_ast::Type::Object(n)) if n.as_str() == "AppDelegate"
            ),
            "expected appDel: AppDelegate, got {:?}",
            app_del_ty
        );
    }

    #[test]
    pub(crate) fn type_completion_surfaces_cocoa_interface_for_example_click() {
        // Load examples/macos/cocoa_click/main.il through the same
        // path the LSP uses and inspect `type_completions(doc)`.
        // The buffer's `use cocoa { … }` does NOT list
        // `NSApplicationDelegate`, so the completion should label
        // it module-qualified (`cocoa.NSApplicationDelegate`).
        // `NSApplication` IS in the use list, so it should appear
        // bare.
        use std::path::PathBuf;
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.pop();
        path.pop();
        path.push("examples/macos/cocoa_click/main.il");
        let doc = analyse::analyse_path_to_doc(&path)
            .expect("examples/macos/cocoa_click/main.il must load");
        let items = type_completions(&doc);
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            labels.iter().any(|l| *l == "cocoa.NSApplicationDelegate"),
            "expected dotted `cocoa.NSApplicationDelegate` (not in use list). \
             First 40 labels: {:?}",
            labels.iter().take(40).collect::<Vec<_>>()
        );
        assert!(
            labels.iter().any(|l| *l == "NSApplication"),
            "expected bare `NSApplication` (in use list). \
             First 40 labels: {:?}",
            labels.iter().take(40).collect::<Vec<_>>()
        );
    }

    #[test]
    pub(crate) fn textual_completion_resolves_dotted_external_iface() {
        // User wrote `class AA: cocoa.NSMenuDelegate {}` at top
        // level and put the cursor inside the body. The textual
        // completion should surface every unimplemented method of
        // the cocoa.NSMenuDelegate interface.
        let local_src = "class AA: cocoa.NSMenuDelegate {\n    \n}\n";

        // Build external_interfaces the way `collect_external_interfaces`
        // would: dotted key + bare key both pointing at the decl.
        let ext_src = "\
@extern(ObjC) {
    @objc pub interface NSMenuDelegate {
        menuWillOpen?(menu: i64)
    }
}
";
        let ext_toks = ilang_lexer::tokenize(ext_src).unwrap();
        let ext_prog = ilang_parser::parse(&ext_toks).unwrap();
        let mut ext_ifaces: std::collections::HashMap<
            AstSymbol,
            ilang_ast::InterfaceDecl,
        > = std::collections::HashMap::new();
        for it in &ext_prog.items {
            if let ilang_ast::Item::ExternC(b) = it {
                for i in b.interfaces.iter() {
                    ext_ifaces.insert(i.name, i.clone());
                    ext_ifaces
                        .insert(AstSymbol::intern("cocoa.NSMenuDelegate"), i.clone());
                }
            }
        }
        let empty: std::collections::HashMap<AstSymbol, ilang_ast::InterfaceDecl> =
            std::collections::HashMap::new();
        let off = crate::text::line_col_to_offset(local_src, 2, 5).expect("offset");
        let stubs = interface_method_stub_completions_textual(
            local_src, off, &empty, &ext_ifaces,
        );
        let labels: Vec<&str> = stubs.iter().map(|(l, _, _)| l.as_str()).collect();
        assert!(
            labels.contains(&"menuWillOpen"),
            "labels did not include menuWillOpen: {labels:?}"
        );
    }

    #[test]
    pub(crate) fn implement_fires_when_cursor_on_class_header_line() {
        // VSCode's lightbulb often anchors on the class header
        // line, not the body. The action must still fire when the
        // cursor sits on `class NAME : Base` rather than between
        // the braces.
        let local_src = "class AA : MyDel {\n}\n";
        let toks = ilang_lexer::tokenize(local_src).unwrap();
        let local_prog = ilang_parser::parse(&toks).unwrap();

        let ext_src = "\
interface MyDel {
    notifyMe(name: i64)
}
";
        let ext_toks = ilang_lexer::tokenize(ext_src).unwrap();
        let ext_prog = ilang_parser::parse(&ext_toks).unwrap();
        let mut ext_ifaces: std::collections::HashMap<
            AstSymbol,
            ilang_ast::InterfaceDecl,
        > = std::collections::HashMap::new();
        for it in &ext_prog.items {
            if let ilang_ast::Item::Interface(i) = it {
                ext_ifaces.insert(i.name, i.clone());
            }
        }
        // Cursor on the `class` keyword (line 0, col 0).
        let res = implement_interface_methods_at(
            local_src,
            &local_prog,
            &ext_ifaces,
            pos(0, 0),
        );
        assert!(res.is_some(), "should fire with cursor on header");
    }

    #[test]
    pub(crate) fn implement_fires_for_objc_class_inside_extern_block() {
        // The `@objc class` is wrapped in an `@extern(ObjC) { … }`
        // block, parsed as `Item::ExternC` with the class buried
        // inside `ExternCItem::Class`. The code action must still
        // locate the enclosing class.
        let local_src = "\
@extern(ObjC) {
    @objc class MyApp : MyDel {
    }
}
";
        let toks = ilang_lexer::tokenize(local_src).unwrap();
        let local_prog = ilang_parser::parse(&toks).unwrap();

        let ext_src = "\
@extern(ObjC) {
    @objc interface MyDel {
        notifyMe(name: i64)
    }
}
";
        let ext_toks = ilang_lexer::tokenize(ext_src).unwrap();
        let ext_prog = ilang_parser::parse(&ext_toks).unwrap();
        let mut ext_ifaces: std::collections::HashMap<
            AstSymbol,
            ilang_ast::InterfaceDecl,
        > = std::collections::HashMap::new();
        for it in &ext_prog.items {
            if let ilang_ast::Item::ExternC(b) = it {
                for i in b.interfaces.iter() {
                    ext_ifaces.insert(i.name, i.clone());
                }
            }
        }

        // Cursor inside the @objc class body (line 2, between the
        // `{` and `}`).
        let res = implement_interface_methods_at(
            local_src,
            &local_prog,
            &ext_ifaces,
            pos(2, 4),
        );
        let (_, new_text, count) =
            res.expect("should fire for @objc class inside @extern(ObjC)");
        assert_eq!(count, 1, "missing count = {count}");
        assert!(new_text.contains("pub notifyMe(name: i64) {"), "new_text:\n{new_text}");
    }

    #[test]
    pub(crate) fn collect_symbols_picks_up_objc_interface() {
        // @objc interface declared inside @extern(ObjC) should
        // surface in `doc.symbols` so hover over the name works.
        let src = "\
@extern(ObjC) {
    @objc pub interface MyDel {
        notifyMe(name: i64)
        cleanup?()
    }
}
";
        let toks = ilang_lexer::tokenize(src).unwrap();
        let prog = ilang_parser::parse(&toks).unwrap();
        let syms = collect_symbols(&prog, src);
        let key = AstSymbol::intern("MyDel");
        let sym = syms.get(&key).expect("MyDel should be in symbols");
        assert!(
            sym.signature.contains("@objc interface MyDel"),
            "signature: {}",
            sym.signature
        );
        // The method list should be included in the hover detail.
        assert!(sym.signature.contains("notifyMe(name: i64)"), "{}", sym.signature);
        assert!(sym.signature.contains("cleanup?()"), "{}", sym.signature);
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
