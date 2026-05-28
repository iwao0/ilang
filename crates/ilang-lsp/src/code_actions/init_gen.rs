//! `generate_init_at` — cursor inside a `class` body that has fields
//! but no `init` → emit a constructor that takes one parameter per
//! field and assigns each to `this.field`. Skips `@extern("...")`
//! opaque-handle classes and `@extern(C) struct` classes (init is
//! rejected for both).

use ilang_ast::{Item, Program};
use tower_lsp::lsp_types::Position;

use super::super::text::{self, line_start_before};
use super::super::walker::is_parser_synth_field;
use super::{match_brace_range, pick_innermost_containing};

/// Find the innermost `class` whose body `{...}` contains the cursor
/// and, when the class has fields but no `init` method, return the
/// byte offset and source text for an inserted constructor that
/// takes one parameter per field and assigns each to `this.field`.
pub(crate) fn generate_init_at(
    text: &str,
    prog: &Program,
    cursor: Position,
) -> Option<(usize, String)> {
    let cursor_byte =
        text::line_col_to_offset(text, cursor.line + 1, cursor.character + 1)?;
    let class_ranges = prog.items.iter().filter_map(|it| {
        let Item::Class(c) = it else { return None };
        let (open, close) = match_brace_range(text, c.span)?;
        Some((c, open, close))
    });
    let (cls, _open, close) = pick_innermost_containing(class_ranges, cursor_byte)?;
    if cls.extern_lib.is_some() || cls.is_repr_c {
        return None;
    }
    if cls
        .fields
        .iter()
        .all(|f| is_parser_synth_field(f, cls.span))
    {
        return None;
    }
    if cls
        .methods
        .iter()
        .any(|m| m.name.as_str() == "init")
    {
        return None;
    }
    // Indentation: copy the closing `}`'s line indent for the class
    // and indent body / params one level deeper.
    let close_line_start = line_start_before(text, close);
    let base_indent: String = text[close_line_start..close]
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .collect();
    let body_indent = format!("{base_indent}    ");
    let assign_indent = format!("{body_indent}    ");
    let user_fields: Vec<_> = cls
        .fields
        .iter()
        .filter(|f| !is_parser_synth_field(f, cls.span))
        .collect();
    let params: Vec<String> = user_fields
        .iter()
        .map(|f| format!("{}: {}", f.name.as_str(), f.ty))
        .collect();
    let mut out = String::new();
    out.push_str(&body_indent);
    out.push_str("init(");
    out.push_str(&params.join(", "));
    out.push_str(") {\n");
    for f in user_fields.iter() {
        out.push_str(&assign_indent);
        out.push_str("this.");
        out.push_str(f.name.as_str());
        out.push_str(" = ");
        out.push_str(f.name.as_str());
        out.push('\n');
    }
    out.push_str(&body_indent);
    out.push_str("}\n");
    Some((close_line_start, out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ilang_lexer::tokenize;
    use ilang_parser::parse;

    fn pos(line: u32, col: u32) -> Position {
        Position { line, character: col }
    }

    fn run_init(src: &str, cursor: Position) -> Option<String> {
        let toks = tokenize(src).ok()?;
        let prog = parse(&toks).ok()?;
        let (insert, new_text) = generate_init_at(src, &prog, cursor)?;
        let mut out = src.to_string();
        out.insert_str(insert, &new_text);
        Some(out)
    }

    #[test]
    fn init_generated_from_fields() {
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
    fn init_skipped_when_already_defined() {
        let src = "\
class Point {
    x: f64
    init(x: f64) { this.x = x }
}
";
        assert!(run_init(src, pos(1, 4)).is_none());
    }

    #[test]
    fn init_skipped_for_empty_class() {
        let src = "\
class Empty {
}
";
        assert!(run_init(src, pos(1, 0)).is_none());
    }

    #[test]
    fn init_outside_class_returns_none() {
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
    fn init_renders_complex_field_types() {
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
}
