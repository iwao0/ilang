//! LSP "code action" entry points:
//!
//! - `fill_match_arms_at`: cursor in a `match` whose scrutinee is an
//!   enum → emit one new arm per missing variant.
//! - `generate_init_at`: cursor inside a `class` body that has
//!   fields but no `init` → emit a constructor that takes one param
//!   per field and assigns each to `this.field`.
//!
//! Plus the shared `collect_matches_in_*` walker that records every
//! `match` expression's `{ ... }` byte range, used by
//! `fill_match_arms_at`.

use std::collections::{HashMap, HashSet};

use ilang_ast::{
    Block, ClassDecl, Expr, ExprKind, ExternCItem, InterfaceDecl, InterfaceMethod, Item,
    PatternKind, Program, Span, StmtKind, Symbol as AstSymbol, Type, VariantPayload,
};
use ilang_lexer::tokenize;
use ilang_parser::parse;
use tower_lsp::lsp_types::{
    CodeAction, CodeActionKind, CodeActionOrCommand, CodeActionParams, CodeActionResponse,
    Position, Range, TextEdit, Url, WorkspaceEdit,
};

use super::imports::organize_imports;
use super::infer_expr_type_with_scope;
use super::text::{self, line_start_before};
use super::text_utils::{byte_range_to_lsp_range, byte_to_position};
use super::walker::is_parser_synth_field;

/// From an iterator of `(item, lo, hi)` byte ranges, return the
/// innermost one whose `[lo..=hi]` contains `cursor_byte`. "Innermost"
/// is the smallest extent, mirroring how nested scopes shrink toward
/// the cursor. Returns `None` when nothing contains the cursor.
///
/// All four cursor-anchored quick-fixes (`fill_match_arms_at`,
/// `generate_init_at`, `implement_interface_methods_at`,
/// `interface_method_stub_completions_at`) used to inline this same
/// pick-smallest-containing loop; share it here.
fn pick_innermost_containing<T>(
    iter: impl IntoIterator<Item = (T, usize, usize)>,
    cursor_byte: usize,
) -> Option<(T, usize, usize)> {
    let mut chosen: Option<(T, usize, usize)> = None;
    for (item, lo, hi) in iter {
        if cursor_byte < lo || cursor_byte > hi {
            continue;
        }
        let extent = hi.saturating_sub(lo);
        match &chosen {
            None => chosen = Some((item, lo, hi)),
            Some((_, c_lo, c_hi)) => {
                if extent < c_hi.saturating_sub(*c_lo) {
                    chosen = Some((item, lo, hi));
                }
            }
        }
    }
    chosen
}

/// Find an enclosing `match` expression at `cursor` and, when its
/// scrutinee resolves to an enum declared in `prog`, return the byte
/// offset just before the closing `}` along with the source text to
/// insert (one new arm per missing variant) and the count of arms
/// added. Returns `None` when no completion is needed (no match,
/// non-enum scrutinee, wildcard arm present, all variants covered,
/// or unresolvable enum).
pub(crate) fn fill_match_arms_at(
    text: &str,
    prog: &Program,
    var_types: &HashMap<AstSymbol, Type>,
    cursor: Position,
) -> Option<(usize, String, usize)> {
    // Build a flat list of (match_expr, brace_open_byte, brace_close_byte)
    // for every match in the file.
    let mut all: Vec<(&Expr, usize, usize)> = Vec::new();
    for item in &prog.items {
        if let Item::Fn(f) = item {
            collect_matches_in_block(&f.body, text, &mut all);
        }
        if let Item::Class(c) = item {
            for m in c.methods.iter() {
                collect_matches_in_block(&m.body, text, &mut all);
            }
        }
    }
    // Pick innermost match whose `{ ... }` contains the cursor.
    let cursor_byte =
        text::line_col_to_offset(text, cursor.line + 1, cursor.character + 1)?;
    let (mexpr, _open, close) =
        pick_innermost_containing(all.iter().copied(), cursor_byte)?;
    let ExprKind::Match { scrutinee, arms } = &mexpr.kind else {
        return None;
    };
    // Bail if the user already has a wildcard arm — match is exhaustive.
    if arms
        .iter()
        .any(|a| matches!(a.pattern.kind, PatternKind::Wildcard))
    {
        return None;
    }
    let enum_name = scrutinee_enum_name(scrutinee, var_types)?;
    let edecl = prog.items.iter().find_map(|it| match it {
        Item::Enum(e) if e.name.as_str() == enum_name.as_str() => Some(e),
        _ => None,
    })?;
    // Variants already covered, by name.
    let mut covered: HashSet<String> = HashSet::new();
    for a in arms.iter() {
        if let PatternKind::Variant { variant, .. } = &a.pattern.kind {
            covered.insert(variant.as_str().to_string());
        }
    }
    let missing: Vec<&ilang_ast::Variant> = edecl
        .variants
        .iter()
        .filter(|v| !covered.contains(v.name.as_str()))
        .collect();
    if missing.is_empty() {
        return None;
    }
    // Indentation: copy the closing `}`'s line indent so each new
    // arm sits one level deeper.
    let close_line_start = line_start_before(text, close);
    let base_indent: String = text[close_line_start..close]
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .collect();
    let arm_indent = format!("{base_indent}    ");
    let mut out = String::new();
    for v in &missing {
        out.push_str(&arm_indent);
        out.push_str(enum_name.as_str());
        out.push('.');
        out.push_str(v.name.as_str());
        match &v.payload {
            VariantPayload::Unit => {}
            VariantPayload::Tuple(elems) => {
                out.push('(');
                let placeholders: Vec<&str> =
                    elems.iter().map(|_| "_").collect();
                out.push_str(&placeholders.join(", "));
                out.push(')');
            }
            VariantPayload::Struct(fields) => {
                out.push_str(" { ");
                let names: Vec<&str> =
                    fields.iter().map(|f| f.name.as_str()).collect();
                out.push_str(&names.join(", "));
                out.push_str(" }");
            }
        }
        out.push_str(" { todo() }\n");
    }
    Some((close_line_start, out, missing.len()))
}

/// Find the innermost `class` whose body `{...}` contains the cursor
/// and, when the class has fields but no `init` method, return the
/// byte offset and source text for an inserted constructor that
/// takes one parameter per field and assigns each to `this.field`.
/// Skips `@extern("...")` opaque-handle classes and `@extern(C)
/// struct` classes (init is rejected for both).
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

/// Recursively walk a block, recording every `match` expression's
/// `{ ... }` byte range (using brace-balance from the source text,
/// since `Match.span` covers only the `match` keyword).
fn collect_matches_in_block<'a>(
    block: &'a Block,
    text: &str,
    out: &mut Vec<(&'a Expr, usize, usize)>,
) {
    for s in &block.stmts {
        if let StmtKind::Expr(e) = &s.kind {
            collect_matches_in_expr(e, text, out);
        } else if let StmtKind::Let { value, .. } = &s.kind {
            collect_matches_in_expr(value, text, out);
        } else if let StmtKind::LetTuple { value, .. } = &s.kind {
            collect_matches_in_expr(value, text, out);
        } else if let StmtKind::LetStruct { value, .. } = &s.kind {
            collect_matches_in_expr(value, text, out);
        }
    }
    if let Some(t) = &block.tail {
        collect_matches_in_expr(t, text, out);
    }
}

fn collect_matches_in_expr<'a>(
    e: &'a Expr,
    text: &str,
    out: &mut Vec<(&'a Expr, usize, usize)>,
) {
    if let ExprKind::Match { scrutinee, arms } = &e.kind {
        if let Some((lo, hi)) = match_brace_range(text, e.span) {
            out.push((e, lo, hi));
        }
        collect_matches_in_expr(scrutinee, text, out);
        for a in arms.iter() {
            collect_matches_in_expr(&a.body, text, out);
        }
    }
    match &e.kind {
        ExprKind::Block(b) => collect_matches_in_block(b, text, out),
        ExprKind::If { cond, then_branch, else_branch } => {
            collect_matches_in_expr(cond, text, out);
            collect_matches_in_block(then_branch, text, out);
            if let Some(eb) = else_branch {
                collect_matches_in_expr(eb, text, out);
            }
        }
        ExprKind::While { cond, body } => {
            collect_matches_in_expr(cond, text, out);
            collect_matches_in_block(body, text, out);
        }
        ExprKind::Loop { body } => collect_matches_in_block(body, text, out),
        ExprKind::ForIn { iter, body, .. } => {
            collect_matches_in_expr(iter, text, out);
            collect_matches_in_block(body, text, out);
        }
        ExprKind::Call { args, .. } => {
            for a in args.iter() {
                collect_matches_in_expr(a, text, out);
            }
        }
        ExprKind::MethodCall { obj, args, .. } => {
            collect_matches_in_expr(obj, text, out);
            for a in args.iter() {
                collect_matches_in_expr(a, text, out);
            }
        }
        ExprKind::Binary { lhs, rhs, .. } => {
            collect_matches_in_expr(lhs, text, out);
            collect_matches_in_expr(rhs, text, out);
        }
        ExprKind::Logical { lhs, rhs, .. } => {
            collect_matches_in_expr(lhs, text, out);
            collect_matches_in_expr(rhs, text, out);
        }
        ExprKind::Unary { expr, .. } => collect_matches_in_expr(expr, text, out),
        ExprKind::Assign { value, .. } => {
            collect_matches_in_expr(value, text, out);
        }
        ExprKind::AssignField { obj, value, .. } => {
            collect_matches_in_expr(obj, text, out);
            collect_matches_in_expr(value, text, out);
        }
        ExprKind::AssignIndex { obj, index, value } => {
            collect_matches_in_expr(obj, text, out);
            collect_matches_in_expr(index, text, out);
            collect_matches_in_expr(value, text, out);
        }
        ExprKind::Index { obj, index } => {
            collect_matches_in_expr(obj, text, out);
            collect_matches_in_expr(index, text, out);
        }
        ExprKind::Field { obj, .. } => {
            collect_matches_in_expr(obj, text, out);
        }
        ExprKind::Cast { expr, .. } => collect_matches_in_expr(expr, text, out),
        ExprKind::TypeTest { expr, .. } => {
            collect_matches_in_expr(expr, text, out);
        }
        ExprKind::TypeDowncast { expr, .. } => {
            collect_matches_in_expr(expr, text, out);
        }
        ExprKind::Return(Some(v)) | ExprKind::Break(Some(v)) => {
            collect_matches_in_expr(v, text, out);
        }
        ExprKind::Some(v) => collect_matches_in_expr(v, text, out),
        ExprKind::Await(v) => collect_matches_in_expr(v, text, out),
        ExprKind::IfLet { expr, then_branch, else_branch, .. } => {
            collect_matches_in_expr(expr, text, out);
            collect_matches_in_block(then_branch, text, out);
            if let Some(eb) = else_branch {
                collect_matches_in_expr(eb, text, out);
            }
        }
        ExprKind::Array(items) | ExprKind::Tuple(items) => {
            for a in items.iter() {
                collect_matches_in_expr(a, text, out);
            }
        }
        ExprKind::StructLit { fields, .. } => {
            for (_, v) in fields.iter() {
                collect_matches_in_expr(v, text, out);
            }
        }
        ExprKind::MapLit(entries) => {
            for (k, v) in entries.iter() {
                collect_matches_in_expr(k, text, out);
                collect_matches_in_expr(v, text, out);
            }
        }
        ExprKind::Range { start, end, .. } => {
            if let Some(s) = start {
                collect_matches_in_expr(s, text, out);
            }
            if let Some(e2) = end {
                collect_matches_in_expr(e2, text, out);
            }
        }
        ExprKind::New { args, .. } => {
            for a in args.iter() {
                collect_matches_in_expr(a, text, out);
            }
        }
        ExprKind::EnumCtor { args, .. } => match args {
            ilang_ast::CtorArgs::Tuple(es) => {
                for a in es.iter() {
                    collect_matches_in_expr(a, text, out);
                }
            }
            ilang_ast::CtorArgs::Struct(fs) => {
                for (_, v) in fs.iter() {
                    collect_matches_in_expr(v, text, out);
                }
            }
            ilang_ast::CtorArgs::Unit => {}
        },
        ExprKind::FnExpr { body, .. } => collect_matches_in_block(body, text, out),
        ExprKind::Match { .. } => {} // already handled above
        _ => {}
    }
}

/// Given the span of a `match` keyword token, find the byte range
/// `[lo, hi]` of its block body, where `lo` is the byte offset of
/// the opening `{` and `hi` is the offset of the closing `}`.
fn match_brace_range(text: &str, match_kw: Span) -> Option<(usize, usize)> {
    let off = text::line_col_to_offset(text, match_kw.line, match_kw.col)?;
    let bytes = text.as_bytes();
    let mut i = off;
    let mut depth: i32 = 0;
    let mut open: Option<usize> = None;
    while i < bytes.len() {
        match bytes[i] {
            b'{' => {
                if open.is_none() {
                    open = Some(i);
                }
                depth += 1;
                i += 1;
            }
            b'}' => {
                depth -= 1;
                if depth == 0 && open.is_some() {
                    return Some((open.unwrap(), i));
                }
                i += 1;
            }
            b'"' => {
                // Skip string literal — match keyword can't appear inside.
                i += 1;
                while i < bytes.len() && bytes[i] != b'"' {
                    if bytes[i] == b'\\' && i + 1 < bytes.len() {
                        i += 2;
                    } else {
                        i += 1;
                    }
                }
                if i < bytes.len() {
                    i += 1;
                }
            }
            b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'/' => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            _ => i += 1,
        }
    }
    None
}

/// Resolve a match scrutinee to the user-defined name it carries
/// (enum or class — the type checker hasn't necessarily run, so a
/// bare `Object("Foo")` is accepted; the caller verifies that the
/// name resolves to an enum decl). `None` for non-named types.
fn scrutinee_enum_name(
    scrutinee: &Expr,
    var_types: &HashMap<AstSymbol, Type>,
) -> Option<AstSymbol> {
    let ty = match &scrutinee.kind {
        ExprKind::Var(name) => var_types.get(name).cloned(),
        _ => infer_expr_type_with_scope(scrutinee, &[]),
    };
    match ty? {
        Type::Enum(name) | Type::Object(name) => Some(name),
        _ => None,
    }
}

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

/// Orchestrate `textDocument/codeAction`. Tokenises + parses the
/// buffer once, then runs every quick-fix probe whose kind the
/// editor asked for. Caller is expected to have cloned the doc's
/// `text` / `var_types` / `external_interfaces` and dropped the
/// docs lock before calling — keeps parsing off the lock-held
/// critical path.
pub(crate) fn handle_code_action(
    p: &CodeActionParams,
    text: &str,
    var_types: &HashMap<AstSymbol, Type>,
    external_interfaces: &HashMap<AstSymbol, InterfaceDecl>,
) -> Option<CodeActionResponse> {
    let uri = &p.text_document.uri;
    let only = p.context.only.as_ref();
    let want_kind = |k: &CodeActionKind| match only {
        None => true,
        Some(kinds) => kinds.iter().any(|requested| {
            // Match on prefix — e.g. requesting "refactor" should
            // include "refactor.rewrite" too.
            let r = requested.as_str();
            let target = k.as_str();
            target == r || target.starts_with(&format!("{r}."))
        }),
    };
    let want_organize = want_kind(&CodeActionKind::SOURCE_ORGANIZE_IMPORTS)
        || want_kind(&CodeActionKind::SOURCE);
    let want_quickfix = want_kind(&CodeActionKind::QUICKFIX);
    if !want_organize && !want_quickfix {
        return None;
    }
    let tokens = tokenize(text).ok()?;
    let prog = parse(&tokens).ok()?;
    let mut actions: Vec<CodeActionOrCommand> = Vec::new();
    if want_organize {
        if let Some((start_byte, end_byte, new_text)) = organize_imports(text, &prog) {
            let range = byte_range_to_lsp_range(text, start_byte, end_byte);
            actions.push(quickfix_action(
                "Organize imports".into(),
                CodeActionKind::SOURCE_ORGANIZE_IMPORTS,
                uri,
                range,
                new_text,
                None,
            ));
        }
    }
    if want_quickfix {
        if let Some((insert_byte, new_text)) = generate_init_at(text, &prog, p.range.start) {
            let pos = byte_to_position(text, insert_byte);
            actions.push(quickfix_action(
                "Generate init from fields".into(),
                CodeActionKind::QUICKFIX,
                uri,
                Range { start: pos, end: pos },
                new_text,
                None,
            ));
        }
        if let Some((insert_byte, new_text, missing_count)) =
            fill_match_arms_at(text, &prog, var_types, p.range.start)
        {
            let pos = byte_to_position(text, insert_byte);
            let title = if missing_count == 1 {
                "Fill missing match arm".to_string()
            } else {
                format!("Fill {missing_count} missing match arms")
            };
            actions.push(quickfix_action(
                title,
                CodeActionKind::QUICKFIX,
                uri,
                Range { start: pos, end: pos },
                new_text,
                Some(true),
            ));
        }
        if let Some((insert_byte, new_text, missing_count)) =
            implement_interface_methods_at(text, &prog, external_interfaces, p.range.start)
        {
            let pos = byte_to_position(text, insert_byte);
            let title = if missing_count == 1 {
                "Implement missing interface method".to_string()
            } else {
                format!("Implement {missing_count} missing interface methods")
            };
            actions.push(quickfix_action(
                title,
                CodeActionKind::QUICKFIX,
                uri,
                Range { start: pos, end: pos },
                new_text,
                Some(true),
            ));
        }
    }
    if actions.is_empty() {
        None
    } else {
        Some(actions)
    }
}

fn quickfix_action(
    title: String,
    kind: CodeActionKind,
    uri: &Url,
    range: Range,
    new_text: String,
    is_preferred: Option<bool>,
) -> CodeActionOrCommand {
    let mut changes: HashMap<Url, Vec<TextEdit>> = HashMap::new();
    changes.insert(uri.clone(), vec![TextEdit { range, new_text }]);
    CodeActionOrCommand::CodeAction(CodeAction {
        title,
        kind: Some(kind),
        edit: Some(WorkspaceEdit {
            changes: Some(changes),
            document_changes: None,
            change_annotations: None,
        }),
        diagnostics: None,
        is_preferred,
        disabled: None,
        data: None,
        command: None,
    })
}
