//! `organize_imports` code action — reorganise a file's leading
//! `use` statements: sort by module (re-exports treated as a
//! separate group), merge selective imports of the same module,
//! dedupe whole-module imports, and emit one `use` per line.

use ilang_ast::{Item, Program};

use super::text_utils::compute_line_starts;

/// Returns `(start_byte, end_byte, replacement)` covering only the
/// leading `use` block — items after the first non-`Use` are left
/// alone. `None` when the file has no `use`s or is already canonical.
pub(crate) fn organize_imports(
    text: &str,
    prog: &Program,
) -> Option<(usize, usize, String)> {
    let mut uses: Vec<&ilang_ast::UseDecl> = Vec::new();
    for item in &prog.items {
        match item {
            Item::Use(u) => uses.push(u),
            _ => break,
        }
    }
    if uses.is_empty() {
        return None;
    }
    let line_starts = compute_line_starts(text);
    let first_line = uses[0].span.line as usize;
    let last_line = uses.last().unwrap().span.line as usize;
    let first_byte = line_starts.get(first_line - 1).copied().unwrap_or(0);
    let after_last = line_starts.get(last_line).copied().unwrap_or(text.len());
    let original = &text[first_byte..after_last];
    let canonical = render_uses(&uses);
    if canonical == original {
        return None;
    }
    Some((first_byte, after_last, canonical))
}

/// Build the canonical, sorted, deduped form of a list of
/// `UseDecl`s. Whole-module and selective imports of the same
/// module coexist on separate lines; selective names are sorted
/// alphabetically; re-exports group with the same module but are
/// emitted with the `pub ` prefix.
fn render_uses(uses: &[&ilang_ast::UseDecl]) -> String {
    use std::collections::{BTreeMap, BTreeSet};
    // alias_key: 0 = Default, 1 = Named(foo) (with `foo` in second
    // String), 2 = Discard. Sorts Default-first so plain `use M`
    // appears before any aliased forms in the canonical output.
    type AliasKey = (u8, String);
    fn alias_key(a: &ilang_ast::UseAlias) -> AliasKey {
        match a {
            ilang_ast::UseAlias::Default => (0, String::new()),
            ilang_ast::UseAlias::Named(n) => (1, n.as_str().to_string()),
            ilang_ast::UseAlias::Discard => (2, String::new()),
        }
    }
    fn alias_suffix(a: &ilang_ast::UseAlias) -> String {
        match a {
            ilang_ast::UseAlias::Default => String::new(),
            ilang_ast::UseAlias::Named(n) => format!(" as {}", n.as_str()),
            ilang_ast::UseAlias::Discard => " as _".to_string(),
        }
    }
    let mut groups: BTreeMap<(String, AliasKey, bool), (bool, BTreeSet<String>, ilang_ast::UseAlias)> =
        BTreeMap::new();
    for u in uses {
        let key = (
            u.module.as_str().to_string(),
            alias_key(&u.alias),
            u.re_export,
        );
        let entry = groups
            .entry(key)
            .or_insert_with(|| (false, BTreeSet::new(), u.alias.clone()));
        match &u.selective {
            None => entry.0 = true,
            Some(names) => {
                for n in names.iter() {
                    entry.1.insert(n.as_str().to_string());
                }
            }
        }
    }
    let mut out = String::new();
    for ((module, _, re_export), (has_whole, names, alias)) in groups.iter() {
        let prefix = if *re_export { "pub use " } else { "use " };
        let suffix = alias_suffix(alias);
        if *has_whole {
            out.push_str(prefix);
            out.push_str(module);
            out.push_str(&suffix);
            out.push('\n');
        }
        if !names.is_empty() {
            out.push_str(prefix);
            out.push_str(module);
            out.push_str(&suffix);
            out.push_str(" { ");
            let joined: Vec<&str> = names.iter().map(|s| s.as_str()).collect();
            out.push_str(&joined.join(", "));
            out.push_str(" }\n");
        }
    }
    out
}
