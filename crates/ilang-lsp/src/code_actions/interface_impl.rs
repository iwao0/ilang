//! `implement_interface_methods_at` and the bare-ident stub
//! completions for unimplemented interface methods. Both walk the
//! enclosing class's base list, look up each interface declaration
//! (local or external), and enumerate the methods the class hasn't
//! implemented yet. The textual variant additionally scans the raw
//! buffer so completions keep firing while the class body is
//! mid-edit and doesn't parse cleanly.

use std::collections::{HashMap, HashSet};

use ilang_ast::{
    ClassDecl, ExternCItem, InterfaceDecl, InterfaceMethod, Item, Program, Symbol as AstSymbol,
    Type,
};
use tower_lsp::lsp_types::Position;

use super::super::text::{self, line_start_before};
use super::{match_brace_range, pick_innermost_containing};

/// `implement_interface_methods_at`: cursor inside a `class` body
/// whose base list includes one or more `interface` declarations
/// — emit one stub method per *missing* interface method (both
/// required and `@optional`, with the `@optional` ones marked in
/// a leading comment so the user knows they can delete the body
/// if they don't actually want to override).
///
/// Returns `(insert_byte, source, missing_count)` or `None` when
/// there's nothing to do (no enclosing class, no interface bases,
/// or every interface method already has an implementation).
pub(crate) fn implement_interface_methods_at(
    text: &str,
    prog: &Program,
    external_interfaces: &HashMap<AstSymbol, InterfaceDecl>,
    cursor: Position,
) -> Option<(usize, String, usize)> {
    let cursor_byte =
        text::line_col_to_offset(text, cursor.line + 1, cursor.character + 1)?;

    // Find the innermost `class … { … }` containing the cursor.
    // Walks top-level classes AND `@objc class` declarations wrapped
    // in an `@extern(ObjC) { … }` block. The cursor counts as
    // "inside" the class anywhere from the `class` keyword through
    // the closing `}` — so VSCode's lightbulb, which often anchors
    // on the header line rather than the body, still surfaces this
    // action.
    let class_ranges = all_classes(prog).filter_map(|c| {
        let (open, close) = match_brace_range(text, c.span)?;
        let start = text::line_col_to_offset(text, c.span.line, c.span.col)
            .unwrap_or(open);
        Some((c, start, close))
    });
    let (cls, _start, close) = pick_innermost_containing(class_ranges, cursor_byte)?;

    let iface_decls = collect_base_interface_decls(cls, prog, external_interfaces);
    if iface_decls.is_empty() {
        return None;
    }
    let missing = enumerate_missing_methods(cls, &iface_decls);
    if missing.is_empty() {
        return None;
    }

    // Indentation: copy the closing `}`'s line indent for the class
    // (whitespace before the brace's column) and add four spaces for
    // the method body.
    let close_line_start = line_start_before(text, close);
    let base_indent: String = text[close_line_start..close]
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .collect();
    let body_indent = format!("{base_indent}    ");
    let inner_indent = format!("{body_indent}    ");

    let mut out = String::new();
    for (_iface, m) in &missing {
        if m.is_optional {
            out.push_str(&body_indent);
            out.push_str("// optional (`?`) — delete if not overriding\n");
        }
        out.push_str(&body_indent);
        out.push_str(&format_method_header(m));
        out.push_str(" {\n");
        out.push_str(&inner_indent);
        out.push_str("// TODO\n");
        if let Some(ret) = &m.ret {
            if let Some(default_lit) = default_value_for(ret) {
                out.push_str(&inner_indent);
                out.push_str(default_lit);
                out.push('\n');
            }
        }
        out.push_str(&body_indent);
        out.push_str("}\n");
    }
    let count = missing.len();
    Some((close_line_start, out, count))
}

/// Yield every `ClassDecl` reachable from a `Program` — both
/// top-level `Item::Class` and `@objc class` declarations wrapped
/// in an `@extern(ObjC) { … }` block (parsed as `Item::ExternC`
/// with `ExternCItem::Class` inside). Cursor-locating code action
/// passes need both, otherwise `@objc class` bodies look invisible.
fn all_classes(prog: &Program) -> impl Iterator<Item = &ClassDecl> {
    prog.items.iter().flat_map(|it| -> Box<dyn Iterator<Item = &ClassDecl>> {
        match it {
            Item::Class(c) => Box::new(std::iter::once(c)),
            Item::ExternC(b) => Box::new(b.items.iter().filter_map(|i| {
                if let ExternCItem::Class(c) = i { Some(c) } else { None }
            })),
            _ => Box::new(std::iter::empty()),
        }
    })
}

/// Find an `Item::Interface` or `block.interfaces[name]` with the
/// given name. Used by `implement_interface_methods_at` to look up
/// the method list a class implements.
fn find_interface_decl(prog: &Program, name: AstSymbol) -> Option<&InterfaceDecl> {
    for it in &prog.items {
        match it {
            Item::Interface(i) if i.name == name => return Some(i),
            Item::ExternC(b) => {
                for iface in b.interfaces.iter() {
                    if iface.name == name {
                        return Some(iface);
                    }
                }
            }
            _ => {}
        }
    }
    None
}

/// Collect every interface declaration named in `cls`'s base list.
/// The parser puts the first base name into `parent` regardless of
/// whether it's a class or interface, so check both `parent` and
/// `interfaces`. Local and external interface registries are tried in
/// turn; cross-module references (`use cocoa { NSApplicationDelegate }`)
/// resolve through `external_interfaces`. Returns an empty vec when
/// the class implements no known interface.
fn collect_base_interface_decls<'a>(
    cls: &ClassDecl,
    prog: &'a Program,
    external_interfaces: &'a HashMap<AstSymbol, InterfaceDecl>,
) -> Vec<&'a InterfaceDecl> {
    let mut out: Vec<&InterfaceDecl> = Vec::new();
    let bases = cls.parent.iter().copied().chain(cls.interfaces.iter().copied());
    for b in bases {
        if let Some(decl) = find_interface_decl(prog, b) {
            out.push(decl);
        } else if let Some(decl) = external_interfaces.get(&b) {
            out.push(decl);
        }
    }
    out
}

/// Enumerate every interface method `cls` doesn't yet implement,
/// paired with the interface it was declared in (for callers that
/// want to render an "interface X — implement" detail string).
/// Skips both methods already on the class and duplicates across
/// multiple base interfaces (first-listed wins, so two protocols
/// declaring `controlTextDidChange` don't yield two stubs).
fn enumerate_missing_methods<'a>(
    cls: &ClassDecl,
    iface_decls: &[&'a InterfaceDecl],
) -> Vec<(&'a InterfaceDecl, &'a InterfaceMethod)> {
    let mut existing: HashSet<&str> = HashSet::new();
    for m in cls.methods.iter().chain(cls.static_methods.iter()) {
        existing.insert(m.name.as_str());
    }
    let mut seen: HashSet<&str> = HashSet::new();
    let mut out: Vec<(&InterfaceDecl, &InterfaceMethod)> = Vec::new();
    for iface in iface_decls {
        for m in iface.methods.iter() {
            let n = m.name.as_str();
            if existing.contains(n) {
                continue;
            }
            if !seen.insert(n) {
                continue;
            }
            out.push((iface, m));
        }
    }
    out
}

/// Render `pub name(params): ret` (the part both quick-fix paths
/// emit verbatim), without the trailing body braces or any
/// indentation — callers append the body themselves to control
/// whitespace / snippet stops.
fn format_method_header(m: &InterfaceMethod) -> String {
    let params: Vec<String> = m
        .params
        .iter()
        .map(|p| format!("{}: {}", p.name.as_str(), p.ty))
        .collect();
    let ret = match &m.ret {
        Some(t) => format!(": {t}"),
        None => String::new(),
    };
    format!("pub {}({}){}", m.name.as_str(), params.join(", "), ret)
}

/// Pick a sensible default value literal for a return-typed
/// interface-method stub. Returns `None` for types where no
/// default makes sense (object refs, arrays, optionals, etc.) —
/// those leave the body without a tail expression, which the
/// compiler then flags so the user fills it in.
fn default_value_for(ret: &Type) -> Option<&'static str> {
    match ret {
        Type::Bool => Some("false"),
        Type::I8 | Type::I16 | Type::I32 | Type::I64 => Some("0"),
        Type::U8 | Type::U16 | Type::U32 | Type::U64 => Some("0"),
        Type::F32 | Type::F64 => Some("0.0"),
        Type::Str => Some("\"\""),
        Type::Unit => None,
        _ => None,
    }
}

/// Per-method completion entries for interface methods that the
/// enclosing class hasn't yet implemented. Used by the bare-ident
/// completion path inside a class body — typing `app` inside
/// `class MyApp : NSApplicationDelegate { … }` should surface
/// `applicationDidFinishLaunching` etc. as one-tap stubs.
///
/// Each entry inserts a complete `pub <name>(<params>): <ret> {
/// // TODO ; <default> }` snippet that mirrors what
/// `implement_interface_methods_at` would emit for that one
/// method.
#[allow(dead_code)]
pub(crate) fn interface_method_stub_completions_at(
    text: &str,
    prog: &Program,
    external_interfaces: &HashMap<AstSymbol, InterfaceDecl>,
    cursor: Position,
) -> Vec<(String, Option<String>, String)> {
    // Returns (label, detail, snippet) triples. Caller converts
    // into CompletionItem so we don't drag the lsp_types here.
    let mut out: Vec<(String, Option<String>, String)> = Vec::new();
    let Some(cursor_byte) =
        text::line_col_to_offset(text, cursor.line + 1, cursor.character + 1)
    else {
        return out;
    };
    // Find the innermost class containing the cursor.
    let class_ranges = prog.items.iter().filter_map(|it| {
        let Item::Class(c) = it else { return None };
        let (open, close) = match_brace_range(text, c.span)?;
        Some((c, open, close))
    });
    let Some((cls, _open, _close)) = pick_innermost_containing(class_ranges, cursor_byte) else {
        return out;
    };

    let iface_decls = collect_base_interface_decls(cls, prog, external_interfaces);
    if iface_decls.is_empty() {
        return out;
    }
    for (iface, m) in enumerate_missing_methods(cls, &iface_decls) {
        let name = m.name.as_str();
        // LSP snippet syntax: `$0` is the final cursor stop. No
        // indentation: the editor inserts at cursor and re-indents.
        let mut snippet = format_method_header(m);
        snippet.push_str(" {\n    $0");
        if let Some(ret) = &m.ret {
            if let Some(default) = default_value_for(ret) {
                snippet.push('\n');
                snippet.push_str("    ");
                snippet.push_str(default);
            }
        }
        snippet.push_str("\n}");
        let detail = Some(format!(
            "{} {}{}",
            if m.is_optional { "optional" } else { "required" },
            iface.name.as_str(),
            if m.is_optional { "" } else { " — implement" }
        ));
        out.push((name.to_string(), detail, snippet));
    }
    out
}

/// Text-based variant of `interface_method_stub_completions_at`.
/// Doesn't need a parsed Program — scans the buffer to find the
/// enclosing `class NAME : A, B, … {` header, extracts the base
/// names, looks each up in the supplied local + external
/// interface maps, and emits one snippet per unimplemented method.
/// Lets the bare-ident completion path keep firing while the
/// buffer is mid-edit (and therefore probably doesn't parse).
pub(crate) fn interface_method_stub_completions_textual(
    text: &str,
    cursor_byte: usize,
    local_interfaces: &HashMap<AstSymbol, InterfaceDecl>,
    external_interfaces: &HashMap<AstSymbol, InterfaceDecl>,
) -> Vec<(String, Option<String>, String)> {
    let mut out = Vec::new();
    let Some((bases, body_start, body_end)) =
        enclosing_class_header(text, cursor_byte)
    else {
        return out;
    };

    // Collect interface decls referenced in the base list.
    let mut iface_decls: Vec<&InterfaceDecl> = Vec::new();
    for b in &bases {
        let sym = AstSymbol::intern(b);
        if let Some(d) = local_interfaces.get(&sym) {
            iface_decls.push(d);
        } else if let Some(d) = external_interfaces.get(&sym) {
            iface_decls.push(d);
        }
    }
    if iface_decls.is_empty() {
        return out;
    }

    // Existing methods in the class body, harvested by text scan
    // (regex-ish: lines containing `pub <name>(` / `<name>(` at
    // the start, ignoring whitespace).
    let existing = scan_class_method_names(&text[body_start..body_end]);

    let mut seen: HashSet<&str> = HashSet::new();
    for iface in iface_decls {
        for m in iface.methods.iter() {
            let name = m.name.as_str();
            if existing.contains(name) {
                continue;
            }
            if !seen.insert(name) {
                continue;
            }
            let params: Vec<String> = m
                .params
                .iter()
                .map(|p| format!("{}: {}", p.name.as_str(), p.ty))
                .collect();
            let mut snippet = String::new();
            snippet.push_str("pub ");
            snippet.push_str(name);
            snippet.push('(');
            snippet.push_str(&params.join(", "));
            snippet.push(')');
            if let Some(ret) = &m.ret {
                snippet.push_str(": ");
                snippet.push_str(&format!("{ret}"));
            }
            snippet.push_str(" {\n    $0");
            if let Some(ret) = &m.ret {
                if let Some(default) = default_value_for(ret) {
                    snippet.push('\n');
                    snippet.push_str("    ");
                    snippet.push_str(default);
                }
            }
            snippet.push_str("\n}");
            let detail = Some(format!(
                "{} {}",
                if m.is_optional { "optional" } else { "required" },
                iface.name.as_str(),
            ));
            out.push((name.to_string(), detail, snippet));
        }
    }
    out
}

/// Walk back from `cursor` in `text` to find the innermost
/// `class NAME : A, B { … }` whose body brackets the cursor.
/// Returns the comma-separated base list, the body's open-brace
/// byte, and its close-brace byte. The close brace may not yet
/// exist in the buffer (the user is mid-typing) — in that case
/// we treat EOF as the closing brace.
fn enclosing_class_header(
    text: &str,
    cursor: usize,
) -> Option<(Vec<String>, usize, usize)> {
    let bytes = text.as_bytes();
    let end = cursor.min(bytes.len());

    // Find the `{` that opens the enclosing block by tracking
    // brace depth backward from `cursor`. The first `{` we
    // un-balance (i.e. extra opens over closes) is the enclosing
    // one.
    let mut depth: i32 = 0;
    let mut open: Option<usize> = None;
    let mut i = end;
    while i > 0 {
        i -= 1;
        match bytes[i] {
            b'}' => depth += 1,
            b'{' => {
                if depth == 0 {
                    open = Some(i);
                    break;
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    let open = open?;

    // Walk forward from `open` to find the matching `}` (or use
    // EOF if absent). Tolerant of unbalanced braces inside the
    // body (user mid-typing) by capping at the next outermost
    // close.
    let mut depth = 0i32;
    let mut close = bytes.len();
    let mut j = open;
    while j < bytes.len() {
        match bytes[j] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    close = j;
                    break;
                }
            }
            _ => {}
        }
        j += 1;
    }
    if cursor > close {
        return None;
    }

    // Walk back from `open` to find the `class NAME : …` header.
    // Skip whitespace, then expect either a bare ident (the
    // class name, with no base list) or `BASE_LIST class NAME`.
    let mut k = open;
    while k > 0 && matches!(bytes[k - 1], b' ' | b'\t' | b'\n') {
        k -= 1;
    }
    // Collect bases until we hit `class NAME :` or some other
    // sentinel. The bytes between `class NAME : ` and the brace
    // are the comma-separated base names.
    let header_end = k;
    let mut header_start = k;
    let mut found_class_kw = false;
    while header_start > 0 {
        let b = bytes[header_start - 1];
        if b == b'\n' {
            // Step over the newline; the class header may span
            // multiple lines.
            header_start -= 1;
            continue;
        }
        header_start -= 1;
        // Bail when we hit any other top-level closing brace —
        // that means we're back in a sibling block, not a class
        // declaration.
        if b == b'}' || b == b';' {
            break;
        }
        if b == b'{' {
            return None;
        }
        // Detect the `class` keyword by looking for a 5-char
        // window matching "class" preceded by whitespace.
        if header_start + 5 <= bytes.len()
            && &bytes[header_start..header_start + 5] == b"class"
            && (header_start == 0
                || matches!(
                    bytes[header_start - 1],
                    b' ' | b'\t' | b'\n' | b'{' | b'}' | b';'
                ))
        {
            found_class_kw = true;
            break;
        }
    }
    if !found_class_kw {
        return None;
    }

    let header_text = std::str::from_utf8(&bytes[header_start..header_end]).ok()?;
    // Header looks like `class NAME : A, B, C` (or
    // `class NAME` with no base list). Walk past the name
    // character-by-character so we don't lose the `:` to a
    // separator-consuming split — `class AA:` (no space) must
    // parse the same as `class AA :`.
    let after_class = header_text.strip_prefix("class")?.trim_start();
    let name_len = after_class
        .find(|c: char| !c.is_alphanumeric() && c != '_')
        .unwrap_or(after_class.len());
    let after_name = &after_class[name_len..];
    let after_colon = after_name.trim_start().strip_prefix(':').unwrap_or("");
    let bases: Vec<String> = after_colon
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    Some((bases, open, close))
}

/// Harvest already-implemented method names from a class body
/// chunk of text. Looks for `pub NAME(` / `NAME(` / `static NAME(`
/// at indent boundaries. Best-effort — false positives (e.g. a
/// call to a function inside a method body) just hide a
/// completion candidate that wouldn't really be missing anyway.
fn scan_class_method_names(body: &str) -> HashSet<String> {
    let mut out: HashSet<String> = HashSet::new();
    let bytes = body.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // Find start of a line (skip leading whitespace).
        while i < bytes.len() && matches!(bytes[i], b' ' | b'\t') {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        // Skip `pub` / `static` keywords.
        let mut j = i;
        loop {
            while j < bytes.len()
                && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_')
            {
                j += 1;
            }
            let kw = &bytes[i..j];
            if kw == b"pub" || kw == b"static" || kw == b"async" || kw == b"override" {
                while j < bytes.len() && matches!(bytes[j], b' ' | b'\t') {
                    j += 1;
                }
                i = j;
                continue;
            }
            break;
        }
        // Now bytes[i..j] should be the ident; check for `(` next.
        if j > i {
            let mut k = j;
            while k < bytes.len() && matches!(bytes[k], b' ' | b'\t') {
                k += 1;
            }
            if k < bytes.len() && bytes[k] == b'(' {
                if let Ok(name) = std::str::from_utf8(&bytes[i..j]) {
                    out.insert(name.to_string());
                }
            }
        }
        // Advance past current line.
        while i < bytes.len() && bytes[i] != b'\n' {
            i += 1;
        }
        if i < bytes.len() {
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use ilang_lexer::tokenize;
    use ilang_parser::parse;

    fn pos(line: u32, col: u32) -> Position {
        Position { line, character: col }
    }

    fn run_iface(src: &str, cursor: Position) -> Option<String> {
        let toks = tokenize(src).ok()?;
        let prog = parse(&toks).ok()?;
        let empty: HashMap<AstSymbol, InterfaceDecl> = HashMap::new();
        let (insert, new_text, _) =
            implement_interface_methods_at(src, &prog, &empty, cursor)?;
        let mut out = src.to_string();
        out.insert_str(insert, &new_text);
        Some(out)
    }

    #[test]
    fn implement_stubs_inserted_for_missing_methods() {
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
    fn implement_skips_existing_methods() {
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
    fn implement_returns_none_when_all_methods_present() {
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
    fn implement_returns_none_outside_class() {
        let src = "\
interface I { f() }
class D : I { pub init() {} pub f() {} }
fn outside() {}
";
        // cursor on the outside fn
        assert!(run_iface(src, pos(2, 0)).is_none());
    }

    #[test]
    fn interface_method_completion_emits_snippets() {
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
        let toks = tokenize(src).unwrap();
        let prog = parse(&toks).unwrap();
        let empty: HashMap<AstSymbol, InterfaceDecl> = HashMap::new();
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
    fn interface_method_completion_textual_works_mid_edit() {
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
        let ext_toks = tokenize(ext_src).unwrap();
        let ext_prog = parse(&ext_toks).unwrap();
        let mut locals: HashMap<AstSymbol, InterfaceDecl> = HashMap::new();
        for it in &ext_prog.items {
            if let Item::Interface(i) = it {
                locals.insert(i.name, i.clone());
            }
        }
        let empty: HashMap<AstSymbol, InterfaceDecl> = HashMap::new();
        // Cursor at line 6 col 6, right after `he` partial ident.
        let off = crate::text::line_col_to_offset(src, 7, 7).expect("offset");
        let stubs =
            interface_method_stub_completions_textual(src, off, &locals, &empty);
        assert_eq!(stubs.len(), 2, "stubs: {:?}", stubs);
        let labels: Vec<&str> = stubs.iter().map(|(l, _, _)| l.as_str()).collect();
        assert!(labels.contains(&"hello"));
        assert!(labels.contains(&"bye"));
    }

    #[test]
    fn implement_works_for_inline_empty_class_body() {
        // `class MyApp : NSApplicationDelegate {}` on a single
        // line with empty body — same shape the user reports.
        let local_src = "class MyApp : MyDel {}\n";
        let toks = tokenize(local_src).unwrap();
        let local_prog = parse(&toks).unwrap();

        let ext_src = "\
interface MyDel {
    notifyMe(name: i64)
}
";
        let ext_toks = tokenize(ext_src).unwrap();
        let ext_prog = parse(&ext_toks).unwrap();
        let mut ext_ifaces: HashMap<AstSymbol, InterfaceDecl> = HashMap::new();
        for it in &ext_prog.items {
            if let Item::Interface(i) = it {
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
    fn implement_uses_external_interfaces_for_cross_module() {
        // Simulate `use cocoa { MyDel }` where MyDel lives in
        // another file: the local buffer has no `interface MyDel`
        // visible, but `external_interfaces` carries the decl
        // populated by the loader. The code action should fall
        // back to that map.
        let local_src = "\
class MyApp : MyDel {
}
";
        let toks = tokenize(local_src).unwrap();
        let local_prog = parse(&toks).unwrap();

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
        let ext_toks = tokenize(ext_src).unwrap();
        let ext_prog = parse(&ext_toks).unwrap();
        let mut ext_ifaces: HashMap<AstSymbol, InterfaceDecl> = HashMap::new();
        for it in &ext_prog.items {
            if let Item::ExternC(b) = it {
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
    fn implement_fires_when_cursor_on_class_header_line() {
        // VSCode's lightbulb often anchors on the class header
        // line, not the body. The action must still fire when the
        // cursor sits on `class NAME : Base` rather than between
        // the braces.
        let local_src = "class AA : MyDel {\n}\n";
        let toks = tokenize(local_src).unwrap();
        let local_prog = parse(&toks).unwrap();

        let ext_src = "\
interface MyDel {
    notifyMe(name: i64)
}
";
        let ext_toks = tokenize(ext_src).unwrap();
        let ext_prog = parse(&ext_toks).unwrap();
        let mut ext_ifaces: HashMap<AstSymbol, InterfaceDecl> = HashMap::new();
        for it in &ext_prog.items {
            if let Item::Interface(i) = it {
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
    fn implement_fires_for_objc_class_inside_extern_block() {
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
        let toks = tokenize(local_src).unwrap();
        let local_prog = parse(&toks).unwrap();

        let ext_src = "\
@extern(ObjC) {
    @objc interface MyDel {
        notifyMe(name: i64)
    }
}
";
        let ext_toks = tokenize(ext_src).unwrap();
        let ext_prog = parse(&ext_toks).unwrap();
        let mut ext_ifaces: HashMap<AstSymbol, InterfaceDecl> = HashMap::new();
        for it in &ext_prog.items {
            if let Item::ExternC(b) = it {
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
    fn textual_completion_resolves_dotted_external_iface() {
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
        let ext_toks = tokenize(ext_src).unwrap();
        let ext_prog = parse(&ext_toks).unwrap();
        let mut ext_ifaces: HashMap<AstSymbol, InterfaceDecl> = HashMap::new();
        for it in &ext_prog.items {
            if let Item::ExternC(b) = it {
                for i in b.interfaces.iter() {
                    ext_ifaces.insert(i.name, i.clone());
                    ext_ifaces.insert(AstSymbol::intern("cocoa.NSMenuDelegate"), i.clone());
                }
            }
        }
        let empty: HashMap<AstSymbol, InterfaceDecl> = HashMap::new();
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
}
