mod analyse;
mod backend;
mod builtins;
mod call_hierarchy;
mod code_actions;
mod code_lens;
mod completion;
mod diag;
mod document_symbol;
mod external;
mod folding_range;
mod formatter;
mod handlers;
mod helpers;
mod implementation;
mod imports;
mod inlay_hints;
mod project;
mod references;
mod rename;
mod rename_conflicts;
mod selection_range;
mod semantic_tokens;
mod signature_help;
mod symbols;
mod text;
mod text_utils;
mod types;
mod walker;
mod workspace_symbol_cache;

use analyse::*;
use backend::*;
use diag::*;
use external::*;
use helpers::*;
use symbols::*;
use types::*;
use walker::*;

use completion::literal_token_at;

use std::time::Duration;

use ilang_ast::{Symbol as AstSymbol, UnOp};
use ilang_lexer::tokenize;
use ilang_parser::parse;
use ilang_types::TypeChecker;
use tower_lsp::{LspService, Server};

use builtins::{
    array_method_doc, array_method_sig, ffi_helper_signature, map_method_doc, map_method_sig,
    primitive_method_doc, primitive_method_sig, set_method_doc, set_method_sig,
    string_method_doc, string_method_sig,
};
pub(crate) use external::ExternalSources;
use project::{find_project_file, find_umbrella};
use text::{
    locate_class_base_name, locate_dot_name, locate_if_let_some_name, locate_let_name,
    locate_let_name_with_kw, locate_property_name, locate_selective_name, locate_type_after_colon,
    span_full_to_range, word_at,
};

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let (service, socket) = LspService::new(Backend::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}
