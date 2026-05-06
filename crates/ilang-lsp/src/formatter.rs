//! Whitespace-only document formatter.
//!
//! The formatter never rewrites token content — it only:
//! 1. recomputes the leading indent of each line from `{` / `}`
//!    nesting depth (4 spaces per level),
//! 2. strips trailing whitespace,
//! 3. converts hard tabs to spaces, and
//! 4. collapses runs of more than one blank line into one.
//!
//! Strings, line comments and block comments are scanned through so
//! braces inside them don't shift the indent. Anything else on a
//! line is preserved verbatim.

const INDENT: &str = "    ";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Code,
    LineComment,
    BlockComment(u32), // nesting depth, mirrors the lexer
    String,
}

/// Reformat `src` line-by-line. Returns `None` when the input is
/// already in canonical shape so the LSP can skip publishing an
/// edit.
pub fn format(src: &str) -> Option<String> {
    let lines: Vec<&str> = src.split('\n').collect();
    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    let mut depth: i32 = 0;
    let mut mode = Mode::Code;
    let mut blank_run: u32 = 0;

    for line in &lines {
        // Lines that begin while we're still inside a block comment
        // are preserved verbatim (modulo trailing whitespace) — the
        // user may have aligned the comment body and we don't want
        // to flatten that.
        if matches!(mode, Mode::BlockComment(_)) {
            let preserved = line.trim_end_matches([' ', '\t']);
            out.push(preserved.to_string());
            scan_modes_for_indent(preserved, &mut mode, &mut depth);
            blank_run = 0;
            continue;
        }
        // Drop tabs / trailing whitespace before measuring content.
        let stripped: String = line.replace('\t', INDENT);
        let stripped = stripped.trim_end();
        let content = stripped.trim_start();

        // Empty / whitespace-only lines: collapse runs and emit a
        // single empty line. Keep the very first run intact for now —
        // the truncation below trims trailing blanks at end-of-file.
        if content.is_empty() {
            blank_run += 1;
            if blank_run <= 1 {
                out.push(String::new());
            }
            continue;
        }
        blank_run = 0;

        // Lines that *start* by closing a block (`}` or `})`) outdent
        // themselves before the rest renders. Without this they'd sit
        // at the inner level. The same goes for `)` / `]` when they
        // close an indent we opened — we don't track paren depth, so
        // only `}` triggers the prefix outdent.
        let leading_closes_block = content.starts_with('}');
        let line_indent = if leading_closes_block {
            (depth - 1).max(0)
        } else {
            depth.max(0)
        };
        let mut formatted = String::with_capacity(content.len() + (line_indent as usize) * INDENT.len());
        for _ in 0..line_indent {
            formatted.push_str(INDENT);
        }
        formatted.push_str(content);
        out.push(formatted);

        scan_modes_for_indent(content, &mut mode, &mut depth);
    }

    // Trim trailing blank lines so the file ends with exactly one
    // newline (and no stray blanks after it).
    while out.last().map(|s| s.is_empty()).unwrap_or(false) {
        out.pop();
    }
    let mut formatted = out.join("\n");
    if !formatted.is_empty() {
        formatted.push('\n');
    }
    if formatted == src {
        None
    } else {
        Some(formatted)
    }
}

/// Walk one line of source and update `mode` / `depth` so the next
/// line knows whether it sits inside a block comment / string and
/// at what brace-nesting level.
fn scan_modes_for_indent(line: &str, mode: &mut Mode, depth: &mut i32) {
    // `LineComment` scope ends at the line break; reset before scan.
    if *mode == Mode::LineComment {
        *mode = Mode::Code;
    }
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        let next = bytes.get(i + 1).copied();
        match *mode {
            Mode::Code => {
                match c {
                    b'/' if next == Some(b'/') => {
                        *mode = Mode::LineComment;
                        i += 2;
                        continue;
                    }
                    b'/' if next == Some(b'*') => {
                        *mode = Mode::BlockComment(1);
                        i += 2;
                        continue;
                    }
                    b'"' => {
                        *mode = Mode::String;
                        i += 1;
                        continue;
                    }
                    b'{' => *depth += 1,
                    b'}' => *depth -= 1,
                    _ => {}
                }
                i += 1;
            }
            Mode::LineComment => {
                i += 1;
            }
            Mode::BlockComment(d) => {
                if c == b'/' && next == Some(b'*') {
                    *mode = Mode::BlockComment(d + 1);
                    i += 2;
                    continue;
                }
                if c == b'*' && next == Some(b'/') {
                    *mode = if d == 1 {
                        Mode::Code
                    } else {
                        Mode::BlockComment(d - 1)
                    };
                    i += 2;
                    continue;
                }
                i += 1;
            }
            Mode::String => {
                // Honour `\\` and `\"` escapes so an inline `"`
                // doesn't close prematurely.
                if c == b'\\' && next.is_some() {
                    i += 2;
                    continue;
                }
                if c == b'"' {
                    *mode = Mode::Code;
                }
                i += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::format;

    #[test]
    fn already_canonical() {
        let src = "fn main() {\n    let x = 1\n    x + 2\n}\n";
        assert_eq!(format(src), None);
    }

    #[test]
    fn fixes_indent() {
        let src = "fn main() {\nlet x = 1\nx + 2\n}\n";
        let want = "fn main() {\n    let x = 1\n    x + 2\n}\n";
        assert_eq!(format(src).unwrap(), want);
    }

    #[test]
    fn strips_trailing_whitespace() {
        let src = "let x = 1   \n";
        assert_eq!(format(src).unwrap(), "let x = 1\n");
    }

    #[test]
    fn tabs_become_four_spaces() {
        let src = "fn main() {\n\tlet x = 1\n}\n";
        let want = "fn main() {\n    let x = 1\n}\n";
        assert_eq!(format(src).unwrap(), want);
    }

    #[test]
    fn nested_braces() {
        let src = "fn outer() {\nfn inner() {\nlet x = 1\n}\n}\n";
        let want = "fn outer() {\n    fn inner() {\n        let x = 1\n    }\n}\n";
        assert_eq!(format(src).unwrap(), want);
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
    fn collapses_blank_runs() {
        let src = "let x = 1\n\n\n\nlet y = 2\n";
        let want = "let x = 1\n\nlet y = 2\n";
        assert_eq!(format(src).unwrap(), want);
    }

    #[test]
    fn closing_brace_outdents() {
        let src = "fn f() {\n    let x = 1\n    }\n";
        let want = "fn f() {\n    let x = 1\n}\n";
        assert_eq!(format(src).unwrap(), want);
    }
}
