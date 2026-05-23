//! Extracted from `main.rs`.
#![allow(unused_imports)]

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};



use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

use ilang_ast::{
    Block, ClassDecl, EnumDecl, Expr, ExprKind, FnDecl, Item, Param, Pattern, PatternBindings,
    PatternKind, Program, Span, Stmt, StmtKind, Symbol as AstSymbol, Type, VariantPayload,
};
use ilang_parser::parse as parse_program;
use ilang_types::{check, TypeError};

use crate::*;

/// Walk past `@attr` lines (and their `@attr(args)` form) that
/// `render_user_attrs` emits at the top of a signature, returning
/// the slice starting at the first real content line. A line is
/// treated as a pure-attr line only when it's `@name` /
/// `@name(...)` and nothing else — `@objc interface Foo` keeps
/// its leading `@objc` because the kind classifier needs that
/// structural prefix.
pub(crate) fn sig_body_skip_attrs(sig: &str) -> &str {
    let mut rest = sig;
    loop {
        let line_end = rest.find('\n').unwrap_or(rest.len());
        let first = rest[..line_end].trim_end();
        if !first.starts_with('@') {
            return rest;
        }
        // Skip past `@name` / `@name(...)`. If there's any non-paren
        // content after that, this isn't a pure attr line — leave it.
        let after_at = &first[1..];
        let name_end = after_at
            .find(|c: char| !(c.is_alphanumeric() || c == '_'))
            .unwrap_or(after_at.len());
        let mut tail = &after_at[name_end..];
        if tail.starts_with('(') {
            // Find the matching closing paren on the same line.
            let close = match tail.find(')') {
                Some(p) => p,
                None => return rest,
            };
            tail = &tail[close + 1..];
        }
        if !tail.trim().is_empty() {
            // Structural prefix like `@objc interface Foo` — content
            // begins here.
            return rest;
        }
        if line_end < rest.len() {
            rest = &rest[line_end + 1..];
        } else {
            return "";
        }
    }
}

/// Render the `@handle` / `@packed` markers that prefix an
/// `@extern(C)` struct declaration. Each marker lands on its own
/// line followed by a newline, so the hover shows
/// `@handle\nstruct Name` instead of cramming them onto one line.
/// Returns an empty string when no markers are set — drop-in for
/// `format!("{attrs}struct {name}")`.
pub(crate) fn render_struct_attrs(is_packed: bool, is_handle: bool) -> String {
    let mut out = String::new();
    if is_packed {
        out.push_str("@packed\n");
    }
    if is_handle {
        out.push_str("@handle\n");
    }
    out
}

/// Render a class declaration's inheritance list as a hover suffix.
/// `parent` is the single inherited base class (if any); `interfaces`
/// is the trailing `: P, I1, I2` slot for the interface list.
/// Returns the empty string when there are no bases — otherwise
/// `" : <bases>"` ready to concatenate onto a `class Name` line.
pub(crate) fn render_class_bases(
    parent: Option<&AstSymbol>,
    interfaces: &[AstSymbol],
) -> String {
    let mut bases: Vec<&str> = Vec::new();
    if let Some(p) = parent {
        bases.push(p.as_str());
    }
    for i in interfaces {
        bases.push(i.as_str());
    }
    if bases.is_empty() {
        String::new()
    } else {
        format!(" : {}", bases.join(", "))
    }
}

/// Extract the dotted (or bare) name an `Var` carries when it stands
/// in for an enum receiver — e.g. `Var("InitFlag")` →
/// `Some("InitFlag")`, `Var("sdl.InitFlag")` → `Some("sdl.InitFlag")`.
/// Returns `None` for anything that isn't a plain `Var`.
pub(crate) fn enum_obj_name(obj: &Expr) -> Option<String> {
    match &obj.kind {
        ExprKind::Var(name) => Some(name.as_str().to_string()),
        _ => None,
    }
}

/// Render the hover signature shown on `new Foo(...)`. Prefer the
/// first `init(...)` line alone (TypeScript-style constructor hover),
/// with a `(+N overload[s])` tail when the class has multiple init
/// signatures. Falls back to `class Foo` for classes without init.
pub(crate) fn class_hover(class: &str, info: &ClassInfo) -> String {
    if let Some(init) = info.methods.get(&"init".into()) {
        let extras = info.init_overloads.saturating_sub(1);
        let mut out = init.signature.clone();
        if extras == 1 {
            out.push_str(" (+1 overload)");
        } else if extras > 1 {
            out.push_str(&format!(" (+{extras} overloads)"));
        }
        out
    } else {
        format!("{} {class}", info.kind.keyword())
    }
}

/// Resolve the F12 target for a class member reference. Returns
/// `(span, no_definition, target_uri)`.
/// - Buffer-local: span is the member's own span, no URI.
/// - External + source file known: span is the member's span (the
///   file's own coordinates), URI is the source file.
/// - External, no source: no_definition = true; cursor stays put.
pub(crate) fn member_target(
    m: &MemberInfo,
    info: &ClassInfo,
    class_name: &str,
    sources: &ExternalSources,
    use_line: u32,
    use_col: u32,
) -> (Span, bool, Option<Url>) {
    if info.external {
        if let Some(loc) = sources.get(&AstSymbol::intern(class_name)) {
            if let Ok(uri) = Url::from_file_path(&loc.path) {
                return (m.span, false, Some(uri));
            }
        }
        (Span::new(use_line, use_col), true, None)
    } else {
        (m.span, false, None)
    }
}

pub(crate) fn type_to_class(t: &Type) -> Option<String> {
    match t {
        Type::Object(n) => Some(n.as_str().to_string()),
        Type::Generic(g) => Some(g.base.as_str().to_string()),
        _ => None,
    }
}

pub(crate) fn bind_pattern(p: &Pattern, scope: &mut Vec<Binding>) {
    match &p.kind {
        PatternKind::Wildcard
        | PatternKind::IntLit(_)
        | PatternKind::IntRange { .. }
        | PatternKind::BoolLit(_)
        | PatternKind::StrLit(_) => {}
        PatternKind::Variant { bindings, .. } => match bindings {
            PatternBindings::Unit => {}
            // The AST stores binding names as bare strings (no per-name
            // spans), so we register them under the pattern's span. F12
            // on the binding will land on the pattern itself rather
            // than the precise identifier.
            PatternBindings::Tuple(names) => {
                for n in names {
                    if n != "_" {
                        scope.push(Binding {
                            name: n.as_str().to_string(),
                            span: p.span,
                            ty: None,
                            kind: BindKind::Pattern,
                            override_signature: None,
                        });
                    }
                }
            }
            PatternBindings::Struct(pairs) => {
                for (_, alias) in pairs {
                    scope.push(Binding {
                        name: alias.as_str().to_string(),
                        span: p.span,
                        ty: None,
                        kind: BindKind::Pattern,
                        override_signature: None,
                    });
                }
            }
        },
    }
}

/// Quick-and-dirty type inference used only for hover / `obj.field`
/// resolution. Covers the cases the type checker has already validated;
/// anything we can't pin down yields `None`.
/// Best-effort type inference used for hover and `obj.field` class
/// resolution. Falls back to the simpler scope-less variant when no
/// scope is available.
pub(crate) fn infer_expr_type_with_scope(e: &Expr, scope: &[Binding]) -> Option<Type> {
    if let ExprKind::FnExpr { params, ret, .. } = &e.kind {
        let ps = params.iter().map(|p| p.ty.clone()).collect();
        let r = ret.clone().unwrap_or(Type::Unit);
        return Some(Type::func(ps, r));
    }
    use ilang_ast::BinOp;
    match &e.kind {
        ExprKind::Int(_) => Some(Type::I64),
        ExprKind::Float(_) => Some(Type::F64),
        ExprKind::Bool(_) => Some(Type::Bool),
        ExprKind::Str(_) => Some(Type::Str),
        ExprKind::Var(name) => scope
            .iter()
            .rev()
            .find(|b| b.name == name.as_str())
            .and_then(|b| b.ty.clone()),
        ExprKind::New { class, type_args, .. } => {
            if type_args.is_empty() {
                Some(Type::Object(class.clone()))
            } else {
                Some(Type::generic(class.clone(), type_args.to_vec()))
            }
        }
        ExprKind::Cast { ty, .. } => Some(ty.clone()),
        // Comparison / logical produce bool. For arithmetic / bitwise,
        // mirror the type checker's literal-adoption rule: a known
        // typed operand wins over a bare integer / float literal on the
        // other side, so `i32_var % 10` infers as i32 (not i64).
        ExprKind::Binary { op, lhs, rhs } => match op {
            BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                Some(Type::Bool)
            }
            _ => {
                let lt = infer_expr_type_with_scope(lhs, scope);
                let rt = infer_expr_type_with_scope(rhs, scope);
                match (lt, rt) {
                    (Some(l), Some(r)) => Some(promote_pair(&l, &r, lhs, rhs)),
                    (Some(t), None) | (None, Some(t)) => Some(t),
                    (None, None) => None,
                }
            }
        },
        ExprKind::Logical { .. } => Some(Type::Bool),
        ExprKind::Unary { op, expr } => match op {
            ilang_ast::UnOp::Not => Some(Type::Bool),
            _ => infer_expr_type_with_scope(expr, scope),
        },
        _ => None,
    }
}

/// Pick which operand's type wins for a binary numeric op. Bare integer
/// or float literals defer to the other side when the other side has a
/// concrete narrower / wider numeric type — same shape as the type
/// checker's `numeric_literal_fits` adoption.
pub(crate) fn promote_pair(l: &Type, r: &Type, l_expr: &Expr, r_expr: &Expr) -> Type {
    let l_is_lit = matches!(l_expr.kind, ExprKind::Int(_) | ExprKind::Float(_));
    let r_is_lit = matches!(r_expr.kind, ExprKind::Int(_) | ExprKind::Float(_));
    if l_is_lit && !r_is_lit && r.is_numeric() {
        return r.clone();
    }
    if r_is_lit && !l_is_lit && l.is_numeric() {
        return l.clone();
    }
    l.clone()
}

/// Walk a `loop` body looking for the first `break v` and infer the
/// type of `v`. `break` without a value yields `Unit`. Doesn't descend
/// into nested loops (their `break`s belong to the inner loop).
pub(crate) fn find_break_type(
    block: &Block,
    scope: &[Binding],
    walker: &Walker,
    out: &mut Option<Type>,
) {
    for s in &block.stmts {
        if out.is_some() {
            return;
        }
        if let StmtKind::Expr(e) = &s.kind {
            scan_break(e, scope, walker, out);
        }
    }
    if out.is_none() {
        if let Some(t) = &block.tail {
            scan_break(t, scope, walker, out);
        }
    }
}

pub(crate) fn scan_break(
    e: &Expr,
    scope: &[Binding],
    walker: &Walker,
    out: &mut Option<Type>,
) {
    if out.is_some() {
        return;
    }
    match &e.kind {
        ExprKind::Break(v) => {
            *out = match v {
                Some(inner) => walker.infer_expr(inner, scope).or(Some(Type::Unit)),
                None => Some(Type::Unit),
            };
        }
        ExprKind::Loop { .. } => {
            // Inner loops swallow their own breaks — skip.
        }
        ExprKind::If { then_branch, else_branch, .. } => {
            find_break_type(then_branch, scope, walker, out);
            if let Some(eb) = else_branch {
                if out.is_none() {
                    scan_break(eb, scope, walker, out);
                }
            }
        }
        ExprKind::Block(b) => find_break_type(b, scope, walker, out),
        ExprKind::While { body, .. } | ExprKind::ForIn { body, .. } => {
            find_break_type(body, scope, walker, out);
        }
        ExprKind::Match { arms, .. } => {
            for a in arms {
                if out.is_some() {
                    break;
                }
                // Same scoping rule as `infer_expr` for Match: a
                // `break <pattern-bound>` inside an arm needs the
                // pattern's bindings visible to infer the break type.
                let mut arm_scope = scope.to_vec();
                bind_pattern(&a.pattern, &mut arm_scope);
                scan_break(&a.body, &arm_scope, walker, out);
            }
        }
        _ => {}
    }
}

/// Render a `const` initializer back to a short source-like string for
/// hover. Covers primitive literals and a leading unary `-` / `+`; more
/// complex expressions fall back to `None` so we don't print noise.
pub(crate) fn render_const_value(e: &Expr) -> Option<String> {
    render_const_value_with_src(e, None)
}

/// Same as `render_const_value` but, when source is provided,
/// preserves the literal text the user wrote for `Int` / `Float`
/// (so `const A: i32 = 0x123` keeps the hex / underscore form on
/// hover instead of being collapsed to decimal). Falls back to
/// the parsed value when the source slice can't be lifted.
pub(crate) fn render_const_value_with_src(e: &Expr, src: Option<&str>) -> Option<String> {
    match &e.kind {
        ExprKind::Int(n) => src
            .and_then(|s| literal_token_at(s, e.span))
            .or(Some(n.to_string())),
        ExprKind::Float(f) => src
            .and_then(|s| literal_token_at(s, e.span))
            .or(Some(f.to_string())),
        ExprKind::Bool(b) => Some(b.to_string()),
        ExprKind::Str(s) => Some(format!("{s:?}")),
        ExprKind::Unary { op, expr } => {
            let inner = render_const_value_with_src(expr, src)?;
            let sym = match op {
                UnOp::Neg => "-",
                UnOp::Pos => "+",
                UnOp::Not => "!",
                UnOp::BitNot => "~",
                UnOp::AddrOf => "&",
            };
            Some(format!("{sym}{inner}"))
        }
        // Non-literal initializers (`const c: i64 = tt()`):
        // copy the source text out of the user's file via the
        // expression's span so hover shows `= tt()` rather
        // than nothing.
        _ => src.and_then(|s| expr_source_text(s, e.span)),
    }
}

/// Read the textual span covered by `span` from `src`. ilang
/// spans are inclusive on both ends. For call-shaped
/// expressions whose AST span only covers the callee (the
/// parser doesn't extend it across the parens / args), we
/// follow the source forward through balanced
/// `()` / `[]` / `{}` until the call's closing paren so the
/// hover shows the full `tt(args)` instead of just `tt`. Used
/// as the fallback for hover rendering of non-foldable const
/// initializers.
pub(crate) fn expr_source_text(src: &str, span: Span) -> Option<String> {
    let start = text::line_col_to_offset(src, span.line, span.col)?;
    let end_inclusive = text::line_col_to_offset(src, span.end_line, span.end_col)?;
    let mut end_exclusive = end_inclusive.saturating_add(1).min(src.len());
    // Extend across an immediately-following balanced paren run.
    let bytes = src.as_bytes();
    if end_exclusive < bytes.len() && bytes[end_exclusive] == b'(' {
        let mut depth = 0i32;
        let mut i = end_exclusive;
        let mut in_str = false;
        let mut esc = false;
        while i < bytes.len() {
            let c = bytes[i];
            if in_str {
                if esc { esc = false; }
                else if c == b'\\' { esc = true; }
                else if c == b'"' { in_str = false; }
            } else {
                match c {
                    b'"' => in_str = true,
                    b'(' | b'[' | b'{' => depth += 1,
                    b')' | b']' | b'}' => {
                        depth -= 1;
                        if depth == 0 {
                            i += 1;
                            break;
                        }
                    }
                    b'\n' => break,
                    _ => {}
                }
            }
            i += 1;
        }
        end_exclusive = i;
    }
    if end_exclusive <= start {
        return None;
    }
    src.get(start..end_exclusive).map(|s| s.to_string())
}

