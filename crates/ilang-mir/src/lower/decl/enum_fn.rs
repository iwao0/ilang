//! Pre-registration for enums and free functions: `register_enum`
//! seeds the enum / variant id tables, `declare_fn` reserves a
//! `FuncId` (delegating `@intrinsic` fns to `declare_intrinsic_fn`)
//! so call sites resolve before any body is lowered.

use ilang_ast::{self as ast, FnDecl, Symbol};

use crate::inst::{FuncId, Terminator};
use crate::program::FunctionKind;
use crate::types::MirTy;

use super::super::utils::{mangle_suffix, placeholder_function};
use super::super::{
    EnumMeta, EnumVariantMeta, FnSig, Lower, LowerError, VariantPayloadMeta,
};

impl Lower {
    pub(in crate::lower) fn register_enum(&mut self, ed: &ast::EnumDecl) -> Result<(), LowerError> {
        if !ed.type_params.is_empty() {
            return Err(LowerError::Unsupported("generic enums"));
        }
        // Reuse the pre-allocated id from the name-pre-pass when
        // present (see `lower_program`'s 1b. step). Otherwise allocate
        // a fresh one.
        let (id, was_pre_allocated) = match self.enum_ids.get(&ed.name).copied() {
            Some(id) => (id, true),
            None => {
                let id = crate::types::EnumId(self.enums.len() as u32);
                self.enum_ids.insert(ed.name, id);
                (id, false)
            }
        };

        let repr_ty = match &ed.repr_ty {
            Some(t) => self.resolve_ty(t)?,
            None => MirTy::I64,
        };
        let is_str_repr = matches!(repr_ty, MirTy::Str);
        if is_str_repr && ed.flags {
            return Err(LowerError::Unsupported(
                "@flags is not allowed on `: string`-repr enums (bitwise ops are int-only)",
            ));
        }

        let mut variants = Vec::with_capacity(ed.variants.len());
        let mut meta = EnumMeta::default();
        let mut prev_disc: i64 = -1;
        for (i, v) in ed.variants.iter().enumerate() {
            let vid = crate::inst::VariantId(i as u32);
            let (disc, disc_str): (i64, Option<String>) = match (&v.discriminant, is_str_repr) {
                (Some(ast::DiscriminantLit::Int(n)), false) => (*n, None),
                (Some(ast::DiscriminantLit::Str(s)), true) => {
                    (i as i64, Some(s.clone()))
                }
                (None, false) => (prev_disc + 1, None),
                (None, true) => {
                    return Err(LowerError::Unsupported(
                        "enum with `: string` repr requires an explicit `= \"…\"` discriminant on every variant",
                    ));
                }
                (Some(ast::DiscriminantLit::Str(_)), false) => {
                    return Err(LowerError::Unsupported(
                        "string discriminant used on a non-string-repr enum",
                    ));
                }
                (Some(ast::DiscriminantLit::Int(_)), true) => {
                    return Err(LowerError::Unsupported(
                        "integer discriminant used on a `: string` repr enum",
                    ));
                }
            };
            prev_disc = disc;
            let (payload_layout, payload_meta) = match &v.payload {
                ast::VariantPayload::Unit => (
                    crate::program::VariantPayload::Unit,
                    VariantPayloadMeta::Unit,
                ),
                ast::VariantPayload::Tuple(tys) => {
                    let mut out = Vec::with_capacity(tys.len());
                    for t in tys.iter() {
                        out.push(self.resolve_ty(t)?);
                    }
                    (
                        crate::program::VariantPayload::Tuple(out.clone().into_boxed_slice()),
                        VariantPayloadMeta::Tuple(out),
                    )
                }
                ast::VariantPayload::Struct(fields) => {
                    let mut out_named: Vec<(Symbol, MirTy)> = Vec::with_capacity(fields.len());
                    for f in fields.iter() {
                        out_named.push((f.name, self.resolve_ty(&f.ty)?));
                    }
                    (
                        crate::program::VariantPayload::Struct(
                            out_named.clone().into_boxed_slice(),
                        ),
                        VariantPayloadMeta::Struct(out_named),
                    )
                }
            };
            variants.push(crate::program::VariantDecl {
                id: vid,
                name: v.name,
                discriminant: disc,
                discriminant_str: disc_str,
                payload: payload_layout,
            });
            meta.variants.insert(
                v.name,
                EnumVariantMeta {
                    id: vid,
                    payload: payload_meta,
                },
            );
        }
        let layout = crate::program::EnumLayout {
            id,
            name: ed.name,
            repr: repr_ty,
            variants,
            is_flags: ed.flags,
        };
        if was_pre_allocated {
            // Overwrite the placeholder layout the pre-pass pushed.
            self.enums[id.0 as usize] = layout;
        } else {
            self.enums.push(layout);
        }
        self.enum_meta.insert(id, meta);
        Ok(())
    }

    pub(in crate::lower) fn declare_fn(&mut self, fd: &FnDecl) -> Result<(), LowerError> {
        if !fd.type_params.is_empty() {
            return Err(LowerError::Unsupported("generic functions"));
        }
        // `@intrinsic` fns are body-less. Route the MIR registration
        // through the extern-style helper so the function lowers as
        // `Extern { sig_only: true }` with the runtime symbol on the
        // import side — independent of the `@lib` / `@symbol` paths
        // (no libs, no @optional). The `intrinsic_name` field on the
        // AST already carries the final `$X` symbol the runtime
        // exports.
        if let Some(sym) = fd.intrinsic_name {
            self.declare_intrinsic_fn(fd, sym)?;
            return Ok(());
        }
        let params: Vec<MirTy> = fd
            .params
            .iter()
            .map(|p| self.resolve_ty(&p.ty))
            .collect::<Result<Vec<_>, _>>()?;
        let ret = match &fd.ret {
            Some(t) => self.resolve_ty(t)?,
            None => MirTy::Unit,
        };
        // Mangle when this name already has a previous declaration —
        // i.e. the second+ overload. The first declaration keeps the
        // user-visible name so non-overloaded code stays simple.
        let mangled = if self.fn_ids.contains_key(&fd.name) {
            Symbol::intern(&format!("{}{}", fd.name, mangle_suffix(&params)))
        } else {
            fd.name
        };
        let id = FuncId(self.funcs.len() as u32);
        self.funcs.push(placeholder_function(mangled));
        self.fn_ids.insert(mangled, id);
        self.fn_sigs
            .insert(mangled, FnSig { params: params.clone(), ret });
        // Track overloads under the user-visible name.
        let entries = self.overloads.entry(fd.name).or_default();
        entries.push(mangled);
        // Stash the source-name → primary-mangled mapping in fnDecl
        // bookkeeping so that `lower_fn` can find the right slot.
        Ok(())
    }

    /// Register an `@intrinsic` fn as a sig-only extern function with
    /// the runtime symbol baked in. The `c_symbol` field is the
    /// cranelift import name; populating it via the dedicated
    /// `intrinsic_name` source field keeps the runtime-intrinsic path
    /// off the `@lib` / `@symbol` resolution flow.
    fn declare_intrinsic_fn(&mut self, fd: &FnDecl, sym: Symbol) -> Result<(), LowerError> {
        use crate::inst::{BlockId, ValueId};
        use crate::program::{Block, FuncParam, Function};
        if self.fn_ids.contains_key(&fd.name) {
            return Ok(());
        }
        let params: Vec<MirTy> = fd
            .params
            .iter()
            .map(|p| self.resolve_ty(&p.ty))
            .collect::<Result<Vec<_>, _>>()?;
        let ret = match &fd.ret {
            Some(t) => self.resolve_ty(t)?,
            None => MirTy::Unit,
        };
        let id = FuncId(self.funcs.len() as u32);
        let mut value_tys: Vec<MirTy> = Vec::with_capacity(params.len());
        let mut params_box: Vec<FuncParam> = Vec::with_capacity(params.len());
        for (i, p) in fd.params.iter().enumerate() {
            let v = ValueId(value_tys.len() as u32);
            value_tys.push(params[i].clone());
            params_box.push(FuncParam {
                name: p.name,
                ty: params[i].clone(),
                value: v,
            });
        }
        self.funcs.push(Function {
            name: fd.name,
            display_name: fd.name,
            params: params_box.into_boxed_slice(),
            ret: ret.clone(),
            value_tys,
            value_spans: vec![None; params.len()],
            blocks: vec![Block {
                params: Vec::new(),
                insts: Vec::new(),
                term: Terminator::Unreachable,
            }],
            entry: BlockId(0),
            kind: FunctionKind::Extern { sig_only: true },
            closure_env: None,
            span: Some(fd.span),
            local_tys: Vec::new(),
            c_symbol: Some(sym),
            is_optional: false,
            libs: Vec::new(),
            is_variadic: false,
        });
        self.fn_ids.insert(fd.name, id);
        self.fn_sigs.insert(
            fd.name,
            FnSig {
                params: params.clone(),
                ret,
            },
        );
        self.extern_meta.insert(
            fd.name,
            crate::lower::ExternMeta {
                libs: Vec::new(),
                optional: false,
                variadic: false,
                c_symbol: sym,
            },
        );
        Ok(())
    }
}
