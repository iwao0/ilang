//! `textDocument/implementation` provider.
//!
//! Three target shapes:
//!   - cursor on an interface name → return every class that
//!     `implements` it (the class decl's name range).
//!   - cursor on an interface method → return every implementing
//!     class's corresponding method.
//!   - cursor on a non-private class method → return every subclass
//!     method that overrides it.

use std::collections::HashSet;
use std::path::PathBuf;

use ilang_ast::{ClassDecl, FnDecl, InterfaceDecl, Item, Program, Symbol as AstSymbol};
use tower_lsp::lsp_types::*;

use crate::types::Doc;
use crate::analyse::for_each_closed_workspace_doc;
use crate::text;

/// What kind of implementation request we're servicing. Drives the
/// workspace walk and the per-class filter.
#[derive(Clone, Debug)]
pub(crate) enum Target {
    /// Cursor on an interface name (or its decl). Match every
    /// class that names this interface in its `: A, B, C` list.
    Interface { name: String },
    /// Cursor on a method of an interface — return every
    /// implementing class's same-named method.
    InterfaceMethod { iface: String, method: String },
    /// Cursor on a class name — return every direct subclass.
    Class { name: String },
    /// Cursor on a class method — return every subclass method
    /// that overrides it. `class` is the declaring class.
    ClassMethod { class: String, method: String },
}

/// Resolve the cursor to a [`Target`] using the live `Doc` index.
pub(crate) fn resolve(doc: &Doc, pos: Position) -> Option<Target> {
    if let Some(entry) = crate::lookup_ref(doc, pos) {
        if entry.signature.starts_with("this:") {
            return None;
        }
        return target_from_signature(&entry.signature);
    }
    // Cursor parked on a decl line itself — fall back to symbols.
    let (word, _) = crate::word_at(&doc.text, pos)?;
    let sym = doc.symbols.get(&AstSymbol::intern(&word))?;
    target_from_signature(&sym.signature)
}

fn target_from_signature(sig: &str) -> Option<Target> {
    // Class method signature: `(method) [@attrs\n]Class.method(...)`.
    if let Some(rest) = sig.strip_prefix("(method) ") {
        // Drop attribute lines (rendered above the qualifier).
        let last_line = rest.lines().last()?;
        let (cls, m_and_args) = last_line.split_once('.')?;
        let method = m_and_args.split('(').next()?;
        return Some(Target::ClassMethod {
            class:  cls.to_string(),
            method: method.to_string(),
        });
    }
    if let Some(rest) = sig.strip_prefix("interface ") {
        let name = rest.split(|c: char| c.is_whitespace() || c == ':').next()?;
        return Some(Target::Interface {
            name: name.to_string(),
        });
    }
    // Class header: `[@attrs\n]class Foo[: Base]`. The attribute
    // prefix may carry newlines, so trim through `lines().last()`
    // before stripping the `class ` keyword.
    let last_line = sig.lines().last().unwrap_or(sig);
    for kw in ["class ", "struct ", "union "] {
        if let Some(rest) = last_line.strip_prefix(kw) {
            let name = rest
                .split(|c: char| c.is_whitespace() || c == ':')
                .next()?;
            return Some(Target::Class {
                name: name.to_string(),
            });
        }
    }
    None
}

/// Walk every `.il` reachable from the workspace's `ilang.toml`
/// (with open buffers winning over disk text), and collect every
/// location that implements `target`.
pub(crate) fn collect(
    target: &Target,
    anchor: &std::path::Path,
    open_docs: &std::collections::HashMap<Url, Doc>,
    iface_class: Option<&str>,
) -> Vec<Location> {
    let mut out: Vec<Location> = Vec::new();
    let mut seen: HashSet<PathBuf> = HashSet::new();
    // Detect whether the "class" name in a `ClassMethod` target is
    // actually an interface (caller may not know up front). If it
    // is, route through the InterfaceMethod path so cross-class
    // implementations are surfaced.
    let effective_target = match target {
        Target::ClassMethod { class, method } if iface_class
            .map(|n| n == class.as_str())
            .unwrap_or(false) =>
        {
            Target::InterfaceMethod {
                iface:  class.clone(),
                method: method.clone(),
            }
        }
        _ => target.clone(),
    };
    for (uri, doc) in open_docs.iter() {
        if let Ok(p) = uri.to_file_path() {
            if let Ok(c) = p.canonicalize() {
                seen.insert(c);
            }
        }
        let Some(prog) = text::try_parse(&doc.text) else { continue };
        gather_from_program(uri, &doc.text, &prog, &effective_target, &mut out);
    }
    for_each_closed_workspace_doc(anchor, &seen, |uri, d| {
        let Some(prog) = text::try_parse(&d.text) else { return };
        gather_from_program(&uri, &d.text, &prog, &effective_target, &mut out);
    });
    out.sort_by(|a, b| {
        (a.uri.as_str(), a.range.start.line, a.range.start.character)
            .cmp(&(b.uri.as_str(), b.range.start.line, b.range.start.character))
    });
    out.dedup_by(|a, b| a.uri == b.uri && a.range == b.range);
    out
}

fn gather_from_program(
    uri: &Url,
    text: &str,
    prog: &Program,
    target: &Target,
    out: &mut Vec<Location>,
) {
    for item in &prog.items {
        match item {
            Item::Class(c) => visit_class(uri, text, c, target, out),
            Item::ExternC(b) => {
                for inner in b.items.iter() {
                    if let ilang_ast::ExternCItem::Class(c) = inner {
                        visit_class(uri, text, c, target, out);
                    }
                }
            }
            _ => {}
        }
    }
}

fn visit_class(
    uri: &Url,
    text: &str,
    c: &ClassDecl,
    target: &Target,
    out: &mut Vec<Location>,
) {
    // Parser semantics: the FIRST entry in `: Base1, Base2, ...`
    // always lands in `c.parent` regardless of whether it's a class
    // or an interface — the type checker is what reclassifies. The
    // LSP doesn't typecheck buffer-local classes, so we have to
    // probe both slots when looking for an interface implementation.
    let names_first_then_rest = c
        .parent
        .as_ref()
        .map(|p| p.as_str())
        .into_iter()
        .chain(c.interfaces.iter().map(|i| i.as_str()));
    match target {
        Target::Interface { name } => {
            if names_first_then_rest.clone().any(|n| n == name.as_str()) {
                push_class_name(uri, text, c, out);
            }
        }
        Target::InterfaceMethod { iface, method } => {
            if !names_first_then_rest.clone().any(|n| n == iface.as_str()) {
                return;
            }
            if let Some(m) = find_method_by_name(c, method) {
                push_method_name(uri, text, m, out);
            }
        }
        Target::Class { name } => {
            // Direct subclass = parent matches OR (interfaces list
            // contains it, in case the target turned out to be an
            // interface after typecheck and we missed the flip).
            if names_first_then_rest.clone().any(|n| n == name.as_str()) {
                push_class_name(uri, text, c, out);
            }
        }
        Target::ClassMethod { class, method } => {
            // Subclass override: the class's `parent` must be the
            // target class (or transitively, but the AST only stores
            // the direct parent — direct-parent matches catch the
            // common case; deeper chains would require workspace
            // class graph that the LSP doesn't pre-compute).
            if c.parent.as_ref().map(|p| p.as_str()) != Some(class.as_str()) {
                return;
            }
            if let Some(m) = find_method_by_name(c, method) {
                if m.is_override {
                    push_method_name(uri, text, m, out);
                }
            }
        }
    }
}

fn find_method_by_name<'a>(c: &'a ClassDecl, name: &str) -> Option<&'a FnDecl> {
    c.methods
        .iter()
        .chain(c.static_methods.iter())
        .find(|m| m.name.as_str() == name)
}

fn push_class_name(
    uri: &Url,
    text: &str,
    c: &ClassDecl,
    out: &mut Vec<Location>,
) {
    let name = c.name.as_str();
    let name_span = ["class", "struct", "union", "interface"]
        .iter()
        .find_map(|kw| text::locate_let_name_with_kw(text, c.span, kw, name))
        .unwrap_or(c.span);
    out.push(Location {
        uri:   uri.clone(),
        range: text::span_to_range(name_span, name.len()),
    });
}

fn push_method_name(
    uri: &Url,
    text: &str,
    m: &FnDecl,
    out: &mut Vec<Location>,
) {
    let name = m.name.as_str();
    let name_span = text::locate_let_name_with_kw(text, m.span, "fn", name)
        .unwrap_or(m.span);
    out.push(Location {
        uri:   uri.clone(),
        range: text::span_to_range(name_span, name.len()),
    });
}

/// Did the LSP's class index register `name` as an interface? Used
/// to flip a `ClassMethod` target to `InterfaceMethod` when the
/// "class" turns out to be an interface (the LSP keeps interface
/// methods in the same `classes[X].methods` slot as class methods,
/// so the signature alone can't tell the two apart).
pub(crate) fn name_is_interface(doc: &Doc, name: &str) -> bool {
    let key = AstSymbol::intern(name);
    doc.classes
        .get(&key)
        .map(|info| info.kind == crate::types::ClassKind::Interface)
        .unwrap_or(false)
        || doc.external.interfaces.contains_key(&key)
        || doc.local_interfaces.contains_key(&key)
}

/// Visit a parsed `Program` to surface interface declarations for
/// debug — kept only as the caller-side reference; not used right
/// now.
#[allow(dead_code)]
fn _interface_decls(prog: &Program) -> Vec<&InterfaceDecl> {
    let mut out = Vec::new();
    for item in &prog.items {
        match item {
            Item::Interface(i) => out.push(i),
            Item::ExternC(b) => out.extend(b.interfaces.iter()),
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyse_path_to_doc;

    #[test]
    fn finds_interface_implementers() {
        let tmp = std::env::temp_dir().join("ilang_impl_test_unit");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(tmp.join("ilang.toml"), "[package]\nname = \"t\"\n").unwrap();
        let src = r#"interface Greeter {
    hello(): string
}

class English : Greeter {
    hello(): string { "hello" }
}

class Japanese : Greeter {
    hello(): string { "konnichiwa" }
}

fn run() { let _ = (new English()).hello() }
run()
"#;
        let path = tmp.join("a.il");
        std::fs::write(&path, src).unwrap();
        let doc = analyse_path_to_doc(&path).expect("analyse");
        eprintln!("symbols: {:?}", doc.symbols.keys().collect::<Vec<_>>());
        eprintln!("classes: {:?}", doc.classes.keys().collect::<Vec<_>>());
        let uri = Url::from_file_path(&path).unwrap();
        let mut docs = std::collections::HashMap::new();
        docs.insert(uri.clone(), doc);
        // 1. Interface target → English + Japanese class names.
        let locs = collect(
            &Target::Interface { name: "Greeter".into() },
            &path, &docs, None,
        );
        assert_eq!(locs.len(), 2, "Interface target: {locs:?}");
        // 2. InterfaceMethod → English.hello + Japanese.hello.
        let locs = collect(
            &Target::InterfaceMethod {
                iface:  "Greeter".into(),
                method: "hello".into(),
            },
            &path, &docs, Some("Greeter"),
        );
        assert_eq!(locs.len(), 2, "InterfaceMethod target: {locs:?}");
        // 3. ClassMethod target whose `class` is actually an
        //    interface — collector should auto-flip it to
        //    InterfaceMethod when iface_class hints so.
        let locs = collect(
            &Target::ClassMethod {
                class:  "Greeter".into(),
                method: "hello".into(),
            },
            &path, &docs, Some("Greeter"),
        );
        assert_eq!(
            locs.len(),
            2,
            "ClassMethod-on-interface target: {locs:?}"
        );
    }

    #[test]
    fn finds_class_subclasses() {
        let tmp = std::env::temp_dir().join("ilang_impl_test_class");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(tmp.join("ilang.toml"), "[package]\nname = \"t\"\n").unwrap();
        let src = r#"class Animal {
}

class Dog : Animal {
}

class Cat : Animal {
}

fn run() { let _ = new Dog(); let _ = new Cat() }
run()
"#;
        let path = tmp.join("a.il");
        std::fs::write(&path, src).unwrap();
        let doc = analyse_path_to_doc(&path).expect("analyse");
        let uri = Url::from_file_path(&path).unwrap();
        let mut docs = std::collections::HashMap::new();
        docs.insert(uri.clone(), doc);
        // Cursor on `class Animal` decl — Class target.
        let locs = collect(
            &Target::Class { name: "Animal".into() },
            &path, &docs, None,
        );
        assert_eq!(locs.len(), 2, "Animal subclasses: {locs:?}");
    }
}
