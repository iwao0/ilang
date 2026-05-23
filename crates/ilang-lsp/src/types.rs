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

#[derive(Clone, Debug)]
pub(crate) struct Symbol {
    pub(crate) name: String,
    pub(crate) span: Span,
    pub(crate) signature: String,
    /// `///`-prefixed doc comment lines immediately above the decl.
    pub(crate) doc: Option<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct ClassInfo {
    pub(crate) decl_span: Span,
    pub(crate) fields: HashMap<AstSymbol, MemberInfo>,
    pub(crate) methods: HashMap<AstSymbol, MemberInfo>,
    /// Per-property getter signature, used at read sites (`p.name`).
    /// Falls back to `fields` when the property is set-only.
    pub(crate) getters: HashMap<AstSymbol, MemberInfo>,
    /// Per-property setter signature, used at write sites
    /// (`p.name = v`). Falls back to `fields` when the property is
    /// get-only.
    pub(crate) setters: HashMap<AstSymbol, MemberInfo>,
    /// `true` for classes pulled in via `use module`. Their member
    /// `MemberInfo.span` values are line/col into another file we
    /// don't carry, so F12 must stay at the use site.
    pub(crate) external: bool,
    /// Number of `init` overloads declared on the class. Used to
    /// append `(+N overloads)` to the constructor hover.
    pub(crate) init_overloads: usize,
    /// All `init` overload signatures in declaration order, used by
    /// signature help on `new ClassName(...)`.
    pub(crate) inits: Vec<MemberInfo>,
    /// Decl keyword used in source (`class` / `struct` / `union`).
    /// Drives hover phrasing for ARC classes vs `@extern(C)` structs.
    pub(crate) kind: ClassKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ClassKind {
    Class,
    Struct,
    Union,
    /// `interface` / `@com interface` / `@objc interface`. Members
    /// are virtual contracts; hover should phrase them as
    /// "interface" rather than borrowing the class wording.
    Interface,
}

impl ClassKind {
    pub(crate) fn keyword(self) -> &'static str {
        match self {
            ClassKind::Class => "class",
            ClassKind::Struct => "struct",
            ClassKind::Union => "union",
            ClassKind::Interface => "interface",
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct MemberInfo {
    pub(crate) span: Span,
    pub(crate) signature: String,
    /// For methods: the declared return type. For fields: the field
    /// type. Used to infer `let x = obj.method(...)`.
    pub(crate) ret_ty: Option<Type>,
    /// `true` for `static` fields / methods. Drives `Counter.<.>`
    /// completion (which should only list static members).
    pub(crate) is_static: bool,
    /// `///`-prefixed doc comment lines above the member.
    pub(crate) doc: Option<String>,
    /// File path containing this member's declaration. Set on
    /// `external` classes so F12 routes through the actual declaring
    /// file — inherited methods land in their parent's file, not in
    /// the receiver's class file at the wrong line number. `None` for
    /// buffer-local members (the LSP resolves those against the
    /// current document's URI).
    pub(crate) source_path: Option<std::path::PathBuf>,
}

#[derive(Clone, Debug)]
pub(crate) struct RefEntry {
    pub(crate) line: u32,
    pub(crate) start_col: u32,
    pub(crate) end_col: u32,
    pub(crate) target_span: Span,
    pub(crate) target_name_len: u32,
    pub(crate) signature: String,
    /// `true` when we don't have a real source-file location for the
    /// definition (imported member, built-in, etc). F12 returns no
    /// definition rather than navigating to the use site, which VSCode
    /// reports as "no references found".
    pub(crate) no_definition: bool,
    /// Cross-file F12 target. When set, F12 navigates to this URI at
    /// `target_span` instead of the current document. Used for
    /// `use module`-imported decls whose source lives in another file.
    pub(crate) target_uri: Option<Url>,
    pub(crate) doc: Option<String>,
}

impl RefEntry {
    /// Convert the entry's 1-based `(line, start_col, end_col)` triple
    /// to the 0-based `Range` LSP expects.
    pub(crate) fn lsp_range(&self) -> Range {
        Range {
            start: Position {
                line: self.line.saturating_sub(1),
                character: self.start_col.saturating_sub(1),
            },
            end: Position {
                line: self.line.saturating_sub(1),
                character: self.end_col.saturating_sub(1),
            },
        }
    }
}

#[derive(Clone, Default)]
pub(crate) struct Doc {
    pub(crate) text: String,
    /// Top-level decls keyed by name.
    pub(crate) symbols: HashMap<AstSymbol, Symbol>,
    /// Per-class field/method index (used when resolving `this.x`).
    #[allow(dead_code)]
    pub(crate) classes: HashMap<AstSymbol, ClassInfo>,
    /// Resolved references with precise spans. Sorted by (line, start_col).
    pub(crate) refs: Vec<RefEntry>,
    /// Variable name → class name, for completion on `obj.`. Populated
    /// from let / param bindings whose static type resolves to a known
    /// class. Last-write-wins across scopes — good enough for most
    /// completion contexts.
    pub(crate) var_classes: HashMap<AstSymbol, String>,
    /// Variable name → full ilang type. Drives `obj.` completion for
    /// non-class types (string / array) so their built-in methods show
    /// up.
    pub(crate) var_types: HashMap<AstSymbol, Type>,
    /// Hover-only signatures for names imported via `use module` (e.g.
    /// `math.sqrt`, `math.pi`). The loader prefixes imported items
    /// with the module name, so this map keyed on `module.fn_name`
    /// catches references the buffer-only walker can't resolve.
    /// F12 to these is not supported because we don't carry per-decl
    /// file paths.
    #[allow(dead_code)]
    pub(crate) external_signatures: HashMap<AstSymbol, String>,
    /// Doc comments (`///`) attached to imported `module.X` decls.
    /// Same key shape as `external_signatures`.
    pub(crate) external_docs: HashMap<AstSymbol, String>,
    /// Source-file location for imported decls (cross-file F12).
    /// Keyed both by `module.X` (whole import) and by bare `X`
    /// (selective import).
    pub(crate) external_sources: ExternalSources,
    /// Return types for `module.fn` declarations brought in via
    /// `use module`. Populated alongside `external_signatures` so
    /// `let x = math.sqrt(...)` infers as f64.
    #[allow(dead_code)]
    pub(crate) external_returns: HashMap<AstSymbol, Type>,
    /// Interface declarations from imported modules, keyed both by
    /// the bare name (`NSApplicationDelegate`) for selective
    /// imports and the prefixed form (`cocoa.NSApplicationDelegate`)
    /// for whole-module references. Drives the "implement missing
    /// interface methods" code action when a class names a
    /// cross-module interface in its base list.
    #[allow(dead_code)]
    pub(crate) external_interfaces:
        HashMap<AstSymbol, ilang_ast::InterfaceDecl>,
    /// Interface declarations from the local buffer's last
    /// successful parse, keyed by bare name. Kept here so the
    /// completion / code-action paths can resolve interface
    /// methods even while the user is mid-edit and the buffer's
    /// current text doesn't parse (the previous `Doc` payload
    /// stays put until a clean parse arrives).
    #[allow(dead_code)]
    pub(crate) local_interfaces:
        HashMap<AstSymbol, ilang_ast::InterfaceDecl>,
    /// Bare names brought into the buffer's namespace by a
    /// selective use (`use M { X, Y }`). Type completion checks
    /// this set to decide whether `cocoa.NSApplicationDelegate`
    /// should be inserted as the dotted form (not in the set) or
    /// the bare form (already imported, dotted would just clutter).
    pub(crate) selective_use_names: HashSet<AstSymbol>,
}

