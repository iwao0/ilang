//! Detect duplicate `pub` declarations in the merged program.
//!
//! Runs once after `renormalize_merged` so every `use`d module's
//! items already carry their final (possibly module-prefixed) name.
//! The check rejects two `pub` declarations of the same nominal
//! kind sharing the same final name — class / interface / struct /
//! union / enum / const collisions are all forbidden because the
//! type system gives each name a single identity. Free `pub fn`
//! overloads with **different** parameter-type lists are kept as
//! intentional overloads; only identical-signature duplicates
//! still error.
//!
//! Triggered the original investigation: `bindings/directx12/`
//! previously declared `pub struct ID3DBlob {}` (opaque struct) in
//! `d3dcompiler.il` **and** `@com pub interface ID3DBlob` in
//! `d3d12_com.il`. Both bare names landed at `directx12.ID3DBlob`
//! after the umbrella's `pub use d3d*.*`, and the LSP picked
//! whichever the walker met first — so hovering `Release()` on
//! an ID3DBlob value silently rendered nothing. Catching the
//! conflict at load time turns the silent surprise into an
//! actionable error.

use std::collections::HashMap;

use ilang_ast::{ExternCItem, Item, Program, Span, Symbol};

use super::LoadError;

/// Logical decl key. Nominal kinds (class / interface / enum / ...)
/// share a single name namespace; free fns are keyed by name + the
/// rendered parameter-type list so overloads stay distinct.
#[derive(PartialEq, Eq, Hash, Clone)]
enum DeclKey {
    Nominal(Symbol),
    Fn(Symbol, Vec<String>),
}

pub(super) fn validate_unique_pub(prog: &Program) -> Result<(), LoadError> {
    let mut seen: HashMap<DeclKey, (Span, &'static str)> = HashMap::new();
    for item in &prog.items {
        match item {
            Item::Class(c) if c.is_pub => {
                insert(&mut seen, DeclKey::Nominal(c.name), c.span, "class", c.name)?;
            }
            Item::Interface(i) if i.is_pub => {
                insert(
                    &mut seen,
                    DeclKey::Nominal(i.name),
                    i.span,
                    "interface",
                    i.name,
                )?;
            }
            Item::Enum(e) if e.is_pub => {
                insert(&mut seen, DeclKey::Nominal(e.name), e.span, "enum", e.name)?;
            }
            Item::Const(c) if c.is_pub => {
                insert(&mut seen, DeclKey::Nominal(c.name), c.span, "const", c.name)?;
            }
            Item::Fn(f) if f.is_pub => {
                let params = f
                    .params
                    .iter()
                    .map(|p| p.ty.to_string())
                    .collect::<Vec<_>>();
                insert(
                    &mut seen,
                    DeclKey::Fn(f.name, params),
                    f.span,
                    "fn",
                    f.name,
                )?;
            }
            Item::ExternC(b) => {
                for inner in b.items.iter() {
                    match inner {
                        ExternCItem::Struct {
                            is_pub: true,
                            name,
                            span,
                            ..
                        } => {
                            insert(
                                &mut seen,
                                DeclKey::Nominal(*name),
                                *span,
                                "struct",
                                *name,
                            )?;
                        }
                        ExternCItem::Union {
                            is_pub: true,
                            name,
                            span,
                            ..
                        } => {
                            insert(
                                &mut seen,
                                DeclKey::Nominal(*name),
                                *span,
                                "union",
                                *name,
                            )?;
                        }
                        ExternCItem::FnDecl {
                            is_pub: true,
                            name,
                            params,
                            span,
                            ..
                        } => {
                            let p = params
                                .iter()
                                .map(|p| p.ty.to_string())
                                .collect::<Vec<_>>();
                            insert(
                                &mut seen,
                                DeclKey::Fn(*name, p),
                                *span,
                                "fn",
                                *name,
                            )?;
                        }
                        ExternCItem::FnDef(f) if f.is_pub => {
                            let p = f
                                .params
                                .iter()
                                .map(|p| p.ty.to_string())
                                .collect::<Vec<_>>();
                            insert(
                                &mut seen,
                                DeclKey::Fn(f.name, p),
                                f.span,
                                "fn",
                                f.name,
                            )?;
                        }
                        ExternCItem::Class(c) if c.is_pub => {
                            insert(
                                &mut seen,
                                DeclKey::Nominal(c.name),
                                c.span,
                                "class",
                                c.name,
                            )?;
                        }
                        _ => {}
                    }
                }
                for iface in b.interfaces.iter() {
                    if iface.is_pub {
                        insert(
                            &mut seen,
                            DeclKey::Nominal(iface.name),
                            iface.span,
                            "interface",
                            iface.name,
                        )?;
                    }
                }
                for c in b.consts.iter() {
                    if c.is_pub {
                        insert(
                            &mut seen,
                            DeclKey::Nominal(c.name),
                            c.span,
                            "const",
                            c.name,
                        )?;
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn insert(
    seen: &mut HashMap<DeclKey, (Span, &'static str)>,
    key: DeclKey,
    span: Span,
    kind: &'static str,
    name: Symbol,
) -> Result<(), LoadError> {
    if let Some((prev_span, _)) = seen.get(&key) {
        return Err(LoadError::DuplicatePubDeclaration {
            kind,
            name,
            first_span: *prev_span,
            second_span: span,
        });
    }
    seen.insert(key, (span, kind));
    Ok(())
}
