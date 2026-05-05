use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use ilang_ast::{
    Block, ClassDecl, Expr, ExprKind, FnDecl, Item, Pattern, PatternBindings, PatternKind,
    Program, Span, Stmt, StmtKind, Type, UnOp, VariantPayload,
};
use ilang_lexer::{tokenize, TokenKind};
use ilang_parser::parse;
use ilang_types::TypeChecker;
use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

#[derive(Clone, Debug)]
struct Symbol {
    name: String,
    span: Span,
    signature: String,
}

#[derive(Clone, Debug)]
struct ClassInfo {
    decl_span: Span,
    fields: HashMap<String, MemberInfo>,
    methods: HashMap<String, MemberInfo>,
    /// Per-property getter signature, used at read sites (`p.name`).
    /// Falls back to `fields` when the property is set-only.
    getters: HashMap<String, MemberInfo>,
    /// Per-property setter signature, used at write sites
    /// (`p.name = v`). Falls back to `fields` when the property is
    /// get-only.
    setters: HashMap<String, MemberInfo>,
}

#[derive(Clone, Debug)]
struct MemberInfo {
    span: Span,
    signature: String,
    /// For methods: the declared return type. For fields: the field
    /// type. Used to infer `let x = obj.method(...)`.
    ret_ty: Option<Type>,
}

#[derive(Clone, Debug)]
struct RefEntry {
    line: u32,
    start_col: u32,
    end_col: u32,
    target_span: Span,
    target_name_len: u32,
    signature: String,
}

#[derive(Default)]
struct Doc {
    text: String,
    /// Top-level decls keyed by name.
    symbols: HashMap<String, Symbol>,
    /// Per-class field/method index (used when resolving `this.x`).
    #[allow(dead_code)]
    classes: HashMap<String, ClassInfo>,
    /// Resolved references with precise spans. Sorted by (line, start_col).
    refs: Vec<RefEntry>,
    /// Hover-only signatures for names imported via `use module` (e.g.
    /// `math.sqrt`, `math.pi`). The loader prefixes imported items
    /// with the module name, so this map keyed on `module.fn_name`
    /// catches references the buffer-only walker can't resolve.
    /// F12 to these is not supported because we don't carry per-decl
    /// file paths.
    #[allow(dead_code)]
    external_signatures: HashMap<String, String>,
    /// Return types for `module.fn` declarations brought in via
    /// `use module`. Populated alongside `external_signatures` so
    /// `let x = math.sqrt(...)` infers as f64.
    #[allow(dead_code)]
    external_returns: HashMap<String, Type>,
}

struct Backend {
    client: Client,
    docs: Mutex<HashMap<Url, Doc>>,
}

impl Backend {
    fn new(client: Client) -> Self {
        Self {
            client,
            docs: Mutex::new(HashMap::new()),
        }
    }

    async fn refresh(&self, uri: Url, text: String) {
        let path = uri.to_file_path().ok();
        // Run the loader pipeline once: its program drives diagnostics
        // and supplies signatures for `use module`-imported names.
        let merged = path
            .as_deref()
            .filter(|p| p.exists())
            .and_then(|p| {
                let extra = collect_dep_paths(p).unwrap_or_default();
                ilang_parser::loader::load_program_with_paths(p, &extra).ok()
            });
        let diags = analyse(&text, path.as_deref(), &merged);
        let (external_sigs, external_rets) = merged
            .as_ref()
            .map(collect_external_signatures)
            .unwrap_or_default();
        let doc = match parse_ok(&text) {
            Ok(prog) => build_doc(text, &prog, &external_sigs, &external_rets),
            Err(_) => Doc {
                text,
                external_signatures: external_sigs,
                external_returns: external_rets,
                ..Doc::default()
            },
        };
        {
            let mut docs = self.docs.lock().unwrap();
            docs.insert(uri.clone(), doc);
        }
        self.client.publish_diagnostics(uri, diags, None).await;
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> LspResult<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                definition_provider: Some(OneOf::Left(true)),
                ..ServerCapabilities::default()
            },
            server_info: Some(ServerInfo {
                name: "ilang-lsp".into(),
                version: Some(env!("CARGO_PKG_VERSION").into()),
            }),
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "ilang-lsp ready")
            .await;
    }

    async fn did_open(&self, p: DidOpenTextDocumentParams) {
        self.refresh(p.text_document.uri, p.text_document.text).await;
    }

    async fn did_change(&self, mut p: DidChangeTextDocumentParams) {
        if let Some(change) = p.content_changes.pop() {
            self.refresh(p.text_document.uri, change.text).await;
        }
    }

    async fn did_close(&self, p: DidCloseTextDocumentParams) {
        let mut docs = self.docs.lock().unwrap();
        docs.remove(&p.text_document.uri);
    }

    async fn hover(&self, p: HoverParams) -> LspResult<Option<Hover>> {
        let uri = p.text_document_position_params.text_document.uri;
        let pos = p.text_document_position_params.position;
        let docs = self.docs.lock().unwrap();
        let Some(doc) = docs.get(&uri) else {
            return Ok(None);
        };
        if let Some(entry) = lookup_ref(doc, pos) {
            return Ok(Some(make_hover(&entry.signature)));
        }
        if let Some((word, _)) = word_at(&doc.text, pos) {
            if let Some(sym) = doc.symbols.get(&word) {
                return Ok(Some(make_hover(&sym.signature)));
            }
        }
        Ok(None)
    }

    async fn goto_definition(
        &self,
        p: GotoDefinitionParams,
    ) -> LspResult<Option<GotoDefinitionResponse>> {
        let uri = p.text_document_position_params.text_document.uri;
        let pos = p.text_document_position_params.position;
        let docs = self.docs.lock().unwrap();
        let Some(doc) = docs.get(&uri) else {
            return Ok(None);
        };
        if let Some(entry) = lookup_ref(doc, pos) {
            let range = span_to_range(entry.target_span, entry.target_name_len as usize);
            return Ok(Some(GotoDefinitionResponse::Scalar(Location {
                uri,
                range,
            })));
        }
        if let Some((word, _)) = word_at(&doc.text, pos) {
            if let Some(sym) = doc.symbols.get(&word) {
                let range = span_to_range(sym.span, sym.name.len());
                return Ok(Some(GotoDefinitionResponse::Scalar(Location {
                    uri,
                    range,
                })));
            }
        }
        Ok(None)
    }

    async fn shutdown(&self) -> LspResult<()> {
        Ok(())
    }
}

fn make_hover(sig: &str) -> Hover {
    Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: format!("```ilang\n{sig}\n```"),
        }),
        range: None,
    }
}

fn lookup_ref(doc: &Doc, pos: Position) -> Option<&RefEntry> {
    let line = pos.line + 1;
    let col = pos.character + 1;
    doc.refs
        .iter()
        .find(|r| r.line == line && col >= r.start_col && col <= r.end_col)
}

fn parse_ok(src: &str) -> Result<Program, ()> {
    let tokens = tokenize(src).map_err(|_| ())?;
    parse(&tokens).map_err(|_| ())
}

fn analyse(src: &str, path: Option<&Path>, merged: &Option<Program>) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    // Always run the lex + parse pass on the in-memory buffer first so
    // unsaved edits surface syntax errors immediately.
    let tokens = match tokenize(src) {
        Ok(t) => t,
        Err(e) => {
            out.push(diag(e.span(), e.to_string()));
            return out;
        }
    };
    if let Err(e) = parse(&tokens) {
        out.push(diag(e.span(), e.to_string()));
        return out;
    }
    if let Some(prog) = merged {
        let mut tc = TypeChecker::new();
        if let Err(e) = tc.check(prog) {
            out.push(diag(e.span(), e.to_string()));
        }
        return out;
    }
    // Fallback: in-memory parse + typecheck (no module resolution, no
    // const inlining). Used for unsaved buffers without an on-disk file
    // or when loading failed (the load error itself is reported by the
    // caller via `refresh`).
    let _ = path;
    let prog = parse(&tokens).expect("parse already validated");
    let mut tc = TypeChecker::new();
    if let Err(e) = tc.check(&prog) {
        out.push(diag(e.span(), e.to_string()));
    }
    out
}

/// Pull top-level names with prefix-style identifiers (e.g.
/// `math.sqrt`, `math.pi`) out of a loader-merged program so the LSP
/// can answer hover queries on imported names. Plain (un-dotted) names
/// are skipped — they're already covered by the buffer-only index when
/// declared in the open file.
fn collect_external_signatures(
    prog: &Program,
) -> (HashMap<String, String>, HashMap<String, Type>) {
    use ilang_ast::ExternCItem;
    let mut out = HashMap::new();
    let mut rets: HashMap<String, Type> = HashMap::new();
    let put_dotted = |name: &str, sig: String, m: &mut HashMap<String, String>| {
        if name.contains('.') {
            m.insert(name.to_string(), sig);
        }
    };
    for item in &prog.items {
        match item {
            Item::Fn(f) => {
                put_dotted(&f.name, fn_signature(f), &mut out);
                if let Some(t) = &f.ret {
                    if f.name.contains('.') {
                        rets.insert(f.name.clone(), t.clone());
                    }
                }
            }
            Item::Const(c) => {
                let ty = match &c.ty {
                    Some(t) => format!(": {t}"),
                    None => String::new(),
                };
                let value = render_const_value(&c.value)
                    .map(|v| format!(" = {v}"))
                    .unwrap_or_default();
                put_dotted(&c.name, format!("const {}{ty}{value}", c.name), &mut out);
            }
            Item::Class(c) => {
                put_dotted(&c.name, format!("class {}", c.name), &mut out);
            }
            Item::Enum(e) => {
                put_dotted(&e.name, format!("enum {}", e.name), &mut out);
            }
            Item::ExternC(b) => {
                for inner in &b.items {
                    match inner {
                        ExternCItem::FnDecl { name, params, ret, .. } => {
                            let ps = params
                                .iter()
                                .map(|p| format!("{}: {}", p.name, p.ty))
                                .collect::<Vec<_>>()
                                .join(", ");
                            let r = match ret {
                                Some(t) => format!(": {t}"),
                                None => String::new(),
                            };
                            put_dotted(
                                name,
                                format!("fn {}({}){}", name, ps, r),
                                &mut out,
                            );
                            if let Some(t) = ret {
                                if name.contains('.') {
                                    rets.insert(name.clone(), t.clone());
                                }
                            }
                        }
                        ExternCItem::FnDef(f) => {
                            put_dotted(&f.name, fn_signature(f), &mut out);
                            if let Some(t) = &f.ret {
                                if f.name.contains('.') {
                                    rets.insert(f.name.clone(), t.clone());
                                }
                            }
                        }
                        ExternCItem::Static { name, ty, .. } => {
                            put_dotted(name, format!("static {}: {}", name, ty), &mut out);
                        }
                        ExternCItem::Struct { name, .. } => {
                            put_dotted(name, format!("struct {}", name), &mut out);
                        }
                        ExternCItem::Union { name, .. } => {
                            put_dotted(name, format!("union {}", name), &mut out);
                        }
                        ExternCItem::Class(c) => {
                            put_dotted(&c.name, format!("class {}", c.name), &mut out);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    (out, rets)
}


#[derive(Debug, serde::Deserialize)]
struct ProjectFile {
    #[serde(default)]
    deps: BTreeMap<String, DepSpec>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(untagged)]
enum DepSpec {
    Path(String),
    Detailed { path: String },
}

impl DepSpec {
    fn path(&self) -> &str {
        match self {
            DepSpec::Path(p) => p,
            DepSpec::Detailed { path } => path,
        }
    }
}

/// Mirror of the CLI's `ilang.toml` discovery. Walks up from the entry
/// file's directory looking for the closest `ilang.toml`; missing file
/// is not an error.
fn collect_dep_paths(entry: &Path) -> Result<Vec<PathBuf>, String> {
    let entry_dir = entry
        .canonicalize()
        .map_err(|e| format!("cannot resolve entry path: {e}"))?
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    let project_file = find_project_file(&entry_dir);
    let Some(project_file) = project_file else {
        return Ok(Vec::new());
    };
    let project_dir = project_file
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    let src = std::fs::read_to_string(&project_file)
        .map_err(|e| format!("cannot read {}: {e}", project_file.display()))?;
    let parsed: ProjectFile = toml::from_str(&src)
        .map_err(|e| format!("invalid {}: {e}", project_file.display()))?;
    let mut out = Vec::new();
    for (_name, dep) in parsed.deps {
        let p = project_dir.join(dep.path());
        if let Ok(canon) = p.canonicalize() {
            out.push(canon);
        }
    }
    Ok(out)
}

fn find_project_file(start: &Path) -> Option<PathBuf> {
    let mut cur = Some(start.to_path_buf());
    while let Some(dir) = cur {
        let candidate = dir.join("ilang.toml");
        if candidate.exists() {
            return Some(candidate);
        }
        cur = dir.parent().map(|p| p.to_path_buf());
    }
    None
}

fn diag(span: Span, msg: String) -> Diagnostic {
    Diagnostic {
        range: span_to_range(span, 1),
        severity: Some(DiagnosticSeverity::ERROR),
        source: Some("ilang".into()),
        message: msg,
        ..Diagnostic::default()
    }
}

// ─── Index building ────────────────────────────────────────────────────────

fn build_doc(
    text: String,
    prog: &Program,
    external_signatures: &HashMap<String, String>,
    external_returns: &HashMap<String, Type>,
) -> Doc {
    let symbols = collect_symbols(prog);
    let classes = collect_classes(prog);
    let mut fn_returns: HashMap<String, Type> = HashMap::new();
    for item in &prog.items {
        if let Item::Fn(f) = item {
            if let Some(t) = &f.ret {
                fn_returns.insert(f.name.clone(), t.clone());
            }
        }
    }
    let mut refs = Vec::new();
    {
        let mut walker = Walker {
            text: &text,
            symbols: &symbols,
            classes: &classes,
            fn_returns: &fn_returns,
            external_signatures,
            external_returns,
            refs: &mut refs,
        };
        for item in &prog.items {
            match item {
                Item::Fn(f) => walker.walk_fn(f, None),
                Item::Class(c) => walker.walk_class(c),
                _ => {}
            }
        }
        // Top-level stmts/tail (script-style code outside any fn).
        let mut top_scope: Vec<Binding> = Vec::new();
        for s in &prog.stmts {
            walker.walk_stmt(s, &mut top_scope, None);
        }
        if let Some(t) = &prog.tail {
            walker.walk_expr(t, &mut top_scope, None);
        }
    }
    refs.sort_by_key(|r| (r.line, r.start_col));
    Doc {
        text,
        symbols,
        classes,
        refs,
        external_signatures: external_signatures.clone(),
        external_returns: external_returns.clone(),
    }
}

fn collect_symbols(prog: &Program) -> HashMap<String, Symbol> {
    let mut out = HashMap::new();
    for item in &prog.items {
        match item {
            Item::Fn(f) => {
                let sig = fn_signature(f);
                out.insert(
                    f.name.clone(),
                    Symbol {
                        name: f.name.clone(),
                        span: f.span,
                        signature: sig,
                    },
                );
            }
            Item::Class(c) => {
                let signature = format!("class {}", c.name);
                out.insert(
                    c.name.clone(),
                    Symbol {
                        name: c.name.clone(),
                        span: c.span,
                        signature,
                    },
                );
            }
            Item::Enum(e) => {
                let variants = e
                    .variants
                    .iter()
                    .map(|v| match &v.payload {
                        VariantPayload::Unit => v.name.clone(),
                        _ => format!("{}(...)", v.name),
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                let signature = format!("enum {} {{ {} }}", e.name, variants);
                out.insert(
                    e.name.clone(),
                    Symbol {
                        name: e.name.clone(),
                        span: e.span,
                        signature,
                    },
                );
            }
            Item::Const(c) => {
                let ty = match &c.ty {
                    Some(t) => format!(": {t}"),
                    None => String::new(),
                };
                let value = render_const_value(&c.value)
                    .map(|v| format!(" = {v}"))
                    .unwrap_or_default();
                let signature = format!("const {}{}{}", c.name, ty, value);
                out.insert(
                    c.name.clone(),
                    Symbol {
                        name: c.name.clone(),
                        span: c.span,
                        signature,
                    },
                );
            }
            _ => {}
        }
    }
    out
}

fn collect_classes(prog: &Program) -> HashMap<String, ClassInfo> {
    let mut out = HashMap::new();
    for item in &prog.items {
        if let Item::Class(c) = item {
            let mut fields = HashMap::new();
            for f in &c.fields {
                fields.insert(
                    f.name.clone(),
                    MemberInfo {
                        span: f.span,
                        signature: format!("(property) {}.{}: {}", c.name, f.name, f.ty),
                        ret_ty: Some(f.ty.clone()),
                    },
                );
            }
            for f in &c.static_fields {
                fields.insert(
                    f.name.clone(),
                    MemberInfo {
                        span: f.span,
                        signature: format!(
                            "(static field) {}.{}: {}",
                            c.name, f.name, f.ty
                        ),
                        ret_ty: Some(f.ty.clone()),
                    },
                );
            }
            let mut getters: HashMap<String, MemberInfo> = HashMap::new();
            let mut setters: HashMap<String, MemberInfo> = HashMap::new();
            for prop in &c.properties {
                fields.insert(
                    prop.name.clone(),
                    MemberInfo {
                        span: prop.span,
                        signature: format!(
                            "(property) {}.{}: {}",
                            c.name, prop.name, prop.ty
                        ),
                        ret_ty: Some(prop.ty.clone()),
                    },
                );
                if let Some(g) = &prop.getter {
                    getters.insert(
                        prop.name.clone(),
                        MemberInfo {
                            span: g.span,
                            signature: format!(
                                "(getter) {}.{}: {}",
                                c.name, prop.name, prop.ty
                            ),
                            ret_ty: Some(prop.ty.clone()),
                        },
                    );
                }
                if let Some(s) = &prop.setter {
                    setters.insert(
                        prop.name.clone(),
                        MemberInfo {
                            span: s.span,
                            signature: format!(
                                "(setter) {}.{}: {}",
                                c.name, prop.name, prop.ty
                            ),
                            ret_ty: Some(prop.ty.clone()),
                        },
                    );
                }
            }
            let mut methods = HashMap::new();
            for m in &c.methods {
                methods.insert(
                    m.name.clone(),
                    MemberInfo {
                        span: m.span,
                        signature: format!("(method) {}.{}", c.name, fn_body(m)),
                        ret_ty: m.ret.clone(),
                    },
                );
            }
            for m in &c.static_methods {
                methods.insert(
                    m.name.clone(),
                    MemberInfo {
                        span: m.span,
                        signature: format!(
                            "(static method) {}.{}",
                            c.name,
                            fn_body(m)
                        ),
                        ret_ty: m.ret.clone(),
                    },
                );
            }
            out.insert(
                c.name.clone(),
                ClassInfo {
                    decl_span: c.span,
                    fields,
                    methods,
                    getters,
                    setters,
                },
            );
        }
    }
    out
}

fn fn_signature(f: &FnDecl) -> String {
    format!("fn {}", fn_body(f))
}

/// `name(params): ret` — the part that comes after `fn` / `(method)` /
/// `(static method)`.
fn fn_body(f: &FnDecl) -> String {
    let params = f
        .params
        .iter()
        .map(|p| format!("{}: {}", p.name, p.ty))
        .collect::<Vec<_>>()
        .join(", ");
    let ret = match &f.ret {
        Some(t) => format!(": {t}"),
        None => String::new(),
    };
    format!("{}({}){}", f.name, params, ret)
}

// ─── Scope walker ──────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
struct Binding {
    name: String,
    span: Span,
    /// Statically-known type, if we can pin it down. Used both for hover
    /// signature and to resolve `local.field` accesses to the right class.
    ty: Option<Type>,
    /// What kind of binder introduced this (let / param / for-in / match
    /// pattern). Carried into hover signatures so use sites read like
    /// the declaration.
    kind: BindKind,
    /// When `Some`, replaces the kind/ty-derived hover signature.
    /// Used for `let func = fn(name: T): R { ... }` where we want to
    /// show parameter names that `Type::Fn` itself doesn't carry.
    override_signature: Option<String>,
}

#[derive(Clone, Copy, Debug)]
enum BindKind {
    Let,
    Param,
    ForIn,
    Pattern,
}

impl BindKind {
    fn render(self, name: &str, ty: Option<&Type>) -> String {
        let prefix = match self {
            BindKind::Let => "let ",
            BindKind::Param => "(parameter) ",
            BindKind::ForIn => "(for-binding) ",
            BindKind::Pattern => "(pattern) ",
        };
        match ty {
            Some(t) => format!("{prefix}{name}: {t}"),
            None => format!("{prefix}{name}"),
        }
    }
}

struct Walker<'a> {
    text: &'a str,
    symbols: &'a HashMap<String, Symbol>,
    classes: &'a HashMap<String, ClassInfo>,
    /// Top-level fn return types, keyed by name. Used to infer
    /// `let x = call()` bindings.
    fn_returns: &'a HashMap<String, Type>,
    /// Hover signatures for `module.name` references that the loader
    /// brought in from a `use module` statement.
    external_signatures: &'a HashMap<String, String>,
    /// Return types for the same set of external fns. Used when
    /// inferring `let x = math.sqrt(...)` etc.
    external_returns: &'a HashMap<String, Type>,
    refs: &'a mut Vec<RefEntry>,
}

impl<'a> Walker<'a> {
    fn walk_fn(&mut self, f: &FnDecl, this_class: Option<&str>) {
        let mut scope: Vec<Binding> = Vec::new();
        for p in &f.params {
            let sig = BindKind::Param.render(&p.name, Some(&p.ty));
            self.push_decl(&p.name, p.span, sig);
            scope.push(Binding {
                name: p.name.clone(),
                span: p.span,
                ty: Some(p.ty.clone()),
                kind: BindKind::Param,
                override_signature: None,
            });
        }
        self.walk_block(&f.body, &mut scope, this_class);
    }

    fn walk_class(&mut self, c: &ClassDecl) {
        // Field declaration name: hover shows the field decl line.
        for f in &c.fields {
            self.push_decl(
                &f.name,
                f.span,
                format!("(property) {}.{}: {}", c.name, f.name, f.ty),
            );
        }
        for f in &c.static_fields {
            self.push_decl(
                &f.name,
                f.span,
                format!("(static field) {}.{}: {}", c.name, f.name, f.ty),
            );
        }
        for p in &c.properties {
            // PropertyDecl.span points at the `get` / `set` keyword, so
            // the name identifier sits a few columns to its right. Push
            // a decl entry at that exact location for hover and F12,
            // distinguishing the getter from the setter.
            for (kind, accessor_span) in [
                ("getter", p.getter.as_ref().map(|g| g.span)),
                ("setter", p.setter.as_ref().map(|s| s.span)),
            ] {
                let Some(span) = accessor_span else { continue };
                let sig = format!("({kind}) {}.{}: {}", c.name, p.name, p.ty);
                if let Some(name_span) =
                    locate_property_name(self.text, span, &p.name)
                {
                    self.push_decl(&p.name, name_span, sig);
                }
            }
        }
        for m in &c.methods {
            self.push_decl(
                &m.name,
                m.span,
                format!("(method) {}.{}", c.name, fn_body(m)),
            );
            self.walk_fn(m, Some(&c.name));
        }
        for m in &c.static_methods {
            self.push_decl(
                &m.name,
                m.span,
                format!("(static method) {}.{}", c.name, fn_body(m)),
            );
            self.walk_fn(m, None);
        }
        for prop in &c.properties {
            // Treat the getter/setter body like a method body so locals
            // and `this.X` resolve normally.
            if let Some(g) = &prop.getter {
                self.walk_fn(g, Some(&c.name));
            }
            if let Some(s) = &prop.setter {
                self.walk_fn(s, Some(&c.name));
            }
        }
    }

    fn walk_block(&mut self, b: &Block, scope: &mut Vec<Binding>, this_class: Option<&str>) {
        let depth = scope.len();
        for s in &b.stmts {
            self.walk_stmt(s, scope, this_class);
        }
        if let Some(t) = &b.tail {
            self.walk_expr(t, scope, this_class);
        }
        scope.truncate(depth);
    }

    fn walk_stmt(&mut self, s: &Stmt, scope: &mut Vec<Binding>, this_class: Option<&str>) {
        match &s.kind {
            StmtKind::Let { name, ty, value } => {
                self.walk_expr(value, scope, this_class);
                let inferred = ty
                    .clone()
                    .or_else(|| self.infer_expr(value, scope));
                // For `let f = fn(name: T): R { ... }` keep the param
                // names in the rendered signature (Type::Fn alone drops
                // them).
                let override_sig = match &value.kind {
                    ExprKind::FnExpr { params, ret, .. } => {
                        let ps = params
                            .iter()
                            .map(|p| format!("{}: {}", p.name, p.ty))
                            .collect::<Vec<_>>()
                            .join(", ");
                        let r = match ret {
                            Some(t) => format!(": {t}"),
                            None => String::new(),
                        };
                        Some(format!("let {name}: fn({ps}){r}"))
                    }
                    _ => None,
                };
                let sig = override_sig
                    .clone()
                    .unwrap_or_else(|| BindKind::Let.render(name, inferred.as_ref()));
                // s.span points at the `let` keyword. Locate the actual
                // name position by skipping `let` + whitespace.
                let name_span = locate_let_name(self.text, s.span, name).unwrap_or(s.span);
                self.push_decl(name, name_span, sig);
                scope.push(Binding {
                    name: name.clone(),
                    span: name_span,
                    ty: inferred,
                    kind: BindKind::Let,
                    override_signature: override_sig,
                });
            }
            StmtKind::Expr(e) => self.walk_expr(e, scope, this_class),
        }
    }

    fn walk_expr(&mut self, e: &Expr, scope: &mut Vec<Binding>, this_class: Option<&str>) {
        match &e.kind {
            ExprKind::Var(name) => {
                if let Some(b) = scope.iter().rev().find(|b| &b.name == name) {
                    let sig = b
                        .override_signature
                        .clone()
                        .unwrap_or_else(|| b.kind.render(name, b.ty.as_ref()));
                    self.push_ref(name, e.span, b.span, name.len() as u32, sig);
                } else if name.contains('.') {
                    self.push_external_dotted_ref(name, e.span);
                } else if let Some(m) = this_class.and_then(|c| self.classes.get(c)).and_then(
                    |info| {
                        info.getters
                            .get(name)
                            .or_else(|| info.fields.get(name))
                            .or_else(|| info.methods.get(name))
                    },
                ) {
                    // Implicit-`this` member access inside a class method.
                    self.push_ref(name, e.span, m.span, name.len() as u32, m.signature.clone());
                } else if let Some(sym) = self.symbols.get(name) {
                    self.push_ref(
                        name,
                        e.span,
                        sym.span,
                        sym.name.len() as u32,
                        sym.signature.clone(),
                    );
                }
            }
            ExprKind::This => {
                if let Some(c) = this_class {
                    if let Some(info) = self.classes.get(c) {
                        // `this` is 4 chars; e.span points at it.
                        self.push_ref("this", e.span, info.decl_span, c.len() as u32, format!("this: {c}"));
                    }
                }
            }
            ExprKind::Field { obj, name } => {
                self.walk_expr(obj, scope, this_class);
                if let Some(class) = self.resolve_obj_class(obj, scope, this_class) {
                    if let Some(info) = self.classes.get(&class) {
                        if let Some(m) = info
                            .getters
                            .get(name)
                            .or_else(|| info.fields.get(name))
                            .or_else(|| info.methods.get(name))
                        {
                            if let Some((line, col)) = locate_dot_name(self.text, obj.span, name) {
                                self.refs.push(RefEntry {
                                    line,
                                    start_col: col,
                                    end_col: col + name.len() as u32,
                                    target_span: m.span,
                                    target_name_len: name.len() as u32,
                                    signature: m.signature.clone(),
                                });
                            }
                        }
                    }
                }
            }
            ExprKind::MethodCall { obj, method, args } => {
                self.walk_expr(obj, scope, this_class);
                for a in args {
                    self.walk_expr(a, scope, this_class);
                }
                if let Some(class) = self.resolve_obj_class(obj, scope, this_class) {
                    if let Some(info) = self.classes.get(&class) {
                        if let Some(m) = info.methods.get(method) {
                            if let Some((line, col)) = locate_dot_name(self.text, obj.span, method)
                            {
                                self.refs.push(RefEntry {
                                    line,
                                    start_col: col,
                                    end_col: col + method.len() as u32,
                                    target_span: m.span,
                                    target_name_len: method.len() as u32,
                                    signature: m.signature.clone(),
                                });
                            }
                        }
                    }
                }
            }
            ExprKind::Call { callee, args } => {
                if let Some(b) = scope.iter().rev().find(|b| &b.name == callee) {
                    let sig = b
                        .override_signature
                        .clone()
                        .unwrap_or_else(|| b.kind.render(callee, b.ty.as_ref()));
                    self.push_ref(callee, e.span, b.span, callee.len() as u32, sig);
                } else if let Some(m) = this_class
                    .and_then(|c| self.classes.get(c))
                    .and_then(|info| info.methods.get(callee))
                {
                    // Implicit-`this` method call inside a class method.
                    self.push_ref(
                        callee,
                        e.span,
                        m.span,
                        callee.len() as u32,
                        m.signature.clone(),
                    );
                } else if let Some(sym) = self.symbols.get(callee) {
                    self.push_ref(
                        callee,
                        e.span,
                        sym.span,
                        sym.name.len() as u32,
                        sym.signature.clone(),
                    );
                } else if callee.contains('.') {
                    self.push_external_dotted_ref(callee, e.span);
                }
                for a in args {
                    self.walk_expr(a, scope, this_class);
                }
            }
            ExprKind::New { class, args, .. } => {
                if let Some(sym) = self.symbols.get(class) {
                    self.push_ref(
                        class,
                        e.span,
                        sym.span,
                        sym.name.len() as u32,
                        sym.signature.clone(),
                    );
                }
                for a in args {
                    self.walk_expr(a, scope, this_class);
                }
            }
            ExprKind::EnumCtor { enum_name, args, .. } => {
                if let Some(sym) = self.symbols.get(enum_name) {
                    self.push_ref(
                        enum_name,
                        e.span,
                        sym.span,
                        sym.name.len() as u32,
                        sym.signature.clone(),
                    );
                }
                match args {
                    ilang_ast::CtorArgs::Tuple(es) => {
                        for x in es {
                            self.walk_expr(x, scope, this_class);
                        }
                    }
                    ilang_ast::CtorArgs::Struct(pairs) => {
                        for (_, x) in pairs {
                            self.walk_expr(x, scope, this_class);
                        }
                    }
                    ilang_ast::CtorArgs::Unit => {}
                }
            }
            ExprKind::Unary { expr, .. } => self.walk_expr(expr, scope, this_class),
            ExprKind::Binary { lhs, rhs, .. } | ExprKind::Logical { lhs, rhs, .. } => {
                self.walk_expr(lhs, scope, this_class);
                self.walk_expr(rhs, scope, this_class);
            }
            ExprKind::If { cond, then_branch, else_branch } => {
                self.walk_expr(cond, scope, this_class);
                self.walk_block(then_branch, scope, this_class);
                if let Some(e) = else_branch {
                    self.walk_expr(e, scope, this_class);
                }
            }
            ExprKind::While { cond, body } => {
                self.walk_expr(cond, scope, this_class);
                self.walk_block(body, scope, this_class);
            }
            ExprKind::ForIn { var, iter, body } => {
                self.walk_expr(iter, scope, this_class);
                let depth = scope.len();
                let elem_ty = match self.infer_expr(iter, scope) {
                    Some(Type::Array { elem, .. }) => Some(*elem),
                    _ => None,
                };
                let sig = BindKind::ForIn.render(var, elem_ty.as_ref());
                self.push_decl(var, iter.span, sig);
                scope.push(Binding {
                    name: var.clone(),
                    span: iter.span,
                    ty: elem_ty,
                    kind: BindKind::ForIn,
                    override_signature: None,
                });
                self.walk_block(body, scope, this_class);
                scope.truncate(depth);
            }
            ExprKind::Loop { body } => self.walk_block(body, scope, this_class),
            ExprKind::Block(b) => self.walk_block(b, scope, this_class),
            ExprKind::Break(opt) | ExprKind::Return(opt) => {
                if let Some(v) = opt {
                    self.walk_expr(v, scope, this_class);
                }
            }
            ExprKind::Assign { target, value } => {
                if let Some(b) = scope.iter().rev().find(|b| &b.name == target) {
                    let sig = b
                        .override_signature
                        .clone()
                        .unwrap_or_else(|| b.kind.render(target, b.ty.as_ref()));
                    self.push_ref(target, e.span, b.span, target.len() as u32, sig);
                } else if let Some(m) = this_class.and_then(|c| self.classes.get(c)).and_then(
                    |info| {
                        info.setters
                            .get(target)
                            .or_else(|| info.fields.get(target))
                    },
                ) {
                    self.push_ref(
                        target,
                        e.span,
                        m.span,
                        target.len() as u32,
                        m.signature.clone(),
                    );
                } else if let Some(sym) = self.symbols.get(target) {
                    self.push_ref(
                        target,
                        e.span,
                        sym.span,
                        sym.name.len() as u32,
                        sym.signature.clone(),
                    );
                }
                self.walk_expr(value, scope, this_class);
            }
            ExprKind::AssignField { obj, field, value } => {
                self.walk_expr(obj, scope, this_class);
                if let Some(class) = self.resolve_obj_class(obj, scope, this_class) {
                    if let Some(info) = self.classes.get(&class) {
                        if let Some(m) = info
                            .setters
                            .get(field)
                            .or_else(|| info.fields.get(field))
                        {
                            if let Some((line, col)) = locate_dot_name(self.text, obj.span, field)
                            {
                                self.refs.push(RefEntry {
                                    line,
                                    start_col: col,
                                    end_col: col + field.len() as u32,
                                    target_span: m.span,
                                    target_name_len: field.len() as u32,
                                    signature: m.signature.clone(),
                                });
                            }
                        }
                    }
                }
                self.walk_expr(value, scope, this_class);
            }
            ExprKind::AssignIndex { obj, index, value } => {
                self.walk_expr(obj, scope, this_class);
                self.walk_expr(index, scope, this_class);
                self.walk_expr(value, scope, this_class);
            }
            ExprKind::Cast { expr, .. } => self.walk_expr(expr, scope, this_class),
            ExprKind::FnExpr { params, body, .. } => {
                let mut inner: Vec<Binding> = Vec::new();
                for p in params {
                    let sig = BindKind::Param.render(&p.name, Some(&p.ty));
                    self.push_decl(&p.name, p.span, sig);
                    inner.push(Binding {
                        name: p.name.clone(),
                        span: p.span,
                        ty: Some(p.ty.clone()),
                        kind: BindKind::Param,
                        override_signature: None,
                    });
                }
                self.walk_block(body, &mut inner, this_class);
            }
            ExprKind::Array(es) | ExprKind::Tuple(es) => {
                for x in es {
                    self.walk_expr(x, scope, this_class);
                }
            }
            ExprKind::StructLit { fields, .. } => {
                for (_, x) in fields {
                    self.walk_expr(x, scope, this_class);
                }
            }
            ExprKind::MapLit(pairs) => {
                for (k, v) in pairs {
                    self.walk_expr(k, scope, this_class);
                    self.walk_expr(v, scope, this_class);
                }
            }
            ExprKind::Index { obj, index } => {
                self.walk_expr(obj, scope, this_class);
                self.walk_expr(index, scope, this_class);
            }
            ExprKind::Range { start, end, .. } => {
                self.walk_expr(start, scope, this_class);
                self.walk_expr(end, scope, this_class);
            }
            ExprKind::Match { scrutinee, arms } => {
                self.walk_expr(scrutinee, scope, this_class);
                for arm in arms {
                    let depth = scope.len();
                    bind_pattern(&arm.pattern, scope);
                    self.walk_expr(&arm.body, scope, this_class);
                    scope.truncate(depth);
                }
            }
            ExprKind::SuperCall { args, .. } => {
                for a in args {
                    self.walk_expr(a, scope, this_class);
                }
            }
            _ => {}
        }
    }

    /// Walker-aware variant of `infer_expr_type_with_scope` that can
    /// also resolve `Call(callee)` to the callee's declared return
    /// type and `MethodCall` to the resolved method's return type.
    fn infer_expr(&self, e: &Expr, scope: &[Binding]) -> Option<Type> {
        match &e.kind {
            ExprKind::Call { callee, .. } => self
                .fn_returns
                .get(callee)
                .or_else(|| self.external_returns.get(callee))
                .cloned(),
            ExprKind::MethodCall { obj, method, .. } => {
                let class = self.resolve_obj_class(obj, scope, None)?;
                let info = self.classes.get(&class)?;
                info.methods.get(method)?.ret_ty.clone()
            }
            ExprKind::Field { obj, name } => {
                let class = self.resolve_obj_class(obj, scope, None)?;
                let info = self.classes.get(&class)?;
                info.fields.get(name)?.ret_ty.clone()
            }
            ExprKind::Index { obj, .. } => match self.infer_expr(obj, scope)? {
                Type::Array { elem, .. } => Some(*elem),
                Type::Str => Some(Type::U8),
                _ => None,
            },
            ExprKind::If { then_branch, else_branch, .. } => {
                let from_then = then_branch
                    .tail
                    .as_ref()
                    .and_then(|t| self.infer_expr(t, scope));
                from_then.or_else(|| {
                    else_branch.as_ref().and_then(|e| self.infer_expr(e, scope))
                })
            }
            ExprKind::Block(b) => b.tail.as_ref().and_then(|t| self.infer_expr(t, scope)),
            // `loop { ... break v ... }` — the value of the loop is the
            // first `break v` we find. Bare `break` (no value) yields
            // Unit; absence of any break we treat as no info.
            ExprKind::Loop { body } => {
                let mut found: Option<Type> = None;
                find_break_type(body, scope, self, &mut found);
                found
            }
            ExprKind::Match { arms, .. } => arms
                .iter()
                .find_map(|a| self.infer_expr(&a.body, scope)),
            ExprKind::Binary { op, lhs, rhs } => {
                use ilang_ast::BinOp;
                if matches!(
                    op,
                    BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge
                ) {
                    return Some(Type::Bool);
                }
                let lt = self.infer_expr(lhs, scope);
                let rt = self.infer_expr(rhs, scope);
                match (lt, rt) {
                    (Some(l), Some(r)) => Some(promote_pair(&l, &r, lhs, rhs)),
                    (Some(t), None) | (None, Some(t)) => Some(t),
                    (None, None) => None,
                }
            }
            ExprKind::Unary { op, expr } => match op {
                ilang_ast::UnOp::Not => Some(Type::Bool),
                _ => self.infer_expr(expr, scope),
            },
            // Fall back to the scope-aware inferer for everything else.
            _ => infer_expr_type_with_scope(e, scope),
        }
    }

    /// For a dotted name like `math.sqrt`, push a hover-only ref entry
    /// at the suffix position (`.sqrt`). Used for names brought in via
    /// `use module` — the loader resolves these to a full signature
    /// but we don't have file-level spans for F12.
    fn push_external_dotted_ref(&mut self, dotted: &str, receiver_span: Span) {
        let Some(sig) = self.external_signatures.get(dotted) else {
            return;
        };
        let Some(dot) = dotted.rfind('.') else {
            return;
        };
        let suffix = &dotted[dot + 1..];
        if let Some((line, col)) = locate_dot_name(self.text, receiver_span, suffix) {
            self.refs.push(RefEntry {
                line,
                start_col: col,
                end_col: col + suffix.len() as u32,
                target_span: receiver_span,
                target_name_len: suffix.len() as u32,
                signature: sig.clone(),
            });
        }
    }

    fn push_decl(&mut self, name: &str, span: Span, signature: String) {
        self.refs.push(RefEntry {
            line: span.line,
            start_col: span.col,
            end_col: span.col + name.len() as u32,
            target_span: span,
            target_name_len: name.len() as u32,
            signature,
        });
    }

    fn push_ref(
        &mut self,
        name: &str,
        use_span: Span,
        target_span: Span,
        target_name_len: u32,
        signature: String,
    ) {
        self.refs.push(RefEntry {
            line: use_span.line,
            start_col: use_span.col,
            end_col: use_span.col + name.len() as u32,
            target_span,
            target_name_len,
            signature,
        });
    }

    /// Best-effort: figure out which class an `obj` expression refers
    /// to, so `obj.field` / `obj.method()` can resolve. Handles `this`,
    /// known-typed locals, and `new ClassName(...)`.
    fn resolve_obj_class(
        &self,
        obj: &Expr,
        scope: &[Binding],
        this_class: Option<&str>,
    ) -> Option<String> {
        match &obj.kind {
            ExprKind::This => this_class.map(|s| s.to_string()),
            ExprKind::Var(name) => {
                if let Some(b) = scope.iter().rev().find(|b| &b.name == name) {
                    type_to_class(b.ty.as_ref()?)
                } else if self.classes.contains_key(name) {
                    // Bare `ClassName.field/method` — static access on
                    // the class itself.
                    Some(name.clone())
                } else {
                    None
                }
            }
            ExprKind::New { class, .. } => Some(class.clone()),
            _ => None,
        }
    }
}

fn type_to_class(t: &Type) -> Option<String> {
    match t {
        Type::Object(n) => Some(n.clone()),
        Type::Generic { base, .. } => Some(base.clone()),
        _ => None,
    }
}

fn bind_pattern(p: &Pattern, scope: &mut Vec<Binding>) {
    match &p.kind {
        PatternKind::Wildcard => {}
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
                            name: n.clone(),
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
                        name: alias.clone(),
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
fn infer_expr_type_with_scope(e: &Expr, scope: &[Binding]) -> Option<Type> {
    if let ExprKind::FnExpr { params, ret, .. } = &e.kind {
        let ps = params.iter().map(|p| p.ty.clone()).collect();
        let r = ret.clone().unwrap_or(Type::Unit);
        return Some(Type::Fn { params: ps, ret: Box::new(r) });
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
            .find(|b| &b.name == name)
            .and_then(|b| b.ty.clone()),
        ExprKind::New { class, type_args, .. } => {
            if type_args.is_empty() {
                Some(Type::Object(class.clone()))
            } else {
                Some(Type::Generic {
                    base: class.clone(),
                    args: type_args.clone(),
                })
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
fn promote_pair(l: &Type, r: &Type, l_expr: &Expr, r_expr: &Expr) -> Type {
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
fn find_break_type(
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

fn scan_break(
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
                scan_break(&a.body, scope, walker, out);
            }
        }
        _ => {}
    }
}

/// Locate the property name after a `get` or `set` keyword.
fn locate_property_name(text: &str, kw_span: Span, name: &str) -> Option<Span> {
    let off = line_col_to_offset(text, kw_span.line, kw_span.col)?;
    let bytes = text.as_bytes();
    // Skip 3 keyword chars (`get` / `set`).
    let mut i = off + 3;
    while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
        i += 1;
    }
    let nb = name.as_bytes();
    if bytes.len() - i >= nb.len() && &bytes[i..i + nb.len()] == nb {
        let next = bytes.get(i + nb.len()).copied().unwrap_or(b' ');
        if !next.is_ascii_alphanumeric() && next != b'_' {
            let (line, col) = offset_to_line_col(text, i)?;
            return Some(Span::new(line, col));
        }
    }
    None
}

/// Locate the `name` token after a `let` keyword. The Stmt span points
/// at `let`, so we skip the keyword + whitespace to land on the binder.
fn locate_let_name(text: &str, stmt_span: Span, name: &str) -> Option<Span> {
    let off = line_col_to_offset(text, stmt_span.line, stmt_span.col)?;
    let bytes = text.as_bytes();
    // Skip `let`.
    let mut i = off + 3;
    while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
        i += 1;
    }
    let nb = name.as_bytes();
    if bytes.len() - i >= nb.len() && &bytes[i..i + nb.len()] == nb {
        let next = bytes.get(i + nb.len()).copied().unwrap_or(b' ');
        if !next.is_ascii_alphanumeric() && next != b'_' {
            let (line, col) = offset_to_line_col(text, i)?;
            return Some(Span::new(line, col));
        }
    }
    None
}

/// Find the `name` identifier that follows the next `.` after `obj_span`.
/// Returns its (line, col). Used to attach a precise span to `Field` and
/// `MethodCall` references whose AST nodes only carry the receiver's
/// span.
fn locate_dot_name(text: &str, obj_span: Span, name: &str) -> Option<(u32, u32)> {
    let offset = line_col_to_offset(text, obj_span.line, obj_span.col)?;
    let bytes = text.as_bytes();
    // Walk forward, skipping a balanced run that ends at the receiver's
    // outer level. Cheap heuristic: find the first `.` followed by
    // `name` at the right depth-0 paren count.
    let mut i = offset;
    let mut paren_depth: i32 = 0;
    let mut bracket_depth: i32 = 0;
    while i < bytes.len() {
        let b = bytes[i];
        match b {
            b'(' => paren_depth += 1,
            b')' => paren_depth -= 1,
            b'[' => bracket_depth += 1,
            b']' => bracket_depth -= 1,
            b'.' if paren_depth <= 0 && bracket_depth <= 0 => {
                // Skip whitespace then match `name`.
                let mut j = i + 1;
                while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t') {
                    j += 1;
                }
                let nb = name.as_bytes();
                if bytes.len() - j >= nb.len() && &bytes[j..j + nb.len()] == nb {
                    let next = bytes.get(j + nb.len()).copied().unwrap_or(b' ');
                    if !next.is_ascii_alphanumeric() && next != b'_' {
                        return offset_to_line_col(text, j);
                    }
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

fn line_col_to_offset(text: &str, line: u32, col: u32) -> Option<usize> {
    let mut cur_line = 1u32;
    let mut line_start = 0usize;
    let bytes = text.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if cur_line == line {
            return Some(line_start + col.saturating_sub(1) as usize);
        }
        if b == b'\n' {
            cur_line += 1;
            line_start = i + 1;
        }
    }
    if cur_line == line {
        return Some(line_start + col.saturating_sub(1) as usize);
    }
    None
}

fn offset_to_line_col(text: &str, offset: usize) -> Option<(u32, u32)> {
    let bytes = text.as_bytes();
    if offset > bytes.len() {
        return None;
    }
    let mut line = 1u32;
    let mut line_start = 0usize;
    for (i, &b) in bytes.iter().enumerate().take(offset) {
        if b == b'\n' {
            line += 1;
            line_start = i + 1;
        }
    }
    let col = (offset - line_start) as u32 + 1;
    Some((line, col))
}

/// Render a `const` initializer back to a short source-like string for
/// hover. Covers primitive literals and a leading unary `-` / `+`; more
/// complex expressions fall back to `None` so we don't print noise.
fn render_const_value(e: &Expr) -> Option<String> {
    match &e.kind {
        ExprKind::Int(n) => Some(n.to_string()),
        ExprKind::Float(f) => Some(f.to_string()),
        ExprKind::Bool(b) => Some(b.to_string()),
        ExprKind::Str(s) => Some(format!("{s:?}")),
        ExprKind::Unary { op, expr } => {
            let inner = render_const_value(expr)?;
            let sym = match op {
                UnOp::Neg => "-",
                UnOp::Pos => "+",
                UnOp::Not => "!",
                UnOp::BitNot => "~",
            };
            Some(format!("{sym}{inner}"))
        }
        _ => None,
    }
}

/// Find the identifier under the cursor by re-tokenising the source and
/// returning the first identifier whose span covers the position. Used
/// as a fallback for top-level names not in the per-file ref index.
fn word_at(src: &str, pos: Position) -> Option<(String, u32)> {
    let tokens = tokenize(src).ok()?;
    let line = pos.line + 1;
    let col = pos.character + 1;
    for tok in &tokens {
        if let TokenKind::Ident(name) = &tok.kind {
            if tok.span.line == line {
                let start = tok.span.col;
                let end = start + name.len() as u32;
                if col >= start && col <= end {
                    return Some((name.clone(), start));
                }
            }
        }
    }
    None
}

fn span_to_range(span: Span, len: usize) -> Range {
    let line = span.line.saturating_sub(1);
    let start_char = span.col.saturating_sub(1);
    let end_char = start_char + len as u32;
    Range {
        start: Position { line, character: start_char },
        end: Position { line, character: end_char },
    }
}

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let (service, socket) = LspService::new(Backend::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}
