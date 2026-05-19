//! macOS-only integration harness for the Foundation bindings under
//! `bindings/cocoa/foundation/test/`. Each `.il` fixture exits
//! non-zero on assertion failure (the `test.expect*` helpers abort
//! the process); we just check the exit status.
//!
//! The harness also synthesises a small coverage report after the
//! runs: which @objc selectors were exercised, and which classes
//! had at least one method touched. Read via the standard
//! `cargo test ... -- --nocapture` flag — printed to stdout so
//! it shows up next to the test names.
//!
//! Non-macOS hosts skip the entire suite — Foundation needs the
//! ObjC runtime + framework symbols.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn ilang_bin() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_BIN_EXE_ilang"));
    p.pop();
    p.push("ilang");
    p
}

fn repo_root() -> PathBuf {
    // CARGO_MANIFEST_DIR = crates/ilang-cli — pop twice for the
    // repo root.
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop();
    p.pop();
    p
}

fn foundation_dir() -> PathBuf {
    repo_root().join("bindings/cocoa/foundation")
}

fn test_dir() -> PathBuf {
    foundation_dir().join("test")
}

/// All `.il` files in `test_dir`, sorted for stable failure order.
fn collect_test_fixtures() -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    let Ok(entries) = fs::read_dir(test_dir()) else {
        return out;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.extension().and_then(|s| s.to_str()) == Some("il") {
            out.push(p);
        }
    }
    out.sort();
    out
}

/// One binding entry: a class's `@objc("selector:")` declaration
/// paired with the user-visible wrapper name on the line beneath
/// it (the name a test would actually type). The coverage check
/// counts an entry as covered when EITHER the wrapper name or the
/// selector head (the part before the first `:`) appears in any
/// test fixture's identifier set — matches both the ergonomic
/// `obj.wrapperName(...)` and the rare callers that reach for the
/// raw selector head.
#[derive(Debug, Clone)]
struct SelEntry {
    selector: String,
    wrapper: String,
}

/// `class_name → entries` parsed from the binding source.
fn parse_binding_selectors() -> BTreeMap<String, Vec<SelEntry>> {
    let mut classes: BTreeMap<String, Vec<SelEntry>> = BTreeMap::new();
    let mut seen: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let Ok(entries) = fs::read_dir(foundation_dir()) else {
        return classes;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.extension().and_then(|s| s.to_str()) != Some("il") {
            continue;
        }
        let Ok(src) = fs::read_to_string(&p) else { continue };
        // Track current class via brace depth so we ignore the
        // braces that close individual method / accessor bodies.
        let mut current_class: Option<String> = None;
        let mut class_open_depth: i32 = 0;
        let mut depth: i32 = 0;
        // When the previous non-blank, non-attribute line was an
        // `@objc("…")` declaration, this carries its selector so
        // we can grab the wrapper name from the following decl
        // line.
        let mut pending_selector: Option<String> = None;
        for line in src.lines() {
            let trimmed = line.trim_start();
            if let Some(rest) = trimmed
                .strip_prefix("@objc pub class ")
                .or_else(|| trimmed.strip_prefix("@objc pub interface "))
            {
                let name: String = rest
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                if !name.is_empty() {
                    current_class = Some(name);
                    class_open_depth = depth;
                }
                pending_selector = None;
            } else if let Some(rest) = trimmed.strip_prefix("@objc(\"") {
                if let Some(end) = rest.find("\")") {
                    pending_selector = Some(rest[..end].to_string());
                }
            } else if !trimmed.is_empty() && !trimmed.starts_with("//") {
                // First non-comment, non-blank line after an
                // `@objc("…")` carries the wrapper signature. Skip
                // sibling attributes like `@optional`, `@since`,
                // `@deprecated` — they don't introduce the wrapper.
                if trimmed.starts_with('@') {
                    // sibling attribute, keep waiting
                } else if let Some(sel) = pending_selector.take() {
                    let wrapper = extract_wrapper_name(trimmed);
                    if let (Some(c), Some(w)) = (&current_class, wrapper) {
                        let class_set = seen.entry(c.clone()).or_default();
                        if class_set.insert(sel.clone()) {
                            classes.entry(c.clone()).or_default().push(SelEntry {
                                selector: sel,
                                wrapper: w,
                            });
                        }
                    }
                }
            }
            for ch in line.chars() {
                match ch {
                    '{' => depth += 1,
                    '}' => {
                        depth -= 1;
                        if current_class.is_some() && depth == class_open_depth {
                            current_class = None;
                            pending_selector = None;
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    classes
}

/// Pull the user-visible method / property name from the line
/// following an `@objc("…")` declaration. Handles the modifier
/// vocabulary the bindings actually use (`pub`, `pub static`,
/// `pub get`, `pub set`, `pub init`, and the leading-underscore
/// non-pub `_name` form).
fn extract_wrapper_name(line: &str) -> Option<String> {
    let mut rest = line;
    for prefix in [
        "pub static ",
        "pub get ",
        "pub set ",
        "pub init",
        "pub ",
        "static ",
    ] {
        if let Some(r) = rest.strip_prefix(prefix) {
            rest = r;
            break;
        }
    }
    // `pub init` (no space) → either bare `init(...)` or
    // `initFoo(...)`. The above strip_prefix handles both: after
    // stripping "pub init", what's left starts with the rest of
    // the name (e.g. "WithBytes(...)") or "(" for plain init.
    // Take leading alphanumerics + `_` as the identifier.
    let name: String = rest
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    // If the previous prefix was "pub init" and the suffix part is
    // empty (i.e. the line was `pub init(...)`), the wrapper name
    // IS `init`. Detect that by checking whether the original line
    // had `pub init(`.
    if name.is_empty() && line.contains("pub init(") {
        return Some("init".to_string());
    }
    if name.is_empty() {
        None
    } else if line.contains("pub init") && !line.starts_with("pub initWith")
        && !line.starts_with("pub init ")
        && line.contains("pub init(")
    {
        Some("init".to_string())
    } else if name == "static" || name == "get" || name == "set" {
        // Defensive: shouldn't happen with the prefixes above but
        // skip if we somehow swallowed the wrong piece.
        None
    } else {
        // Re-prefix `init` if the wrapper is an init flavour like
        // `initWithBytes` — the strip already left "WithBytes" for
        // us; prepend `init` so the test text `initWithBytes` matches.
        if line.starts_with("pub initWith")
            || line.starts_with("pub init")
                && !line.starts_with("pub init(")
                && name.chars().next().is_some_and(|c| c.is_uppercase())
        {
            Some(format!("init{name}"))
        } else {
            Some(name)
        }
    }
}

/// Approximate cover-set: any identifier that appears in a method
/// / property position inside the test fixtures. Catches three
/// shapes:
///   - `.name(` and `.name<` — method calls
///   - `.name`              — property reads (the binding side
///                             marks these with `pub get name`)
///   - `name(` and `name<`  — free fn / static method calls
///
/// False positives are possible (an unrelated identifier sharing
/// a name with a selector) but for the binding tests they're
/// acceptable noise.
fn parse_test_method_usage() -> BTreeSet<String> {
    let mut used: BTreeSet<String> = BTreeSet::new();
    for path in collect_test_fixtures() {
        let Ok(src) = fs::read_to_string(&path) else { continue };
        for line in src.lines() {
            // Strip line comments — selectors don't appear inside.
            let line = match line.find("//") {
                Some(i) => &line[..i],
                None => line,
            };
            let bytes: Vec<char> = line.chars().collect();
            let mut i = 0;
            while i < bytes.len() {
                let c = bytes[i];
                if c.is_alphabetic() || c == '_' {
                    let start = i;
                    while i < bytes.len()
                        && (bytes[i].is_alphanumeric() || bytes[i] == '_')
                    {
                        i += 1;
                    }
                    let ident: String = bytes[start..i].iter().collect();
                    let prev = if start == 0 { ' ' } else { bytes[start - 1] };
                    let next = bytes.get(i).copied().unwrap_or(' ');
                    // `.ident` always counts (method or property).
                    // `ident(` and `ident<` count as call / generic.
                    let is_member = prev == '.';
                    let is_call = next == '(' || next == '<';
                    if is_member || is_call {
                        used.insert(ident);
                    }
                } else {
                    i += 1;
                }
            }
        }
    }
    used
}

fn coverage_report() -> String {
    let classes = parse_binding_selectors();
    let used = parse_test_method_usage();

    let mut out = String::new();
    out.push_str("\n=== Foundation binding coverage ===\n");

    let mut total_sel = 0usize;
    let mut covered_sel = 0usize;
    let mut total_cls = 0usize;
    let mut covered_cls = 0usize;

    for (cls, sels) in &classes {
        total_cls += 1;
        let mut class_covered = 0usize;
        for entry in sels {
            // An entry is "covered" when the user-visible wrapper
            // name OR the selector head (pre-`:`) shows up in any
            // test fixture's identifier set. Wrapper covers
            // ergonomic `obj.wrapperName(...)` calls; selector
            // head catches direct uses of the raw ObjC name.
            let head: String = entry
                .selector
                .chars()
                .take_while(|c| *c != ':')
                .collect();
            if used.contains(&entry.wrapper) || used.contains(&head) {
                class_covered += 1;
            }
        }
        total_sel += sels.len();
        covered_sel += class_covered;
        if class_covered > 0 {
            covered_cls += 1;
        }
        if !sels.is_empty() {
            let pct = class_covered * 100 / sels.len();
            out.push_str(&format!(
                "  {:<32} {:>3}/{:<3} ({:>3}%)\n",
                cls,
                class_covered,
                sels.len(),
                pct
            ));
        }
    }
    let sel_pct = if total_sel == 0 {
        0
    } else {
        covered_sel * 100 / total_sel
    };
    let cls_pct = if total_cls == 0 {
        0
    } else {
        covered_cls * 100 / total_cls
    };
    out.push_str(&format!(
        "  -----\n  selectors: {}/{} ({}%)\n  classes  : {}/{} ({}%)\n",
        covered_sel, total_sel, sel_pct, covered_cls, total_cls, cls_pct
    ));
    out
}

#[test]
fn run_foundation_fixtures() {
    if !cfg!(target_os = "macos") {
        eprintln!("skipping: foundation tests only run on macOS");
        return;
    }

    let bin = ilang_bin();
    let fixtures = collect_test_fixtures();
    assert!(
        !fixtures.is_empty(),
        "no test fixtures discovered in {}",
        test_dir().display()
    );

    let mut failures: Vec<String> = Vec::new();
    for path in &fixtures {
        let out = Command::new(&bin)
            .arg("run")
            .arg(path)
            .current_dir(test_dir())
            .output()
            .unwrap_or_else(|e| panic!("failed to spawn ilang: {e}"));
        if !out.status.success() {
            failures.push(format!(
                "FAIL {}\n  stdout: {}\n  stderr: {}",
                path.file_name().unwrap().to_string_lossy(),
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr),
            ));
        } else {
            eprintln!(
                "pass: {}",
                path.file_name().unwrap().to_string_lossy()
            );
        }
    }

    // Print coverage even on failures so a partial run still
    // gives the user a sense of what's exercised.
    eprintln!("{}", coverage_report());

    if !failures.is_empty() {
        panic!(
            "{} fixture(s) failed:\n{}",
            failures.len(),
            failures.join("\n")
        );
    }
}
