//! Token-aware document formatter for `.il`.
//!
//! Approach: re-lex the source, then walk tokens emitting canonical
//! whitespace between them while passing comments through verbatim.
//! The user's hand-written line breaks are respected (we never join
//! or split lines beyond collapsing 3+ blank lines into 1), so this
//! is a "mini" formatter — closer to gofmt's spirit than rustfmt's.
//!
//! What gets normalised:
//!   - 4-space indent recomputed from `{` / `}` depth
//!   - exactly one space after `,` / `;` / `:` (none before)
//!   - exactly one space around binary operators and `=` family
//!   - no space around `.`, `..`, `..=`, `::`
//!   - no space inside `(` / `[` boundaries
//!   - tabs flattened to spaces, trailing whitespace stripped, runs of
//!     3+ blank lines collapsed
//!
//! What the formatter intentionally avoids:
//!   - line wrapping / column-budget aware breaking
//!   - touching whitespace around `<` / `>` (ambiguous between
//!     comparison and generic brackets — leave the user's choice)
//!   - rewriting comments themselves
//!
//! On lex failure we return `None` so the LSP keeps the buffer as-is.
//!
//! The rules are intentionally conservative — output is meant to be a
//! superset of what users hand-write, not a strict reformat.

use ilang_lexer::tokenize;

use crate::text_utils::compute_line_starts;

mod emit;
mod match_break;
mod rewrap;
mod spacing;

use emit::format_tokens;
use match_break::match_break_pass;
use rewrap::rewrap_long_lines;

pub(super) const INDENT: &str = "    ";
/// Soft column budget. Lines longer than this trigger the
/// "break each comma-separated arg onto its own line" pass for the
/// outermost paren / bracket group on the line. Tuned to match the
/// `///` doc style already used in this codebase.
pub(super) const LINE_BUDGET: usize = 100;

pub fn format(src: &str) -> Option<String> {
    let tokens = tokenize(src).ok()?;
    // Pre-pass: when the program parses, expand `match` whose
    // arms share a line so each arm sits on its own line. This
    // is the AST-aware step (mid-B) — the rest of the formatter
    // is purely token-level and runs unchanged afterwards.
    let pre_src = match ilang_parser::parse(&tokens) {
        Ok(prog) => match_break_pass(src, &prog),
        Err(_) => src.to_string(),
    };
    let pre_tokens = if pre_src == src {
        tokens
    } else {
        match tokenize(&pre_src) {
            Ok(t) => t,
            Err(_) => return None,
        }
    };
    let line_starts = compute_line_starts(&pre_src);
    let mini = format_tokens(&pre_src, &pre_tokens, &line_starts);
    let formatted = rewrap_long_lines(&mini);
    if formatted == src {
        None
    } else {
        Some(formatted)
    }
}


/// `line` / `col` are 1-based; returns the byte offset of that
/// character in `src`. Walks the line from its start to count
/// characters (so multi-byte UTF-8 cols still resolve correctly).
pub(super) fn line_col_to_byte(src: &str, line_starts: &[usize], line: u32, col: u32) -> usize {
    let line_idx = (line as usize).saturating_sub(1).min(line_starts.len().saturating_sub(1));
    let line_start = line_starts[line_idx];
    let target_col = col as usize;
    let mut byte = line_start;
    let mut current_col = 1usize;
    for ch in src[line_start..].chars() {
        if current_col >= target_col {
            return byte;
        }
        byte += ch.len_utf8();
        current_col += 1;
        if ch == '\n' {
            // Shouldn't happen for valid spans, but be safe.
            return byte;
        }
    }
    byte
}



#[cfg(test)]
mod tests {
    use super::format;

    fn fmt(src: &str) -> String {
        format(src).unwrap_or_else(|| src.to_string())
    }

    #[test]
    fn objc_attr_breaks_onto_its_own_line() {
        let src = concat!(
            "@extern(ObjC) {\n",
            "    @objc pub class NSObject {\n",
            "        @objc(\"release\") pub release()\n",
            "    }\n",
            "}\n",
        );
        let want = concat!(
            "@extern(ObjC) {\n",
            "    @objc pub class NSObject {\n",
            "        @objc(\"release\")\n",
            "        pub release()\n",
            "    }\n",
            "}\n",
        );
        assert_eq!(fmt(src), want);
    }

    #[test]
    fn objc_attr_already_on_own_line_is_a_no_op() {
        let src = concat!(
            "@extern(ObjC) {\n",
            "    @objc pub class NSObject {\n",
            "        @objc(\"release\")\n",
            "        pub release()\n",
            "    }\n",
            "}\n",
        );
        assert_eq!(format(src), None);
    }

    #[test]
    fn already_canonical() {
        let src = "fn main() {\n    let x = 1\n    x + 2\n}\n";
        assert_eq!(format(src), None);
    }

    #[test]
    fn fixes_indent() {
        let src = "fn main() {\nlet x = 1\nx + 2\n}\n";
        let want = "fn main() {\n    let x = 1\n    x + 2\n}\n";
        assert_eq!(fmt(src), want);
    }

    #[test]
    fn normalizes_assignment_spacing() {
        let src = "let x=1\n";
        let want = "let x = 1\n";
        assert_eq!(fmt(src), want);
    }

    #[test]
    fn normalizes_comma_spacing() {
        let src = "fn f(a:i64,b:i64):i64{a+b}\n";
        let want = "fn f(a: i64, b: i64): i64 { a + b }\n";
        assert_eq!(fmt(src), want);
    }

    #[test]
    fn no_space_inside_parens() {
        let src = "let x = ( 1 + 2 )\n";
        let want = "let x = (1 + 2)\n";
        assert_eq!(fmt(src), want);
    }

    #[test]
    fn no_space_around_dot() {
        let src = "let n = obj . field\n";
        let want = "let n = obj.field\n";
        assert_eq!(fmt(src), want);
    }

    #[test]
    fn collapses_extra_spaces() {
        let src = "let   x   =   1\n";
        let want = "let x = 1\n";
        assert_eq!(fmt(src), want);
    }

    #[test]
    fn preserves_line_comment() {
        let src = "let x = 1 // value\nlet y = 2\n";
        // Already canonical — formatter should be a no-op.
        assert_eq!(format(src), None);
    }

    #[test]
    fn block_comment_kept_inline() {
        let src = "let x = 1 /* note */ + 2\n";
        assert_eq!(format(src), None);
    }

    #[test]
    fn collapses_blank_runs() {
        let src = "let x = 1\n\n\n\nlet y = 2\n";
        let want = "let x = 1\n\nlet y = 2\n";
        assert_eq!(fmt(src), want);
    }

    #[test]
    fn closing_brace_outdents() {
        let src = "fn f() {\n    let x = 1\n    }\n";
        let want = "fn f() {\n    let x = 1\n}\n";
        assert_eq!(fmt(src), want);
    }

    #[test]
    fn range_no_space() {
        let src = "for i in 1 .. 5 { }\n";
        // Empty `{ }` block bodies stay as-is (block braces keep
        // a space inside; collapsing to `{}` would diverge from
        // the rest of the codebase's style).
        let want = "for i in 1..5 { }\n";
        assert_eq!(fmt(src), want);
    }

    #[test]
    fn optional_suffix_no_space() {
        let src = "let a : i64 ? = none\n";
        let want = "let a: i64? = none\n";
        assert_eq!(fmt(src), want);
    }

    #[test]
    fn unary_minus_no_space() {
        let src = "let x = -1\nlet y = a + -b\n";
        // Both are well-formed; formatter shouldn't insert a stray
        // space after the unary `-`.
        assert_eq!(format(src), None);
    }

    #[test]
    fn idempotent_on_messy_input() {
        let src = "class Foo{\na:i32\nb:string\ninit(x:i32,y:string){this.a=x;this.b=y}\ngreet():string{\"hi \"+this.b}\n}\nlet f=new Foo(1,\"a\")\nfor i in 0..10{console.log(i)}\n";
        let once = format(src).expect("first pass should reformat");
        let twice = format(&once);
        assert_eq!(
            twice, None,
            "second pass should be a no-op (idempotent), got: {twice:?}"
        );
    }

    #[test]
    fn preserves_inline_block_comment() {
        let src = "let x = 1 /* note */ + 2\n";
        // Already canonical — should be a no-op.
        assert_eq!(format(src), None);
    }

    #[test]
    fn tabs_become_four_spaces() {
        let src = "fn main() {\n\tlet x = 1\n}\n";
        let want = "fn main() {\n    let x = 1\n}\n";
        assert_eq!(fmt(src), want);
    }

    #[test]
    fn nested_braces() {
        let src = "fn outer() {\nfn inner() {\nlet x = 1\n}\n}\n";
        let want = "fn outer() {\n    fn inner() {\n        let x = 1\n    }\n}\n";
        assert_eq!(fmt(src), want);
    }

    #[test]
    fn ignores_braces_in_strings() {
        let src = "let s = \"a {b} c\"\nlet t = 1\n";
        assert_eq!(format(src), None);
    }

    #[test]
    fn ignores_braces_in_line_comments() {
        let src = "// what about { this }\nlet t = 1\n";
        assert_eq!(format(src), None);
    }

    #[test]
    fn ignores_braces_in_block_comments() {
        let src = "/* { not\n   a } block */\nlet t = 1\n";
        assert_eq!(format(src), None);
    }

    #[test]
    fn long_call_breaks_args() {
        // 100+ char single-line call should fan out one arg per line.
        let src = "let r = something_long_named_function(\
                   alpha_value, beta_value, gamma_value, \
                   delta_value, epsilon_value, zeta_value)\n";
        let out = fmt(src);
        // Head line ends with `(`.
        let lines: Vec<&str> = out.split('\n').collect();
        assert!(lines[0].ends_with('('), "head: {:?}", lines[0]);
        // Each arg on its own line, indented one level. The source
        // had no trailing comma, so the last element doesn't get one
        // (formatter preserves the source's trailing-comma choice).
        assert!(lines.iter().any(|l| l == &"    alpha_value,"), "lines: {lines:#?}");
        assert!(lines.iter().any(|l| l == &"    zeta_value"), "lines: {lines:#?}");
        // Closing paren back at base indent.
        assert!(lines.iter().any(|l| l == &")"), "lines: {lines:#?}");
    }

    #[test]
    fn short_call_stays_inline() {
        let src = "let r = f(a, b, c)\n";
        assert_eq!(format(src), None);
    }

    #[test]
    fn long_array_literal_breaks() {
        let src = "let xs: i64[] = [\
                   100000, 200000, 300000, 400000, 500000, 600000, \
                   700000, 800000, 900000, 1000000, 1100000]\n";
        let out = fmt(src);
        assert!(out.contains("[\n"), "expected break after `[`:\n{out}");
        assert!(out.contains("\n]"), "expected close on own line:\n{out}");
    }

    #[test]
    fn broken_call_stays_idempotent() {
        // Pre-broken multi-line — second pass shouldn't merge.
        let src = "let r = foo(\n    a,\n    b,\n    c,\n)\n";
        // Already canonical (4-space inner indent, `)` at column 0).
        assert_eq!(format(src), None);
    }

    #[test]
    fn match_arms_get_their_own_lines() {
        // Comma-separated arms on one line — valid ilang. Formatter
        // should expand each arm onto its own line.
        let src = "let v = match n { 0 { \"zero\" }, 1 { \"one\" }, _ { \"other\" } }\n";
        let out = fmt(src);
        // Each arm sits on its own line, indented one level deep
        // relative to the `match`'s opening line.
        assert!(out.contains("match n {\n"), "out:\n{out}");
        assert!(out.contains("    0 { \"zero\" }"), "out:\n{out}");
        assert!(out.contains("    1 { \"one\" }"), "out:\n{out}");
        assert!(out.contains("    _ { \"other\" }"), "out:\n{out}");
        assert!(out.contains("\n}"), "closing brace on own line:\n{out}");
    }

    #[test]
    fn match_already_broken_stays() {
        let src = "\
let v = match n {
    0 { \"zero\" }
    1 { \"one\" }
    _ { \"other\" }
}
";
        // Already canonical — no change.
        assert_eq!(format(src), None);
    }

    #[test]
    fn single_arm_match_stays_inline() {
        let src = "let v = match n { _ { 0 } }\n";
        // Don't break a one-arm match (less than 2 arms — no point).
        assert_eq!(format(src), None);
    }

    #[test]
    fn match_break_idempotent() {
        let src = "let v = match n { 0 { \"zero\" } 1 { \"one\" } _ { \"other\" } }\n";
        let once = fmt(src);
        let twice = format(&once);
        assert!(twice.is_none(), "second pass changed: {twice:?}");
    }

    /// Walks every `.il` file under the cargo workspace, runs the
    /// formatter, and verifies the formatted output still tokenises
    /// cleanly. Catches whole-file regressions where the formatter
    /// loses or shifts syntactically meaningful tokens.
    #[test]
    fn formatter_preserves_lexability_on_corpus() {
        use std::path::PathBuf;
        // Workspace root is the parent of this crate's manifest.
        let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let workspace = crate_dir.parent().and_then(|p| p.parent()).unwrap();
        let mut paths: Vec<PathBuf> = Vec::new();
        collect_il(workspace, &mut paths);
        let mut tested = 0usize;
        for p in paths {
            let src = match std::fs::read_to_string(&p) {
                Ok(s) => s,
                Err(_) => continue,
            };
            // Original must parse; otherwise nothing to compare.
            if ilang_lexer::tokenize(&src).is_err() {
                continue;
            }
            let formatted = format(&src).unwrap_or_else(|| src.clone());
            let orig_toks =
                ilang_lexer::tokenize(&src).expect("orig tokenises");
            let fmt_toks = match ilang_lexer::tokenize(&formatted) {
                Ok(t) => t,
                Err(_) => panic!(
                    "formatter broke lex of {}: \n{}",
                    p.display(),
                    formatted
                ),
            };
            // Token kinds must line up — formatter can't drop or
            // invent any token. Spans and `leading_newline` may
            // shift legitimately, so compare kinds only.
            let orig_kinds: Vec<_> =
                orig_toks.iter().map(|t| &t.kind).collect();
            let fmt_kinds: Vec<_> =
                fmt_toks.iter().map(|t| &t.kind).collect();
            assert_eq!(
                orig_kinds,
                fmt_kinds,
                "token sequence diverged in {}",
                p.display()
            );
            // Idempotency on real code.
            let again = format(&formatted);
            assert!(
                again.is_none(),
                "format not idempotent on {}: {:?}",
                p.display(),
                again
            );
            tested += 1;
        }
        assert!(tested > 0, "didn't find any .il fixtures");
    }

    fn collect_il(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        for entry in rd.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if path.file_name().and_then(|n| n.to_str()) == Some("target") {
                    continue;
                }
                collect_il(&path, out);
            } else if path.extension().and_then(|e| e.to_str()) == Some("il") {
                out.push(path);
            }
        }
    }
}
