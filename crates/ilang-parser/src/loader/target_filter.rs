//! Compile-time OS filter for `@target("os")` / `@target(not "os")`.
//!
//! Run on each file's parsed `Program` right after the loader gets it
//! back from the parser. Walks every item that carries an `attrs` list
//! (top-level `fn` / `class` and class methods / static methods, plus
//! the same shapes inside `@extern(C) { ... }`) and drops the ones
//! whose `@target(...)` attribute doesn't match the build host's OS.
//! Items that survive have the `@target` attribute stripped so it
//! doesn't leak into hover / formatter output.
//!
//! Matching rules (matches Rust `cfg(target_os = "...")` semantics):
//!
//! - `@target("X")` — keep if `host == X`
//! - `@target("X", "Y")` — keep if host is in the listed set (OR)
//! - `@target(not "X")` — keep if `host != X`
//! - Multiple `@target` attrs on the same item — AND (every attr must
//!   match)
//! - No `@target` attr — always keep
//!
//! Same-name dispatch falls out for free: two `fn foo()` declarations
//! with mutually-exclusive `@target` attrs leave exactly one survivor
//! on any host. If both happen to survive (e.g. the user wrote
//! `@target("macos")` twice by mistake), the existing
//! duplicate-declaration checks downstream report it.

use ilang_ast::{Attribute, AttrArg, ClassDecl, ExternCItem, FnDecl, Item, Program, Span};

use crate::loader::LoadError;

/// Build-time host OS name, matching `os.platform` runtime values.
pub(crate) const fn current_os() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "macos"
    }
    #[cfg(target_os = "linux")]
    {
        "linux"
    }
    #[cfg(target_os = "windows")]
    {
        "windows"
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        "other"
    }
}

/// Check whether an item carrying `attrs` should survive the OS filter.
/// Returns `Ok(true)` to keep, `Ok(false)` to drop, or `Err(...)` if a
/// `@target(...)` argument list is malformed (e.g. an integer or a
/// path inside the parens).
fn keep_for_os(attrs: &[Attribute], outer_span: Span) -> Result<bool, LoadError> {
    for a in attrs.iter() {
        if a.name.as_str() != "target" {
            continue;
        }
        // Each `@target(...)` is its own AND clause. The current host
        // must satisfy this attr's inner OR / negation form to keep
        // looking at the rest of the attrs.
        if !attr_matches(a, outer_span)? {
            return Ok(false);
        }
    }
    Ok(true)
}

fn attr_matches(a: &Attribute, outer_span: Span) -> Result<bool, LoadError> {
    if a.args.is_empty() {
        return Err(target_error(outer_span, "expected at least one OS name"));
    }
    let host = current_os();
    // Negation: `@target(not "X")` — single argument, NotStr form.
    if a.args.iter().any(|x| matches!(x, AttrArg::NotStr(_))) {
        // Only the simple shape — a single `not "X"` — is honoured.
        // Mixing `not` with positive args (or repeating `not`) is
        // refused to avoid the "is it AND or OR?" ambiguity.
        if a.args.len() != 1 {
            return Err(target_error(
                outer_span,
                "`not \"X\"` cannot be combined with other arguments — \
                 use a second `@target` attribute for AND",
            ));
        }
        let AttrArg::NotStr(name) = &a.args[0] else { unreachable!() };
        return Ok(host != name.as_str());
    }
    // Positive form: every arg must be a string. Match if host equals
    // any of them (OR within a single attribute).
    let mut any_matched = false;
    for arg in a.args.iter() {
        let AttrArg::Str(s) = arg else {
            return Err(target_error(
                outer_span,
                "arguments must be string literals like \"macos\"",
            ));
        };
        if s == host {
            any_matched = true;
        }
    }
    Ok(any_matched)
}

fn target_error(span: Span, msg: &str) -> LoadError {
    LoadError::ParseError(crate::error::ParseError::Generic {
        msg: format!("@target: {msg}"),
        span,
    })
}

/// Drop every `@target` attribute from `attrs` so it doesn't leak into
/// hover signatures / formatter output. Called only after the filter
/// has decided to keep the surrounding item.
fn strip_target_attrs(attrs: &mut Box<[Attribute]>) {
    if attrs.iter().any(|a| a.name.as_str() == "target") {
        let kept: Vec<Attribute> = attrs
            .iter()
            .filter(|a| a.name.as_str() != "target")
            .cloned()
            .collect();
        *attrs = kept.into_boxed_slice();
    }
}

fn filter_fn(f: &mut FnDecl) -> Result<bool, LoadError> {
    if !keep_for_os(&f.attrs, f.span)? {
        return Ok(false);
    }
    strip_target_attrs(&mut f.attrs);
    Ok(true)
}

fn filter_class(c: &mut ClassDecl) -> Result<bool, LoadError> {
    if !keep_for_os(&c.attrs, c.span)? {
        return Ok(false);
    }
    strip_target_attrs(&mut c.attrs);
    let mut methods: Vec<FnDecl> = Vec::with_capacity(c.methods.len());
    for m in c.methods.iter().cloned() {
        let mut m = m;
        if filter_fn(&mut m)? {
            methods.push(m);
        }
    }
    c.methods = methods.into_boxed_slice();
    let mut static_methods: Vec<FnDecl> = Vec::with_capacity(c.static_methods.len());
    for m in c.static_methods.iter().cloned() {
        let mut m = m;
        if filter_fn(&mut m)? {
            static_methods.push(m);
        }
    }
    c.static_methods = static_methods.into_boxed_slice();
    Ok(true)
}

/// Walk every item that has an `attrs` list and prune by `@target`.
/// Modifies `prog` in place. Bubble parser-shaped errors back up as
/// `LoadError::ParseError`.
pub(crate) fn filter_program(prog: &mut Program) -> Result<(), LoadError> {
    let mut kept: Vec<Item> = Vec::with_capacity(prog.items.len());
    for item in std::mem::take(&mut prog.items).into_iter() {
        let keep = match item {
            Item::Fn(mut f) => {
                if filter_fn(&mut f)? {
                    kept.push(Item::Fn(f));
                }
                continue;
            }
            Item::Class(mut c) => {
                if filter_class(&mut c)? {
                    kept.push(Item::Class(c));
                }
                continue;
            }
            Item::ExternC(mut block) => {
                let mut inner: Vec<ExternCItem> = Vec::with_capacity(block.items.len());
                for ec in block.items.iter().cloned() {
                    match ec {
                        ExternCItem::FnDef(mut f) => {
                            if filter_fn(&mut f)? {
                                inner.push(ExternCItem::FnDef(f));
                            }
                        }
                        ExternCItem::Class(mut c) => {
                            if filter_class(&mut c)? {
                                inner.push(ExternCItem::Class(c));
                            }
                        }
                        other => inner.push(other),
                    }
                }
                block.items = inner.into_boxed_slice();
                kept.push(Item::ExternC(block));
                continue;
            }
            other => other,
        };
        let _ = keep; // pacify clippy on the trailing arm — the body above always continues.
    }
    prog.items = kept;
    Ok(())
}
