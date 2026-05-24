//! Semantic scope-conflict detection for `rename` requests.
//!
//! Classifies the rename target via its hover signature and walks
//! the appropriate scope to decide whether `new_name` would collide
//! with an existing binding.
//!
//! Categories covered (mirrors the user's punch list):
//!   - top-level decl (fn / class / interface / enum / const /
//!     struct / union) → buffer-local `Doc.symbols` + selectively-
//!     imported names
//!   - class field / method / static / property /
//!     getter / setter → `Doc.classes[Class]`
//!   - enum variant → re-parse the file's enum
//!   - local `let` → same-block siblings
//!   - function parameter → other params on the same fn

use ilang_ast::{Item, Span, Symbol as AstSymbol};

use crate::types::Doc;

mod local_visitor;
mod param_visitor;

use local_visitor::check_local;
use param_visitor::check_parameter;

/// Detect whether renaming the symbol at `decl_name_span` to
/// `new_name` would collide with an existing binding. `Ok(())`
/// means the rename is safe; `Err(msg)` is a one-line description
/// the LSP can surface as an `invalid_params` rejection.
pub(crate) fn detect(
    doc: &Doc,
    sig: &str,
    decl_name_span: Span,
    old_name: &str,
    new_name: &str,
) -> Result<(), String> {
    if old_name == new_name {
        return Ok(());
    }
    let Some(kind) = classify(sig) else { return Ok(()) };
    match kind {
        Kind::TopLevel => check_top_level(doc, decl_name_span, new_name),
        Kind::ClassMember { class } => {
            check_class_member(doc, &class, decl_name_span, new_name)
        }
        Kind::Variant { enum_name } => check_variant(&doc.text, &enum_name, new_name),
        Kind::Local => check_local(&doc.text, decl_name_span, new_name),
        Kind::Parameter => check_parameter(&doc.text, decl_name_span, new_name),
    }
}

#[derive(Debug)]
enum Kind {
    /// Top-level decl: fn / class / interface / enum / const /
    /// struct / union.
    TopLevel,
    /// Class member — fields / methods / static_methods /
    /// properties / getters / setters / static_fields. `class` is
    /// the declaring class name (used to index `Doc.classes`).
    ClassMember { class: String },
    /// Enum variant. `enum_name` is the enum's name.
    Variant { enum_name: String },
    /// `let` inside a function body (or tuple / struct destructure).
    Local,
    /// Function parameter (top-level fn or class method).
    Parameter,
}

/// Strip attribute lines (`@objc(\"...\")\n`) and return the last
/// non-empty line — the form `Class.method(...)` or `class Foo`
/// our signatures end with.
fn last_line(sig: &str) -> &str {
    sig.lines().last().unwrap_or(sig)
}

fn classify(sig: &str) -> Option<Kind> {
    // Class members: `(method) Class.m(...)`, `(static method)`,
    // `(property) Class.x: T`, `(getter)`, `(setter)`,
    // `(static property)`, `(static const)`.
    let class_member_prefixes = [
        "(static method) ",
        "(static getter) ",
        "(static setter) ",
        "(static property) ",
        "(static const) ",
        "(method) ",
        "(getter) ",
        "(setter) ",
        "(property) ",
    ];
    for prefix in class_member_prefixes {
        if let Some(rest) = sig.strip_prefix(prefix) {
            let tail = last_line(rest);
            let class = tail.split('.').next()?.to_string();
            return Some(Kind::ClassMember { class });
        }
    }
    if let Some(rest) = sig.strip_prefix("(variant) ") {
        let enum_name = rest.split('.').next()?.to_string();
        return Some(Kind::Variant { enum_name });
    }
    if sig.starts_with("(parameter) ") {
        return Some(Kind::Parameter);
    }
    if sig.starts_with("let ") || sig.starts_with("(for-binding) ") || sig.starts_with("(pattern) ") {
        return Some(Kind::Local);
    }
    // Top-level header forms. `fn ` may include `@objc fn ` /
    // `@extern fn ` prefixes; the last line check handles both.
    let head = last_line(sig);
    for prefix in [
        "fn ", "class ", "interface ", "enum ", "const ", "struct ", "union ",
    ] {
        if head.starts_with(prefix) {
            return Some(Kind::TopLevel);
        }
    }
    None
}

fn check_top_level(
    doc: &Doc,
    decl_name_span: Span,
    new_name: &str,
) -> Result<(), String> {
    let key = AstSymbol::intern(new_name);
    if let Some(sym) = doc.symbols.get(&key) {
        if sym.span != decl_name_span {
            return Err(format!(
                "`{new_name}` is already defined in this file"
            ));
        }
    }
    if doc.selective_use_names.contains(&key) {
        return Err(format!(
            "`{new_name}` collides with a `use` import"
        ));
    }
    Ok(())
}

fn check_class_member(
    doc: &Doc,
    class: &str,
    decl_name_span: Span,
    new_name: &str,
) -> Result<(), String> {
    let key = AstSymbol::intern(new_name);
    let class_key = AstSymbol::intern(class);
    let Some(info) = doc.classes.get(&class_key) else { return Ok(()) };
    // For each member map, a collision is anything carrying the
    // new_name except the target itself (matched by span).
    let buckets = [
        ("field/property", &info.fields),
        ("method", &info.methods),
        ("getter", &info.getters),
        ("setter", &info.setters),
    ];
    for (label, map) in buckets {
        if let Some(m) = map.get(&key) {
            if m.span != decl_name_span {
                return Err(format!(
                    "`{new_name}` is already a {label} on `{class}`"
                ));
            }
        }
    }
    Ok(())
}

fn check_variant(text: &str, enum_name: &str, new_name: &str) -> Result<(), String> {
    let Some(prog) = crate::text::try_parse(text) else { return Ok(()) };
    for item in &prog.items {
        if let Item::Enum(e) = item {
            if e.name.as_str() == enum_name {
                for v in e.variants.iter() {
                    if v.name.as_str() == new_name {
                        return Err(format!(
                            "`{enum_name}.{new_name}` already exists"
                        ));
                    }
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyse_path_to_doc;

    fn prep(src: &str) -> (std::path::PathBuf, Doc) {
        let tmp = std::env::temp_dir().join(format!(
            "ilang_rename_conflicts_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(tmp.join("ilang.toml"), "[package]\nname = \"t\"\n").unwrap();
        let path = tmp.join("a.il");
        std::fs::write(&path, src).unwrap();
        let doc = analyse_path_to_doc(&path).expect("analyse");
        (path, doc)
    }

    #[test]
    fn detects_top_level_collision() {
        let src = "fn foo() { }\nfn bar() { }\nrun()\nfn run() { }\n";
        let (_path, doc) = prep(src);
        // Rename `foo` to `bar` — should collide.
        let target_span = doc.symbols.get(&AstSymbol::intern("foo")).unwrap().span;
        let err = detect(&doc, "fn foo()", target_span, "foo", "bar").unwrap_err();
        assert!(err.contains("already defined"), "got: {err}");
        // Rename `foo` to `baz` — no collision.
        detect(&doc, "fn foo()", target_span, "foo", "baz").unwrap();
    }

    #[test]
    fn detects_class_member_collision() {
        let src = "class C { x: i64; y: i64 }\nfn run() { }\nrun()\n";
        let (_path, doc) = prep(src);
        let class_info = doc.classes.get(&AstSymbol::intern("C")).unwrap();
        let target_span = class_info.fields.get(&AstSymbol::intern("x")).unwrap().span;
        let err = detect(
            &doc,
            "(property) C.x: i64",
            target_span,
            "x",
            "y",
        )
        .unwrap_err();
        assert!(err.contains("already"), "got: {err}");
    }

    #[test]
    fn detects_variant_collision() {
        let src = "enum Color { Red, Green, Blue }\nfn run() { }\nrun()\n";
        let (_path, doc) = prep(src);
        // Span isn't critical here — variant check just looks for
        // the new name in the enum's variant list.
        let dummy_span = ilang_ast::Span::new(1, 1);
        let err = detect(
            &doc,
            "(variant) Color.Red",
            dummy_span,
            "Red",
            "Green",
        )
        .unwrap_err();
        assert!(err.contains("already exists"), "got: {err}");
    }

    #[test]
    fn detects_same_block_let_collision() {
        let src = "fn run() {\n    let x = 1\n    let y = 2\n    let _ = x + y\n}\nrun()\n";
        let (_path, doc) = prep(src);
        // Target `let x = 1` is on line 2 (1-based). Its decl-name
        // span is line=2, col=9 (the `x` after `let `).
        let target_span = ilang_ast::Span::new(2, 9);
        let err = detect(&doc, "let x", target_span, "x", "y").unwrap_err();
        assert!(err.contains("already declared"), "got: {err}");
        // Renaming to an unused name is fine.
        detect(&doc, "let x", target_span, "x", "z").unwrap();
    }

    #[test]
    fn detects_parameter_collision() {
        let src = "fn add(a: i64, b: i64): i64 { a + b }\nrun()\nfn run() { }\n";
        let (_path, doc) = prep(src);
        // Param `a` on line 1. The parser stores `Param.span` at
        // the name position.
        let target_span = ilang_ast::Span::new(1, 8);
        let err = detect(
            &doc,
            "(parameter) a: i64",
            target_span,
            "a",
            "b",
        )
        .unwrap_err();
        assert!(err.contains("already a parameter"), "got: {err}");
    }
}
