//! Type-name resolution (`walk_type_at` / `walk_type_name_at`) and
//! the low-level `push_decl*` / `push_ref*` builders that drop a
//! `RefEntry` into `Walker::refs`. These are the primitives the rest
//! of the walker uses to advertise hover / F12 / documentHighlight
//! anchors back to the LSP.

use super::*;

impl<'a> Walker<'a> {
    /// Walk a `Type` at `start_span` (the first character of the
    /// type token in source) and push hover / F12 entries for each
    /// dotted `Type::Object` name. Suffixes like `[]`, `?`, `.weak`
    /// don't shift the type-name's start, so nested types inherit
    /// `start_span`.
    pub(crate) fn walk_type_at(&mut self, ty: &Type, start_span: Span) {
        match ty {
            Type::Object(name) => self.walk_type_name_at(name.as_str(), start_span),
            Type::Array { elem, .. } => self.walk_type_at(elem, start_span),
            Type::Optional(inner) => self.walk_type_at(inner, start_span),
            Type::Weak(inner) => self.walk_type_at(inner, start_span),
            Type::Generic(g) => self.walk_type_name_at(g.base.as_str(), start_span),
            _ => {}
        }
    }

    /// Resolve and push a Ref for a type-name occurrence at
    /// `start_span`. Handles three shapes:
    ///   * the source literally spells the full dotted name
    ///     (`cocoa.NSObject` in code) → `push_external_dotted_ref`
    ///   * the AST carries a dotted name but the source spells
    ///     just the suffix (typical after `use M { Name }` lets
    ///     the loader rewrite `Name` → `M.Name`) → look up the
    ///     suffix in `external_signatures` and point F12 at the
    ///     selective-import source
    ///   * bare name, either in buffer-local `symbols` or in the
    ///     selective-import maps
    fn walk_type_name_at(&mut self, name: &str, start_span: Span) {
        if name.contains('.') {
            if text::text_at_span_starts_with_at(self.line_starts, self.text, start_span, name) {
                self.push_external_dotted_ref(name, start_span);
                return;
            }
            if let Some((_, suffix)) = name.rsplit_once('.') {
                if text::text_at_span_starts_with_at(self.line_starts, self.text, start_span, suffix) {
                    self.push_external_type_ref(suffix, start_span);
                    return;
                }
            }
            self.push_external_dotted_ref(name, start_span);
            return;
        }
        if let Some(sym) = self.symbols.get(&AstSymbol::intern(name)) {
            self.push_ref_with_doc(
                name,
                start_span,
                sym.span,
                name.len() as u32,
                sym.signature.clone(),
                sym.doc.clone(),
            );
        } else {
            self.push_external_type_ref(name, start_span);
        }
    }

    /// Bare type name not found in `symbols` (so not buffer-local)
    /// — try the selective-import maps. `use cocoa { NSObject }`
    /// lands NSObject's signature under the bare key in
    /// `external_signatures` with the originating source in
    /// `external_sources`, which gives us the F12 target.
    pub(crate) fn push_external_type_ref(&mut self, name: &str, span: Span) {
        let key = AstSymbol::intern(name);
        let Some(sig) = self.external_signatures.get(&key) else {
            return;
        };
        let loc = self.external_sources.get(&key);
        let target_uri = loc.and_then(|l| Url::from_file_path(&l.path).ok());
        let (target_span, target_name_len, no_def) = match loc {
            Some(l) if target_uri.is_some() => (l.span, l.name_len, false),
            _ => (span, name.len() as u32, target_uri.is_none()),
        };
        self.refs.push(RefEntry {
            line: span.line,
            start_col: span.col,
            end_col: span.col + name.len() as u32,
            target_span,
            target_name_len,
            signature: sig.clone(),
            no_definition: no_def,
            target_uri,
            doc: self.external_docs.get(&key).cloned(),
        });
    }

    /// For a dotted name like `math.sqrt`, push a hover-only ref entry
    /// at the suffix position (`.sqrt`). Used for names brought in via
    /// `use module` — the loader resolves these to a full signature
    /// but we don't have file-level spans for F12.
    pub(crate) fn push_external_dotted_ref(&mut self, dotted: &str, receiver_span: Span) {
        let Some(sig) = self.external_signatures.get(&AstSymbol::intern(dotted)) else {
            return;
        };
        let segments: Vec<&str> = dotted.split('.').collect();
        if segments.len() < 2 {
            return;
        }
        // The AST may carry more segments than the source literally
        // wrote — `use std.math as math` aliases `math.abs(...)` to
        // the canonical `std.math.abs` callee, but only `math.abs`
        // shows in the buffer. Find which segment of the dotted
        // chain the buffer starts at by matching the identifier at
        // `receiver_span` against the segments; treat everything
        // before that as a logical prefix (skipped for refs).
        let source_head = crate::text::read_identifier_at_with(self.line_starts, self.text, receiver_span);
        let start_idx = match source_head {
            Some(head) => segments.iter().position(|s| *s == head).unwrap_or(0),
            None => 0,
        };
        // Each ref produces one RefEntry per segment so hover on
        // each segment shows the right level of detail:
        //   * intermediate segments → `(module) <cumulative>` with
        //     doc pulled from the matching top-of-file `///` block
        //   * the last segment → the item's own signature + doc
        for i in start_idx..segments.len() {
            let seg = segments[i];
            let is_last = i + 1 == segments.len();
            let cumulative: String = segments[..=i].join(".");
            // For an intermediate segment we synthesise a module
            // hover; for the leaf we look up the actual item.
            let (signature, doc, loc, name_len_hint) = if is_last {
                let item_loc = self.external_sources.get(&AstSymbol::intern(dotted));
                let item_doc = self.external_docs.get(&AstSymbol::intern(dotted)).cloned();
                (sig.clone(), item_doc, item_loc, seg.len() as u32)
            } else {
                let mod_loc = self.external_sources.get(&AstSymbol::intern(&cumulative));
                let mod_doc = self.external_docs.get(&AstSymbol::intern(&cumulative)).cloned();
                (format!("(module) {cumulative}"), mod_doc, mod_loc, seg.len() as u32)
            };
            let target_uri = loc.and_then(|l| Url::from_file_path(&l.path).ok());
            let (target_span, target_name_len, no_def) = match loc {
                Some(l) if target_uri.is_some() => (l.span, l.name_len, false),
                _ => (receiver_span, name_len_hint, true),
            };
            // First *visible* segment sits at receiver_span.col;
            // later segments are placed by walking the source for
            // each remaining dotted suffix so their column matches
            // the literal buffer position.
            let (line, start_col) = if i == start_idx {
                (receiver_span.line, receiver_span.col)
            } else {
                let tail: String = segments[i..].join(".");
                let Some((l, c)) = text::locate_dot_name_at(self.line_starts, self.text, receiver_span, &tail) else {
                    continue;
                };
                (l, c)
            };
            self.refs.push(RefEntry {
                line,
                start_col,
                end_col: start_col + seg.len() as u32,
                target_span,
                target_name_len,
                signature,
                no_definition: no_def,
                target_uri,
                doc,
            });
        }
    }

    pub(crate) fn push_decl(&mut self, name: &str, span: Span, signature: String) {
        self.push_decl_with_doc(name, span, signature, None);
    }

    pub(crate) fn push_decl_with_doc(
        &mut self,
        name: &str,
        span: Span,
        signature: String,
        doc: Option<String>,
    ) {
        // Synthesised desugar names (`__cached_sel`,
        // `__objc_b<line>c<col>_sel_cache`, the `_ilang_impl_<name>`
        // pair from the @objc subclass IMP rename, …) borrow user
        // source spans from the surrounding declaration, so their
        // refs hijack hover at unrelated tokens. Filter through
        // `is_synthesized_objc_helper` so every desugar-emitted name
        // is rejected uniformly, not just the `__`-prefixed subset.
        if crate::symbols::is_synthesized_objc_helper(name) {
            return;
        }
        self.refs.push(RefEntry {
            line: span.line,
            start_col: span.col,
            end_col: span.col + name.len() as u32,
            target_span: span,
            target_name_len: name.len() as u32,
            signature,
            no_definition: false,
            target_uri: None,
            doc,
        });
    }

    pub(crate) fn push_ref(
        &mut self,
        name: &str,
        use_span: Span,
        target_span: Span,
        target_name_len: u32,
        signature: String,
    ) {
        self.push_ref_with_doc(name, use_span, target_span, target_name_len, signature, None)
    }

    pub(crate) fn push_ref_with_doc(
        &mut self,
        name: &str,
        use_span: Span,
        target_span: Span,
        target_name_len: u32,
        signature: String,
        doc: Option<String>,
    ) {
        // Parser-synthesised calls (the `@objc class` desugar emits
        // a pile of `cstrFromString("ClassName")`, `__get_class(...)`,
        // etc.) reuse user spans so error messages stay anchored
        // somewhere sensible. They confuse hover though — without
        // this check, hovering on the class name picks up the
        // synthesised Call ref instead of `class ClassName`. Drop
        // any push whose use_span doesn't actually contain `name`
        // in the source text.
        if !text::text_at_span_starts_with_at(self.line_starts, self.text, use_span, name) {
            return;
        }
        self.refs.push(RefEntry {
            line: use_span.line,
            start_col: use_span.col,
            end_col: use_span.col + name.len() as u32,
            target_span,
            target_name_len,
            signature,
            no_definition: false,
            target_uri: None,
            doc,
        });
    }
}
