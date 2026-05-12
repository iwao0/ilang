//! `@extern(C) { ... }` block lowering on `Lower`.
//!
//! Two passes — `register_extern_c_shells` populates struct /
//! union / class shells so forward references resolve, then
//! `declare_extern_c_fns` declares each contained fn shell so call
//! sites can resolve before bodies are lowered, then
//! `lower_extern_c` lowers the bodies. `c_size_align_of` is the
//! shared layout helper that drives offset / alignment computation
//! for nested CRepr fields. `validate_extern_c_by_value` checks
//! the by-value passing constraints up front.

use ilang_ast::{self as ast};

use crate::inst::{BlockId, FuncId, Terminator, ValueId};
use crate::program::{Function, FunctionKind};
use crate::types::MirTy;

use super::super::{ClassMeta, ExternMeta, FnSig, Lower, LowerError};

impl Lower {
    pub(in crate::lower) fn register_extern_c_shells(&mut self, blk: &ast::ExternCBlock) -> Result<(), LowerError> {
        // First pass: register every struct/union NAME so forward
        // references (struct A containing struct B that's declared
        // later) work without requiring source-level ordering.
        for item in blk.items.iter() {
            match item {
                ast::ExternCItem::Struct { name, .. }
                | ast::ExternCItem::Union { name, .. } => {
                    if !self.class_ids.contains_key(name) {
                        let id = crate::types::ClassId(self.classes.len() as u32);
                        self.class_ids.insert(*name, id);
                        // Push a placeholder layout — fields filled
                        // in by the second pass.
                        self.classes.push(crate::program::ClassLayout {
                            id,
                            name: *name,
                            parent: None,
                            fields: Vec::new(),
                            methods: Vec::new(),
                            statics: Vec::new(),
                            drop_fn: FuncId(u32::MAX),
                            vtable: None,
                            repr: crate::program::ClassRepr::CRepr,
                    c_field_offsets: Vec::new(),
                    c_size: 0,
                    flex_elem_size: 0,
                        });
                        self.class_meta.insert(id, ClassMeta::default());
                    }
                }
                _ => {}
            }
        }
        // Second pass: now that every name resolves, fill in field
        // layouts.
        for item in blk.items.iter() {
            match item {
                ast::ExternCItem::Struct { name, fields, is_packed, .. } => {
                    let id = *self.class_ids.get(name).expect("struct registered in pass 1");
                    let mut meta = ClassMeta::default();
                    let mut field_decls = Vec::with_capacity(fields.len());
                    for (i, fd) in fields.iter().enumerate() {
                        let fid = crate::inst::FieldId(i as u32);
                        let fty = self.resolve_ty(&fd.ty)?;
                        meta.field_ix.insert(fd.name, fid);
                        meta.field_ty.insert(fid, fty.clone());
                        let bit_field = fd.bits.map(|w| crate::program::BitField {
                            offset: 0,
                            width: w,
                        });
                        field_decls.push(crate::program::FieldDecl {
                            id: fid,
                            name: fd.name,
                            ty: fty,
                            bit_field,
                        });
                    }
                    let repr = if *is_packed {
                        crate::program::ClassRepr::CPacked
                    } else {
                        crate::program::ClassRepr::CRepr
                    };
                    let layout = &mut self.classes[id.0 as usize];
                    layout.fields = field_decls;
                    layout.repr = repr;
                    self.class_meta.insert(id, meta);
                }
                ast::ExternCItem::Union { name, fields, .. } => {
                    let id = *self.class_ids.get(name).expect("union registered in pass 1");
                    let mut meta = ClassMeta::default();
                    let mut field_decls = Vec::with_capacity(fields.len());
                    for (i, fd) in fields.iter().enumerate() {
                        let fid = crate::inst::FieldId(i as u32);
                        let fty = self.resolve_ty(&fd.ty)?;
                        meta.field_ix.insert(fd.name, fid);
                        meta.field_ty.insert(fid, fty.clone());
                        field_decls.push(crate::program::FieldDecl {
                            id: fid,
                            name: fd.name,
                            ty: fty,
                            bit_field: None,
                        });
                    }
                    let layout = &mut self.classes[id.0 as usize];
                    layout.fields = field_decls;
                    layout.repr = crate::program::ClassRepr::CUnion;
                    self.class_meta.insert(id, meta);
                }
                ast::ExternCItem::Class(_) => {
                    // ARC-managed wrapper class declared inside the
                    // block — register in the second loop below.
                }
                ast::ExternCItem::FnDecl { .. } | ast::ExternCItem::FnDef(_) => {
                    // Wired in lower_extern_c (after all types known).
                }
            }
        }
        // After shell registration, also register any wrapper class
        // shells inside the block (so subsequent types resolve them).
        for item in blk.items.iter() {
            if let ast::ExternCItem::Class(cd) = item {
                self.register_class(cd)?;
            }
        }
        // Compute C-struct field offsets + total sizes. Iterates a few
        // times to settle on nested struct sizes (forward references
        // produce a 0 placeholder on the first pass).
        for _ in 0..8 {
            let mut updated = false;
            for cid_idx in 0..self.classes.len() {
                let layout_clone = self.classes[cid_idx].clone();
                if !matches!(
                    layout_clone.repr,
                    crate::program::ClassRepr::CRepr
                        | crate::program::ClassRepr::CPacked
                        | crate::program::ClassRepr::CUnion
                ) {
                    continue;
                }
                let packed = matches!(layout_clone.repr, crate::program::ClassRepr::CPacked);
                let is_union = matches!(layout_clone.repr, crate::program::ClassRepr::CUnion);
                let mut offsets = Vec::with_capacity(layout_clone.fields.len());
                let mut bit_offsets: Vec<Option<u32>> =
                    Vec::with_capacity(layout_clone.fields.len());
                let mut cur: i64 = 0;
                let mut max_align: i64 = 1;
                let mut max_size: i64 = 0;
                // Bitfield run state: when the previous field was a
                // bitfield, we keep packing into the same storage
                // unit until either the type changes or the bit
                // budget overflows.
                let mut bit_run_offset: i64 = 0;
                let mut bit_run_size: i64 = 0;
                let mut bit_run_align: i64 = 0;
                let mut bit_run_consumed: u32 = 0;
                for f in &layout_clone.fields {
                    let (sz, al) = self.c_size_align_of(&f.ty);
                    let align = if packed { 1 } else { al };
                    let is_bitfield = f.bit_field.is_some();
                    if is_union {
                        offsets.push(0);
                        bit_offsets.push(None);
                        if sz > max_size { max_size = sz; }
                        if align > max_align { max_align = align; }
                        continue;
                    }
                    if is_bitfield {
                        let width = f.bit_field.unwrap().width;
                        let f_total_bits = (sz * 8) as u32;
                        let same_unit = bit_run_size == sz
                            && bit_run_align == align
                            && bit_run_consumed + width <= f_total_bits
                            && bit_run_size > 0;
                        if !same_unit {
                            // Start a new storage unit for this bitfield.
                            if align > max_align { max_align = align; }
                            cur = (cur + align - 1) / align * align;
                            bit_run_offset = cur;
                            bit_run_size = sz;
                            bit_run_align = align;
                            bit_run_consumed = 0;
                            cur += sz;
                        }
                        offsets.push(bit_run_offset);
                        bit_offsets.push(Some(bit_run_consumed));
                        bit_run_consumed += width;
                    } else {
                        // Normal field — close any open bitfield run.
                        bit_run_size = 0;
                        bit_run_align = 0;
                        bit_run_consumed = 0;
                        if align > max_align { max_align = align; }
                        cur = (cur + align - 1) / align * align;
                        offsets.push(cur);
                        bit_offsets.push(None);
                        cur += sz;
                    }
                }
                // Flexible array member: last field of a (non-union)
                // CRepr struct typed `T[]` (dynamic). The size of the
                // FAM area is decided at `new StructName(n)` time;
                // the field contributes 0 bytes here. Roll back the
                // pointer-sized contribution we added above and
                // re-anchor the field's c_field_offset to the byte
                // start of the trailing area.
                let mut flex_elem_size: i64 = 0;
                if !is_union {
                    if let Some(last) = layout_clone.fields.last() {
                        if let MirTy::Array { elem, len: None } = &last.ty {
                            let (es, _) = self.c_size_align_of(elem);
                            flex_elem_size = es;
                            cur -= 8;
                            if let Some(last_off) = offsets.last_mut() {
                                *last_off = cur;
                            }
                        }
                    }
                }
                let total = if is_union {
                    let aligned = (max_size + max_align - 1) / max_align * max_align;
                    aligned
                } else {
                    (cur + max_align - 1) / max_align * max_align
                };
                let mut bit_changed = false;
                for (i, bf_offset) in bit_offsets.iter().enumerate() {
                    if let (Some(off), Some(bf)) =
                        (bf_offset, &mut self.classes[cid_idx].fields[i].bit_field)
                    {
                        if bf.offset != *off {
                            bf.offset = *off;
                            bit_changed = true;
                        }
                    }
                }
                if self.classes[cid_idx].c_field_offsets != offsets
                    || self.classes[cid_idx].c_size != total
                    || self.classes[cid_idx].flex_elem_size != flex_elem_size
                    || bit_changed
                {
                    self.classes[cid_idx].c_field_offsets = offsets;
                    self.classes[cid_idx].c_size = total;
                    self.classes[cid_idx].flex_elem_size = flex_elem_size;
                    updated = true;
                }
            }
            if !updated {
                break;
            }
        }
        Ok(())
    }

    /// (size, alignment) of a MirTy when laid out as a C value.
    pub(in crate::lower) fn c_size_align_of(&self, t: &MirTy) -> (i64, i64) {
        match t {
            MirTy::I8 | MirTy::U8 | MirTy::CChar | MirTy::Bool => (1, 1),
            MirTy::I16 | MirTy::U16 => (2, 2),
            MirTy::I32 | MirTy::U32 | MirTy::F32 => (4, 4),
            MirTy::I64 | MirTy::U64 | MirTy::F64 | MirTy::Size | MirTy::SSize => (8, 8),
            // Fixed-length array: inline `T[N]` lays out as N×T.
            MirTy::Array { elem, len: Some(n) } => {
                let (es, ea) = self.c_size_align_of(elem);
                (es * (*n as i64), ea)
            }
            MirTy::Object(cid) => {
                let layout = &self.classes[cid.0 as usize];
                if matches!(
                    layout.repr,
                    crate::program::ClassRepr::CRepr
                        | crate::program::ClassRepr::CPacked
                        | crate::program::ClassRepr::CUnion
                ) {
                    let s = layout.c_size;
                    // Nested struct alignment = its max field alignment
                    // (re-derived; cheap for small structs). Defaults
                    // to 8 if unknown.
                    let mut al: i64 = 1;
                    for f in &layout.fields {
                        let (_, fa) = self.c_size_align_of(&f.ty);
                        if fa > al { al = fa; }
                    }
                    if matches!(layout.repr, crate::program::ClassRepr::CPacked) {
                        (s.max(0), 1)
                    } else {
                        (s.max(0), al)
                    }
                } else {
                    (8, 8) // ARC pointer
                }
            }
            MirTy::RawPtr { .. } => (8, 8),
            // Unit-only enums marshal as their underlying repr int
            // (`enum X: u16` → 2 bytes, etc.) so they line up with
            // C `enum`-typed struct fields. Payload-bearing enums
            // are heap-allocated (`NewEnum`) — keep the 8/8 default
            // since they aren't meaningful inside a C ABI struct.
            // `: string`-repr enums fall back to (8, 8) (heap
            // pointer); using one inside `@extern(C) struct` is a
            // sketch case anyway since SDL never reads its own
            // hint enum back from a struct, but we keep the size
            // unambiguous.
            MirTy::Enum(eid) => {
                let layout = &self.enums[eid.0 as usize];
                let unit_only = layout
                    .variants
                    .iter()
                    .all(|v| matches!(v.payload, crate::program::VariantPayload::Unit));
                let int_repr = !matches!(layout.repr, MirTy::Str);
                if unit_only && int_repr {
                    self.c_size_align_of(&layout.repr)
                } else {
                    (8, 8)
                }
            }
            _ => (8, 8),
        }
    }

    /// By-value `@extern(C) struct` ABI checker: refuse to register an
    /// extern fn whose param is a CRepr struct mixing integer/bool
    /// fields with float fields (an HFA / SSE classification mismatch
    /// the codegen can't honour).
    pub(in crate::lower) fn validate_extern_c_by_value(&self, params: &[MirTy]) -> Result<(), LowerError> {
        for pty in params {
            if let MirTy::Object(cid) = pty {
                let layout = &self.classes[cid.0 as usize];
                if matches!(
                    layout.repr,
                    crate::program::ClassRepr::CRepr | crate::program::ClassRepr::CPacked
                ) {
                    let mut has_int = false;
                    let mut has_float = false;
                    for f in &layout.fields {
                        if f.ty.is_int() || matches!(f.ty, MirTy::Bool) {
                            has_int = true;
                        }
                        if matches!(f.ty, MirTy::F32 | MirTy::F64) {
                            has_float = true;
                        }
                    }
                    if has_int && has_float {
                        return Err(LowerError::Other(format!(
                            "@extern(C) by-value `{}`: supported shapes are integer/bool fields or homogeneous float aggregates",
                            layout.name
                        )));
                    }
                }
            }
        }
        Ok(())
    }

    /// Pre-register every extern fn / fn definition declared in the
    /// block so other items (free fns, class methods) that call them
    /// resolve correctly during their own pre-pass.
    pub(in crate::lower) fn declare_extern_c_fns(&mut self, blk: &ast::ExternCBlock) -> Result<(), LowerError> {
        for item in blk.items.iter() {
            match item {
                ast::ExternCItem::FnDecl {
                    name, params, ret, libs, optional, variadic, c_symbol, ..
                } => {
                    if self.fn_ids.contains_key(name) {
                        continue;
                    }
                    let mangled = *name;
                    let id = FuncId(self.funcs.len() as u32);
                    let kind = FunctionKind::Extern { sig_only: true };
                    let mir_params: Vec<MirTy> = params
                        .iter()
                        .map(|p| self.resolve_ty(&p.ty))
                        .collect::<Result<Vec<_>, _>>()?;
                    let mir_ret = match ret {
                        Some(t) => self.resolve_ty(t)?,
                        None => MirTy::Unit,
                    };
                    self.validate_extern_c_by_value(&mir_params)?;
                    let mut value_tys: Vec<MirTy> = Vec::with_capacity(mir_params.len());
                    let mut params_box: Vec<crate::program::FuncParam> =
                        Vec::with_capacity(mir_params.len());
                    for (i, p) in params.iter().enumerate() {
                        let v = ValueId(value_tys.len() as u32);
                        let pty = mir_params[i].clone();
                        value_tys.push(pty.clone());
                        params_box.push(crate::program::FuncParam {
                            name: p.name,
                            ty: pty,
                            value: v,
                        });
                    }
                    self.funcs.push(Function {
                        name: mangled,
                        display_name: mangled,
                        params: params_box.into_boxed_slice(),
                        ret: mir_ret.clone(),
                        value_tys,
                        value_spans: vec![None; mir_params.len()],
                        blocks: vec![crate::program::Block {
                            params: Vec::new(),
                            insts: Vec::new(),
                            term: Terminator::Unreachable,
                        }],
                        entry: BlockId(0),
                        kind,
                        closure_env: None,
                        span: None,
                        local_tys: Vec::new(),
                        c_symbol: *c_symbol,
                        is_optional: *optional,
                        libs: libs.iter().copied().collect(),
                        is_variadic: *variadic,
                    });
                    self.fn_ids.insert(mangled, id);
                    self.fn_sigs.insert(
                        mangled,
                        FnSig {
                            params: mir_params,
                            ret: mir_ret,
                        },
                    );
                    self.extern_meta.insert(
                        mangled,
                        ExternMeta {
                            libs: libs.iter().copied().collect(),
                            optional: *optional,
                            variadic: *variadic,
                            c_symbol: c_symbol.unwrap_or(mangled),
                        },
                    );
                }
                ast::ExternCItem::FnDef(fd) => {
                    if !self.fn_ids.contains_key(&fd.name) {
                        self.declare_fn(fd)?;
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    pub(in crate::lower) fn lower_extern_c(&mut self, blk: &ast::ExternCBlock) -> Result<(), LowerError> {
        // Pre-declare extern fns (for forward references).
        for item in blk.items.iter() {
            match item {
                ast::ExternCItem::FnDecl {
                    name, params, ret, libs, optional, variadic, c_symbol, ..
                } => {
                    if self.fn_ids.contains_key(name) {
                        continue;
                    }
                    let mangled = *name;
                    let id = FuncId(self.funcs.len() as u32);
                    let kind = FunctionKind::Extern { sig_only: true };
                    let mir_params: Vec<MirTy> = params
                        .iter()
                        .map(|p| self.resolve_ty(&p.ty))
                        .collect::<Result<Vec<_>, _>>()?;
                    let mir_ret = match ret {
                        Some(t) => self.resolve_ty(t)?,
                        None => MirTy::Unit,
                    };
                    // Extern declaration: synthesise FuncParams so
                    // `clif_signature_for` reports the right param
                    // count. Each param gets a placeholder ValueId
                    // (the body is empty / unreachable, so no body
                    // inst references them).
                    let mut value_tys: Vec<MirTy> = Vec::with_capacity(mir_params.len());
                    let mut params_box: Vec<crate::program::FuncParam> =
                        Vec::with_capacity(mir_params.len());
                    for (i, p) in params.iter().enumerate() {
                        let v = ValueId(value_tys.len() as u32);
                        let pty = mir_params[i].clone();
                        value_tys.push(pty.clone());
                        params_box.push(crate::program::FuncParam {
                            name: p.name,
                            ty: pty,
                            value: v,
                        });
                    }
                    self.funcs.push(Function {
                        name: mangled,
                        display_name: mangled,
                        params: params_box.into_boxed_slice(),
                        ret: mir_ret.clone(),
                        value_tys,
                        value_spans: vec![None; mir_params.len()],
                        blocks: vec![crate::program::Block {
                            params: Vec::new(),
                            insts: Vec::new(),
                            term: Terminator::Unreachable,
                        }],
                        entry: BlockId(0),
                        kind,
                        closure_env: None,
                        span: None,
                        local_tys: Vec::new(),
                        c_symbol: *c_symbol,
                        is_optional: *optional,
                        libs: libs.iter().copied().collect(),
                        is_variadic: *variadic,
                    });
                    self.fn_ids.insert(mangled, id);
                    self.fn_sigs.insert(
                        mangled,
                        FnSig {
                            params: mir_params.clone(),
                            ret: mir_ret,
                        },
                    );
                    // Stash the FFI binding metadata so callers know
                    // which library and symbol to bind.
                    self.extern_meta.insert(
                        mangled,
                        ExternMeta {
                            libs: libs.iter().copied().collect(),
                            optional: *optional,
                            variadic: *variadic,
                            c_symbol: c_symbol.unwrap_or(mangled),
                        },
                    );
                }
                _ => {}
            }
        }
        // Lower @extern(C) ilang-side fn definitions like normal fns.
        for item in blk.items.iter() {
            if let ast::ExternCItem::FnDef(fd) = item {
                if !self.fn_ids.contains_key(&fd.name) {
                    self.declare_fn(fd)?;
                }
            }
        }
        for item in blk.items.iter() {
            if let ast::ExternCItem::FnDef(fd) = item {
                self.lower_fn(fd)?;
                // Mark the lowered fn as ExternBody so the codegen
                // emits it under the C ABI.
                let id = *self.fn_ids.get(&fd.name).unwrap();
                self.funcs[id.0 as usize].kind = FunctionKind::ExternBody;
            }
        }
        // Wrapper classes: declare + lower their methods.
        for item in blk.items.iter() {
            if let ast::ExternCItem::Class(cd) = item {
                self.declare_class_methods(cd)?;
            }
        }
        for item in blk.items.iter() {
            if let ast::ExternCItem::Class(cd) = item {
                self.lower_class_methods(cd)?;
            }
        }
        Ok(())
    }
}
