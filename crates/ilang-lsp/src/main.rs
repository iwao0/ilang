use std::collections::HashMap;
use std::sync::Mutex;

use ilang_ast::{Item, Program, Span};
use ilang_lexer::{tokenize, TokenKind};
use ilang_parser::parse;
use ilang_types::TypeChecker;
use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

#[derive(Clone, Debug)]
struct Symbol {
    name: String,
    /// Span pointing at the identifier in the declaration. We synthesise
    /// the end column from the name length since the AST only stores
    /// start positions.
    span: Span,
    /// Human-readable signature shown on hover.
    signature: String,
}

#[derive(Default)]
struct Doc {
    text: String,
    /// Top-level decls keyed by name. Locals and class members are not
    /// indexed in this stage — go-to-definition / hover only resolve
    /// names visible at the file scope.
    symbols: HashMap<String, Symbol>,
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
        let diags = analyse(&text);
        let symbols = if let Ok(prog) = parse_ok(&text) {
            collect_symbols(&prog)
        } else {
            HashMap::new()
        };
        {
            let mut docs = self.docs.lock().unwrap();
            docs.insert(uri.clone(), Doc { text, symbols });
        }
        self.client
            .publish_diagnostics(uri, diags, None)
            .await;
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
        let doc = match docs.get(&uri) {
            Some(d) => d,
            None => return Ok(None),
        };
        let Some((word, _start_col)) = word_at(&doc.text, pos) else {
            return Ok(None);
        };
        let Some(sym) = doc.symbols.get(&word) else {
            return Ok(None);
        };
        let md = format!("```ilang\n{}\n```", sym.signature);
        Ok(Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: md,
            }),
            range: None,
        }))
    }

    async fn goto_definition(
        &self,
        p: GotoDefinitionParams,
    ) -> LspResult<Option<GotoDefinitionResponse>> {
        let uri = p.text_document_position_params.text_document.uri;
        let pos = p.text_document_position_params.position;
        let docs = self.docs.lock().unwrap();
        let doc = match docs.get(&uri) {
            Some(d) => d,
            None => return Ok(None),
        };
        let Some((word, _)) = word_at(&doc.text, pos) else {
            return Ok(None);
        };
        let Some(sym) = doc.symbols.get(&word) else {
            return Ok(None);
        };
        let range = span_to_range(sym.span, sym.name.len());
        Ok(Some(GotoDefinitionResponse::Scalar(Location {
            uri,
            range,
        })))
    }

    async fn shutdown(&self) -> LspResult<()> {
        Ok(())
    }
}

fn parse_ok(src: &str) -> Result<Program, ()> {
    let tokens = tokenize(src).map_err(|_| ())?;
    parse(&tokens).map_err(|_| ())
}

fn analyse(src: &str) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    let tokens = match tokenize(src) {
        Ok(t) => t,
        Err(e) => {
            out.push(Diagnostic {
                range: span_to_range(e.span(), 1),
                severity: Some(DiagnosticSeverity::ERROR),
                source: Some("ilang".into()),
                message: e.to_string(),
                ..Diagnostic::default()
            });
            return out;
        }
    };
    let prog = match parse(&tokens) {
        Ok(p) => p,
        Err(e) => {
            out.push(Diagnostic {
                range: span_to_range(e.span(), 1),
                severity: Some(DiagnosticSeverity::ERROR),
                source: Some("ilang".into()),
                message: e.to_string(),
                ..Diagnostic::default()
            });
            return out;
        }
    };
    let mut tc = TypeChecker::new();
    if let Err(e) = tc.check(&prog) {
        out.push(Diagnostic {
            range: span_to_range(e.span(), 1),
            severity: Some(DiagnosticSeverity::ERROR),
            source: Some("ilang".into()),
            message: e.to_string(),
            ..Diagnostic::default()
        });
    }
    out
}

fn collect_symbols(prog: &Program) -> HashMap<String, Symbol> {
    let mut out = HashMap::new();
    for item in &prog.items {
        match item {
            Item::Fn(f) => {
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
                let signature = format!("fn {}({}){}", f.name, params, ret);
                out.insert(
                    f.name.clone(),
                    Symbol {
                        name: f.name.clone(),
                        span: f.span,
                        signature,
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
                    .map(|v| v.name.clone())
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
                let signature = format!("const {}{}", c.name, ty);
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

/// Find the identifier under the cursor by re-tokenising the source and
/// returning the first identifier whose span covers the position.
fn word_at(src: &str, pos: Position) -> Option<(String, u32)> {
    let tokens = tokenize(src).ok()?;
    let line = pos.line + 1; // LSP is 0-based, ilang spans are 1-based
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
    // ilang spans: 1-based line/col of the first char.
    // LSP ranges: 0-based, end-exclusive.
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
