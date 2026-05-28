//! Doc-comment extraction: the leading `//!` module banner and the
//! `///` block immediately above a declaration line.


/// Module-level doc: the `///` block that starts the file (after
/// any leading blank lines). Returns `None` if the first non-blank
/// line isn't `///` — keeps existing `//` file-header comments
/// out of the module hover.
pub(crate) fn extract_module_doc(text: &str) -> Option<String> {
    let mut lines = text.split('\n');
    // Skip leading blank lines so a stray blank line at the top
    // doesn't suppress the doc.
    let first = loop {
        let line = lines.next()?;
        if !line.trim().is_empty() {
            break line;
        }
    };
    let trimmed = first.trim_start();
    if !trimmed.starts_with("///") {
        return None;
    }
    let mut doc_lines: Vec<String> = Vec::new();
    let push = |dst: &mut Vec<String>, raw: &str| {
        let t = raw.trim_start();
        // Strip `///` and an optional single space.
        let body = &t[3..];
        let body = body.strip_prefix(' ').unwrap_or(body);
        dst.push(body.to_string());
    };
    push(&mut doc_lines, first);
    for line in lines {
        let t = line.trim_start();
        if t.starts_with("///") {
            push(&mut doc_lines, line);
        } else if t.is_empty() {
            // Blank `///` line authors might use to break paragraphs
            // stops the block; the file's real content is right
            // after. Module docs are meant to be a short opener.
            break;
        } else {
            break;
        }
    }
    if doc_lines.is_empty() {
        return None;
    }
    Some(doc_lines.join("\n"))
}

/// Extract a Rust-style doc comment block (`/// line` form) immediately
/// above the line containing `decl_line` (1-based). Returns the joined
/// body lines (without the leading `///` or single space) or `None`
/// when no contiguous `///` block precedes the decl.
pub(crate) fn extract_doc_above(text: &str, decl_line: u32) -> Option<String> {
    if decl_line <= 1 {
        return None;
    }
    // Only collect lines 0..decl_line-1 — we never look past the decl
    // itself. `split` is lazy, so `take` lets it stop early instead of
    // scanning the entire (possibly multi-thousand-line) source.
    let lines: Vec<&str> = text
        .split('\n')
        .take(decl_line.saturating_sub(1) as usize)
        .collect();
    let mut doc_lines: Vec<&str> = Vec::new();
    // Decl is at lines[decl_line - 1] (0-based). Walk back from there.
    let mut i = (decl_line as usize).saturating_sub(2); // line above
    loop {
        let Some(line) = lines.get(i) else { break };
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("///") {
            // Strip a single leading space (so `/// foo` -> `foo`,
            // `///foo` -> `foo`, `/// foo bar` -> `foo bar`).
            let body = rest.strip_prefix(' ').unwrap_or(rest);
            doc_lines.push(body);
            if i == 0 {
                break;
            }
            i -= 1;
            continue;
        }
        // Allow `@attribute(args)` between docs and decl; everything
        // else (blank line, code) ends the block. A line that also
        // contains `{` is a block-opening declaration (`@extern(C) {`,
        // `@objc pub class NSObject {`), not a pure attribute — stop
        // there so a method's `extract_doc_above` doesn't leak past
        // the class opener and pick up the class's doc comment.
        let pure_attr = trimmed.starts_with('@') && !trimmed.contains('{');
        if pure_attr || (trimmed.is_empty() && doc_lines.is_empty()) {
            // Blank line *before* any doc lines → no doc block here.
            // `@attr` lines between docs and decl are silently skipped.
            if i == 0 {
                break;
            }
            i -= 1;
            continue;
        }
        break;
    }
    if doc_lines.is_empty() {
        return None;
    }
    doc_lines.reverse();
    // Hover popups render this as Markdown. Use CommonMark's
    // default behaviour: a single `\n` between two non-blank lines
    // is a soft break (renders as a space, so multi-line `///`
    // comments flow as one wrapped paragraph), and a blank `///`
    // line stays empty and produces a real paragraph break. The
    // author opts into a paragraph by inserting an empty `///`
    // line; otherwise the lines join.
    Some(doc_lines.join("\n"))
}
