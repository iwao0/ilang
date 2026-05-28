#[cfg(test)]
mod lib_filter_tests {
    use super::super::*;
    use crate::types::Doc;

    fn doc_with_windows_module() -> Doc {
        use crate::types::{ClassInfo, ClassKind, MemberInfo};
        use ilang_ast::Span;
        use std::collections::HashMap;
        let mut doc = Doc::default();
        doc.text = "use windows\n\nwindows.\n".to_string();
        // Module marker — `analyse_path_to_doc` would normally emit
        // this so `windows` resolves as a receiver in the completion
        // dispatch.
        doc.external.signatures.insert(
            AstSymbol::intern("windows"),
            "(module) windows".to_string(),
        );
        // Two children of the `windows` namespace: one with the
        // `@lib(...)` prefix that the harvest emits for `@extern(C,
        // "kernel32") { @lib pub fn ... }` declarations, and one
        // plain re-export that should always remain visible.
        doc.external.signatures.insert(
            AstSymbol::intern("windows.GetModuleHandleA"),
            "@lib(\"kernel32\")\nfn windows.GetModuleHandleA(lpModuleName: *const char): HMODULE"
                .to_string(),
        );
        doc.external.signatures.insert(
            AstSymbol::intern("windows.WindowsHelper"),
            "fn windows.WindowsHelper(x: i64): i64".to_string(),
        );
        // Two struct entries: STARTUPINFOA holds a `*char` field
        // (C-only — hide from non-extern completion), and PLAIN_RECT
        // is all `i32` (must stay visible).
        doc.external.signatures.insert(
            AstSymbol::intern("windows.STARTUPINFOA"),
            "struct windows.STARTUPINFOA".to_string(),
        );
        doc.external.signatures.insert(
            AstSymbol::intern("windows.PLAIN_RECT"),
            "struct windows.PLAIN_RECT".to_string(),
        );
        let mk_field = |name: &str, ty: Type| -> (AstSymbol, MemberInfo) {
            (
                AstSymbol::intern(name),
                MemberInfo {
                    span: Span::new(1, 1),
                    signature: format!("(property) X.{name}: {ty}"),
                    ret_ty: Some(ty),
                    is_static: false,
                    is_pub: true,
                    doc: None,
                    source_path: None,
                },
            )
        };
        let mut startup_fields = HashMap::new();
        startup_fields.extend([mk_field("cb", Type::U32), mk_field(
            "lpTitle",
            Type::RawPtr { is_const: false, inner: Box::new(Type::CChar) },
        )]);
        doc.classes.insert(
            AstSymbol::intern("kernel32.STARTUPINFOA"),
            ClassInfo {
                decl_span: Span::new(1, 1),
                type_params: Vec::new(),
                fields: startup_fields,
                methods: HashMap::new(),
                getters: HashMap::new(),
                setters: HashMap::new(),
                external: true,
                init_overloads: 0,
                inits: Vec::new(),
                kind: ClassKind::Struct,
            },
        );
        let mut rect_fields = HashMap::new();
        rect_fields.extend([
            mk_field("x", Type::I32),
            mk_field("y", Type::I32),
            mk_field("w", Type::I32),
            mk_field("h", Type::I32),
        ]);
        doc.classes.insert(
            AstSymbol::intern("windef.PLAIN_RECT"),
            ClassInfo {
                decl_span: Span::new(1, 1),
                type_params: Vec::new(),
                fields: rect_fields,
                methods: HashMap::new(),
                getters: HashMap::new(),
                setters: HashMap::new(),
                external: true,
                init_overloads: 0,
                inits: Vec::new(),
                kind: ClassKind::Struct,
            },
        );
        doc.imported_modules.insert(AstSymbol::intern("windows"));
        doc
    }

    fn labels_after_dot(text: &str, after_dot_line: u32, after_dot_col: u32) -> Vec<String> {
        let mut doc = doc_with_windows_module();
        doc.text = text.to_string();
        let resp = handle_completion(
            &doc,
            Position { line: after_dot_line, character: after_dot_col },
        )
        .expect("expected a completion response");
        let items = match resp {
            CompletionResponse::Array(items) => items,
            CompletionResponse::List(list) => list.items,
        };
        items.into_iter().map(|it| it.label).collect()
    }

    #[test]
    fn lib_fn_hidden_after_windows_dot_at_top_level() {
        // Cursor is at the end of `windows.` on line 3.
        let labels = labels_after_dot("use windows\n\nwindows.\n", 2, 8);
        assert!(
            !labels.iter().any(|l| l == "GetModuleHandleA"),
            "expected @lib fn `GetModuleHandleA` to be hidden outside @extern(C), \
             got: {labels:?}"
        );
        assert!(
            labels.iter().any(|l| l == "WindowsHelper"),
            "non-@lib re-exports must still surface, got: {labels:?}"
        );
    }

    #[test]
    fn lib_fn_visible_after_windows_dot_inside_extern_c() {
        // Same dotted access, but cursor sits inside an
        // `@extern(C) { ... }` block — the @lib fn now belongs.
        let src = "use windows\n@extern(C) {\n    windows.\n}\n";
        let labels = labels_after_dot(src, 2, 12);
        assert!(
            labels.iter().any(|l| l == "GetModuleHandleA"),
            "@lib fn must surface inside @extern(C), got: {labels:?}"
        );
    }

    #[test]
    fn c_only_struct_hidden_after_windows_dot_at_top_level() {
        let labels = labels_after_dot("use windows\n\nwindows.\n", 2, 8);
        assert!(
            !labels.iter().any(|l| l == "STARTUPINFOA"),
            "STARTUPINFOA (has `*char` field) must be hidden outside \
             @extern(C), got: {labels:?}"
        );
        assert!(
            labels.iter().any(|l| l == "PLAIN_RECT"),
            "PLAIN_RECT (only i32 fields) must stay visible, got: {labels:?}"
        );
    }

    #[test]
    fn c_only_struct_visible_after_windows_dot_inside_extern_c() {
        let src = "use windows\n@extern(C) {\n    windows.\n}\n";
        let labels = labels_after_dot(src, 2, 12);
        assert!(
            labels.iter().any(|l| l == "STARTUPINFOA"),
            "C-only struct must surface inside @extern(C), got: {labels:?}"
        );
    }

    #[test]
    fn array_filter_completion_seeds_true_body() {
        // `b.filter` needs a `fn(T): bool` closure. Seed the body
        // with `true` so accepting the completion produces a
        // type-checkable lambda, not an empty-body unit lambda
        // that fails the return-type check.
        use std::collections::HashMap;
        let mut doc = Doc::default();
        doc.text = "let b: i64[] = []\nb.\n".to_string();
        doc.var_types = HashMap::new();
        doc.var_types.insert(
            AstSymbol::intern("b"),
            Type::Array { elem: Box::new(Type::I64), fixed: None },
        );
        let pos = Position { line: 1, character: 2 };
        let resp = handle_completion(&doc, pos)
            .expect("expected a completion response for `b.`");
        let items = match resp {
            CompletionResponse::Array(items) => items,
            CompletionResponse::List(list) => list.items,
        };
        let filter = items
            .iter()
            .find(|it| it.label == "filter")
            .expect("filter must be in the candidates");
        let snippet = filter
            .insert_text
            .as_ref()
            .expect("filter completion must carry a snippet");
        assert!(
            snippet.contains("true"),
            "filter body must seed a bool literal so the lambda \
             returns the expected `bool`, got: {snippet}"
        );
        assert!(
            snippet.contains("): bool"),
            "filter closure must carry an explicit `: bool` return \
             annotation — ilang doesn't infer it from the call site, \
             got: {snippet}"
        );
    }

    #[test]
    fn array_for_each_completion_expands_lambda_snippet() {
        // Typing `b.` for `let b: i64[] = []` should offer `forEach`
        // with a snippet that drops the cursor into a pre-built
        // `fn(${1:_}: i64) { ${2} }` body — same expansion the
        // user-defined-method path already provides.
        use std::collections::HashMap;
        let mut doc = Doc::default();
        doc.text = "let b: i64[] = []\nb.\n".to_string();
        doc.var_types = HashMap::new();
        doc.var_types.insert(
            AstSymbol::intern("b"),
            Type::Array { elem: Box::new(Type::I64), fixed: None },
        );
        let pos = Position { line: 1, character: 2 };
        let resp = handle_completion(&doc, pos)
            .expect("expected a completion response for `b.`");
        let items = match resp {
            CompletionResponse::Array(items) => items,
            CompletionResponse::List(list) => list.items,
        };
        let for_each = items
            .iter()
            .find(|it| it.label == "forEach")
            .expect("forEach must be in the candidates");
        assert_eq!(
            for_each.insert_text_format,
            Some(InsertTextFormat::SNIPPET),
            "forEach must be inserted as a SNIPPET so placeholders are honoured"
        );
        let snippet = for_each
            .insert_text
            .as_ref()
            .expect("forEach completion must carry a snippet");
        assert!(
            snippet.contains("fn(") && snippet.contains("i64"),
            "snippet should pre-build a `fn(_: i64) {{ }}` lambda, got: {snippet}"
        );
    }

    #[test]
    fn primitive_receiver_surfaces_to_string() {
        // `let a: i64 = 0\na.` — the receiver is a numeric primitive,
        // so completion should at minimum suggest `toString` (which
        // the type checker accepts on every numeric / bool value).
        // Before the primitive_method_* hookup the dispatch fell
        // through to the empty case and the user saw nothing.
        use std::collections::HashMap;
        let mut doc = Doc::default();
        doc.text = "let a: i64 = 0\na.\n".to_string();
        doc.var_types = HashMap::new();
        doc.var_types.insert(AstSymbol::intern("a"), Type::I64);
        let pos = Position { line: 1, character: 2 };
        let resp = handle_completion(&doc, pos)
            .expect("expected a completion response for `a.`");
        let items = match resp {
            CompletionResponse::Array(items) => items,
            CompletionResponse::List(list) => list.items,
        };
        let labels: Vec<&str> = items.iter().map(|it| it.label.as_str()).collect();
        assert!(
            labels.iter().any(|l| *l == "toString"),
            "expected `toString` in i64 receiver completion, got: {labels:?}"
        );
    }

    #[test]
    fn paren_int_literal_receiver_surfaces_i64_methods() {
        // `(1).` — parenthesised int literal. Same fix shape as the
        // float-literal case; the int branch lists `toString` but
        // NOT the float-only `isFinite` / `isNaN`.
        let mut doc = Doc::default();
        doc.text = "(1).\n".to_string();
        let pos = Position { line: 0, character: 4 };
        let resp = handle_completion(&doc, pos)
            .expect("expected a completion response for `(1).`");
        let items = match resp {
            CompletionResponse::Array(items) => items,
            CompletionResponse::List(list) => list.items,
        };
        let labels: Vec<&str> = items.iter().map(|it| it.label.as_str()).collect();
        assert!(
            labels.iter().any(|l| *l == "toString"),
            "expected `toString` in (1). completion, got: {labels:?}"
        );
        for unwanted in ["isFinite", "isNaN"] {
            assert!(
                !labels.iter().any(|l| *l == unwanted),
                "did not expect float-only `{unwanted}` in (1). completion, got: {labels:?}"
            );
        }
    }

    #[test]
    fn paren_bool_literal_receiver_surfaces_bool_methods() {
        // `(true).` — bool literal. Only `toString` is offered; the
        // float-only predicates must stay hidden.
        let mut doc = Doc::default();
        doc.text = "(true).\n".to_string();
        let pos = Position { line: 0, character: 7 };
        let resp = handle_completion(&doc, pos)
            .expect("expected a completion response for `(true).`");
        let items = match resp {
            CompletionResponse::Array(items) => items,
            CompletionResponse::List(list) => list.items,
        };
        let labels: Vec<&str> = items.iter().map(|it| it.label.as_str()).collect();
        assert!(
            labels.iter().any(|l| *l == "toString"),
            "expected `toString` in (true). completion, got: {labels:?}"
        );
        for unwanted in ["isFinite", "isNaN"] {
            assert!(
                !labels.iter().any(|l| *l == unwanted),
                "did not expect float-only `{unwanted}` in (true). completion, got: {labels:?}"
            );
        }
    }

    #[test]
    fn paren_hex_int_literal_receiver_surfaces_i64_methods() {
        // `(0xFF).` — hex int literal in parens. Same int sentinel
        // as decimal; `is_int_literal` handles the `0x` prefix.
        let mut doc = Doc::default();
        doc.text = "(0xFF).\n".to_string();
        let pos = Position { line: 0, character: 7 };
        let resp = handle_completion(&doc, pos)
            .expect("expected a completion response for `(0xFF).`");
        let items = match resp {
            CompletionResponse::Array(items) => items,
            CompletionResponse::List(list) => list.items,
        };
        let labels: Vec<&str> = items.iter().map(|it| it.label.as_str()).collect();
        assert!(
            labels.iter().any(|l| *l == "toString"),
            "expected `toString` in (0xFF). completion, got: {labels:?}"
        );
    }

    #[test]
    fn paren_float_literal_receiver_surfaces_f64_methods() {
        // `(1.0).` — parenthesised float literal as the receiver.
        // The receiver isn't a variable name, so the old path looked
        // it up in `var_types`, got `None`, and bailed out. Now
        // `receiver_before_dot` returns the FLOAT_LITERAL_RECEIVER
        // sentinel and completion surfaces the f64 primitive methods
        // (`toString` / `isFinite` / `isNaN`).
        let mut doc = Doc::default();
        doc.text = "(1.0).\n".to_string();
        let pos = Position { line: 0, character: 6 };
        let resp = handle_completion(&doc, pos)
            .expect("expected a completion response for `(1.0).`");
        let items = match resp {
            CompletionResponse::Array(items) => items,
            CompletionResponse::List(list) => list.items,
        };
        let labels: Vec<&str> = items.iter().map(|it| it.label.as_str()).collect();
        for want in ["toString", "isFinite", "isNaN"] {
            assert!(
                labels.iter().any(|l| *l == want),
                "expected `{want}` in (1.0). completion, got: {labels:?}"
            );
        }
    }

    #[test]
    fn c_only_struct_hidden_in_type_position_at_top_level() {
        // `let x: <here>` — VSCode invokes completion in a type
        // position, which goes through `type_completions`. Dotted
        // labels like `windows.STARTUPINFOA` flow through this path
        // separately from the value-position bare list.
        let doc = doc_with_windows_module();
        let mut local = doc.clone();
        local.text = "use windows\nlet x: \n".to_string();
        let pos = Position { line: 1, character: 7 };
        let resp = handle_completion(&local, pos).expect("type completion response");
        let items = match resp {
            CompletionResponse::Array(items) => items,
            CompletionResponse::List(list) => list.items,
        };
        let labels: Vec<&str> = items.iter().map(|it| it.label.as_str()).collect();
        assert!(
            !labels.iter().any(|l| *l == "windows.STARTUPINFOA"),
            "type-position completion must hide C-only struct outside \
             @extern(C), got: {labels:?}"
        );
        assert!(
            labels.iter().any(|l| *l == "windows.PLAIN_RECT"),
            "plain C struct must stay visible in type position, got: {labels:?}"
        );
    }
}

#[cfg(test)]
mod dispatch_tests {
    use super::super::handle_completion;
    use crate::analyse;
    use std::path::PathBuf;
    use tower_lsp::lsp_types::{CompletionResponse, Position};

    #[test]
    fn diag_scan_user_file_for_actctxw() {
        // Replays target/test.il's exact shape and tries every cursor
        // position on every line, dumping any completion that mentions
        // ACTCTXW. Catches a context I might have missed in the more
        // targeted tests.
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.push("tests/fixtures/use_windows/main.il");
        let mut doc = analyse::analyse_path_to_doc(&path)
            .expect("fixtures/use_windows/main.il must load");
        doc.text = "use windows\n\nfn add(a: i64, _: i64) {\n    console.log(\"add\", a)\n}\nadd(1, 2)\n".to_string();
        let mut leaks: Vec<String> = Vec::new();
        let line_count = doc.text.lines().count() as u32;
        for line in 0..=line_count {
            let line_str = doc.text.lines().nth(line as usize).unwrap_or("");
            let line_len = line_str.chars().count() as u32;
            for col in 0..=line_len {
                let pos = Position { line, character: col };
                let Some(resp) = handle_completion(&doc, pos) else { continue };
                let items = match resp {
                    CompletionResponse::Array(items) => items,
                    CompletionResponse::List(list) => list.items,
                };
                for it in items {
                    if it.label.contains("ACTCTXW") {
                        leaks.push(format!(
                            "line={line} col={col} label={:?} detail={:?}",
                            it.label, it.detail
                        ));
                    }
                }
            }
        }
        assert!(
            leaks.is_empty(),
            "ACTCTXW leaked into completion at these cursor positions:\n{}",
            leaks.join("\n")
        );
    }

    #[test]
    fn selective_use_surfaces_extern_c_lib_fn_inside_extern_c_block() {
        // End-to-end reproduction of the user-reported bug at
        // `libs/gui/win32/edit.il`: `use windows { HeapFree, ... }`
        // selectively imports an `@lib pub fn HeapFree(...)`
        // declaration that lives behind a chain of `pub use`
        // re-exports (windows.il → kernel32.il). Inside an
        // `@extern(C) { ... }` block in the same file, typing `h`
        // must surface `HeapFree` as a completion candidate.
        // (The original fixture was `button.il` — that file got
        // refactored to drop its HeapAlloc/HeapFree dance, so we
        // probe edit.il which still demonstrates the pattern.)
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.pop();
        path.pop();
        path.push("libs/gui/win32/edit.il");
        let mut doc = analyse::analyse_path_to_doc(&path)
            .expect("libs/gui/win32/edit.il must load");
        // Slip a probe `            h` line in just before the
        // existing `HeapFree(heap, 0 as u32, wideBuf as *void)` call
        // so the cursor sits inside the same @extern(C) block.
        let mut lines: Vec<String> = doc.text.lines().map(|s| s.to_string()).collect();
        let probe_line_idx = lines
            .iter()
            .position(|l| l.contains("HeapFree(heap, 0 as u32, wideBuf"))
            .expect("expected HeapFree(heap, …, wideBuf) line in fixture");
        lines.insert(probe_line_idx, "            h".to_string());
        doc.text = lines.join("\n");
        let pos = Position { line: probe_line_idx as u32, character: 13 };
        let items = match handle_completion(&doc, pos)
            .expect("completion at `h` must return a response")
        {
            CompletionResponse::Array(items) => items,
            CompletionResponse::List(list) => list.items,
        };
        let labels: Vec<String> = items.into_iter().map(|it| it.label).collect();
        assert!(
            labels.iter().any(|l| l == "HeapFree"),
            "expected `HeapFree` in completion list; matching `h*` \
             labels: {:?}",
            labels.iter().filter(|l| l.starts_with('H')).collect::<Vec<_>>()
        );
    }

    #[test]
    fn lambda_param_dot_completion_surfaces_event_fields() {
        // Real reproduction of the user-reported bug:
        // `examples/libs/gui/window/main.il` has multiple
        // `<emitter>.add(fn(e: gui.<EventType>) { ... })` lambdas.
        // Pre-fix, `resolve_receiver_class` couldn't see `e` because
        // the walker registered FnExpr params only in its transient
        // `Binding` scope, never in `var_classes` / `var_types`, so
        // `handle_completion` returned `None` at `e.|` inside the
        // lambda body.
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.pop();
        path.pop();
        path.push("examples/libs/gui/window/main.il");
        let mut doc = analyse::analyse_path_to_doc(&path)
            .expect("examples/libs/gui/window/main.il must load");
        // Drop a probe `    e.` line inside the lambda the user
        // pointed at (the one that logs "clicked at"). The column
        // lands right after the dot.
        let mut lines: Vec<String> = doc.text.lines().map(|s| s.to_string()).collect();
        let probe_line_idx = lines
            .iter()
            .position(|l| l.contains("clicked at"))
            .expect("expected `clicked at` line in fixture");
        lines[probe_line_idx] = "    e.".to_string();
        doc.text = lines.join("\n");
        let pos = Position { line: probe_line_idx as u32, character: 6 };
        let items = match handle_completion(&doc, pos)
            .expect("completion at `e.` must return some response")
        {
            CompletionResponse::Array(items) => items,
            CompletionResponse::List(list) => list.items,
        };
        let labels: Vec<String> = items.into_iter().map(|it| it.label).collect();
        for required in ["x", "y", "button"] {
            assert!(
                labels.iter().any(|l| l == required),
                "expected MouseEvent field `{required}` after `e.` inside \
                 Button.onClick lambda; got {labels:?}"
            );
        }
    }

    #[test]
    fn windows_actctxw_hidden_for_bare_top_level_prefix() {
        // Reproduce the user's actual scenario: target/test.il with
        // `use windows` and a regular top-level fn, cursor typing
        // a bare prefix (no `.`, no `:`) at the very top. Every
        // completion item returned must NOT mention ACTCTXW in any
        // form — bare label, dotted label, or filter text.
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.push("tests/fixtures/use_windows/main.il");
        let mut doc = analyse::analyse_path_to_doc(&path)
            .expect("fixtures/use_windows/main.il must load");
        // Type a single letter at the very top of the file to mimic
        // VSCode's per-keystroke completion request.
        doc.text = "use windows\n\nA\n".to_string();
        let pos = Position { line: 2, character: 1 };
        let resp = handle_completion(&doc, pos).expect("completion response");
        let items = match resp {
            CompletionResponse::Array(items) => items,
            CompletionResponse::List(list) => list.items,
        };
        let offending: Vec<String> = items
            .iter()
            .filter(|it| {
                it.label.ends_with("ACTCTXW")
                    || it.filter_text.as_deref().is_some_and(|t| t.contains("ACTCTXW"))
            })
            .map(|it| {
                format!(
                    "label={:?} filter_text={:?} detail={:?}",
                    it.label, it.filter_text, it.detail
                )
            })
            .collect();
        assert!(
            offending.is_empty(),
            "bare top-level completion still surfaces ACTCTXW: {offending:#?}"
        );
    }

    #[test]
    fn windows_actctxw_hidden_after_dot_at_top_level() {
        // Same fixture, but exercise the receiver-after-dot path
        // (`windows.<.>`). The user-visible bug also surfaced here.
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.push("tests/fixtures/use_windows/main.il");
        let mut doc = analyse::analyse_path_to_doc(&path)
            .expect("fixtures/use_windows/main.il must load");
        // Overlay a buffer line that ends in `windows.` so
        // `receiver_before_dot` resolves to `windows`.
        doc.text.push_str("\nwindows.\n");
        let line = doc.text.lines().count() as u32 - 2;
        let pos = Position { line, character: 8 };
        let resp = handle_completion(&doc, pos).expect("completion response");
        let items = match resp {
            CompletionResponse::Array(items) => items,
            CompletionResponse::List(list) => list.items,
        };
        let labels: Vec<String> = items.into_iter().map(|it| it.label).collect();
        assert!(
            !labels.iter().any(|l| l == "ACTCTXW"),
            "ACTCTXW leaked into `windows.<.>` at top level: \
             labels ending in ACTCTXW = {:?}",
            labels.iter().filter(|l| l.ends_with("ACTCTXW")).collect::<Vec<_>>()
        );
    }
}
